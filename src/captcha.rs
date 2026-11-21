//! 双通道人机验证（2026-11-14）：hCaptcha 为主，Cloudflare Turnstile 为备用。
//!
//! 设计（用户定）：
//! - 平时走 hCaptcha；hCaptcha 额度用尽/限流/超时/不可达时自动切到 Turnstile；
//!   hCaptcha 一恢复就立即换回（Turnstile 太容易被 AI 绕过，能不用就不用）。
//! - `GET  /api/captcha/provider` → `{"provider": "hcaptcha"|"turnstile", "sitekey": "..."}`，
//!   前端按返回渲染对应控件；token 连同 provider 一起发给后端校验。
//! - 通道状态（模式/当前通道/连续失败数/冷却截止）持久化在 settings 表（key=captcha_provider），
//!   服务重启不丢失；切换记审计（captcha.provider_switch / provider_recovered / provider_override）。
//! - hCaptcha token、Turnstile token 都一次有效，服务端不再维护一次性记录。
//! - 任一通道未配置时自动回落到另一通道；两个都未配置 → 503 captcha_disabled。

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;

pub const HCAPTCHA_SITEVERIFY_URL: &str = "https://api.hcaptcha.com/siteverify";
pub const TURNSTILE_SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
pub const TURNSTILE_ACTION: &str = "checkin";
pub const SETTINGS_KEY: &str = "captcha_provider";
/// 当前通道连续多少次服务端失败后切换到另一通道。
pub const FAILURE_THRESHOLD: u32 = 3;
/// 切到 Turnstile 后，隔多久再试探 hCaptcha 是否恢复（秒）。
pub const COOLDOWN_SECS: i64 = 600;
/// 试探 hCaptcha 存活的超时（秒）。
pub const PROBE_TIMEOUT_SECS: u64 = 8;

pub const MSG_INVALID: &str = "验证码无效，请再试一次";
pub const MSG_FAILED: &str = "人机验证失败，请重新验证";
pub const MSG_DISABLED: &str = "人机验证未配置";
pub const MSG_CONFIG: &str = "人机验证配置错误，请联系站长";
pub const MSG_UNAVAILABLE: &str = "人机验证服务暂时不可用，请重新验证";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Hcaptcha,
    Turnstile,
}

impl Provider {
    pub fn other(self) -> Provider {
        match self {
            Provider::Hcaptcha => Provider::Turnstile,
            Provider::Turnstile => Provider::Hcaptcha,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Hcaptcha => "hcaptcha",
            Provider::Turnstile => "turnstile",
        }
    }
    pub fn parse(s: &str) -> Option<Provider> {
        match s {
            "hcaptcha" => Some(Provider::Hcaptcha),
            "turnstile" => Some(Provider::Turnstile),
            _ => None,
        }
    }
}

impl Default for Provider {
    fn default() -> Self {
        Provider::Hcaptcha
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderMode {
    Auto,
    Hcaptcha,
    Turnstile,
}

impl Default for ProviderMode {
    fn default() -> Self {
        ProviderMode::Auto
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderState {
    #[serde(default)]
    pub mode: ProviderMode,
    #[serde(default)]
    pub active: Provider,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub down_until: Option<i64>,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self { mode: ProviderMode::Auto, active: Provider::Hcaptcha, failures: 0, down_until: None }
    }
}

impl ProviderState {
    /// 当前生效通道：强制模式用强制值，auto 用 active。
    pub fn effective(&self) -> Provider {
        match self.mode {
            ProviderMode::Hcaptcha => Provider::Hcaptcha,
            ProviderMode::Turnstile => Provider::Turnstile,
            ProviderMode::Auto => self.active,
        }
    }

    /// 当前通道连续失败一次；达到阈值且为 auto 模式、另一通道可用时切换。
    /// 返回 (from, to) 表示发生了切换。
    pub fn record_failure(&mut self, now: i64, other_configured: bool) -> Option<(Provider, Provider)> {
        self.failures = self.failures.saturating_add(1);
        if self.mode != ProviderMode::Auto || self.failures < FAILURE_THRESHOLD || !other_configured {
            return None;
        }
        let from = self.active;
        let to = from.other();
        self.active = to;
        self.failures = 0;
        self.down_until = if to == Provider::Turnstile {
            Some(now + COOLDOWN_SECS)
        } else {
            None
        };
        Some((from, to))
    }

    /// 当前通道成功一次，清零连续失败数。
    pub fn record_success(&mut self) {
        self.failures = 0;
    }
}

#[derive(Clone, Debug)]
pub struct CaptchaConfig {
    pub hcaptcha_sitekey: Option<String>,
    pub hcaptcha_secret: Option<String>,
    pub turnstile_sitekey: Option<String>,
    pub turnstile_secret: Option<String>,
    /// Turnstile siteverify 的 hostname 允许列表（TURNSTILE_HOSTNAMES，逗号分隔）。
    pub turnstile_hostnames: Vec<String>,
}

impl CaptchaConfig {
    pub fn from_env() -> Self {
        let non_empty = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let hostnames = std::env::var("TURNSTILE_HOSTNAMES")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|h| h.trim().to_lowercase())
                    .filter(|h| !h.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            hcaptcha_sitekey: non_empty("HCAPTCHA_SITEKEY"),
            hcaptcha_secret: non_empty("HCAPTCHA_SECRET"),
            turnstile_sitekey: non_empty("TURNSTILE_SITEKEY"),
            turnstile_secret: non_empty("TURNSTILE_SECRET"),
            turnstile_hostnames: hostnames,
        }
    }

    pub fn sitekey(&self, provider: Provider) -> Option<&str> {
        match provider {
            Provider::Hcaptcha => self.hcaptcha_sitekey.as_deref(),
            Provider::Turnstile => self.turnstile_sitekey.as_deref(),
        }
    }

    /// 通道可用：sitekey 与 secret 都已配置（Turnstile 还要求 hostname 允许列表非空）。
    pub fn configured(&self, provider: Provider) -> bool {
        match provider {
            Provider::Hcaptcha => self.hcaptcha_sitekey.is_some() && self.hcaptcha_secret.is_some(),
            Provider::Turnstile => {
                self.turnstile_sitekey.is_some()
                    && self.turnstile_secret.is_some()
                    && !self.turnstile_hostnames.is_empty()
            }
        }
    }
}

fn disabled() -> AppError {
    AppError::new(StatusCode::SERVICE_UNAVAILABLE, "captcha_disabled", MSG_DISABLED)
}

pub async fn load_provider_state(pool: &PgPool) -> ProviderState {
    let row: Option<(Value,)> = sqlx::query_as("SELECT value FROM settings WHERE key = $1")
        .bind(SETTINGS_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    match row {
        Some((v,)) => serde_json::from_value(v).unwrap_or_default(),
        None => ProviderState::default(),
    }
}

pub async fn save_provider_state(pool: &PgPool, state: &ProviderState) {
    let value = serde_json::to_string(state).unwrap_or_else(|_| "{}".to_string());
    let r = sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2::jsonb) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(SETTINGS_KEY)
    .bind(value)
    .execute(pool)
    .await;
    if let Err(e) = r {
        tracing::warn!(error = %e, "保存 captcha_provider 状态失败");
    }
}

async fn audit_provider(pool: &PgPool, actor_id: Option<i64>, action: &str, detail: Value) {
    let _ = sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
         VALUES ($1, $2, 'captcha_provider', 'provider', $3)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(detail)
    .execute(pool)
    .await;
}

/// 校验结果：Ok 通过；User 是用户 token 问题（400，不切换通道）；
/// Provider 是通道服务端问题（计失败、可能触发切换）。
#[derive(Debug)]
enum VerifyOutcome {
    Ok,
    User(AppError),
    Provider(String),
}

fn error_codes(value: &Value) -> Vec<&str> {
    value
        .get("error-codes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

fn check_hcaptcha_response(value: &Value) -> VerifyOutcome {
    if value.get("success").and_then(Value::as_bool) == Some(true) {
        return VerifyOutcome::Ok;
    }
    let codes = error_codes(value);
    if codes.iter().any(|c| *c == "invalid-input-secret" || *c == "sitekey-secret-mismatch") {
        tracing::error!(error_codes = ?codes, "hCaptcha secret 无效，请检查 HCAPTCHA_SECRET 配置");
        return VerifyOutcome::Provider("hCaptcha secret 无效".to_string());
    }
    // 限流/额度耗尽属于通道级故障（不是用户 token 的问题）：计失败，满 3 次触发切到 Turnstile。
    // 注意：hCaptcha 额度耗尽时若只返回普通 invalid-input-response，服务端无法从单次
    // 响应区分，只能靠连续失败计数兜底；恢复探测（假 token）同样只能确认服务可达与 secret 有效。
    if codes.iter().any(|c| *c == "rate-limited") {
        tracing::warn!(error_codes = ?codes, "hCaptcha 限流/额度不足，计为通道故障");
        return VerifyOutcome::Provider("hCaptcha 限流/额度不足".to_string());
    }
    VerifyOutcome::User(AppError::bad_request(MSG_FAILED))
}

fn check_turnstile_response(value: &Value, allowed_hostnames: &[String]) -> VerifyOutcome {
    let codes = error_codes(value);
    if codes.iter().any(|c| *c == "invalid-input-secret") {
        tracing::error!(error_codes = ?codes, "Turnstile secret 无效，请检查 TURNSTILE_SECRET 配置");
        return VerifyOutcome::Provider("Turnstile secret 无效".to_string());
    }
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return VerifyOutcome::User(AppError::bad_request(MSG_FAILED));
    }
    if value.get("action").and_then(Value::as_str) != Some(TURNSTILE_ACTION) {
        tracing::warn!("Turnstile action 不匹配");
        return VerifyOutcome::User(AppError::bad_request(MSG_FAILED));
    }
    let hostname = value.get("hostname").and_then(Value::as_str).unwrap_or("");
    if allowed_hostnames.is_empty()
        || !allowed_hostnames.iter().any(|h| h.eq_ignore_ascii_case(hostname))
    {
        tracing::warn!(hostname = %hostname, "Turnstile hostname 不在允许列表");
        return VerifyOutcome::Provider(format!("Turnstile hostname 不在允许列表: {hostname}"));
    }
    VerifyOutcome::Ok
}

async fn verify_hcaptcha(
    client: &reqwest::Client,
    cfg: &CaptchaConfig,
    token: &str,
    remote_ip: Option<&str>,
) -> VerifyOutcome {
    let Some(secret) = cfg.hcaptcha_secret.as_deref() else {
        return VerifyOutcome::Provider("hCaptcha secret 未配置".to_string());
    };
    let mut form: Vec<(&str, &str)> = vec![("secret", secret), ("response", token)];
    if let Some(sitekey) = cfg.hcaptcha_sitekey.as_deref() {
        form.push(("sitekey", sitekey));
    }
    if let Some(ip) = remote_ip.filter(|s| !s.is_empty()) {
        form.push(("remoteip", ip));
    }
    let resp = match client.post(HCAPTCHA_SITEVERIFY_URL).form(&form).send().await {
        Ok(r) => r,
        Err(e) => return VerifyOutcome::Provider(format!("hCaptcha 请求失败: {e}")),
    };
    if !resp.status().is_success() {
        return VerifyOutcome::Provider(format!("hCaptcha 返回 HTTP {}", resp.status()));
    }
    match resp.json::<Value>().await {
        Ok(v) => check_hcaptcha_response(&v),
        Err(e) => VerifyOutcome::Provider(format!("hCaptcha 响应解析失败: {e}")),
    }
}

async fn verify_turnstile(
    client: &reqwest::Client,
    cfg: &CaptchaConfig,
    token: &str,
    remote_ip: Option<&str>,
) -> VerifyOutcome {
    let Some(secret) = cfg.turnstile_secret.as_deref() else {
        return VerifyOutcome::Provider("Turnstile secret 未配置".to_string());
    };
    let mut form: Vec<(&str, &str)> = vec![("secret", secret), ("response", token)];
    if let Some(ip) = remote_ip.filter(|s| !s.is_empty()) {
        form.push(("remoteip", ip));
    }
    let resp = match client.post(TURNSTILE_SITEVERIFY_URL).form(&form).send().await {
        Ok(r) => r,
        Err(e) => return VerifyOutcome::Provider(format!("Turnstile 请求失败: {e}")),
    };
    if !resp.status().is_success() {
        return VerifyOutcome::Provider(format!("Turnstile 返回 HTTP {}", resp.status()));
    }
    match resp.json::<Value>().await {
        Ok(v) => check_turnstile_response(&v, &cfg.turnstile_hostnames),
        Err(e) => VerifyOutcome::Provider(format!("Turnstile 响应解析失败: {e}")),
    }
}

/// 试探 hCaptcha 是否恢复：用假 token 调 siteverify，只要服务可达且配置有效
///（返回体可解析、且不含 secret 错误）即认为恢复。
async fn probe_hcaptcha(client: &reqwest::Client, cfg: &CaptchaConfig) -> bool {
    let Some(secret) = cfg.hcaptcha_secret.as_deref() else {
        return false;
    };
    let resp = client
        .post(HCAPTCHA_SITEVERIFY_URL)
        .form(&[("secret", secret), ("response", "probe")])
        .timeout(std::time::Duration::from_secs(PROBE_TIMEOUT_SECS))
        .send()
        .await;
    let Ok(resp) = resp else { return false };
    if !resp.status().is_success() {
        return false;
    }
    let Ok(value) = resp.json::<Value>().await else {
        return false;
    };
    let codes = error_codes(&value);
    !codes
        .iter()
        .any(|c| *c == "invalid-input-secret" || *c == "sitekey-secret-mismatch")
}

/// 前端取当前通道及对应 sitekey。附带另一已配置通道作为 fallback，
/// 供前端在主通道脚本加载/渲染失败时就地切换（无需再请求一次）；
/// sitekey 本就是公开配置，两通道的都给出无妨。
pub async fn provider(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (provider, sitekey) = resolve_provider(&state).await?;
    let cfg = &state.captcha;
    let other = provider.other();
    let fallback = if cfg.configured(other) {
        Some(json!({ "provider": other.as_str(), "sitekey": cfg.sitekey(other).unwrap() }))
    } else {
        None
    };
    Ok(Json(json!({ "provider": provider.as_str(), "sitekey": sitekey, "fallback": fallback })))
}

async fn resolve_provider(state: &AppState) -> AppResult<(Provider, String)> {
    let mut st = load_provider_state(&state.pool).await;
    let cfg = &state.captcha;
    // auto 模式 + 当前在 Turnstile + 冷却已过 → 试探 hCaptcha 是否恢复
    if st.mode == ProviderMode::Auto
        && st.active == Provider::Turnstile
        && cfg.configured(Provider::Hcaptcha)
    {
        let now = Utc::now().timestamp();
        if st.down_until.map_or(true, |until| now >= until) {
            if probe_hcaptcha(&state.plain_http, cfg).await {
                st.active = Provider::Hcaptcha;
                st.failures = 0;
                st.down_until = None;
                save_provider_state(&state.pool, &st).await;
                audit_provider(
                    &state.pool,
                    None,
                    "captcha.provider_recovered",
                    json!({"from": "turnstile", "to": "hcaptcha", "reason": "probe_ok"}),
                )
                .await;
            } else {
                st.down_until = Some(now + COOLDOWN_SECS);
                save_provider_state(&state.pool, &st).await;
            }
        }
    }
    let active = st.effective();
    if !cfg.configured(active) {
        // 强制模式下通道未配置 → 503；auto 模式可回落到另一通道
        if st.mode == ProviderMode::Auto {
            let other = active.other();
            if cfg.configured(other) {
                return Ok((other, cfg.sitekey(other).unwrap().to_string()));
            }
        }
        return Err(disabled());
    }
    Ok((active, cfg.sitekey(active).unwrap().to_string()))
}

/// token 为空时返回的错误（纯函数，便于单测）。
fn require_token(token: Option<&str>) -> AppResult<&str> {
    let token = token.unwrap_or("").trim();
    if token.is_empty() {
        return Err(AppError::bad_request(MSG_INVALID));
    }
    Ok(token)
}

/// 签到/补签前调用：按当前通道（或前端提示的通道）校验 token。
/// hint 是前端实际渲染的通道（"hcaptcha"/"turnstile"），用于用户刚解出、但服务端
/// 已切换通道的竞态：以用户实际解出的通道为准校验。
pub async fn check_checkin_token(
    state: &AppState,
    token: Option<&str>,
    hint: Option<&str>,
    remote_ip: Option<&str>,
) -> AppResult<()> {
    let token = require_token(token)?;
    let cfg = &state.captcha;
    let mut st = load_provider_state(&state.pool).await;
    let hint_provider = hint.and_then(Provider::parse);
    let mut verify_provider = hint_provider.unwrap_or_else(|| st.effective());
    if !cfg.configured(verify_provider) {
        let fallback = st.effective();
        if cfg.configured(fallback) {
            verify_provider = fallback;
        } else {
            return Err(disabled());
        }
    }
    let outcome = match verify_provider {
        Provider::Hcaptcha => verify_hcaptcha(&state.plain_http, cfg, token, remote_ip).await,
        Provider::Turnstile => verify_turnstile(&state.plain_http, cfg, token, remote_ip).await,
    };
    match outcome {
        VerifyOutcome::Ok => {
            // hCaptcha 解出且当前在 Turnstile（auto）→ hCaptcha 已恢复，立即换回
            if verify_provider == Provider::Hcaptcha
                && st.mode == ProviderMode::Auto
                && st.active == Provider::Turnstile
            {
                st.active = Provider::Hcaptcha;
                st.failures = 0;
                st.down_until = None;
                save_provider_state(&state.pool, &st).await;
                audit_provider(
                    &state.pool,
                    None,
                    "captcha.provider_recovered",
                    json!({"from": "turnstile", "to": "hcaptcha", "reason": "verify_ok"}),
                )
                .await;
            } else if verify_provider == st.effective() && st.failures > 0 {
                st.record_success();
                save_provider_state(&state.pool, &st).await;
            }
            Ok(())
        }
        VerifyOutcome::User(e) => Err(e),
        VerifyOutcome::Provider(reason) => {
            // 只有用当前生效通道验证失败时才计失败、触发切换
            if verify_provider == st.effective() {
                let now = Utc::now().timestamp();
                let other_configured = cfg.configured(verify_provider.other());
                if let Some((from, to)) = st.record_failure(now, other_configured) {
                    save_provider_state(&state.pool, &st).await;
                    audit_provider(
                        &state.pool,
                        None,
                        "captcha.provider_switch",
                        json!({"from": from.as_str(), "to": to.as_str(), "reason": reason}),
                    )
                    .await;
                } else {
                    save_provider_state(&state.pool, &st).await;
                }
            }
            Err(AppError::new(StatusCode::SERVICE_UNAVAILABLE, "captcha_unavailable", MSG_UNAVAILABLE))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_user(outcome: VerifyOutcome, msg: &str) {
        match outcome {
            VerifyOutcome::User(e) => {
                assert_eq!(e.code, "invalid_request");
                assert_eq!(e.message, msg);
            }
            other => panic!("expected User error, got {other:?}"),
        }
    }

    fn assert_provider(outcome: VerifyOutcome) {
        match outcome {
            VerifyOutcome::Provider(_) => {}
            other => panic!("expected Provider error, got {other:?}"),
        }
    }

    #[test]
    fn hcaptcha_response_outcomes() {
        assert!(matches!(check_hcaptcha_response(&json!({"success": true})), VerifyOutcome::Ok));
        assert_user(
            check_hcaptcha_response(&json!({"success": false, "error-codes": ["invalid-input-response"]})),
            MSG_FAILED,
        );
        assert_user(
            check_hcaptcha_response(&json!({"success": false, "error-codes": ["expired-input-response"]})),
            MSG_FAILED,
        );
        assert_user(check_hcaptcha_response(&json!({"success": false})), MSG_FAILED);
        assert_user(check_hcaptcha_response(&json!({})), MSG_FAILED);
        assert_provider(check_hcaptcha_response(
            &json!({"success": false, "error-codes": ["invalid-input-secret"]}),
        ));
        assert_provider(check_hcaptcha_response(
            &json!({"success": false, "error-codes": ["sitekey-secret-mismatch"]}),
        ));
        // 限流/额度不足 → 通道故障（计失败、可触发切换），不是用户 token 问题
        assert_provider(check_hcaptcha_response(
            &json!({"success": false, "error-codes": ["rate-limited"]}),
        ));
    }

    #[test]
    fn turnstile_response_outcomes() {
        let hosts = vec!["plan.pmcat.top".to_string()];
        assert!(matches!(
            check_turnstile_response(
                &json!({"success": true, "action": "checkin", "hostname": "plan.pmcat.top"}),
                &hosts
            ),
            VerifyOutcome::Ok
        ));
        // action 不匹配 → 用户 token 问题
        assert_user(
            check_turnstile_response(
                &json!({"success": true, "action": "login", "hostname": "plan.pmcat.top"}),
                &hosts
            ),
            MSG_FAILED,
        );
        // hostname 不在允许列表 → 通道问题（触发回落）
        assert_provider(check_turnstile_response(
            &json!({"success": true, "action": "checkin", "hostname": "evil.example.com"}),
            &hosts
        ));
        // 允许列表为空 → 通道问题
        assert_provider(check_turnstile_response(
            &json!({"success": true, "action": "checkin", "hostname": "plan.pmcat.top"}),
            &[]
        ));
        // 假 token → 用户问题
        assert_user(
            check_turnstile_response(
                &json!({"success": false, "error-codes": ["invalid-input-response"]}),
                &hosts
            ),
            MSG_FAILED,
        );
        // secret 配错 → 通道问题
        assert_provider(check_turnstile_response(
            &json!({"success": false, "error-codes": ["invalid-input-secret"]}),
            &hosts
        ));
    }

    #[test]
    fn provider_state_defaults_and_roundtrip() {
        let st = ProviderState::default();
        assert_eq!(st.mode, ProviderMode::Auto);
        assert_eq!(st.active, Provider::Hcaptcha);
        assert_eq!(st.effective(), Provider::Hcaptcha);
        let v = serde_json::to_value(&st).unwrap();
        let back: ProviderState = serde_json::from_value(v).unwrap();
        assert_eq!(back.mode, ProviderMode::Auto);
        assert_eq!(back.active, Provider::Hcaptcha);
        assert_eq!(back.failures, 0);
        assert_eq!(back.down_until, None);
        // 缺字段的旧数据也能解析（默认值兜底）
        let partial: ProviderState = serde_json::from_value(json!({"mode": "auto"})).unwrap();
        assert_eq!(partial.active, Provider::Hcaptcha);
    }

    #[test]
    fn record_failure_switches_after_threshold() {
        let now = 1_794_000_000;
        let mut st = ProviderState::default();
        assert_eq!(st.record_failure(now, true), None);
        assert_eq!(st.record_failure(now, true), None);
        assert_eq!(
            st.record_failure(now, true),
            Some((Provider::Hcaptcha, Provider::Turnstile))
        );
        assert_eq!(st.active, Provider::Turnstile);
        assert_eq!(st.failures, 0);
        assert_eq!(st.down_until, Some(now + COOLDOWN_SECS));
        // Turnstile 再连续失败 3 次 → 切回 hCaptcha
        assert_eq!(st.record_failure(now, true), None);
        assert_eq!(st.record_failure(now, true), None);
        assert_eq!(
            st.record_failure(now, true),
            Some((Provider::Turnstile, Provider::Hcaptcha))
        );
        assert_eq!(st.down_until, None);
    }

    #[test]
    fn record_failure_no_switch_when_forced_or_other_missing() {
        let now = 1_794_000_000;
        let mut st = ProviderState { mode: ProviderMode::Hcaptcha, ..Default::default() };
        for _ in 0..5 {
            assert_eq!(st.record_failure(now, true), None);
        }
        assert_eq!(st.active, Provider::Hcaptcha);
        let mut st = ProviderState::default();
        for _ in 0..5 {
            assert_eq!(st.record_failure(now, false), None);
        }
        assert_eq!(st.active, Provider::Hcaptcha);
    }

    #[test]
    fn record_success_resets_failures() {
        let mut st = ProviderState::default();
        st.record_failure(1_794_000_000, true);
        assert_eq!(st.failures, 1);
        st.record_success();
        assert_eq!(st.failures, 0);
    }

    #[test]
    fn require_token_rejects_empty() {
        assert_eq!(require_token(None).unwrap_err().message, MSG_INVALID);
        assert_eq!(require_token(Some("   ")).unwrap_err().message, MSG_INVALID);
        assert_eq!(require_token(Some("  tok  ")).unwrap(), "tok");
    }
}
