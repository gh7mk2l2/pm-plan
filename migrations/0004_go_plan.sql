-- 新增 Go 档（低于 Lite）；档位改名：pro -> plus，max -> pro；新增 max（120 天候补）。
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_plan_check;
ALTER TABLE users DROP CONSTRAINT IF EXISTS users_earned_plan_check;
ALTER TABLE channel_tiers DROP CONSTRAINT IF EXISTS channel_tiers_tier_check;
ALTER TABLE channel_model_tiers DROP CONSTRAINT IF EXISTS channel_model_tiers_tier_check;
ALTER TABLE plan_applications DROP CONSTRAINT IF EXISTS plan_applications_target_plan_check;

-- 改名（先 pro->plus，再 max->pro，顺序不能反）
UPDATE users SET plan = 'plus' WHERE plan = 'pro';
UPDATE users SET plan = 'pro' WHERE plan = 'max';
UPDATE users SET earned_plan = 'plus' WHERE earned_plan = 'pro';
UPDATE users SET earned_plan = 'pro' WHERE earned_plan = 'max';
UPDATE channel_tiers SET tier = 'plus' WHERE tier = 'pro';
UPDATE channel_tiers SET tier = 'pro' WHERE tier = 'max';
UPDATE channel_model_tiers SET tier = 'plus' WHERE tier = 'pro';
UPDATE channel_model_tiers SET tier = 'pro' WHERE tier = 'max';
UPDATE plan_applications SET target_plan = 'plus' WHERE target_plan = 'pro';
UPDATE plan_applications SET target_plan = 'pro' WHERE target_plan = 'max';
UPDATE plan_events SET from_plan = CASE from_plan WHEN 'pro' THEN 'plus' WHEN 'max' THEN 'pro' ELSE from_plan END,
                       to_plan   = CASE to_plan   WHEN 'pro' THEN 'plus' WHEN 'max' THEN 'pro' ELSE to_plan END
 WHERE from_plan IN ('pro', 'max') OR to_plan IN ('pro', 'max');

-- 新 Max 继承新 Pro（原 Max）能用的渠道与模型
INSERT INTO channel_tiers (channel_id, tier) SELECT channel_id, 'max' FROM channel_tiers WHERE tier = 'pro' ON CONFLICT DO NOTHING;
INSERT INTO channel_model_tiers (channel_model_id, tier) SELECT channel_model_id, 'max' FROM channel_model_tiers WHERE tier = 'pro' ON CONFLICT DO NOTHING;

ALTER TABLE users ADD CONSTRAINT users_plan_check CHECK (plan IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max'));
ALTER TABLE users ADD CONSTRAINT users_earned_plan_check CHECK (earned_plan IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max'));
ALTER TABLE users ALTER COLUMN plan SET DEFAULT 'go';
ALTER TABLE users ALTER COLUMN earned_plan SET DEFAULT 'go';
ALTER TABLE channel_tiers ADD CONSTRAINT channel_tiers_tier_check CHECK (tier IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max'));
ALTER TABLE channel_model_tiers ADD CONSTRAINT channel_model_tiers_tier_check CHECK (tier IN ('go', 'lite', 'essential', 'standard', 'plus', 'pro', 'max'));
ALTER TABLE plan_applications ADD CONSTRAINT plan_applications_target_plan_check CHECK (target_plan IN ('plus', 'pro', 'max'));

-- Go 只开放 MiniMax M3.1 Flash 所在渠道
INSERT INTO channel_tiers (channel_id, tier)
SELECT DISTINCT cm.channel_id, 'go' FROM channel_models cm WHERE cm.model = 'minimax-m3.1-flash'
ON CONFLICT DO NOTHING;
