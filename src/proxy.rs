use crate::error::AppError;
use crate::models::{ChannelPick, User};
use crate::state::AppState;
use crate::usage::{
    self, cost_micro, estimate_prompt_tokens, extract_stream_text, extract_usage, new_request_id,
    quota_response_headers, quota_usage, record_usage, UsageRecord,
};
use axum::body::Body;
use axum::extract::{OriginalUri, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use futures_util::StreamExt;
use serde_json::{json, Value};
use sqlx::FromRow;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

#[derive(Debug, Clone, FromRow)]
struct KeyRow {
    id: i64,
    user_id: i64,
    #[allow(dead_code)]
    key_prefix: String,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

struct KeyAuth {
    key: KeyRow,
    user: User,
}

struct AuthFailure {
    status: StatusCode,
    code: &'static str,
    message: String,
    user_id: Option<i64>,
    key_id: Option<i64>,
}

pub async fn handle_models(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let auth = match authenticate_key(&state, &headers).await {
        Ok(auth) => auth,
        Err(err) => {
            return error_response(
                err.status,
                err.code,
                &err.message,
                None,
                None,
                &new_request_id(),
            )
        }
    };

    let available = match usage::available_models(&state.pool, &auth.user.plan).await {
        Ok(models) => models,
        Err(err) => {
            tracing::error!(error = %err, "list models failed");
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "获取模型列表失败",
                None,
                None,
                &new_request_id(),
            );
        }
    };
    let scoped: Vec<String> =
        sqlx::query_scalar("SELECT model FROM api_key_models WHERE key_id = $1 ORDER BY model")
            .bind(auth.key.id)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
    let models: Vec<Value> = available
        .into_iter()
        .filter(|model| scoped.is_empty() || scoped.contains(model))
        .map(|model| {
            json!({
                "id": model,
                "object": "model",
                "created": 0,
                "owned_by": "pm-plan"
            })
        })
        .collect();

    let mut resp = axum::Json(json!({ "object": "list", "data": models })).into_response();
    if let Ok(value) = HeaderValue::from_str(&auth.user.plan) {
        resp.headers_mut().insert("x-pmplan-plan", value);
    }
    resp
}

pub async fn handle_proxy(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    let request_id = new_request_id();
    let client_ip = crate::util::client_ip(&headers);
    let path = uri.path().to_string();
    let query = uri.query().unwrap_or("").to_string();

    let mut rec = UsageRecord::new(&request_id);
    rec.client_ip = client_ip.clone();
    rec.is_stream = false;

    let auth = match authenticate_key(&state, &headers).await {
        Ok(auth) => auth,
        Err(err) => {
            rec.user_id = err.user_id;
            rec.key_id = err.key_id;
            rec.status = err.status.as_u16() as i64;
            rec.error_code = Some(err.code.to_string());
            rec.latency_ms = started.elapsed().as_millis() as i64;
            record_usage(&state.pool, &rec).await;
            return error_response(
                err.status,
                err.code,
                &err.message,
                err.user_id,
                err.key_id,
                &request_id,
            );
        }
    };
    rec.user_id = Some(auth.user.id);
    rec.key_id = Some(auth.key.id);
    let _ = sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
        .bind(auth.key.id)
        .execute(&state.pool)
        .await;

    // 反分发 Step 1：单 Key 24 小时内不同 IP 数统计。>10 写审计告警，
    // >20 返回 429（旧 IP 滑出窗口后自动恢复，不自动吊销 Key）。
    if !client_ip.is_empty() {
        let (decision, alert) = state
            .dist_guard
            .observe_key(&state.pool, auth.key.id, &client_ip)
            .await;
        if let Some(alert) = alert {
            let action = if alert.level == crate::distguard::ALERT_LEVEL_BLOCK {
                "key.distinct_ip_block"
            } else {
                "key.distinct_ip_alert"
            };
            crate::admin::write_audit(
                &state.pool,
                None,
                action,
                "api_key",
                &auth.key.id.to_string(),
                json!({
                    "user_id": auth.user.id,
                    "distinct_ips": alert.distinct_ips,
                    "window_hours": 24,
                    "alert_at": state.dist_guard.alert_at,
                    "block_at": state.dist_guard.block_at,
                }),
            )
            .await;
        }
        if let crate::distguard::DistDecision::Block { distinct_ips } = decision {
            tracing::warn!(
                key_id = auth.key.id,
                user_id = auth.user.id,
                distinct_ips,
                "key blocked: too many distinct IPs in 24h"
            );
            return deny(
                &state,
                rec,
                StatusCode::TOO_MANY_REQUESTS,
                "key_distinct_ip_limit",
                "该 API Key 在 24 小时内已从过多不同 IP 使用，已被临时限流；待旧 IP 滑出窗口后自动恢复，如需处理请联系站长",
                started,
                &request_id,
            )
            .await;
        }
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value @ Value::Object(_)) => value,
        Ok(_) => {
            return deny(
                &state,
                rec,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "请求体必须是 JSON 对象",
                started,
                &request_id,
            )
            .await
        }
        Err(_) => {
            return deny(
                &state,
                rec,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "请求体不是合法的 JSON",
                started,
                &request_id,
            )
            .await
        }
    };

    let model = match payload.get("model").and_then(Value::as_str) {
        Some(model) if !model.trim().is_empty() => model.trim().to_string(),
        _ => {
            return deny(
                &state,
                rec,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "请求体缺少 model 字段",
                started,
                &request_id,
            )
            .await
        }
    };
    rec.model = model.clone();
    let stream = payload
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    rec.is_stream = stream;

    // Step 2: the key's optional model allow-list.
    let (key_scoped, key_allows) = match usage::key_has_model_scope(&state.pool, auth.key.id, &model).await {
        Ok(value) => value,
        Err(err) => {
            tracing::error!(error = %err, "key scope check failed");
            return deny(
                &state,
                rec,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "校验 Key 模型范围失败",
                started,
                &request_id,
            )
            .await;
        }
    };
    if key_scoped && !key_allows {
        return deny(
            &state,
            rec,
            StatusCode::FORBIDDEN,
            "key_model_forbidden",
            "Key 无权调用该模型",
            started,
            &request_id,
        )
        .await;
    }

    // Step 3: the user's plan tier intersected with channel tier authorization.
    let pick = match usage::pick_channel(&state.pool, &auth.user.plan, &model).await {
        Ok(Some(pick)) => pick,
        Ok(None) => {
            let exists = usage::model_has_enabled_channel(&state.pool, &model)
                .await
                .unwrap_or(false);
            if exists {
                return deny(
                    &state,
                    rec,
                    StatusCode::FORBIDDEN,
                    "tier_forbidden",
                    "套餐档位无权调用该渠道模型",
                    started,
                    &request_id,
                )
                .await;
            }
            return deny(
                &state,
                rec,
                StatusCode::NOT_FOUND,
                "model_not_found",
                "模型不存在或未启用",
                started,
                &request_id,
            )
            .await;
        }
        Err(err) => {
            tracing::error!(error = %err, "channel selection failed");
            return deny(
                &state,
                rec,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "选择渠道失败",
                started,
                &request_id,
            )
            .await;
        }
    };
    rec.channel_id = Some(pick.channel_id);

    // Step 4: rolling 5h / 7d / 30d quota.
    let plan_info = crate::plans::plan(&auth.user.plan);
    let used = match quota_usage(&state.pool, auth.user.id).await {
        Ok(used) => used,
        Err(err) => {
            tracing::error!(error = %err, "quota check failed");
            return deny(
                &state,
                rec,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "额度校验失败",
                started,
                &request_id,
            )
            .await;
        }
    };
    if let Some((window, used_micro, limit)) = usage::exceeded_window(plan_info, &used) {
        tracing::info!(
            user_id = auth.user.id,
            window = window.code(),
            used = %crate::util::micro_to_usd(used_micro),
            limit = %crate::util::micro_to_usd(limit),
            "quota exceeded"
        );
        rec.status = StatusCode::TOO_MANY_REQUESTS.as_u16() as i64;
        rec.error_code = Some(window.code().to_string());
        rec.latency_ms = started.elapsed().as_millis() as i64;
        record_usage(&state.pool, &rec).await;
        let mut resp = quota_error_response(window);
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            resp.headers_mut().insert("x-request-id", value);
        }
        return resp;
    }
    let quota_headers = quota_response_headers(&auth.user.plan, &used);

    // Step 4b: 免费模型（零定价）当日 token 上限。定价为 0 的模型不受成本
    // 额度约束，单 Key / 单用户每天各限生效上限（默认 1,000,000,000，
    // 环境变量 DIST_FREE_TOKEN_CAP 可调；管理员可对指定 Key/用户单独设，
    // 见 free_token_cap_overrides 表，Key 单独设置 > 用户单独设置 > 默认值）。
    if pick.input_micro == 0 && pick.output_micro == 0 {
        let estimate = usage::estimate_prompt_tokens(&payload);
        let cap = crate::distguard::effective_free_token_cap(
            &state.pool,
            auth.key.id,
            auth.user.id,
            state.dist_guard.free_token_cap,
        )
        .await;
        match usage::free_tokens_today(&state.pool, auth.key.id, auth.user.id).await {
            Ok((used_key, used_user)) => {
                if state
                    .dist_guard
                    .free_token_exceeded(used_key, used_user, estimate, cap)
                {
                    tracing::info!(
                        key_id = auth.key.id,
                        user_id = auth.user.id,
                        used_key,
                        used_user,
                        cap,
                        "free model daily token cap exceeded"
                    );
                    return deny(
                        &state,
                        rec,
                        StatusCode::TOO_MANY_REQUESTS,
                        "free_token_daily_cap",
                        "免费模型当日用量已达上限，请明天再用或使用付费模型",
                        started,
                        &request_id,
                    )
                    .await;
                }
            }
            Err(err) => {
                // 查询失败时 fail-open：只记日志，不拦截。
                tracing::error!(error = %err, "free token cap check failed");
            }
        }
    }

    // Step 5: forward to the selected upstream channel.
    let upstream_model = pick
        .upstream_model
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| model.clone());
    let mut outgoing = payload.clone();
    if upstream_model != model {
        if let Some(obj) = outgoing.as_object_mut() {
            obj.insert("model".to_string(), Value::String(upstream_model));
        }
    }
    if stream {
        if let Some(obj) = outgoing.as_object_mut() {
            if obj.get("stream_options").is_none() {
                obj.insert(
                    "stream_options".to_string(),
                    json!({ "include_usage": true }),
                );
            }
        }
    }

    // Step 5b: channel-level body rules (strip reasoning params when tools are present,
    // then the channel's param override). `model` / `stream` are never touched.
    let changed = crate::channel_opts::apply_channel_body_rules(
        &mut outgoing,
        pick.strip_reasoning_with_tools,
        &pick.param_override,
    );
    if !changed.is_empty() {
        tracing::debug!(channel_id = pick.channel_id, changed = ?changed, "channel body rules applied");
    }

    let api_key = match state.crypto.decrypt(&pick.api_key_enc) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        Err(err) => {
            tracing::error!(error = %err, channel_id = pick.channel_id, "channel key decrypt failed");
            return deny(
                &state,
                rec,
                StatusCode::BAD_GATEWAY,
                "channel_key_invalid",
                "渠道密钥配置异常",
                started,
                &request_id,
            )
            .await;
        }
    };

    let suffix = path.strip_prefix("/v1").unwrap_or(&path);
    let mut upstream_url = format!("{}{}", pick.base_url.trim_end_matches('/'), suffix);
    if !query.is_empty() {
        upstream_url.push('?');
        upstream_url.push_str(&query);
    }

    let mut request = state
        .http
        .post(&upstream_url)
        .bearer_auth(api_key)
        .header(header::CONTENT_TYPE, "application/json")
        .json(&outgoing);
    if let Some(accept) = headers.get(header::ACCEPT) {
        request = request.header(header::ACCEPT, accept.clone());
    }
    if let Some(beta) = headers.get("openai-beta") {
        request = request.header("openai-beta", beta.clone());
    }
    let extra = crate::channel_opts::header_pairs(&pick.extra_headers);
    if !extra.is_empty() {
        let mut map = HeaderMap::new();
        for (name, value) in extra {
            map.insert(name, value);
        }
        // RequestBuilder::headers replaces same-named headers set above.
        request = request.headers(map);
    }

    let upstream = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            tracing::warn!(error = %err, channel_id = pick.channel_id, "upstream request failed");
            rec.latency_ms = started.elapsed().as_millis() as i64;
            return deny(
                &state,
                rec,
                StatusCode::BAD_GATEWAY,
                "upstream_unreachable",
                "上游渠道请求失败",
                started,
                &request_id,
            )
            .await;
        }
    };

    let status = upstream.status();
    let content_type = upstream.headers().get(header::CONTENT_TYPE).cloned();
    let is_event_stream = content_type
        .as_ref()
        .and_then(|value| value.to_str().ok())
        .map(|value| value.contains("text/event-stream"))
        .unwrap_or(false);

    if !status.is_success() {
        let body = read_limited(upstream, 512 * 1024)
            .await
            .unwrap_or_else(|_| Bytes::new());
        rec.status = status.as_u16() as i64;
        rec.error_code = Some("upstream_error".to_string());
        rec.latency_ms = started.elapsed().as_millis() as i64;
        record_usage(&state.pool, &rec).await;
        return raw_response(
            StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            body,
            content_type,
            &request_id,
            &quota_headers,
        );
    }

    if stream && is_event_stream {
        return stream_response(
            state,
            upstream,
            rec,
            auth.user.plan.clone(),
            pick,
            payload,
            started,
            request_id,
            quota_headers,
            content_type,
        )
        .await;
    }

    let body = match read_limited(upstream, 64 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(message) => {
            return deny(
                &state,
                rec,
                StatusCode::BAD_GATEWAY,
                "upstream_body_error",
                &message,
                started,
                &request_id,
            )
            .await;
        }
    };
    let parsed: Option<Value> = serde_json::from_slice(&body).ok();
    let (prompt_tokens, completion_tokens) = if let Some(found) =
        parsed.as_ref().and_then(extract_usage)
    {
        found
    } else {
        let prompt = estimate_prompt_tokens(&payload);
        let completion = parsed
            .as_ref()
            .map(extract_response_text)
            .map(|text| crate::util::estimate_tokens(&text))
            .unwrap_or(0);
        (prompt, completion)
    };
    let cost = cost_micro(
        prompt_tokens,
        completion_tokens,
        pick.input_micro,
        pick.output_micro,
    );
    rec.prompt_tokens = prompt_tokens;
    rec.completion_tokens = completion_tokens;
    rec.cost_micro = cost;
    rec.status = status.as_u16() as i64;
    rec.latency_ms = started.elapsed().as_millis() as i64;
    record_usage(&state.pool, &rec).await;

    raw_response(
        StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
        body,
        content_type,
        &request_id,
        &quota_headers,
    )
}

fn extract_response_text(value: &Value) -> String {
    let mut out = String::new();
    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                if let Some(content) = message.get("content") {
                    usage::collect_text(content, &mut out);
                }
                if let Some(reasoning) = message.get("reasoning_content") {
                    usage::collect_text(reasoning, &mut out);
                }
            }
            if let Some(text) = choice.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }
    if let Some(output) = value.get("output") {
        usage::collect_text(output, &mut out);
    }
    out
}

#[allow(clippy::too_many_arguments)]
async fn stream_response(
    state: AppState,
    upstream: reqwest::Response,
    mut rec: UsageRecord,
    plan: String,
    pick: ChannelPick,
    payload: Value,
    started: Instant,
    request_id: String,
    quota_headers: Vec<(&'static str, String)>,
    content_type: Option<HeaderValue>,
) -> Response {
    let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(32);
    let record_user_id = rec.user_id;
    let record_key_id = rec.key_id;
    let pool = state.pool.clone();
    let input_price = pick.input_micro;
    let output_price = pick.output_micro;
    let prompt_estimate = estimate_prompt_tokens(&payload);

    tokio::spawn(async move {
        let mut stream = upstream.bytes_stream();
        let mut buffer: Vec<u8> = Vec::new();
        let mut usage: Option<(i64, i64)> = None;
        let mut text = String::new();
        let mut aborted = false;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);
                    process_sse_buffer(&mut buffer, &mut usage, &mut text);
                    if tx.send(Ok(bytes)).await.is_err() {
                        aborted = true;
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "upstream stream error");
                    let _ = tx
                        .send(Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "upstream stream error",
                        )))
                        .await;
                    break;
                }
            }
        }
        if !buffer.is_empty() {
            let line = String::from_utf8_lossy(&buffer).to_string();
            process_sse_line(&line, &mut usage, &mut text);
        }
        let (prompt_tokens, completion_tokens) = usage.unwrap_or_else(|| {
            (
                prompt_estimate,
                crate::util::estimate_tokens(&text).max(1),
            )
        });
        rec.prompt_tokens = prompt_tokens;
        rec.completion_tokens = completion_tokens;
        rec.cost_micro = cost_micro(prompt_tokens, completion_tokens, input_price, output_price);
        rec.status = 200;
        rec.latency_ms = started.elapsed().as_millis() as i64;
        if aborted {
            rec.error_code = Some("client_disconnected".to_string());
        }
        record_usage(&pool, &rec).await;
    });

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            content_type.unwrap_or_else(|| HeaderValue::from_static("text/event-stream")),
        )
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-accel-buffering", "no");
    builder = builder.header("x-request-id", request_id);
    builder = builder.header("x-pmplan-plan", plan);
    for (name, value) in quota_headers {
        builder = builder.header(name, value);
    }
    builder
        .body(Body::from_stream(ReceiverStream::new(rx)))
        .unwrap_or_else(|_| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "构建流式响应失败",
                record_user_id,
                record_key_id,
                &new_request_id(),
            )
        })
}

fn process_sse_buffer(
    buffer: &mut Vec<u8>,
    usage: &mut Option<(i64, i64)>,
    text: &mut String,
) {
    let mut start = 0usize;
    while let Some(pos) = buffer[start..].iter().position(|byte| *byte == b'\n') {
        let end = start + pos;
        let line = String::from_utf8_lossy(&buffer[start..end]).to_string();
        process_sse_line(&line, usage, text);
        start = end + 1;
    }
    if start > 0 {
        buffer.drain(0..start);
    }
}

fn process_sse_line(line: &str, usage: &mut Option<(i64, i64)>, text: &mut String) {
    let line = line.trim();
    let Some(data) = line.strip_prefix("data:") else {
        return;
    };
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return;
    };
    if let Some(found) = extract_usage(&value) {
        *usage = Some(found);
    }
    extract_stream_text(&value, text);
}

async fn read_limited(response: reqwest::Response, limit: usize) -> Result<Bytes, String> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| format!("读取上游响应失败：{err}"))?;
        if buffer.len() + chunk.len() > limit {
            return Err("上游响应体过大".to_string());
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(buffer))
}

fn raw_response(
    status: StatusCode,
    body: Bytes,
    content_type: Option<HeaderValue>,
    request_id: &str,
    quota_headers: &[(&'static str, String)],
) -> Response {
    let mut builder = Response::builder().status(status);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    } else {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    builder = builder.header("x-request-id", request_id);
    for (name, value) in quota_headers {
        builder = builder.header(*name, value.as_str());
    }
    builder
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn deny(
    state: &AppState,
    mut rec: UsageRecord,
    status: StatusCode,
    code: &'static str,
    message: &str,
    started: Instant,
    request_id: &str,
) -> Response {
    rec.status = status.as_u16() as i64;
    rec.error_code = Some(code.to_string());
    rec.latency_ms = started.elapsed().as_millis() as i64;
    record_usage(&state.pool, &rec).await;
    error_response(status, code, message, rec.user_id, rec.key_id, request_id)
}

fn error_response(
    status: StatusCode,
    code: &'static str,
    message: &str,
    _user_id: Option<i64>,
    _key_id: Option<i64>,
    request_id: &str,
) -> Response {
    let mut response = AppError::new(status, code, message).into_response();
    if let Ok(value) = HeaderValue::from_str(request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

/// 额度超限 429：message 与 type 都是中文提示（部分客户端只显示 type），code 保留 quota_5h / quota_7d / quota_30d。
fn quota_error_response(window: usage::QuotaWindow) -> Response {
    let text = window.user_message();
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(json!({
            "error": {
                "message": text,
                "type": text,
                "code": window.code(),
            }
        })),
    )
        .into_response()
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let trimmed = value.trim();
        if let Some(rest) = trimmed.strip_prefix("Bearer ").or_else(|| trimmed.strip_prefix("bearer ")) {
            if !rest.trim().is_empty() {
                return Some(rest.trim().to_string());
            }
        }
    }
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn authenticate_key(state: &AppState, headers: &HeaderMap) -> Result<KeyAuth, AuthFailure> {
    let token = bearer_token(headers).ok_or_else(|| AuthFailure {
        status: StatusCode::UNAUTHORIZED,
        code: "invalid_api_key",
        message: "缺少 API Key，请在 Authorization 头中使用 Bearer".to_string(),
        user_id: None,
        key_id: None,
    })?;
    let hash = crate::util::sha256(token.as_bytes());
    let key = sqlx::query_as::<_, KeyRow>(
        "SELECT id, user_id, key_prefix, expires_at, revoked_at FROM api_keys WHERE key_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| {
        tracing::error!(error = %err, "key lookup failed");
        AuthFailure {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "校验 API Key 失败".to_string(),
            user_id: None,
            key_id: None,
            }
    })?
    .ok_or_else(|| AuthFailure {
        status: StatusCode::UNAUTHORIZED,
        code: "invalid_api_key",
        message: "无效的 API Key".to_string(),
        user_id: None,
        key_id: None,
    })?;

    if key.revoked_at.is_some() {
        return Err(AuthFailure {
            status: StatusCode::UNAUTHORIZED,
            code: "key_revoked",
            message: "API Key 已撤销".to_string(),
            user_id: Some(key.user_id),
            key_id: Some(key.id),
        });
    }
    if key
        .expires_at
        .map(|expires| expires <= chrono::Utc::now())
        .unwrap_or(false)
    {
        return Err(AuthFailure {
            status: StatusCode::UNAUTHORIZED,
            code: "key_expired",
            message: "API Key 已过期".to_string(),
            user_id: Some(key.user_id),
            key_id: Some(key.id),
        });
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(key.user_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "user lookup failed");
            AuthFailure {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: "读取用户失败".to_string(),
                user_id: Some(key.user_id),
                key_id: Some(key.id),
                }
        })?
        .ok_or_else(|| AuthFailure {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_api_key",
            message: "API Key 对应账号不存在".to_string(),
            user_id: Some(key.user_id),
            key_id: Some(key.id),
        })?;

    if user.is_banned {
        return Err(AuthFailure {
            status: StatusCode::FORBIDDEN,
            code: "user_banned",
            message: user
                .ban_reason
                .clone()
                .unwrap_or_else(|| "账号已被封禁".to_string()),
            user_id: Some(user.id),
            key_id: Some(key.id),
        });
    }

    Ok(KeyAuth { key, user })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_usage_and_text_are_parsed() {
        let mut usage = None;
        let mut text = String::new();
        process_sse_line(
            r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#,
            &mut usage,
            &mut text,
        );
        process_sse_line(
            r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":7}}"#,
            &mut usage,
            &mut text,
        );
        process_sse_line("data: [DONE]", &mut usage, &mut text);
        assert_eq!(usage, Some((12, 7)));
        assert_eq!(text, "hello\n");
    }

    #[test]
    fn quota_errors_are_chinese_with_machine_code() {
        use crate::usage::QuotaWindow;
        let cases = [
            (QuotaWindow::H5, "quota_5h", "已达到5小时额度限制，请5小时后继续或通过签到升级套餐"),
            (QuotaWindow::Week, "quota_7d", "已达到周期限制，请一周后继续或签到升级套餐"),
            (QuotaWindow::Month, "quota_30d", "已达到月度额度限制，请等待额度恢复或通过签到升级套餐"),
        ];
        for (window, code, text) in cases {
            assert_eq!(window.code(), code);
            assert_eq!(window.user_message(), text);
            let resp = quota_error_response(window);
            assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        }
    }

    #[test]
    fn sse_split_lines_are_buffered() {
        let mut buffer = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n".to_vec();
        let mut usage = None;
        let mut text = String::new();
        process_sse_buffer(&mut buffer, &mut usage, &mut text);
        assert!(buffer.is_empty());
    }
}
