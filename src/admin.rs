use crate::auth::{require_admin, require_root, AuthUser};
use crate::channels;
use crate::crypto::mask_secret;
use crate::error::{AppError, AppJson, AppResult};
use crate::models::{Channel, ChannelModel, PlanApplication, UsageLog, User};
use crate::plans;
use crate::state::AppState;
use crate::util::{truncate_chars};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bootstrap-channel", post(bootstrap_channel))
        .route("/overview", get(overview))
        .route("/users", get(list_users))
        .route("/users/{id}/ban", post(ban_user))
        .route("/users/{id}/promote", post(promote_user))
        .route("/users/{id}/demote", post(demote_user))
        .route("/captcha/provider", get(get_captcha_provider).post(set_captcha_provider))
        .route("/users/{id}/grant", post(grant_plan))
        .route("/users/{id}/revoke", post(revoke_plan))
        .route("/applications", get(list_applications))
        .route("/applications/{id}/approve", post(approve_application))
        .route("/applications/{id}/reject", post(reject_application))
        .route("/makeup-cards/issue-all", post(crate::cards::issue_makeup_all))
        .route("/makeup-cards/issue", post(crate::cards::issue_makeup_one))
        .route("/makeup-cards", get(crate::cards::list_makeup_cards_admin))
        .route("/reset-codes", get(crate::cards::list_reset_codes_admin).post(crate::cards::create_reset_codes))
        .route("/reset-codes/issue", post(crate::cards::issue_reset_one))
        .route("/reset-codes/issue-all", post(crate::cards::issue_reset_all))
        .route("/reset-codes/{id}/revoke", post(crate::cards::revoke_reset_code))
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/bulk-tiers", post(bulk_tiers))
        .route("/channels/test-all", post(test_all_channels))
        .route("/channels/{id}", patch(update_channel).delete(delete_channel))
        .route("/channels/{id}/test", post(test_channel))
        .route("/channels/{id}/sync-models", post(sync_models))
        .route(
            "/channels/{id}/models",
            get(list_channel_models).post(add_model),
        )
        .route("/channels/{id}/tiers", put(set_channel_tiers))
        .route(
            "/models/{id}",
            patch(update_model).delete(delete_model),
        )
        .route("/models/{id}/tiers", put(set_model_tiers))
        .route("/logs", get(list_logs))
        .route("/audit", get(list_audit))
        .route("/settings", get(get_settings).patch(update_settings))
        .route("/notices", post(crate::notices::send_notice))
        .route("/prices", get(list_prices).post(create_price))
        .route(
            "/prices/{model}",
            put(update_price).delete(delete_price),
        )
        .route("/free-token-caps", get(list_free_token_caps).post(set_free_token_cap))
        .route("/free-token-caps/{id}", delete(delete_free_token_cap))
        .route("/plans", get(list_plans).post(create_plan))
        .route("/plans/{code}", patch(update_plan).delete(delete_plan))
}

#[derive(Serialize)]
struct Stats {
    users: i64,
    active_users: i64,
    banned_users: i64,
    active_keys: i64,
    channels: i64,
    healthy_channels: i64,
    pending_apps: i64,
    requests_today: i64,
    cost_today_micro: i64,
    requests_total: i64,
    cost_total_micro: i64,
}

#[derive(Serialize, FromRow)]
struct PlanCount {
    plan: String,
    count: i64,
}

#[derive(Serialize, FromRow)]
struct HealthCount {
    health: String,
    count: i64,
}

#[derive(Serialize, FromRow)]
struct SeriesPoint {
    day: String,
    requests: i64,
    cost_micro: i64,
}

#[derive(Serialize)]
struct OverviewResponse {
    stats: Stats,
    series: Vec<SeriesPoint>,
    plan_distribution: Vec<PlanCount>,
    channel_health: Vec<HealthCount>,
    recent: Vec<UsageLog>,
}

async fn overview(State(state): State<AppState>, AuthUser(user): AuthUser) -> AppResult<Json<OverviewResponse>> {
    require_admin(&user)?;
    let stats: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT \
          (SELECT count(*) FROM users), \
          (SELECT count(*) FROM users WHERE last_login_at >= now() - interval '7 days'), \
          (SELECT count(*) FROM users WHERE is_banned), \
          (SELECT count(*) FROM api_keys WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())), \
          (SELECT count(*) FROM channels), \
          (SELECT count(*) FROM channels WHERE enabled AND health = 'healthy'), \
          (SELECT count(*) FROM plan_applications WHERE status = 'pending'), \
          (SELECT count(*) FROM usage_logs WHERE (created_at AT TIME ZONE 'Asia/Shanghai')::date = (now() AT TIME ZONE 'Asia/Shanghai')::date), \
          (SELECT COALESCE(sum(cost_micro), 0)::bigint FROM usage_logs WHERE (created_at AT TIME ZONE 'Asia/Shanghai')::date = (now() AT TIME ZONE 'Asia/Shanghai')::date), \
          (SELECT count(*) FROM usage_logs), \
          (SELECT COALESCE(sum(cost_micro), 0)::bigint FROM usage_logs)",
    )
    .fetch_one(&state.pool)
    .await?;

    let series: Vec<SeriesPoint> = sqlx::query_as(
        "SELECT to_char(d, 'MM-DD') AS day, COUNT(l.id)::bigint AS requests, \
                COALESCE(SUM(l.cost_micro), 0)::bigint AS cost_micro \
         FROM generate_series((now() AT TIME ZONE 'Asia/Shanghai')::date - 6, \
              (now() AT TIME ZONE 'Asia/Shanghai')::date, interval '1 day') d \
         LEFT JOIN usage_logs l ON (l.created_at AT TIME ZONE 'Asia/Shanghai')::date = d \
         GROUP BY d ORDER BY d",
    )
    .fetch_all(&state.pool)
    .await?;

    let plan_distribution: Vec<PlanCount> = sqlx::query_as(
        "SELECT plan, count(*)::bigint AS count FROM users GROUP BY plan ORDER BY count DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    let channel_health: Vec<HealthCount> = sqlx::query_as(
        "SELECT health, count(*)::bigint AS count FROM channels GROUP BY health ORDER BY health",
    )
    .fetch_all(&state.pool)
    .await?;

    let recent: Vec<UsageLog> = sqlx::query_as(
        "SELECT l.id, l.request_id, l.model, l.prompt_tokens, l.completion_tokens, \
                l.total_tokens, l.cost_micro, l.status, l.error_code, l.latency_ms, \
                l.is_stream, l.created_at, c.name AS channel_name, k.key_prefix, \
                COALESCE(u.display_name, u.username) AS user_name \
         FROM usage_logs l \
         LEFT JOIN channels c ON c.id = l.channel_id \
         LEFT JOIN api_keys k ON k.id = l.key_id \
         LEFT JOIN users u ON u.id = l.user_id \
         ORDER BY l.id DESC LIMIT 12",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(OverviewResponse {
        stats: Stats {
            users: stats.0,
            active_users: stats.1,
            banned_users: stats.2,
            active_keys: stats.3,
            channels: stats.4,
            healthy_channels: stats.5,
            pending_apps: stats.6,
            requests_today: stats.7,
            cost_today_micro: stats.8,
            requests_total: stats.9,
            cost_total_micro: stats.10,
        },
        series,
        plan_distribution,
        channel_health,
        recent,
    }))
}

#[derive(Deserialize)]
struct UsersQuery {
    q: Option<String>,
    page: Option<i64>,
}

async fn list_users(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<UsersQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let q = query.q.unwrap_or_default().trim().to_string();
    let today = state.cfg.today();
    let yesterday = today.pred_opt().unwrap_or(today);
    let page = query.page.unwrap_or(1).max(1);
    let limit = 20i64;
    let offset = (page - 1) * limit;

    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users \
         WHERE ($1 = '' OR username ILIKE '%' || $1 || '%' OR COALESCE(display_name, '') ILIKE '%' || $1 || '%')",
    )
    .bind(&q)
    .fetch_one(&state.pool)
    .await?;
    let users: Vec<User> = sqlx::query_as(
        "SELECT * FROM users \
         WHERE ($1 = '' OR username ILIKE '%' || $1 || '%' OR COALESCE(display_name, '') ILIKE '%' || $1 || '%') \
         ORDER BY id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&q)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let ids: Vec<i64> = users.iter().map(|u| u.id).collect();

    let key_counts: Vec<(i64, i64)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT user_id, count(*)::bigint FROM api_keys WHERE user_id = ANY($1) \
             AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()) GROUP BY user_id",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?
    };
    let usage_stats: Vec<(i64, i64, i64)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT user_id, count(*)::bigint, COALESCE(sum(cost_micro), 0)::bigint \
             FROM usage_logs WHERE user_id = ANY($1) AND created_at >= now() - interval '30 days' \
             GROUP BY user_id",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?
    };
    let pending_counts: Vec<(i64, i64)> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT user_id, count(*)::bigint FROM plan_applications WHERE user_id = ANY($1) \
             AND status = 'pending' GROUP BY user_id",
        )
        .bind(&ids)
        .fetch_all(&state.pool)
        .await?
    };

    let key_map: HashMap<i64, i64> = key_counts.into_iter().collect();
    let pending_map: HashMap<i64, i64> = pending_counts.into_iter().collect();
    let usage_map: HashMap<i64, (i64, i64)> = usage_stats
        .into_iter()
        .map(|(id, requests, cost)| (id, (requests, cost)))
        .collect();

    let rows: Vec<Value> = users
        .into_iter()
        .map(|u| {
            let (requests, cost) = usage_map.get(&u.id).copied().unwrap_or((0, 0));
            let streak_active = matches!(u.last_checkin_on, Some(day) if day == today || day == yesterday);
            let active_streak = if streak_active { u.consecutive_days } else { 0 };
            json!({
                "id": u.id,
                "linuxdo_id": u.linuxdo_id,
                "username": u.username,
                "display_name": u.display_name,
                "avatar_url": u.avatar_url,
                "trust_level": u.trust_level,
                "role": u.role,
                "plan": u.plan,
                "earned_plan": u.earned_plan,
                "plan_source": u.plan_source,
                "is_banned": u.is_banned,
                "ban_reason": u.ban_reason,
                "streak": active_streak,
                "longest_streak": u.longest_streak,
                "last_checkin_on": u.last_checkin_on,
                "created_at": u.created_at,
                "last_login_at": u.last_login_at,
                "key_count": key_map.get(&u.id).copied().unwrap_or(0),
                "requests_30d": requests,
                "cost_30d_micro": cost,
                "pending_apps": pending_map.get(&u.id).copied().unwrap_or(0),
            })
        })
        .collect();

    Ok(Json(json!({
        "users": rows,
        "total": total,
        "page": page,
        "pages": (total + limit - 1) / limit,
    })))
}

#[derive(Deserialize)]
struct BanRequest {
    banned: bool,
    reason: Option<String>,
}

async fn ban_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<BanRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if target.is_root() {
        return Err(AppError::bad_request("不能封禁站长账号"));
    }
    if target.id == actor.id {
        return Err(AppError::bad_request("不能封禁自己"));
    }
    let reason = body
        .reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 200));
    sqlx::query("UPDATE users SET is_banned = $2, ban_reason = $3, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(body.banned)
        .bind(&reason)
        .execute(&state.pool)
        .await?;
    if body.banned {
        sqlx::query("DELETE FROM sessions WHERE user_id = $1")
            .bind(id)
            .execute(&state.pool)
            .await?;
    }
    write_audit(
        &state.pool,
        Some(actor.id),
        if body.banned { "user.ban" } else { "user.unban" },
        "user",
        &id.to_string(),
        json!({ "username": target.username, "reason": reason }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "banned": body.banned })))
}

/// 提拔目标校验（纯函数，便于单测）：站长不能被提拔，已是管理员的不能重复提拔。
fn check_promote_target(target: &User) -> AppResult<()> {
    if target.is_root() {
        return Err(AppError::conflict("该用户已是站长"));
    }
    if target.role == "admin" {
        return Err(AppError::conflict("该用户已经是管理员"));
    }
    Ok(())
}

/// 取消管理员目标校验（纯函数，便于单测）：站长不能被取消，不是管理员的不能取消。
fn check_demote_target(target: &User) -> AppResult<()> {
    if target.is_root() {
        return Err(AppError::conflict("不能取消站长的管理员身份"));
    }
    if target.role != "admin" {
        return Err(AppError::conflict("该用户不是管理员"));
    }
    Ok(())
}

/// POST /admin/users/{id}/promote：仅站长可调用，把普通用户提升为管理员。
async fn promote_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_root(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    check_promote_target(&target)?;
    sqlx::query("UPDATE users SET role = 'admin', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "user.promote",
        "user",
        &id.to_string(),
        json!({ "username": target.username, "from": target.role, "to": "admin" }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "role": "admin" })))
}

/// POST /admin/users/{id}/demote：仅站长可调用，取消管理员身份，恢复为普通用户。
async fn demote_user(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_root(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    check_demote_target(&target)?;
    sqlx::query("UPDATE users SET role = 'user', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "user.demote",
        "user",
        &id.to_string(),
        json!({ "username": target.username, "from": target.role, "to": "user" }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "role": "user" })))
}

/// GET /api/admin/captcha/provider：查看人机验证双通道状态（管理员）。
/// 返回模式（auto/hcaptcha/turnstile）、当前生效通道、连续失败数、冷却截止、
/// 两通道是否已配置及各自 sitekey（sitekey 是公开配置）。
async fn get_captcha_provider(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&user)?;
    let st = crate::captcha::load_provider_state(&state.pool).await;
    let cfg = &state.captcha;
    Ok(Json(json!({
        "mode": st.mode,
        "active": st.active,
        "provider": st.effective(),
        "failures": st.failures,
        "down_until": st.down_until,
        "configured": {
            "hcaptcha": cfg.configured(crate::captcha::Provider::Hcaptcha),
            "turnstile": cfg.configured(crate::captcha::Provider::Turnstile),
        },
        "sitekeys": {
            "hcaptcha": cfg.sitekey(crate::captcha::Provider::Hcaptcha),
            "turnstile": cfg.sitekey(crate::captcha::Provider::Turnstile),
        },
    })))
}

#[derive(Deserialize)]
struct SetCaptchaProviderRequest {
    mode: String,
}

/// POST /api/admin/captcha/provider：管理员手动强制指定通道模式。
/// body {"mode": "auto"|"hcaptcha"|"turnstile"}；auto 为自动切换（默认），
/// 其余两项强制走指定通道且不再自动切换。记审计 captcha.provider_override。
async fn set_captcha_provider(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<SetCaptchaProviderRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let mode = match body.mode.to_lowercase().as_str() {
        "auto" => crate::captcha::ProviderMode::Auto,
        "hcaptcha" => crate::captcha::ProviderMode::Hcaptcha,
        "turnstile" => crate::captcha::ProviderMode::Turnstile,
        other => return Err(AppError::bad_request(format!("未知的通道模式: {other}"))),
    };
    let mut st = crate::captcha::load_provider_state(&state.pool).await;
    let from = st.mode;
    st.mode = mode;
    if mode != crate::captcha::ProviderMode::Auto {
        // 强制模式下清零失败计数与冷却，避免残留状态干扰
        st.failures = 0;
        st.down_until = None;
    }
    crate::captcha::save_provider_state(&state.pool, &st).await;
    write_audit(
        &state.pool,
        Some(actor.id),
        "captcha.provider_override",
        "captcha_provider",
        "provider",
        json!({ "from": from, "to": mode }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "mode": st.mode, "provider": st.effective() })))
}

#[derive(Deserialize)]
struct GrantRequest {
    plan: String,
}

async fn grant_plan(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<GrantRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let target_plan = body.plan.trim().to_ascii_lowercase();
    // 手动授予：内置档只放 Plus / Pro / Max / Ultra（低档位靠签到获得）；
    // 自定义套餐只能靠手动授予获得，所以一律可授。目标必须真实存在。
    let merged = plans::load_all_plans(&state.pool).await?;
    let target_view = merged
        .iter()
        .find(|plan| plan.code == target_plan)
        .ok_or_else(|| AppError::bad_request(format!("未知套餐：{target_plan}")))?;
    if target_view.builtin && !matches!(target_plan.as_str(), "plus" | "pro" | "max" | "ultra") {
        return Err(AppError::bad_request(
            "内置套餐只能手动授予 Plus、Pro、Max 或 Ultra（低档位靠签到获得）",
        ));
    }
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    let current_rank = plans::merged_rank(&state.pool, &target.plan).await?;
    let target_rank = plans::merged_rank(&state.pool, &target_plan).await?;
    if current_rank >= target_rank {
        let current_name = merged
            .iter()
            .find(|plan| plan.code == target.plan)
            .map(|plan| plan.name.clone())
            .unwrap_or_else(|| target.plan.clone());
        return Err(AppError::bad_request(format!(
            "用户当前已是 {current_name} 或更高档位"
        )));
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE users SET plan = $2, plan_source = 'manual', updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(&target_plan)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO plan_events (user_id, from_plan, to_plan, source, actor_id, detail) \
         VALUES ($1, $2, $3, 'manual', $4, $5)",
    )
    .bind(id)
    .bind(&target.plan)
    .bind(&target_plan)
    .bind(actor.id)
    .bind("站长手动授予")
    .execute(&mut *tx)
    .await?;

    let pending: Vec<PlanApplication> = sqlx::query_as(
        "SELECT * FROM plan_applications WHERE user_id = $1 AND status = 'pending' FOR UPDATE",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    for app in pending {
        if plans::merged_rank(&state.pool, &app.target_plan).await? <= target_rank {
            sqlx::query(
                "UPDATE plan_applications SET status = 'approved', reviewed_at = now(), \
                 reviewed_by = $2, review_note = $3 WHERE id = $1",
            )
            .bind(app.id)
            .bind(actor.id)
            .bind("站长手动授予，候补申请自动通过")
            .execute(&mut *tx)
            .await?;
        }
    }
    if target_plan == "max" {
        enqueue_ultra_candidate(&mut tx, id).await?;
    }
    tx.commit().await?;

    write_audit(
        &state.pool,
        Some(actor.id),
        "user.grant",
        "user",
        &id.to_string(),
        json!({ "username": target.username, "from": target.plan, "to": target_plan }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "plan": target_plan })))
}

async fn revoke_plan(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if plans::merged_rank(&state.pool, &target.plan).await? < plans::merged_rank(&state.pool, "plus").await? {
        return Err(AppError::bad_request("当前没有可撤销的 Plus / Pro / Max 档位"));
    }
    let fallback = if plans::merged_rank(&state.pool, &target.earned_plan).await?
        < plans::merged_rank(&state.pool, &target.plan).await?
    {
        target.earned_plan.clone()
    } else {
        "standard".to_string()
    };
    sqlx::query(
        "UPDATE users SET plan = $2, plan_source = 'revoke', updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(&fallback)
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "INSERT INTO plan_events (user_id, from_plan, to_plan, source, actor_id, detail) \
         VALUES ($1, $2, $3, 'revoke', $4, $5)",
    )
    .bind(id)
    .bind(&target.plan)
    .bind(&fallback)
    .bind(actor.id)
    .bind("站长撤销 Plus / Pro / Max")
    .execute(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "user.revoke",
        "user",
        &id.to_string(),
        json!({ "username": target.username, "from": target.plan, "to": fallback }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "plan": fallback })))
}

#[derive(Deserialize)]
struct ApplicationQuery {
    status: Option<String>,
}

#[derive(Serialize, FromRow)]
struct ApplicationRow {
    id: i64,
    user_id: i64,
    target_plan: String,
    status: String,
    note: Option<String>,
    created_at: DateTime<Utc>,
    reviewed_at: Option<DateTime<Utc>>,
    reviewed_by: Option<i64>,
    review_note: Option<String>,
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    current_plan: String,
    earned_plan: String,
    consecutive_days: i32,
    longest_streak: i32,
    trust_level: i32,
}

async fn list_applications(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<ApplicationQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let status = query
        .status
        .unwrap_or_else(|| "pending".to_string())
        .to_ascii_lowercase();
    let apps: Vec<ApplicationRow> = sqlx::query_as(
        "SELECT a.id, a.user_id, a.target_plan, a.status, a.note, a.created_at, \
                a.reviewed_at, a.reviewed_by, a.review_note, u.username, u.display_name, \
                u.avatar_url, u.plan AS current_plan, u.earned_plan, \
                u.consecutive_days, u.longest_streak, u.trust_level \
         FROM plan_applications a JOIN users u ON u.id = a.user_id \
         WHERE ($1 = 'all' OR a.status = $1) \
         ORDER BY a.created_at DESC LIMIT 300",
    )
    .bind(&status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "applications": apps })))
}

#[derive(Deserialize)]
struct ReviewRequest {
    note: Option<String>,
}

async fn approve_application(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<ReviewRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let mut tx = state.pool.begin().await?;
    let app: PlanApplication = sqlx::query_as(
        "SELECT * FROM plan_applications WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::not_found("申请不存在"))?;
    if app.status != "pending" {
        return Err(AppError::conflict("该申请已经处理过了"));
    }
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1 FOR UPDATE")
        .bind(app.user_id)
        .fetch_one(&mut *tx)
        .await?;

    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 200));
    let mut changed = false;
    if plans::merged_rank(&state.pool, &app.target_plan).await?
        > plans::merged_rank(&state.pool, &target.plan).await?
    {
        sqlx::query(
            "UPDATE users SET plan = $2, plan_source = 'approval', updated_at = now() WHERE id = $1",
        )
        .bind(target.id)
        .bind(&app.target_plan)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO plan_events (user_id, from_plan, to_plan, source, actor_id, detail) \
             VALUES ($1, $2, $3, 'approval', $4, $5)",
        )
        .bind(target.id)
        .bind(&target.plan)
        .bind(&app.target_plan)
        .bind(actor.id)
        .bind(note.as_deref().unwrap_or("站长审批通过"))
        .execute(&mut *tx)
        .await?;
        changed = true;
    }
    sqlx::query(
        "UPDATE plan_applications SET status = 'approved', reviewed_at = now(), \
         reviewed_by = $2, review_note = $3 WHERE id = $1",
    )
    .bind(app.id)
    .bind(actor.id)
    .bind(note.as_deref().unwrap_or("审批通过"))
    .execute(&mut *tx)
    .await?;

    if app.target_plan == "max" || app.target_plan == "ultra" {
        sqlx::query(
            "UPDATE plan_applications SET status = 'rejected', reviewed_at = now(), \
             reviewed_by = $2, review_note = $4 \
             WHERE user_id = $1 AND status = 'pending' AND id <> $3",
        )
        .bind(target.id)
        .bind(actor.id)
        .bind(app.id)
        .bind(format!("已晋升 {}，申请自动关闭", plans::plan(&app.target_plan).name))
        .execute(&mut *tx)
        .await?;
    }
    if app.target_plan == "max"
        && plans::merged_rank(&state.pool, &target.plan).await?
            < plans::merged_rank(&state.pool, "ultra").await?
    {
        enqueue_ultra_candidate(&mut tx, target.id).await?;
    }
    tx.commit().await?;

    write_audit(
        &state.pool,
        Some(actor.id),
        "application.approve",
        "user",
        &target.id.to_string(),
        json!({ "application_id": app.id, "target": app.target_plan, "changed": changed, "note": note }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "plan": app.target_plan })))
}

async fn reject_application(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<ReviewRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let app: PlanApplication = sqlx::query_as(
        "SELECT * FROM plan_applications WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("申请不存在"))?;
    if app.status != "pending" {
        return Err(AppError::conflict("该申请已经处理过了"));
    }
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 200));
    sqlx::query(
        "UPDATE plan_applications SET status = 'rejected', reviewed_at = now(), \
         reviewed_by = $2, review_note = $3 WHERE id = $1",
    )
    .bind(app.id)
    .bind(actor.id)
    .bind(note.as_deref().unwrap_or("站长驳回"))
    .execute(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "application.reject",
        "user",
        &app.user_id.to_string(),
        json!({ "application_id": app.id, "target": app.target_plan, "note": note }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct BootstrapChannelRequest {
    name: String,
    base_url: String,
    api_key: String,
    #[serde(default)]
    tiers: Vec<String>,
    priority: Option<i32>,
    weight: Option<i32>,
}

/// First-run provisioning: provision the first channel before any user exists.
async fn bootstrap_channel(
    State(state): State<AppState>,
    axum::extract::Extension(secret): axum::extract::Extension<crate::error::SetupSecret>,
    headers: axum::http::HeaderMap,
    AppJson(body): AppJson<BootstrapChannelRequest>,
) -> AppResult<Json<Value>> {
    let used: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::boolean FROM settings WHERE key = 'bootstrap_used'), FALSE)",
    )
    .fetch_one(&state.pool)
    .await?;
    if used {
        return Err(AppError::conflict("初始化通道已使用，请使用站长后台"));
    }
    let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&state.pool)
        .await?;
    let channel_count: i64 = sqlx::query_scalar("SELECT count(*) FROM channels")
        .fetch_one(&state.pool)
        .await?;
    if user_count > 0 || channel_count > 0 {
        return Err(AppError::conflict("站点已有数据，初始化通道已关闭"));
    }
    let provided = headers
        .get("x-bootstrap-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = secret.0.as_bytes();
    let ok = expected.len() > 0
        && provided.len() == expected.len()
        && provided
            .as_bytes()
            .iter()
            .zip(expected)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0;
    if !ok {
        return Err(AppError::unauthorized("初始化口令无效"));
    }
    if body.name.trim().is_empty() || body.api_key.trim().is_empty() {
        return Err(AppError::bad_request("渠道名称与 API Key 不能为空"));
    }
    let base_url = normalize_base_url(&body.base_url)?;
    let tiers = checked_tiers(&state.pool, &body.tiers).await?;
    let enc = state
        .crypto
        .encrypt(body.api_key.trim().as_bytes())
        .map_err(|e| AppError::internal(format!("密钥加密失败：{e}")))?;
    let hint = mask_secret(body.api_key.trim());

    let mut tx = state.pool.begin().await?;
    let channel_id: i64 = sqlx::query_scalar(
        "INSERT INTO channels (name, base_url, api_key_enc, key_hint, priority, weight) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(body.name.trim())
    .bind(&base_url)
    .bind(&enc)
    .bind(&hint)
    .bind(body.priority.unwrap_or(0))
    .bind(body.weight.unwrap_or(1).clamp(1, 100))
    .fetch_one(&mut *tx)
    .await?;
    for tier in &tiers {
        sqlx::query("INSERT INTO channel_tiers (channel_id, tier) VALUES ($1, $2)")
            .bind(channel_id)
            .bind(tier)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('bootstrap_used', 'true'::jsonb, now()) \
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    write_audit(
        &state.pool,
        None,
        "channel.bootstrap",
        "channel",
        &channel_id.to_string(),
        json!({ "name": body.name, "base_url": base_url, "tiers": tiers }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": channel_id })))
}

async fn list_channels(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let channels: Vec<Channel> =
        sqlx::query_as("SELECT * FROM channels ORDER BY priority DESC, id ASC")
            .fetch_all(&state.pool)
            .await?;
    let channel_tiers: Vec<(i64, String)> =
        sqlx::query_as("SELECT channel_id, tier FROM channel_tiers ORDER BY tier")
            .fetch_all(&state.pool)
            .await?;
    let models: Vec<ChannelModel> =
        sqlx::query_as("SELECT * FROM channel_models ORDER BY model")
            .fetch_all(&state.pool)
            .await?;
    let model_tiers: Vec<(i64, String)> =
        sqlx::query_as("SELECT channel_model_id, tier FROM channel_model_tiers ORDER BY tier")
            .fetch_all(&state.pool)
            .await?;

    let mut tier_map: HashMap<i64, Vec<String>> = HashMap::new();
    for (channel_id, tier) in channel_tiers {
        tier_map.entry(channel_id).or_default().push(tier);
    }
    let mut model_tier_map: HashMap<i64, Vec<String>> = HashMap::new();
    for (model_id, tier) in model_tiers {
        model_tier_map.entry(model_id).or_default().push(tier);
    }
    let mut models_by_channel: HashMap<i64, Vec<Value>> = HashMap::new();
    for model in models {
        let own_tiers = model_tier_map.get(&model.id).cloned().unwrap_or_default();
        let channel_own = tier_map.get(&model.channel_id).cloned().unwrap_or_default();
        let effective = if !own_tiers.is_empty() {
            own_tiers.clone()
        } else if !channel_own.is_empty() {
            channel_own.clone()
        } else {
            Vec::new()
        };
        let row = json!({
            "id": model.id,
            "channel_id": model.channel_id,
            "model": model.model,
            "upstream_model": model.upstream_model,
            "enabled": model.enabled,
            "input_micro": model.input_micro,
            "output_micro": model.output_micro,
            "tiers": own_tiers,
            "inherits_channel_tiers": own_tiers.is_empty(),
            "effective_tiers": effective,
            "created_at": model.created_at,
            "last_test_at": model.last_test_at,
            "last_test_ok": model.last_test_ok,
            "last_test_ms": model.last_test_ms,
            "last_test_error": model.last_test_error,
        });
        models_by_channel.entry(model.channel_id).or_default().push(row);
    }

    let rows: Vec<Value> = channels
        .into_iter()
        .map(|channel| {
            let tiers = tier_map.get(&channel.id).cloned().unwrap_or_default();
            let channel_models = models_by_channel.remove(&channel.id).unwrap_or_default();
            let model_count = channel_models.len();
            json!({
                "id": channel.id,
                "name": channel.name,
                "base_url": channel.base_url,
                "key_hint": channel.key_hint,
                "enabled": channel.enabled,
                "priority": channel.priority,
                "weight": channel.weight,
                "health": channel.health,
                "last_health_at": channel.last_health_at,
                "last_error": channel.last_error,
                "created_at": channel.created_at,
                "tiers": tiers,
                "tiers_all": tiers.is_empty(),
                "models": channel_models,
                "model_count": model_count,
                "extra_headers": channel.extra_headers,
                "strip_reasoning_with_tools": channel.strip_reasoning_with_tools,
                "param_override": channel.param_override,
                "test_model": channel.test_model,
                "test_endpoint": channel.test_endpoint,
                "last_test_at": channel.last_test_at,
                "last_test_ok": channel.last_test_ok,
                "last_test_ms": channel.last_test_ms,
                "last_test_model": channel.last_test_model,
            })
        })
        .collect();
    Ok(Json(json!({ "channels": rows })))
}

/// 校验档位列表：内置档 + 自定义套餐都合法（查合并后的套餐表）。
/// 空列表 = 全部档位可用（含未来新增档位），与渠道授权语义一致。
async fn checked_tiers(pool: &PgPool, input: &[String]) -> AppResult<Vec<String>> {
    let known = plans::load_all_plans(pool).await?;
    let codes: std::collections::HashSet<&str> =
        known.iter().map(|plan| plan.code.as_str()).collect();
    let mut out = Vec::new();
    for raw in input {
        let tier = raw.trim().to_ascii_lowercase();
        if tier.is_empty() {
            continue;
        }
        if !codes.contains(tier.as_str()) {
            return Err(AppError::bad_request(format!("未知套餐档位：{tier}")));
        }
        out.push(tier);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn normalize_base_url(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| AppError::bad_request("渠道 Base URL 不是合法的 URL"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::bad_request("渠道 Base URL 必须使用 http 或 https"));
    }
    if parsed.host_str().is_none() {
        return Err(AppError::bad_request("渠道 Base URL 缺少主机名"));
    }
    Ok(trimmed.to_string())
}

/// 接受 JSON 对象，或一段 JSON 文本（前端 textarea 原文）。空串/null 视为 {}。
fn json_object_input(value: &Value, label: &str) -> AppResult<Value> {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return Ok(json!({}));
            }
            serde_json::from_str::<Value>(text)
                .map_err(|e| AppError::bad_request(format!("{label}不是合法的 JSON：{e}")))
        }
        other => Ok(other.clone()),
    }
}

fn checked_extra_headers(value: &Value) -> AppResult<Value> {
    let parsed = json_object_input(value, "额外请求头")?;
    crate::channel_opts::validate_extra_headers(&parsed).map_err(AppError::bad_request)
}

fn checked_param_override(value: &Value) -> AppResult<Value> {
    let parsed = json_object_input(value, "参数覆盖")?;
    crate::channel_opts::validate_param_override(&parsed).map_err(AppError::bad_request)
}

fn checked_test_endpoint(value: Option<&str>, current: &str) -> AppResult<String> {
    match value.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(current.to_string()),
        Some(v) if channels::is_valid_test_endpoint(v) => Ok(v.to_string()),
        Some(v) => Err(AppError::bad_request(format!("检测接口只能是 chat 或 responses：{v}"))),
    }
}

fn checked_test_model(value: Option<&str>) -> AppResult<Option<String>> {
    let v = value.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    if let Some(m) = &v {
        if m.chars().count() > 200 {
            return Err(AppError::bad_request("测试模型名称过长"));
        }
    }
    Ok(v)
}

#[derive(Deserialize)]
struct CreateChannelRequest {
    name: String,
    base_url: String,
    api_key: String,
    priority: Option<i32>,
    weight: Option<i32>,
    enabled: Option<bool>,
    #[serde(default)]
    tiers: Vec<String>,
    #[serde(default)]
    extra_headers: Option<Value>,
    #[serde(default)]
    strip_reasoning_with_tools: Option<bool>,
    #[serde(default)]
    param_override: Option<Value>,
    #[serde(default)]
    test_model: Option<String>,
    #[serde(default)]
    test_endpoint: Option<String>,
}

async fn create_channel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<CreateChannelRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::bad_request("渠道名称不能为空且不能超过 100 字"));
    }
    let base_url = normalize_base_url(&body.base_url)?;
    let api_key = body.api_key.trim();
    if api_key.is_empty() {
        return Err(AppError::bad_request("渠道 API Key 不能为空"));
    }
    let tiers = checked_tiers(&state.pool, &body.tiers).await?;
    let extra_headers = checked_extra_headers(body.extra_headers.as_ref().unwrap_or(&Value::Null))?;
    let param_override = checked_param_override(body.param_override.as_ref().unwrap_or(&Value::Null))?;
    let test_model = checked_test_model(body.test_model.as_deref())?;
    let strip_reasoning = body.strip_reasoning_with_tools.unwrap_or(false);
    let test_endpoint = checked_test_endpoint(body.test_endpoint.as_deref(), "chat")?;
    let enc = state
        .crypto
        .encrypt(api_key.as_bytes())
        .map_err(|e| AppError::internal(format!("密钥加密失败：{e}")))?;
    let hint = mask_secret(api_key);
    let weight = body.weight.unwrap_or(1).clamp(1, 100);

    let mut tx = state.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO channels (name, base_url, api_key_enc, key_hint, enabled, priority, weight, \
         extra_headers, strip_reasoning_with_tools, param_override, test_model, test_endpoint) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
    )
    .bind(name)
    .bind(&base_url)
    .bind(&enc)
    .bind(&hint)
    .bind(body.enabled.unwrap_or(true))
    .bind(body.priority.unwrap_or(0))
    .bind(weight)
    .bind(&extra_headers)
    .bind(strip_reasoning)
    .bind(&param_override)
    .bind(&test_model)
    .bind(&test_endpoint)
    .fetch_one(&mut *tx)
    .await?;
    for tier in &tiers {
        sqlx::query("INSERT INTO channel_tiers (channel_id, tier) VALUES ($1, $2)")
            .bind(id)
            .bind(tier)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.create",
        "channel",
        &id.to_string(),
        json!({ "name": name, "base_url": base_url, "tiers": tiers,
                "extra_header_names": extra_headers.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                "strip_reasoning_with_tools": strip_reasoning, "param_override": param_override }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Deserialize)]
struct UpdateChannelRequest {
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    priority: Option<i32>,
    weight: Option<i32>,
    enabled: Option<bool>,
    tiers: Option<Vec<String>>,
    extra_headers: Option<Value>,
    strip_reasoning_with_tools: Option<bool>,
    param_override: Option<Value>,
    /// 传空串清除。
    test_model: Option<String>,
    test_endpoint: Option<String>,
}

async fn update_channel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<UpdateChannelRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let channel: Channel = sqlx::query_as("SELECT * FROM channels WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("渠道不存在"))?;

    let name = body
        .name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| channel.name.clone());
    if name.chars().count() > 100 {
        return Err(AppError::bad_request("渠道名称不能超过 100 字"));
    }
    let base_url = match body.base_url.as_deref() {
        Some(value) => normalize_base_url(value)?,
        None => channel.base_url.clone(),
    };
    let (enc, hint) = match body.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(key) => (
            state
                .crypto
                .encrypt(key.as_bytes())
                .map_err(|e| AppError::internal(format!("密钥加密失败：{e}")))?,
            mask_secret(key),
        ),
        None => (channel.api_key_enc.clone(), channel.key_hint.clone()),
    };
    let tiers = match body.tiers {
        Some(tiers) => Some(checked_tiers(&state.pool, &tiers).await?),
        None => None,
    };
    let extra_headers = match &body.extra_headers {
        Some(v) => checked_extra_headers(v)?,
        None => channel.extra_headers.clone(),
    };
    let param_override = match &body.param_override {
        Some(v) => checked_param_override(v)?,
        None => channel.param_override.clone(),
    };
    let test_model = match &body.test_model {
        Some(v) => checked_test_model(Some(v))?,
        None => channel.test_model.clone(),
    };
    let strip_reasoning = body
        .strip_reasoning_with_tools
        .unwrap_or(channel.strip_reasoning_with_tools);
    let test_endpoint = checked_test_endpoint(body.test_endpoint.as_deref(), &channel.test_endpoint)?;

    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE channels SET name = $2, base_url = $3, api_key_enc = $4, key_hint = $5, \
         priority = $6, weight = $7, enabled = $8, extra_headers = $9, \
         strip_reasoning_with_tools = $10, param_override = $11, test_model = $12, test_endpoint = $13, \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(&base_url)
    .bind(&enc)
    .bind(&hint)
    .bind(body.priority.unwrap_or(channel.priority))
    .bind(body.weight.unwrap_or(channel.weight).clamp(1, 100))
    .bind(body.enabled.unwrap_or(channel.enabled))
    .bind(&extra_headers)
    .bind(strip_reasoning)
    .bind(&param_override)
    .bind(&test_model)
    .bind(&test_endpoint)
    .execute(&mut *tx)
    .await?;
    if let Some(tiers) = &tiers {
        sqlx::query("DELETE FROM channel_tiers WHERE channel_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for tier in tiers {
            sqlx::query("INSERT INTO channel_tiers (channel_id, tier) VALUES ($1, $2)")
                .bind(id)
                .bind(tier)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.update",
        "channel",
        &id.to_string(),
        json!({ "name": name, "base_url": base_url, "tiers": tiers,
                "extra_header_names": extra_headers.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                "strip_reasoning_with_tools": strip_reasoning, "param_override": param_override }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_channel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let channel: Option<Channel> = sqlx::query_as("SELECT * FROM channels WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(channel) = channel else {
        return Err(AppError::not_found("渠道不存在"));
    };
    sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.delete",
        "channel",
        &id.to_string(),
        json!({ "name": channel.name }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
struct TestChannelRequest {
    #[serde(default)]
    model: Option<String>,
}

/// POST /admin/channels/{id}/test
/// - body `{ "model": "gpt-6-astra" }` → 用该模型发最小 chat/completions（New-API 式模型检测）
/// - body `{}` 或空 → 旧行为：拉取上游 /models 做连通性检查（线上前端「同步」前仍会用）
async fn test_channel(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    raw: axum::body::Bytes,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let body: TestChannelRequest = if raw.iter().all(|b| b.is_ascii_whitespace()) {
        TestChannelRequest::default()
    } else {
        serde_json::from_slice(&raw)
            .map_err(|e| AppError::bad_request(format!("请求参数错误：{e}")))?
    };
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channels WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(AppError::not_found("渠道不存在"));
    }
    if let Some(model) = body.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        if model.chars().count() > 200 {
            return Err(AppError::bad_request("模型名称过长"));
        }
        let result = channels::test_channel_model(&state, id, model)
            .await
            .map_err(|e| AppError::new(StatusCode::BAD_GATEWAY, "channel_check_failed", e))?;
        return Ok(Json(serde_json::to_value(&result).unwrap_or_else(|_| json!({}))));
    }
    let result = channels::check_channel(&state, id)
        .await
        .map_err(|e| AppError::new(StatusCode::BAD_GATEWAY, "channel_check_failed", e))?;
    Ok(Json(json!({
        "ok": result.ok,
        "status": result.status,
        "models": result.models.iter().take(300).collect::<Vec<_>>(),
        "model_count": result.models.len(),
        "error": result.error,
        "latency_ms": result.latency_ms,
    })))
}

/// POST /admin/channels/test-all：每个启用渠道测其默认测试模型（并发 3）。
async fn test_all_channels(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let results = channels::test_all_channels(&state).await?;
    let ok = results.iter().filter(|r| r.ok).count();
    Ok(Json(json!({
        "total": results.len(),
        "ok": ok,
        "failed": results.len() - ok,
        "results": results,
    })))
}

async fn sync_models(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let channel: Channel = sqlx::query_as("SELECT * FROM channels WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("渠道不存在"))?;
    let key = state
        .crypto
        .decrypt(&channel.api_key_enc)
        .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
        .map_err(|e| AppError::internal(format!("渠道密钥解密失败：{e}")))?;
    let (_, models) = channels::fetch_upstream_models(&state, &channel.base_url, &key)
        .await
        .map_err(|(status, message)| {
            AppError::new(
                if status == 0 {
                    StatusCode::BAD_GATEWAY
                } else {
                    StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY)
                },
                "upstream_models_failed",
                message,
            )
        })?;
    if models.is_empty() {
        return Err(AppError::bad_request("上游没有返回任何模型"));
    }
    let mut added = 0u64;
    let mut tx = state.pool.begin().await?;
    for model in &models {
        let result = sqlx::query(
            "INSERT INTO channel_models (channel_id, model, upstream_model) VALUES ($1, $2, $2) \
             ON CONFLICT (channel_id, model) DO NOTHING",
        )
        .bind(id)
        .bind(model)
        .execute(&mut *tx)
        .await?;
        added += result.rows_affected();
    }
    sqlx::query("UPDATE channels SET health = 'healthy', last_health_at = now(), last_error = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.sync_models",
        "channel",
        &id.to_string(),
        json!({ "name": channel.name, "added": added, "total": models.len() }),
    )
    .await;
    Ok(Json(json!({
        "ok": true,
        "added": added,
        "total": models.len(),
        "models": models,
    })))
}

async fn list_channel_models(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let channel_tiers: Vec<String> = sqlx::query_scalar(
        "SELECT tier FROM channel_tiers WHERE channel_id = $1 ORDER BY tier",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    let models: Vec<ChannelModel> =
        sqlx::query_as("SELECT * FROM channel_models WHERE channel_id = $1 ORDER BY model")
            .bind(id)
            .fetch_all(&state.pool)
            .await?;
    let mut rows = Vec::new();
    for model in models {
        let tiers: Vec<String> = sqlx::query_scalar(
            "SELECT tier FROM channel_model_tiers WHERE channel_model_id = $1 ORDER BY tier",
        )
        .bind(model.id)
        .fetch_all(&state.pool)
        .await?;
        let effective = if !tiers.is_empty() {
            tiers.clone()
        } else {
            channel_tiers.clone()
        };
        rows.push(json!({
            "id": model.id,
            "model": model.model,
            "upstream_model": model.upstream_model,
            "enabled": model.enabled,
            "input_micro": model.input_micro,
            "output_micro": model.output_micro,
            "tiers": tiers,
            "effective_tiers": effective,
            "last_test_at": model.last_test_at,
            "last_test_ok": model.last_test_ok,
            "last_test_ms": model.last_test_ms,
            "last_test_error": model.last_test_error,
        }));
    }
    Ok(Json(json!({ "models": rows, "channel_tiers": channel_tiers })))
}

#[derive(Deserialize)]
struct ModelRequest {
    model: String,
    upstream_model: Option<String>,
    input_micro: Option<i64>,
    output_micro: Option<i64>,
    enabled: Option<bool>,
    tiers: Option<Vec<String>>,
}

async fn add_model(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<ModelRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM channels WHERE id = $1)")
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    if !exists {
        return Err(AppError::not_found("渠道不存在"));
    }
    let model = body.model.trim();
    if model.is_empty() || model.chars().count() > 200 {
        return Err(AppError::bad_request("模型名称不能为空且不能超过 200 字"));
    }
    let input = body.input_micro.filter(|v| *v >= 0);
    let output = body.output_micro.filter(|v| *v >= 0);
    let tiers = checked_tiers(&state.pool, body.tiers.as_deref().unwrap_or(&[])).await?;
    let upstream = body
        .upstream_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    let mut tx = state.pool.begin().await?;
    let model_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO channel_models (channel_id, model, upstream_model, enabled, input_micro, output_micro) \
         VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (channel_id, model) DO NOTHING RETURNING id",
    )
    .bind(id)
    .bind(model)
    .bind(&upstream)
    .bind(body.enabled.unwrap_or(true))
    .bind(input)
    .bind(output)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(model_id) = model_id else {
        return Err(AppError::conflict("该渠道下已存在这个模型"));
    };
    for tier in &tiers {
        sqlx::query("INSERT INTO channel_model_tiers (channel_model_id, tier) VALUES ($1, $2)")
            .bind(model_id)
            .bind(tier)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel_model.create",
        "channel_model",
        &model_id.to_string(),
        json!({ "channel_id": id, "model": model, "tiers": tiers }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": model_id })))
}

#[derive(Deserialize)]
struct UpdateModelRequest {
    upstream_model: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    input_micro: Option<Option<i64>>,
    #[serde(default, deserialize_with = "deserialize_optional_i64")]
    output_micro: Option<Option<i64>>,
    enabled: Option<bool>,
    tiers: Option<Vec<String>>,
}

fn deserialize_optional_i64<'de, D>(deserializer: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<i64>::deserialize(deserializer).map(Some)
}

async fn update_model(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<UpdateModelRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let model: ChannelModel = sqlx::query_as("SELECT * FROM channel_models WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("模型不存在"))?;
    let input = match body.input_micro {
        Some(Some(value)) if value >= 0 => Some(value),
        Some(Some(_)) => return Err(AppError::bad_request("输入价格不能为负数")),
        Some(None) => None,
        None => model.input_micro,
    };
    let output = match body.output_micro {
        Some(Some(value)) if value >= 0 => Some(value),
        Some(Some(_)) => return Err(AppError::bad_request("输出价格不能为负数")),
        Some(None) => None,
        None => model.output_micro,
    };
    let upstream = match body.upstream_model {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        None => model.upstream_model.clone(),
    };
    let tiers = match body.tiers {
        Some(tiers) => Some(checked_tiers(&state.pool, &tiers).await?),
        None => None,
    };
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE channel_models SET upstream_model = $2, input_micro = $3, output_micro = $4, \
         enabled = $5 WHERE id = $1",
    )
    .bind(id)
    .bind(&upstream)
    .bind(input)
    .bind(output)
    .bind(body.enabled.unwrap_or(model.enabled))
    .execute(&mut *tx)
    .await?;
    if let Some(tiers) = &tiers {
        sqlx::query("DELETE FROM channel_model_tiers WHERE channel_model_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for tier in tiers {
            sqlx::query("INSERT INTO channel_model_tiers (channel_model_id, tier) VALUES ($1, $2)")
                .bind(id)
                .bind(tier)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel_model.update",
        "channel_model",
        &id.to_string(),
        json!({ "model": model.model, "tiers": tiers, "enabled": body.enabled }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

async fn delete_model(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let model: Option<ChannelModel> = sqlx::query_as("SELECT * FROM channel_models WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(model) = model else {
        return Err(AppError::not_found("模型不存在"));
    };
    sqlx::query("DELETE FROM channel_models WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel_model.delete",
        "channel_model",
        &id.to_string(),
        json!({ "model": model.model, "channel_id": model.channel_id }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
struct TiersRequest {
    #[serde(default)]
    tiers: Vec<String>,
}

async fn set_channel_tiers(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<TiersRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let tiers = checked_tiers(&state.pool, &body.tiers).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM channel_tiers WHERE channel_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for tier in &tiers {
        sqlx::query("INSERT INTO channel_tiers (channel_id, tier) VALUES ($1, $2)")
            .bind(id)
            .bind(tier)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.set_tiers",
        "channel",
        &id.to_string(),
        json!({ "tiers": tiers }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "tiers": tiers })))
}

async fn set_model_tiers(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
    AppJson(body): AppJson<TiersRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let tiers = checked_tiers(&state.pool, &body.tiers).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM channel_model_tiers WHERE channel_model_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for tier in &tiers {
        sqlx::query("INSERT INTO channel_model_tiers (channel_model_id, tier) VALUES ($1, $2)")
            .bind(id)
            .bind(tier)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel_model.set_tiers",
        "channel_model",
        &id.to_string(),
        json!({ "tiers": tiers }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "tiers": tiers })))
}

#[derive(Deserialize)]
struct BulkTiersRequest {
    channel_ids: Vec<i64>,
    #[serde(default)]
    tiers: Vec<String>,
}

async fn bulk_tiers(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<BulkTiersRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    if body.channel_ids.is_empty() {
        return Err(AppError::bad_request("请选择至少一个渠道"));
    }
    let tiers = checked_tiers(&state.pool, &body.tiers).await?;
    let mut tx = state.pool.begin().await?;
    for channel_id in &body.channel_ids {
        sqlx::query("DELETE FROM channel_tiers WHERE channel_id = $1")
            .bind(channel_id)
            .execute(&mut *tx)
            .await?;
        for tier in &tiers {
            sqlx::query("INSERT INTO channel_tiers (channel_id, tier) VALUES ($1, $2)")
                .bind(channel_id)
                .bind(tier)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "channel.bulk_tiers",
        "channel",
        &body.channel_ids.len().to_string(),
        json!({ "channel_ids": body.channel_ids, "tiers": tiers }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "updated": body.channel_ids.len(), "tiers": tiers })))
}

#[derive(Deserialize)]
struct LogsQuery {
    user_id: Option<i64>,
    model: Option<String>,
    status: Option<i64>,
    page: Option<i64>,
}

async fn list_logs(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<LogsQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let model = query.model.unwrap_or_default().trim().to_string();
    let page = query.page.unwrap_or(1).max(1);
    let limit = 30i64;
    let offset = (page - 1) * limit;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM usage_logs l \
         WHERE ($1::bigint IS NULL OR l.user_id = $1) \
           AND ($2 = '' OR l.model ILIKE '%' || $2 || '%') \
           AND ($3::int IS NULL OR l.status = $3::int)",
    )
    .bind(query.user_id)
    .bind(&model)
    .bind(query.status)
    .fetch_one(&state.pool)
    .await?;
    let logs: Vec<UsageLog> = sqlx::query_as(
        "SELECT l.id, l.request_id, l.model, l.prompt_tokens, l.completion_tokens, \
                l.total_tokens, l.cost_micro, l.status, l.error_code, l.latency_ms, \
                l.is_stream, l.created_at, c.name AS channel_name, k.key_prefix, \
                COALESCE(u.display_name, u.username) AS user_name \
         FROM usage_logs l \
         LEFT JOIN channels c ON c.id = l.channel_id \
         LEFT JOIN api_keys k ON k.id = l.key_id \
         LEFT JOIN users u ON u.id = l.user_id \
         WHERE ($1::bigint IS NULL OR l.user_id = $1) \
           AND ($2 = '' OR l.model ILIKE '%' || $2 || '%') \
           AND ($3::int IS NULL OR l.status = $3::int) \
         ORDER BY l.id DESC LIMIT $4 OFFSET $5",
    )
    .bind(query.user_id)
    .bind(&model)
    .bind(query.status)
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
struct AuditRow {
    id: i64,
    action: String,
    target_type: String,
    target_id: String,
    detail: Option<Value>,
    created_at: DateTime<Utc>,
    actor_name: Option<String>,
}

async fn list_audit(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let rows: Vec<AuditRow> = sqlx::query_as(
        "SELECT a.id, a.action, a.target_type, a.target_id, a.detail, a.created_at, \
                COALESCE(u.display_name, u.username) AS actor_name \
         FROM audit_logs a LEFT JOIN users u ON u.id = a.actor_id \
         ORDER BY a.id DESC LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "audit": rows })))
}

async fn get_settings(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let registration_open: bool = sqlx::query_scalar(
        "SELECT COALESCE((SELECT (value #>> '{}')::boolean FROM settings WHERE key = 'registration_open'), TRUE)",
    )
    .fetch_one(&state.pool)
    .await?;
    let auto = channels::auto_test_settings(&state.pool).await;
    let plans = plans::load_all_plans(&state.pool).await?;
    Ok(Json(json!({
        "registration_open": registration_open,
        "channel_auto_test_minutes": auto.interval_minutes,
        "channel_auto_disable": auto.auto_disable,
        "base_url": state.cfg.base_url,
        "callback_url": state.cfg.redirect_uri(),
        "plans": plans,
    })))
}

#[derive(Deserialize)]
struct UpdateSettingsRequest {
    registration_open: Option<bool>,
    channel_auto_test_minutes: Option<i64>,
    channel_auto_disable: Option<bool>,
}

async fn update_settings(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<UpdateSettingsRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    if let Some(minutes) = body.channel_auto_test_minutes {
        if !(0..=10080).contains(&minutes) {
            return Err(AppError::bad_request("自动检测间隔需在 0–10080 分钟之间（0 表示关闭）"));
        }
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('channel_auto_test_minutes', to_jsonb($1::bigint), now()) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(minutes)
        .execute(&state.pool)
        .await?;
        write_audit(&state.pool, Some(actor.id), "settings.channel_auto_test", "settings",
            "channel_auto_test_minutes", json!({ "value": minutes })).await;
    }
    if let Some(flag) = body.channel_auto_disable {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('channel_auto_disable', to_jsonb($1::boolean), now()) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(flag)
        .execute(&state.pool)
        .await?;
        write_audit(&state.pool, Some(actor.id), "settings.channel_auto_disable", "settings",
            "channel_auto_disable", json!({ "value": flag })).await;
    }
    if let Some(open) = body.registration_open {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('registration_open', to_jsonb($1::boolean), now()) \
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(open)
        .execute(&state.pool)
        .await?;
        write_audit(
        &state.pool,
        Some(actor.id),
            "settings.registration",
            "settings",
            "registration_open",
            json!({ "value": open }),
        )
        .await;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Serialize, FromRow)]
struct PriceRow {
    model: String,
    input_micro: i64,
    output_micro: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PriceQuery {
    q: Option<String>,
}

async fn list_prices(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<PriceQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let q = query.q.unwrap_or_default().trim().to_string();
    let rows: Vec<PriceRow> = sqlx::query_as(
        "SELECT model, input_micro, output_micro, updated_at FROM model_prices \
         WHERE ($1 = '' OR model ILIKE '%' || $1 || '%') ORDER BY model LIMIT 500",
    )
    .bind(&q)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({ "prices": rows })))
}

#[derive(Deserialize)]
struct PriceRequest {
    model: Option<String>,
    input_micro: i64,
    output_micro: i64,
}

async fn create_price(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<PriceRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let model = body
        .model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("模型名称不能为空"))?;
    save_price(&state, actor.id, model, body.input_micro, body.output_micro).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn update_price(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(model): Path<String>,
    AppJson(body): AppJson<PriceRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    save_price(&state, actor.id, &model, body.input_micro, body.output_micro).await?;
    Ok(Json(json!({ "ok": true })))
}

async fn save_price(
    state: &AppState,
    actor_id: i64,
    model: &str,
    input_micro: i64,
    output_micro: i64,
) -> AppResult<()> {
    if input_micro < 0 || output_micro < 0 {
        return Err(AppError::bad_request("价格不能为负数"));
    }
    if input_micro > 1_000_000_000_000 || output_micro > 1_000_000_000_000 {
        return Err(AppError::bad_request("价格数值过大"));
    }
    sqlx::query(
        "INSERT INTO model_prices (model, input_micro, output_micro, updated_at) \
         VALUES ($1, $2, $3, now()) \
         ON CONFLICT (model) DO UPDATE SET input_micro = EXCLUDED.input_micro, \
         output_micro = EXCLUDED.output_micro, updated_at = now()",
    )
    .bind(model)
    .bind(input_micro)
    .bind(output_micro)
    .execute(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor_id),
        "price.save",
        "model_price",
        model,
        json!({ "input_micro": input_micro, "output_micro": output_micro }),
    )
    .await;
    Ok(())
}

async fn delete_price(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(model): Path<String>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    sqlx::query("DELETE FROM model_prices WHERE model = $1")
        .bind(&model)
        .execute(&state.pool)
        .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "price.delete",
        "model_price",
        &model,
        json!({}),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

/// 免费模型日上限的管理员单独设置（2026-11-21，Wing 定：默认值 30M -> 1B，
/// 并允许管理员给指定 Key 或指定用户单独调高上限）。
/// 单独设置存在 free_token_cap_overrides 表里，重启不丢；删除即恢复默认值。
/// 生效优先级：Key 单独设置 > 用户单独设置 > 默认值/环境变量 DIST_FREE_TOKEN_CAP。
#[derive(Serialize, FromRow)]
struct FreeTokenCapOverrideRow {
    id: i64,
    key_id: Option<i64>,
    user_id: Option<i64>,
    cap_tokens: i64,
    note: String,
    created_by: Option<i64>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    key_prefix: Option<String>,
    username: Option<String>,
}

/// GET /api/admin/free-token-caps：列出全部单独设置 + 当前默认值（1B 或环境变量）。
async fn list_free_token_caps(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let overrides: Vec<FreeTokenCapOverrideRow> = sqlx::query_as(
        "SELECT o.id, o.key_id, o.user_id, o.cap_tokens, o.note, o.created_by, \
                o.created_at, o.updated_at, k.key_prefix, u.username \
         FROM free_token_cap_overrides o \
         LEFT JOIN api_keys k ON k.id = o.key_id \
         LEFT JOIN users u ON u.id = o.user_id \
         ORDER BY o.updated_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(json!({
        "default_cap": state.dist_guard.free_token_cap,
        "overrides": overrides,
    })))
}

#[derive(Deserialize)]
struct SetFreeTokenCapRequest {
    key_id: Option<i64>,
    user_id: Option<i64>,
    cap_tokens: i64,
    note: Option<String>,
}

/// POST /api/admin/free-token-caps：给指定 Key 或指定用户设单独上限。
/// body {"key_id": N} 或 {"user_id": N} 二选一 + {"cap_tokens": 正整数, "note": "可选备注"}。
/// 已有设置则覆盖（不报错）；目标不存在返回 404。记审计 free_token_cap.set。
/// 注意：单独设置可高于默认值，也可低于默认值（例如临时压测或处置滥用）。
async fn set_free_token_cap(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<SetFreeTokenCapRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let (target_type, target_id) = match (body.key_id, body.user_id) {
        (Some(key_id), None) => ("api_key", key_id),
        (None, Some(user_id)) => ("user", user_id),
        _ => {
            return Err(AppError::bad_request("key_id 与 user_id 二选一，只能传一个"));
        }
    };
    if body.cap_tokens <= 0 {
        return Err(AppError::bad_request("cap_tokens 必须是大于 0 的整数"));
    }
    let exists: bool = if target_type == "api_key" {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM api_keys WHERE id = $1)")
            .bind(target_id)
            .fetch_one(&state.pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE id = $1)")
            .bind(target_id)
            .fetch_one(&state.pool)
            .await?
    };
    if !exists {
        return Err(AppError::not_found(if target_type == "api_key" {
            "API Key 不存在"
        } else {
            "用户不存在"
        }));
    }
    let note = body
        .note
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| truncate_chars(s, 200))
        .unwrap_or_default();
    let (key_id, user_id): (Option<i64>, Option<i64>) = if target_type == "api_key" {
        (Some(target_id), None)
    } else {
        (None, Some(target_id))
    };
    let previous: Option<i64> = sqlx::query_scalar(
        "SELECT cap_tokens FROM free_token_cap_overrides \
         WHERE key_id IS NOT DISTINCT FROM $1 AND user_id IS NOT DISTINCT FROM $2",
    )
    .bind(key_id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?;
    let updated = sqlx::query(
        "UPDATE free_token_cap_overrides SET cap_tokens = $3, note = $4, \
                created_by = $5, updated_at = now() \
         WHERE key_id IS NOT DISTINCT FROM $1 AND user_id IS NOT DISTINCT FROM $2",
    )
    .bind(key_id)
    .bind(user_id)
    .bind(body.cap_tokens)
    .bind(&note)
    .bind(actor.id)
    .execute(&state.pool)
    .await?;
    let id: i64 = if updated.rows_affected() == 0 {
        sqlx::query_scalar(
            "INSERT INTO free_token_cap_overrides (key_id, user_id, cap_tokens, note, created_by) \
             VALUES ($1, $2, $3, $4, $5) RETURNING id",
        )
        .bind(key_id)
        .bind(user_id)
        .bind(body.cap_tokens)
        .bind(&note)
        .bind(actor.id)
        .fetch_one(&state.pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT id FROM free_token_cap_overrides \
             WHERE key_id IS NOT DISTINCT FROM $1 AND user_id IS NOT DISTINCT FROM $2",
        )
        .bind(key_id)
        .bind(user_id)
        .fetch_one(&state.pool)
        .await?
    };
    write_audit(
        &state.pool,
        Some(actor.id),
        "free_token_cap.set",
        target_type,
        &target_id.to_string(),
        json!({ "cap_tokens": body.cap_tokens, "previous_cap": previous, "note": note }),
    )
    .await;
    Ok(Json(json!({
        "ok": true,
        "id": id,
        "target_type": target_type,
        "target_id": target_id,
        "cap_tokens": body.cap_tokens,
        "previous_cap": previous,
    })))
}

/// DELETE /api/admin/free-token-caps/{id}：删除一条单独设置，该 Key/用户恢复默认值。
/// 不存在返回 404。记审计 free_token_cap.delete。
async fn delete_free_token_cap(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let row: Option<FreeTokenCapOverrideRow> = sqlx::query_as(
        "SELECT o.id, o.key_id, o.user_id, o.cap_tokens, o.note, o.created_by, \
                o.created_at, o.updated_at, k.key_prefix, u.username \
         FROM free_token_cap_overrides o \
         LEFT JOIN api_keys k ON k.id = o.key_id \
         LEFT JOIN users u ON u.id = o.user_id \
         WHERE o.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("该上限设置不存在"))?;
    sqlx::query("DELETE FROM free_token_cap_overrides WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    let (target_type, target_id) = match (row.key_id, row.user_id) {
        (Some(key_id), None) => ("api_key", key_id),
        (None, Some(user_id)) => ("user", user_id),
        _ => ("unknown", 0),
    };
    write_audit(
        &state.pool,
        Some(actor.id),
        "free_token_cap.delete",
        target_type,
        &target_id.to_string(),
        json!({ "cap_tokens": row.cap_tokens, "note": row.note }),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// 套餐管理（2026-11-21 自定义套餐）：管理员添加/删除套餐、改描述。
// 内置档不可删除、不可改档位与额度（仅可改描述，存在 plan_descriptions）；
// 自定义套餐可改描述/名称/额度/排序，删除受 delete_block_reason 保护。
// 自定义套餐不参与签到自动升级，只能手动授予（见 plans.rs 模块注释）。
// ---------------------------------------------------------------------------

/// GET /api/admin/plans：内置档 + 自定义套餐合并列表。
async fn list_plans(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let plans = plans::load_all_plans(&state.pool).await?;
    Ok(Json(json!({ "plans": plans })))
}

#[derive(Deserialize)]
struct CreatePlanRequest {
    code: String,
    name: String,
    /// 描述；JSON 字段名保持 tagline（与 /api/plans 一致），后台 UI 里叫“描述”。
    tagline: String,
    /// 各窗口额度（美元，空 = 不限；库里存 micro-USD）。
    #[serde(default)]
    h5_usd: Option<f64>,
    #[serde(default)]
    weekly_usd: Option<f64>,
    #[serde(default)]
    monthly_usd: Option<f64>,
    /// 仅作展示与将来扩展用；自定义套餐不参与签到自动升级。
    #[serde(default)]
    checkin_days: Option<i32>,
    #[serde(default)]
    requires_approval: bool,
    #[serde(default)]
    sort_order: i32,
}

fn usd_limit(value: Option<f64>, label: &str) -> AppResult<Option<i64>> {
    match value {
        None => Ok(None),
        Some(v) => plans::dollars_to_micro(v)
            .map(Some)
            .ok_or_else(|| AppError::bad_request(format!("{label}额度需在 0–1,000,000 美元之间"))),
    }
}

fn checked_checkin_days(value: Option<i32>) -> AppResult<Option<i32>> {
    match value {
        None => Ok(None),
        Some(days) if (1..=365).contains(&days) => Ok(Some(days)),
        Some(_) => Err(AppError::bad_request("连续签到天数需在 1–365 之间")),
    }
}

/// POST /api/admin/plans：创建自定义套餐。body {code, name, tagline,
/// h5_usd?, weekly_usd?, monthly_usd?, checkin_days?, requires_approval?,
/// sort_order?}；code 不可与内置档重名、不可重复。记审计 plan.create。
async fn create_plan(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<CreatePlanRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let code = plans::validate_code(&body.code).map_err(AppError::bad_request)?;
    if plans::is_builtin(&code) {
        return Err(AppError::conflict("该 code 是内置套餐，不能创建同名自定义套餐"));
    }
    let name = plans::validate_name(&body.name).map_err(AppError::bad_request)?;
    let tagline = plans::validate_tagline(&body.tagline).map_err(AppError::bad_request)?;
    let h5 = usd_limit(body.h5_usd, "5 小时")?;
    let weekly = usd_limit(body.weekly_usd, "7 天")?;
    let monthly = usd_limit(body.monthly_usd, "30 天")?;
    let checkin_days = checked_checkin_days(body.checkin_days)?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM custom_plans WHERE code = $1)")
        .bind(&code)
        .fetch_one(&state.pool)
        .await?;
    if exists {
        return Err(AppError::conflict("该套餐 code 已存在"));
    }
    sqlx::query(
        "INSERT INTO custom_plans (code, name, tagline, h5_micro, weekly_micro, monthly_micro, \
         checkin_days, requires_approval, sort_order) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(&code)
    .bind(&name)
    .bind(&tagline)
    .bind(h5)
    .bind(weekly)
    .bind(monthly)
    .bind(checkin_days)
    .bind(body.requires_approval)
    .bind(body.sort_order.clamp(-1000, 1000))
    .execute(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "plan.create",
        "plan",
        &code,
        json!({ "name": name, "tagline": tagline }),
    )
    .await;
    let plan = plans::load_plan(&state.pool, &code).await?;
    Ok(Json(json!({ "ok": true, "plan": plan })))
}

#[derive(Deserialize)]
struct UpdatePlanRequest {
    #[serde(default)]
    name: Option<String>,
    /// 描述；内置档与自定义套餐都可改。
    #[serde(default)]
    tagline: Option<String>,
    /// 三档额度：字段缺失 = 不改；显式 null = 改为不限；数字 = 改为该美元额度。
    #[serde(default)]
    h5_usd: Option<Option<f64>>,
    #[serde(default)]
    weekly_usd: Option<Option<f64>>,
    #[serde(default)]
    monthly_usd: Option<Option<f64>>,
    #[serde(default)]
    checkin_days: Option<Option<i32>>,
    #[serde(default)]
    requires_approval: Option<bool>,
    #[serde(default)]
    sort_order: Option<i32>,
}

/// PATCH /api/admin/plans/{code}：改描述（内置档与自定义套餐都可改）；
/// 自定义套餐还可改名称、额度、签到天数、审批与排序。内置档的其它字段
/// 一律拒绝。记审计 plan.update。
async fn update_plan(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(code): Path<String>,
    AppJson(body): AppJson<UpdatePlanRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let code = code.trim().to_ascii_lowercase();
    if plans::is_builtin(&code) {
        if body.name.is_some()
            || body.h5_usd.is_some()
            || body.weekly_usd.is_some()
            || body.monthly_usd.is_some()
            || body.checkin_days.is_some()
            || body.requires_approval.is_some()
            || body.sort_order.is_some()
        {
            return Err(AppError::bad_request("内置套餐仅可修改描述，档位与额度不可改"));
        }
        let tagline = body
            .tagline
            .as_deref()
            .map(plans::validate_tagline)
            .transpose()
            .map_err(AppError::bad_request)?
            .ok_or_else(|| AppError::bad_request("请填写新的描述"))?;
        sqlx::query(
            "INSERT INTO plan_descriptions (code, tagline, updated_at) VALUES ($1, $2, now()) \
             ON CONFLICT (code) DO UPDATE SET tagline = EXCLUDED.tagline, updated_at = now()",
        )
        .bind(&code)
        .bind(&tagline)
        .execute(&state.pool)
        .await?;
        write_audit(
            &state.pool,
            Some(actor.id),
            "plan.update",
            "plan",
            &code,
            json!({ "tagline": tagline }),
        )
        .await;
        let plan = plans::load_plan(&state.pool, &code).await?;
        return Ok(Json(json!({ "ok": true, "plan": plan })));
    }

    let row: Option<plans::CustomPlanRow> = sqlx::query_as(
        "SELECT code, name, tagline, h5_micro, weekly_micro, monthly_micro, \
                checkin_days, requires_approval, sort_order, created_at \
         FROM custom_plans WHERE code = $1",
    )
    .bind(&code)
    .fetch_optional(&state.pool)
    .await?;
    let row = row.ok_or_else(|| AppError::not_found("套餐不存在"))?;
    let name = match body.name.as_deref() {
        Some(value) => plans::validate_name(value).map_err(AppError::bad_request)?,
        None => row.name.clone(),
    };
    let tagline = match body.tagline.as_deref() {
        Some(value) => plans::validate_tagline(value).map_err(AppError::bad_request)?,
        None => row.tagline.clone(),
    };
    let h5 = match body.h5_usd {
        Some(value) => usd_limit(value, "5 小时")?,
        None => row.h5_micro,
    };
    let weekly = match body.weekly_usd {
        Some(value) => usd_limit(value, "7 天")?,
        None => row.weekly_micro,
    };
    let monthly = match body.monthly_usd {
        Some(value) => usd_limit(value, "30 天")?,
        None => row.monthly_micro,
    };
    let checkin_days = match body.checkin_days {
        Some(value) => checked_checkin_days(value)?,
        None => row.checkin_days,
    };
    let requires_approval = body.requires_approval.unwrap_or(row.requires_approval);
    let sort_order = body.sort_order.unwrap_or(row.sort_order).clamp(-1000, 1000);
    sqlx::query(
        "UPDATE custom_plans SET name = $2, tagline = $3, h5_micro = $4, weekly_micro = $5, \
         monthly_micro = $6, checkin_days = $7, requires_approval = $8, sort_order = $9, \
         updated_at = now() WHERE code = $1",
    )
    .bind(&code)
    .bind(&name)
    .bind(&tagline)
    .bind(h5)
    .bind(weekly)
    .bind(monthly)
    .bind(checkin_days)
    .bind(requires_approval)
    .bind(sort_order)
    .execute(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "plan.update",
        "plan",
        &code,
        json!({ "name": name, "tagline": tagline }),
    )
    .await;
    let plan = plans::load_plan(&state.pool, &code).await?;
    Ok(Json(json!({ "ok": true, "plan": plan })))
}

/// DELETE /api/admin/plans/{code}：删除自定义套餐。内置档、有人持有、
/// 被渠道/模型档位授权引用、有待审批申请的，一律拒绝（见
/// plans::delete_block_reason，不静默改派用户）。记审计 plan.delete。
async fn delete_plan(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(code): Path<String>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let code = code.trim().to_ascii_lowercase();
    let users_holding: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM users WHERE plan = $1 OR earned_plan = $1",
    )
    .bind(&code)
    .fetch_one(&state.pool)
    .await?;
    let channel_tiers: i64 = sqlx::query_scalar("SELECT count(*) FROM channel_tiers WHERE tier = $1")
        .bind(&code)
        .fetch_one(&state.pool)
        .await?;
    let model_tiers: i64 =
        sqlx::query_scalar("SELECT count(*) FROM channel_model_tiers WHERE tier = $1")
            .bind(&code)
            .fetch_one(&state.pool)
            .await?;
    let pending: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM plan_applications WHERE target_plan = $1 AND status = 'pending'",
    )
    .bind(&code)
    .fetch_one(&state.pool)
    .await?;
    if let Some(reason) = plans::delete_block_reason(
        plans::is_builtin(&code),
        users_holding,
        channel_tiers,
        model_tiers,
        pending,
    ) {
        return Err(AppError::conflict(reason));
    }
    let deleted = sqlx::query("DELETE FROM custom_plans WHERE code = $1")
        .bind(&code)
        .execute(&state.pool)
        .await?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::not_found("套餐不存在"));
    }
    write_audit(
        &state.pool,
        Some(actor.id),
        "plan.delete",
        "plan",
        &code,
        json!({}),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn write_audit(
    pool: &PgPool,
    actor_id: Option<i64>,
    action: &str,
    target_type: &str,
    target_id: &str,
    detail: Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(detail)
    .execute(pool)
    .await;
    if let Err(err) = result {
        tracing::warn!(error = %err, "failed to write audit log");
    }
}

/// 升级到 Max 后自动进入 Ultra 候补池（已有待审申请则跳过）。
async fn enqueue_ultra_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO plan_applications (user_id, target_plan, note) \
         SELECT $1, 'ultra', '已升级 Max，自动进入 Ultra 候补池' \
         WHERE NOT EXISTS (SELECT 1 FROM plan_applications WHERE user_id = $1 AND target_plan = 'ultra' AND status = 'pending')",
    )
    .bind(user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod admin_role_tests {
    use super::*;

    fn user_with_role(id: i64, role: &str) -> User {
        User {
            id,
            linuxdo_id: id * 1000,
            username: format!("user{id}"),
            display_name: None,
            avatar_url: None,
            trust_level: 0,
            role: role.to_string(),
            plan: "lite".to_string(),
            earned_plan: "lite".to_string(),
            plan_source: "default".to_string(),
            is_banned: false,
            ban_reason: None,
            consecutive_days: 0,
            longest_streak: 0,
            last_checkin_on: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_login_at: None,
        }
    }

    #[test]
    fn require_root_only_allows_root() {
        assert!(require_root(&user_with_role(10, "root")).is_ok());
        assert!(require_root(&user_with_role(11, "admin")).is_err());
        assert!(require_root(&user_with_role(12, "user")).is_err());
    }

    #[test]
    fn require_admin_allows_root_and_admin() {
        assert!(require_admin(&user_with_role(10, "root")).is_ok());
        assert!(require_admin(&user_with_role(11, "admin")).is_ok());
        assert!(require_admin(&user_with_role(12, "user")).is_err());
    }

    #[test]
    fn promote_rejects_root_and_existing_admin() {
        assert!(check_promote_target(&user_with_role(10, "root")).is_err());
        assert!(check_promote_target(&user_with_role(11, "admin")).is_err());
        assert!(check_promote_target(&user_with_role(12, "user")).is_ok());
    }

    #[test]
    fn demote_requires_admin_target() {
        assert!(check_demote_target(&user_with_role(10, "root")).is_err());
        assert!(check_demote_target(&user_with_role(11, "admin")).is_ok());
        assert!(check_demote_target(&user_with_role(12, "user")).is_err());
    }
}
