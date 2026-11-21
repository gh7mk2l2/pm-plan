//! 补签卡与重置卡。
//!
//! 补签卡：补最近 7 天内漏签的任意一天；补完后按签到记录重算连续天数、
//! 累计天数与升档；卡自发放起 30 天过期；每人同时只能持有一张未使用的卡。
//! 重置卡：站长生成兑换码，用户凭码领取后使用，清零 5h / 7d / 30d 已用额度
//! （用量按 usage_logs 滚动窗口统计，重置后只统计重置时间点之后的用量）。

use crate::auth::{require_admin, AuthUser};
use crate::error::{AppError, AppJson, AppResult};
use crate::models::User;
use crate::plans;
use crate::state::AppState;
use crate::usage;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::{DateTime, NaiveDate, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use std::collections::BTreeSet;

/// 可补签的回溯天数（不含今天）。
pub const MAKEUP_WINDOW_DAYS: i64 = 7;

/// 签到写库后的统一收尾结果。
#[derive(Debug)]
pub struct CheckinOutcome {
    pub streak: i32,
    pub total: i32,
    pub plan: String,
    pub earned_plan: String,
    pub upgraded_to: Option<String>,
    pub candidate_created: Option<String>,
}

/// 以 `end` 结尾的连续签到天数（`end` 当天计入）。
fn streak_ending_at(dates: &BTreeSet<NaiveDate>, end: NaiveDate) -> i32 {
    let mut count = 0i32;
    let mut day = end;
    loop {
        if !dates.contains(&day) {
            break;
        }
        count += 1;
        match day.pred_opt() {
            Some(prev) => day = prev,
            None => break,
        }
    }
    count
}

/// 每一天截至当日的连续天数（按日期升序）。与 SQL 修复用的 islands
/// 算法一致：连续日期递增，断开后重新从 1 开始。
fn streaks_for_dates(dates: &BTreeSet<NaiveDate>) -> Vec<(NaiveDate, i32)> {
    let mut out = Vec::with_capacity(dates.len());
    let mut prev: Option<NaiveDate> = None;
    let mut run = 0i32;
    for date in dates {
        match prev {
            Some(p) if p.succ_opt() == Some(*date) => run += 1,
            _ => run = 1,
        }
        out.push((*date, run));
        prev = Some(*date);
    }
    out
}

/// 全历史最长连续签到天数（最长的连续区间）。
fn longest_run(dates: &BTreeSet<NaiveDate>) -> i32 {
    streaks_for_dates(dates)
        .iter()
        .map(|(_, streak)| *streak)
        .max()
        .unwrap_or(0)
}

/// 在事务内写入一条签到记录并重算用户状态。调用方需已锁定用户行，
/// 且保证该日期尚未签到。`source` 写入 plan_events / plan_source，
/// 正常签到为 "checkin"，补签为 "makeup"。
pub(crate) async fn record_checkin(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user: &User,
    date: NaiveDate,
    source: &str,
) -> Result<CheckinOutcome, sqlx::Error> {
    let existing: Vec<NaiveDate> = sqlx::query_scalar(
        "SELECT checkin_on FROM checkins WHERE user_id = $1 ORDER BY checkin_on",
    )
    .bind(user.id)
    .fetch_all(&mut **tx)
    .await?;
    let mut dates: BTreeSet<NaiveDate> = existing.into_iter().collect();
    let day_before = date.pred_opt().unwrap_or(date);
    let insert_streak = streak_ending_at(&dates, day_before) + 1;
    sqlx::query("INSERT INTO checkins (user_id, checkin_on, streak) VALUES ($1, $2, $3)")
        .bind(user.id)
        .bind(date)
        .bind(insert_streak)
        .execute(&mut **tx)
        .await?;
    dates.insert(date);
    // 回补过去某天时，它后面已有的签到行的 streak 会变 stale（之前只算了
    // 新插这行的 streak）。按全量日期重算该用户从这天起所有行的 streak，
    // 只写变了的行；正常签当天时后面没有行，这一步是空操作。
    for (day, correct) in streaks_for_dates(&dates) {
        if day < date {
            continue;
        }
        sqlx::query(
            "UPDATE checkins SET streak = $3 \
             WHERE user_id = $1 AND checkin_on = $2 AND streak <> $3",
        )
        .bind(user.id)
        .bind(day)
        .bind(correct)
        .execute(&mut **tx)
        .await?;
    }
    let total = dates.len() as i32;
    let last = *dates.iter().next_back().unwrap_or(&date);
    let streak = streak_ending_at(&dates, last);
    // 最长连续按全量日期重算：回补可能让中间某段变长，而它不一定是
    // 截至最后一天的连续段，用旧的 max(缓存, 截至最后一天) 会漏算。
    let longest = longest_run(&dates);

    let auto = plans::auto_earned_plan(streak, total);
    let mut earned = user.earned_plan.clone();
    let mut plan = user.plan.clone();
    let mut upgraded_to: Option<String> = None;
    if plans::rank(auto) > plans::rank(&earned) {
        earned = auto.to_string();
    }
    if plans::rank(auto) > plans::rank(&plan) {
        plan = auto.to_string();
        upgraded_to = Some(auto.to_string());
    }

    let updated: User = sqlx::query_as(
        "UPDATE users SET consecutive_days = $2, longest_streak = $3, last_checkin_on = $4, \
         earned_plan = $5, plan = $6, \
         plan_source = CASE WHEN $7 THEN $8 ELSE plan_source END, updated_at = now() \
         WHERE id = $1 RETURNING *",
    )
    .bind(user.id)
    .bind(streak)
    .bind(longest)
    .bind(last)
    .bind(&earned)
    .bind(&plan)
    .bind(upgraded_to.is_some())
    .bind(source)
    .fetch_one(&mut **tx)
    .await?;

    if upgraded_to.is_some() {
        sqlx::query(
            "INSERT INTO plan_events (user_id, from_plan, to_plan, source, detail) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user.id)
        .bind(&user.plan)
        .bind(&plan)
        .bind(source)
        .bind(format!("连续签到 {} 天自动升级", streak))
        .execute(&mut **tx)
        .await?;
    }

    let mut candidate_created: Option<String> = None;
    if let Some(target) = plans::candidate_for_streak(streak) {
        if plans::rank(target) > plans::rank(&plan) {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM plan_applications WHERE user_id = $1 AND target_plan = $2)",
            )
            .bind(user.id)
            .bind(target)
            .fetch_one(&mut **tx)
            .await?;
            if !exists {
                sqlx::query(
                    "INSERT INTO plan_applications (user_id, target_plan, note) \
                     VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                )
                .bind(user.id)
                .bind(target)
                .bind(format!("连续签到 {} 天，自动进入候补池", streak))
                .execute(&mut **tx)
                .await?;
                candidate_created = Some(target.to_string());
            }
        }
    }

    Ok(CheckinOutcome {
        streak,
        total,
        plan: updated.plan,
        earned_plan: updated.earned_plan,
        upgraded_to,
        candidate_created,
    })
}

async fn write_audit(
    pool: &sqlx::PgPool,
    actor: Option<i64>,
    action: &str,
    target_type: &str,
    target_id: &str,
    detail: Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor)
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

/// 把已过期的未使用补签卡标记为 expired。`user_id` 为空时处理全部用户。
async fn expire_stale_makeup_cards(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Option<i64>,
) -> Result<(), sqlx::Error> {
    if let Some(uid) = user_id {
        sqlx::query(
            "UPDATE makeup_cards SET status = 'expired' \
             WHERE user_id = $1 AND status = 'issued' AND expires_at <= now()",
        )
        .bind(uid)
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            "UPDATE makeup_cards SET status = 'expired' \
             WHERE status = 'issued' AND expires_at <= now()",
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MakeupCardView {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub used_for_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ResetCodeView {
    pub id: i64,
    pub code_prefix: String,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub used_at: Option<DateTime<Utc>>,
}

/// 用户自己的全部卡片（补签卡 + 已领取的重置卡）。
pub(crate) async fn user_cards(
    pool: &sqlx::PgPool,
    user_id: i64,
) -> Result<(Vec<MakeupCardView>, Vec<ResetCodeView>), sqlx::Error> {
    let makeup = sqlx::query_as::<_, MakeupCardView>(
        "SELECT id, kind, status, issued_at, expires_at, used_at, used_for_date FROM makeup_cards \
         WHERE user_id = $1 ORDER BY id DESC LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let resets = sqlx::query_as::<_, ResetCodeView>(
        "SELECT id, code_prefix, status, expires_at, redeemed_at, used_at FROM reset_codes \
         WHERE user_id = $1 ORDER BY id DESC LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok((makeup, resets))
}

pub(crate) async fn list_cards(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let (makeup_cards, reset_cards) = user_cards(&state.pool, user.id).await?;
    Ok(Json(json!({ "makeup_cards": makeup_cards, "reset_cards": reset_cards })))
}

#[derive(Deserialize)]
pub struct UseMakeupRequest {
    pub date: NaiveDate,
    /// 滑块验证通过后发的一次性 pass_token。
    #[serde(default)]
    pub captcha_token: Option<String>,
}

pub(crate) async fn use_makeup_card(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
    headers: HeaderMap,
    AppJson(body): AppJson<UseMakeupRequest>,
) -> AppResult<Json<Value>> {
    let remote_ip = crate::util::client_ip(&headers);
    // 人机验证（2026-11-15 换回 hCaptcha + Turnstile 双通道）：按当前通道校验 token。
    crate::captcha::check_checkin_token(&state, body.captcha_token.as_deref(), None, Some(&remote_ip)).await?;
    let today = state.cfg.today();
    let earliest = today - chrono::Duration::days(MAKEUP_WINDOW_DAYS);
    let latest = today.pred_opt().unwrap_or(today);
    if body.date < earliest || body.date > latest {
        return Err(AppError::bad_request(format!(
            "只能补 {} 至 {} 之间漏签的日期",
            earliest.format("%Y-%m-%d"),
            latest.format("%Y-%m-%d")
        )));
    }
    let mut tx = state.pool.begin().await?;
    let current: User = sqlx::query_as("SELECT * FROM users WHERE id = $1 FOR UPDATE")
        .bind(user.id)
        .fetch_one(&mut *tx)
        .await?;
    let card: Option<(String, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT status, expires_at, kind FROM makeup_cards WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, expires_at, kind)) = card else {
        return Err(AppError::not_found("补签卡不存在"));
    };
    if status != "issued" {
        return Err(AppError::conflict("这张补签卡已经使用或失效"));
    }
    if expires_at <= Utc::now() {
        sqlx::query("UPDATE makeup_cards SET status = 'expired' WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Err(AppError::bad_request("这张补签卡已过期"));
    }
    if kind == "plus" {
        // Plus 补签卡：一次补齐窗口内所有漏签日期。
        let existing: Vec<NaiveDate> = sqlx::query_scalar(
            "SELECT checkin_on FROM checkins WHERE user_id = $1 AND checkin_on >= $2 AND checkin_on <= $3",
        )
        .bind(user.id)
        .bind(earliest)
        .bind(latest)
        .fetch_all(&mut *tx)
        .await?;
        let existing_set: BTreeSet<NaiveDate> = existing.into_iter().collect();
        let mut missed: Vec<NaiveDate> = Vec::new();
        let mut day = earliest;
        while day <= latest {
            if !existing_set.contains(&day) {
                missed.push(day);
            }
            match day.succ_opt() {
                Some(next) => day = next,
                None => break,
            }
        }
        if missed.is_empty() {
            return Err(AppError::bad_request("最近 7 天没有漏签，无需补签"));
        }
        let mut current = current;
        let mut outcome: Option<CheckinOutcome> = None;
        for date in &missed {
            let step = record_checkin(&mut tx, &current, *date, "makeup").await?;
            // 每补一天后用最新用户行重算，避免重复写升级事件。
            current = sqlx::query_as("SELECT * FROM users WHERE id = $1")
                .bind(user.id)
                .fetch_one(&mut *tx)
                .await?;
            outcome = Some(step);
        }
        let outcome = outcome.expect("missed 非空，至少补签一天");
        let last_date = *missed.last().expect("missed 非空");
        sqlx::query(
            "UPDATE makeup_cards SET status = 'used', used_at = now(), used_for_date = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(last_date)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
             VALUES ($1, 'card.makeup_use', 'makeup_card', $2, $3)",
        )
        .bind(user.id)
        .bind(id.to_string())
        .bind(json!({ "dates": missed, "streak": outcome.streak, "kind": "plus" }))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        let message = if let Some(code) = &outcome.upgraded_to {
            format!(
                "Plus 补签成功，已补齐 {} 天漏签，已连续签到 {} 天，自动升级到 {} 套餐",
                missed.len(),
                outcome.streak,
                plans::plan(code).name
            )
        } else if let Some(code) = &outcome.candidate_created {
            format!(
                "Plus 补签成功，已补齐 {} 天漏签，已连续签到 {} 天，进入 {} 候补池，等待站长审批",
                missed.len(),
                outcome.streak,
                plans::plan(code).name
            )
        } else {
            format!(
                "Plus 补签成功，已补齐 {} 天漏签，已连续签到 {} 天",
                missed.len(),
                outcome.streak
            )
        };
        return Ok(Json(json!({
            "ok": true,
            "kind": "plus",
            "filled_dates": missed,
            "date": last_date,
            "streak": outcome.streak,
            "total_checkins": outcome.total,
            "plan": outcome.plan,
            "earned_plan": outcome.earned_plan,
            "upgraded_to": outcome.upgraded_to,
            "candidate_created": outcome.candidate_created,
            "message": message,
        })));
    }
    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM checkins WHERE user_id = $1 AND checkin_on = $2)",
    )
    .bind(user.id)
    .bind(body.date)
    .fetch_one(&mut *tx)
    .await?;
    if already {
        return Err(AppError::conflict("这一天已经签到过了"));
    }
    let outcome = record_checkin(&mut tx, &current, body.date, "makeup").await?;
    sqlx::query(
        "UPDATE makeup_cards SET status = 'used', used_at = now(), used_for_date = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(body.date)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO audit_logs (actor_id, action, target_type, target_id, detail) \
         VALUES ($1, 'card.makeup_use', 'makeup_card', $2, $3)",
    )
    .bind(user.id)
    .bind(id.to_string())
    .bind(json!({ "date": body.date, "streak": outcome.streak }))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let message = if let Some(code) = &outcome.upgraded_to {
        format!(
            "补签成功，已连续签到 {} 天，自动升级到 {} 套餐",
            outcome.streak,
            plans::plan(code).name
        )
    } else if let Some(code) = &outcome.candidate_created {
        format!(
            "补签成功，已连续签到 {} 天，进入 {} 候补池，等待站长审批",
            outcome.streak,
            plans::plan(code).name
        )
    } else {
        format!("补签成功，已连续签到 {} 天", outcome.streak)
    };
    Ok(Json(json!({
        "ok": true,
        "date": body.date,
        "streak": outcome.streak,
        "total_checkins": outcome.total,
        "plan": outcome.plan,
        "earned_plan": outcome.earned_plan,
        "upgraded_to": outcome.upgraded_to,
        "candidate_created": outcome.candidate_created,
        "message": message,
    })))
}

#[derive(Deserialize)]
pub struct RedeemRequest {
    pub code: String,
}

pub(crate) async fn redeem_reset_code(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    AppJson(body): AppJson<RedeemRequest>,
) -> AppResult<Json<Value>> {
    let code = body.code.trim().to_uppercase();
    if code.is_empty() {
        return Err(AppError::bad_request("请输入兑换码"));
    }
    let hash = crate::util::sha256(code.as_bytes());
    let mut tx = state.pool.begin().await?;
    let row: Option<(i64, String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT id, status, expires_at FROM reset_codes WHERE code_hash = $1 FOR UPDATE",
    )
    .bind(&hash)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, status, expires_at)) = row else {
        return Err(AppError::not_found("兑换码不存在"));
    };
    if status != "unassigned" {
        return Err(AppError::conflict("该兑换码已被使用"));
    }
    if let Some(exp) = expires_at {
        if exp <= Utc::now() {
            return Err(AppError::bad_request("该兑换码已过期"));
        }
    }
    sqlx::query(
        "UPDATE reset_codes SET status = 'issued', user_id = $1, redeemed_at = now() WHERE id = $2",
    )
    .bind(user.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "id": id })))
}

pub(crate) async fn use_reset_code(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let mut tx = state.pool.begin().await?;
    let row: Option<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT status, expires_at FROM reset_codes WHERE id = $1 AND user_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((status, expires_at)) = row else {
        return Err(AppError::not_found("重置卡不存在"));
    };
    if status != "issued" {
        return Err(AppError::conflict("这张重置卡已经使用或失效"));
    }
    if let Some(exp) = expires_at {
        if exp <= Utc::now() {
            return Err(AppError::bad_request("这张重置卡已过期"));
        }
    }
    sqlx::query("INSERT INTO usage_resets (user_id, reset_code_id) VALUES ($1, $2)")
        .bind(user.id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE reset_codes SET status = 'used', used_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let quota = usage::quota_usage(&state.pool, user.id).await?;
    Ok(Json(json!({
        "ok": true,
        "h5_used": quota.h5,
        "weekly_used": quota.d7,
        "monthly_used": quota.d30,
    })))
}

#[derive(Deserialize)]
pub struct IssueMakeupAllRequest {
    #[serde(default)]
    pub kind: Option<String>,
}

fn normalize_card_kind(kind: Option<&str>) -> Result<&'static str, AppError> {
    match kind {
        None | Some("normal") => Ok("normal"),
        Some("plus") => Ok("plus"),
        Some(_) => Err(AppError::bad_request("未知的卡片类型")),
    }
}

fn makeup_issue_note(kind: &str) -> &'static str {
    if kind == "plus" {
        "站长发放·Plus"
    } else {
        "站长发放"
    }
}

pub(crate) async fn issue_makeup_all(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    body: Bytes,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    // 兼容旧前端（无 body）和新前端（{"kind": "plus"}）。
    let kind = if body.is_empty() {
        "normal"
    } else {
        let req: IssueMakeupAllRequest = serde_json::from_slice(&body)
            .map_err(|_| AppError::bad_request("请求参数错误"))?;
        normalize_card_kind(req.kind.as_deref())?
    };
    let note = makeup_issue_note(kind);
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(20261110)")
        .execute(&mut *tx)
        .await?;
    expire_stale_makeup_cards(&mut tx, None).await?;
    let result = sqlx::query(
        "INSERT INTO makeup_cards (user_id, kind, expires_at, issued_by, note) \
         SELECT id, $2, now() + interval '30 days', $1, $3 FROM users \
         WHERE NOT is_banned \
           AND NOT EXISTS (SELECT 1 FROM makeup_cards \
                            WHERE user_id = users.id AND kind = $2 AND status = 'issued' AND expires_at > now())",
    )
    .bind(actor.id)
    .bind(kind)
    .bind(note)
    .execute(&mut *tx)
    .await?;
    let issued = result.rows_affected();
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.makeup_issue_all",
        "makeup_card",
        "all",
        json!({ "issued": issued, "kind": kind }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "issued": issued })))
}

#[derive(Deserialize)]
pub struct IssueMakeupRequest {
    pub user_id: i64,
    #[serde(default)]
    pub kind: Option<String>,
}

pub(crate) async fn issue_makeup_one(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<IssueMakeupRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(body.user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if target.is_banned {
        return Err(AppError::bad_request("封禁用户不发卡"));
    }
    let kind = normalize_card_kind(body.kind.as_deref())?;
    let mut tx = state.pool.begin().await?;
    expire_stale_makeup_cards(&mut tx, Some(target.id)).await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM makeup_cards \
                        WHERE user_id = $1 AND kind = $2 AND status = 'issued' AND expires_at > now())",
    )
    .bind(target.id)
    .bind(kind)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(AppError::conflict(if kind == "plus" {
            "该用户已持有一张未使用的 Plus 补签卡"
        } else {
            "该用户已持有一张未使用的补签卡"
        }));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO makeup_cards (user_id, kind, expires_at, issued_by, note) \
         VALUES ($1, $2, now() + interval '30 days', $3, $4) RETURNING id",
    )
    .bind(target.id)
    .bind(kind)
    .bind(actor.id)
    .bind(makeup_issue_note(kind))
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.makeup_issue",
        "makeup_card",
        &id.to_string(),
        json!({ "user_id": target.id, "username": target.username, "kind": kind }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MakeupCardAdminRow {
    pub id: i64,
    pub kind: String,
    pub user_id: i64,
    pub username: String,
    pub display_name: Option<String>,
    pub status: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub used_for_date: Option<NaiveDate>,
}

#[derive(Deserialize)]
pub struct CardListQuery {
    pub status: Option<String>,
    pub page: Option<i64>,
}

pub(crate) async fn list_makeup_cards_admin(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<CardListQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let status = query.status.unwrap_or_default();
    let page = query.page.unwrap_or(1).max(1);
    let limit = 30i64;
    let offset = (page - 1) * limit;
    let rows: Vec<MakeupCardAdminRow> = sqlx::query_as(
        "SELECT m.id, m.kind, m.user_id, u.username, u.display_name, m.status, m.issued_at, \
                m.expires_at, m.used_at, m.used_for_date \
         FROM makeup_cards m JOIN users u ON u.id = m.user_id \
         WHERE ($1 = '' OR m.status = $1) ORDER BY m.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM makeup_cards WHERE ($1 = '' OR status = $1)",
    )
    .bind(&status)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({
        "cards": rows,
        "total": total,
        "page": page,
        "pages": (total + limit - 1) / limit,
    })))
}

fn generate_reset_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    let mut groups = Vec::with_capacity(4);
    for _ in 0..4 {
        let group: String = (0..4)
            .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
            .collect();
        groups.push(group);
    }
    format!("PMRC-{}", groups.join("-"))
}

#[derive(Deserialize)]
pub struct CreateResetCodesRequest {
    #[serde(default = "default_reset_count")]
    pub count: i64,
    pub expires_days: Option<i64>,
}

fn default_reset_count() -> i64 {
    1
}

pub(crate) async fn create_reset_codes(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<CreateResetCodesRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    if body.count < 1 || body.count > 100 {
        return Err(AppError::bad_request("一次最多生成 100 个兑换码"));
    }
    let expires_at = body
        .expires_days
        .filter(|days| *days > 0)
        .map(|days| Utc::now() + chrono::Duration::days(days));
    let mut codes: Vec<String> = Vec::with_capacity(body.count as usize);
    let mut tx = state.pool.begin().await?;
    for _ in 0..body.count {
        let mut inserted = false;
        for _ in 0..3 {
            let code = generate_reset_code();
            let hash = crate::util::sha256(code.as_bytes());
            let prefix: String = code.chars().take(9).collect();
            let result = sqlx::query(
                "INSERT INTO reset_codes (code_hash, code_prefix, created_by, expires_at) \
                 VALUES ($1, $2, $3, $4) ON CONFLICT (code_hash) DO NOTHING",
            )
            .bind(&hash)
            .bind(&prefix)
            .bind(actor.id)
            .bind(expires_at)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 1 {
                codes.push(code);
                inserted = true;
                break;
            }
        }
        if !inserted {
            return Err(AppError::internal("生成兑换码失败，请重试"));
        }
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.reset_create",
        "reset_code",
        "batch",
        json!({ "count": codes.len() }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "codes": codes })))
}

#[derive(Deserialize)]
pub struct IssueResetRequest {
    pub user_id: i64,
}

/// 直接给单个用户发一张重置卡（无需兑换码），长期有效。
pub(crate) async fn issue_reset_one(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<IssueResetRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(body.user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    if target.is_banned {
        return Err(AppError::bad_request("封禁用户不发卡"));
    }
    let mut tx = state.pool.begin().await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM reset_codes \
                        WHERE user_id = $1 AND status = 'issued' \
                          AND (expires_at IS NULL OR expires_at > now()))",
    )
    .bind(target.id)
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(AppError::conflict("该用户已持有一张未使用的重置卡"));
    }
    let mut id: Option<i64> = None;
    for _ in 0..3 {
        let code = generate_reset_code();
        let hash = crate::util::sha256(code.as_bytes());
        let prefix: String = code.chars().take(9).collect();
        let row: Option<i64> = sqlx::query_scalar(
            "INSERT INTO reset_codes (code_hash, code_prefix, status, user_id, created_by, redeemed_at) \
             VALUES ($1, $2, 'issued', $3, $4, now()) ON CONFLICT (code_hash) DO NOTHING RETURNING id",
        )
        .bind(&hash)
        .bind(&prefix)
        .bind(target.id)
        .bind(actor.id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row_id) = row {
            id = Some(row_id);
            break;
        }
    }
    let Some(id) = id else {
        return Err(AppError::internal("生成重置卡失败，请重试"));
    };
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.reset_issue",
        "reset_code",
        &id.to_string(),
        json!({ "user_id": target.id, "username": target.username }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": id })))
}

/// 给所有未封禁、且未持有未使用重置卡的用户各直接发一张重置卡（长期有效）。
pub(crate) async fn issue_reset_all(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(20261114)")
        .execute(&mut *tx)
        .await?;
    let targets: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM users WHERE NOT is_banned \
           AND NOT EXISTS (SELECT 1 FROM reset_codes \
                            WHERE user_id = users.id AND status = 'issued' \
                              AND (expires_at IS NULL OR expires_at > now())) \
         ORDER BY id",
    )
    .fetch_all(&mut *tx)
    .await?;
    let mut issued = 0i64;
    for user_id in targets {
        let mut inserted = false;
        for _ in 0..3 {
            let code = generate_reset_code();
            let hash = crate::util::sha256(code.as_bytes());
            let prefix: String = code.chars().take(9).collect();
            let result = sqlx::query(
                "INSERT INTO reset_codes (code_hash, code_prefix, status, user_id, created_by, redeemed_at) \
                 VALUES ($1, $2, 'issued', $3, $4, now()) ON CONFLICT (code_hash) DO NOTHING",
            )
            .bind(&hash)
            .bind(&prefix)
            .bind(user_id)
            .bind(actor.id)
            .execute(&mut *tx)
            .await?;
            if result.rows_affected() == 1 {
                inserted = true;
                break;
            }
        }
        if !inserted {
            return Err(AppError::internal("生成重置卡失败，请重试"));
        }
        issued += 1;
    }
    tx.commit().await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.reset_issue_all",
        "reset_code",
        "all",
        json!({ "issued": issued }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "issued": issued })))
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ResetCodeAdminRow {
    pub id: i64,
    pub code_prefix: String,
    pub status: String,
    pub user_id: Option<i64>,
    pub username: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub used_at: Option<DateTime<Utc>>,
}

pub(crate) async fn list_reset_codes_admin(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Query(query): Query<CardListQuery>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let status = query.status.unwrap_or_default();
    let page = query.page.unwrap_or(1).max(1);
    let limit = 30i64;
    let offset = (page - 1) * limit;
    let rows: Vec<ResetCodeAdminRow> = sqlx::query_as(
        "SELECT r.id, r.code_prefix, r.status, r.user_id, u.username, r.created_at, \
                r.expires_at, r.redeemed_at, r.used_at \
         FROM reset_codes r LEFT JOIN users u ON u.id = r.user_id \
         WHERE ($1 = '' OR r.status = $1) ORDER BY r.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    let total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reset_codes WHERE ($1 = '' OR status = $1)",
    )
    .bind(&status)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({
        "codes": rows,
        "total": total,
        "page": page,
        "pages": (total + limit - 1) / limit,
    })))
}

pub(crate) async fn revoke_reset_code(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let result = sqlx::query(
        "UPDATE reset_codes SET status = 'revoked' \
         WHERE id = $1 AND status IN ('unassigned', 'issued')",
    )
    .bind(id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::not_found("兑换码不存在或已使用、已作废"));
    }
    write_audit(
        &state.pool,
        Some(actor.id),
        "card.reset_revoke",
        "reset_code",
        &id.to_string(),
        json!({}),
    )
    .await;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").unwrap()
    }

    fn date_set(values: &[&str]) -> BTreeSet<NaiveDate> {
        values.iter().map(|value| day(value)).collect()
    }

    #[test]
    fn streak_counts_consecutive_run() {
        let dates = date_set(&["2026-11-01", "2026-11-02", "2026-11-03", "2026-11-05"]);
        assert_eq!(streak_ending_at(&dates, day("2026-11-03")), 3);
        assert_eq!(streak_ending_at(&dates, day("2026-11-05")), 1);
        assert_eq!(streak_ending_at(&dates, day("2026-11-04")), 0);
    }

    #[test]
    fn makeup_fills_gap_and_restores_streak() {
        // 周一、周二、（漏周三）、周四、周五：补周三后连续 5 天。
        let mut dates = date_set(&["2026-11-02", "2026-11-03", "2026-11-05", "2026-11-06"]);
        assert_eq!(streak_ending_at(&dates, day("2026-11-06")), 2);
        dates.insert(day("2026-11-04"));
        assert_eq!(streak_ending_at(&dates, day("2026-11-06")), 5);
    }

    #[test]
    fn backfill_recomputes_later_rows_streaks() {
        // 补签前的逐行 streak：11-02:1、11-03:2、11-05:1、11-06:2。
        let dates = date_set(&["2026-11-02", "2026-11-03", "2026-11-05", "2026-11-06"]);
        assert_eq!(
            streaks_for_dates(&dates),
            vec![
                (day("2026-11-02"), 1),
                (day("2026-11-03"), 2),
                (day("2026-11-05"), 1),
                (day("2026-11-06"), 2),
            ]
        );
        // 补上 11-04 后，后面两行的 streak 必须跟着重算为 4、5，
        // 而不是留在 1、2（这就是之前 stale 的根源）。
        let mut filled = dates.clone();
        filled.insert(day("2026-11-04"));
        assert_eq!(
            streaks_for_dates(&filled),
            vec![
                (day("2026-11-02"), 1),
                (day("2026-11-03"), 2),
                (day("2026-11-04"), 3),
                (day("2026-11-05"), 4),
                (day("2026-11-06"), 5),
            ]
        );
    }

    #[test]
    fn longest_run_finds_middle_run() {
        // 11-01~11-05、11-08~11-12：两段各 5 天；补上 11-06、11-07 后连成 12 天。
        // 只看截至最后一天的连续段会漏算中间这段，所以最长连续要按全量算。
        let dates = date_set(&[
            "2026-11-01", "2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05",
            "2026-11-08", "2026-11-09", "2026-11-10", "2026-11-11", "2026-11-12",
        ]);
        assert_eq!(longest_run(&dates), 5);
        let mut filled = dates.clone();
        filled.insert(day("2026-11-06"));
        filled.insert(day("2026-11-07"));
        assert_eq!(longest_run(&filled), 12);
    }

    #[test]
    fn makeup_window_is_last_seven_days() {
        let today = day("2026-11-10");
        let earliest = today - chrono::Duration::days(MAKEUP_WINDOW_DAYS);
        let latest = today.pred_opt().unwrap();
        assert_eq!(earliest, day("2026-11-03"));
        assert_eq!(latest, day("2026-11-09"));
    }

    #[test]
    fn reset_codes_are_unique_and_prefixed() {
        let first = generate_reset_code();
        let second = generate_reset_code();
        assert_ne!(first, second);
        assert!(first.starts_with("PMRC-"));
        assert_eq!(first.len(), 24);
        assert!(first
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-'));
    }
}
