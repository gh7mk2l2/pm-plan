use crate::auth::AuthUser;
use crate::error::{AppError, AppJson, AppResult};
use crate::models::{PlanApplication, UsageLog, User};
use crate::plans::{self, PlanView};
use crate::state::AppState;
use crate::usage;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::{Json, Router};
use axum::routing::{delete, get, post};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bootstrap", get(bootstrap))
        .route("/health", get(health))
        .route("/plans", get(plans_handler))
        .route("/me", get(me))
        .route("/checkin", post(checkin))
        .route("/checkins", get(checkin_history))
        .route("/cards", get(crate::cards::list_cards))
        .route("/makeup-cards/{id}/use", post(crate::cards::use_makeup_card))
        .route("/reset-cards/redeem", post(crate::cards::redeem_reset_code))
        .route("/reset-cards/{id}/use", post(crate::cards::use_reset_code))
        .route("/keys", get(list_keys).post(create_key))
        .route("/keys/{id}", delete(delete_key))
        .route("/keys/{id}/models", axum::routing::put(update_key_models))
        .route("/usage", get(usage_logs))
        .route("/models", get(models_handler))
        .route("/model-square", get(model_square))
        .route("/model-tiers", get(model_tiers_handler))
        .route("/captcha/provider", get(crate::captcha::provider))
        .route("/v1/zcode/contribute", get(crate::zcode::contribute))
        .route("/notices", get(crate::notices::list_notices))
        .route("/notices/{id}/read", post(crate::notices::mark_read))
}

#[derive(Serialize)]
struct AppInfo {
    name: &'static str,
    description: &'static str,
    icon_url: &'static str,
    homepage: &'static str,
    callback_url: String,
    level: i32,
}

#[derive(Serialize)]
struct BootstrapResponse {
    app: AppInfo,
    registration_open: bool,
    has_users: bool,
    plans: Vec<PlanView>,
    oauth_login_url: &'static str,
}

async fn bootstrap(State(state): State<AppState>) -> AppResult<Json<BootstrapResponse>> {
    let registration_open: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::boolean FROM settings WHERE key = 'registration_open'), TRUE)",
    )
    .fetch_one(&state.pool)
    .await?;
    let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&state.pool)
        .await?;
    let plans = plans::load_all_plans(&state.pool).await?;
    Ok(Json(BootstrapResponse {
        app: AppInfo {
            name: "PM-Plan",
            description: "胖猫也开始卖 Token Plan 了吗",
            icon_url: "/static/logo.jpg",
            homepage: "https://plan.pmcat.top",
            callback_url: state.cfg.redirect_uri(),
            level: 0,
        },
        registration_open,
        has_users: user_count > 0,
        plans,
        oauth_login_url: "/v1/oauth/login",
    }))
}

async fn health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(json!({
        "ok": true,
        "name": "PM-Plan",
        "version": env!("CARGO_PKG_VERSION"),
        "time": Utc::now(),
    })))
}

async fn plans_handler(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let plans = plans::load_all_plans(&state.pool).await?;
    Ok(Json(json!({ "plans": plans })))
}

#[derive(Serialize)]
struct MeUser {
    id: i64,
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    trust_level: i32,
    role: String,
    is_root: bool,
    plan: String,
    earned_plan: String,
    plan_source: String,
    streak: i32,
    longest_streak: i32,
    last_checkin_on: Option<NaiveDate>,
    created_at: DateTime<Utc>,
    last_login_at: Option<DateTime<Utc>>,
    key_count: i64,
}

#[derive(Serialize)]
struct UsageView {
    h5_used: i64,
    h5_limit: Option<i64>,
    weekly_used: i64,
    weekly_limit: Option<i64>,
    monthly_used: i64,
    monthly_limit: Option<i64>,
    requests_30d: i64,
    tokens_30d: i64,
}

#[derive(Serialize, FromRow)]
struct DailyUsage {
    day: String,
    requests: i64,
    cost_micro: i64,
}

#[derive(Serialize)]
struct NextAuto {
    plan: &'static str,
    /// true = 累计签到天数（不必连续），false = 连续签到天数
    cumulative: bool,
    name: &'static str,
    days_required: i32,
    days_left: i32,
    current_streak: i32,
}

#[derive(Serialize)]
struct CheckinView {
    checked_in_today: bool,
    can_checkin: bool,
    streak: i32,
    longest_streak: i32,
    last_checkin_on: Option<NaiveDate>,
    today: NaiveDate,
    recent: Vec<NaiveDate>,
    next_auto: Option<NextAuto>,
}

#[derive(Serialize)]
struct MeResponse {
    user: MeUser,
    plan: PlanView,
    usage: UsageView,
    checkin: CheckinView,
    pending_applications: Vec<PlanApplication>,
    makeup_cards: Vec<crate::cards::MakeupCardView>,
    reset_cards: Vec<crate::cards::ResetCodeView>,
    available_models: Vec<String>,
    daily_usage: Vec<DailyUsage>,
    recent_usage: Vec<UsageLog>,
}

async fn me(State(state): State<AppState>, AuthUser(user): AuthUser) -> AppResult<Json<MeResponse>> {
    let plan_info = plans::load_plan(&state.pool, &user.plan).await?;
    let quota = usage::quota_usage(&state.pool, user.id).await?;
    let available = usage::available_models(&state.pool, &user.plan).await?;
    let pending: Vec<PlanApplication> = sqlx::query_as(
        "SELECT * FROM plan_applications WHERE user_id = $1 AND status = 'pending' ORDER BY created_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let (makeup_cards, reset_cards) = crate::cards::user_cards(&state.pool, user.id).await?;
    let key_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM api_keys WHERE user_id = $1 AND revoked_at IS NULL \
         AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    let (requests_30d, tokens_30d): (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint, COALESCE(sum(total_tokens), 0)::bigint FROM usage_logs \
         WHERE user_id = $1 AND created_at >= now() - interval '30 days'",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    let today = state.cfg.today();
    let yesterday = today.pred_opt().unwrap_or(today);
    let streak_active = matches!(user.last_checkin_on, Some(day) if day == today || day == yesterday);
    let display_streak = if streak_active { user.consecutive_days } else { 0 };
    let recent_checkins: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT checkin_on FROM checkins WHERE user_id = $1 AND checkin_on >= $2 - 60 \
         ORDER BY checkin_on DESC",
    )
    .bind(user.id)
    .bind(today)
    .fetch_all(&state.pool)
    .await?;
    let recent_usage: Vec<UsageLog> = sqlx::query_as(
        "SELECT l.id, l.request_id, l.model, l.prompt_tokens, l.completion_tokens, \
                l.total_tokens, l.cost_micro, l.status, l.error_code, l.latency_ms, \
                l.is_stream, l.created_at, c.name AS channel_name, k.key_prefix, \
                NULL::text AS user_name \
         FROM usage_logs l \
         LEFT JOIN channels c ON c.id = l.channel_id \
         LEFT JOIN api_keys k ON k.id = l.key_id \
         WHERE l.user_id = $1 ORDER BY l.id DESC LIMIT 10",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let daily_usage: Vec<DailyUsage> = sqlx::query_as(
        "SELECT to_char(d, 'MM-DD') AS day, COUNT(l.id)::bigint AS requests, \
                COALESCE(SUM(l.cost_micro), 0)::bigint AS cost_micro \
         FROM generate_series($1::date - 6, $1::date, interval '1 day') d \
         LEFT JOIN usage_logs l ON l.user_id = $2 \
           AND (l.created_at AT TIME ZONE 'Asia/Shanghai')::date = d \
         GROUP BY d ORDER BY d",
    )
    .bind(today)
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let total_checkins: i64 = if user.earned_plan == "go" {
        sqlx::query_scalar("SELECT count(*) FROM checkins WHERE user_id = $1")
            .bind(user.id)
            .fetch_one(&state.pool)
            .await?
    } else {
        0
    };
    let next_auto = plans::next_auto_milestone(&user.earned_plan).map(|(code, days)| {
        let info = plans::plan(code);
        let progress = if user.earned_plan == "go" { total_checkins as i32 } else { display_streak };
        let days_left = (days - progress).max(0);
        NextAuto {
            plan: code,
            cumulative: user.earned_plan == "go",
            name: info.name,
            days_required: days,
            days_left,
            current_streak: display_streak,
        }
    });

    let checkin = CheckinView {
        checked_in_today: user.last_checkin_on == Some(today),
        can_checkin: user.last_checkin_on != Some(today),
        streak: display_streak,
        longest_streak: user.longest_streak,
        last_checkin_on: user.last_checkin_on,
        today,
        recent: recent_checkins,
        next_auto,
    };

    let user_view = MeUser {
        id: user.id,
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        avatar_url: user.avatar_url.clone(),
        trust_level: user.trust_level,
        role: user.role.clone(),
        is_root: user.is_root(),
        plan: user.plan.clone(),
        earned_plan: user.earned_plan.clone(),
        plan_source: user.plan_source.clone(),
        streak: display_streak,
        longest_streak: user.longest_streak,
        last_checkin_on: user.last_checkin_on,
        created_at: user.created_at,
        last_login_at: user.last_login_at,
        key_count,
    };

    Ok(Json(MeResponse {
        user: user_view,
        plan: plan_info,
        usage: UsageView {
            h5_used: quota.h5,
            h5_limit: plan_info.limits.h5,
            weekly_used: quota.d7,
            weekly_limit: plan_info.limits.weekly,
            monthly_used: quota.d30,
            monthly_limit: plan_info.limits.monthly,
            requests_30d,
            tokens_30d,
        },
        checkin,
        pending_applications: pending,
        makeup_cards,
        reset_cards,
        available_models: available,
        daily_usage,
        recent_usage,
    }))
}

#[derive(Serialize)]
struct CheckinResult {
    checked_in: bool,
    date: NaiveDate,
    streak: i32,
    plan: String,
    earned_plan: String,
    upgraded_to: Option<String>,
    candidate_created: Option<String>,
    message: String,
}

#[derive(Deserialize, Default)]
struct CheckinBody {
    /// 滑块验证通过后发的一次性 pass_token。
    #[serde(default)]
    captcha_token: Option<String>,
}

async fn checkin(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    headers: HeaderMap,
    AppJson(body): AppJson<CheckinBody>,
) -> AppResult<Json<CheckinResult>> {
    let remote_ip = crate::util::client_ip(&headers);
    // 人机验证（2026-11-15 换回 hCaptcha + Turnstile 双通道）：按当前通道校验 token。
    crate::captcha::check_checkin_token(&state, body.captcha_token.as_deref(), None, Some(&remote_ip)).await?;
    let today = state.cfg.today();
    let mut tx = state.pool.begin().await?;
    let current: User = sqlx::query_as("SELECT * FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;

    if current.last_checkin_on == Some(today) {
        return Err(AppError::conflict("今天已经签到过了，明天再来吧"));
    }

    // Shared check-in core (also used by makeup cards): inserts the day,
    // recomputes streak/total from the checkins table, applies auto-earned
    // plans and Max-candidate applications.
    let outcome = crate::cards::record_checkin(&mut tx, &current, today, "checkin").await?;
    let new_streak = outcome.streak;
    let upgraded_to = outcome.upgraded_to.clone();
    let candidate_created = outcome.candidate_created.clone();

    tx.commit().await?;

    let message = if let Some(code) = &upgraded_to {
        format!("签到成功，已自动升级到 {} 套餐", plans::plan(code).name)
    } else if let Some(code) = &candidate_created {
        format!(
            "签到成功，已进入 {} 候补池，等待站长审批",
            plans::plan(code).name
        )
    } else {
        format!("签到成功，已连续签到 {} 天", new_streak)
    };

    Ok(Json(CheckinResult {
        checked_in: true,
        date: today,
        streak: new_streak,
        plan: outcome.plan,
        earned_plan: outcome.earned_plan,
        upgraded_to,
        candidate_created,
        message,
    }))
}

async fn checkin_history(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let dates: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT checkin_on FROM checkins WHERE user_id = $1 ORDER BY checkin_on DESC LIMIT 120",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "checkins": dates })))
}

#[derive(Deserialize)]
struct CreateKeyRequest {
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    models: Vec<String>,
}

#[derive(Serialize)]
struct KeyView {
    id: i64,
    key_prefix: String,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    status: &'static str,
    models: Vec<String>,
    all_models: bool,
}

async fn list_keys(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let rows: Vec<(i64, String, Option<DateTime<Utc>>, DateTime<Utc>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT id, key_prefix, expires_at, created_at, last_used_at, revoked_at \
             FROM api_keys WHERE user_id = $1 ORDER BY id DESC",
        )
        .bind(user.id)
        .fetch_all(&state.pool)
        .await?;
    let mut keys = Vec::with_capacity(rows.len());
    let now = Utc::now();
    for (id, prefix, expires_at, created_at, last_used_at, revoked_at) in rows {
        let models: Vec<String> = sqlx::query_scalar(
            "SELECT model FROM api_key_models WHERE key_id = $1 ORDER BY model",
        )
        .bind(id)
        .fetch_all(&state.pool)
        .await?;
        let status = if revoked_at.is_some() {
            "revoked"
        } else if expires_at.map(|e| e <= now).unwrap_or(false) {
            "expired"
        } else {
            "active"
        };
        keys.push(KeyView {
            id,
            key_prefix: prefix,
            expires_at,
            created_at,
            last_used_at,
            revoked_at,
            status,
            all_models: models.is_empty(),
            models,
        });
    }
    Ok(Json(json!({ "keys": keys })))
}

async fn create_key(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    AppJson(body): AppJson<CreateKeyRequest>,
) -> AppResult<Json<Value>> {
    if let Some(expires) = body.expires_at {
        if expires <= Utc::now() + chrono::Duration::seconds(30) {
            return Err(AppError::bad_request("过期时间必须晚于当前时间"));
        }
    }
    let all_models = body.models.is_empty();
    let mut models: Vec<String> = body
        .models
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    models.sort();
    models.dedup();
    if models.len() > 200 {
        return Err(AppError::bad_request("可指定模型数量不能超过 200 个"));
    }
    if !models.is_empty() {
        let available = usage::available_models(&state.pool, &user.plan).await?;
        if let Some(bad) = models.iter().find(|model| !available.contains(model)) {
            return Err(AppError::bad_request(format!(
                "模型 {bad} 不在你当前套餐的可用范围内"
            )));
        }
    }

    let key = crate::util::random_key();
    let hash = crate::util::sha256(key.as_bytes());
    let prefix: String = format!("{}…", key.chars().take(14).collect::<String>());
    let mut tx = state.pool.begin().await?;
    let key_id: i64 = sqlx::query_scalar(
        "INSERT INTO api_keys (user_id, key_hash, key_prefix, expires_at) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(user.id)
    .bind(&hash)
    .bind(&prefix)
    .bind(body.expires_at)
    .fetch_one(&mut *tx)
    .await?;
    for model in &models {
        sqlx::query("INSERT INTO api_key_models (key_id, model) VALUES ($1, $2)")
            .bind(key_id)
            .bind(model)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    Ok(Json(json!({
        "id": key_id,
        "key": key,
        "key_prefix": prefix,
        "expires_at": body.expires_at,
        "models": models,
        "all_models": all_models,
    })))
}

#[derive(Deserialize)]
struct UpdateKeyModelsRequest {
    #[serde(default)]
    models: Vec<String>,
}

/// 修改已有 Key 的可用模型；空列表 = 动态全部可用。
async fn update_key_models(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<UpdateKeyModelsRequest>,
) -> AppResult<Json<Value>> {
    let owned: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM api_keys WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;
    if owned.is_none() {
        return Err(AppError::not_found("Key 不存在或已撤销"));
    }
    let mut models: Vec<String> = body
        .models
        .into_iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    models.sort();
    models.dedup();
    if models.len() > 200 {
        return Err(AppError::bad_request("可指定模型数量不能超过 200 个"));
    }
    if !models.is_empty() {
        let available = usage::available_models(&state.pool, &user.plan).await?;
        if let Some(bad) = models.iter().find(|model| !available.contains(model)) {
            return Err(AppError::bad_request(format!(
                "模型 {bad} 不在你当前套餐的可用范围内"
            )));
        }
    }
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM api_key_models WHERE key_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for model in &models {
        sqlx::query("INSERT INTO api_key_models (key_id, model) VALUES ($1, $2)")
            .bind(id)
            .bind(model)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "models": models, "all_models": models.is_empty() })))
}

async fn delete_key(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let result = sqlx::query(
        "UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("Key 不存在或已撤销"));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct PageQuery {
    page: Option<i64>,
}

async fn usage_logs(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<Value>> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = 20i64;
    let offset = (page - 1) * limit;
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_logs WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    let logs: Vec<UsageLog> = sqlx::query_as(
        "SELECT l.id, l.request_id, l.model, l.prompt_tokens, l.completion_tokens, \
                l.total_tokens, l.cost_micro, l.status, l.error_code, l.latency_ms, \
                l.is_stream, l.created_at, c.name AS channel_name, k.key_prefix, \
                NULL::text AS user_name \
         FROM usage_logs l \
         LEFT JOIN channels c ON c.id = l.channel_id \
         LEFT JOIN api_keys k ON k.id = l.key_id \
         WHERE l.user_id = $1 ORDER BY l.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(user.id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "logs": logs,
        "total": total,
        "page": page,
        "pages": (total + limit - 1) / limit,
    })))
}

#[derive(Serialize, FromRow)]
struct AvailableModel {
    name: String,
    channel_count: i64,
    input_micro: i64,
    output_micro: i64,
}

async fn models_handler(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let models: Vec<AvailableModel> = sqlx::query_as(
        "SELECT cm.model AS name, COUNT(DISTINCT c.id)::bigint AS channel_count, \
                MIN(COALESCE(cm.input_micro, mp.input_micro, 1000000))::bigint AS input_micro, \
                MIN(COALESCE(cm.output_micro, mp.output_micro, 3000000))::bigint AS output_micro \
         FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         LEFT JOIN model_prices mp ON mp.model = cm.model \
         WHERE cm.enabled AND c.enabled AND (\
           CASE \
             WHEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id) \
               THEN EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id AND t.tier = $1) \
             WHEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id) \
               THEN EXISTS (SELECT 1 FROM channel_tiers ct WHERE ct.channel_id = c.id AND ct.tier = $1) \
             ELSE TRUE \
           END) \
         GROUP BY cm.model ORDER BY cm.model",
    )
    .bind(&user.plan)
    .fetch_all(&state.pool)
    .await?;
    let limits = plans::load_plan(&state.pool, &user.plan).await?.limits;
    Ok(Json(json!({
        "models": models,
        "plan": user.plan,
        "limits": limits,
    })))
}


// ---------------------------------------------------------------------------
// 模型广场 / 模型档位（还原自线上二进制：/api/model-square 需登录，/api/model-tiers 公开）
// ---------------------------------------------------------------------------

#[derive(FromRow)]
struct SquareRow {
    model: String,
    channel_count: i64,
    input_micro: i64,
    output_micro: i64,
}

/// tier -> 该档位可用的模型（按字节序排序）。规则与线上一致：
/// 1) 模型自身有档位授权 → 用模型档位；2) 否则渠道有档位 → 用渠道档位；
/// 3) 既不在 1) 也不在 2) 出现的模型 → 全部档位可用。
/// 返回 (tier->模型, 合并后的档位顺序)：自定义套餐也作为 tier 出现。
async fn model_tier_map(
    pool: &sqlx::PgPool,
) -> Result<(std::collections::BTreeMap<String, std::collections::BTreeSet<String>>, Vec<String>), sqlx::Error> {
    let explicit: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT cm.model, t.tier \
         FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         JOIN channel_model_tiers t ON t.channel_model_id = cm.id \
         WHERE cm.enabled AND c.enabled",
    )
    .fetch_all(pool)
    .await?;
    let inherited: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT cm.model, ct.tier \
         FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         JOIN channel_tiers ct ON ct.channel_id = c.id \
         WHERE cm.enabled AND c.enabled \
           AND NOT EXISTS (SELECT 1 FROM channel_model_tiers t WHERE t.channel_model_id = cm.id)",
    )
    .fetch_all(pool)
    .await?;
    let all: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT cm.model FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         WHERE cm.enabled AND c.enabled",
    )
    .fetch_all(pool)
    .await?;
    let codes: Vec<String> = plans::load_all_plans(pool)
        .await?
        .into_iter()
        .map(|p| p.code)
        .collect();
    Ok((build_tier_map(explicit, inherited, all, &codes), codes))
}

fn build_tier_map(
    explicit: Vec<(String, String)>,
    inherited: Vec<(String, String)>,
    all: Vec<String>,
    codes: &[String],
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut map: BTreeMap<String, BTreeSet<String>> = codes
        .iter()
        .map(|code| (code.clone(), BTreeSet::new()))
        .collect();
    let mut restricted: BTreeSet<String> = BTreeSet::new();
    for (model, tier) in explicit.into_iter().chain(inherited) {
        restricted.insert(model.clone());
        map.entry(tier).or_default().insert(model);
    }
    for model in all {
        if !restricted.contains(&model) {
            for set in map.values_mut() {
                set.insert(model.clone());
            }
        }
    }
    map
}

async fn model_tiers_handler(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let (map, _codes) = model_tier_map(&state.pool).await?;
    Ok(Json(json!({ "tiers": map })))
}

async fn model_square(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let rows: Vec<SquareRow> = sqlx::query_as(
        "SELECT cm.model, \
                COUNT(DISTINCT c.id)::bigint AS channel_count, \
                MIN(COALESCE(cm.input_micro, mp.input_micro, 1000000))::bigint AS input_micro, \
                MIN(COALESCE(cm.output_micro, mp.output_micro, 3000000))::bigint AS output_micro \
         FROM channel_models cm \
         JOIN channels c ON c.id = cm.channel_id \
         LEFT JOIN model_prices mp ON mp.model = cm.model \
         WHERE cm.enabled AND c.enabled \
         GROUP BY cm.model ORDER BY cm.model",
    )
    .fetch_all(&state.pool)
    .await?;
    let map = model_tier_map(&state.pool).await?;
    let (map, codes) = map;
    let models: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            let tiers: Vec<&str> = codes
                .iter()
                .map(|code| code.as_str())
                .filter(|code| map.get(*code).map(|s| s.contains(&row.model)).unwrap_or(false))
                .collect();
            let available_now = map
                .get(user.plan.as_str())
                .map(|s| s.contains(&row.model))
                .unwrap_or(false);
            json!({
                "name": row.model,
                "channel_count": row.channel_count,
                "input_micro": row.input_micro,
                "output_micro": row.output_micro,
                "tiers": tiers,
                "available_now": available_now,
            })
        })
        .collect();
    let available = models
        .iter()
        .filter(|m| m["available_now"].as_bool().unwrap_or(false))
        .count();
    Ok(Json(json!({
        "plan": user.plan,
        "total": models.len(),
        "available": available,
        "models": models,
    })))
}

#[cfg(test)]
mod square_tests {
    use super::build_tier_map;

    #[test]
    fn tier_map_rules() {
        let explicit = vec![("m-explicit".to_string(), "pro".to_string())];
        let inherited = vec![
            ("m-chan".to_string(), "lite".to_string()),
            ("m-chan".to_string(), "max".to_string()),
        ];
        let all = vec!["m-explicit".into(), "m-chan".into(), "m-open".into()];
        let codes: Vec<String> = plans::PLANS.iter().map(|p| p.code.to_string()).collect();
        let map = build_tier_map(explicit, inherited, all, &codes);
        assert_eq!(map.len(), 8);
        assert!(map["pro"].contains("m-explicit"));
        assert!(!map["lite"].contains("m-explicit"));
        assert!(map["lite"].contains("m-chan") && map["max"].contains("m-chan"));
        assert!(!map["pro"].contains("m-chan"));
        for set in map.values() {
            assert!(set.contains("m-open"));
        }
    }
}
