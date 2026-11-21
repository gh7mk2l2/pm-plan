-- 免费模型日上限的管理员单独设置（2026-11-21 Wing 定：默认值 30M -> 1B，
-- 并允许管理员给指定 Key 或指定用户单独调高上限）。
-- 每行只针对一个目标：key_id 与 user_id 恰好一个非空；每 Key / 每用户最多一行。
-- 生效优先级：Key 单独设置 > 用户单独设置 > 默认值/环境变量 DIST_FREE_TOKEN_CAP。
-- 持久化在库里，重启不丢；删除行即恢复默认值。
CREATE TABLE IF NOT EXISTS free_token_cap_overrides (
    id BIGSERIAL PRIMARY KEY,
    key_id BIGINT REFERENCES api_keys(id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    cap_tokens BIGINT NOT NULL CHECK (cap_tokens > 0),
    note TEXT NOT NULL DEFAULT '',
    created_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT free_token_cap_overrides_target CHECK (
        (key_id IS NOT NULL AND user_id IS NULL)
        OR (key_id IS NULL AND user_id IS NOT NULL)
    )
);
CREATE UNIQUE INDEX IF NOT EXISTS free_token_cap_overrides_key_uidx
    ON free_token_cap_overrides(key_id) WHERE key_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS free_token_cap_overrides_user_uidx
    ON free_token_cap_overrides(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS free_token_cap_overrides_created_idx
    ON free_token_cap_overrides(created_at DESC);
