-- 新增 Ultra（Max 之上，升级 Max 后自动进入候补，需站长审批）
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_plan_check;
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_earned_plan_check;
ALTER TABLE channel_tiers DROP CONSTRAINT IF EXISTS channel_tiers_tier_check;
ALTER TABLE channel_model_tiers DROP CONSTRAINT IF EXISTS channel_model_tiers_tier_check;
ALTER TABLE plan_applications DROP CONSTRAINT IF EXISTS plan_applications_target_plan_check;
ALTER TABLE users ADD CONSTRAINT users_plan_check CHECK (plan IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max', 'ultra'));
ALTER TABLE users ADD CONSTRAINT users_earned_plan_check CHECK (earned_plan IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max', 'ultra'));
ALTER TABLE channel_tiers ADD CONSTRAINT channel_tiers_tier_check CHECK (tier IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max', 'ultra'));
ALTER TABLE channel_model_tiers ADD CONSTRAINT channel_model_tiers_tier_check CHECK (tier IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max', 'ultra'));
ALTER TABLE plan_applications ADD CONSTRAINT plan_applications_target_plan_check CHECK (target_plan IN ('plus', 'pro', 'max', 'ultra'));
-- Ultra 继承 Max 能用的渠道与模型
INSERT INTO channel_tiers (channel_id, tier) SELECT channel_id, 'ultra' FROM channel_tiers WHERE tier = 'max' ON CONFLICT DO NOTHING;
INSERT INTO channel_model_tiers (channel_model_id, tier) SELECT channel_model_id, 'ultra' FROM channel_model_tiers WHERE tier = 'max' ON CONFLICT DO NOTHING;
