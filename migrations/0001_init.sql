-- PM Plan initial schema. No user/channel/key grouping concepts exist by design.

CREATE TABLE IF NOT EXISTS users (
    id                BIGSERIAL PRIMARY KEY,
    linuxdo_id        BIGINT NOT NULL UNIQUE,
    username          TEXT NOT NULL,
    display_name      TEXT,
    avatar_url        TEXT,
    trust_level       INT NOT NULL DEFAULT 0,
    role              TEXT NOT NULL DEFAULT 'user' CHECK (role IN ('user', 'admin', 'root')),
    plan              TEXT NOT NULL DEFAULT 'lite' CHECK (plan IN ('lite', 'essential', 'standard', 'pro', 'max')),
    earned_plan       TEXT NOT NULL DEFAULT 'lite' CHECK (earned_plan IN ('lite', 'essential', 'standard', 'pro', 'max')),
    plan_source       TEXT NOT NULL DEFAULT 'default',
    is_banned         BOOLEAN NOT NULL DEFAULT FALSE,
    ban_reason        TEXT,
    consecutive_days  INT NOT NULL DEFAULT 0,
    longest_streak    INT NOT NULL DEFAULT 0,
    last_checkin_on   DATE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at     TIMESTAMPTZ
);

-- Exactly one root account is allowed.
CREATE UNIQUE INDEX IF NOT EXISTS users_one_root ON users ((role)) WHERE role = 'root';
CREATE INDEX IF NOT EXISTS users_created_idx ON users (created_at DESC);

CREATE TABLE IF NOT EXISTS sessions (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  BYTEA NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    ip          TEXT,
    user_agent  TEXT
);
CREATE INDEX IF NOT EXISTS sessions_user_idx ON sessions (user_id);

CREATE TABLE IF NOT EXISTS oauth_states (
    state_hash    BYTEA PRIMARY KEY,
    bind_hash     BYTEA NOT NULL,
    code_verifier TEXT NOT NULL,
    next_path     TEXT NOT NULL DEFAULT '/',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS oauth_credentials (
    user_id          BIGINT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    access_token_enc BYTEA NOT NULL,
    refresh_token_enc BYTEA,
    token_expires_at TIMESTAMPTZ,
    scope            TEXT,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS checkins (
    id         BIGSERIAL PRIMARY KEY,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    checkin_on DATE NOT NULL,
    streak     INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, checkin_on)
);
CREATE INDEX IF NOT EXISTS checkins_user_idx ON checkins (user_id, checkin_on DESC);

CREATE TABLE IF NOT EXISTS channels (
    id              BIGSERIAL PRIMARY KEY,
    name            TEXT NOT NULL,
    base_url        TEXT NOT NULL,
    api_key_enc     BYTEA NOT NULL,
    key_hint        TEXT NOT NULL DEFAULT '',
    enabled         BOOLEAN NOT NULL DEFAULT TRUE,
    priority        INT NOT NULL DEFAULT 0,
    weight          INT NOT NULL DEFAULT 1 CHECK (weight BETWEEN 1 AND 100),
    health          TEXT NOT NULL DEFAULT 'unknown' CHECK (health IN ('unknown', 'healthy', 'error')),
    last_health_at  TIMESTAMPTZ,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS channel_models (
    id             BIGSERIAL PRIMARY KEY,
    channel_id     BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    model          TEXT NOT NULL,
    upstream_model TEXT,
    enabled        BOOLEAN NOT NULL DEFAULT TRUE,
    input_micro    BIGINT,
    output_micro   BIGINT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (channel_id, model)
);
CREATE INDEX IF NOT EXISTS channel_models_model_idx ON channel_models (model);

-- Empty channel_tiers means: every current and future plan tier may call this channel.
CREATE TABLE IF NOT EXISTS channel_tiers (
    channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    tier       TEXT NOT NULL CHECK (tier IN ('lite', 'essential', 'standard', 'pro', 'max')),
    PRIMARY KEY (channel_id, tier)
);

-- Empty channel_model_tiers means: inherit the channel-level tier authorization.
CREATE TABLE IF NOT EXISTS channel_model_tiers (
    channel_model_id BIGINT NOT NULL REFERENCES channel_models(id) ON DELETE CASCADE,
    tier             TEXT NOT NULL CHECK (tier IN ('lite', 'essential', 'standard', 'pro', 'max')),
    PRIMARY KEY (channel_model_id, tier)
);

CREATE TABLE IF NOT EXISTS api_keys (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_hash     BYTEA NOT NULL UNIQUE,
    key_prefix   TEXT NOT NULL,
    expires_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    revoked_at   TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS api_keys_user_idx ON api_keys (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS api_key_models (
    key_id BIGINT NOT NULL REFERENCES api_keys(id) ON DELETE CASCADE,
    model  TEXT NOT NULL,
    PRIMARY KEY (key_id, model)
);

CREATE TABLE IF NOT EXISTS plan_applications (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    target_plan  TEXT NOT NULL CHECK (target_plan IN ('pro', 'max')),
    status       TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected')),
    note         TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at  TIMESTAMPTZ,
    reviewed_by  BIGINT REFERENCES users(id) ON DELETE SET NULL,
    review_note  TEXT
);
CREATE UNIQUE INDEX IF NOT EXISTS plan_apps_one_pending ON plan_applications (user_id, target_plan) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS plan_apps_status_idx ON plan_applications (status, created_at);

CREATE TABLE IF NOT EXISTS plan_events (
    id         BIGSERIAL PRIMARY KEY,
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    from_plan  TEXT NOT NULL,
    to_plan    TEXT NOT NULL,
    source     TEXT NOT NULL,
    actor_id   BIGINT REFERENCES users(id) ON DELETE SET NULL,
    detail     TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS usage_logs (
    id                BIGSERIAL PRIMARY KEY,
    request_id        TEXT NOT NULL,
    user_id           BIGINT REFERENCES users(id) ON DELETE SET NULL,
    key_id            BIGINT REFERENCES api_keys(id) ON DELETE SET NULL,
    channel_id        BIGINT REFERENCES channels(id) ON DELETE SET NULL,
    model             TEXT NOT NULL DEFAULT '',
    prompt_tokens     INT NOT NULL DEFAULT 0,
    completion_tokens INT NOT NULL DEFAULT 0,
    total_tokens      INT NOT NULL DEFAULT 0,
    cost_micro        BIGINT NOT NULL DEFAULT 0,
    status            SMALLINT NOT NULL DEFAULT 200,
    error_code        TEXT,
    latency_ms        INT NOT NULL DEFAULT 0,
    is_stream         BOOLEAN NOT NULL DEFAULT FALSE,
    client_ip         TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS usage_user_time_idx ON usage_logs (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS usage_time_idx ON usage_logs (created_at DESC);
CREATE INDEX IF NOT EXISTS usage_key_idx ON usage_logs (key_id);
CREATE INDEX IF NOT EXISTS usage_model_idx ON usage_logs (model);

CREATE TABLE IF NOT EXISTS model_prices (
    model        TEXT PRIMARY KEY,
    input_micro  BIGINT NOT NULL CHECK (input_micro >= 0),
    output_micro BIGINT NOT NULL CHECK (output_micro >= 0),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS audit_logs (
    id          BIGSERIAL PRIMARY KEY,
    actor_id    BIGINT REFERENCES users(id) ON DELETE SET NULL,
    action      TEXT NOT NULL,
    target_type TEXT NOT NULL DEFAULT '',
    target_id   TEXT NOT NULL DEFAULT '',
    detail      JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS audit_time_idx ON audit_logs (created_at DESC);

INSERT INTO settings (key, value) VALUES
    ('registration_open', 'true'::jsonb)
ON CONFLICT (key) DO NOTHING;

INSERT INTO model_prices (model, input_micro, output_micro) VALUES
    ('gpt-4o',                 2500000, 10000000),
    ('gpt-4o-mini',             150000,   600000),
    ('gpt-4.1',                2000000,  8000000),
    ('gpt-4.1-mini',            400000,  1600000),
    ('gpt-4.1-nano',            100000,   400000),
    ('o1',                    15000000, 60000000),
    ('o1-mini',                1100000,  4400000),
    ('o3',                     2000000,  8000000),
    ('o3-mini',                1100000,  4400000),
    ('o4-mini',                1100000,  4400000),
    ('claude-3-5-sonnet',      3000000, 15000000),
    ('claude-3-7-sonnet',      3000000, 15000000),
    ('claude-3-5-haiku',        800000,  4000000),
    ('claude-sonnet-4',        3000000, 15000000),
    ('claude-opus-4',         15000000, 75000000),
    ('gemini-2.0-flash',        100000,   400000),
    ('gemini-2.5-flash',        300000,  2500000),
    ('gemini-2.5-pro',         1250000, 10000000),
    ('deepseek-chat',           270000,  1100000),
    ('deepseek-reasoner',       550000,  2190000),
    ('qwen-max',               1600000,  6400000),
    ('qwen-plus',               400000,  1200000),
    ('llama-3.3-70b',           590000,   790000)
ON CONFLICT (model) DO NOTHING;
