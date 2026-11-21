-- Plus 补签卡（2026-11-14）：makeup_cards 加 kind；每种卡每人同时只能持有一张未使用的。
ALTER TABLE makeup_cards ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'normal' CHECK (kind IN ('normal', 'plus'));
DROP INDEX IF EXISTS makeup_cards_one_active;
CREATE UNIQUE INDEX IF NOT EXISTS makeup_cards_one_active_per_kind ON makeup_cards (user_id, kind) WHERE status = 'issued';
