//! 滑块（拼图）人机验证（2026-11-15）：替代 2026-11-14 的 3×3 图片选图验证（selfcheck.rs）。
//!
//! 诚实定位（必读）：滑块 + 轨迹分析比图片选图多了一层行为信号，但它挡不住有决心的攻击者。
//! 研究依据见 `~/workspace/pmplan-slider/research/report.md`（Guo et al., arXiv:2611.11355，2026-11）：
//! 手写 2-D sigmoid + ±3px Y 抖动对六家商业滑块验证码的成功率是 99.9%，GAN 生成轨迹 99.8%，
//! 直接回放一段录制好的人类轨迹 99.4%。轨迹形状检查无法区分“回放的人类拖动”和“现场的人类拖动”
//! ——轨迹本身就是真的。真正的防护是 LinuxDo 登录 + 限流；这个滑块只是减速带，
//! 不要在代码注释、前端文案或任何报告里宣称它能挡住机器人。
//!
//! 流程：
//! - `GET  /api/slider/challenge` → 发一道挑战：随机背景图（复用 selfcheck 的 210 张
//!   Wikimedia Commons CC/PD 图片池中足够大的那些），随机缺口位置，随机拼图块；
//!   背景图上缺口处已挖空变暗，拼图块单独提供；返回一次性 token（120 秒有效）。
//! - `POST /api/slider/verify` `{token, offset, points: [{x, y, t}], start_delay_ms, pointer_type}`
//!   → 服务端校验落点（见下）+ 轨迹评分（R1–R13），通过返回一次性 pass_token（120 秒有效），
//!   失败则 token 作废并直接带一道新挑战。
//! - 签到/补签：请求体里的 `captcha_token` 是 pass_token，服务端校验并一次消费（与旧版相同语义）。
//!
//! 缺口位置只存在服务端，绝不发给前端。前端只拿到：背景图（缺口可见）、拼图块、拼图块的
//! 垂直位置 gap_y、以及按钮/拼图位移比 k（见下）。注意：拼图块本身就是从背景上裁下来的，
//! 攻击者可以用模板匹配在背景里定位缺口——这是所有拼图验证码共有的弱点，k 只是让“找到缺口”
//! 不直接等于“知道按钮要拖多远”。
//!
//! 按钮/拼图位移比 k（报告 §5.4 的公开防御，Guo et al. 2026 §VI）：每道题随机取 k ∈ [0.8, 1.2]，
//! 前端按 piece_x = button_dx × k 渲染拼图块，服务端按 expected_button_dx = gap_x / k 校验。
//! k 放在 challenge payload 里，前端渲染必须用它，所以攻击者也能读到它——它只是轻微防御，
//! 诚实记录于此，不要夸大。
//!
//! 轨迹规则 R1–R13（全部出自研究报告 §4；凡是标注“proposed, needs calibration”的阈值，
//! 都是没有已发表截断值的起点猜测，代码注释里逐一标明来源，绝不在任何单个评分特征上硬拒用户）：
//! - R1 硬拒：拖动时长 < 200 ms（S13 厂商说法，弱证据；S1 方法 1：“人类不可能在 0.1 秒内直线滑到缺口”）。
//! - R2 硬拒：轨迹点 < 4 个；时间戳非单调 / Δt ≤ 0；全部 Δt 完全相同（精确到毫秒）。
//!   真实 pointermove 投递是不规则的（报告 §4 R2；“全部相同”这一条是 proposed, needs calibration）。
//! - R3–R11 加权评分（每项过关 +1，要求 ≥ 60% 适用分；触屏放宽到 ≥ 50%，见下）：
//!   R3 Y 方向位移 ≥ 2 px（proposed；S1 的有效抖动是 ±3 px）；
//!   R4 速度变异系数 CV ≥ 0.5 且峰值速度出现在拖动时长 20%–60% 处
//!   （峰值位置 30–40% 来自 S15；CV 数值是 proposed, needs calibration）；
//!   R5 速度曲线临界点 2–4 个为理想，≥ 6 判过抖（人类 3–4 个，S7/S15；2/6 截断是 proposed）；
//!   R6 超调 1–15 px 后回拉是加分项不是必备项（仅 ~15% 的人类试验会超调，S15）；
//!   R7 初始加速阶段（首次速度峰值）在时长 10%–60% 处（S6 认为加速阶段时长是最具信息量的特征，
//!   但只给了排序没给数值；10%–60% 是 proposed）；jerk 标准差只记录不评分（S6 同样只给排序）；
//!   R8 至少一次停顿（位移 < 1 px 持续 ≥ 50 ms）+ 释放前 100 ms 内速度已降到峰值一半以下
//!   （50 ms / 100 ms 是 proposed）；
//!   R9 直线度 > 0.995 且 Y 位移 < 2 px 时扣分（0.995 是 proposed；S16）；
//!   R10 开始延迟 ≥ 100 ms（100 ms 是 proposed；S14 的 1–5 s 是未验证的厂商说法）；
//!   R11 距离 > 150 px 且时长 < 300 ms 时扣分（proposed；基本被 R1 覆盖，作交叉检查）。
//! - R12 硬拒：一次性 token + 120 秒过期 + 单次使用；拒绝与之前已通过的轨迹完全相同
//!   或粗量化后相同的轨迹（回放攻击 S1 方法 8 成功率 99.4%，这是唯一针对回放的防线，
//!   但攻击者可以录很多条按距离挑着用，所以不要宣称能防回放）。
//! - R13 限流：沿用 selfcheck 的数字，单 IP 10 分钟内最多领 20 题 / 答错 5 次 → 429。
//! - 键盘操作（无障碍）：pointer_type 为 "keyboard" 时只验落点、不评轨迹——键盘拖动没有
//!   pointermove 轨迹可供评分。这条路径比鼠标/触屏弱，是有意为之的取舍；攻击者本就可以
//!   回放人类轨迹，所以它没有实质性降低本就很弱的屏障。日志里 verdict 会标明 keyboard。
//!
//! 所有拖动从第一天起记日志：`slider_attempts` 表（migration 0009）存原始轨迹点、
//! 逐项特征值、 verdict、pointer 类型、挑战几何（含 k）。阈值日后用真实数据校准。
//! 客户端永远只收到通用失败文案，绝不透露哪一项特征没过。

use crate::error::{AppError, AppJson, AppResult};
use crate::state::AppState;
use crate::util;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use image::imageops::FilterType;
use image::{ImageBuffer, Rgb};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

/// 挑战 token 有效期（秒），与 hCaptcha 默认的 120 秒一致，也与旧 selfcheck 相同。
pub const CHALLENGE_TTL_SECS: u64 = 120;
/// pass_token 有效期（秒）。
pub const PASS_TTL_SECS: u64 = 120;
/// 画布尺寸（服务端坐标系；前端按显示宽度等比换算后上报）。
pub const CANVAS_W: u32 = 320;
pub const CANVAS_H: u32 = 200;
/// 拼图块边长（px）。
pub const PIECE: u32 = 46;
/// 缺口横坐标范围：保证按钮行程在任何合理显示宽度下都够得着（见前端换算注释）。
const GAP_X_MIN: u32 = 60;
const GAP_X_MAX: u32 = 200;
/// 缺口纵坐标范围。
const GAP_Y_MIN: u32 = 12;
const GAP_Y_MAX: u32 = CANVAS_H - PIECE - 12;
/// 按钮/拼图位移比 k 的随机范围（报告 §5.4；k 在 payload 里可见，只是轻微防御）。
const K_MIN: f64 = 0.8;
const K_MAX: f64 = 1.2;
/// 落点容差（画布坐标 px）。拼图块 46 px 宽，人类微调一般落在 ±2–4 px 内；
/// ±5 px 约为块宽的 11%，随机猜中概率约 5%（缺口横向范围约 140 px），再由限流兜底。
/// 该值是起点猜测（proposed, needs calibration），日后按 slider_attempts 的数据调。
const OFFSET_TOLERANCE_PX: f64 = 5.0;
/// 单 IP 10 分钟内最多领题次数（沿用 selfcheck）。
const ISSUE_LIMIT_PER_WINDOW: usize = 20;
/// 单 IP 10 分钟内最多答错次数（沿用 selfcheck）。
const FAIL_LIMIT_PER_WINDOW: usize = 5;
const RATE_WINDOW: Duration = Duration::from_secs(600);
/// 轨迹点数量上限（防超大请求体撑内存）。
const MAX_POINTS: usize = 1000;
/// 评分：适用分 10 分（R3、R4×2、R5、R7、R8×2、R9、R10、R11），R6 超调为额外 +1 分。
const SCORE_MAX: i32 = 10;
/// 鼠标/笔要求 ≥ 60%（报告 §4 评分建议）。
const SCORE_PASS_MOUSE: i32 = 6;
/// 触屏放宽到 ≥ 50%：报告 §5 明确指出没有可用的触屏轨迹数据，触屏拖动通常更平滑、更短，
/// 用桌面鼠标数据调出的阈值可能误拒手机用户。该值是 proposed, needs calibration。
const SCORE_PASS_TOUCH: i32 = 5;
/// 已通过轨迹的去重指纹最多保留条数（防内存无限增长）。
const ACCEPTED_HASHES_CAP: usize = 10_000;
/// 背景图做画布的最低原始尺寸：400 px 宽、≥ 280 px 高（池里 210 张中有 128 张满足）。
const MIN_SRC_W: u32 = 400;
const MIN_SRC_H: u32 = 280;

pub const MSG_INVALID: &str = "验证码无效，请再试一次";
pub const MSG_EXPIRED: &str = "验证已过期，请重新验证";
pub const MSG_DISABLED: &str = "人机验证未配置";
pub const MSG_RATE_LIMITED: &str = "验证失败次数过多，请稍后再试";
pub const MSG_ISSUE_LIMITED: &str = "请求过于频繁，请稍后再试";
/// 通用失败文案：绝不透露哪一项特征没过（R1–R11 的具体原因只写日志和数据库）。
pub const MSG_WRONG: &str = "验证未通过，已换一题";

/// 前端上报的单个轨迹点：x/y 为画布坐标 px（前端按显示宽度等比换算），t 为按下后毫秒数。
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct TrajPoint {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub t: i64,
}

#[derive(Clone, Debug, Deserialize)]
struct ManifestImage {
    file: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ManifestCategory {
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

#[derive(Clone, Debug)]
struct Challenge {
    gap_x: f64,
    gap_y: u32,
    k: f64,
    created: Instant,
    ip: String,
}

#[derive(Clone, Debug)]
struct PassToken {
    created: Instant,
    ip: String,
}

/// 一次验证的评分报告：同时用于日志（tracing）和 `slider_attempts` 表。
#[derive(Clone, Debug, Serialize)]
pub struct FeatureReport {
    pub verdict: String,
    pub offset: Option<f64>,
    pub expected_offset: Option<f64>,
    pub gap_x: Option<f64>,
    pub k: Option<f64>,
    pub duration_ms: i64,
    pub point_count: usize,
    pub score: i32,
    pub score_max: i32,
    pub pointer_type: String,
    pub start_delay_ms: i64,
    pub hard_fail: Option<String>,
    // 逐项特征值（供日后校准；阈值见模块头注释）。
    pub y_range: f64,
    pub vel_cv: f64,
    pub peak_pos: f64,
    pub critical_points: usize,
    pub overshoot_px: f64,
    pub jerk_std: f64,
    pub accel_phase_frac: f64,
    pub pauses: usize,
    pub release_dwell_ok: bool,
    pub straightness: f64,
    pub points: Value,
}

impl FeatureReport {
    fn empty(verdict: &str, pointer_type: &str, start_delay_ms: i64) -> Self {
        Self {
            verdict: verdict.to_string(),
            offset: None,
            expected_offset: None,
            gap_x: None,
            k: None,
            duration_ms: 0,
            point_count: 0,
            score: 0,
            score_max: SCORE_MAX,
            pointer_type: pointer_type.to_string(),
            start_delay_ms,
            hard_fail: None,
            y_range: 0.0,
            vel_cv: 0.0,
            peak_pos: 0.0,
            critical_points: 0,
            overshoot_px: 0.0,
            jerk_std: 0.0,
            accel_phase_frac: 0.0,
            pauses: 0,
            release_dwell_ok: false,
            straightness: 1.0,
            points: Value::Array(vec![]),
        }
    }
}

#[derive(Debug)]
pub enum VerifyResult {
    /// 通过：返回 pass_token + 评分报告（供日志）。
    Passed(String, FeatureReport),
    /// 失败：返回一道新挑战 + 评分报告（供日志）。
    Failed(Value, FeatureReport),
}

pub struct Slider {
    canvases: Vec<ImageBuffer<Rgb<u8>, Vec<u8>>>,
    challenges: RwLock<HashMap<String, Challenge>>,
    pass_tokens: RwLock<HashMap<String, PassToken>>,
    issue_log: Mutex<HashMap<String, Vec<Instant>>>,
    fail_log: Mutex<HashMap<String, Vec<Instant>>>,
    /// 已通过轨迹的（精细指纹，粗指纹），用于 R12 回放去重。
    accepted_hashes: Mutex<Vec<(String, String)>>,
    ttl: Duration,
    pass_ttl: Duration,
}

impl Slider {
    /// 从 SELFCHECK_IMAGES_DIR（默认 selfcheck-images，与 selfcheck 共用图片池）加载背景图。
    /// 只用原始尺寸 ≥ 400×280 的图，等比缩到 320 宽、居中裁成 320×200 画布。
    /// 加载失败时返回“未启用”实例并记 error 日志，服务照常启动，验证接口返回 503。
    pub fn load_from_env() -> Self {
        let dir = std::env::var("SELFCHECK_IMAGES_DIR")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("selfcheck-images"));
        match Self::load(&dir) {
            Ok(s) => {
                tracing::info!(dir = %dir.display(), backgrounds = s.canvases.len(), "滑块拼图验证已加载");
                s
            }
            Err(e) => {
                tracing::error!(dir = %dir.display(), error = %e, "滑块拼图验证加载失败，验证功能将不可用");
                Self::disabled()
            }
        }
    }

    pub fn disabled() -> Self {
        Self {
            canvases: Vec::new(),
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            accepted_hashes: Mutex::new(Vec::new()),
            ttl: Duration::from_secs(CHALLENGE_TTL_SECS),
            pass_ttl: Duration::from_secs(PASS_TTL_SECS),
        }
    }

    pub fn enabled(&self) -> bool {
        !self.canvases.is_empty()
    }

    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = dir.join("images.json");
        let raw = std::fs::read_to_string(&manifest_path)
            .map_err(|e| anyhow::anyhow!("读 images.json 失败 {}: {e}", manifest_path.display()))?;
        let manifest: Manifest = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("images.json 解析失败: {e}"))?;
        let mut canvases = Vec::new();
        let mut skipped = 0usize;
        for cat in &manifest.categories {
            for img in cat.targets.iter().chain(cat.distractors.iter()) {
                let file = dir.join(&img.file);
                let Ok(decoded) = image::open(&file) else {
                    skipped += 1;
                    continue;
                };
                if decoded.width() < MIN_SRC_W || decoded.height() < MIN_SRC_H {
                    skipped += 1;
                    continue;
                }
                let rgb = decoded.to_rgb8();
                // 等比缩到 320 宽，再居中裁 200 高。
                let scaled_h = (decoded.height() * CANVAS_W / decoded.width()).max(CANVAS_H);
                let resized =
                    image::imageops::resize(&rgb, CANVAS_W, scaled_h, FilterType::Triangle);
                let y0 = (scaled_h - CANVAS_H) / 2;
                let canvas =
                    image::imageops::crop_imm(&resized, 0, y0, CANVAS_W, CANVAS_H).to_image();
                canvases.push(canvas);
            }
        }
        tracing::info!(loaded = canvases.len(), skipped, "滑块背景图加载完成");
        if canvases.is_empty() {
            return Err(anyhow::anyhow!("没有可用的滑块背景图"));
        }
        Ok(Self {
            canvases,
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            accepted_hashes: Mutex::new(Vec::new()),
            ttl: Duration::from_secs(CHALLENGE_TTL_SECS),
            pass_ttl: Duration::from_secs(PASS_TTL_SECS),
        })
    }

    /// 测试用：直接用给定的画布构造。
    #[cfg(test)]
    pub fn from_canvases(canvases: Vec<ImageBuffer<Rgb<u8>, Vec<u8>>>) -> Self {
        Self {
            canvases,
            challenges: RwLock::new(HashMap::new()),
            pass_tokens: RwLock::new(HashMap::new()),
            issue_log: Mutex::new(HashMap::new()),
            fail_log: Mutex::new(HashMap::new()),
            accepted_hashes: Mutex::new(Vec::new()),
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

    fn record_and_check(
        log: &Mutex<HashMap<String, Vec<Instant>>>,
        key: &str,
        limit: usize,
        now: Instant,
    ) -> bool {
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

    /// 生成一题的图片：背景（缺口处变暗 + 白边）与单独裁下的拼图块，JPEG 编码后包成 data URI。
    fn render_challenge_images(
        &self,
        canvas: &ImageBuffer<Rgb<u8>, Vec<u8>>,
        gap_x: u32,
        gap_y: u32,
    ) -> (String, String) {
        let piece = image::imageops::crop_imm(canvas, gap_x, gap_y, PIECE, PIECE).to_image();
        let mut bg = canvas.clone();
        // 缺口变暗到 30% 亮度，外圈 2 px 白边，让缺口肉眼可见。
        for y in gap_y..gap_y + PIECE {
            for x in gap_x..gap_x + PIECE {
                let border = x < gap_x + 2
                    || x >= gap_x + PIECE - 2
                    || y < gap_y + 2
                    || y >= gap_y + PIECE - 2;
                let p = bg.get_pixel_mut(x, y);
                if border {
                    *p = Rgb([235, 235, 235]);
                } else {
                    p.0 = [
                        (p.0[0] as u16 * 30 / 100) as u8,
                        (p.0[1] as u16 * 30 / 100) as u8,
                        (p.0[2] as u16 * 30 / 100) as u8,
                    ];
                }
            }
        }
        (encode_jpeg(&bg, 72), encode_jpeg(&piece, 80))
    }

    fn challenge_json(&self, token: &str, ch: &Challenge, bg: &str, piece: &str) -> Value {
        json!({
            "token": token,
            "bg": bg,
            "piece": piece,
            "piece_w": PIECE,
            "piece_h": PIECE,
            "canvas_w": CANVAS_W,
            "canvas_h": CANVAS_H,
            // 只给纵坐标（拼图块垂直位置前端渲染必需）；横坐标 gap_x 绝不发送。
            "gap_y": ch.gap_y,
            // k 在 payload 里可见（前端渲染必需），只是轻微防御，见模块头注释。
            "k": ch.k,
            "expires_in": self.ttl.as_secs(),
        })
    }

    /// 发一道新挑战。同一个 IP 领新题时，之前没用完的旧题一并作废（换题后旧 token 失效）。
    pub fn issue(&self, ip: &str) -> AppResult<Value> {
        if !self.enabled() {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "captcha_disabled",
                MSG_DISABLED,
            ));
        }
        let now = Instant::now();
        self.prune_expired(now);
        if !Self::record_and_check(&self.issue_log, ip, ISSUE_LIMIT_PER_WINDOW, now) {
            return Err(AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                MSG_ISSUE_LIMITED,
            ));
        }
        let mut rng = rand::thread_rng();
        let canvas = self
            .canvases
            .get(rng.gen_range(0..self.canvases.len()))
            .expect("enabled 时背景图非空");
        let gap_x = rng.gen_range(GAP_X_MIN..=GAP_X_MAX);
        let gap_y = rng.gen_range(GAP_Y_MIN..=GAP_Y_MAX);
        // k ∈ [0.8, 1.2]：piece_x = button_dx × k，服务端按 gap_x / k 校验按钮位移。
        let k = rng.gen_range(K_MIN..=K_MAX);
        let (bg, piece) = self.render_challenge_images(canvas, gap_x, gap_y);
        let token = util::random_token(24);
        let challenge = Challenge {
            gap_x: gap_x as f64,
            gap_y,
            k,
            created: now,
            ip: ip.to_string(),
        };
        let body = self.challenge_json(&token, &challenge, &bg, &piece);
        if let Ok(mut ch) = self.challenges.write() {
            // 同 IP 的旧题作废。
            ch.retain(|_, c| c.ip != ip);
            ch.insert(token, challenge);
        }
        Ok(body)
    }
}

fn encode_jpeg(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, quality: u8) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    // 编码失败时返回空 data URI（几乎不可能发生）；调用方不因此崩溃。
    if enc.encode_image(img).is_ok() {
        format!("data:image/jpeg;base64,{}", STANDARD.encode(&buf))
    } else {
        String::new()
    }
}

/// 轨迹指纹（R12 回放去重）：精细版按 2 px / 25 ms 量化，粗版取 8 个等距采样点按 5 px / 50 ms
/// 量化。精确相同的回放一定命中；加了微小噪声的回放大概率命中粗指纹。
/// 容差本身是 proposed, needs calibration；误判的代价只是“换一题再拖一次”。
fn trajectory_hashes(points: &[TrajPoint]) -> (String, String) {
    let mut fine = String::new();
    for p in points {
        fine.push_str(&format!(
            "{}:{}:{};",
            (p.x / 2.0).round() as i64,
            (p.y / 2.0).round() as i64,
            p.t / 25
        ));
    }
    let mut coarse = String::new();
    if !points.is_empty() {
        for i in 0..8 {
            let idx = i * (points.len() - 1) / 7;
            let p = points[idx];
            coarse.push_str(&format!(
                "{}:{}:{};",
                (p.x / 5.0).round() as i64,
                (p.y / 5.0).round() as i64,
                p.t / 50
            ));
        }
    }
    let hash = |s: &str| {
        let mut h = Sha256::new();
        h.update(s.as_bytes());
        hex::encode(h.finalize())
    };
    (hash(&fine), hash(&coarse))
}

/// 分析一条拖动轨迹，返回逐项特征 + 加权评分（R3–R11，见模块头注释）。
/// 只有 R1/R2 这类物理上不可能的拖动才在这里之外硬拒；本函数只做评分，不直接否决用户。
pub fn analyze_trajectory(
    points: &[TrajPoint],
    start_delay_ms: i64,
    pointer_type: &str,
) -> FeatureReport {
    let mut rep = FeatureReport::empty("scored", pointer_type, start_delay_ms);
    rep.point_count = points.len();
    rep.points = serde_json::to_value(points).unwrap_or(Value::Array(vec![]));
    if points.len() < 2 {
        return rep;
    }
    let duration_ms = points.last().unwrap().t - points.first().unwrap().t;
    rep.duration_ms = duration_ms;
    if duration_ms <= 0 {
        return rep;
    }
    let duration = duration_ms as f64;

    // 逐段速度（x 方向带符号的用于超调，绝对值用于速度曲线）。
    let mut speeds: Vec<f64> = Vec::with_capacity(points.len() - 1); // |vx| px/ms
    let mut step_dt: Vec<i64> = Vec::with_capacity(points.len() - 1);
    let mut step_t: Vec<i64> = Vec::with_capacity(points.len() - 1);
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    let mut x_max = f64::MIN;
    for p in points {
        y_min = y_min.min(p.y);
        y_max = y_max.max(p.y);
        x_max = x_max.max(p.x);
    }
    for w in points.windows(2) {
        let dt = (w[1].t - w[0].t).max(1);
        step_dt.push(dt);
        step_t.push(w[1].t);
        speeds.push((w[1].x - w[0].x).abs() / dt as f64);
    }
    rep.y_range = y_max - y_min;
    let first_x = points.first().unwrap().x;
    let last_x = points.last().unwrap().x;
    let first_y = points.first().unwrap().y;
    let last_y = points.last().unwrap().y;
    // R6 超调：最大 x 超过最终 x 的部分。
    rep.overshoot_px = (x_max - last_x).max(0.0);

    // 速度均值 / 变异系数（R4a）。
    let mean_v = speeds.iter().sum::<f64>() / speeds.len() as f64;
    let var_v = speeds.iter().map(|v| (v - mean_v).powi(2)).sum::<f64>() / speeds.len() as f64;
    let std_v = var_v.sqrt();
    rep.vel_cv = if mean_v > 0.0 { std_v / mean_v } else { 0.0 };

    // 峰值速度位置（R4b）：峰值步的结束时间占总时长的比例。
    let peak_v = speeds.iter().cloned().fold(0.0f64, f64::max);
    let peak_idx = speeds
        .iter()
        .position(|v| *v == peak_v)
        .unwrap_or(0);
    rep.peak_pos = if peak_v > 0.0 {
        (step_t[peak_idx] - points.first().unwrap().t) as f64 / duration
    } else {
        0.0
    };

    // 3 点平滑后的速度曲线，用于临界点（R5）与初始加速阶段（R7）。
    let mut smooth: Vec<f64> = Vec::with_capacity(speeds.len());
    for i in 0..speeds.len() {
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(speeds.len() - 1);
        smooth.push(speeds[lo..=hi].iter().sum::<f64>() / (hi - lo + 1) as f64);
    }
    let eps = 0.05 * peak_v; //  prominence 门限：峰值的 5%（proposed, needs calibration）
    let mut critical = 0usize;
    let mut first_peak_frac = 0.0f64;
    for i in 1..smooth.len().saturating_sub(1) {
        let d1 = smooth[i] - smooth[i - 1];
        let d2 = smooth[i + 1] - smooth[i];
        if d1 * d2 < 0.0 && d1.abs() > eps && d2.abs() > eps {
            critical += 1;
            // 首次速度峰值（加速阶段结束）出现的位置。
            if d1 > 0.0 && d2 < 0.0 && first_peak_frac == 0.0 {
                first_peak_frac = (step_t[i] - points.first().unwrap().t) as f64 / duration;
            }
        }
    }
    rep.critical_points = critical;
    rep.accel_phase_frac = first_peak_frac;

    // jerk 标准差（R7 的另一半：S6 认为 jerk 类特征信息量最高，但只给了排序没给数值，
    // 所以只记录不评分，供日后校准）。
    if speeds.len() >= 3 {
        let mut accels: Vec<f64> = Vec::new();
        for i in 1..speeds.len() {
            let dt = ((step_dt[i] + step_dt[i - 1]) as f64 / 2.0).max(1.0);
            accels.push((speeds[i] - speeds[i - 1]) / dt);
        }
        if accels.len() >= 2 {
            let mut jerks: Vec<f64> = Vec::new();
            for i in 1..accels.len() {
                let dt = (step_dt[i] as f64).max(1.0);
                jerks.push((accels[i] - accels[i - 1]) / dt);
            }
            if !jerks.is_empty() {
                let mean_j = jerks.iter().sum::<f64>() / jerks.len() as f64;
                rep.jerk_std =
                    (jerks.iter().map(|j| (j - mean_j).powi(2)).sum::<f64>() / jerks.len() as f64)
                        .sqrt();
            }
        }
    }

    // R8a 停顿：连续两点位移 < 1 px 且时间间隔 ≥ 50 ms（相邻的合并计一次）。
    let mut pauses = 0usize;
    let mut in_pause = false;
    for w in points.windows(2) {
        let dt = w[1].t - w[0].t;
        let disp = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
        if dt >= 50 && disp < 1.0 {
            if !in_pause {
                pauses += 1;
                in_pause = true;
            }
        } else {
            in_pause = false;
        }
    }
    rep.pauses = pauses;

    // R8b 释放前 100 ms 内速度已降到峰值一半以下（减速入停）。
    let tail_start = points.last().unwrap().t - 100;
    let tail_speeds: Vec<f64> = speeds
        .iter()
        .zip(step_t.iter())
        .filter(|(_, t)| **t >= tail_start)
        .map(|(v, _)| *v)
        .collect();
    rep.release_dwell_ok = !tail_speeds.is_empty()
        && peak_v > 0.0
        && (tail_speeds.iter().sum::<f64>() / tail_speeds.len() as f64)
            < 0.5 * peak_v;

    // R9 直线度：起点→终点直线距离 / 路径总长；1.0 = 完全直线。
    let mut path = 0.0f64;
    for w in points.windows(2) {
        path += ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
    }
    let displacement = ((last_x - first_x).powi(2) + (last_y - first_y).powi(2)).sqrt();
    rep.straightness = if path > 0.0 { displacement / path } else { 1.0 };

    // ---- 加权评分（R3–R11；R6 为额外加分）----
    let mut score = 0i32;
    if rep.y_range >= 2.0 {
        score += 1; // R3
    }
    if rep.vel_cv >= 0.5 {
        score += 1; // R4a（CV 数值 proposed）
    }
    if (0.20..=0.60).contains(&rep.peak_pos) {
        score += 1; // R4b（人类峰值 ~30–40%，S15；放宽到 20–60%）
    }
    if (2..=5).contains(&rep.critical_points) {
        score += 1; // R5（2/6 截断 proposed；≥6 过抖得 0 分）
    }
    if (0.10..=0.60).contains(&rep.accel_phase_frac) {
        score += 1; // R7（10%–60% proposed）
    }
    if rep.pauses >= 1 {
        score += 1; // R8a（50 ms proposed）
    }
    if rep.release_dwell_ok {
        score += 1; // R8b（100 ms / 50% proposed）
    }
    if !(rep.straightness > 0.995 && rep.y_range < 2.0) {
        score += 1; // R9（0.995 proposed）
    }
    if start_delay_ms >= 100 {
        score += 1; // R10（100 ms proposed）
    }
    let distance = (last_x - first_x).abs();
    if !(distance > 150.0 && duration_ms < 300) {
        score += 1; // R11（300 ms / 150 px proposed）
    }
    if (1.0..=15.0).contains(&rep.overshoot_px) {
        score += 1; // R6 超调加分（仅 ~15% 人类会超调，S15；不作必备项）
    }
    rep.score = score;
    rep.score_max = SCORE_MAX;
    rep
}

/// R1/R2 硬门：物理上不可能的拖动。返回 Some(原因) 表示硬拒。
fn hard_gate(points: &[TrajPoint]) -> Option<&'static str> {
    if points.len() < 4 {
        return Some("too_few_points"); // R2：< 4 个点拟合不出任何速度曲线
    }
    let mut all_dt_same = true;
    let mut first_dt: Option<i64> = None;
    for w in points.windows(2) {
        let dt = w[1].t - w[0].t;
        if dt <= 0 {
            return Some("bad_timestamps"); // R2：非单调 / Δt ≤ 0
        }
        match first_dt {
            None => first_dt = Some(dt),
            Some(d) => {
                if d != dt {
                    all_dt_same = false;
                }
            }
        }
    }
    if all_dt_same {
        // R2：全部 Δt 精确到毫秒都相同——真实 pointermove 投递是不规则的。
        // 机器人常按固定步长生成时间戳。proposed, needs calibration。
        return Some("uniform_timestamps");
    }
    let duration = points.last().unwrap().t - points.first().unwrap().t;
    if duration < 200 {
        return Some("too_fast"); // R1：< 200 ms，人类不可能完成一次拼图拖动
    }
    None
}

impl Slider {
    /// 校验一次滑块验证。token 无论对错都会被消费（一次有效）；失败时直接发一道新题。
    /// 注意：失败原因（落点/轨迹/回放）只写进返回的 FeatureReport（日志用），
    /// 客户端收到的永远是通用文案 MSG_WRONG。
    pub fn verify(
        &self,
        ip: &str,
        token: &str,
        offset: f64,
        points: &[TrajPoint],
        start_delay_ms: i64,
        pointer_type: &str,
    ) -> AppResult<VerifyResult> {
        if !self.enabled() {
            return Err(AppError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "captcha_disabled",
                MSG_DISABLED,
            ));
        }
        let now = Instant::now();
        if self.failures_in_window(ip, now) >= FAIL_LIMIT_PER_WINDOW {
            return Err(AppError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                MSG_RATE_LIMITED,
            ));
        }
        // 格式校验（在消费 token 之前；格式错不计失败）。
        if !offset.is_finite()
            || points.len() > MAX_POINTS
            || start_delay_ms < 0
            || start_delay_ms > 120_000
            || points.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x < -100.0
                    || p.x > 500.0
                    || p.y < -200.0
                    || p.y > 500.0
                    || p.t < 0
                    || p.t > 120_000
            })
        {
            return Err(AppError::bad_request("答案格式不正确"));
        }
        let pointer_type = normalize_pointer_type(pointer_type);
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

        let expected_offset = challenge.gap_x / challenge.k;
        let mut report = FeatureReport::empty("failed", &pointer_type, start_delay_ms);
        report.offset = Some(offset);
        report.expected_offset = Some(expected_offset);
        report.gap_x = Some(challenge.gap_x);
        report.k = Some(challenge.k);
        report.points = serde_json::to_value(points).unwrap_or(Value::Array(vec![]));
        report.point_count = points.len();

        let offset_ok = (offset - expected_offset).abs() <= OFFSET_TOLERANCE_PX;

        // 键盘操作（无障碍）：只验落点，不评轨迹。
        let passed = if pointer_type == "keyboard" {
            report.verdict = if offset_ok {
                "passed_keyboard".to_string()
            } else {
                "failed_offset".to_string()
            };
            report.score_max = 0;
            offset_ok
        } else {
            let scored = analyze_trajectory(points, start_delay_ms, &pointer_type);
            report.duration_ms = scored.duration_ms;
            report.score = scored.score;
            report.score_max = scored.score_max;
            report.y_range = scored.y_range;
            report.vel_cv = scored.vel_cv;
            report.peak_pos = scored.peak_pos;
            report.critical_points = scored.critical_points;
            report.overshoot_px = scored.overshoot_px;
            report.jerk_std = scored.jerk_std;
            report.accel_phase_frac = scored.accel_phase_frac;
            report.pauses = scored.pauses;
            report.release_dwell_ok = scored.release_dwell_ok;
            report.straightness = scored.straightness;

            let hard = hard_gate(points);
            let need = if pointer_type == "touch" {
                SCORE_PASS_TOUCH
            } else {
                SCORE_PASS_MOUSE
            };
            let score_ok = scored.score >= need;
            let (fine_hash, coarse_hash) = trajectory_hashes(points);
            let duplicate = {
                let seen = self.accepted_hashes.lock().unwrap_or_else(|e| e.into_inner());
                seen.iter()
                    .any(|(f, c)| *f == fine_hash || *c == coarse_hash)
            };
            if !offset_ok {
                report.verdict = "failed_offset".to_string();
                report.hard_fail = Some("bad_offset".to_string());
                false
            } else if let Some(reason) = hard {
                report.verdict = "failed_hard_gate".to_string();
                report.hard_fail = Some(reason.to_string());
                false
            } else if !score_ok {
                report.verdict = "failed_score".to_string();
                false
            } else if duplicate {
                // R12：与之前已通过的轨迹相同/近似——回放攻击的唯一防线。
                report.verdict = "failed_duplicate".to_string();
                report.hard_fail = Some("duplicate_trajectory".to_string());
                false
            } else {
                report.verdict = "passed".to_string();
                let mut seen = self.accepted_hashes.lock().unwrap_or_else(|e| e.into_inner());
                seen.push((fine_hash, coarse_hash));
                if seen.len() > ACCEPTED_HASHES_CAP {
                    let excess = seen.len() - ACCEPTED_HASHES_CAP;
                    seen.drain(..excess);
                }
                true
            }
        };

        if !passed {
            Self::record_and_check(&self.fail_log, ip, FAIL_LIMIT_PER_WINDOW, now);
            if self.failures_in_window(ip, now) >= FAIL_LIMIT_PER_WINDOW {
                return Err(AppError::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    MSG_RATE_LIMITED,
                ));
            }
            let fresh = self.issue(ip)?;
            return Ok(VerifyResult::Failed(fresh, report));
        }
        let pass_token = util::random_token(24);
        if let Ok(mut pt) = self.pass_tokens.write() {
            pt.insert(
                pass_token.clone(),
                PassToken {
                    created: now,
                    ip: ip.to_string(),
                },
            );
        }
        Ok(VerifyResult::Passed(pass_token, report))
    }

    /// 签到/补签消费 pass_token：一次有效、120 秒过期、绑定发题 IP（与旧 selfcheck 同语义）。
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

fn normalize_pointer_type(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "touch" => "touch".to_string(),
        "pen" => "pen".to_string(),
        "keyboard" => "keyboard".to_string(),
        _ => "mouse".to_string(),
    }
}

/// 把一次验证尝试记进 slider_attempts 表（migration 0009），供日后校准阈值。
/// 写库失败只记 warn，绝不影响验证本身；没有表（迁移未跑）时同样只记 warn。
pub async fn log_attempt(pool: &sqlx::PgPool, ip: &str, report: &FeatureReport) {
    let features = json!({
        "y_range": report.y_range,
        "vel_cv": report.vel_cv,
        "peak_pos": report.peak_pos,
        "critical_points": report.critical_points,
        "overshoot_px": report.overshoot_px,
        "jerk_std": report.jerk_std,
        "accel_phase_frac": report.accel_phase_frac,
        "pauses": report.pauses,
        "release_dwell_ok": report.release_dwell_ok,
        "straightness": report.straightness,
        "hard_fail": report.hard_fail,
    });
    let res = sqlx::query(
        "INSERT INTO slider_attempts \
         (ip, verdict, offset_x, expected_offset, gap_x, k, duration_ms, point_count, \
          score, score_max, features, points, pointer_type, start_delay_ms) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(ip)
    .bind(&report.verdict)
    .bind(report.offset)
    .bind(report.expected_offset)
    .bind(report.gap_x)
    .bind(report.k)
    .bind(report.duration_ms as i32)
    .bind(report.point_count as i32)
    .bind(report.score)
    .bind(report.score_max)
    .bind(features)
    .bind(report.points.clone())
    .bind(&report.pointer_type)
    .bind(report.start_delay_ms as i32)
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, "slider_attempts 写库失败（仅影响校准日志）");
    }
}

/// GET /api/slider/challenge
pub async fn challenge(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Value>> {
    let ip = util::client_ip(&headers);
    Ok(Json(state.slider.issue(&ip)?))
}

#[derive(Debug, Deserialize)]
pub struct VerifyBody {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub offset: f64,
    #[serde(default)]
    pub points: Vec<TrajPoint>,
    #[serde(default)]
    pub start_delay_ms: i64,
    #[serde(default)]
    pub pointer_type: String,
}

/// POST /api/slider/verify
pub async fn verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    AppJson(body): AppJson<VerifyBody>,
) -> AppResult<Json<Value>> {
    let ip = util::client_ip(&headers);
    let result = state.slider.verify(
        &ip,
        &body.token,
        body.offset,
        &body.points,
        body.start_delay_ms,
        &body.pointer_type,
    );
    match result {
        Ok(VerifyResult::Passed(pass_token, report)) => {
            tracing::info!(
                verdict = %report.verdict, score = report.score, duration_ms = report.duration_ms,
                points = report.point_count, pointer_type = %report.pointer_type,
                gap_x = ?report.gap_x, k = ?report.k,
                "滑块验证通过"
            );
            log_attempt(&state.pool, &ip, &report).await;
            Ok(Json(json!({ "ok": true, "pass_token": pass_token })))
        }
        Ok(VerifyResult::Failed(fresh, report)) => {
            tracing::info!(
                verdict = %report.verdict, score = report.score, duration_ms = report.duration_ms,
                points = report.point_count, pointer_type = %report.pointer_type,
                hard_fail = ?report.hard_fail, gap_x = ?report.gap_x, k = ?report.k,
                "滑块验证失败"
            );
            log_attempt(&state.pool, &ip, &report).await;
            Ok(Json(json!({
                "ok": false,
                "message": MSG_WRONG,
                "challenge": fresh,
            })))
        }
        Err(e) => {
            // token 无效/过期/限流：记一条最小日志（没有评分报告）。
            let mut report =
                FeatureReport::empty(&format!("error:{}", e.code), &body.pointer_type, body.start_delay_ms);
            report.offset = Some(body.offset);
            report.point_count = body.points.len();
            report.points = serde_json::to_value(&body.points).unwrap_or(Value::Array(vec![]));
            if !body.points.is_empty() {
                report.duration_ms =
                    body.points.last().unwrap().t - body.points.first().unwrap().t;
            }
            log_attempt(&state.pool, &ip, &report).await;
            Err(e)
        }
    }
}

/// 签到/补签前调用：校验并一次消费 pass_token。
pub async fn check_pass_token(state: &AppState, token: Option<&str>, remote_ip: &str) -> AppResult<()> {
    state.slider.consume_pass_token(token, remote_ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_canvas() -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        // 320×200 渐变画布（有足够纹理，JPEG 编码有意义）。
        ImageBuffer::from_fn(CANVAS_W, CANVAS_H, |x, y| {
            Rgb([
                ((x * 255 / CANVAS_W) & 0xff) as u8,
                ((y * 255 / CANVAS_H) & 0xff) as u8,
                (((x + y) * 255 / (CANVAS_W + CANVAS_H)) & 0xff) as u8,
            ])
        })
    }

    fn test_slider() -> Slider {
        Slider::from_canvases(vec![test_canvas()])
    }

    /// 类人拖动：sigmoid 主行程（峰值 ~45% 处）+ 中途一次 120 ms 停顿 + 6 px 超调后回拉
    /// + ±3 px Y 抖动。目标总行程 200 canvas px，约 1.3 s。
    /// 注意：这只是合成轨迹，用来验证评分逻辑，不是真实人类数据。
    fn human_like_points() -> Vec<TrajPoint> {
        let mut points = Vec::new();
        let mut t: i64 = 0;
        let n = 30;
        for i in 0..=n {
            let u = i as f64 / n as f64;
            // sigmoid 主行程，峰值在 u≈0.45
            let sig = 1.0 / (1.0 + (-(u - 0.45) * 8.0).exp());
            let sig0 = 1.0 / (1.0 + (0.45f64 * 8.0).exp());
            let sig1 = 1.0 / (1.0 + (-(1.0f64 - 0.45) * 8.0).exp());
            let mut x = 206.0 * (sig - sig0) / (sig1 - sig0);
            // u > 0.85 后从 206 回拉到 200（6 px 超调）。
            if u > 0.85 {
                let s = (u - 0.85) / 0.15;
                let s = s * s * (3.0 - 2.0 * s);
                x = 206.0 - 6.0 * s;
            }
            let y = 5.0 + 3.0 * (u * 9.0).sin() + 0.7 * (u * 23.0).sin();
            // 不规则采样间隔（真实 pointermove 不是等间隔的）。
            let dt = 38 + ((i * 7) % 9) as i64;
            if i > 0 {
                t += dt;
            }
            // 中途停顿一次：同一位置重复一个点，间隔 120 ms。
            if i == 15 {
                points.push(TrajPoint { x, y, t });
                t += 120;
            }
            points.push(TrajPoint { x, y, t });
        }
        points
    }

    /// 机器人拖动 A：0.1 s 直线匀速滑完（S1 方法 1）。时间戳故意不规则，
    /// 使其命中 R1（过快）而非 R2 的均匀时间戳门。
    fn bot_direct_points() -> Vec<TrajPoint> {
        let ts = [0, 18, 41, 59, 82, 100];
        ts.iter()
            .enumerate()
            .map(|(i, t)| TrajPoint {
                x: i as f64 * 40.0,
                y: 5.0,
                t: *t,
            })
            .collect()
    }

    /// 机器人拖动 B：1 s 匀速直线拖动，Y 完全平直。
    fn bot_constant_speed_points() -> Vec<TrajPoint> {
        let dts = [39, 41, 40, 42, 38, 41, 40, 39, 42, 40, 41, 38, 40, 42, 39, 41, 40, 38, 42, 40, 39, 41, 40, 42];
        let mut points = vec![TrajPoint { x: 0.0, y: 5.0, t: 0 }];
        let mut t = 0i64;
        for (i, dt) in dts.iter().enumerate() {
            t += dt;
            points.push(TrajPoint {
                x: (i + 1) as f64 * 200.0 / 24.0,
                y: 5.0,
                t,
            });
        }
        points
    }

    fn issue_for(slider: &Slider, ip: &str) -> (String, f64, f64) {
        let body = slider.issue(ip).unwrap();
        let token = body["token"].as_str().unwrap().to_string();
        let ch = slider
            .challenges
            .read()
            .unwrap()
            .get(&token)
            .unwrap()
            .clone();
        (token, ch.gap_x, ch.k)
    }

    #[test]
    fn challenge_json_never_exposes_gap_x() {
        let s = test_slider();
        let body = s.issue("1.2.3.4").unwrap();
        assert!(body.get("gap_x").is_none(), "gap_x 绝不能出现在 challenge 里");
        assert!(body["bg"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(body["piece"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
        assert!(body["gap_y"].is_number());
        let k = body["k"].as_f64().unwrap();
        assert!((K_MIN..=K_MAX).contains(&k), "k 应在 [{K_MIN}, {K_MAX}] 内，实际 {k}");
        assert_eq!(body["canvas_w"], CANVAS_W);
        assert_eq!(body["piece_w"], PIECE);
    }

    #[test]
    fn human_like_trajectory_passes_and_scores_high() {
        let pts = human_like_points();
        let rep = analyze_trajectory(&pts, 800, "mouse");
        assert!(
            rep.score >= SCORE_PASS_MOUSE,
            "类人轨迹应通过评分，实际 score={} 详情：y_range={:.1} cv={:.2} peak={:.2} crit={} \
             accel={:.2} pauses={} dwell={} straight={:.4} overshoot={:.1}",
            rep.score, rep.y_range, rep.vel_cv, rep.peak_pos, rep.critical_points,
            rep.accel_phase_frac, rep.pauses, rep.release_dwell_ok, rep.straightness,
            rep.overshoot_px
        );
        assert!(rep.duration_ms >= 500 && rep.duration_ms <= 3000);
    }

    #[test]
    fn constant_speed_bot_scores_low() {
        let pts = bot_constant_speed_points();
        let rep = analyze_trajectory(&pts, 0, "mouse");
        assert!(
            rep.score < SCORE_PASS_MOUSE,
            "匀速直线机器人应评分失败，实际 score={}（y_range={:.1} cv={:.3} crit={}）",
            rep.score, rep.y_range, rep.vel_cv, rep.critical_points
        );
        assert_eq!(rep.y_range, 0.0);
        assert!(rep.vel_cv < 0.5);
    }

    #[test]
    fn correct_offset_passes() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        let pass = match s
            .verify("1.2.3.4", &token, expected, &pts, 800, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(p, report) => {
                assert_eq!(report.verdict, "passed");
                p
            }
            VerifyResult::Failed(_, report) => panic!(
                "正确落点 + 类人轨迹应通过，verdict={} score={} hard={:?}",
                report.verdict, report.score, report.hard_fail
            ),
        };
        assert!(s.consume_pass_token(Some(&pass), "1.2.3.4").is_ok());
        // pass_token 只能消费一次。
        assert!(s.consume_pass_token(Some(&pass), "1.2.3.4").is_err());
    }

    #[test]
    fn offset_tolerance_boundary() {
        // ±5 px 内通过，±5 px 外失败。
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected + 5.0, &pts, 800, "mouse")
                .unwrap(),
            VerifyResult::Passed(..)
        ));
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected + 5.1, &pts, 800, "mouse")
                .unwrap(),
            VerifyResult::Failed(..)
        ));
    }

    #[test]
    fn wrong_offset_fails() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        match s
            .verify("1.2.3.4", &token, expected + 40.0, &pts, 800, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(..) => panic!("落点偏差 40 px 应失败"),
            VerifyResult::Failed(_, report) => assert_eq!(report.verdict, "failed_offset"),
        }
        // 旧 token 已被消费，再验应拒绝。
        assert!(s
            .verify("1.2.3.4", &token, expected, &pts, 800, "mouse")
            .is_err());
    }

    #[test]
    fn direct_fast_bot_rejected_by_hard_gate() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = bot_direct_points();
        match s
            .verify("1.2.3.4", &token, expected, &pts, 0, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(..) => panic!("0.1 s 直线拖动应被 R1 硬拒"),
            VerifyResult::Failed(_, report) => {
                assert_eq!(report.verdict, "failed_hard_gate");
                assert_eq!(report.hard_fail.as_deref(), Some("too_fast"));
            }
        }
    }

    #[test]
    fn constant_speed_bot_rejected_by_score() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = bot_constant_speed_points();
        match s
            .verify("1.2.3.4", &token, expected, &pts, 0, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(..) => panic!("匀速直线拖动应评分失败"),
            VerifyResult::Failed(_, report) => assert_eq!(report.verdict, "failed_score"),
        }
    }

    #[test]
    fn replayed_trajectory_rejected() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected, &pts, 800, "mouse")
                .unwrap(),
            VerifyResult::Passed(..)
        ));
        // 同一条轨迹换一题重放（落点按新题算对）：R12 去重应拒绝。
        let (token2, gap_x2, k2) = issue_for(&s, "1.2.3.4");
        let expected2 = gap_x2 / k2;
        match s
            .verify("1.2.3.4", &token2, expected2, &pts, 800, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(..) => panic!("回放已通过的轨迹应被 R12 拒绝"),
            VerifyResult::Failed(_, report) => {
                assert_eq!(report.verdict, "failed_duplicate");
            }
        }
    }

    #[test]
    fn expired_token_rejected() {
        let s = test_slider().with_ttl(Duration::from_millis(1));
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        std::thread::sleep(Duration::from_millis(5));
        let err = s
            .verify("1.2.3.4", &token, expected, &human_like_points(), 800, "mouse")
            .unwrap_err();
        assert_eq!(err.message, MSG_EXPIRED);
    }

    #[test]
    fn used_token_rejected() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pts = human_like_points();
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected, &pts, 800, "mouse")
                .unwrap(),
            VerifyResult::Passed(..)
        ));
        let err = s
            .verify("1.2.3.4", &token, expected, &pts, 800, "mouse")
            .unwrap_err();
        assert_eq!(err.message, MSG_INVALID);
    }

    #[test]
    fn refresh_invalidates_old_token() {
        let s = test_slider();
        let first = s.issue("1.2.3.4").unwrap();
        let first_token = first["token"].as_str().unwrap().to_string();
        let second = s.issue("1.2.3.4").unwrap();
        assert_ne!(second["token"].as_str().unwrap(), first_token);
        let err = s
            .verify(
                "1.2.3.4",
                &first_token,
                100.0,
                &human_like_points(),
                800,
                "mouse",
            )
            .unwrap_err();
        assert_eq!(err.message, MSG_INVALID);
    }

    #[test]
    fn repeated_failures_rate_limited() {
        let s = test_slider();
        for _ in 0..FAIL_LIMIT_PER_WINDOW {
            let (token, _, _) = issue_for(&s, "9.9.9.9");
            let pts = human_like_points();
            // 故意落点错误。
            let _ = s.verify("9.9.9.9", &token, 0.0, &pts, 800, "mouse");
        }
        let (token, gap_x, k) = issue_for(&s, "9.9.9.9");
        let expected = gap_x / k;
        let err = s
            .verify(
                "9.9.9.9",
                &token,
                expected,
                &human_like_points(),
                800,
                "mouse",
            )
            .unwrap_err();
        assert_eq!(err.code, "rate_limited");
    }

    #[test]
    fn pass_token_bound_to_ip() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        let pass = match s
            .verify("1.2.3.4", &token, expected, &human_like_points(), 800, "mouse")
            .unwrap()
        {
            VerifyResult::Passed(p, _) => p,
            VerifyResult::Failed(..) => panic!("应通过"),
        };
        assert!(s.consume_pass_token(Some(&pass), "5.6.7.8").is_err());
    }

    #[test]
    fn keyboard_verifies_offset_only() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        // 键盘：没有轨迹点，只验落点。
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected, &[], 0, "keyboard")
                .unwrap(),
            VerifyResult::Passed(..)
        ));
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        assert!(matches!(
            s.verify("1.2.3.4", &token, expected + 30.0, &[], 0, "keyboard")
                .unwrap(),
            VerifyResult::Failed(..)
        ));
    }

    #[test]
    fn bad_shape_rejected_without_consuming_token() {
        let s = test_slider();
        let (token, gap_x, k) = issue_for(&s, "1.2.3.4");
        let expected = gap_x / k;
        // 超大点数。
        let too_many = vec![TrajPoint { x: 0.0, y: 0.0, t: 0 }; MAX_POINTS + 1];
        assert!(s
            .verify("1.2.3.4", &token, expected, &too_many, 800, "mouse")
            .is_err());
        // 格式错不消费 token：同一 token 还能正常用。
        assert!(matches!(
            s.verify(
                "1.2.3.4",
                &token,
                expected,
                &human_like_points(),
                800,
                "mouse"
            )
            .unwrap(),
            VerifyResult::Passed(..)
        ));
    }
}
