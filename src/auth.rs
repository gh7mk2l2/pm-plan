use crate::error::{AppError, AppResult};
use crate::models::User;
use crate::state::AppState;
use crate::util::{random_token, sanitize_next, sha256};
use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect, Response};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::Value;

pub const SESSION_COOKIE: &str = "pmplan_session";
pub const OAUTH_BIND_COOKIE: &str = "pmplan_oauth_bind";

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = cookie_value(&parts.headers, SESSION_COOKIE)
            .ok_or_else(|| AppError::unauthorized("请先登录"))?;
        let token_hash = sha256(token.as_bytes());
        let user = sqlx::query_as::<_, User>(
            "SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = $1 AND s.expires_at > now()",
        )
        .bind(&token_hash)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::unauthorized("登录状态已失效，请重新登录"))?;

        if user.is_banned {
            return Err(AppError::forbidden(
                user.ban_reason
                    .clone()
                    .unwrap_or_else(|| "账号已被封禁".to_string()),
            ));
        }
        Ok(AuthUser(user))
    }
}

pub fn require_admin(user: &User) -> AppResult<()> {
    if user.is_admin() {
        Ok(())
    } else {
        Err(AppError::forbidden("需要站长权限"))
    }
}

/// 仅站长（root）可调用的操作使用此守卫；普通管理员会被拒绝。
pub fn require_root(user: &User) -> AppResult<()> {
    if user.is_root() {
        Ok(())
    } else {
        Err(AppError::forbidden("只有站长可以操作"))
    }
}

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for header_value in headers.get_all(header::COOKIE).iter() {
        let Ok(raw) = header_value.to_str() else {
            continue;
        };
        for part in raw.split(';') {
            let mut kv = part.trim().splitn(2, '=');
            if kv.next() == Some(name) {
                if let Some(value) = kv.next() {
                    return Some(value.trim().to_string());
                }
            }
        }
    }
    None
}

fn cookie_string(name: &str, value: &str, max_age: i64, secure: bool) -> String {
    let mut out = format!(
        "{name}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    );
    if secure {
        out.push_str("; Secure");
    }
    out
}

fn append_cookie(resp: &mut Response, cookie: String) {
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().append(header::SET_COOKIE, value);
    }
}

fn clear_cookie(name: &str, secure: bool) -> String {
    cookie_string(name, "", 0, secure)
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
}

pub async fn oauth_login(
    State(state): State<AppState>,
    Query(query): Query<LoginQuery>,
) -> AppResult<Response> {
    let oauth_state = random_token(32);
    let bind = random_token(32);
    let verifier = random_token(48);
    let challenge = URL_SAFE_NO_PAD.encode(sha256(verifier.as_bytes()));
    let next_path = sanitize_next(query.next.as_deref().unwrap_or("/"));
    let state_hash = sha256(oauth_state.as_bytes());
    let bind_hash = sha256(bind.as_bytes());

    sqlx::query("DELETE FROM oauth_states WHERE expires_at < now() - interval '1 day'")
        .execute(&state.pool)
        .await?;
    sqlx::query(
        "INSERT INTO oauth_states (state_hash, bind_hash, code_verifier, next_path, expires_at) \
         VALUES ($1, $2, $3, $4, now() + interval '10 minutes')",
    )
    .bind(&state_hash)
    .bind(&bind_hash)
    .bind(&verifier)
    .bind(&next_path)
    .execute(&state.pool)
    .await?;

    let mut url =
        url::Url::parse(&state.cfg.auth_endpoint).map_err(|e| AppError::internal(e.to_string()))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &state.cfg.client_id)
        .append_pair("redirect_uri", &state.cfg.redirect_uri())
        .append_pair("scope", "openid profile email")
        .append_pair("state", &oauth_state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    let mut resp = Redirect::to(url.as_str()).into_response();
    append_cookie(
        &mut resp,
        cookie_string(OAUTH_BIND_COOKIE, &bind, 600, state.cfg.cookie_secure),
    );
    Ok(resp)
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    match callback_inner(&state, &query, &headers).await {
        Ok((session_token, next_path)) => {
            let mut resp =
                Redirect::to(&format!("{}{}", state.cfg.base_url, next_path)).into_response();
            append_cookie(
                &mut resp,
                cookie_string(
                    SESSION_COOKIE,
                    &session_token,
                    30 * 24 * 3600,
                    state.cfg.cookie_secure,
                ),
            );
            append_cookie(
                &mut resp,
                clear_cookie(OAUTH_BIND_COOKIE, state.cfg.cookie_secure),
            );
            resp
        }
        Err(message) => {
            tracing::warn!(error = %message, "oauth callback failed");
            let encoded: String =
                url::form_urlencoded::byte_serialize(message.as_bytes()).collect();
            let mut resp = Redirect::to(&format!(
                "{}/?oauth_error={}",
                state.cfg.base_url, encoded
            ))
            .into_response();
            append_cookie(
                &mut resp,
                clear_cookie(OAUTH_BIND_COOKIE, state.cfg.cookie_secure),
            );
            resp
        }
    }
}

async fn callback_inner(
    state: &AppState,
    query: &CallbackQuery,
    headers: &HeaderMap,
) -> Result<(String, String), String> {
    if let Some(err) = &query.error {
        let detail = query
            .error_description
            .clone()
            .unwrap_or_else(|| err.clone());
        return Err(format!("LINUX DO 授权失败：{detail}"));
    }
    let code = query
        .code
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "缺少授权码".to_string())?;
    let oauth_state = query
        .state
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| "缺少 state 参数".to_string())?;
    let bind = cookie_value(headers, OAUTH_BIND_COOKIE)
        .ok_or_else(|| "浏览器登录状态缺失，请重新发起登录".to_string())?;

    let state_hash = sha256(oauth_state.as_bytes());
    let row: Option<(Vec<u8>, String, String)> = sqlx::query_as(
        "UPDATE oauth_states SET used_at = now() \
         WHERE state_hash = $1 AND used_at IS NULL AND expires_at > now() \
         RETURNING bind_hash, code_verifier, next_path",
    )
    .bind(&state_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "consume oauth state failed");
        "登录状态校验失败，请重试".to_string()
    })?;
    let (bind_hash, verifier, next_path) =
        row.ok_or_else(|| "登录状态已失效或已被使用，请重新登录".to_string())?;

    if sha256(bind.as_bytes()) != bind_hash {
        return Err("登录状态与浏览器不匹配，请重新登录".to_string());
    }

    let token = exchange_code(state, code, &verifier).await?;
    let profile = fetch_user(state, &token.access_token).await?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| format!("数据库连接失败：{e}"))?;
    sqlx::query("SELECT pg_advisory_xact_lock(20261030)")
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("数据库锁失败：{e}"))?;

    let existing: Option<User> = sqlx::query_as("SELECT * FROM users WHERE linuxdo_id = $1")
        .bind(profile.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("读取用户失败：{e}"))?;

    let user = if let Some(existing) = existing {
        if existing.is_banned {
            return Err(existing
                .ban_reason
                .unwrap_or_else(|| "账号已被封禁".to_string()));
        }
        sqlx::query_as::<_, User>(
            "UPDATE users SET username = $2, display_name = $3, avatar_url = $4, \
             trust_level = $5, last_login_at = now(), updated_at = now() \
             WHERE id = $1 RETURNING *",
        )
        .bind(existing.id)
        .bind(&profile.username)
        .bind(&profile.display_name)
        .bind(&profile.avatar_url)
        .bind(profile.trust_level)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("更新用户失败：{e}"))?
    } else {
        let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("统计用户失败：{e}"))?;
        let registration_open: bool = sqlx::query_scalar(
            "SELECT COALESCE((SELECT (value #>> '{}')::boolean FROM settings \
              WHERE key = 'registration_open'), TRUE)",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("读取设置失败：{e}"))?;
        if user_count > 0 && !registration_open {
            return Err("站点当前已关闭注册，请联系站长".to_string());
        }
        let role = if user_count == 0 { "root" } else { "user" };
        sqlx::query_as::<_, User>(
            "INSERT INTO users (linuxdo_id, username, display_name, avatar_url, trust_level, \
             role, plan, earned_plan, plan_source, last_login_at) \
             VALUES ($1, $2, $3, $4, $5, $6, 'go', 'go', 'default', now()) RETURNING *",
        )
        .bind(profile.id)
        .bind(&profile.username)
        .bind(&profile.display_name)
        .bind(&profile.avatar_url)
        .bind(profile.trust_level)
        .bind(role)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("创建用户失败：{e}"))?
    };

    let access_enc = state
        .crypto
        .encrypt(token.access_token.as_bytes())
        .map_err(|e| format!("凭据加密失败：{e}"))?;
    let refresh_enc = match token.refresh_token.as_deref() {
        Some(value) => Some(
            state
                .crypto
                .encrypt(value.as_bytes())
                .map_err(|e| format!("凭据加密失败：{e}"))?,
        ),
        None => None,
    };
    let expires_at = token
        .expires_in
        .map(|seconds| Utc::now() + Duration::seconds(seconds));

    sqlx::query(
        "INSERT INTO oauth_credentials (user_id, access_token_enc, refresh_token_enc, \
         token_expires_at, scope, updated_at) VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (user_id) DO UPDATE SET access_token_enc = EXCLUDED.access_token_enc, \
         refresh_token_enc = EXCLUDED.refresh_token_enc, token_expires_at = EXCLUDED.token_expires_at, \
         scope = EXCLUDED.scope, updated_at = now()",
    )
    .bind(user.id)
    .bind(&access_enc)
    .bind(&refresh_enc)
    .bind(expires_at)
    .bind(&token.scope)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("保存登录凭据失败：{e}"))?;

    let session_token = random_token(32);
    let session_hash = sha256(session_token.as_bytes());
    let ip = crate::util::client_ip(headers);
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(300)
        .collect::<String>();
    sqlx::query(
        "INSERT INTO sessions (user_id, token_hash, expires_at, ip, user_agent) \
         VALUES ($1, $2, now() + interval '30 days', NULLIF($3, ''), NULLIF($4, ''))",
    )
    .bind(user.id)
    .bind(&session_hash)
    .bind(&ip)
    .bind(&user_agent)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("创建会话失败：{e}"))?;

    tx.commit()
        .await
        .map_err(|e| format!("提交登录失败：{e}"))?;
    tracing::info!(user_id = user.id, username = %user.username, role = %user.role, "linux.do login");
    Ok((session_token, next_path))
}

pub async fn oauth_logout(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_value(&headers, SESSION_COOKIE) {
        let hash = sha256(token.as_bytes());
        let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
            .bind(&hash)
            .execute(&state.pool)
            .await;
    }
    let mut resp = Redirect::to(&format!("{}/", state.cfg.base_url)).into_response();
    append_cookie(&mut resp, clear_cookie(SESSION_COOKIE, state.cfg.cookie_secure));
    resp
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
}

async fn exchange_code(
    state: &AppState,
    code: &str,
    verifier: &str,
) -> Result<TokenResponse, String> {
    let endpoint = &state.cfg.token_endpoint;
    let redirect_uri = state.cfg.redirect_uri();
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier),
    ];

    let first = state
        .http
        .post(endpoint)
        .basic_auth(&state.cfg.client_id, Some(&state.cfg.client_secret))
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("连接 LINUX DO 令牌端点失败：{e}"))?;
    let status = first.status();
    let body = first
        .text()
        .await
        .map_err(|e| format!("读取令牌响应失败：{e}"))?;
    if status.is_success() {
        return serde_json::from_str(&body).map_err(|e| format!("解析令牌响应失败：{e}"));
    }

    // Some deployments only accept client_secret_post. Retry once on auth errors.
    if status.as_u16() == 401 || status.as_u16() == 400 {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", verifier),
            ("client_id", state.cfg.client_id.as_str()),
            ("client_secret", state.cfg.client_secret.as_str()),
        ];
        let second = state
            .http
            .post(endpoint)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("连接 LINUX DO 令牌端点失败：{e}"))?;
        let status = second.status();
        let body = second
            .text()
            .await
            .map_err(|e| format!("读取令牌响应失败：{e}"))?;
        if status.is_success() {
            return serde_json::from_str(&body).map_err(|e| format!("解析令牌响应失败：{e}"));
        }
    }

    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("error_description")
                .or_else(|| v.get("error"))
                .and_then(|x| x.as_str().map(str::to_string))
        })
        .unwrap_or_else(|| format!("HTTP {status}"));
    Err(format!("换取访问令牌失败：{detail}"))
}

struct OAuthProfile {
    id: i64,
    username: String,
    display_name: Option<String>,
    avatar_url: Option<String>,
    trust_level: i32,
}

async fn fetch_user(state: &AppState, access_token: &str) -> Result<OAuthProfile, String> {
    let resp = state
        .http
        .get(&state.cfg.userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("请求 LINUX DO 用户信息失败：{e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取用户信息失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("获取 LINUX DO 用户信息失败：HTTP {status}"));
    }
    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("解析用户信息失败：{e}"))?;

    let id = value
        .get("id")
        .and_then(value_i64)
        .or_else(|| value.get("sub").and_then(value_i64))
        .ok_or_else(|| "LINUX DO 用户信息缺少 id".to_string())?;
    if value.get("active").and_then(Value::as_bool) == Some(false) {
        return Err("LINUX DO 账号当前不可用".to_string());
    }
    let username = value
        .get("username")
        .and_then(Value::as_str)
        .or_else(|| value.get("login").and_then(Value::as_str))
        .or_else(|| value.get("preferred_username").and_then(Value::as_str))
        .unwrap_or("linuxdo-user")
        .to_string();
    let display_name = value
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| value.get("display_name").and_then(Value::as_str))
        .map(|s| s.to_string());
    let trust_level = value
        .get("trust_level")
        .and_then(value_i64)
        .unwrap_or(0) as i32;

    let mut avatar = value
        .get("avatar_url")
        .and_then(Value::as_str)
        .or_else(|| value.get("picture").and_then(Value::as_str))
        .map(|s| s.to_string());
    if avatar.is_none() {
        if let Some(template) = value.get("avatar_template").and_then(Value::as_str) {
            let replaced = template.replace("{size}", "144");
            avatar = Some(if replaced.starts_with('/') {
                format!("https://linux.do{replaced}")
            } else {
                replaced
            });
        }
    }
    let avatar_url = avatar.filter(|url| {
        url.starts_with("https://") || url.starts_with("http://")
    });

    Ok(OAuthProfile {
        id,
        username,
        display_name,
        avatar_url,
        trust_level,
    })
}

fn value_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

