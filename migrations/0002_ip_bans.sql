-- IP level flood protection.
--
-- Rows are created by the in-process guard when a single address exceeds the
-- per-second ceiling. Keeping them in the database means a ban survives a
-- restart and can be lifted by the site owner.

CREATE TABLE IF NOT EXISTS ip_bans (
    ip         TEXT PRIMARY KEY,
    reason     TEXT NOT NULL,
    hits       BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS ip_bans_created_idx ON ip_bans (created_at DESC);
