-- 0003: 渠道模型检测、自定义请求头与参数覆盖。全部为增量列，带默认值，不改动已有数据。

-- 渠道级：转发上游时附加的额外请求头（JSON 对象，键为头名，值为字符串）。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS extra_headers JSONB NOT NULL DEFAULT '{}'::jsonb;
-- 渠道级：请求体中带 tools/functions 时去掉推理参数（chat 的 reasoning_effort、responses 的 reasoning）。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS strip_reasoning_with_tools BOOLEAN NOT NULL DEFAULT FALSE;
-- 渠道级：请求体覆盖（JSON 对象，顶层键覆盖请求体同名字段，值为 null 表示删除该字段）。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS param_override JSONB NOT NULL DEFAULT '{}'::jsonb;
-- 渠道级：默认测试模型（为空时取该渠道第一个启用的模型）。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS test_model TEXT;
-- 渠道级：检测用的接口格式。chat = /chat/completions（默认），responses = /responses（只支持 Responses 格式的上游）。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS test_endpoint TEXT NOT NULL DEFAULT 'chat';
-- 渠道级：最近一次模型检测结果。
ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_test_at TIMESTAMPTZ;
ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_test_ok BOOLEAN;
ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_test_ms INTEGER;
ALTER TABLE channels ADD COLUMN IF NOT EXISTS last_test_model TEXT;

-- 模型级：最近一次检测结果，供检测弹窗回显。
ALTER TABLE channel_models ADD COLUMN IF NOT EXISTS last_test_at TIMESTAMPTZ;
ALTER TABLE channel_models ADD COLUMN IF NOT EXISTS last_test_ok BOOLEAN;
ALTER TABLE channel_models ADD COLUMN IF NOT EXISTS last_test_ms INTEGER;
ALTER TABLE channel_models ADD COLUMN IF NOT EXISTS last_test_error TEXT;
