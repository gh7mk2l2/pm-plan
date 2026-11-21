use serde::Serialize;

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

pub const PLANS: [PlanInfo; 8] = [
    PlanInfo {
        code: "go",
        name: "Go",
        tagline: "注册即得，仅 MiniMax M3.1 Flash",
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

pub fn plan(code: &str) -> &'static PlanInfo {
    PLANS.iter().find(|p| p.code == code).unwrap_or(&PLANS[0])
}

pub fn rank(code: &str) -> i32 {
    PLANS
        .iter()
        .position(|p| p.code == code)
        .map(|i| i as i32)
        .unwrap_or(0)
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

    #[test]
    fn plan_ranks_are_ordered() {
        assert!(rank("go") < rank("lite"));
        assert!(rank("lite") < rank("essential"));
        assert!(rank("essential") < rank("standard"));
        assert!(rank("standard") < rank("plus"));
        assert!(rank("plus") < rank("pro"));
        assert!(rank("pro") < rank("max"));
        assert!(rank("max") < rank("ultra"));
        assert_eq!(rank("unknown"), 0);
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
}
