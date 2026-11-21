# PM-Plan

> 胖猫也开始卖 Token Plan 了吗

PM-Plan 是一个从零实现的 Token Plan 分发平台：Rust + axum + sqlx + PostgreSQL 后端，原生 LINUX DO Connect OAuth，签到养成套餐，按套餐档位与渠道授权的交集开放模型，Key 只用两个配置项。前端为无构建步骤的原生 SPA。

本项目独立实现，未复制 New-API 代码。系统内**不存在任何分组概念**：数据库没有 `group` / `group_id` / `user_group` 字段，用户、渠道、Key 都没有分组属性。

---

## 功能

### 账号与站长

- LINUX DO Connect 授权码登录：`state` 防 CSRF、PKCE S256、浏览器绑定 Cookie、授权码一次性消费。
- 首个注册用户自动成为站长（root）；数据库唯一索引保证只有一个 root。
- 站长可开关注册、封禁/解封用户、查看全局统计与日志。
- OAuth 凭据与渠道 API Key 使用 AES-256-GCM 加密落库；会话只存 SHA-256 哈希。

### 套餐与签到

| 套餐 | 获取条件 | 5H | 7D | 30D |
| --- | --- | --- | --- | --- |
| Lite | 注册即得 | $1 | $2 | $5 |
| Essential | 连续签到 15 天 | $5 | $30 | $75 |
| Standard | 连续签到 30 天 | $10 | $35 | 不限 |
| Pro | 连续签到 50 天进入候补，站长审批 | $25 | 不限 | 不限 |
| Max | 连续签到 100 天进入候补，站长审批；也可手动授予 | $50 | 不限 | 不限 |

- 连续签到按自然日（`Asia/Shanghai`）计算，断签后连续计数重置。
- 断签保留已获得的最高套餐；自动升级只升不降。
- Pro / Max 的最终归属以站长审批为准；站长可手动授予/撤销。
- 额度窗口为滚动 5H / 7D / 30D，请求前硬性校验，超出返回 429 并写入用量审计。

### 渠道与档位授权

- 渠道字段：名称、Base URL、加密 API Key、优先级、权重、启用状态、健康状态。
- 档位授权有两级：
  - **渠道级**：不勾选任何档位 = 全部档位可调用（含未来新增档位）。
  - **模型级**：模型单独勾选时优先于渠道级；不勾选 = 继承渠道级。
- 用户可用模型 = 用户当前套餐档位 ∩ 渠道/模型档位授权。
- 未授权档位调用返回 `403`，错误码 `tier_forbidden`，并记录到 `usage_logs` 与管理员日志。
- 站长后台支持渠道列表档位标签、单渠道编辑、模型级授权、批量修改渠道档位。
- 渠道健康巡检每 10 分钟一次；支持手动测试、从上游 `/models` 同步模型。

### Key

用户创建 Key 只有两个可配置项：

1. 过期时间（或永久）
2. 可用模型（不选 = 动态覆盖当前及未来全部可用模型）

没有分组、额度、IP 白名单、渠道绑定等额外配置。Key 的额度完全继承用户套餐。

校验顺序固定为：

1. Key 是否存在、是否撤销/过期；用户是否封禁
2. 请求模型是否在 Key 的模型范围内（若 Key 指定了模型）
3. 模型是否在用户当前可用集合内（套餐档位 ∩ 渠道授权）
4. 套餐 5H / 7D / 30D 额度校验
5. 转发到上游渠道

Key 只存 SHA-256 哈希，完整 Key 仅在创建时返回一次。

### 前端

- 登录落地页、套餐展示、签到日历、额度进度、Key 管理、用量记录、套餐规则。
- 站长后台：概览、用户、渠道/模型/档位授权、晋升审批、全局日志、注册开关、模型价格、审计日志。
- 深色/浅色主题、移动端适配、无外部 CDN 依赖。

---

## 技术结构

```
src/
  main.rs       启动、路由、CSRF/安全响应头、静态文件
  config.rs     环境变量
  auth.rs       LINUX DO OAuth、Cookie 会话、鉴权 extractor
  api.rs        用户侧 API（me/签到/Key/用量/模型）
  admin.rs      站长后台 API
  proxy.rs      OpenAI 兼容代理、SSE 流式转发、校验顺序
  usage.rs      额度窗口、费用计算、模型可用集合、渠道选择、审计写入
  plans.rs      套餐常量与升级规则
  channels.rs   上游模型拉取与健康巡检
  models.rs     数据库行结构
  crypto.rs     AES-256-GCM
  error.rs      API 错误信封与 JSON extractor
  util.rs       随机数/哈希/金额/路径
migrations/     0001_init.sql
web/            index.html + static/app.js + static/styles.css + logo.jpg
```

- `sqlx::migrate!` 启动时自动迁移。
- 渠道选择：同优先级下优先健康渠道，再按权重加权随机。
- 费用：`tokens × 单价 / 1M`，金额以 micro-USD 整数存储，避免浮点误差。
- 模型价格优先级：渠道模型覆盖 > 全局 `model_prices` > 默认 `$1 / $3` 每 1M。
- 流式请求自动补 `stream_options.include_usage`，优先使用上游返回的真实 usage，缺失时按文本估算。

---

## 环境变量

复制 `.env.example` 为 `.env`：

```bash
cp .env.example .env
```

关键项：

- `BASE_URL`：必须与 LINUX DO Connect 中登记的回调地址前缀一致，例如 `https://plan.pmcat.top`。
- `ENCRYPTION_KEY`：32 字节随机数的 base64，生成：`openssl rand -base64 32`。
- `COOKIE_SECURE`：生产 HTTPS 必须为 `true`。
- `DATABASE_URL`：PostgreSQL 连接串。
- `WEB_DIR`：前端目录（默认 `/data/pmplan/web`）。

---

## 本地运行

```bash
cargo build --release
./target/release/pm-plan
```

需要先准备 PostgreSQL 数据库并执行迁移（启动时自动执行）。

```bash
cargo test
```

当前包含 17 个单元测试：套餐限额/等级/晋升阈值、费用、额度窗口、usage 解析、SSE 解析、加密往返、路径安全、Key 格式等。

---

## 服务器部署（当前实例）

- 服务器：`ssh -p 2222 <instabox-ssh-user>@ssh.instabox.sh`
- 应用目录：`/data/pmplan`
- 数据目录：`/data/pmplan/{bin,web,logs,target}`
- systemd：`pm-plan.service`，监听 `127.0.0.1:8080`
- PostgreSQL 17，本地库 `pmplan`，仅监听本机

```bash
# 构建
cd /data/pmplan
CARGO_HOME=/data/pmplan/cargo CARGO_TARGET_DIR=/data/pmplan/target cargo build --release
# 部署
sudo systemctl stop pm-plan
cp target/release/pm-plan bin/pm-plan
sudo systemctl start pm-plan
# 观察
sudo journalctl -u pm-plan -f
curl -s http://127.0.0.1:8080/api/health
```

服务器没有公网入站端口，公网入口需通过隧道/反代/EdgeOne 等提供。**`plan.pmcat.top` 的 DNS/接入是部署前置条件**，当前该域名无 A/AAAA/CNAME 记录。

LINUX DO Connect 配置：

- Client ID：`z1kWvUxpjYMEdH2W29hzUrj4awsaIi7G`
- 回调：`https://plan.pmcat.top/v1/oauth/callback`
- Scope：`openid profile email`
- 授权/令牌/用户端点：`https://connect.linux.do/oauth2/{authorize,token}`、`https://connect.linux.do/api/user`

---

## 主要 API

### 公共 / 用户

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/health` | 健康检查 |
| GET | `/api/bootstrap` | 登录页配置、套餐、注册状态 |
| GET | `/v1/oauth/login?next=/#/keys` | 发起 LINUX DO 登录 |
| GET | `/v1/oauth/callback` | OAuth 回调 |
| GET | `/v1/oauth/logout` | 退出 |
| GET | `/api/me` | 用户、套餐、额度、签到、可用模型 |
| POST | `/api/checkin` | 签到 |
| GET | `/api/checkins` | 签到历史 |
| GET | `/api/keys` | Key 列表 |
| POST | `/api/keys` | 创建 Key（`expires_at`, `models`） |
| DELETE | `/api/keys/{id}` | 撤销 Key |
| GET | `/api/usage?page=` | 个人用量 |
| GET | `/api/models` | 当前套餐可用模型与价格 |
| GET | `/api/plans` | 套餐常量 |

### 站长（需 root/admin + CSRF 头）

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/admin/overview` | 全局统计 |
| GET | `/api/admin/users?q=&page=` | 用户列表 |
| POST | `/api/admin/users/{id}/ban` | 封禁/解封 |
| POST | `/api/admin/users/{id}/grant` | 手动授予 Pro/Max |
| POST | `/api/admin/users/{id}/revoke` | 撤销 Pro/Max |
| GET | `/api/admin/applications?status=` | 晋升申请 |
| POST | `/api/admin/applications/{id}/approve` | 通过 |
| POST | `/api/admin/applications/{id}/reject` | 驳回 |
| GET/POST | `/api/admin/channels` | 渠道列表/新增 |
| PATCH/DELETE | `/api/admin/channels/{id}` | 编辑/删除渠道 |
| POST | `/api/admin/channels/{id}/test` | 连通性测试 |
| POST | `/api/admin/channels/{id}/sync-models` | 同步上游模型 |
| GET/POST | `/api/admin/channels/{id}/models` | 渠道模型 |
| PATCH/DELETE | `/api/admin/models/{id}` | 编辑/删除模型 |
| PUT | `/api/admin/channels/{id}/tiers` | 设置渠道档位 |
| PUT | `/api/admin/models/{id}/tiers` | 设置模型档位 |
| POST | `/api/admin/channels/bulk-tiers` | 批量设置渠道档位 |
| GET | `/api/admin/logs?user_id=&model=&status=&page=` | 全局日志 |
| GET/PATCH | `/api/admin/settings` | 注册开关等 |
| GET/POST | `/api/admin/prices` | 全局模型价格 |
| PUT/DELETE | `/api/admin/prices/{model}` | 修改/删除价格 |
| GET | `/api/admin/audit` | 审计日志 |

### OpenAI 兼容代理

- `GET /v1/models`
- `POST /v1/chat/completions`
- `POST /v1/completions`
- `POST /v1/embeddings`
- `POST /v1/responses`
- `POST /v1/{*path}`（其他 OpenAI 兼容路径）

请求头：`Authorization: Bearer sk-pm-...`（也支持 `x-api-key`）。

错误示例：

```json
{"error":{"code":"tier_forbidden","message":"套餐档位无权调用该渠道模型","type":"tier_forbidden"}}
```

---

## 安全说明

- 所有状态变更 API 要求 `X-PMPlan-CSRF: 1`，配合 `SameSite=Lax` 会话 Cookie。
- OAuth `state` 一次性、10 分钟过期，并与浏览器绑定 Cookie 的哈希比对。
- 会话 Cookie `HttpOnly`、`Secure`（生产）、`SameSite=Lax`，30 天。
- 管理员操作写入 `audit_logs`；渠道 Key 与 OAuth token 加密存储。
- 静态响应附带 CSP、`X-Content-Type-Options`、`X-Frame-Options`、Referrer-Policy、HSTS。
- 代理不记录完整 API Key，仅存储哈希和前缀。

## IP 封禁（2026-11-06 加入）

- 迁移 `0002_ip_bans.sql` 与线上版本逐字节一致（sqlx 校验和相同），可直接接现有数据库。
- 单个 IP 在 1 秒内对 `/api`、`/v1` 发出的请求超过 `IP_FLOOD_PER_SEC`（默认 30，设为 0 关闭）时自动封禁，写入 `ip_bans`；在该 IP 登录或调用过的普通账号会一并封禁并踢下线，管理员账号不受影响。
- 被封禁的 IP 访问任何页面都返回 403 `address blocked by flood guard`。封禁记录启动时加载到内存，重启后仍然有效。
- `IP_GUARD_ALLOW=1.2.3.4,5.6.7.8` 为白名单，名单内的地址永不封禁。
- 客户端 IP 按顺序取自 `CF-Connecting-IP`、`X-Real-IP`、`X-Forwarded-For`，请务必放在 Cloudflare 或反向代理之后，否则请求头可以伪造。
- 后台“IP 封禁”页面可查看、手动封禁、解封；解封时会一并恢复被连带封禁的账号。
- 接口：`GET /api/admin/ip-bans`、`POST /api/admin/ip-bans {ip, reason?, ban_users?}`、`DELETE /api/admin/ip-bans/{ip}?unban_users=true`

## 补签卡 / 重置卡 / Ultra（2026-11-10 加入）

- 迁移 `0006_cards.sql`：`makeup_cards`（补签卡）、`reset_codes`（重置卡兑换码）、`usage_resets`（重置记录）三张表；另有迁移 `0005`（Ultra 档位）随本次一并上线。
- 补签卡：站长在后台全员发放（每人仅一张、30 天有效，封禁用户跳过）或单独发放；用户在首页选择最近 7 天内漏签的一天补签，补签后重算连续与累计签到、自动升档与候补；使用时需过签到人机验证。签到与补签共用 `record_checkin` 核心逻辑。
- 重置卡：站长生成兑换码（`PMRC-XXXX-XXXX-XXXX-XXXX`，库里只存 SHA-256 哈希）；用户兑换后使用，使用时写入 `usage_resets`，5 小时 / 7 天 / 30 天已用额度全部清零。
- Ultra：高于 Max 的档位，5 小时额度 $200，7 天 / 30 天不限；升到 Max 后自动进入候补，站长审批通过后生效，永不过期，渠道权限与 Max 相同。
- 接口：`GET /api/cards`、`POST /api/makeup-cards/{id}/use`、`POST /api/reset-cards/redeem`、`POST /api/reset-cards/{id}/use`；管理：`POST /api/admin/makeup-cards/issue-all`、`POST /api/admin/makeup-cards/issue`、`GET /api/admin/makeup-cards`、`GET /api/admin/reset-codes`、`POST /api/admin/reset-codes`、`POST /api/admin/reset-codes/{id}/revoke`。

## 感谢来自 linux.do 佬友的建议与贡献
