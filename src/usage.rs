use crate::models::ChannelPick;
use crate::plans::{plan, PlanInfo};
use crate::util::micro_to_usd;
use rand::Rng;
use serde_json::Value;
use sqlx::PgPool;

#[derive(Debug, Default, Clone, Copy)]
pub struct QuotaUsage {
    pub h5: i64,
    pub d7: i64,
    pub d30: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaWindow {
    H5,
    Week,
    Month,
}

impl QuotaWindow {
    #[allow(dead_code)]
    pub fn label(&self) -> &'static str {
        match self {
            QuotaWindow::H5 => "5 小时",
            QuotaWindow::Week => "7 天",
            QuotaWindow::Month => "30 天",
        }
    }

    /// 返回给 API 客户端的中文提示（error.message 与 error.type 都用它，code 保留机器码）。
    pub fn user_message(&self) -> &'static str {
        match self {
            QuotaWindow::H5 => "已达到5小时额度限制，请5小时后继续或通过签到升级套餐",
            QuotaWindow::Week => "已达到周期限制，请一周后继续或签到升级套餐",
            QuotaWindow::Month => "已达到月度额度限制，请等待额度恢复或通过签到升级套餐",
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            QuotaWindow::H5 => "quota_5h",
            QuotaWindow::Week => "quota_7d",
            QuotaWindow::Month => "quota_30d",
        }
    }
}

pub async fn quota_usage(pool: &PgPool, user_id: i64) -> Result<QuotaUsage, sqlx::Error> {
    // A used reset card zeroes all three windows: only usage after the latest
    // reset counts (usage_resets.created_at is when the reset was applied).
    let row: (i64, i64, i64) = sqlx::query_as(
        "SELECT \
           COALESCE(SUM(cost_micro) FILTER (WHERE created_at >= now() - interval '5 hours'), 0)::bigint, \
           COALESCE(SUM(cost_micro) FILTER (WHERE created_at >= now() - interval '7 days'), 0)::bigint, \
           COALESCE(SUM(cost_micro) FILTER (WHERE created_at >= now() - interval '30 days'), 0)::bigint \
         FROM usage_logs WHERE user_id = $1 AND created_at >= now() - interval '30 days' \
         AND created_at > COALESCE((SELECT MAX(created_at) FROM usage_resets WHERE user_id = $1), '-infinity'::timestamptz)",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(QuotaUsage {
        h5: row.0,
        d7: row.1,
        d30: row.2,
    })
}

pub fn exceeded_window(plan_info: &PlanInfo, usage: &QuotaUsage) -> Option<(QuotaWindow, i64, i64)> {
    if let Some(limit) = plan_info.limits.h5 {
        if usage.h5 >= limit {
            return Some((QuotaWindow::H5, usage.h5, limit));
        }
    }
    if let Some(limit) = plan_info.limits.weekly {
        if usage.d7 >= limit {
            return Some((QuotaWindow::Week, usage.d7, limit));
        }
    }
    if let Some(limit) = plan_info.limits.monthly {
        if usage.d30 >= limit {
            return Some((QuotaWindow::Month, usage.d30, limit));
        }
    }
    None
}

pub fn quota_response_headers(plan_code: &str, usage: &QuotaUsage) -> Vec<(&'static str, String)> {
    let info = plan(plan_code);
    let mut headers = vec![("x-pmplan-plan", plan_code.to_string())];
    if let Some(limit) = info.limits.h5 {
        headers.push(("x-pmplan-5h-limit", micro_to_usd(limit)));
        headers.push((
            "x-pmplan-5h-remaining",
            micro_to_usd((limit - usage.h5).max(0)),
        ));
    }
    if let Some(limit) = info.limits.weekly {
        headers.push(("x-pmplan-7d-limit", micro_to_usd(limit)));
        headers.push((
            "x-pmplan-7d-remaining",
            micro_to_usd((limit - usage.d7).max(0)),
        ));
    }
    if let Some(limit) = info.limits.monthly {
        headers.push(("x-pmplan-30d-limit", micro_to_usd(limit)));
        headers.push((
            "x-pmplan-30d-remaining",
            micro_to_usd((limit - usage.d30).max(0)),
        ));
    }
    headers
}

pub async fn available_models(pool: &PgPool, tier: &str) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT DISTINCT cm.model FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         WHERE cm.enabled AND c.enabled AND (\
           CASE \
             WHEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id) \
               THEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id AND t.tier = $1) \
             WHEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id) \
               THEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id AND ct.tier = $1) \
             ELSE TRUE \
           END) \
         ORDER BY cm.model",
    )
    .bind(tier)
    .fetch_all(pool)
    .await
}

pub async fn pick_channel(
    pool: &PgPool,
    tier: &str,
    model: &str,
) -> Result<Option<ChannelPick>, sqlx::Error> {
    let rows = sqlx::query_as::<_, ChannelPick>(
        "SELECT c.id AS channel_id, c.base_url, c.api_key_enc, \
                c.priority, c.weight, c.health, cm.upstream_model, \
                c.extra_headers, c.strip_reasoning_with_tools, c.param_override, \
                COALESCE(cm.input_micro, mp.input_micro, 1000000)::bigint AS input_micro, \
                COALESCE(cm.output_micro, mp.output_micro, 3000000)::bigint AS output_micro \
         FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         LEFT JOIN model_prices mp ON mp.model = cm.model \
         WHERE cm.model = $1 AND cm.enabled AND c.enabled AND (\
           CASE \
             WHEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id) \
               THEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id AND t.tier = $2) \
             WHEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id) \
               THEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id AND ct.tier = $2) \
             ELSE TRUE \
           END) \
         ORDER BY c.priority DESC, c.id ASC",
    )
    .bind(model)
    .bind(tier)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(None);
    }
    let top_priority = rows.iter().map(|r| r.priority).max().unwrap_or(0);
    let mut candidates: Vec<ChannelPick> = rows
        .into_iter()
        .filter(|r| r.priority == top_priority)
        .collect();

    let health_rank = |h: &str| match h {
        "healthy" => 2,
        "unknown" => 1,
        _ => 0,
    };
    let best_health = candidates
        .iter()
        .map(|r| health_rank(&r.health))
        .max()
        .unwrap_or(0);
    candidates.retain(|r| health_rank(&r.health) == best_health);

    let total: i64 = candidates.iter().map(|r| r.weight.max(1) as i64).sum();
    if total <= 1 {
        return Ok(candidates.into_iter().next());
    }
    let mut roll = rand::thread_rng().gen_range(0..total);
    for candidate in candidates {
        roll -= candidate.weight.max(1) as i64;
        if roll < 0 {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

pub async fn model_has_enabled_channel(
    pool: &PgPool,
    model: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM channel_models cm JOIN channels c ON c.id = cm.channel_id \
         WHERE cm.model = $1 AND cm.enabled AND c.enabled)",
    )
    .bind(model)
    .fetch_one(pool)
    .await
}

pub async fn key_has_model_scope(
    pool: &PgPool,
    key_id: i64,
    model: &str,
) -> Result<(bool, bool), sqlx::Error> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM api_key_models WHERE key_id = $1")
        .bind(key_id)
        .fetch_one(pool)
        .await?;
    if count == 0 {
        return Ok((false, true));
    }
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM api_key_models WHERE key_id = $1 AND model = $2)",
    )
    .bind(key_id)
    .bind(model)
    .fetch_one(pool)
    .await?;
    Ok((true, allowed))
}

pub struct UsageRecord {
    pub request_id: String,
    pub user_id: Option<i64>,
    pub key_id: Option<i64>,
    pub channel_id: Option<i64>,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_micro: i64,
    pub status: i64,
    pub error_code: Option<String>,
    pub latency_ms: i64,
    pub is_stream: bool,
    pub client_ip: String,
}

impl UsageRecord {
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            user_id: None,
            key_id: None,
            channel_id: None,
            model: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            cost_micro: 0,
            status: 200,
            error_code: None,
            latency_ms: 0,
            is_stream: false,
            client_ip: String::new(),
        }
    }
}

pub async fn record_usage(pool: &PgPool, record: &UsageRecord) {
    let total = record.prompt_tokens + record.completion_tokens;
    let result = sqlx::query(
        "INSERT INTO usage_logs (request_id, user_id, key_id, channel_id, model, \
         prompt_tokens, completion_tokens, total_tokens, cost_micro, status, error_code, \
         latency_ms, is_stream, client_ip) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(&record.request_id)
    .bind(record.user_id)
    .bind(record.key_id)
    .bind(record.channel_id)
    .bind(&record.model)
    .bind(record.prompt_tokens as i32)
    .bind(record.completion_tokens as i32)
    .bind(total as i32)
    .bind(record.cost_micro)
    .bind(record.status as i16)
    .bind(&record.error_code)
    .bind(record.latency_ms as i32)
    .bind(record.is_stream)
    .bind(&record.client_ip)
    .execute(pool)
    .await;
    if let Err(err) = result {
        tracing::error!(error = %err, "failed to record usage");
    }
}

pub fn cost_micro(prompt_tokens: i64, completion_tokens: i64, input_micro: i64, output_micro: i64) -> i64 {
    let input = (prompt_tokens.max(0) as i128 * input_micro.max(0) as i128) / 1_000_000;
    let output = (completion_tokens.max(0) as i128 * output_micro.max(0) as i128) / 1_000_000;
    let total = input + output;
    if total > 0 {
        total.min(i64::MAX as i128) as i64
    } else {
        0
    }
}

pub fn extract_usage(value: &Value) -> Option<(i64, i64)> {
    let usage = value.get("usage")?;
    if usage.is_null() {
        return None;
    }
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("input_tokens").and_then(Value::as_i64))
        .unwrap_or(0);
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_i64)
        .or_else(|| usage.get("output_tokens").and_then(Value::as_i64))
        .unwrap_or(0);
    if prompt == 0 && completion == 0 {
        None
    } else {
        Some((prompt, completion))
    }
}

pub fn estimate_prompt_tokens(body: &Value) -> i64 {
    let mut text = String::new();
    if let Some(messages) = body.get("messages").and_then(Value::as_array) {
        for message in messages {
            if let Some(content) = message.get("content") {
                collect_text(content, &mut text);
            }
            if let Some(tools) = message.get("tool_calls") {
                collect_text(tools, &mut text);
            }
        }
    }
    if let Some(input) = body.get("input") {
        collect_text(input, &mut text);
    }
    if let Some(prompt) = body.get("prompt") {
        collect_text(prompt, &mut text);
    }
    if let Some(system) = body.get("system") {
        collect_text(system, &mut text);
    }
    (crate::util::estimate_tokens(&text) + 4).max(4)
}

pub fn collect_text(value: &Value, out: &mut String) {
    match value {
        Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        Value::Array(items) => {
            for item in items {
                collect_text(item, out);
            }
        }
        Value::Object(map) => {
            for key in ["text", "content", "input_text", "output_text"] {
                if let Some(v) = map.get(key) {
                    collect_text(v, out);
                }
            }
        }
        _ => {}
    }
}

pub fn extract_stream_text(value: &Value, out: &mut String) {
    if let Some(choices) = value.get("choices").and_then(Value::as_array) {
        for choice in choices {
            if let Some(delta) = choice.get("delta") {
                if let Some(content) = delta.get("content") {
                    collect_text(content, out);
                }
                if let Some(reasoning) = delta.get("reasoning_content") {
                    collect_text(reasoning, out);
                }
            }
            if let Some(text) = choice.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }
    }
    if let Some(output) = value.get("output") {
        collect_text(output, out);
    }
}

pub fn new_request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plans::plan;
    use serde_json::json;

    #[test]
    fn cost_is_computed_per_million_tokens() {
        assert_eq!(cost_micro(1_200, 800, 1_000_000, 3_000_000), 3_600);
        assert_eq!(cost_micro(1_000_000, 0, 2_500_000, 10_000_000), 2_500_000);
        assert_eq!(cost_micro(0, 1_000_000, 2_500_000, 10_000_000), 10_000_000);
        assert_eq!(cost_micro(0, 0, 1_000_000, 1_000_000), 0);
    }

    #[test]
    fn usage_parser_supports_openai_and_responses_shapes() {
        let openai = json!({"usage": {"prompt_tokens": 10, "completion_tokens": 20}});
        assert_eq!(extract_usage(&openai), Some((10, 20)));
        let responses = json!({"usage": {"input_tokens": 5, "output_tokens": 7}});
        assert_eq!(extract_usage(&responses), Some((5, 7)));
        assert_eq!(extract_usage(&json!({"usage": null})), None);
    }

    #[test]
    fn quota_windows_are_checked_in_order() {
        let lite = plan("lite");
        let usage = QuotaUsage { h5: 1_000_000, d7: 0, d30: 0 };
        assert_eq!(
            exceeded_window(lite, &usage),
            Some((QuotaWindow::H5, 1_000_000, 1_000_000))
        );
        let usage = QuotaUsage { h5: 0, d7: 2_000_000, d30: 0 };
        assert_eq!(
            exceeded_window(lite, &usage),
            Some((QuotaWindow::Week, 2_000_000, 2_000_000))
        );
        let max = plan("max");
        let usage = QuotaUsage { h5: 100_000_000, d7: i64::MAX, d30: i64::MAX };
        assert_eq!(
            exceeded_window(max, &usage),
            Some((QuotaWindow::H5, 100_000_000, 100_000_000))
        );
    }

    #[test]
    fn prompt_estimate_counts_messages() {
        let body = json!({"messages": [{"role": "user", "content": "你好，世界"}]});
        assert!(estimate_prompt_tokens(&body) >= 4);
    }
}
