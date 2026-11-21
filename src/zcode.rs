//! 「贡献 ZCode 账号」：免登录，向本机 magpie 服务要一个登录链接（还原自线上二进制行为）。
//!
//! GET /api/v1/zcode/contribute
//!   → GET http://127.0.0.1:3431/login?type=1，`Authorization: Bearer {ZCODE_MAGPIE_KEY}`
//!   → 成功返回 `{ "id": <magpie.id 或 "">, "url": <magpie.url> }`
//! 未配置 ZCODE_MAGPIE_KEY → 503 zcode_disabled「贡献功能未开放」。

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

pub const MAGPIE_LOGIN_URL: &str = "http://127.0.0.1:3431/login?type=1";

pub async fn contribute(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let key = state
        .zcode_magpie_key
        .as_deref()
        .ok_or_else(|| AppError::new(StatusCode::SERVICE_UNAVAILABLE, "zcode_disabled", "贡献功能未开放"))?;
    let resp = state
        .plain_http
        .get(MAGPIE_LOGIN_URL)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|e| AppError::internal(format!("magpie 请求失败: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(AppError::internal(format!("magpie 返回 {status}")));
    }
    let value: Value = resp
        .json()
        .await
        .map_err(|e| AppError::internal(format!("magpie 响应解析失败: {e}")))?;
    pick_login(&value)
        .map(Json)
        .ok_or_else(|| AppError::internal("magpie 未返回登录链接"))
}

/// 从 magpie 响应里取 url（非空字符串）与 id（字符串，否则空串）。
pub fn pick_login(value: &Value) -> Option<Value> {
    let url = value.get("url").and_then(Value::as_str).filter(|s| !s.is_empty())?;
    let id = value.get("id").and_then(Value::as_str).unwrap_or("");
    Some(json!({ "id": id, "url": url }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_login_matches_prod_behavior() {
        assert_eq!(
            pick_login(&json!({"id": "abc123", "url": "https://e/v1", "extra": 1})),
            Some(json!({"id": "abc123", "url": "https://e/v1"}))
        );
        assert_eq!(pick_login(&json!({"url": "https://e/u1"})), Some(json!({"id": "", "url": "https://e/u1"})));
        assert_eq!(pick_login(&json!({"id": 42, "url": ""})), None);
        assert_eq!(pick_login(&json!({"url": null})), None);
        assert_eq!(pick_login(&json!({"login_url": "x"})), None);
        assert_eq!(pick_login(&json!({"data": {"url": "x"}})), None);
    }
}
