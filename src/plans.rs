//! 套餐（2026-11-21 自定义套餐）：
//! 内置 8 档写死在 [`PLANS`]，是签到养成的主梯队；管理员在后台创建的
//! 自定义套餐存在 `custom_plans` 表里，运行时与内置档合并（见
//! [`load_all_plans`]）。内置档的描述可通过 `plan_descriptions` 表覆盖
//! （后台“改描述”只改描述，不动档位与额度）；自定义套餐的描述、额度、
//! 排序都在 `custom_plans` 里。
//!
//! 设计取舍（写进 README 了，这里再记一笔）：
//! - 自定义套餐**不参与签到自动升级**：`auto_earned_plan` /
//!   `candidate_for_streak` / `next_auto_milestone` 只走内置梯队；
//!   自定义套餐只能由管理员手动授予。`checkin_days` 字段照常存，
//!   仅作展示与将来扩展用。
//! - 排序：内置档按 [`PLANS`] 顺序排在前面，自定义套餐按
//!   `sort_order`（再按 code）排在后面；合并后的位置就是档位 rank。
//! - 同步的 [`rank`] 不认识自定义 code 时返回 1000（高于全部内置
//!   档），这样只走同步 rank 的路径（签到自动升级、候补）永远不会
//!   把一个自定义套餐用户降级；需要精确比较两个自定义套餐时，
//!   管理后台用异步的 [`merged_rank`]。

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PlanLimits {
    /// Rolling 5 hour limit, in micro-USD. None means unlimited.
    pub h5: Option<i64>,
    /// Rolling 7 day limit, in micro-USD. None means unlimited.
    pub weekly: Option<i64>,
    /// Rolling 30 day limit, in micro-USD. None means unlimited.
    pub monthly: Option<i64>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PlanInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub tagline: &'static str,
    /// Consecutive check-in days required to reach this plan automatically.
    /// Max is a candidate plan that requires root approval.
    pub checkin_days: Option<i32>,
    pub requires_approval: bool,
    pub limits: PlanLimits,
}

/// 合并后的套餐视图：内置档与自定义套餐共用这个形状，`/api/plans` 与
/// 后台 `/api/admin/plans` 都返回它。`tagline` 就是“描述”（字段名保持
/// 兼容，管理后台 UI 里叫“描述”）。
#[derive(Clone, Debug, Serialize)]
pub struct PlanView {
    pub code: String,
    pub name: String,
    pub tagline: String,
    pub checkin_days: Option<i32>,
    pub requires_approval: bool,
    pub limits: PlanLimits,
    /// true = 内置档（不可删除），false = 管理员创建的自定义套餐。
    pub builtin: bool,
    /// 内置档 = 它在 PLANS 里的下标；自定义套餐 = 管理员填的排序值。
    pub sort_order: i32,
}

impl PlanView {
    pub fn from_builtin(info: &PlanInfo, sort_order: i32) -> Self {
        Self {
            code: info.code.to_string(),
            name: info.name.to_string(),
            tagline: info.tagline.to_string(),
            checkin_days: info.checkin_days,
            requires_approval: info.requires_approval,
            limits: info.limits,
            builtin: true,
            sort_order,
        }
    }

    pub fn from_custom(row: &CustomPlanRow) -> Self {
        Self {
            code: row.code.clone(),
            name: row.name.clone(),
            tagline: row.tagline.clone(),
            checkin_days: row.checkin_days,
            requires_approval: row.requires_approval,
            limits: PlanLimits {
                h5: row.h5_micro,
                weekly: row.weekly_micro,
                monthly: row.monthly_micro,
            },
            builtin: false,
            sort_order: row.sort_order,
        }
    }
}

/// `custom_plans` 表的一行（迁移 0012）。
#[derive(Clone, Debug, FromRow)]
pub struct CustomPlanRow {
    pub code: String,
    pub name: String,
    pub tagline: String,
    pub h5_micro: Option<i64>,
    pub weekly_micro: Option<i64>,
    pub monthly_micro: Option<i64>,
    pub checkin_days: Option<i32>,
    pub requires_approval: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

pub const PLANS: [PlanInfo; 8] = [
    PlanInfo {
        code: "go",
        name: "Go",
        tagline: "注册即得",
        checkin_days: None,
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(500_000),
            weekly: Some(1_000_000),
            monthly: Some(2_500_000),
        },
    },
    PlanInfo {
        code: "lite",
        name: "Lite",
        tagline: "累计签到 7 天（不必连续）",
        checkin_days: Some(7),
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(1_000_000),
            weekly: Some(2_000_000),
            monthly: Some(5_000_000),
        },
    },
    PlanInfo {
        code: "essential",
        name: "Essential",
        tagline: "连续签到 15 天",
        checkin_days: Some(15),
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(5_000_000),
            weekly: Some(30_000_000),
            monthly: Some(75_000_000),
        },
    },
    PlanInfo {
        code: "standard",
        name: "Standard",
        tagline: "连续签到 30 天",
        checkin_days: Some(30),
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(10_000_000),
            weekly: Some(35_000_000),
            monthly: None,
        },
    },
    PlanInfo {
        code: "plus",
        name: "Plus",
        tagline: "连续签到 50 天",
        checkin_days: Some(50),
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(25_000_000),
            weekly: None,
            monthly: None,
        },
    },
    PlanInfo {
        code: "pro",
        name: "Pro",
        tagline: "连续签到 70 天",
        checkin_days: Some(70),
        requires_approval: false,
        limits: PlanLimits {
            h5: Some(50_000_000),
            weekly: None,
            monthly: None,
        },
    },
    PlanInfo {
        code: "max",
        name: "Max",
        tagline: "连续签到 120 天，候补审批",
        checkin_days: Some(120),
        requires_approval: true,
        limits: PlanLimits {
            h5: Some(100_000_000),
            weekly: None,
            monthly: None,
        },
    },
    PlanInfo {
        code: "ultra",
        name: "Ultra",
        tagline: "升级 Max 后自动进入候补",
        checkin_days: None,
        requires_approval: true,
        limits: PlanLimits {
            h5: Some(200_000_000),
            weekly: None,
            monthly: None,
        },
    },
];

pub const BUILTIN_CODES: [&str; 8] = ["go", "lite", "essential", "standard", "plus", "pro", "max", "ultra"];

pub fn is_builtin(code: &str) -> bool {
    BUILTIN_CODES.contains(&code)
}

/// 内置档的同步查找；自定义套餐不在这里（它在库里），找不到时回落到 Go。
/// 需要自定义套餐时用异步的 [`load_plan`]。
pub fn plan(code: &str) -> &'static PlanInfo {
    PLANS.iter().find(|p| p.code == code).unwrap_or(&PLANS[0])
}

/// 内置档的同步 rank；不认识的 code（含自定义套餐）返回 1000，高于全部
/// 内置档——同步路径只用它做“自动升级是否更高”的比较，见模块头注释。
pub fn rank(code: &str) -> i32 {
    PLANS
        .iter()
        .position(|p| p.code == code)
        .map(|i| i as i32)
        .unwrap_or(1000)
}

/// 合并内置档与自定义套餐（纯函数，便于单测）：内置档在前（PLANS 顺序，
/// 描述可被 `plan_descriptions` 覆盖），自定义套餐在后（sort_order，
/// 再按 code）。
pub fn merge_plans(overrides: &HashMap<String, String>, customs: &[CustomPlanRow]) -> Vec<PlanView> {
    let mut out: Vec<PlanView> = PLANS
        .iter()
        .enumerate()
        .map(|(i, info)| {
            let mut view = PlanView::from_builtin(info, i as i32);
            if let Some(tagline) = overrides.get(info.code) {
                view.tagline = tagline.clone();
            }
            view
        })
        .collect();
    let mut customs: Vec<&CustomPlanRow> = customs.iter().collect();
    customs.sort_by(|a, b| (a.sort_order, a.code.as_str()).cmp(&(b.sort_order, b.code.as_str())));
    out.extend(customs.iter().map(|row| PlanView::from_custom(row)));
    out
}

pub async fn load_custom_plans(pool: &PgPool) -> Result<Vec<CustomPlanRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT code, name, tagline, h5_micro, weekly_micro, monthly_micro, \
                checkin_days, requires_approval, sort_order, created_at \
         FROM custom_plans ORDER BY sort_order, code",
    )
    .fetch_all(pool)
    .await
}

pub async fn load_all_plans(pool: &PgPool) -> Result<Vec<PlanView>, sqlx::Error> {
    let customs = load_custom_plans(pool).await?;
    let overrides: Vec<(String, String)> =
        sqlx::query_as("SELECT code, tagline FROM plan_descriptions")
            .fetch_all(pool)
            .await?;
    Ok(merge_plans(&overrides.into_iter().collect(), &customs))
}

/// 合并后的精确 rank：合并列表里的位置；既不在内置也不在自定义表里
/// 的 code 返回 1000（与同步 rank 一致）。
pub async fn merged_rank(pool: &PgPool, code: &str) -> Result<i32, sqlx::Error> {
    let all = load_all_plans(pool).await?;
    Ok(all
        .iter()
        .position(|p| p.code == code)
        .map(|i| i as i32)
        .unwrap_or(1000))
}

/// 合并后的单个套餐：内置档（含描述覆盖）或自定义套餐；不存在的 code
/// 回落到 Go（与同步 `plan()` 的行为一致，不让调用方拿到空档位）。
pub async fn load_plan(pool: &PgPool, code: &str) -> Result<PlanView, sqlx::Error> {
    if let Some(info) = PLANS.iter().find(|p| p.code == code) {
        let mut view = PlanView::from_builtin(info, rank(code));
        let tagline: Option<String> = sqlx::query_scalar(
            "SELECT tagline FROM plan_descriptions WHERE code = $1",
        )
        .bind(code)
        .fetch_optional(pool)
        .await?;
        if let Some(tagline) = tagline {
            view.tagline = tagline;
        }
        return Ok(view);
    }
    let row: Option<CustomPlanRow> = sqlx::query_as(
        "SELECT code, name, tagline, h5_micro, weekly_micro, monthly_micro, \
                checkin_days, requires_approval, sort_order, created_at \
         FROM custom_plans WHERE code = $1",
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(row) => PlanView::from_custom(&row),
        None => PlanView::from_builtin(&PLANS[0], 0),
    })
}

/// 自定义套餐 code 格式：小写 slug，字母开头，字母/数字/下划线，2–32 字符。
pub fn validate_code(raw: &str) -> Result<String, String> {
    let code = raw.trim().to_ascii_lowercase();
    let len = code.chars().count();
    if !(2..=32).contains(&len) {
        return Err("套餐 code 需为 2–32 个字符".to_string());
    }
    let mut chars = code.chars();
    if !chars.next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        return Err("套餐 code 须以小写字母开头".to_string());
    }
    if !code
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err("套餐 code 只能含小写字母、数字和下划线".to_string());
    }
    Ok(code)
}

/// 套餐名称：去空白后 1–50 字。
pub fn validate_name(raw: &str) -> Result<String, String> {
    let name = raw.trim().to_string();
    if name.is_empty() {
        return Err("套餐名称不能为空".to_string());
    }
    if name.chars().count() > 50 {
        return Err("套餐名称不能超过 50 字".to_string());
    }
    Ok(name)
}

/// 套餐描述（即 tagline）：去空白后 1–200 字。
pub fn validate_tagline(raw: &str) -> Result<String, String> {
    let tagline = raw.trim().to_string();
    if tagline.is_empty() {
        return Err("套餐描述不能为空".to_string());
    }
    if tagline.chars().count() > 200 {
        return Err("套餐描述不能超过 200 字".to_string());
    }
    Ok(tagline)
}

/// 美元转 micro-USD（库里存的单位）：0–1,000,000 美元，非法/超限返回 None。
pub fn dollars_to_micro(dollars: f64) -> Option<i64> {
    if !dollars.is_finite() || dollars < 0.0 || dollars > 1_000_000.0 {
        return None;
    }
    Some((dollars * 1_000_000.0).round() as i64)
}

/// 删除保护（纯函数）：返回 Some(原因) 表示拒绝删除，None 表示可删。
/// 内置档永远不可删；有人持有、或渠道/模型档位、或待审批申请引用，
/// 一律拒绝（不静默改派用户）。
pub fn delete_block_reason(
    is_builtin_plan: bool,
    users_holding: i64,
    channel_tiers: i64,
    model_tiers: i64,
    pending_applications: i64,
) -> Option<String> {
    if is_builtin_plan {
        return Some("内置套餐不可删除".to_string());
    }
    if users_holding > 0 {
        return Some(format!("仍有 {users_holding} 位用户持有该套餐，请先为他们改派套餐"));
    }
    if channel_tiers > 0 || model_tiers > 0 {
        return Some("该套餐仍被渠道或模型档位授权引用，请先取消授权".to_string());
    }
    if pending_applications > 0 {
        return Some(format!("仍有 {pending_applications} 个待审批申请指向该套餐"));
    }
    None
}

/// Go 累计签到天数达到该值升级 Lite（不要求连续）。
pub const LITE_TOTAL_CHECKINS: i32 = 7;

pub fn auto_earned_plan(streak: i32, total_checkins: i32) -> &'static str {
    if streak >= 70 {
        "pro"
    } else if streak >= 50 {
        "plus"
    } else if streak >= 30 {
        "standard"
    } else if streak >= 15 {
        "essential"
    } else if total_checkins >= LITE_TOTAL_CHECKINS {
        "lite"
    } else {
        "go"
    }
}

/// Candidate plan created automatically when a check-in streak crosses a threshold.
pub fn candidate_for_streak(streak: i32) -> Option<&'static str> {
    if streak >= 120 {
        Some("max")
    } else {
        None
    }
}

/// The next automatically granted plan and the streak at which it is granted.
pub fn next_auto_milestone(earned: &str) -> Option<(&'static str, i32)> {
    match earned {
        "go" => Some(("lite", LITE_TOTAL_CHECKINS)),
        "lite" => Some(("essential", 15)),
        "essential" => Some(("standard", 30)),
        "standard" => Some(("plus", 50)),
        "plus" => Some(("pro", 70)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_row(code: &str, sort_order: i32) -> CustomPlanRow {
        CustomPlanRow {
            code: code.to_string(),
            name: code.to_string(),
            tagline: "custom".to_string(),
            h5_micro: Some(1_000_000),
            weekly_micro: None,
            monthly_micro: None,
            checkin_days: None,
            requires_approval: false,
            sort_order,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn plan_ranks_are_ordered() {
        assert!(rank("go") < rank("lite"));
        assert!(rank("lite") < rank("essential"));
        assert!(rank("essential") < rank("standard"));
        assert!(rank("standard") < rank("plus"));
        assert!(rank("plus") < rank("pro"));
        assert!(rank("pro") < rank("max"));
        assert!(rank("max") < rank("ultra"));
        // 不认识的 code（含自定义套餐）排在全部内置档之上，同步路径
        // 永远不会因此把自定义套餐用户降级；精确比较用 merged_rank。
        assert_eq!(rank("unknown"), 1000);
        assert_eq!(rank("vip2026"), 1000);
    }

    #[test]
    fn go_tagline_has_no_minimax() {
        assert_eq!(plan("go").tagline, "注册即得");
        assert!(!plan("go").tagline.contains("MiniMax"));
        assert!(!plan("go").tagline.contains("M3.1"));
        assert!(!plan("go").tagline.contains("仅"));
    }

    #[test]
    fn go_plan_and_ladder() {
        assert_eq!(plan("go").limits.h5, Some(500_000));
        assert_eq!(plan("go").limits.weekly, Some(1_000_000));
        assert_eq!(plan("go").limits.monthly, Some(2_500_000));
        assert_eq!(auto_earned_plan(1, 1), "go");
        assert_eq!(auto_earned_plan(1, 7), "lite");
        assert_eq!(auto_earned_plan(15, 15), "essential");
        assert_eq!(auto_earned_plan(30, 30), "standard");
    }

    #[test]
    fn plan_limits_match_spec() {
        assert_eq!(plan("lite").limits.h5, Some(1_000_000));
        assert_eq!(plan("lite").limits.weekly, Some(2_000_000));
        assert_eq!(plan("lite").limits.monthly, Some(5_000_000));
        assert_eq!(plan("essential").limits.h5, Some(5_000_000));
        assert_eq!(plan("essential").limits.weekly, Some(30_000_000));
        assert_eq!(plan("essential").limits.monthly, Some(75_000_000));
        assert_eq!(plan("standard").limits.weekly, Some(35_000_000));
        assert_eq!(plan("standard").limits.monthly, None);
        assert_eq!(plan("plus").limits.h5, Some(25_000_000));
        assert_eq!(plan("plus").limits.weekly, None);
        assert_eq!(plan("pro").limits.h5, Some(50_000_000));
        assert_eq!(plan("max").limits.h5, Some(100_000_000));
        assert_eq!(plan("max").limits.monthly, None);
        assert_eq!(plan("ultra").limits.h5, Some(200_000_000));
    }

    #[test]
    fn auto_earned_plan_follows_streak() {
        assert_eq!(auto_earned_plan(0, 0), "go");
        assert_eq!(auto_earned_plan(14, 14), "lite");
        assert_eq!(auto_earned_plan(15, 15), "essential");
        assert_eq!(auto_earned_plan(29, 29), "essential");
        assert_eq!(auto_earned_plan(30, 30), "standard");
        assert_eq!(auto_earned_plan(50, 50), "plus");
        assert_eq!(auto_earned_plan(70, 70), "pro");
        assert_eq!(auto_earned_plan(200, 200), "pro");
    }

    #[test]
    fn candidate_plans_need_approval() {
        assert_eq!(candidate_for_streak(49), None);
        assert_eq!(candidate_for_streak(50), None);
        assert_eq!(candidate_for_streak(119), None);
        assert_eq!(candidate_for_streak(120), Some("max"));
    }

    #[test]
    fn next_auto_milestone_only_for_auto_plans() {
        assert_eq!(next_auto_milestone("lite"), Some(("essential", 15)));
        assert_eq!(next_auto_milestone("essential"), Some(("standard", 30)));
        assert_eq!(next_auto_milestone("standard"), Some(("plus", 50)));
        assert_eq!(next_auto_milestone("go"), Some(("lite", 7)));
        assert_eq!(next_auto_milestone("pro"), None);
        assert_eq!(next_auto_milestone("max"), None);
        assert_eq!(next_auto_milestone("ultra"), None);
    }

    #[test]
    fn merge_puts_custom_plans_after_builtins_in_sort_order() {
        let customs = vec![custom_row("zeta", 5), custom_row("alpha", 1), custom_row("beta", 1)];
        let merged = merge_plans(&HashMap::new(), &customs);
        assert_eq!(merged.len(), 11);
        assert_eq!(merged[0].code, "go");
        assert!(merged[0].builtin);
        assert_eq!(merged[7].code, "ultra");
        // 自定义套餐按 sort_order 再按 code 排在内置档之后。
        assert_eq!(merged[8].code, "alpha");
        assert_eq!(merged[9].code, "beta");
        assert_eq!(merged[10].code, "zeta");
        assert!(!merged[8].builtin);
        assert_eq!(merged[8].limits.h5, Some(1_000_000));
    }

    #[test]
    fn merge_applies_description_override_to_builtin() {
        let mut overrides = HashMap::new();
        overrides.insert("go".to_string(), "注册即得，欢迎加入".to_string());
        let merged = merge_plans(&overrides, &[]);
        assert_eq!(merged[0].tagline, "注册即得，欢迎加入");
        assert_eq!(merged[1].tagline, plan("lite").tagline);
    }

    #[test]
    fn plan_code_validation() {
        assert_eq!(validate_code("  Vip2026 ").unwrap(), "vip2026");
        assert!(validate_code("go").is_ok());
        assert!(validate_code("1abc").is_err());
        assert!(validate_code("a").is_err());
        assert!(validate_code("has space").is_err());
        assert!(validate_code("has-dash").is_err());
        assert!(validate_code("").is_err());
        assert!(is_builtin("go"));
        assert!(is_builtin("ultra"));
        assert!(!is_builtin("vip2026"));
    }

    #[test]
    fn plan_description_validation() {
        assert_eq!(validate_tagline("  注册即得  ").unwrap(), "注册即得");
        assert!(validate_tagline("").is_err());
        assert!(validate_tagline("   ").is_err());
        assert!(validate_tagline(&"x".repeat(200)).is_ok());
        assert!(validate_tagline(&"x".repeat(201)).is_err());
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(51)).is_err());
    }

    #[test]
    fn dollars_to_micro_conversion() {
        assert_eq!(dollars_to_micro(0.5), Some(500_000));
        assert_eq!(dollars_to_micro(2.5), Some(2_500_000));
        assert_eq!(dollars_to_micro(0.0), Some(0));
        assert_eq!(dollars_to_micro(-1.0), None);
        assert_eq!(dollars_to_micro(1_000_001.0), None);
        assert_eq!(dollars_to_micro(f64::NAN), None);
    }

    #[test]
    fn delete_guard_refuses_builtin_and_in_use_plans() {
        assert!(delete_block_reason(true, 0, 0, 0, 0).is_some());
        assert!(delete_block_reason(false, 3, 0, 0, 0)
            .unwrap()
            .contains("3 位用户"));
        assert!(delete_block_reason(false, 0, 1, 0, 0).is_some());
        assert!(delete_block_reason(false, 0, 0, 2, 0).is_some());
        assert!(delete_block_reason(false, 0, 0, 0, 1).is_some());
        assert_eq!(delete_block_reason(false, 0, 0, 0, 0), None);
    }
}
