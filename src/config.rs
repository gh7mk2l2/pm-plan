use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{FixedOffset, NaiveDate, Utc};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: String,
    pub base_url: String,
    pub database_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub auth_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub encryption_key: Vec<u8>,
    pub cookie_secure: bool,
    pub web_dir: String,
    pub tz_offset: FixedOffset,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        let get = |key: &str| -> Result<String> {
            std::env::var(key).with_context(|| format!("缺少环境变量 {key}"))
        };

        let base_url = get("BASE_URL")?.trim_end_matches('/').to_string();
        let key_b64 = get("ENCRYPTION_KEY")?;
        let encryption_key = STANDARD
            .decode(key_b64.trim())
            .context("ENCRYPTION_KEY 不是合法的 base64")?;
        if encryption_key.len() != 32 {
            bail!("ENCRYPTION_KEY 必须是 32 字节（base64 编码）");
        }

        let cookie_secure = std::env::var("COOKIE_SECURE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or_else(|_| base_url.starts_with("https://"));

        let offset_raw = std::env::var("APP_TIMEZONE_OFFSET").unwrap_or_else(|_| "+08:00".into());
        let tz_offset = parse_offset(&offset_raw)?;

        Ok(Self {
            bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            base_url,
            database_url: get("DATABASE_URL")?,
            client_id: get("LINUXDO_CLIENT_ID")?,
            client_secret: get("LINUXDO_CLIENT_SECRET")?,
            auth_endpoint: get("LINUXDO_AUTH_ENDPOINT")?,
            token_endpoint: get("LINUXDO_TOKEN_ENDPOINT")?,
            userinfo_endpoint: get("LINUXDO_USERINFO_ENDPOINT")?,
            encryption_key,
            cookie_secure,
            web_dir: std::env::var("WEB_DIR").unwrap_or_else(|_| "./web".into()),
            tz_offset,
        })
    }

    pub fn redirect_uri(&self) -> String {
        format!("{}/v1/oauth/callback", self.base_url)
    }

    pub fn today(&self) -> NaiveDate {
        Utc::now().with_timezone(&self.tz_offset).date_naive()
    }
}

fn parse_offset(raw: &str) -> Result<FixedOffset> {
    let raw = raw.trim();
    let (sign, rest) = match raw.chars().next() {
        Some('+') => (1, &raw[1..]),
        Some('-') => (-1, &raw[1..]),
        _ => (1, raw),
    };
    let mut parts = rest.split(':');
    let hours: i32 = parts
        .next()
        .unwrap_or("8")
        .parse()
        .context("APP_TIMEZONE_OFFSET 小时部分非法")?;
    let minutes: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let seconds = sign * (hours * 3600 + minutes * 60);
    FixedOffset::east_opt(seconds).context("APP_TIMEZONE_OFFSET 超出范围")
}
