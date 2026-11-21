use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: i64,
    pub linuxdo_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub trust_level: i32,
    pub role: String,
    pub plan: String,
    pub earned_plan: String,
    pub plan_source: String,
    pub is_banned: bool,
    pub ban_reason: Option<String>,
    pub consecutive_days: i32,
    pub longest_streak: i32,
    pub last_checkin_on: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == "root" || self.role == "admin"
    }

    pub fn is_root(&self) -> bool {
        self.role == "root"
    }

}

#[derive(Debug, Clone, FromRow)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    pub base_url: String,
    pub api_key_enc: Vec<u8>,
    pub key_hint: String,
    pub enabled: bool,
    pub priority: i32,
    pub weight: i32,
    pub health: String,
    pub last_health_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub extra_headers: serde_json::Value,
    pub strip_reasoning_with_tools: bool,
    pub param_override: serde_json::Value,
    pub test_model: Option<String>,
    pub test_endpoint: String,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_ok: Option<bool>,
    pub last_test_ms: Option<i32>,
    pub last_test_model: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ChannelModel {
    pub id: i64,
    pub channel_id: i64,
    pub model: String,
    pub upstream_model: Option<String>,
    pub enabled: bool,
    pub input_micro: Option<i64>,
    pub output_micro: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_ok: Option<bool>,
    pub last_test_ms: Option<i32>,
    pub last_test_error: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PlanApplication {
    pub id: i64,
    pub user_id: i64,
    pub target_plan: String,
    pub status: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<i64>,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UsageLog {
    pub id: i64,
    pub request_id: String,
    pub model: String,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cost_micro: i64,
    pub status: i16,
    pub error_code: Option<String>,
    pub latency_ms: i32,
    pub is_stream: bool,
    pub created_at: DateTime<Utc>,
    pub channel_name: Option<String>,
    pub key_prefix: Option<String>,
    pub user_name: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct ChannelPick {
    pub channel_id: i64,
    pub base_url: String,
    pub api_key_enc: Vec<u8>,
    pub priority: i32,
    pub weight: i32,
    pub health: String,
    pub upstream_model: Option<String>,
    pub input_micro: i64,
    pub output_micro: i64,
    pub extra_headers: serde_json::Value,
    pub strip_reasoning_with_tools: bool,
    pub param_override: serde_json::Value,
}
