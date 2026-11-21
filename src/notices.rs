//! 站内通知（2026-11-21）：管理员给指定用户发站内警告/通知，用户登录后在首页顶部看到。
//! 最小可用：查自己的通知、标已读、管理员发送。写审计 notice.send。

use crate::admin::write_audit;
use crate::auth::{require_admin, AuthUser};
use crate::error::{AppError, AppJson, AppResult};
use crate::models::User;
use crate::state::AppState;
use crate::util::truncate_chars;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;

#[derive(Serialize, FromRow)]
pub struct Notice {
    pub id: i64,
    pub user_id: i64,
    pub title: String,
    pub body: String,
    pub kind: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}

/// 当前用户的站内通知（最新 20 条）+ 未读数。
pub async fn list_notices(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> AppResult<Json<Value>> {
    let notices: Vec<Notice> = sqlx::query_as(
        "SELECT id, user_id, title, body, kind, is_read, created_at \
         FROM notices WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let unread_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM notices WHERE user_id = $1 AND NOT is_read",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(json!({ "notices": notices, "unread_count": unread_count })))
}

/// 标记自己的某条通知为已读（不是自己的、或已读的，统一返回 404，不泄露归属）。
pub async fn mark_read(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<i64>,
) -> AppResult<Json<Value>> {
    let updated = sqlx::query(
        "UPDATE notices SET is_read = TRUE, read_at = now() \
         WHERE id = $1 AND user_id = $2 AND NOT is_read",
    )
    .bind(id)
    .bind(user.id)
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::not_found("通知不存在"));
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SendNoticeRequest {
    pub user_id: i64,
    pub title: String,
    pub body: String,
    pub kind: Option<String>,
}

/// 管理员给指定用户发站内通知。
pub async fn send_notice(
    State(state): State<AppState>,
    AuthUser(actor): AuthUser,
    AppJson(body): AppJson<SendNoticeRequest>,
) -> AppResult<Json<Value>> {
    require_admin(&actor)?;
    let title = truncate_chars(body.title.trim(), 100);
    let text = truncate_chars(body.body.trim(), 2000);
    if title.is_empty() || text.is_empty() {
        return Err(AppError::bad_request("标题和内容不能为空"));
    }
    let kind = match body.kind.as_deref() {
        Some("info") => "info",
        Some("warning") => "warning",
        Some(_) => return Err(AppError::bad_request("kind 只能是 warning 或 info")),
        None => "warning",
    };
    let target: User = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(body.user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::not_found("用户不存在"))?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO notices (user_id, title, body, kind, created_by) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(target.id)
    .bind(&title)
    .bind(&text)
    .bind(kind)
    .bind(actor.id)
    .fetch_one(&state.pool)
    .await?;
    write_audit(
        &state.pool,
        Some(actor.id),
        "notice.send",
        "user",
        &target.id.to_string(),
        json!({ "username": target.username, "notice_id": id, "title": title }),
    )
    .await;
    Ok(Json(json!({ "ok": true, "id": id })))
}
