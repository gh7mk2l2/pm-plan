mod admin;
mod api;
mod auth;
mod captcha;
mod cards;
mod channel_opts;
mod channels;
mod config;
mod crypto;
mod error;
mod ipban;
mod models;
mod plans;
mod proxy;
mod selfcheck;
mod slider;
mod state;
mod usage;
mod util;
mod zcode;

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use config::Config;
use crypto::Crypto;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("pm_plan=info")),
        )
        .with_target(false)
        .init();

    let cfg = Config::from_env()?;
    tracing::info!(base_url = %cfg.base_url, bind = %cfg.bind, "starting PM-Plan");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(15))
        .connect(&cfg.database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let setup_token = {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"pm-plan-setup-v1");
        hasher.update(&cfg.encryption_key);
        hasher.update(b"bootstrap-channel");
        URL_SAFE_NO_PAD.encode(hasher.finalize())[..32].to_string()
    };
    let crypto = Crypto::new(&cfg.encryption_key)?;
    let web_dir = cfg.web_dir.clone();
    let bind = cfg.bind.clone();
    let state = AppState::new(pool, cfg, crypto)?;
    state.ip_guard.load(&state.pool).await;

    tokio::spawn(channels::health_loop(state.clone()));
    tokio::spawn(channels::auto_test_loop(state.clone()));

    let api = Router::new()
        .merge(api::router())
        .nest("/admin", admin::router().merge(ipban::admin_router()))
        .layer(axum::extract::Extension(error::SetupSecret(setup_token)))
        .layer(middleware::from_fn(csrf_guard));

    // 与线上一致：静态文件直出；找不到时回落到 index.html（text/html; charset=utf-8），
    // 但 /static/ 下或带扩展名的路径返回 404（同样带 index.html 正文），其余 SPA 路由 200。
    let spa_dir = web_dir.clone();
    let spa = move |uri: axum::http::Uri| {
        let dir = spa_dir.clone();
        async move { spa_fallback(&dir, uri.path()).await }
    };
    let static_files = ServeDir::new(&web_dir)
        .append_index_html_on_directories(false)
        .fallback(axum::handler::HandlerWithoutStateExt::into_service(spa));

    let app = Router::new()
        .route(
            "/favicon.ico",
            get(|| async { axum::response::Redirect::temporary("/static/logo.jpg") }),
        )
        .route("/v1/oauth/login", get(auth::oauth_login))
        .route("/v1/oauth/callback", get(auth::oauth_callback))
        .route("/v1/oauth/logout", get(auth::oauth_logout))
        .route("/v1/models", get(proxy::handle_models))
        .route("/v1/chat/completions", post(proxy::handle_proxy))
        .route("/v1/completions", post(proxy::handle_proxy))
        .route("/v1/embeddings", post(proxy::handle_proxy))
        .route("/v1/responses", post(proxy::handle_proxy))
        .route("/v1/{*path}", post(proxy::handle_proxy))
        .nest("/api", api)
        .fallback_service(static_files)
        .layer(middleware::from_fn_with_state(state.clone(), ipban::guard))
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .layer(axum::extract::DefaultBodyLimit::max(20 * 1024 * 1024))
        .with_state(state);

    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    Ok(())
}

async fn csrf_guard(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    if matches!(
        method,
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        return next.run(req).await;
    }
    let valid = req
        .headers()
        .get("x-pmplan-csrf")
        .and_then(|value| value.to_str().ok())
        .map(|value| value == "1")
        .unwrap_or(false);
    if !valid {
        return error::AppError::new(
            StatusCode::FORBIDDEN,
            "csrf_rejected",
            "请求缺少 CSRF 校验头",
        )
        .into_response();
    }
    next.run(req).await
}

async fn security_headers(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            // 人机验证：hCaptcha + Turnstile 双通道（2026-11-15 换回）：
            // 放行两家验证的脚本、iframe 与连接。
            "default-src 'self'; img-src 'self' https: data:; \
             style-src 'self' 'unsafe-inline' https://hcaptcha.com https://*.hcaptcha.com; \
             script-src 'self' https://hcaptcha.com https://*.hcaptcha.com https://challenges.cloudflare.com; \
             connect-src 'self' https://hcaptcha.com https://*.hcaptcha.com https://challenges.cloudflare.com; \
             frame-src https://hcaptcha.com https://*.hcaptcha.com https://challenges.cloudflare.com; \
             font-src 'self'; base-uri 'self'; \
             form-action 'self'; frame-ancestors 'none'",
        ),
    );
    headers.insert(
        HeaderName::from_static("strict-transport-security"),
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    response
}

/// SPA 回落：读取 index.html。资源类路径（/static/ 下或末段带扩展名）返回 404。
async fn spa_fallback(web_dir: &str, path: &str) -> Response {
    let status = if is_asset_path(path) {
        StatusCode::NOT_FOUND
    } else {
        StatusCode::OK
    };
    match tokio::fs::read(format!("{web_dir}/index.html")).await {
        Ok(body) => (
            status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn is_asset_path(path: &str) -> bool {
    path.starts_with("/static/") || path.rsplit('/').next().map(|seg| seg.contains('.')).unwrap_or(false)
}

#[cfg(test)]
mod spa_tests {
    use super::is_asset_path;

    #[test]
    fn asset_paths_are_detected_like_prod() {
        assert!(!is_asset_path("/"));
        assert!(!is_asset_path("/admin"));
        assert!(!is_asset_path("/admin/x"));
        assert!(is_asset_path("/static/"));
        assert!(is_asset_path("/static/nope.js"));
        assert!(is_asset_path("/x/y.png"));
        assert!(is_asset_path("/admin/favicon.ico"));
    }
}
