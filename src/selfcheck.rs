//! 自建图片人机验证（2026-11-14）：照 hCaptcha 的九宫格选图样子抄，替代 hCaptcha/Turnstile。
//!
//! 诚实定位：图片选图能被现在的 AI 视觉模型高成功率解开，这个验证只是减速带，
//! 真正防刷靠 LinuxDo 登录 + 限流。不要在任何地方宣称它能挡住机器人。
//!
//! 流程：
//! - `GET  /api/selfcheck/challenge` → 发一道挑战（2 页 × 3×3），返回一次性 token（120 秒有效）。
//! - `POST /api/selfcheck/verify` `{token, selections: [[bool; 9]; 2]}` → 答对返回一次性
//!   pass_token（120 秒有效），签到/补签用它换服务；答错则 token 作废，响应里直接带一道新挑战。
//! - `GET  /api/selfcheck/img/{id}` → 返回图片文件（按 manifest 白名单，不暴露文件名与分类）。
//! - 签到/补签：请求体里的 `captcha_token` 现在是 pass_token，服务端校验并一次消费。
//!
//! 图片池：只用 openly licensed 的图片（Wikimedia Commons），下载到机器本地、由后端自己提供，
//! 禁止热链、禁止 CDN 外链。图片清单是图片目录下的 `images.json`（manifest）；
//! 对外暴露的图片 id 是加载时随机打乱后分配的不透明编号（img-001…），文件名与分类
//! 绝不出现在发给浏览器的任何内容里（否则答案直接泄露）。
//!
//! 状态全部在内存里（挑战、pass token、限流记录）：服务重启即清空，挑战本来就只有 120 秒寿命。
//! 不需要数据库迁移；hCaptcha/Turnstile 的后端代码（captcha.rs）留着不动，不再被调用。

use crate::error::{AppError, AppJson, AppResult};
use crate::state::AppState;
use crate::util;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

/// 挑战 token 有效期（秒），与 hCaptcha 的默认 120 秒一致。
pub const CHALLENGE_TTL_SECS: u64 = 120;
/// pass_token 有效期（秒）。
pub const PASS_TTL_SECS: u64 = 120;
/// 每页目标图数量范围（2~4 张）。
const TARGETS_PER_PAGE_MIN: usize = 2;
const TARGETS_PER_PAGE_MAX: usize = 4;
/// 每页格子数。
pub const CELLS_PER_PAGE: usize = 9;
/// 每道挑战页数。
pub const PAGES_PER_CHALLENGE: usize = 2;
/// 单 IP 10 分钟内最多领题次数（防刷题库）。
const ISSUE_LIMIT_PER_WINDOW: usize = 20;
/// 单 IP 10 分钟内最多答错次数，超了就暂时拒绝。
const FAIL_LIMIT_PER_WINDOW: usize = 5;
const RATE_WINDOW: Duration = Duration::from_secs(600);
/// 一类图片至少要有的目标/干扰图数量（低于这个数该类不启用）。
const MIN_TARGETS_PER_CATEGORY: usize = 10;
const MIN_DISTRACTORS_PER_CATEGORY: usize = 20;

pub const MSG_INVALID: &str = "验证码无效，请再试一次";
pub const MSG_EXPIRED: &str = "验证已过期，请重新验证";
pub const MSG_DISABLED: &str = "人机验证未配置";
pub const MSG_RATE_LIMITED: &str = "验证失败次数过多，请稍后再试";
pub const MSG_ISSUE_LIMITED: &str = "请求过于频繁，请稍后再试";
pub const MSG_WRONG: &str = "验证未通过，已换一题";

/// manifest（images.json）里的一张图。
#[derive(Clone, Debug, Deserialize)]
struct ManifestImage {
    id: String,
    file: String,
    #[serde(default)]
    #[allow(dead_code)]
    title: String,
    #[serde(default)]
    #[allow(dead_code)]
    source: String,
    #[serde(default)]
    #[allow(dead_code)]
    license: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ManifestCategory {
    key: String,
    prompt: String,
    #[serde(default)]
    targets: Vec<ManifestImage>,
    #[serde(default)]
    distractors: Vec<ManifestImage>,
}

#[derive(Clone, Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    categories: Vec<ManifestCategory>,
}

/// 对外暴露的图片：只有不透明 id，没有文件名、标题与分类。
#[derive(Clone, Debug)]
pub struct PublicImage {
    pub id: String,
    pub file: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Category {
    pub key: String,
    pub prompt: String,
    pub targets: Vec<PublicImage>,
    pub distractors: Vec<PublicImage>,
}

#[derive(Clone, Debug)]
struct ChallengeCell {
    id: String,
    target: bool,
}

#[derive(Clone, Debug)]
struct Challenge {
    prompt: String,
    pages: Vec<Vec<ChallengeCell>>,
    created: Instant,
    ip: String,
}

#[derive(Clone, Debug)]
struct PassToken {
    created: Instant,
    ip: String,
}

pub struct SelfCheck {
    categories: Vec<Category>,
    /// 不透明图片 id → 磁盘文件。
    files: HashMap<String, PathBuf>,
    challenges: RwLock<HashMap<String, Challenge>>,
    pass_tokens: RwLock<HashMap<String, PassToken>>,
    issue_log: Mutex<HashMap<String, Vec<Instant>>>,
    fail_log: Mutex<HashMap<String, Vec<Instant>>>,
    ttl: Duration,
    pass_ttl: Duration,
}

#[derive(Debug)]
pub enum VerifyResult {
    /// 答对：返回 pass_token。
    Passed(String),
    /// 答错：返回一道新挑战。
    Failed(Value),
}

impl SelfCheck {
    /// 从环境变量 SELFCHECK_IMAGES_DIR（默认 selfcheck-images）加载图片池。
    /// 加载失败（目录/清单缺失、没有可用类别）时返回一个“未启用”的实例并记 error 日志，
    /// 服务照常启动，验证接口返回 503——绝不因为图片问题拖垮整个服务。
    pub fn load_from_env() -> Self {
        let dir = std::env::var("SELFCHECK_IMAGES_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("selfcheck-images"));
        match Self::load(&dir) {
            Ok(sc) => {
                tracing::info!(
                    dir = %dir.display(),
                    categories = sc.categories.len(),
                    images = sc.files.len(),
                    "自建图片验证已加载"
                );
                sc
            }
            Err(e) => {
                tracing::error!(dir = %dir.display(), error = %e, "自建图片验证加载失败，验证功能将不可用");
                Self::disabled()
            }
        }
    }

    pub fn disabled() -> Self {
        Self {
            categories: Vec::new(),
            files: HashMap::new(),
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(CHALLENGE_TTL_SECS),
            pass_ttl: Duration::from_secs(PASS_TTL_SECS),
        }
    }

    pub fn enabled(&self) -> bool {
        !self.categories.is_empty()
    }

    /// 从目录加载 manifest 并分配不透明图片 id。缺失的图片文件会被跳过并记 warn。
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = dir.join("images.json");
        let raw = std::fs::read_to_string(&manifest_path)
            .map_err(|e| anyhow::anyhow!("读 images.json 失败 {}: {e}", manifest_path.display()))?;
        let manifest: Manifest = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("images.json 解析失败: {e}"))?;

        // 所有图片先打乱再编号：编号顺序与类别、角色都无关，浏览器看不到任何分类信息。
        let mut all: Vec<(String, PathBuf)> = Vec::new(); // (category_key, file)
        for cat in &manifest.categories {
            for img in cat.targets.iter().chain(cat.distractors.iter()) {
                all.push((cat.key.clone(), dir.join(&img.file)));
            }
        }
        let mut rng = rand::thread_rng();
        all.shuffle(&mut rng);
        let mut public_ids: HashMap<(String, String), String> = HashMap::new(); // (category_key, file) -> public id
        let mut files: HashMap<String, PathBuf> = HashMap::new();
        for (i, (key, file)) in all.iter().enumerate() {
            let public_id = format!("img-{:03}", i + 1);
            public_ids.insert((key.clone(), file.to_string_lossy().to_string()), public_id.clone());
            files.insert(public_id, file.clone());
        }

        let mut categories = Vec::new();
        for cat in &manifest.categories {
            let mut targets = Vec::new();
            let mut distractors = Vec::new();
            for img in &cat.targets {
                let file = dir.join(&img.file);
                if !file.is_file() {
                    tracing::warn!(file = %img.file, "图片文件缺失，已跳过");
                    continue;
                }
                let Some(pid) = public_ids.get(&(cat.key.clone(), file.to_string_lossy().to_string())) else {
                    continue;
                };
                targets.push(PublicImage { id: pid.clone(), file });
            }
            for img in &cat.distractors {
                let file = dir.join(&img.file);
                if !file.is_file() {
                    tracing::warn!(file = %img.file, "图片文件缺失，已跳过");
                    continue;
                }
                let Some(pid) = public_ids.get(&(cat.key.clone(), file.to_string_lossy().to_string())) else {
                    continue;
                };
                distractors.push(PublicImage { id: pid.clone(), file });
            }
            if targets.len() < MIN_TARGETS_PER_CATEGORY || distractors.len() < MIN_DISTRACTORS_PER_CATEGORY {
                tracing::warn!(
                    category = %cat.key,
                    targets = targets.len(),
                    distractors = distractors.len(),
                    "图片太少，该类别不启用"
                );
                continue;
            }
            categories.push(Category {
                key: cat.key.clone(),
                prompt: cat.prompt.clone(),
                targets,
                distractors,
            });
        }
        if categories.is_empty() {
            return Err(anyhow::anyhow!("没有可用的图片类别"));
        }
        // 只保留启用类别的图片文件映射。
        let keep: std::collections::HashSet<String> = categories
            .iter()
            .flat_map(|c| c.targets.iter().chain(c.distractors.iter()).map(|i| i.id.clone()))
            .collect();
        files.retain(|id, _| keep.contains(id));
        Ok(Self {
            categories,
            files,
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(CHALLENGE_TTL_SECS),
            pass_ttl: Duration::from_secs(PASS_TTL_SECS),
        })
    }

    /// 测试用：直接用给定的类别构造（不需要磁盘文件）。
    #[cfg(test)]
    pub fn from_categories(categories: Vec<Category>) -> Self {
        let files = categories
            .iter()
            .flat_map(|c| c.targets.iter().chain(c.distractors.iter()))
            .map(|i| (i.id.clone(), i.file.clone()))
            .collect();
        Self {
            categories,
            files,
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(CHALLENGE_TTL_SECS),
            pass_ttl: Duration::from_secs(PASS_TTL_SECS),
        }
    }

    #[cfg(test)]
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self.pass_ttl = ttl;
        self
    }

    fn prune_expired(&self, now: Instant) {
        if let Ok(mut ch) = self.challenges.write() {
            ch.retain(|_, c| now.duration_since(c.created) <= self.ttl);
        }
        if let Ok(mut pt) = self.pass_tokens.write() {
            pt.retain(|_, p| now.duration_since(p.created) <= self.pass_ttl);
        }
    }

    fn record_and_check(log: &Mutex<HashMap<String, Vec<Instant>>>, key: &str, limit: usize, now: Instant) -> bool {
        let mut map = log.lock().unwrap_or_else(|e| e.into_inner());
        let entries = map.entry(key.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) <= RATE_WINDOW);
        if entries.len() >= limit {
            return false;
        }
        entries.push(now);
        true
    }

    fn failures_in_window(&self, ip: &str, now: Instant) -> usize {
        let mut map = self.fail_log.lock().unwrap_or_else(|e| e.into_inner());
        let entries = map.entry(ip.to_string()).or_default();
        entries.retain(|t| now.duration_since(*t) <= RATE_WINDOW);
        entries.len()
    }

    fn challenge_json(&self, token: &str, ch: &Challenge) -> Value {
        let pages: Vec<Value> = ch
            .pages
            .iter()
            .map(|page| {
                Value::Array(
                    page.iter()
                        .map(|cell| json!({ "id": cell.id, "url": format!("/api/selfcheck/img/{}", cell.id) }))
                        .collect(),
                )
            })
            .collect();
        json!({
            "token": token,
            "prompt": ch.prompt,
            "pages": pages,
            "expires_in": self.ttl.as_secs(),
        })
    }

    /// 发一道新挑战。同一个 IP 领新题时，之前没用完的旧题一并作废（换题后旧 token 失效）。
    pub fn issue(&self, ip: &str) -> AppResult<Value> {
        if !self.enabled() {
            return Err(AppError::new(StatusCode::SERVICE_UNAVAILABLE, "captcha_disabled", MSG_DISABLED));
        }
        let now = Instant::now();
        self.prune_expired(now);
        if !Self::record_and_check(&self.issue_log, ip, ISSUE_LIMIT_PER_WINDOW, now) {
            return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", MSG_ISSUE_LIMITED));
        }
        let mut rng = rand::thread_rng();
        let cat = self.categories.choose(&mut rng).expect("enabled 时类别非空");
        // 两页共用抽样池：先打乱目标/干扰池，两页按顺序取，图绝不重复。
        let mut targets: Vec<&PublicImage> = cat.targets.iter().collect();
        let mut distractors: Vec<&PublicImage> = cat.distractors.iter().collect();
        targets.shuffle(&mut rng);
        distractors.shuffle(&mut rng);
        let mut t_iter = targets.into_iter();
        let mut d_iter = distractors.into_iter();
        let mut pages: Vec<Vec<ChallengeCell>> = Vec::with_capacity(PAGES_PER_CHALLENGE);
        for _ in 0..PAGES_PER_CHALLENGE {
            let n_targets = rng.gen_range(TARGETS_PER_PAGE_MIN..=TARGETS_PER_PAGE_MAX);
            let mut cells: Vec<ChallengeCell> = Vec::with_capacity(CELLS_PER_PAGE);
            for img in t_iter.by_ref().take(n_targets) {
                cells.push(ChallengeCell { id: img.id.clone(), target: true });
            }
            for img in d_iter.by_ref().take(CELLS_PER_PAGE - n_targets) {
                cells.push(ChallengeCell { id: img.id.clone(), target: false });
            }
            cells.shuffle(&mut rng);
            pages.push(cells);
        }
        let token = util::random_token(24);
        let challenge = Challenge { prompt: cat.prompt.clone(), pages, created: now, ip: ip.to_string() };
        let body = self.challenge_json(&token, &challenge);
        if let Ok(mut ch) = self.challenges.write() {
            // 同 IP 的旧题作废。
            ch.retain(|_, c| c.ip != ip);
            ch.insert(token, challenge);
        }
        Ok(body)
    }

    /// 校验答案。token 无论对错都会被消费（一次有效）；答错时直接发一道新题。
    pub fn verify(&self, ip: &str, token: &str, selections: &[Vec<bool>]) -> AppResult<VerifyResult> {
        if !self.enabled() {
            return Err(AppError::new(StatusCode::SERVICE_UNAVAILABLE, "captcha_disabled", MSG_DISABLED));
        }
        let now = Instant::now();
        if self.failures_in_window(ip, now) >= FAIL_LIMIT_PER_WINDOW {
            return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", MSG_RATE_LIMITED));
        }
        if selections.len() != PAGES_PER_CHALLENGE
            || selections.iter().any(|p| p.len() != CELLS_PER_PAGE)
        {
            return Err(AppError::bad_request("答案格式不正确"));
        }
        let challenge = {
            let mut ch = self.challenges.write().unwrap_or_else(|e| e.into_inner());
            match ch.remove(token) {
                Some(c) => c,
                None => return Err(AppError::bad_request(MSG_INVALID)),
            }
        };
        if challenge.ip != ip {
            return Err(AppError::bad_request(MSG_INVALID));
        }
        if now.duration_since(challenge.created) > self.ttl {
            return Err(AppError::bad_request(MSG_EXPIRED));
        }
        let correct = challenge.pages.iter().zip(selections.iter()).all(|(page, sel)| {
            page.iter().zip(sel.iter()).all(|(cell, s)| cell.target == *s)
        });
        if !correct {
            // 答错计一次失败；到上限就不再发新题，直接限流。
            Self::record_and_check(&self.fail_log, ip, FAIL_LIMIT_PER_WINDOW, now);
            if self.failures_in_window(ip, now) >= FAIL_LIMIT_PER_WINDOW {
                return Err(AppError::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", MSG_RATE_LIMITED));
            }
            let fresh = self.issue(ip)?;
            return Ok(VerifyResult::Failed(fresh));
        }
        let pass_token = util::random_token(24);
        if let Ok(mut pt) = self.pass_tokens.write() {
            pt.insert(pass_token.clone(), PassToken { created: now, ip: ip.to_string() });
        }
        Ok(VerifyResult::Passed(pass_token))
    }

    /// 签到/补签消费 pass_token：一次有效、120 秒过期、绑定发题 IP。
    pub fn consume_pass_token(&self, token: Option<&str>, remote_ip: &str) -> AppResult<()> {
        let token = token.unwrap_or("").trim();
        if token.is_empty() {
            return Err(AppError::bad_request(MSG_INVALID));
        }
        let now = Instant::now();
        let pass = {
            let mut pt = self.pass_tokens.write().unwrap_or_else(|e| e.into_inner());
            match pt.remove(token) {
                Some(p) => p,
                None => return Err(AppError::bad_request(MSG_INVALID)),
            }
        };
        if pass.ip != remote_ip {
            return Err(AppError::bad_request(MSG_INVALID));
        }
        if now.duration_since(pass.created) > self.pass_ttl {
            return Err(AppError::bad_request(MSG_EXPIRED));
        }
        Ok(())
    }
}

/// GET /api/selfcheck/challenge
pub async fn challenge(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    let ip = util::client_ip(&headers);
    Ok(Json(state.selfcheck.issue(&ip)?))
}

#[derive(Debug, Deserialize)]
pub struct VerifyBody {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub selections: Vec<Vec<bool>>,
}

/// POST /api/selfcheck/verify
pub async fn verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    AppJson(body): AppJson<VerifyBody>,
) -> AppResult<Json<Value>> {
    let ip = util::client_ip(&headers);
    match state.selfcheck.verify(&ip, &body.token, &body.selections)? {
        crate::selfcheck::VerifyResult::Passed(pass_token) => {
            Ok(Json(json!({ "ok": true, "pass_token": pass_token })))
        }
        crate::selfcheck::VerifyResult::Failed(fresh) => Ok(Json(json!({
            "ok": false,
            "message": MSG_WRONG,
            "challenge": fresh,
        }))),
    }
}

/// GET /api/selfcheck/img/{id}：按不透明 id 白名单提供图片，不暴露真实文件名。
pub async fn image(State(state): State<AppState>, axum::extract::Path(id): axum::extract::Path<String>) -> AppResult<Response> {
    let sc = &state.selfcheck;
    let Some(path) = sc.files.get(&id) else {
        return Err(AppError::not_found("图片不存在"));
    };
    let bytes = tokio::fs::read(path).await.map_err(|_| AppError::not_found("图片不存在"))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        bytes,
    )
        .into_response())
}

/// 签到/补签前调用：校验并一次消费 pass_token。
pub async fn check_pass_token(state: &AppState, token: Option<&str>, remote_ip: &str) -> AppResult<()> {
    state.selfcheck.consume_pass_token(token, remote_ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_category(key: &str) -> Category {
        let mk = |prefix: &str, n: usize| {
            (0..n)
                .map(|i| PublicImage {
                    id: format!("{prefix}-{i}"),
                    file: PathBuf::from(format!("{prefix}-{i}.jpg")),
                })
                .collect::<Vec<_>>()
        };
        Category {
            key: key.to_string(),
            prompt: "找出所有目标".to_string(),
            targets: mk("t", 30),
            distractors: mk("d", 60),
        }
    }

    fn selections_for(ch: &Challenge) -> Vec<Vec<bool>> {
        ch.pages
            .iter()
            .map(|page| page.iter().map(|c| c.target).collect())
            .collect()
    }

    #[test]
    fn challenge_pages_have_no_repeated_images() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        for _ in 0..20 {
            let body = sc.issue("1.2.3.4").unwrap();
            let pages = body["pages"].as_array().unwrap();
            assert_eq!(pages.len(), 2);
            let mut seen = std::collections::HashSet::new();
            for page in pages {
                let cells = page.as_array().unwrap();
                assert_eq!(cells.len(), 9);
                for cell in cells {
                    let id = cell["id"].as_str().unwrap();
                    assert!(seen.insert(id.to_string()), "图片重复出现: {id}");
                    assert!(cell["url"].as_str().unwrap().starts_with("/api/selfcheck/img/"));
                }
            }
            assert_eq!(seen.len(), 18);
        }
    }

    #[test]
    fn challenge_targets_per_page_in_range() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        for _ in 0..20 {
            let body = sc.issue("1.2.3.4").unwrap();
            let token = body["token"].as_str().unwrap().to_string();
            let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
            for page in &ch.pages {
                let n = page.iter().filter(|c| c.target).count();
                assert!((TARGETS_PER_PAGE_MIN..=TARGETS_PER_PAGE_MAX).contains(&n), "每页目标数 {n}");
            }
        }
    }

    #[test]
    fn correct_answer_passes_and_pass_token_is_single_use() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let sel = selections_for(&ch);
        let pass = match sc.verify("1.2.3.4", &token, &sel).unwrap() {
            VerifyResult::Passed(p) => p,
            VerifyResult::Failed(_) => panic!("正确答案应通过"),
        };
        assert!(sc.consume_pass_token(Some(&pass), "1.2.3.4").is_ok());
        // pass_token 只能消费一次。
        assert!(sc.consume_pass_token(Some(&pass), "1.2.3.4").is_err());
    }

    #[test]
    fn wrong_answer_fails_and_issues_new_challenge() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let mut sel = selections_for(&ch);
        // 漏选第一个目标（一定存在，因为每页至少 2 个目标）。
        let first_target = ch.pages[0].iter().position(|c| c.target).unwrap();
        sel[0][first_target] = false;
        match sc.verify("1.2.3.4", &token, &sel).unwrap() {
            VerifyResult::Passed(_) => panic!("漏选应失败"),
            VerifyResult::Failed(fresh) => {
                assert_ne!(fresh["token"].as_str().unwrap(), token);
                assert!(fresh["pages"].is_array());
            }
        }
        // 旧 token 已被消费，再验应拒绝。
        assert!(sc.verify("1.2.3.4", &token, &sel).is_err());
    }

    #[test]
    fn extra_selection_fails() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let mut sel = selections_for(&ch);
        let first_distractor = ch.pages[0].iter().position(|c| !c.target).unwrap();
        sel[0][first_distractor] = true;
        assert!(matches!(
            sc.verify("1.2.3.4", &token, &sel).unwrap(),
            VerifyResult::Failed(_)
        ));
    }

    #[test]
    fn expired_token_rejected() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]).with_ttl(Duration::from_millis(1));
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        std::thread::sleep(Duration::from_millis(5));
        let sel = selections_for(&ch);
        let err = sc.verify("1.2.3.4", &token, &sel).unwrap_err();
        assert_eq!(err.message, MSG_EXPIRED);
    }

    #[test]
    fn used_token_rejected() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let sel = selections_for(&ch);
        assert!(matches!(
            sc.verify("1.2.3.4", &token, &sel).unwrap(),
            VerifyResult::Passed(_)
        ));
        // 同一 token 再验一次（即使答案正确）也应拒绝。
        let err = sc.verify("1.2.3.4", &token, &sel).unwrap_err();
        assert_eq!(err.message, MSG_INVALID);
    }

    #[test]
    fn refresh_invalidates_old_token() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let first = sc.issue("1.2.3.4").unwrap();
        let first_token = first["token"].as_str().unwrap().to_string();
        // 同一 IP 换一题。
        let second = sc.issue("1.2.3.4").unwrap();
        assert_ne!(second["token"].as_str().unwrap(), first_token);
        let ch = sc.challenges.read().unwrap().get(second["token"].as_str().unwrap()).unwrap().clone();
        let sel = selections_for(&ch);
        // 旧 token 已作废。
        let err = sc.verify("1.2.3.4", &first_token, &sel).unwrap_err();
        assert_eq!(err.message, MSG_INVALID);
        // 新 token 正常可用。
        assert!(matches!(
            sc.verify("1.2.3.4", second["token"].as_str().unwrap(), &sel).unwrap(),
            VerifyResult::Passed(_)
        ));
    }

    #[test]
    fn repeated_failures_rate_limited() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        for _ in 0..FAIL_LIMIT_PER_WINDOW {
            let body = sc.issue("9.9.9.9").unwrap();
            let token = body["token"].as_str().unwrap().to_string();
            let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
            let mut sel = selections_for(&ch);
            sel[0][0] = !sel[0][0]; // 故意答错
            let _ = sc.verify("9.9.9.9", &token, &sel);
        }
        // 第 6 次验证（即使拿新题）也应被限流。
        let body = sc.issue("9.9.9.9").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let sel = selections_for(&ch);
        let err = sc.verify("9.9.9.9", &token, &sel).unwrap_err();
        assert_eq!(err.code, "rate_limited");
    }

    #[test]
    fn pass_token_bound_to_ip() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = sc.challenges.read().unwrap().get(&token).unwrap().clone();
        let sel = selections_for(&ch);
        let pass = match sc.verify("1.2.3.4", &token, &sel).unwrap() {
            VerifyResult::Passed(p) => p,
            VerifyResult::Failed(_) => panic!("应通过"),
        };
        // 换 IP 消费应拒绝。
        assert!(sc.consume_pass_token(Some(&pass), "5.6.7.8").is_err());
    }

    #[test]
    fn bad_selection_shape_rejected() {
        let sc = SelfCheck::from_categories(vec![test_category("c1")]);
        let body = sc.issue("1.2.3.4").unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        assert!(sc.verify("1.2.3.4", &token, &vec![vec![true; 9]]).is_err());
        assert!(sc.verify("1.2.3.4", &token, &vec![vec![true; 8], vec![true; 9]]).is_err());
    }
}
