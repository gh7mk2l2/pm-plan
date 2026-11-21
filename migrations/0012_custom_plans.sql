-- 自定义套餐（2026-11-21）：管理员在后台添加/删除套餐，自定义描述。
--
-- custom_plans：管理员创建的套餐。内置 8 档（go/lite/essential/standard/
-- plus/pro/max/ultra）仍写死在 src/plans.rs，不进这张表；code 与内置档
-- 重名由后端拒绝（这里不靠 CHECK 约束兜底）。
-- plan_descriptions：内置档的描述覆盖。后台“改描述”对内置档只写这里
-- （不动档位与额度）；自定义套餐的描述直接存在 custom_plans.tagline。
--
-- 自定义套餐不参与签到自动升级（只由管理员手动授予），checkin_days
-- 仅作展示与将来扩展用；删除保护见 src/plans.rs::delete_block_reason。
CREATE TABLE IF NOT EXISTS custom_plans (
    code             TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    tagline          TEXT NOT NULL,
    h5_micro         BIGINT,
    weekly_micro     BIGINT,
    monthly_micro    BIGINT,
    checkin_days     INT,
    requires_approval BOOLEAN NOT NULL DEFAULT FALSE,
    sort_order       INT NOT NULL DEFAULT 0,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT custom_plans_code_format CHECK (code ~ '^[a-z][a-z0-9_]{1,31}$'),
    CONSTRAINT custom_plans_name_nonempty CHECK (char_length(name) BETWEEN 1 AND 50),
    CONSTRAINT custom_plans_tagline_len CHECK (char_length(tagline) BETWEEN 1 AND 200),
    CONSTRAINT custom_plans_limits_nonneg CHECK (
        (h5_micro IS NULL OR h5_micro >= 0)
        AND (weekly_micro IS NULL OR weekly_micro >= 0)
        AND (monthly_micro IS NULL OR monthly_micro >= 0)
    ),
    CONSTRAINT custom_plans_checkin_days_range CHECK (
        checkin_days IS NULL OR checkin_days BETWEEN 1 AND 365
    )
);

CREATE TABLE IF NOT EXISTS plan_descriptions (
    code       TEXT PRIMARY KEY,
    tagline    TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT plan_descriptions_tagline_len CHECK (char_length(tagline) BETWEEN 1 AND 200)
);

-- 放开档位枚举 CHECK：自定义套餐 code 必须能出现在 users.plan /
-- users.earned_plan、channel_tiers.tier、channel_model_tiers.tier、
-- plan_applications.target_plan 里。只删约束，不再加新的枚举检查；
-- 非法 code 由后端校验（plans::validate_code + 档位白名单）拦截。
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_plan_check;
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_earned_plan_check;
ALTER TABLE channel_tiers DROP CONSTRAINT IF EXISTS channel_tiers_tier_check;
ALTER TABLE channel_model_tiers DROP CONSTRAINT IF EXISTS channel_model_tiers_tier_check;
ALTER TABLE plan_applications DROP CONSTRAINT IF EXISTS plan_applications_target_plan_check;
