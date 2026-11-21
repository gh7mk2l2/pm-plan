//! IP 级封禁与防刷（flood guard）。
//!
//! - 启动时从 `ip_bans` 表加载封禁名单到内存，后续请求在内存里判定，不查库。
//! - 单个地址在 1 秒内的 `/api`、`/v1` 请求数超过阈值（`IP_FLOOD_PER_SEC`，默认 30，
//!   设为 0 关闭自动封禁）就自动封禁：写入 `ip_bans`，并把在该地址登录或调用过的
//!   非管理员账号一起封掉、踢下线。
//! - `IP_GUARD_ALLOW` 填逗号分隔的白名单地址，白名单永不封禁。
//! - 站长可在后台查看、手动添加、解除封禁。

use crate::auth::{require_admin, AuthUser};
use crate::error::{AppError, AppJson, AppResult};
use crate::state::AppState;
use crate::util::{client_ip, truncate_chars};
use axum::extract::{Path, Query, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// 自动封禁写入的原因。
pub const AUTO_REASON: &str = "DDoS 是吧，进去！";
/// 被连带封禁的账号上记录的原因。
pub const ACCOUNT_REASON: &str = "account disabled by ip guard";

pub struct IpGuard {
    banned: RwLock<HashSet<String>>,
    window: Mutex<HashMap<String, (u64, u32)>>,
    allow: HashSet<String>,
    per_sec: u32,
}

impl IpGuard {
    pub fn from_env() -> Self {
        let per_sec = std::env::var("IP_FLOOD_PER_SEC")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(30);
        let allow = std::env::var("IP_GUARD_ALLOW")
            .unwrap_or_default()
            .split(',')
            .filter_map(normalize_ip)
            .collect();
        Self::new(per_sec, allow)
    }

    pub fn new(per_sec: u32, allow: HashSet<String>) -> Self {
        Self {
            banned: RwLock::new(HashSet::new()),
            window: Mutex::new(HashMap::new()),
            allow,
            per_sec,
        }
    }

    /// 启动时加载封禁名单。失败只记日志，不阻止启动。
    pub async fn load(&self, pool: &PgPool) {
        match sqlx::query_scalar::<_, String>("SELECT ip FROM ip_bans")
            .fetch_all(pool)
            .await
        {
            Ok(rows) => {
                let count = rows.len();
                *self.banned.write().unwrap() = rows.into_iter().collect();
                tracing::info!(count, "ip guard loaded block list");
            }
            Err(err) => tracing::error!(error = %err, "ip guard failed to load block list"),
        }
    }

    pub fn is_banned(&self, ip: &str) -> bool {
        self.banned.read().unwrap().contains(ip)
    }

    pub fn is_allowed(&self, ip: &str) -> bool {
        self.allow.contains(ip)
    }

    fn insert(&self, ip: &str) {
        self.banned.write().unwrap().insert(ip.to_string());
    }

    fn remove(&self, ip: &str) -> bool {
        self.banned.write().unwrap().remove(ip)
    }

    /// 计一次请求；本秒超过阈值时返回 true（只在越线那一下返回一次）。
    pub fn hit(&self, ip: &str, now_sec: u64) -> bool {
        if self.per_sec == 0 || self.is_allowed(ip) {
            return false;
        }
        let mut window = self.window.lock().unwrap();
        if window.len() > 50_000 {
            window.retain(|_, (sec, _)| *sec + 2 >= now_sec);
        }
        let entry = window.entry(ip.to_string()).or_insert((now_sec, 0));
        if entry.0 != now_sec {
            *entry = (now_sec, 0);
        }
        entry.1 += 1;
        entry.1 == self.per_sec + 1
    }
}

/// 规范化地址：去空白，必须能解析成 IPv4/IPv6，统一成标准写法。
pub fn normalize_ip(raw: &str) -> Option<String> {
    raw.trim().parse::<IpAddr>().ok().map(|ip| ip.to_string())
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn guarded_path(path: &str) -> bool {
    path.starts_with("/api/") || path.starts_with("/v1/")
}

fn blocked_response() -> Response {
    AppError::new(
        axum::http::StatusCode::FORBIDDEN,
        "forbidden",
        "address blocked by flood guard",
    )
    .into_response()
}

/// 全局中间件：封禁地址直接 403；`/api`、`/v1` 请求计数，越线自动封禁。
pub async fn guard(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(ip) = normalize_ip(&client_ip(req.headers())) else {
        return next.run(req).await;
    };
    if state.ip_guard.is_allowed(&ip) {
        return next.run(req).await;
    }
    if state.ip_guard.is_banned(&ip) {
        return blocked_response();
    }
    if guarded_path(req.uri().path()) && state.ip_guard.hit(&ip, now_sec()) {
        state.ip_guard.insert(&ip);
        tracing::warn!(%ip, "ip guard tripped, banning address");
        let pool = state.pool.clone();
        tokio::spawn(async move {
            if let Err(err) = persist_ban(&pool, &ip, AUTO_REASON, None).await {
                tracing::error!(error = %err, %ip, "failed to persist ip ban");
            }
        });
        return blocked_response();
    }
    next.run(req).await
}

/// 落库并连带封禁该地址上出现过的非管理员账号。返回被封的账号数。
async fn persist_ban(
    pool: &PgPool,
    ip: &str,
    reason: &str,
    actor_id: Option<i64>,
) -> Result<usize, sqlx::Error> {
    sqlx::query(
        "INSERT INTO ip_bans (ip, reason) VALUES ($1, $2) \
         ON CONFLICT (ip) DO UPDATE SET hits = ip_bans.hits + 1",
    )
    .bind(ip)
    .bind(reason)
    .execute(pool)
    .await?;

    // 与线上二进制一致：先取出在该地址出现过的账号，再逐个封禁（仅跳过 root）。
    let user_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM ( \
           SELECT user_id FROM sessions WHERE ip = $1 \
           UNION SELECT user_id FROM usage_logs WHERE client_ip = $1 AND user_id IS NOT NULL \
         ) seen",
    )
    .bind(ip)
    .fetch_all(pool)
    .await?;

    let mut banned = Vec::new();
    for user_id in user_ids {
        let done = sqlx::query(
            "UPDATE users SET is_banned = TRUE, ban_reason = $2, updated_at = now() \
             WHERE id = $1 AND is_banned = FALSE AND role <> 'root'",
        )
        .bind(user_id)
        .bind(ACCOUNT_REASON)
        .execute(pool)
        .await?;
        if done.rows_affected() > 0 {
            sqlx::query("DELETE FROM sessions WHERE user_id = $1")
                .bind(user_id)
                .execute(pool)
                .await?;
            banned.push(user_id);
        }
    }

    // 线上自动封禁不写审计；站长手动封禁（本版新增）才记一笔。
    if actor_id.is_some() {
        audit(
            pool,
            actor_id,
            "ip.ban",
            ip,
            json!({ "reason": reason, "banned_users": banned }),
        )
        .await;
    }
    Ok(banned.len())
}

async fn audit(pool: &PgPool, actor_id: Option<i64>, action: &str, ip: &str, detail: Value) {
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
         VALUES ($1, $2, 'ip', $3, $4)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(ip)
    .bind(detail)
    .execute(pool)
    .await;
    if let Err(err) = result {
        tracing::warn!(error = %err, "failed to write audit log");
    }
}

// ---------------------------------------------------------------------------
// 后台接口：/api/admin/ip-bans
// ---------------------------------------------------------------------------

pub fn admin_router() -> Router<AppState> {
    Router::new()
        .route("/ip-bans", get(list_bans).post(add_ban))
        .route("/ip-bans/{ip}", delete(remove_ban))
}

#[derive(Serialize, FromRow)]
struct BanRow {
    ip: String,
    reason: String,
    hits: i64,
    created_at: DateTime<Utc>,
    linked_users: i64,
}

async fn list_bans(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let rows: Vec<BanRow> = sqlx::query_as(
        "SELECT b.ip, b.reason, b.hits, b.created_at, \
                (SELECT count(DISTINCT s.user_id) FROM sessions s WHERE s.ip = b.ip)::bigint AS linked_users \
         FROM ip_bans b ORDER BY b.created_at DESC LIMIT 500",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "ip_bans": rows, "per_sec": state.ip_guard.per_sec })))
}

#[derive(Deserialize)]
struct AddBan {
    ip: String,
    reason: Option<String>,
    /// 是否连带封禁在该地址出现过的账号，默认 true。
    ban_users: Option<bool>,
}

async fn add_ban(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<AddBan>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let ip = normalize_ip(&body.ip).ok_or_else(|| AppError::bad_request("IP 地址格式不对"))?;
    if state.ip_guard.is_allowed(&ip) {
        return Err(AppError::bad_request("该地址在白名单 IP_GUARD_ALLOW 中"));
    }
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 200))
        .unwrap_or_else(|| "站长手动封禁".to_string());

    let banned_users = if body.ban_users.unwrap_or(true) {
        persist_ban(&state.pool, &ip, &reason, Some(actor.id)).await?
    } else {
        sqlx::query(
            "INSERT INTO ip_bans (ip, reason) VALUES ($1, $2) \
             ON CONFLICT (ip) DO UPDATE SET reason = EXCLUDED.reason",
        )
        .bind(&ip)
        .bind(&reason)
        .execute(&state.pool)
        .await?;
        audit(&state.pool, Some(actor.id), "ip.ban", &ip, json!({ "reason": reason })).await;
        0
    };
    state.ip_guard.insert(&ip);
    Ok(Json(json!({ "ok": true, "ip": ip, "banned_users": banned_users })))
}

#[derive(Deserialize)]
struct RemoveQuery {
    /// 同时解封因这次 IP 封禁被连带封掉的账号。
    unban_users: Option<bool>,
}

async fn remove_ban(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(ip): Path<String>,
    Query(q): Query<RemoveQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let ip = normalize_ip(&ip).ok_or_else(|| AppError::bad_request("IP 地址格式不对"))?;
    let deleted = sqlx::query("DELETE FROM ip_bans WHERE ip = $1")
        .bind(&ip)
        .execute(&state.pool)
        .await?
        .rows_affected();
    let in_memory = state.ip_guard.remove(&ip);
    if deleted == 0 && !in_memory {
        return Err(AppError::not_found("该地址不在封禁列表中"));
    }

    let mut unbanned = 0;
    if q.unban_users.unwrap_or(false) {
        unbanned = sqlx::query(
            "UPDATE users SET is_banned = FALSE, ban_reason = NULL, updated_at = now() \
             WHERE is_banned AND ban_reason = $2 AND id IN ( \
               SELECT user_id FROM sessions WHERE ip = $1 \
               UNION SELECT user_id FROM usage_logs WHERE client_ip = $1 AND user_id IS NOT NULL)",
        )
        .bind(&ip)
        .bind(ACCOUNT_REASON)
        .execute(&state.pool)
        .await?
        .rows_affected();
    }
    // 与线上一致：审计动作 ip.release；只有显式 unban_users=true 时才附带解封账号数。
    let detail = if q.unban_users.unwrap_or(false) {
        json!({ "unbanned_users": unbanned })
    } else {
        json!({})
    };
    audit(&state.pool, Some(actor.id), "ip.release", &ip, detail).await;
    if q.unban_users.unwrap_or(false) {
        Ok(Json(json!({ "ok": true, "ip": ip, "unbanned_users": unbanned })))
    } else {
        Ok(Json(json!({ "ok": true, "ip": ip })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_once_above_ceiling() {
        let g = IpGuard::new(3, HashSet::new());
        assert!(!g.hit("1.2.3.4", 100));
        assert!(!g.hit("1.2.3.4", 100));
        assert!(!g.hit("1.2.3.4", 100));
        assert!(g.hit("1.2.3.4", 100));
        assert!(!g.hit("1.2.3.4", 100));
        // 新的一秒重新计数
        assert!(!g.hit("1.2.3.4", 101));
    }

    #[test]
    fn allow_list_and_disabled() {
        let allow: HashSet<String> = ["10.0.0.1".to_string()].into_iter().collect();
        let g = IpGuard::new(1, allow);
        for _ in 0..5 {
            assert!(!g.hit("10.0.0.1", 1));
        }
        let off = IpGuard::new(0, HashSet::new());
        for _ in 0..100 {
            assert!(!off.hit("9.9.9.9", 1));
        }
    }

    #[test]
    fn normalizes() {
        assert_eq!(normalize_ip(" 1.2.3.4 ").as_deref(), Some("1.2.3.4"));
        assert_eq!(normalize_ip("2001:DB8::1").as_deref(), Some("2001:db8::1"));
        assert!(normalize_ip("not-an-ip").is_none());
    }
}
