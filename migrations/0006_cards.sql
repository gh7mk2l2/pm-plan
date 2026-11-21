-- 补签卡 / 重置卡 / 用量重置记录（2026-11-10）
-- 0005 定义 Ultra 档位（与本迁移一同上线）；本迁移新增卡片功能。

CREATE TABLE IF NOT EXISTS makeup_cards (
    id            BIGSERIAL PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status        TEXT NOT NULL DEFAULT 'issued' CHECK (status IN ('issued', 'used', 'expired')),
    issued_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ,
    used_for_date DATE,
    issued_by     BIGINT REFERENCES users(id) ON DELETE SET NULL,
    note          TEXT
);
CREATE INDEX IF NOT EXISTS makeup_cards_user_idx ON makeup_cards (user_id, status);
-- 防重复发放：每人同时只能持有一张未使用的补签卡。
CREATE UNIQUE INDEX IF NOT EXISTS makeup_cards_one_active ON makeup_cards (user_id) WHERE status = 'issued';

CREATE TABLE IF NOT EXISTS reset_codes (
    id           BIGSERIAL PRIMARY KEY,
    code_hash    BYTEA NOT NULL UNIQUE,
    code_prefix  TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'unassigned' CHECK (status IN ('unassigned', 'issued', 'used', 'revoked')),
    user_id      BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_by   BIGINT REFERENCES users(id) ON DELETE SET NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at   TIMESTAMPTZ,
    redeemed_at  TIMESTAMPTZ,
    used_at      TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS reset_codes_user_idx ON reset_codes (user_id, status);

CREATE TABLE IF NOT EXISTS usage_resets (
    id            BIGSERIAL PRIMARY KEY,
    user_id       BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    reset_code_id BIGINT REFERENCES reset_codes(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS usage_resets_user_idx ON usage_resets (user_id, created_at DESC);
