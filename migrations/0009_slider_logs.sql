-- 滑块拼图验证（2026-11-15）的尝试日志：每一次 verify 都记一条，供日后校准轨迹阈值。
-- 评分规则 R1–R13 的“proposed, needs calibration”阈值要用这里的真实数据替换。
-- verdict 取值：passed / passed_keyboard / failed_offset / failed_hard_gate /
-- failed_score / failed_duplicate / error:<code>。
CREATE TABLE IF NOT EXISTS slider_attempts (
    id BIGSERIAL PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    ip TEXT NOT NULL DEFAULT '',
    verdict TEXT NOT NULL,
    offset_x DOUBLE PRECISION,
    expected_offset DOUBLE PRECISION,
    gap_x DOUBLE PRECISION,
    k DOUBLE PRECISION,
    duration_ms INTEGER,
    point_count INTEGER,
    score INTEGER,
    score_max INTEGER,
    features JSONB NOT NULL DEFAULT '{}'::jsonb,
    points JSONB NOT NULL DEFAULT '[]'::jsonb,
    pointer_type TEXT,
    start_delay_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_slider_attempts_created ON slider_attempts (created_at);
CREATE INDEX IF NOT EXISTS idx_slider_attempts_verdict ON slider_attempts (verdict);
