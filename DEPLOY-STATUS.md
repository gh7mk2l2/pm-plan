# PM-Plan 部署状态与隧道排障记录

## 已完成

- 服务器 `ssh -p 2222 <instabox-ssh-user>@ssh.instabox.sh`（公网 IP `16.148.253.55`，内网 `172.16.0.2`）。
- 代码在 `/data/pmplan`，systemd 服务 `pm-plan` 监听 `0.0.0.0:8080`，本机健康检查 200。
- PostgreSQL 17 本地库 `pmplan`，启动时自动迁移；生产库已清空测试数据（0 用户 / 0 渠道 / 注册开放）。
- 17 个单元测试通过；端到端测试（mock OAuth、首个用户站长、签到升级、审批、档位 403、Key 校验、429 额度、封禁、流式代理）全部通过。
- 服务器经 CF quick tunnel 出站可用：
  - 当前续命隧道：`cloudflared tunnel --url http://127.0.0.1:8080`（pid 103531），URL `https://return-tales-significantly-lucky.trycloudflare.com`
  - 旧 URL `exact-species-vault-show` 已 530（用户已把它的回调写进 LINUX DO 应用）。
  - 经验：前台 `timeout N ssh` 会话退出会连带杀掉后台 cloudflared，必须用 `nohup … &` 且不复用该 ssh 会话。

## 排障发现（关键）

- 用户报 `HTTP ERROR 405` 发生在 LINUX DO 授权页之后的回跳阶段，说明本地网络代理/加速器把 `connect.linux.do` DNS 劫持到了本地 MITM。
- 本机（1.95.76.200）`connect.linux.do` 证书为 `CN=Linux.do Accelerator Root CA`（非公信 CA），服务器侧为 Google Trust Services 正常证书。
- 服务器入站 80/443/8080 全部不可达；`plan.pmcat.top` 无任何 DNS 记录。
- 本机有 mihomo/dnscrypt/doh_relay 等代理组件；GitHub 下载极慢（cloudflared 40MB 传了 10 分钟只到 1MB）。

## 进行中

- 方案：在本机做「带拦截的 CF 隧道」（CF 原站指向本机 relay，CF 用 Anycast 证书做 TLS，浏览器证书正常）。
- `/root/pm-relay/relay.py`：
  - 18095 入口（按 SNI 分流），18096 INTERCEPTED TLS（本地 CA `certs/ca-cert.pem` 签发 `connect.linux.do` 证书），18097 控制端口（`on`/`off`/`status`）。
  - 拦截开启时：解析请求 → 转发到 `http://127.0.0.1:8080`（PM-Plan 回调）→ 记录请求与 303 响应。
  - 拦截关闭时：纯 TCP 透传。
  - 日志：`/root/pm-relay/captured-oauth.log`、`relay.log`。
- 本地 cloudflared 二进制下载受阻，正在尝试 `ssh … "gzip -9 -c /usr/local/bin/cloudflared" > cloudflared.gz` 流式拉取（/root/pm-relay）。

## 待用户确认

- 注入后可拿到真实授权码，用 curl 实测 LINUX DO token 端点（redirect_uri 指定为 `https://return-tales-significantly-lucky.trycloudflare.com/v1/oauth/callback`）：若返回 200，说明业务线路可用，405 只是本地 MITM 假象；此时应为该隧道 URL 加回调并直接登录，不应继续拦截。
