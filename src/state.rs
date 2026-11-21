use crate::config::Config;
use crate::crypto::Crypto;
use sqlx::PgPool;
use std::sync::Arc;
use crate::ipban::IpGuard;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    pub crypto: Arc<Crypto>,
    pub ip_guard: Arc<IpGuard>,
    /// Client without a User-Agent, used for magpie calls and hCaptcha siteverify
    /// (matches the production binary's outgoing requests).
    pub plain_http: reqwest::Client,
    pub captcha: Arc<crate::captcha::CaptchaConfig>,
    /// 自建图片人机验证（2026-11-14 起替代 hCaptcha/Turnstile）；captcha.rs 的代码留着不动，不再被调用。
    pub selfcheck: Arc<crate::selfcheck::SelfCheck>,
    /// 滑块拼图验证（2026-11-15 起替代 selfcheck 的 3×3 选图）：签到/补签现在调它。
    /// selfcheck 模块保留编译（旧测试作回归对照），但路由已摘掉，不再被调用。
    pub slider: Arc<crate::slider::Slider>,
    pub zcode_magpie_key: Option<String>,
}

impl AppState {
    pub fn new(pool: PgPool, cfg: Config, crypto: Crypto) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(600))
            .user_agent("PM-Plan/1.0")
            .build()?;
        Ok(Self {
            pool,
            cfg: Arc::new(cfg),
            http,
            crypto: Arc::new(crypto),
            ip_guard: Arc::new(IpGuard::from_env()),
            plain_http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            captcha: Arc::new(crate::captcha::CaptchaConfig::from_env()),
            selfcheck: Arc::new(crate::selfcheck::SelfCheck::load_from_env()),
            slider: Arc::new(crate::slider::Slider::load_from_env()),
            zcode_magpie_key: std::env::var("ZCODE_MAGPIE_KEY")
                .ok()
                .filter(|v| !v.trim().is_empty()),
        })
    }
}
