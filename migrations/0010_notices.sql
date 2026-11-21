-- 站内通知（2026-11-21）：管理员可给指定用户发站内警告/通知，用户登录后在首页顶部看到。
CREATE TABLE IF NOT EXISTS notices (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'warning',
    created_by BIGINT REFERENCES users(id) ON DELETE SET NULL,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_notices_user_created ON notices (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_notices_user_unread ON notices (user_id, is_read) WHERE NOT is_read;
