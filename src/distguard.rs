//! 反分发：单 Key 24 小时内不同 IP 数统计 + 告警/限流；免费模型每日 token 上限。
//!
//! - 每个已鉴权的 /v1 代理请求都会记一次 (key_id, client_ip)。
//! - 24 小时滚动窗口内不同 IP 数超过 `DIST_IP_ALERT`（默认 10）时写一条
//!   `key.distinct_ip_alert` 审计；超过 `DIST_IP_BLOCK`（默认 20）时该 Key
//!   的代理请求返回 429（`key_distinct_ip_limit`），旧 IP 滑出窗口后自动恢复。
//!   不自动吊销 Key，吊销由管理员在后台决定。
//! - 内存统计；进程启动后首次见到某 Key 时从 usage_logs 回填最近 24 小时
//!   的不同 IP（每 Key 每进程一次），重启不丢历史。回填失败时 fail-open。
//! - 误伤说明：正常用户一天换几个出口 IP 很常见，10/20 的阈值留了余地；
//!   出口 IP 每次都变的极端情况（如某些代理池）仍可能触发，届时管理员
//!   可在审计日志里看到并处理，用户也可换一把新 Key。
//!
//! 免费模型 token 上限（`DIST_FREE_TOKEN_CAP`，默认 1_000_000_000/天）：
//! 零定价模型不受成本额度约束，单 Key 与单用户每天各限这么多 token。
//! 阈值依据 2026-11-21 的 7 天数据：除分发 Key 外最高单 Key 日为 28.7M，
//! 分发 Key 最低完整日为 97M；30M 夹在中间，两边都有余地。
//! 2026-11-21 Wing 定：默认值改为 1_000_000_000（1B）/天；管理员可对指定
//! Key 或指定用户单独设上限（表 free_token_cap_overrides），优先级：
//! Key 单独设置 > 用户单独设置 > 默认值/环境变量。单独设置重启不丢。

use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 24 小时滚动窗口。
pub const WINDOW: Duration = Duration::from_secs(24 * 3600);

/// 免费模型每日 token 上限默认值（1B）。环境变量 DIST_FREE_TOKEN_CAP 可调；
/// 管理员单独设置的上限（free_token_cap_overrides 表）优先级高于此值。
pub const DEFAULT_FREE_TOKEN_CAP: i64 = 1_000_000_000;

/// 回填时每 Key 最多取的不同 IP 数（再多也早已超过拦截线）。
const SEED_LIMIT: i64 = 100;

/// 告警级别：1 = 告警线，2 = 拦截线。
pub const ALERT_LEVEL_WARN: u8 = 1;
pub const ALERT_LEVEL_BLOCK: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistDecision {
    Allow,
    Alert { distinct_ips: usize },
    Block { distinct_ips: usize },
}

/// 一次需要写审计的新告警（调用方负责写审计，本模块不碰 admin 表）。
#[derive(Debug, Clone, Copy)]
pub struct DistAlert {
    pub level: u8,
    pub distinct_ips: usize,
}

pub struct DistGuard {
    /// key_id -> (ip -> 最近一次出现时间）
    ips: Mutex<HashMap<i64, HashMap<String, Instant>>>,
    /// (key_id, 级别) -> 上次写审计的时间；同一级别 24 小时内只告警一次。
    alerted: Mutex<HashMap<(i64, u8), Instant>>,
    /// 已从库里回填过的 Key（每进程一次）。
    seeded: Mutex<HashSet<i64>>,
    pub alert_at: usize,
    pub block_at: usize,
    pub free_token_cap: i64,
    window: Duration,
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_i64(name: &str, default: i64) -> i64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

impl DistGuard {
    pub fn from_env() -> Self {
        Self::new(
            env_usize("DIST_IP_ALERT", 10),
            env_usize("DIST_IP_BLOCK", 20),
            env_i64("DIST_FREE_TOKEN_CAP", DEFAULT_FREE_TOKEN_CAP),
        )
    }

    pub fn new(alert_at: usize, block_at: usize, free_token_cap: i64) -> Self {
        Self::with_window(alert_at, block_at, free_token_cap, WINDOW)
    }

    pub fn with_window(
        alert_at: usize,
        block_at: usize,
        free_token_cap: i64,
        window: Duration,
    ) -> Self {
        Self {
            ips: Mutex::new(HashMap::new()),
            alerted: Mutex::new(HashMap::new()),
            seeded: Mutex::new(HashSet::new()),
            alert_at,
            block_at,
            free_token_cap,
            window,
        }
    }

    /// 记一次 (key_id, ip)，返回判定与是否需要新写一条审计。
    /// 回填/审计去重在这里完成；写审计本身由调用方做。
    pub async fn observe_key(
        &self,
        pool: &PgPool,
        key_id: i64,
        ip: &str,
    ) -> (DistDecision, Option<DistAlert>) {
        if ip.is_empty() {
            return (DistDecision::Allow, None);
        }
        let need_seed = {
            let mut seeded = self.seeded.lock().unwrap();
            seeded.insert(key_id)
        };
        if need_seed {
            self.seed_from_db(pool, key_id).await;
        }
        let now = Instant::now();
        let (count, decision) = self.observe_inner(key_id, ip, now);
        let level = match decision {
            DistDecision::Block { .. } => Some(ALERT_LEVEL_BLOCK),
            DistDecision::Alert { .. } => Some(ALERT_LEVEL_WARN),
            DistDecision::Allow => None,
        };
        let alert = match level {
            Some(level) if self.should_alert(key_id, level, now) => Some(DistAlert {
                level,
                distinct_ips: count,
            }),
            _ => None,
        };
        (decision, alert)
    }

    /// 首次见到某 Key 时，从 usage_logs 回填最近 24 小时的不同 IP。
    /// 失败只记日志、不阻塞请求；无论成败本进程内只试一次。
    async fn seed_from_db(&self, pool: &PgPool, key_id: i64) {
        let rows = sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT client_ip FROM usage_logs \
             WHERE key_id = $1 AND created_at >= now() - interval '24 hours' \
             AND client_ip <> '' LIMIT $2",
        )
        .bind(key_id)
        .bind(SEED_LIMIT)
        .fetch_all(pool)
        .await;
        match rows {
            Ok(rows) => {
                if !rows.is_empty() {
                    let now = Instant::now();
                    let mut ips = self.ips.lock().unwrap();
                    let entry = ips.entry(key_id).or_default();
                    for ip in rows {
                        entry.entry(ip).or_insert(now);
                    }
                    tracing::info!(key_id, distinct_ips = entry.len(), "dist guard seeded from usage_logs");
                }
            }
            Err(err) => {
                tracing::error!(error = %err, key_id, "dist guard seed failed, continuing unseeded");
            }
        }
    }

    /// 纯内存部分：记一次 IP、清掉窗口外的旧 IP、返回当前不同 IP 数与判定。
    fn observe_inner(&self, key_id: i64, ip: &str, now: Instant) -> (usize, DistDecision) {
        let mut ips = self.ips.lock().unwrap();
        let entry = ips.entry(key_id).or_default();
        entry.retain(|_, seen| now.saturating_duration_since(*seen) <= self.window);
        entry.insert(ip.to_string(), now);
        let count = entry.len();
        if entry.is_empty() {
            ips.remove(&key_id);
        }
        (count, self.decide(count))
    }

    fn decide(&self, count: usize) -> DistDecision {
        if self.block_at > 0 && count > self.block_at {
            DistDecision::Block { distinct_ips: count }
        } else if self.alert_at > 0 && count > self.alert_at {
            DistDecision::Alert { distinct_ips: count }
        } else {
            DistDecision::Allow
        }
    }

    /// 同一 Key 同一级别 24 小时内只告警一次；返回本次是否需要写审计。
    fn should_alert(&self, key_id: i64, level: u8, now: Instant) -> bool {
        let mut alerted = self.alerted.lock().unwrap();
        match alerted.get(&(key_id, level)) {
            Some(last) if now.saturating_duration_since(*last) <= self.window => false,
            _ => {
                alerted.insert((key_id, level), now);
                true
            }
        }
    }

    /// 免费模型当日 token 是否超限：任一口径（Key 或用户）已用 + 本次预估
    /// 超过上限即拦。预估只是 prompt 的粗估，实际超限幅度至多多一次响应。
    /// `cap` 是本次请求生效的上限（见 effective_free_token_cap）。
    pub fn free_token_exceeded(
        &self,
        used_key: i64,
        used_user: i64,
        estimate: i64,
        cap: i64,
    ) -> bool {
        used_key.saturating_add(estimate) > cap || used_user.saturating_add(estimate) > cap
    }

    /// 生效上限（纯函数，便于单测）：Key 单独设置 > 用户单独设置 > 默认值。
    /// 同一请求的 Key 口径与用户口径都按这个生效上限判断——否则单 Key 用户
    /// 的 Key 单独设置会被用户口径的默认值顶掉，等于没设。
    pub fn resolve_free_token_cap(
        key_override: Option<i64>,
        user_override: Option<i64>,
        default_cap: i64,
    ) -> i64 {
        key_override.or(user_override).unwrap_or(default_cap)
    }
}

/// 本次请求生效的免费 token 日上限：先查该 Key 的单独设置，再查该用户的
/// 单独设置，都没有则用默认值（环境变量 DIST_FREE_TOKEN_CAP 或 1B）。
/// 查库失败时 fail-open 用默认值，只记日志、不拦截。
pub async fn effective_free_token_cap(
    pool: &PgPool,
    key_id: i64,
    user_id: i64,
    default_cap: i64,
) -> i64 {
    let key_override: Option<i64> = sqlx::query_scalar(
        "SELECT cap_tokens FROM free_token_cap_overrides WHERE key_id = $1",
    )
    .bind(key_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|err| {
        tracing::error!(error = %err, key_id, "free token cap key override lookup failed");
        None
    });
    if key_override.is_some() {
        return DistGuard::resolve_free_token_cap(key_override, None, default_cap);
    }
    let user_override: Option<i64> = sqlx::query_scalar(
        "SELECT cap_tokens FROM free_token_cap_overrides WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .unwrap_or_else(|err| {
        tracing::error!(error = %err, user_id, "free token cap user override lookup failed");
        None
    });
    DistGuard::resolve_free_token_cap(key_override, user_override, default_cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guard() -> DistGuard {
        DistGuard::with_window(10, 20, 1_000_000_000, Duration::from_secs(24 * 3600))
    }

    #[test]
    fn same_ip_counts_once_and_prunes_after_window() {
        let g = guard();
        let base = Instant::now();
        let (n, d) = g.observe_inner(1, "1.1.1.1", base);
        assert_eq!((n, d), (1, DistDecision::Allow));
        let (n, _) = g.observe_inner(1, "1.1.1.1", base + Duration::from_secs(60));
        assert_eq!(n, 1);
        // 25 小时后旧 IP 滑出窗口，只剩新来的这一个。
        let (n, _) = g.observe_inner(1, "2.2.2.2", base + Duration::from_secs(25 * 3600));
        assert_eq!(n, 1);
    }

    #[test]
    fn alert_and_block_thresholds() {
        let g = guard();
        let base = Instant::now();
        let mut last = DistDecision::Allow;
        for i in 0..10 {
            let (_, d) = g.observe_inner(7, &format!("10.0.0.{i}"), base);
            last = d;
        }
        assert_eq!(last, DistDecision::Allow);
        let (_, d) = g.observe_inner(7, "10.0.0.10", base);
        assert_eq!(d, DistDecision::Alert { distinct_ips: 11 });
        let mut last = d;
        for i in 11..20 {
            let (_, d) = g.observe_inner(7, &format!("10.0.0.{i}"), base);
            last = d;
        }
        assert_eq!(last, DistDecision::Alert { distinct_ips: 20 });
        let (_, d) = g.observe_inner(7, "10.0.0.20", base);
        assert_eq!(d, DistDecision::Block { distinct_ips: 21 });
    }

    #[test]
    fn alert_dedup_once_per_window() {
        let g = guard();
        let base = Instant::now();
        assert!(g.should_alert(3, ALERT_LEVEL_WARN, base));
        assert!(!g.should_alert(3, ALERT_LEVEL_WARN, base + Duration::from_secs(60)));
        assert!(g.should_alert(3, ALERT_LEVEL_BLOCK, base + Duration::from_secs(60)));
        assert!(g.should_alert(3, ALERT_LEVEL_WARN, base + Duration::from_secs(25 * 3600)));
    }

    #[test]
    fn free_token_cap_boundaries() {
        let g = guard();
        assert!(!g.free_token_exceeded(999_000_000, 1_000, 999_999, g.free_token_cap));
        assert!(g.free_token_exceeded(999_999_999, 1_000, 2, g.free_token_cap));
        assert!(g.free_token_exceeded(1_000, 1_000_000_000, 1, g.free_token_cap));
        assert!(!g.free_token_exceeded(0, 0, 1_000_000_000, g.free_token_cap));
    }

    #[test]
    fn free_token_cap_default_is_one_billion() {
        assert_eq!(DEFAULT_FREE_TOKEN_CAP, 1_000_000_000);
        assert_eq!(DistGuard::from_env().free_token_cap, 1_000_000_000);
    }

    #[test]
    fn resolve_free_token_cap_key_beats_user_beats_default() {
        assert_eq!(
            DistGuard::resolve_free_token_cap(Some(5_000_000_000), Some(2_000_000_000), 1_000_000_000),
            5_000_000_000
        );
        assert_eq!(
            DistGuard::resolve_free_token_cap(None, Some(2_000_000_000), 1_000_000_000),
            2_000_000_000
        );
        assert_eq!(
            DistGuard::resolve_free_token_cap(None, None, 1_000_000_000),
            1_000_000_000
        );
    }
}
