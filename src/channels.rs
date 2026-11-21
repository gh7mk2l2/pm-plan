use crate::models::Channel;
use crate::state::AppState;
use futures_util::stream::{self, StreamExt};
use serde_json::Value;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct ChannelCheck {
    pub ok: bool,
    pub status: u16,
    pub models: Vec<String>,
    pub error: Option<String>,
    pub latency_ms: i64,
}

pub fn parse_model_ids(value: &Value) -> Vec<String> {
    let array = value
        .as_array()
        .or_else(|| value.get("data").and_then(Value::as_array))
        .or_else(|| value.get("models").and_then(Value::as_array));
    let Some(items) = array else {
        return Vec::new();
    };
    let mut models: Vec<String> = items
        .iter()
        .filter_map(|item| {
            item.get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
                .or_else(|| item.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .collect();
    models.sort();
    models.dedup();
    models
}

pub async fn fetch_upstream_models(
    state: &AppState,
    base_url: &str,
    api_key: &str,
) -> Result<(u16, Vec<String>), (u16, String)> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let response = state
        .http
        .get(&url)
        .bearer_auth(api_key)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|err| (0, format!("请求上游失败：{err}")))?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|err| (status, format!("读取上游响应失败：{err}")))?;
    if !(200..300).contains(&status) {
        return Err((
            status,
            format!(
                "HTTP {}：{}",
                status,
                crate::util::truncate_chars(text.trim(), 300)
            ),
        ));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|err| (status, format!("上游返回不是合法 JSON：{err}")))?;
    let models = parse_model_ids(&value);
    Ok((status, models))
}

pub async fn check_channel(state: &AppState, channel_id: i64) -> Result<ChannelCheck, String> {
    let channel: Option<Channel> = sqlx::query_as("SELECT * FROM channels WHERE id = $1")
        .bind(channel_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|err| format!("读取渠道失败：{err}"))?;
    let channel = channel.ok_or_else(|| "渠道不存在".to_string())?;
    let api_key = state
        .crypto
        .decrypt(&channel.api_key_enc)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|err| format!("渠道密钥解密失败：{err}"))?;

    let started = Instant::now();
    let result = fetch_upstream_models(state, &channel.base_url, &api_key).await;
    let latency_ms = started.elapsed().as_millis() as i64;

    let (ok, status, models, error) = match result {
        Ok((status, models)) => (true, status, models, None),
        Err((status, message)) => (false, status, Vec::new(), Some(message)),
    };

    let health = if ok { "healthy" } else { "error" };
    let _ = sqlx::query(
        "UPDATE channels SET health = $2, last_health_at = now(), last_error = $3, updated_at = now() \
         WHERE id = $1",
    )
    .bind(channel_id)
    .bind(health)
    .bind(error.as_deref().map(|e| crate::util::truncate_chars(e, 500)))
    .execute(&state.pool)
    .await;

    Ok(ChannelCheck {
        ok,
        status,
        models,
        error,
        latency_ms,
    })
}

pub async fn health_loop(state: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    loop {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM channels WHERE enabled ORDER BY id")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
        if !ids.is_empty() {
            let checks = ids.into_iter().map(|id| {
                let state = state.clone();
                async move {
                    if let Err(err) = check_channel(&state, id).await {
                        tracing::warn!(channel_id = id, error = %err, "channel health check failed");
                    }
                }
            });
            stream::iter(checks).buffer_unordered(4).collect::<Vec<_>>().await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
}

// ---------------------------------------------------------------------------
// 渠道模型检测（New-API 式）：用渠道 key + upstream_model 发一个最小 chat/completions。
// 不写 usage_logs，不计入任何用户用量。
// ---------------------------------------------------------------------------

/// 模型检测请求的超时。
pub const MODEL_TEST_TIMEOUT_SECS: u64 = 60;
/// 检测请求的 max_tokens。取一个很小但对推理模型也不至于直接报错的值。
pub const MODEL_TEST_MAX_TOKENS: u64 = 16;
/// 批量检测的并发上限。
pub const MODEL_TEST_CONCURRENCY: usize = 3;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ModelTestResult {
    pub channel_id: i64,
    pub model: String,
    pub upstream_model: String,
    pub ok: bool,
    pub latency_ms: i64,
    /// 上游 HTTP 状态码，0 表示请求没发出去 / 超时。
    pub status: u16,
    /// 失败时的一行摘要。
    pub error: Option<String>,
    /// 失败时较完整的错误（最多 2000 字符），供前端悬停查看。
    pub error_detail: Option<String>,
    /// 成功时回复内容的前 80 个字符。
    pub reply: Option<String>,
    /// 该模型是否是渠道里已配置的模型（未配置的临时探测不会改动渠道健康状态）。
    pub configured: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TestTarget {
    id: Option<i64>,
    upstream_model: Option<String>,
}

/// 检测请求体与路径。`endpoint` = "responses" 时走 /responses，否则 /chat/completions。
pub fn test_request(endpoint: &str, upstream_model: &str) -> (&'static str, Value) {
    if endpoint == "responses" {
        (
            "/responses",
            serde_json::json!({
                "model": upstream_model,
                "input": "hi",
                "max_output_tokens": MODEL_TEST_MAX_TOKENS,
                "stream": false,
            }),
        )
    } else {
        ("/chat/completions", test_request_body(upstream_model))
    }
}

pub fn test_request_body(upstream_model: &str) -> Value {
    serde_json::json!({
        "model": upstream_model,
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": MODEL_TEST_MAX_TOKENS,
        "stream": false,
    })
}

pub fn is_valid_test_endpoint(value: &str) -> bool {
    matches!(value, "chat" | "responses")
}

/// 从上游错误体里提炼一行摘要：优先 error.message / message / detail，其次原文前 200 字。
pub fn summarize_error(status: u16, body: &str) -> String {
    let trimmed = body.trim();
    let from_json = serde_json::from_str::<Value>(trimmed).ok().and_then(|v| {
        let candidates = [
            v.pointer("/error/message"),
            v.get("error").filter(|e| e.is_string()),
            v.get("message"),
            v.get("detail"),
            v.get("msg"),
        ];
        candidates
            .into_iter()
            .flatten()
            .find_map(|x| x.as_str().map(|s| s.to_string()))
    });
    let text = from_json.unwrap_or_else(|| trimmed.to_string());
    let text = crate::util::truncate_chars(text.trim(), 200);
    if status == 0 {
        text
    } else if text.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status}：{text}")
    }
}

/// 判断一个 2xx 响应体是不是真的成功（有的上游会 200 + {"error":…}）。
pub fn success_body(body: &str) -> Result<Option<String>, String> {
    let value: Value = serde_json::from_str(body.trim())
        .map_err(|_| format!("上游返回不是合法 JSON：{}", crate::util::truncate_chars(body.trim(), 200)))?;
    if let Some(err) = value.get("error").filter(|e| !e.is_null()) {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| err.to_string());
        return Err(crate::util::truncate_chars(&msg, 200));
    }
    let reply = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/choices/0/message/reasoning_content").and_then(Value::as_str))
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))
        .or_else(|| value.get("output_text").and_then(Value::as_str))
        .map(|s| s.to_string())
        .or_else(|| {
            // Responses 格式：output[].content[].text
            let mut text = String::new();
            for item in value.get("output").and_then(Value::as_array).into_iter().flatten() {
                for part in item.get("content").and_then(Value::as_array).into_iter().flatten() {
                    if let Some(t) = part.get("text").and_then(Value::as_str) {
                        text.push_str(t);
                    }
                }
            }
            (!text.is_empty()).then_some(text)
        })
        .map(|s| crate::util::truncate_chars(s.trim(), 80));
    if value.get("choices").is_none() && value.get("output").is_none() && value.get("id").is_none() {
        return Err(format!(
            "上游响应缺少 choices：{}",
            crate::util::truncate_chars(body.trim(), 200)
        ));
    }
    Ok(reply)
}

/// 检测某渠道的某个模型。`model` 是对外模型名；渠道里有该模型时用其 upstream_model。
pub async fn test_channel_model(
    state: &AppState,
    channel_id: i64,
    model: &str,
) -> Result<ModelTestResult, String> {
    let channel: Option<Channel> = sqlx::query_as("SELECT * FROM channels WHERE id = $1")
        .bind(channel_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|err| format!("读取渠道失败：{err}"))?;
    let channel = channel.ok_or_else(|| "渠道不存在".to_string())?;
    let model = model.trim();
    if model.is_empty() {
        return Err("缺少要检测的模型".to_string());
    }
    let target: Option<TestTarget> = sqlx::query_as(
        "SELECT id, upstream_model FROM channel_models WHERE channel_id = $1 AND model = $2 ORDER BY id LIMIT 1",
    )
    .bind(channel_id)
    .bind(model)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| format!("读取渠道模型失败：{err}"))?;
    let configured = target.is_some();
    let model_row_id = target.as_ref().and_then(|t| t.id);
    let upstream_model = target
        .and_then(|t| t.upstream_model)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| model.to_string());

    let api_key = state
        .crypto
        .decrypt(&channel.api_key_enc)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|err| format!("渠道密钥解密失败：{err}"))?;

    let (path, mut body) = test_request(&channel.test_endpoint, &upstream_model);
    crate::channel_opts::apply_channel_body_rules(
        &mut body,
        channel.strip_reasoning_with_tools,
        &channel.param_override,
    );
    let url = format!("{}{}", channel.base_url.trim_end_matches('/'), path);
    let mut request = state
        .http
        .post(&url)
        .bearer_auth(&api_key)
        .timeout(std::time::Duration::from_secs(MODEL_TEST_TIMEOUT_SECS))
        .json(&body);
    let extra = crate::channel_opts::header_pairs(&channel.extra_headers);
    if !extra.is_empty() {
        let mut map = axum::http::HeaderMap::new();
        for (name, value) in extra {
            map.insert(name, value);
        }
        request = request.headers(map);
    }

    let started = Instant::now();
    let outcome: Result<Option<String>, (u16, String, String)> = match request.send().await {
        Err(err) => {
            let msg = if err.is_timeout() {
                format!("请求超时（{MODEL_TEST_TIMEOUT_SECS} 秒）")
            } else {
                format!("请求上游失败：{err}")
            };
            Err((0, msg.clone(), msg))
        }
        Ok(resp) => {
            let status = resp.status().as_u16();
            match resp.text().await {
                Err(err) => {
                    let msg = format!("读取上游响应失败：{err}");
                    Err((status, msg.clone(), msg))
                }
                Ok(text) => {
                    if (200..300).contains(&status) {
                        success_body(&text).map_err(|summary| {
                            (status, format!("HTTP {status}：{summary}"), crate::util::truncate_chars(text.trim(), 2000))
                        })
                    } else {
                        Err((
                            status,
                            summarize_error(status, &text),
                            crate::util::truncate_chars(text.trim(), 2000),
                        ))
                    }
                }
            }
        }
    };
    let latency_ms = started.elapsed().as_millis() as i64;

    let result = match outcome {
        Ok(reply) => ModelTestResult {
            channel_id,
            model: model.to_string(),
            upstream_model,
            ok: true,
            latency_ms,
            status: 200,
            error: None,
            error_detail: None,
            reply,
            configured,
        },
        Err((status, summary, detail)) => ModelTestResult {
            channel_id,
            model: model.to_string(),
            upstream_model,
            ok: false,
            latency_ms,
            status,
            error: Some(summary),
            error_detail: Some(detail),
            reply: None,
            configured,
        },
    };

    if configured {
        let health = if result.ok { "healthy" } else { "error" };
        let err_text = result.error.as_deref().map(|e| crate::util::truncate_chars(e, 500));
        let _ = sqlx::query(
            "UPDATE channels SET health = $2, last_health_at = now(), last_error = $3, \
             last_test_at = now(), last_test_ok = $4, last_test_ms = $5, last_test_model = $6, \
             updated_at = now() WHERE id = $1",
        )
        .bind(channel_id)
        .bind(health)
        .bind(&err_text)
        .bind(result.ok)
        .bind(result.latency_ms.min(i32::MAX as i64) as i32)
        .bind(&result.model)
        .execute(&state.pool)
        .await;
        if let Some(row_id) = model_row_id {
            let _ = sqlx::query(
                "UPDATE channel_models SET last_test_at = now(), last_test_ok = $2, last_test_ms = $3, \
                 last_test_error = $4 WHERE id = $1",
            )
            .bind(row_id)
            .bind(result.ok)
            .bind(result.latency_ms.min(i32::MAX as i64) as i32)
            .bind(result.error_detail.as_deref().map(|e| crate::util::truncate_chars(e, 2000)))
            .execute(&state.pool)
            .await;
        }
    }
    Ok(result)
}

/// 渠道的默认测试模型：channels.test_model（需在渠道内），否则第一个启用的模型（按 id）。
pub async fn default_test_model(state: &AppState, channel_id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT cm.model FROM channel_models cm JOIN channels c ON c.id = cm.channel_id \
         WHERE cm.channel_id = $1 \
         ORDER BY (c.test_model IS NOT NULL AND cm.model = c.test_model) DESC, cm.enabled DESC, cm.id ASC \
         LIMIT 1",
    )
    .bind(channel_id)
    .fetch_optional(&state.pool)
    .await
}

/// 批量检测：对每个 (渠道, 模型) 并发（上限 3）检测。
pub async fn test_many(state: &AppState, targets: Vec<(i64, String)>) -> Vec<ModelTestResult> {
    let jobs = targets.into_iter().map(|(channel_id, model)| {
        let state = state.clone();
        async move {
            match test_channel_model(&state, channel_id, &model).await {
                Ok(r) => r,
                Err(err) => ModelTestResult {
                    channel_id,
                    upstream_model: model.clone(),
                    model,
                    ok: false,
                    latency_ms: 0,
                    status: 0,
                    error: Some(err.clone()),
                    error_detail: Some(err),
                    reply: None,
                    configured: false,
                },
            }
        }
    });
    stream::iter(jobs)
        .buffer_unordered(MODEL_TEST_CONCURRENCY)
        .collect::<Vec<_>>()
        .await
}

/// 「测试全部渠道」：每个启用渠道测其默认测试模型。
pub async fn test_all_channels(state: &AppState) -> Result<Vec<ModelTestResult>, sqlx::Error> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM channels WHERE enabled ORDER BY priority DESC, id ASC")
        .fetch_all(&state.pool)
        .await?;
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        match default_test_model(state, id).await? {
            Some(model) => targets.push((id, model)),
            None => skipped.push(id),
        }
    }
    let mut results = test_many(state, targets).await;
    for id in skipped {
        results.push(ModelTestResult {
            channel_id: id,
            model: String::new(),
            upstream_model: String::new(),
            ok: false,
            latency_ms: 0,
            status: 0,
            error: Some("渠道没有配置任何模型".into()),
            error_detail: None,
            reply: None,
            configured: false,
        });
    }
    results.sort_by_key(|r| r.channel_id);
    Ok(results)
}

/// 自动检测设置（settings 表）：channel_auto_test_minutes（0=关闭）、channel_auto_disable（失败自动禁用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct AutoTestSettings {
    pub interval_minutes: i64,
    pub auto_disable: bool,
}

pub async fn auto_test_settings(pool: &sqlx::PgPool) -> AutoTestSettings {
    let minutes: i64 = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::bigint FROM settings WHERE key = 'channel_auto_test_minutes'), 0)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let auto_disable: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::boolean FROM settings WHERE key = 'channel_auto_disable'), FALSE)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    AutoTestSettings {
        interval_minutes: minutes.max(0),
        auto_disable,
    }
}

/// 后台自动检测：每分钟看一次设置，到点就测全部启用渠道；开启「失败自动禁用」时禁用失败渠道。
pub async fn auto_test_loop(state: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(45)).await;
    let mut last_run: Option<Instant> = None;
    loop {
        let cfg = auto_test_settings(&state.pool).await;
        let due = cfg.interval_minutes > 0
            && last_run
                .map(|t| t.elapsed() >= std::time::Duration::from_secs(cfg.interval_minutes as u64 * 60))
                .unwrap_or(true);
        if due {
            last_run = Some(Instant::now());
            match test_all_channels(&state).await {
                Ok(results) => {
                    let failed: Vec<&ModelTestResult> =
                        results.iter().filter(|r| !r.ok && r.configured).collect();
                    tracing::info!(total = results.len(), failed = failed.len(), "channel auto test finished");
                    if cfg.auto_disable {
                        for r in failed {
                            let res = sqlx::query(
                                "UPDATE channels SET enabled = FALSE, updated_at = now() WHERE id = $1 AND enabled",
                            )
                            .bind(r.channel_id)
                            .execute(&state.pool)
                            .await;
                            if matches!(res, Ok(ref done) if done.rows_affected() > 0) {
                                tracing::warn!(channel_id = r.channel_id, error = ?r.error, "channel auto-disabled after failed test");
                                let _ = sqlx::query(
                                    "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
                                     VALUES (NULL, 'channel.auto_disable', 'channel', $1, $2)",
                                )
                                .bind(r.channel_id.to_string())
                                .bind(serde_json::json!({ "model": r.model, "error": r.error, "status": r.status }))
                                .execute(&state.pool)
                                .await;
                            }
                        }
                    }
                }
                Err(err) => tracing::warn!(error = %err, "channel auto test failed to start"),
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    }
}

#[cfg(test)]
mod model_test_tests {
    use super::*;

    #[test]
    fn test_body_is_minimal_and_non_streaming() {
        let b = test_request_body("gpt-6-astra");
        assert_eq!(b["model"], "gpt-6-astra");
        assert_eq!(b["stream"], false);
        assert_eq!(b["max_tokens"], MODEL_TEST_MAX_TOKENS);
        assert_eq!(b["messages"][0]["content"], "hi");
        let (path, r) = test_request("responses", "m");
        assert_eq!(path, "/responses");
        assert_eq!(r["input"], "hi");
        assert_eq!(r["stream"], false);
        assert_eq!(test_request("chat", "m").0, "/chat/completions");
        assert_eq!(test_request("weird", "m").0, "/chat/completions");
        assert!(is_valid_test_endpoint("chat") && is_valid_test_endpoint("responses") && !is_valid_test_endpoint("x"));
    }

    #[test]
    fn responses_success_body() {
        let body = r#"{"id":"resp_1","object":"response","output":[{"type":"reasoning"},{"type":"message","content":[{"type":"output_text","text":"Hello"}]}]}"#;
        assert_eq!(success_body(body).unwrap(), Some("Hello".to_string()));
    }

    #[test]
    fn error_summary_prefers_message() {
        let s = summarize_error(400, r#"{"error":{"message":"Function tools with reasoning_effort are not supported","type":"invalid_request_error"}}"#);
        assert_eq!(s, "HTTP 400：Function tools with reasoning_effort are not supported");
        assert_eq!(summarize_error(502, "  "), "HTTP 502");
        assert_eq!(summarize_error(404, "not here"), "HTTP 404：not here");
        assert_eq!(summarize_error(0, "timeout"), "timeout");
        assert_eq!(summarize_error(500, r#"{"error":"boom"}"#), "HTTP 500：boom");
    }

    #[test]
    fn success_body_detects_embedded_errors() {
        assert_eq!(
            success_body(r#"{"id":"x","choices":[{"message":{"content":"Hello!"}}]}"#).unwrap(),
            Some("Hello!".to_string())
        );
        assert!(success_body(r#"{"error":{"message":"model not found"}}"#).is_err());
        assert!(success_body("<html>").is_err());
        assert!(success_body(r#"{"foo":1}"#).is_err());
        assert_eq!(success_body(r#"{"id":"x","choices":[{"message":{"content":null}}]}"#).unwrap(), None);
    }
}
