/* PM-Plan front-end. Vanilla JS, no build step. */

(() => {
  "use strict";

  const TIERS = ["lite", "essential", "standard", "pro", "max"];
  const PLAN_ORDER = { go: 0, lite: 1, essential: 2, standard: 3, plus: 4, pro: 5, max: 6, ultra: 7 };
  const app = document.getElementById("app");
  const toastRoot = document.getElementById("toast-root");
  const modalRoot = document.getElementById("modal-root");

  const state = {
    bootstrap: null,
    me: null,
    bootError: null,
  };

  const ui = {
    usagePage: 1,
    userQuery: "",
    userPage: 1,
    logFilters: { user_id: "", model: "", status: "" },
    logPage: 1,
    appStatus: "pending",
    selectedChannels: new Set(),
    channels: null,
    adminPlans: null,
    theme: localStorage.getItem("pmplan-theme") || "dark",
  };

  document.documentElement.dataset.theme = ui.theme;

  const ICONS = {
    grid: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
    key: '<circle cx="8" cy="15" r="4"/><path d="m11 12 9-9"/><path d="m17 4 3 3"/><path d="m14 7 3 3"/>',
    chart: '<path d="M4 20V10"/><path d="M10 20V4"/><path d="M16 20v-7"/><path d="M22 20H2"/>',
    layers: '<path d="m12 2 9 5-9 5-9-5 9-5Z"/><path d="m3 12 9 5 9-5"/><path d="m3 17 9 5 9-5"/>',
    shield: '<path d="M12 22s8-3.5 8-10V5l-8-3-8 3v7c0 6.5 8 10 8 10Z"/><path d="m9 12 2 2 4-4"/>',
    logout: '<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><path d="m16 17 5-5-5-5"/><path d="M21 12H9"/>',
    sun: '<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="m4.93 4.93 1.42 1.42"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/><path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>',
    heart: '<path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z"/>',
    loader: '<path d="M21 12a9 9 0 1 1-6.22-8.56"/>',
    alert: '<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
    moon: '<path d="M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8Z"/>',
    check: '<path d="m20 6-11 11-5-5"/>',
    x: '<path d="m18 6-12 12"/><path d="m6 6 12 12"/>',
    plus: '<path d="M12 5v14"/><path d="M5 12h14"/>',
    trash: '<path d="M3 6h18"/><path d="M8 6V4h8v2"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6"/><path d="M14 11v6"/>',
    refresh: '<path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/>',
    copy: '<rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
    edit: '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/>',
    server: '<rect x="2" y="3" width="20" height="7" rx="2"/><rect x="2" y="14" width="20" height="7" rx="2"/><path d="M6 6.5h.01"/><path d="M6 17.5h.01"/>',
    users: '<path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>',
    calendar: '<rect x="3" y="4" width="18" height="18" rx="2"/><path d="M16 2v4"/><path d="M8 2v4"/><path d="M3 10h18"/>',
    spark: '<path d="m12 3 1.9 5.6L20 10l-6.1 1.4L12 17l-1.9-5.6L4 10l6.1-1.4L12 3Z"/>',
    arrowRight: '<path d="M5 12h14"/><path d="m13 6 6 6-6 6"/>',
    arrowLeft: '<path d="M19 12H5"/><path d="m11 18-6-6 6-6"/>',
    alert: '<path d="m10.3 3.9-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z"/><path d="M12 9v4"/><path d="M12 17h.01"/>',
    info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v5"/><path d="M12 8h.01"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
    external: '<path d="M15 3h6v6"/><path d="m10 14 11-11"/><path d="M21 14v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5"/>',
    clock: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
    zap: '<path d="M13 2 3 14h9l-1 8 10-12h-9l1-8Z"/>',
    settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1Z"/>',
    filter: '<path d="M22 3H2l8 9.46V19l4 2v-8.54L22 3Z"/>',
    crown: '<path d="m2 8 4 4 6-7 6 7 4-4-2 12H4L2 8Z"/>',
    dot: '<circle cx="12" cy="12" r="3"/>',
  };

  function icon(name, size = 16) {
    return `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ICONS.dot}</svg>`;
  }

  const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };
  function esc(value) {
    return String(value ?? "").replace(/[&<>"']/g, (ch) => ESC[ch]);
  }

  function money(micro) {
    if (micro === null || micro === undefined) return "不限";
    const value = Number(micro) / 1e6;
    if (!Number.isFinite(value)) return "—";
    if (value === 0) return "$0";
    if (value < 1) {
      return "$" + value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
    }
    if (value < 100) {
      return "$" + value.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
    }
    return "$" + value.toLocaleString("en-US", { maximumFractionDigits: 2 });
  }

  function moneyExact(micro) {
    if (!micro) return "$0";
    const value = Number(micro) / 1e6;
    return "$" + value.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
  }

  function pct(used, limit) {
    if (!limit || limit <= 0) return 0;
    return Math.max(0, Math.min(100, (used / limit) * 100));
  }

  function barClass(percent) {
    if (percent >= 95) return "danger";
    if (percent >= 75) return "warn";
    return "";
  }

  function fmtDate(value) {
    if (!value) return "永不过期";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return esc(value);
    return date.toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function fmtShortDate(value) {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return esc(value);
    return date.toLocaleString("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function addDays(dateStr, delta) {
    const [y, m, d] = String(dateStr).split("-").map(Number);
    const date = new Date(Date.UTC(y, m - 1, d + delta));
    return date.toISOString().slice(0, 10);
  }

  function dayLabel(dateStr) {
    return String(dateStr).slice(8, 10);
  }

  function planLabel(code) {
    return ({ lite: "Lite", essential: "Essential", standard: "Standard", pro: "Pro", max: "Max" })[code] || code;
  }

  function planBadge(plan, label) {
    return `<span class="badge badge-${esc(plan)}">${esc(label || planLabel(plan) || "Lite")}</span>`;
  }

  function statusDot(status) {
    return `<span class="status-dot ${esc(status)}"></span>`;
  }

  function tierTags(tiers, allLabel = "全部档位") {
    if (!tiers || tiers.length === 0) {
      return `<span class="tier-tag all">${esc(allLabel)}</span>`;
    }
    return tiers.map((tier) => `<span class="tier-tag ${esc(tier)}">${esc(tier)}</span>`).join("");
  }

  function avatar(user, cls = "") {
    const name = user?.display_name || user?.username || "?";
    const initial = esc(String(name).slice(0, 1).toUpperCase());
    const img = user?.avatar_url
      ? `<img src="${esc(user.avatar_url)}" alt="" data-avatar-image>`
      : "";
    return `<span class="avatar ${cls}"><span>${initial}</span>${img}</span>`;
  }

  function bindAvatarFallbacks(root = document) {
    root.querySelectorAll("img[data-avatar-image]").forEach((img) => {
      img.addEventListener("error", () => img.remove(), { once: true });
    });
  }

  function isAdmin() {
    return !!state.me && (state.me.user.role === "root" || state.me.user.role === "admin");
  }

  /* ---------- API ---------- */

  // 贡献 ZCode 账号: 免登录, 点击后自动获取登录链接并新窗口打开
  document.addEventListener("click", async (ev) => {
    const btn = ev.target.closest("#zcode-contribute-btn");
    if (!btn) return;
    if (btn.disabled) return;
    const original = btn.innerHTML;
    btn.disabled = true;
    btn.innerHTML = `${icon("loader", 16)}正在获取登录链接…`;
    try {
      const data = await api("/api/v1/zcode/contribute");
      if (data && data.url) {
        window.open(data.url, "_blank", "noopener");
        btn.innerHTML = `${icon("check", 16)}已打开登录页，完成后自动加入`;
        setTimeout(() => { btn.innerHTML = original; btn.disabled = false; }, 6000);
      } else {
        throw new Error("empty url");
      }
    } catch (e) {
      btn.innerHTML = `${icon("alert", 16)}获取失败，请稍后再试`;
      setTimeout(() => { btn.innerHTML = original; btn.disabled = false; }, 3000);
    }
  });

  /* ---------- 动效增强 (2026-11-01) ---------- */

  const prefersReducedMotion =
    window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // 数字滚动: 对 .stat-value 里首个数字做 0 -> 目标 的缓动计数
  function animateCounters(root = document) {
    if (prefersReducedMotion) return;
    root.querySelectorAll(".stat-value").forEach((el) => {
      if (el.dataset.counted) return;
      const text = el.textContent || "";
      const m = text.match(/^([\d,]+)([\s\S]*)$/);
      if (!m) return;
      const target = Number(m[1].replace(/,/g, ""));
      if (!Number.isFinite(target) || target === 0) return;
      el.dataset.counted = "1";
      const suffix = m[2];
      const dur = 780;
      const start = performance.now();
      const tick = (now) => {
        const t = Math.min(1, (now - start) / dur);
        const eased = 1 - Math.pow(1 - t, 3);
        const val = Math.round(target * eased);
        el.textContent = val.toLocaleString() + suffix;
        if (t < 1) requestAnimationFrame(tick);
        else el.textContent = target.toLocaleString() + suffix;
      };
      requestAnimationFrame(tick);
    });
  }

  // 卡片光标跟随高光
  function bindCardSpotlight(root = document) {
    if (prefersReducedMotion) return;
    root.querySelectorAll(".card").forEach((card) => {
      if (card.dataset.spotlight) return;
      card.dataset.spotlight = "1";
      card.addEventListener("mousemove", (e) => {
        const r = card.getBoundingClientRect();
        card.style.setProperty("--mx", ((e.clientX - r.left) / r.width) * 100 + "%");
        card.style.setProperty("--my", ((e.clientY - r.top) / r.height) * 100 + "%");
      });
    });
  }

  // 入口动效: render 成功后统一调用
  function enhanceMotion() {
    if (prefersReducedMotion) return;
    animateCounters();
    bindCardSpotlight();
  }

  // 复制成功脉冲反馈
  function pulseElement(el) {
    if (!el || prefersReducedMotion) return;
    el.classList.remove("pulse-once");
    void el.offsetWidth;
    el.classList.add("pulse-once");
    window.setTimeout(() => el.classList.remove("pulse-once"), 750);
  }

  async function api(path, options = {}) {
    const method = options.method || "GET";
    const headers = { Accept: "application/json" };
    if (options.body !== undefined) headers["Content-Type"] = "application/json";
    if (method !== "GET" && method !== "HEAD") headers["X-PMPlan-CSRF"] = "1";
    const response = await fetch(path, {
      method,
      headers,
      credentials: "same-origin",
      body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
    });
    const text = await response.text();
    let data = null;
    if (text) {
      try {
        data = JSON.parse(text);
      } catch {
        data = null;
      }
    }
    if (!response.ok) {
      const message = data?.error?.message || data?.message || `请求失败（HTTP ${response.status}）`;
      const error = new Error(message);
      error.status = response.status;
      error.code = data?.error?.code;
      throw error;
    }
    return data;
  }

  /* ---------- toast / modal ---------- */

  function toast(message, type = "info", timeout = 3600) {
    const el = document.createElement("div");
    el.className = `toast ${type}`;
    const iconName = type === "success" ? "check" : type === "error" ? "alert" : "info";
    el.innerHTML = `${icon(iconName, 17)}<div>${esc(message)}</div>`;
    toastRoot.appendChild(el);
    window.setTimeout(() => {
      el.classList.add("leaving");
      window.setTimeout(() => el.remove(), 220);
    }, timeout);
  }

  let modalEscHandler = null;

  function openModal({ title, subtitle = "", body = "", footer = "", wide = false, xwide = false, onMount }) {
    if (modalEscHandler) document.removeEventListener("keydown", modalEscHandler);
    modalRoot.innerHTML = `
      <div class="modal-backdrop">
        <div class="modal ${xwide ? "xwide" : wide ? "wide" : ""}" role="dialog" aria-modal="true" aria-label="${esc(title)}">
          <div class="modal-head">
            <div>
              <h3 class="modal-title">${esc(title)}</h3>
              ${subtitle ? `<p class="modal-sub">${esc(subtitle)}</p>` : ""}
            </div>
            <button type="button" class="modal-close" data-action="close-modal" aria-label="关闭">${icon("x", 16)}</button>
          </div>
          <div class="modal-body">${body}</div>
          ${footer ? `<div class="modal-foot">${footer}</div>` : ""}
        </div>
      </div>`;
    const backdrop = modalRoot.firstElementChild;
    requestAnimationFrame(() => backdrop.classList.add("open"));
    backdrop.addEventListener("click", (event) => {
      if (event.target === backdrop) closeModal();
    });
    modalEscHandler = (event) => {
      if (event.key === "Escape") closeModal();
    };
    document.addEventListener("keydown", modalEscHandler);
    bindAvatarFallbacks(modalRoot);
    if (onMount) onMount(modalRoot);
    return modalRoot;
  }

  function closeModal() {
    if (modalEscHandler) {
      document.removeEventListener("keydown", modalEscHandler);
      modalEscHandler = null;
    }
    const backdrop = modalRoot.firstElementChild;
    if (!backdrop) return;
    backdrop.classList.remove("open");
    window.setTimeout(() => {
      if (modalRoot.firstElementChild === backdrop) modalRoot.innerHTML = "";
    }, 180);
  }

  function confirmModal({ title, message, confirmText = "确认", danger = false, onConfirm }) {
    openModal({
      title,
      body: `<p class="muted" style="margin:0;line-height:1.75">${esc(message)}</p>`,
      footer: `
        <button type="button" class="btn btn-ghost" data-action="close-modal">取消</button>
        <button type="button" class="btn ${danger ? "btn-danger" : "btn-primary"}" id="confirm-action">${esc(confirmText)}</button>`,
      onMount(root) {
        root.querySelector("#confirm-action").addEventListener("click", async () => {
          const button = root.querySelector("#confirm-action");
          button.disabled = true;
          try {
            await onConfirm();
            closeModal();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  /* ---------- shell ---------- */

  function navLink(href, label, iconName) {
    const active = currentRoute() === href || (href !== "/" && currentRoute().startsWith(href));
    return `<a class="nav-link ${active ? "active" : ""}" href="#${href}">${icon(iconName, 15)}${esc(label)}</a>`;
  }

  function mobileTabs() {
    const items = [
      ["/", "概览", "grid"],
      ["/keys", "Keys", "key"],
      ["/usage", "用量", "chart"],
      ["/models", "模型", "grid"],
      ["/plans", "套餐", "layers"],
    ];
    return `<nav class="shell mobile-tabs" aria-label="移动导航">${items
      .map(([href, label, iconName]) => {
        const active = currentRoute() === href || (href !== "/" && currentRoute().startsWith(href));
        return `<a class="mobile-tab ${active ? "active" : ""}" href="#${href}">${icon(iconName, 16)}${label}</a>`;
      })
      .join("")}</nav>`;
  }

  function appShell(content) {
    const me = state.me;
    const adminLink = isAdmin()
      ? `<a class="icon-btn" href="#/admin" title="站长后台" aria-label="站长后台">${icon("shield", 17)}</a>`
      : "";
    return `
      <header class="topbar">
        <div class="shell topbar-inner">
          <a class="brand" href="#/">
            <img class="brand-logo" src="/static/logo.jpg" alt="PM-Plan">
            <span>
              <span class="brand-name">PM-Plan</span>
              <span class="brand-sub">Token Plan</span>
            </span>
          </a>
          <nav class="nav-links" aria-label="主导航">
            ${navLink("/", "概览", "grid")}
            ${navLink("/keys", "Keys", "key")}
            ${navLink("/usage", "用量", "chart")}
            ${navLink("/models", "模型广场", "grid")}
            ${navLink("/plans", "套餐", "layers")}
          </nav>
          <span class="nav-spacer"></span>
          <div class="nav-actions">
            <button type="button" class="btn btn-soft btn-sm" id="zcode-contribute-btn" title="贡献一个 ZCode 账号加入算力池，无奖励，纯公益">${icon("heart", 14)}贡献</button>
            <button type="button" class="icon-btn" data-action="toggle-theme" title="切换主题" aria-label="切换主题">${icon(ui.theme === "dark" ? "sun" : "moon", 17)}</button>
            ${adminLink}
            <a class="user-chip" href="#/" title="${esc(me.user.username)}">
              ${avatar(me.user, "avatar-sm")}
              <span class="user-chip-meta">
                <span class="user-chip-name">${esc(me.user.display_name || me.user.username)}</span>
                <span class="user-chip-role">${esc(me.plan.name)}${me.user.is_root ? " · 站长" : ""}</span>
              </span>
            </a>
            <a class="icon-btn" href="/v1/oauth/logout" title="退出登录" aria-label="退出登录">${icon("logout", 17)}</a>
          </div>
        </div>
        ${mobileTabs()}
      </header>
      <main class="shell app-page">${content}</main>
      <footer class="shell footer">
        <span>PM-Plan · 胖猫也开始卖 Token Plan 了吗</span>
        <span>模型权限仅由套餐档位 × 渠道授权决定</span>
      </footer>`;
  }

  function loadingContent() {
    return `
      <div class="page-head">
        <div>
          <div class="skeleton" style="width:160px;height:32px;margin-bottom:10px"></div>
          <div class="skeleton" style="width:260px;height:14px"></div>
        </div>
      </div>
      <div class="card"><div class="skeleton" style="height:260px"></div></div>`;
  }

  function errorContent(error) {
    return `
      <div class="card" style="text-align:center;padding:56px 24px">
        <div class="empty-icon" style="color:var(--danger)">${icon("alert", 22)}</div>
        <h2 style="margin:0 0 8px;font-size:20px">页面加载失败</h2>
        <p class="muted" style="margin:0 0 20px">${esc(error.message || "未知错误")}</p>
        <button type="button" class="btn btn-soft" data-action="reload-page">${icon("refresh", 15)}重新加载</button>
      </div>`;
  }

  /* ---------- login page ---------- */

  function nextPath() {
    const hash = location.hash;
    if (hash && hash.startsWith("#/")) return "/" + hash;
    return "/";
  }

  function planCard(plan, currentCode) {
    const current = currentCode === plan.code;
    return `
      <div class="plan-card ${plan.requires_approval ? "featured" : ""} ${current ? "current" : ""}" data-plan="${esc(plan.code)}">
        <div class="plan-kicker">
          <span class="plan-name">${esc(plan.name)}</span>
          <span class="plan-dot"></span>
        </div>
        <div class="plan-req">${esc(plan.tagline)}${plan.requires_approval ? " · 需站长审批" : ""}</div>
        <div class="plan-limits">
          <div class="plan-limit"><span>5H</span><strong>${money(plan.limits.h5)}</strong></div>
          <div class="plan-limit"><span>7D</span><strong>${money(plan.limits.weekly)}</strong></div>
          <div class="plan-limit"><span>30D</span><strong>${money(plan.limits.monthly)}</strong></div>
        </div>
        <div class="plan-note">${current ? "当前套餐" : plan.requires_approval ? "候补池 · 站长审批" : plan.checkin_days ? `连续签到 ${plan.checkin_days} 天` : "注册即得"}</div>
      </div>`;
  }

  function renderLogin() {
    const boot = state.bootstrap;
    if (!boot) {
      app.innerHTML = `
        <div class="boot-screen">
          <img class="boot-logo" src="/static/logo.jpg" alt="PM-Plan">
          <div class="boot-text">${state.bootError ? "服务暂时不可用" : "正在连接 PM-Plan…"}</div>
          ${state.bootError ? `<button class="btn btn-soft" data-action="reload-page">重新加载</button>` : ""}
        </div>`;
      return;
    }
    const plans = boot.plans || [];
    const loginHref = `/v1/oauth/login?next=${encodeURIComponent(nextPath())}`;
    const regNote = !boot.registration_open
      ? `<div class="notice mt-24">${icon("alert", 16)}<div><strong>当前已关闭注册。</strong>已有账号仍可正常登录；如需新账号请联系站长。</div></div>`
      : "";
    const rootNote = !boot.has_users
      ? `<div class="notice mint mt-24">${icon("crown", 16)}<div><strong>首位登录用户将自动成为站长。</strong>登录后即可开启注册、添加渠道并配置档位授权。</div></div>`
      : "";

    app.innerHTML = `
      <header class="topbar">
        <div class="shell topbar-inner">
          <a class="brand" href="#/">
            <img class="brand-logo" src="/static/logo.jpg" alt="PM-Plan">
            <span><span class="brand-name">PM-Plan</span><span class="brand-sub">Token Plan</span></span>
          </a>
          <span class="nav-spacer"></span>
          <div class="nav-actions">
            <button type="button" class="icon-btn" data-action="toggle-theme" title="切换主题">${icon(ui.theme === "dark" ? "sun" : "moon", 17)}</button>
            <a class="btn btn-primary" href="${loginHref}">${icon("logout", 16)}LINUX DO 登录</a>
          </div>
        </div>
      </header>
      <main class="shell landing">
        <section class="hero">
          <div>
            <div class="hero-badge"><span class="pulse-dot"></span>LINUX DO Connect · 原生 OAuth · Level 0</div>
            <h1>把每一份额度，<br>花在<span class="gradient-text">真正需要的模型</span>。</h1>
            <p class="hero-desc">${esc(boot.app.description)}。签到养成套餐，按档位调用渠道模型，Key 只保留过期时间与可用模型两个选项。</p>
            <div class="hero-cta">
              <a class="btn btn-primary btn-lg" href="${loginHref}">${icon("logout", 18)}使用 LINUX DO 登录</a>
              <a class="btn btn-soft btn-lg" href="#/plans">查看套餐 ${icon("arrowRight", 16)}</a>
              <button type="button" class="btn btn-soft btn-lg" id="zcode-contribute-btn" title="贡献一个 ZCode 账号加入算力池，无奖励，纯公益">${icon("heart", 16)}贡献 ZCode 账号</button>
            </div>
            <div class="hero-meta">
              <span>${icon("shield", 13)}OAuth state + PKCE</span>
              <span>${icon("clock", 13)}滚动 5H / 7D / 30D</span>
              <span>${icon("layers", 13)}档位 × 渠道授权</span>
            </div>
            ${rootNote}
            ${regNote}
          </div>
          <div class="hero-visual">
            <div class="floating-tag floating-tag-a">${icon("spark", 14)}连续签到升级</div>
            <div class="visual-card">
              <div class="visual-top">
                <span class="visual-title">YOUR PLAN</span>
                <span class="visual-plan">${icon("crown", 11)} STANDARD</span>
              </div>
              <div class="visual-balance"><strong>$8.42</strong><span>/ $10.00 · 5H</span></div>
              <div class="visual-sub">滚动窗口 · 30 分钟后重置部分额度</div>
              <div class="mini-bars" aria-hidden="true">
                ${[32, 46, 38, 60, 52, 74, 66, 88, 70, 58, 78, 92, 64, 48, 72, 56]
                  .map((h) => `<i style="height:${h}%"></i>`)
                  .join("")}
              </div>
              <div class="visual-foot"><span>${icon("key", 12)} sk-pm-••••8f2a</span><span>12 个模型可用</span></div>
            </div>
            <div class="floating-tag floating-tag-b">${icon("check", 14)}额度硬性拦截</div>
          </div>
        </section>

        <section class="section">
          <div class="section-head">
            <div>
              <h2>签到养成的套餐体系</h2>
              <p>连续签到自动升级，断签保留已获得的最高套餐。Pro / Max 需站长审批。</p>
            </div>
          </div>
          <div class="plan-grid">${plans.map((plan) => planCard(plan)).join("")}</div>
        </section>

        <section class="section">
          <div class="section-head">
            <div>
              <h2>简单，但不简陋</h2>
              <p>独立实现的 Token Plan 分发平台，没有额外的权限层级。</p>
            </div>
          </div>
          <div class="feature-grid">
            <div class="feature">
              <div class="feature-icon">${icon("shield", 20)}</div>
              <h3>LINUX DO 原生登录</h3>
              <p>授权码流程、state 防 CSRF、PKCE 校验，OAuth 凭据加密落库。首位用户自动成为站长。</p>
            </div>
            <div class="feature">
              <div class="feature-icon">${icon("zap", 20)}</div>
              <h3>滚动限额，硬性拦截</h3>
              <p>套餐 5H / 7D / 30D 滚动窗口，请求前校验、请求后记账，超出即返回 429 并写入用量审计。</p>
            </div>
            <div class="feature">
              <div class="feature-icon">${icon("layers", 20)}</div>
              <h3>档位 × 渠道授权</h3>
              <p>站长可为渠道或具体模型勾选可调用的套餐档位。用户能调用什么，只由套餐和渠道授权的交集决定。</p>
            </div>
          </div>
        </section>

        <footer class="footer">
          <span>PM-Plan · ${esc(boot.app.homepage)}</span>
          <span>LINUX DO Connect · 0 级应用</span>
        </footer>
      </main>`;
    bindAvatarFallbacks();
  }

  /* ---------- dashboard ---------- */

  function planHero(me) {
    const plan = me.plan;
    const usage = me.usage;
    const pending = me.pending_applications || [];
    const windows = [];
    if (plan.limits.h5 !== null) {
      windows.push({ key: "5H", used: usage.h5_used, limit: plan.limits.h5 });
    }
    if (plan.limits.weekly !== null) {
      windows.push({ key: "7D", used: usage.weekly_used, limit: plan.limits.weekly });
    }
    if (plan.limits.monthly !== null) {
      windows.push({ key: "30D", used: usage.monthly_used, limit: plan.limits.monthly });
    }
    const percents = windows.map((item) => pct(item.used, item.limit));
    const ringPercent = percents.length ? Math.max(...percents) : 0;
    const pendingLabel = pending.length
      ? `<span class="badge badge-warning">${esc(pending[0].target_plan.toUpperCase())} 候补审核中</span>`
      : "";
    return `
      <div class="card plan-hero">
        <div class="plan-hero-top">
          <div>
            <div class="plan-eyebrow">CURRENT PLAN</div>
            <h2 class="plan-hero-name">${esc(plan.name)}</h2>
            <p class="plan-hero-tagline">${esc(plan.tagline)}</p>
            <div class="flex flex-wrap mt-12">${planBadge(plan.code, plan.name)}${pendingLabel}</div>
          </div>
          <div class="plan-ring" style="--ring:${Math.min(100, ringPercent).toFixed(2)}%">
            <div class="plan-ring-inner">
              <div><strong>${Math.round(ringPercent)}%</strong>已用</div>
            </div>
          </div>
        </div>
        <div class="quota-list">
          ${windows
            .map(
              (item) => `
            <div class="quota">
              <div class="quota-top">
                <span class="quota-label">${item.key} 滚动额度</span>
                <span class="quota-value"><strong>${money(item.used)}</strong> <span>/ ${money(item.limit)}</span></span>
              </div>
              <div class="progress ${barClass(pct(item.used, item.limit))}"><i style="width:${pct(item.used, item.limit).toFixed(2)}%"></i></div>
            </div>`
            )
            .join("")}
          ${windows.length === 0 ? `<div class="faint">当前套餐没有额度限制。</div>` : ""}
        </div>
        <div class="plan-foot">
          <span>额度窗口按滚动时间实时计算 · 不累计、不结转</span>
          <a class="card-link" href="#/plans">查看套餐权益 ${icon("arrowRight", 12)}</a>
        </div>
      </div>`;
  }

  function checkinCard(me) {
    const checkin = me.checkin;
    const recent = new Set(checkin.recent || []);
    const days = [];
    for (let i = 13; i >= 0; i -= 1) {
      days.push(addDays(checkin.today, -i));
    }
    const next = checkin.next_auto;
    return `
      <div class="card checkin-card">
        <div class="card-head" style="justify-content:center;margin-bottom:14px">
          <div>
            <div class="card-title">每日签到</div>
            <div class="card-sub">连续签到，自动升级套餐</div>
          </div>
        </div>
        <div class="streak-num">${checkin.streak}</div>
        <div class="streak-label">连续签到天数</div>
        <div class="streak-longest">最长纪录 ${checkin.longest_streak} 天</div>
        <div class="calendar">
          ${days
            .map((day) => {
              const cls = recent.has(day) ? "done" : day === checkin.today ? "today" : "";
              return `<span class="cal-day ${cls}" title="${esc(day)}">${dayLabel(day)}</span>`;
            })
            .join("")}
        </div>
        <button type="button" class="btn btn-mint btn-block" data-action="checkin" ${checkin.checked_in_today ? "disabled" : ""}>
          ${checkin.checked_in_today ? `${icon("check", 16)}今日已签到` : `${icon("calendar", 16)}今日签到`}
        </button>
        ${(() => {
          const pending = me.pending_applications || [];
          if (pending.length) {
            return `<div class="checkin-note">${esc(pending[0].target_plan.toUpperCase())} 候补审核中，等待站长处理。</div>`;
          }
          if (next) {
            return `<div class="checkin-note">再连续签到 <strong class="text-mint">${next.days_left}</strong> 天升级 ${esc(next.name)}</div>`;
          }
          if (me.user.plan === "max") {
            return `<div class="checkin-note">已是最高档位。保持签到记录，享受 Max 额度。</div>`;
          }
          if (checkin.streak >= 100) {
            return `<div class="checkin-note">已达成 100 天里程碑；Max 最终归属以站长审批为准。</div>`;
          }
          if (checkin.streak >= 50) {
            return `<div class="checkin-note">已达成 50 天里程碑；Pro 最终归属以站长审批为准。</div>`;
          }
          return `<div class="checkin-note">保持连续签到，第 15 / 30 天自动升级。</div>`;
        })()}
      </div>`;
  }

  // 补签卡 / 重置卡: 补签最近 7 天漏签, 重置卡清零 5h/7d/30d 已用额度
  function cardsSection(me) {
    const makeup = me.makeup_cards || [];
    const resets = me.reset_cards || [];
    const active = makeup.filter((c) => c.status === "issued" && new Date(c.expires_at) > new Date());
    const normal = active.filter((c) => (c.kind || "normal") === "normal");
    const plus = active.filter((c) => c.kind === "plus");
    const recent = new Set(me.checkin.recent || []);
    const missed = [];
    for (let i = 1; i <= 7; i += 1) {
      const day = addDays(me.checkin.today, -i);
      if (!recent.has(day)) missed.push(day);
    }
    const issued = resets.filter((c) => c.status === "issued");
    return `
      <div class="card">
        <div class="card-head"><div><div class="card-title">补签卡 / 重置卡</div><div class="card-sub">补签最近 7 天漏签 · 重置卡清零额度</div></div></div>
        ${
          normal.length
            ? `
          <div class="field"><label>补签日期（最近 7 天内漏签）</label>
            <select class="input" id="makeup-date">
              ${missed.map((d) => `<option value="${esc(d)}">${esc(d)}</option>`).join("")}
            </select>
          </div>
          <button type="button" class="btn btn-mint btn-block" data-action="use-makeup" data-id="${normal[0].id}" ${missed.length ? "" : "disabled"}>
            ${icon("calendar", 16)}使用补签卡（剩余 ${normal.length} 张）
          </button>
          <div class="checkin-note">补签卡 ${fmtDate(normal[0].expires_at)} 前有效；补签后重算连续签到。</div>`
            : ""
        }
        ${
          plus.length
            ? `
          <div class="field"><label>Plus补签卡：补齐7天内所有漏签</label></div>
          <button type="button" class="btn btn-mint btn-block" data-action="use-makeup-plus" data-id="${plus[0].id}" data-missed="${missed.join(",")}" ${missed.length ? "" : "disabled"}>
            ${icon("calendar", 16)}使用 Plus 补签卡${missed.length ? `（补齐 ${missed.length} 天漏签）` : "（最近 7 天无漏签）"}
          </button>
          <div class="checkin-note">Plus补签卡 ${fmtDate(plus[0].expires_at)} 前有效；一次补齐 7 天内所有漏签，补签后重算连续签到。</div>`
            : ""
        }
        ${!active.length ? `<div class="checkin-note">暂无可用补签卡，站长发放后这里会显示。</div>` : ""}
        <div class="divider"></div>
        <div class="field"><label>兑换重置卡</label>
          <div style="display:flex;gap:8px">
            <input class="input" id="reset-code-input" placeholder="PMRC-XXXX-XXXX-XXXX-XXXX" autocomplete="off">
            <button type="button" class="btn btn-soft" data-action="redeem-reset">兑换</button>
          </div>
        </div>
        ${
          issued.length
            ? `<div class="mini-list">${issued
                .map(
                  (c) => `<div class="mini-row"><div class="mini-row-main">${icon("key", 14)}<strong>${esc(c.code_prefix)}…</strong><span>${c.expires_at ? `有效期至 ${fmtDate(c.expires_at)}` : "长期有效"}</span></div>
            <button type="button" class="btn btn-soft btn-sm" data-action="use-reset" data-id="${c.id}">使用</button></div>`
                )
                .join("")}</div>
          <div class="checkin-note">使用后 5 小时 / 7 天 / 30 天已用额度全部清零。</div>`
            : ""
        }
      </div>`;
  }

  function modelsCard(me) {
    const models = me.available_models || [];
    const shown = models.slice(0, 12);
    return `
      <div class="card">
        <div class="card-head">
          <div>
            <div class="card-title">当前可用模型</div>
            <div class="card-sub">套餐档位与渠道授权的交集</div>
          </div>
          <span class="badge badge-muted">${models.length} 个</span>
        </div>
        ${
          models.length
            ? `<div class="model-chips">${shown
                .map((model) => `<span class="model-chip">${esc(model)}</span>`)
                .join("")}${models.length > shown.length ? `<span class="model-chip more">+${models.length - shown.length} 更多</span>` : ""}</div>`
            : `<div class="faint" style="font-size:12px;line-height:1.7">还没有可用模型。等站长添加渠道并配置档位授权后，这里会自动出现。</div>`
        }
        <div class="divider"></div>
        <div class="mini-row">
          <span class="muted">创建 Key 时可不选模型</span>
          <span class="faint">默认动态覆盖全部</span>
        </div>
      </div>`;
  }

  function dailyChart(points) {
    if (!points || points.length === 0) {
      return `<div class="chart-empty">最近 7 天还没有调用记录</div>`;
    }
    const total = points.reduce((sum, point) => sum + (Number(point.cost_micro) || 0), 0);
    if (total <= 0) {
      return `<div class="chart-empty">最近 7 天还没有产生消耗</div>`;
    }
    const max = Math.max(...points.map((point) => Number(point.cost_micro) || 0), 1);
    return `<div class="chart">
      ${points
        .map((point) => {
          const height = Math.max(3, (Number(point.cost_micro) / max) * 100);
          return `<div class="chart-col" title="${esc(point.day)} · ${point.requests} 次 · ${moneyExact(point.cost_micro)}">
            <div class="chart-bar-wrap"><div class="chart-bar" style="height:${height.toFixed(1)}%"></div></div>
            <div class="chart-label">${esc(point.day)}</div>
          </div>`;
        })
        .join("")}
    </div>`;
  }

  function usageTable(logs, emptyText = "还没有调用记录", showUser = false) {
    if (!logs || logs.length === 0) {
      return `<div class="empty"><div class="empty-icon">${icon("chart", 20)}</div>${esc(emptyText)}</div>`;
    }
    return `<div class="table-wrap"><table>
      <thead><tr>${showUser ? "<th>用户</th>" : ""}<th>模型</th><th>渠道</th><th class="num">Token</th><th class="num">费用</th><th>状态</th><th>时间</th></tr></thead>
      <tbody>${logs
        .map(
          (log) => `<tr>
            ${showUser ? `<td><span class="cell-strong">${esc(log.user_name || "—")}</span><div class="cell-sub">${esc(log.key_prefix || "")}</div></td>` : ""}
            <td><span class="cell-strong mono" style="font-size:11.5px">${esc(log.model || "—")}</span>${log.error_code ? `<div class="cell-sub text-danger">${esc(log.error_code)}</div>` : ""}</td>
            <td><span class="muted">${esc(log.channel_name || "—")}</span></td>
            <td class="num">${Number(log.total_tokens || 0).toLocaleString()}</td>
            <td class="num">${moneyExact(log.cost_micro)}</td>
            <td>${log.status < 400 ? `<span class="badge badge-success">${log.status}</span>` : `<span class="badge badge-danger">${log.status}</span>`}</td>
            <td><span class="faint">${fmtShortDate(log.created_at)}</span></td>
          </tr>`
        )
        .join("")}</tbody>
    </table></div>`;
  }

  function noticesSection(me) {
    const notices = (me.notices || []).filter((n) => !n.is_read);
    if (!notices.length) return "";
    return `<div style="margin-bottom:16px;display:grid;gap:12px">${notices
      .map(
        (n) => `<div class="notice danger" role="alert">
          ${icon("alert", 16)}
          <div style="flex:1;min-width:0">
            <strong>${esc(n.title)}</strong>
            <div style="white-space:pre-wrap;line-height:1.7;margin-top:4px">${esc(n.body)}</div>
            <div class="faint" style="font-size:11px;margin-top:6px">${esc(fmtShortDate(n.created_at))}</div>
          </div>
          <button type="button" class="btn btn-soft btn-sm" data-action="notice-read" data-id="${n.id}">我知道了</button>
        </div>`
      )
      .join("")}</div>`;
  }

  function renderDashboard(me) {
    const user = me.user;
    const hour = new Date().getHours();
    const greeting = hour < 6 ? "夜深了" : hour < 12 ? "早上好" : hour < 18 ? "下午好" : "晚上好";
    const pending = me.pending_applications || [];
    return `
      <div class="page-head">
        <div>
          <div class="faint" style="font-size:11px;letter-spacing:.12em;text-transform:uppercase">DASHBOARD</div>
          <h1 class="page-title">${greeting}，${esc(user.display_name || user.username)}</h1>
          <p class="page-sub">今天是 ${esc(me.checkin.today)}，${me.checkin.checked_in_today ? "已完成签到，继续保持。" : "别忘了签到，连续天数会累积。"}</p>
        </div>
        <div class="head-actions">
          <span class="badge badge-${esc(user.plan)}">${icon("crown", 11)}${esc(me.plan.name)}</span>
          ${user.is_root ? `<span class="badge badge-root">站长</span>` : ""}
          <a class="btn btn-soft btn-sm" href="#/keys">${icon("key", 13)}管理 Keys</a>
        </div>
      </div>
      ${noticesSection(me)}
      <div class="dashboard-grid">
        <div class="stack">
          ${planHero(me)}
          <div class="stat-grid">
            <div class="stat"><div class="stat-label">30 天请求</div><div class="stat-value">${Number(me.usage.requests_30d || 0).toLocaleString()}<small> 次</small></div><div class="stat-foot">全部 API 调用</div></div>
            <div class="stat"><div class="stat-label">30 天 Token</div><div class="stat-value">${Number(me.usage.tokens_30d || 0).toLocaleString()}<small> tokens</small></div><div class="stat-foot">输入 + 输出</div></div>
            <div class="stat"><div class="stat-label">有效 Keys</div><div class="stat-value">${me.user.key_count}<small> 个</small></div><div class="stat-foot">额度继承当前套餐</div></div>
          </div>
          <div class="card">
            <div class="card-head">
              <div><div class="card-title">最近 7 天消耗</div><div class="card-sub">按自然日聚合 · 单位为美元</div></div>
              <span class="badge badge-muted">${Number(me.usage.requests_30d || 0)} 次请求</span>
            </div>
            ${dailyChart(me.daily_usage)}
          </div>
          <div class="card">
            <div class="card-head">
              <div><div class="card-title">最近调用</div><div class="card-sub">实时来自用量审计</div></div>
              <a class="card-link" href="#/usage">全部记录 ${icon("arrowRight", 12)}</a>
            </div>
            ${usageTable(me.recent_usage, "还没有调用记录，创建 Key 后即可开始")}
          </div>
        </div>
        <div class="stack">
          ${checkinCard(me)}
          ${cardsSection(me)}
          ${modelsCard(me)}
          ${
            pending.length
              ? `<div class="card">
                  <div class="card-head"><div><div class="card-title">晋升申请</div><div class="card-sub">等待站长处理</div></div></div>
                  <div class="mini-list">${pending
                    .map(
                      (application) => `<div class="mini-row"><div class="mini-row-main">${icon("crown", 14)}<strong>${esc(application.target_plan.toUpperCase())}</strong><span>候补</span></div><span class="badge badge-warning">审核中</span></div>`
                    )
                    .join("")}</div>
                </div>`
              : `<div class="notice">${icon("info", 16)}<div>连续签到 50 天进入 Pro 候补、100 天进入 Max 候补；站长审批通过后生效。</div></div>`
          }
        </div>
      </div>`;
  }

  /* ---------- keys ---------- */

  function keyStatusBadge(key) {
    if (key.status === "active") return `<span class="badge badge-success">${statusDot("active")}有效</span>`;
    if (key.status === "expired") return `<span class="badge badge-warning">${statusDot("expired")}已过期</span>`;
    return `<span class="badge badge-muted">${statusDot("revoked")}已撤销</span>`;
  }

  function renderKeys(data) {
    const { me, keys, models } = data;
    const available = (models.models || []).map((model) => model.name);
    return `
      <div class="page-head">
        <div>
          <h1 class="page-title">Keys</h1>
          <p class="page-sub">Key 只有两个可配置项：过期时间与可用模型。额度完全继承当前套餐。</p>
        </div>
        <div class="head-actions">
          <button type="button" class="btn btn-primary" data-action="open-key-modal">${icon("plus", 15)}创建 Key</button>
        </div>
      </div>
      <div class="notice mint" style="margin-bottom:18px">${icon("info", 16)}<div><strong>未指定模型 = 动态全部可用。</strong>Key 不绑定模型快照，会随套餐升降与渠道授权变化实时计算；已指定的模型仍会做二次校验。</div></div>
      <div class="key-list">
        ${
          keys.keys.length
            ? keys.keys
                .map(
                  (key) => `<div class="key-card">
                    <div class="key-main">
                      <div class="flex flex-wrap"><span class="key-prefix">${esc(key.key_prefix)}</span>${keyStatusBadge(key)}</div>
                      <div class="key-meta">
                        <span>${icon("clock", 11)} 过期：${key.expires_at ? fmtDate(key.expires_at) : "永久"}</span>
                        <span>创建：${fmtShortDate(key.created_at)}</span>
                        <span>最近使用：${key.last_used_at ? fmtShortDate(key.last_used_at) : "从未"}</span>
                      </div>
                      <div class="key-models model-chips">
                        ${
                          key.all_models
                            ? `<span class="model-chip" style="color:var(--accent)">全部可用模型 · 动态</span>`
                            : key.models.map((model) => `<span class="model-chip">${esc(model)}</span>`).join("")
                        }
                      </div>
                    </div>
                    <div class="key-side">
                      ${key.status === "active" ? `<button type="button" class="btn btn-danger btn-sm" data-action="revoke-key" data-id="${key.id}" data-prefix="${esc(key.key_prefix)}">${icon("trash", 13)}撤销</button>` : ""}
                    </div>
                  </div>`
                )
                .join("")
            : `<div class="card"><div class="empty"><div class="empty-icon">${icon("key", 20)}</div>还没有创建 Key<br><button type="button" class="btn btn-primary btn-sm mt-16" data-action="open-key-modal">${icon("plus", 13)}创建第一个 Key</button></div></div>`
        }
      </div>
      <div class="card mt-24">
        <div class="card-head"><div><div class="card-title">调用方式</div><div class="card-sub">兼容 OpenAI Chat Completions</div></div></div>
        <div class="secret-box"><code>curl ${esc(location.origin)}/v1/chat/completions \\<br>  -H "Authorization: Bearer sk-pm-..." \\<br>  -H "Content-Type: application/json" \\<br>  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"你好"}]}'</code></div>
        <p class="field-hint mt-12">可用模型：${available.length ? available.slice(0, 10).map(esc).join(" · ") + (available.length > 10 ? " 等" : "") : "暂无"}</p>
      </div>`;
  }

  function openKeyModal(models) {
    const modelList = models.models || [];
    openModal({
      title: "创建 Key",
      subtitle: "只设置过期时间与可用模型，其余全部继承套餐",
      wide: true,
      body: `
        <div class="field">
          <label for="key-expiry">过期时间</label>
          <select class="select" id="key-expiry" name="expiry">
            <option value="7">7 天后过期</option>
            <option value="30" selected>30 天后过期</option>
            <option value="90">90 天后过期</option>
            <option value="never">永不过期</option>
            <option value="custom">自定义时间</option>
          </select>
          <input class="input" type="datetime-local" id="key-expiry-custom" style="display:none;margin-top:8px">
        </div>
        <div class="field">
          <div class="flex between" style="gap:12px;flex-wrap:wrap">
            <label>可用模型</label>
            <span class="faint" style="font-size:11px">不勾选 = 全部当前及未来可用模型</span>
          </div>
          <div class="search-input mb-0" style="margin-bottom:8px">
            ${icon("search", 14)}
            <input class="input" id="key-model-search" placeholder="搜索模型…" autocomplete="off">
          </div>
          ${
            modelList.length
              ? `<div class="check-grid" id="key-model-list">
                  ${modelList
                    .map(
                      (model) => `<label class="check" data-model-name="${esc(model.name.toLowerCase())}">
                        <input type="checkbox" name="key-model" value="${esc(model.name)}">
                        <span>${esc(model.name)}<br><span class="faint" style="font-size:10px">${money(model.input_micro)} / ${money(model.output_micro)} 每 1M</span></span>
                      </label>`
                    )
                    .join("")}
                </div>
                <div class="flex mt-8" style="gap:6px">
                  <button type="button" class="btn btn-ghost btn-sm" id="key-select-all">全选</button>
                  <button type="button" class="btn btn-ghost btn-sm" id="key-clear-all">清空</button>
                  <span class="faint" style="font-size:11px">已选 <span id="key-selected-count">0</span> 个</span>
                </div>`
              : `<div class="notice">${icon("alert", 16)}<div>当前套餐还没有可用模型，无法指定模型。你仍然可以创建一个不指定模型的 Key，等渠道配置完成后自动生效。</div></div>`
          }
        </div>
        <div class="notice">${icon("info", 16)}<div>Key 的额度完全继承当前套餐的 5H / 7D / 30D 限额，不可单独设置额度、IP 白名单或渠道绑定。</div></div>`,
      footer: `
        <button type="button" class="btn btn-ghost" data-action="close-modal">取消</button>
        <button type="button" class="btn btn-primary" id="key-submit">${icon("key", 14)}生成 Key</button>`,
      onMount(root) {
        const expiry = root.querySelector("#key-expiry");
        const custom = root.querySelector("#key-expiry-custom");
        const search = root.querySelector("#key-model-search");
        const checks = () => Array.from(root.querySelectorAll('input[name="key-model"]'));
        const count = root.querySelector("#key-selected-count");
        const updateCount = () => {
          if (count) count.textContent = String(checks().filter((item) => item.checked).length);
        };
        expiry.addEventListener("change", () => {
          custom.style.display = expiry.value === "custom" ? "block" : "none";
        });
        checks().forEach((item) => item.addEventListener("change", updateCount));
        root.querySelector("#key-select-all")?.addEventListener("click", () => {
          checks().forEach((item) => {
            if (item.closest(".check").style.display !== "none") item.checked = true;
          });
          updateCount();
        });
        root.querySelector("#key-clear-all")?.addEventListener("click", () => {
          checks().forEach((item) => (item.checked = false));
          updateCount();
        });
        search?.addEventListener("input", () => {
          const query = search.value.trim().toLowerCase();
          root.querySelectorAll("#key-model-list .check").forEach((label) => {
            label.style.display = !query || label.dataset.modelName.includes(query) ? "" : "none";
          });
        });
        root.querySelector("#key-submit").addEventListener("click", async (event) => {
          event.preventDefault();
          const button = event.currentTarget;
          button.disabled = true;
          try {
            let expiresAt = null;
            if (expiry.value !== "never") {
              if (expiry.value === "custom") {
                if (!custom.value) throw new Error("请选择自定义过期时间");
                expiresAt = new Date(custom.value).toISOString();
              } else {
                expiresAt = new Date(Date.now() + Number(expiry.value) * 86400000).toISOString();
              }
            }
            const selected = checks().filter((item) => item.checked).map((item) => item.value);
            const result = await api("/api/keys", {
              method: "POST",
              body: { expires_at: expiresAt, models: selected },
            });
            closeModal();
            window.setTimeout(() => showSecretModal(result), 220);
            toast("Key 创建成功", "success");
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function showSecretModal(result) {
    openModal({
      title: "Key 已生成",
      subtitle: "这是唯一一次完整展示，请立即复制保存",
      body: `
        <div class="secret-box"><code id="secret-value">${esc(result.key)}</code><button type="button" class="btn btn-soft btn-sm" data-action="copy-secret" data-copy="${esc(result.key)}">${icon("copy", 13)}复制</button></div>
        <div class="mini-list mt-16">
          <div class="mini-row"><span class="muted">可用模型</span><span>${result.all_models ? "全部可用模型（动态）" : `${result.models.length} 个指定模型`}</span></div>
          <div class="mini-row"><span class="muted">过期时间</span><span>${result.expires_at ? fmtDate(result.expires_at) : "永久"}</span></div>
          <div class="mini-row"><span class="muted">额度</span><span>继承当前套餐限额</span></div>
        </div>
        <div class="notice danger mt-16">${icon("alert", 16)}<div>平台只保存 Key 的哈希，关闭后无法再次查看完整 Key。</div></div>`,
      footer: `<button type="button" class="btn btn-primary" data-action="close-modal">我已保存</button>`,
    });
  }

  /* ---------- usage ---------- */

  function renderUsage(data) {
    const { me, logs } = data;
    const totalPages = Math.max(1, logs.pages || 1);
    return `
      <div class="page-head">
        <div><h1 class="page-title">用量记录</h1><p class="page-sub">每一次 API 调用都会写入用量审计，包含档位拦截与额度拒绝。</p></div>
        <div class="head-actions"><span class="badge badge-${esc(me.user.plan)}">${esc(me.plan.name)}</span><span class="badge badge-muted">共 ${Number(logs.total || 0)} 条</span></div>
      </div>
      <div class="stat-grid" style="margin-bottom:18px">
        <div class="stat"><div class="stat-label">5H 已用</div><div class="stat-value">${money(me.usage.h5_used)}</div><div class="stat-foot">限额 ${money(me.usage.h5_limit)}</div></div>
        <div class="stat"><div class="stat-label">7D 已用</div><div class="stat-value">${money(me.usage.weekly_used)}</div><div class="stat-foot">限额 ${money(me.usage.weekly_limit)}</div></div>
        <div class="stat"><div class="stat-label">30D 已用</div><div class="stat-value">${money(me.usage.monthly_used)}</div><div class="stat-foot">限额 ${money(me.usage.monthly_limit)}</div></div>
      </div>
      <div class="card">
        ${usageTable(logs.logs, "还没有调用记录")}
        <div class="pagination">
          <span>第 ${logs.page} / ${totalPages} 页</span>
          <div class="pager">
            <button type="button" class="btn btn-soft btn-sm" data-action="usage-page" data-page="${logs.page - 1}" ${logs.page <= 1 ? "disabled" : ""}>${icon("arrowLeft", 13)}上一页</button>
            <button type="button" class="btn btn-soft btn-sm" data-action="usage-page" data-page="${logs.page + 1}" ${logs.page >= totalPages ? "disabled" : ""}>下一页${icon("arrowRight", 13)}</button>
          </div>
        </div>
      </div>`;
  }

  /* ---------- plans ---------- */

  function renderPlans(data) {
    const { me, plans } = data;
    return `
      <div class="page-head">
        <div><h1 class="page-title">套餐体系</h1><p class="page-sub">连续签到自动升级；断签保留已获得的最高套餐；Pro / Max 以站长审批为准。</p></div>
        <div class="head-actions"><span class="badge badge-${esc(me.user.plan)}">${icon("crown", 11)}当前 ${esc(me.plan.name)}</span></div>
      </div>
      <div class="plan-grid">${plans.map((plan) => planCard(plan, me.user.plan)).join("")}</div>
      <div class="dashboard-grid mt-24">
        <div class="card">
          <div class="card-head"><div><div class="card-title">升级规则</div><div class="card-sub">按自然日计算连续签到</div></div></div>
          <div class="mini-list">
            <div class="mini-row"><div class="mini-row-main">${icon("spark", 14)}<strong>15 天</strong><span>自动升级 Essential</span></div><span class="badge badge-essential">$5 / $30 / $75</span></div>
            <div class="mini-row"><div class="mini-row-main">${icon("spark", 14)}<strong>30 天</strong><span>自动升级 Standard</span></div><span class="badge badge-standard">$10 / $35 / ∞</span></div>
            <div class="mini-row"><div class="mini-row-main">${icon("crown", 14)}<strong>50 天</strong><span>进入 Pro 候补池</span></div><span class="badge badge-pro">站长审批</span></div>
            <div class="mini-row"><div class="mini-row-main">${icon("crown", 14)}<strong>100 天</strong><span>进入 Max 候补池</span></div><span class="badge badge-max">站长审批</span></div>
          </div>
          <div class="divider"></div>
          <p class="field-hint">断签后连续计数重置，但已获得的最高套餐保留。Pro / Max 的最终归属以站长审批结果为准，站长也可手动授予或撤销。</p>
        </div>
        <div class="card">
          <div class="card-head"><div><div class="card-title">额度窗口</div><div class="card-sub">全部为滚动窗口</div></div></div>
          <div class="quota-list">
            <div class="quota"><div class="quota-top"><span class="quota-label">5 小时</span><span class="quota-value"><strong>${money(me.usage.h5_used)}</strong> <span>/ ${money(me.usage.h5_limit)}</span></span></div><div class="progress ${barClass(pct(me.usage.h5_used, me.usage.h5_limit))}"><i style="width:${pct(me.usage.h5_used, me.usage.h5_limit)}%"></i></div></div>
            <div class="quota"><div class="quota-top"><span class="quota-label">7 天</span><span class="quota-value"><strong>${money(me.usage.weekly_used)}</strong> <span>/ ${money(me.usage.weekly_limit)}</span></span></div><div class="progress ${barClass(pct(me.usage.weekly_used, me.usage.weekly_limit))}"><i style="width:${pct(me.usage.weekly_used, me.usage.weekly_limit)}%"></i></div></div>
            <div class="quota"><div class="quota-top"><span class="quota-label">30 天</span><span class="quota-value"><strong>${money(me.usage.monthly_used)}</strong> <span>/ ${money(me.usage.monthly_limit)}</span></span></div><div class="progress ${barClass(pct(me.usage.monthly_used, me.usage.monthly_limit))}"><i style="width:${pct(me.usage.monthly_used, me.usage.monthly_limit)}%"></i></div></div>
          </div>
          <div class="divider"></div>
          <p class="field-hint">费用按模型输入 / 输出价格与 Token 用量计算；未配置价格的模型使用全局默认价。额度用尽时 API 返回 429，并记录到用量审计。</p>
        </div>
      </div>`;
  }

  /* ---------- model square ---------- */

  // 模型图标: 用内联 SVG + 品牌色圆底(无外链依赖)
  const MODEL_BRANDS = [
    // 自有/第三方聚合模型(按具体前缀优先匹配)
    { test: /^(agnes|jev)/i, key: "aggregate", label: "Aggregate", color: "#6366f1" },
    { test: /^mini(max)?/i, key: "minimax", label: "MiniMax", color: "#e73562" },
    { test: /^(gpt|o1|o3|o4|chatgpt|davinci|text-|tts|whisper)/i, key: "openai", label: "OpenAI", color: "#10a37f" },
    { test: /^claude/i, key: "anthropic", label: "Anthropic", color: "#d97757" },
    { test: /^(gemini|palm|bison)/i, key: "google", label: "Google", color: "#4285f4" },
    { test: /^(grok|xai)/i, key: "xai", label: "xAI", color: "#111827" },
    { test: /^(deepseek|ds-)/i, key: "deepseek", label: "DeepSeek", color: "#4d6bfe" },
    { test: /^(qwen|qwq|qvd|tongyi)/i, key: "qwen", label: "Qwen", color: "#615ced" },
    { test: /^(glm|chatglm|zhipu)/i, key: "zhipu", label: "智谱", color: "#3b6cf6" },
    { test: /^(moonshot|kimi)/i, key: "moonshot", label: "Moonshot", color: "#1f2937" },
    { test: /^(llama|meta-)/i, key: "meta", label: "Meta", color: "#0866ff" },
    { test: /^(mistral|mixtral|codestral|magistral)/i, key: "mistral", label: "Mistral", color: "#fa520f" },
    { test: /^(yi-|01-)/i, key: "yi", label: "01.AI", color: "#0ea5e9" },
    { test: /^(doubao|skylark|ep-)/i, key: "bytedance", label: "豆包", color: "#3b82f6" },
    { test: /^(ernie|wenxin)/i, key: "baidu", label: "文心", color: "#2932e1" },
    { test: /^(hunyuan)/i, key: "tencent", label: "混元", color: "#0ea5e9" },
    { test: /^(command|cohere)/i, key: "cohere", label: "Cohere", color: "#39594d" },
    { test: /^(gpt-oss|oss)/i, key: "oss", label: "OSS", color: "#6b7280" },
  ];

  function modelBrand(name) {
    const n = String(name || "");
    for (const b of MODEL_BRANDS) {
      if (b.test.test(n)) return b;
    }
    return { key: "generic", label: "Model", color: "#8b93a7" };
  }

  // 简洁的字母/图形商标(SVG, 避免外链图片被 CSP/网络挡)
  const BRAND_GLYPH = {
    openai: '<path d="M12 3.2a4.2 4.2 0 0 1 3.6 2l2.6 1.5a4.2 4.2 0 0 1 .1 4.1 4.2 4.2 0 0 1-.1 4.1l-2.6 1.5a4.2 4.2 0 0 1-3.6 2 4.2 4.2 0 0 1-3.6-2l-2.6-1.5a4.2 4.2 0 0 1-.1-4.1 4.2 4.2 0 0 1 .1-4.1l2.6-1.5A4.2 4.2 0 0 1 12 3.2Z" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="12" cy="12" r="2.2" fill="currentColor"/>',
    anthropic: '<path d="M6 19 12 5l6 14M8.2 14.4h7.6" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>',
    google: '<path d="M21 12.2c0-.6-.06-1.2-.16-1.8H12v3.4h5.05a4.3 4.3 0 0 1-1.9 2.8v2.3h3.1c1.8-1.7 2.75-4.15 2.75-6.7Z" fill="currentColor"/><path d="M12 22c2.6 0 4.8-.86 6.25-2.34l-3.1-2.3c-.86.58-1.96.92-3.15.92-2.42 0-4.47-1.63-5.2-3.83H3.6v2.4A9.9 9.9 0 0 0 12 22Z" fill="currentColor" opacity=".75"/><path d="M6.8 14.45A6 6 0 0 1 6.5 12c0-.85.15-1.68.3-2.45V7.15H3.6A9.9 9.9 0 0 0 2 12c0 1.75.42 3.4 1.6 4.85l3.2-2.4Z" fill="currentColor" opacity=".5"/>',
    xai: '<path d="M5 5h3.2l4 5.6L16.4 5H19l-5.6 7 5.9 7h-3.2l-4.3-5.9L7.6 19H5l5.9-7.2L5 5Z" fill="currentColor"/>',
    deepseek: '<path d="M12 3c-3 2.2-5.6 4.6-5.6 8.4 0 3.6 2.4 6.6 5.6 8.6 3.2-2 5.6-5 5.6-8.6C17.6 7.6 15 5.2 12 3Z" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M12 8.5v7" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>',
    qwen: '<path d="M12 3.5 4.5 7.8v8.4L12 20.5l7.5-4.3V7.8L12 3.5Z" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M12 8.2v7.6M8.4 10.2v3.6M15.6 10.2v3.6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>',
    zhipu: '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8 12h8M12 8v8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>',
    moonshot: '<path d="M12 4c2.6 0 4.6 3.1 4.6 7S14.6 20 12 20s-4.6-3.1-4.6-9S9.4 4 12 4Z" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M7.4 12h9.2" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>',
    meta: '<path d="M4 15.5c0-4 2-8 4-8s2.6 2.4 4 6c1.4-3.6 2-6 4-6s4 4 4 8" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/>',
    mistral: '<path d="M5 5h3v3H5zM10.5 5h3v3h-3zM16 5h3v3h-3zM7.75 10.5h3v3h-3zM13.25 10.5h3v3h-3zM5 16h3v3H5zM16 16h3v3h-3z" fill="currentColor"/>',
    yi: '<path d="M12 4 5 20h14L12 4Z" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"/>',
    bytedance: '<path d="M12 3.5 20 8v8l-8 4.5L4 16V8l8-4.5Z" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M9 12.5 11 14.5 15.5 10" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>',
    baidu: '<path d="M7 10c1.2 0 2-1 2-2.2S8.2 5.4 7 5.4s-2 1-2 2.4S5.8 10 7 10Zm10 0c1.2 0 2-1 2-2.2s-.8-2.4-2-2.4-2 1-2 2.4S15.8 10 17 10ZM12 12.4c-3 0-5.5 2-5.5 4.4 0 1.9 2.5 2.7 5.5 2.7s5.5-.8 5.5-2.7c0-2.4-2.5-4.4-5.5-4.4Z" fill="currentColor"/>',
    tencent: '<path d="M6 6a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v8a4 4 0 0 1-4 4H10a4 4 0 0 1-4-4V6Z" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M12 6.5v11" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>',
    cohere: '<path d="M5 8.5h6M5 12h4.5M5 15.5h6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/><path d="M14.5 7.5 19 12l-4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>',
    oss: '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M8.5 12.5 11 15l4.5-6" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>',
    jev: '<path d="M8 6h8M12 6v9a3 3 0 0 1-3 3H8" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/>',
    generic: '<path d="M12 3.5 20 8v8l-8 4.5L4 16V8l8-4.5Z" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M12 3.5V20.5M4 8l8 4.5L20 8" fill="none" stroke="currentColor" stroke-width="1.2" opacity=".6"/>',
  };

  // 官方品牌矢量图标(simple-icons / iconify logos 集), 单色时用 currentColor 跟随主题
  const BRAND_SVG = {
    openai: "/static/model-icons/openai.svg",
    anthropic: "/static/model-icons/anthropic.svg",
    google: "/static/model-icons/google.svg",
    gemini: "/static/model-icons/gemini.svg",
    meta: "/static/model-icons/meta.svg",
    mistral: "/static/model-icons/mistral.svg",
    deepseek: "/static/model-icons/deepseek.svg",
    qwen: "/static/model-icons/qwen.svg",
    minimax: "/static/model-icons/minimax.svg",
    moonshot: "/static/model-icons/moonshot.svg",
    baidu: "/static/model-icons/baidu.svg",
    bytedance: "/static/model-icons/bytedance.svg",
    zhipu: "/static/model-icons/zhipuai.svg",
    huggingface: "/static/model-icons/huggingface.svg",
  };

  function modelIcon(name, size = 30) {
    const brand = modelBrand(name);
    const svg = BRAND_SVG[brand.key];
    if (svg) {
      return `<span class="model-logo brand-svg" style="--brand:${brand.color}" title="${esc(brand.label)}">
        <img src="${svg}" width="${size}" height="${size}" alt="${esc(brand.label)}" loading="lazy">
      </span>`;
    }
    const glyph = BRAND_GLYPH[brand.key] || BRAND_GLYPH.generic;
    return `<span class="model-logo" style="--brand:${brand.color}" title="${esc(brand.label)}">
      <svg width="${size}" height="${size}" viewBox="0 0 24 24" aria-hidden="true">${glyph}</svg>
    </span>`;
  }

  const TIER_LABELS = { lite: "Lite", essential: "Essential", standard: "Standard", pro: "Pro", max: "Max" };

  function loadModelSquare() {
    return Promise.all([api("/api/me"), api("/api/model-square")]).then(([me, square]) => ({ me, square }));
  }

  function renderModelSquare(data) {
    const { me, square } = data;
    const models = square.models || [];
    const current = me.user.plan;
    return `
      <div class="page-head">
        <div>
          <div class="faint" style="font-size:11px;letter-spacing:.12em;text-transform:uppercase">MODEL SQUARE</div>
          <h1 class="page-title">模型广场</h1>
          <p class="page-sub">共 ${square.total} 个模型，当前 ${esc(me.plan.name)} 档位可用 ${square.available} 个。徽章表示该模型开放的档位。</p>
        </div>
        <div class="head-actions">
          <span class="badge badge-${esc(current)}">${icon("crown", 11)}当前 ${esc(me.plan.name)}</span>
          <a class="btn btn-soft btn-sm" href="#/keys">${icon("key", 13)}管理 Keys</a>
        </div>
      </div>
      <div class="model-square-toolbar card card-flat">
        <label class="model-search">
          ${icon("search", 15)}
          <input id="model-square-search" class="input" type="search" placeholder="搜索模型 ID，如 gpt / claude / deepseek" autocomplete="off">
        </label>
        <div class="model-filters" id="model-square-filters">
          <button type="button" class="chip active" data-filter="all">全部 <b>${square.total}</b></button>
          <button type="button" class="chip" data-filter="mine">我可用 <b>${square.available}</b></button>
          ${["lite", "essential", "standard", "pro", "max"].map((t) => `<button type="button" class="chip" data-filter="${t}">${TIER_LABELS[t]}</button>`).join("")}
        </div>
      </div>
      <div class="model-grid" id="model-square-grid">
        ${models.map((m) => modelCard(m, current)).join("")}
      </div>
      <div class="model-empty" id="model-square-empty" hidden>${icon("search", 20)}<span>没有匹配的模型</span></div>`;
  }

  function modelCard(m, current) {
    const tiers = m.tiers || [];
    const mine = tiers.includes(current);
    return `
      <article class="model-card ${mine ? "" : "locked"}" data-model="${esc(m.name)}" data-tiers="${tiers.join(",")}" data-mine="${mine ? "1" : "0"}">
        <div class="model-card-top">
          ${modelIcon(m.name, 30)}
          <div class="model-card-meta">
            <code class="model-id" title="${esc(m.name)}">${esc(m.name)}</code>
            <span class="model-channel">${m.channel_count} 个渠道 · ${money(m.input_micro)} / ${money(m.output_micro)} 每 1K</span>
          </div>
          <button type="button" class="icon-btn model-copy" data-action="copy-secret" data-copy="${esc(m.name)}" title="复制模型 ID" aria-label="复制模型 ID">${icon("copy", 15)}</button>
        </div>
        <div class="model-tiers">
          ${tiers.map((t) => `<span class="badge badge-${esc(t)} ${t === current ? "badge-current" : ""}">${TIER_LABELS[t] || t}${t === current ? " ·当前" : ""}</span>`).join("")}
        </div>
      </article>`;
  }

  function bindModelSquare() {
    const grid = document.getElementById("model-square-grid");
    if (!grid) return;
    const search = document.getElementById("model-square-search");
    const filters = document.getElementById("model-square-filters");
    const empty = document.getElementById("model-square-empty");
    const cards = Array.from(grid.querySelectorAll(".model-card"));
    let filter = "all";

    const apply = () => {
      const q = (search ? search.value : "").trim().toLowerCase();
      let shown = 0;
      cards.forEach((card) => {
        const name = card.dataset.model.toLowerCase();
        const tiers = (card.dataset.tiers || "").split(",").filter(Boolean);
        const mine = card.dataset.mine === "1";
        let ok = !q || name.includes(q);
        if (ok && filter === "mine") ok = mine;
        else if (ok && filter !== "all") ok = tiers.includes(filter);
        // 注意: .model-card 是 display:flex, 会覆盖 UA 的 [hidden]{display:none},
        // 因此必须用 style.display 显式控制可见性
        card.style.display = ok ? "" : "none";
        if (ok) shown += 1;
      });
      if (empty) empty.style.display = shown > 0 ? "none" : "";
    };

    search?.addEventListener("input", apply);
    filters?.addEventListener("click", (e) => {
      const btn = e.target.closest(".chip");
      if (!btn) return;
      filter = btn.dataset.filter;
      filters.querySelectorAll(".chip").forEach((c) => c.classList.toggle("active", c === btn));
      apply();
    });
  }

  /* ---------- routes ---------- */

  const ROUTES = {
    "/": { load: loadDashboard, render: renderDashboard },
    "/keys": { load: loadKeys, render: renderKeys },
    "/usage": { load: loadUsage, render: renderUsage },
    "/plans": { load: loadPlans, render: renderPlans },
    "/models": { load: loadModelSquare, render: renderModelSquare, bind: bindModelSquare },
    "/admin": { admin: true, load: loadAdminOverview, render: renderAdminOverview, bind: bindAdminOverview },
    "/admin/users": { admin: true, load: loadAdminUsers, render: renderAdminUsers, bind: bindAdminUsers },
    "/admin/channels": { admin: true, load: loadAdminChannels, render: renderAdminChannels, bind: bindAdminChannels },
    "/admin/applications": { admin: true, load: loadAdminApplications, render: renderAdminApplications, bind: bindAdminApplications },
    "/admin/plans": { admin: true, load: loadAdminPlans, render: renderAdminPlans, bind: bindAdminPlans },
    "/admin/logs": { admin: true, load: loadAdminLogs, render: renderAdminLogs, bind: bindAdminLogs },
    "/admin/ip-bans": { admin: true, load: loadAdminIpBans, render: renderAdminIpBans, bind: bindAdminIpBans },
    "/admin/settings": { admin: true, load: loadAdminSettings, render: renderAdminSettings, bind: bindAdminSettings },
  };

  function currentRoute() {
    const hash = location.hash || "#/";
    let route = hash.replace(/^#/, "");
    if (!route.startsWith("/")) route = "/" + route;
    route = route.split("?")[0];
    if (route.length > 1) route = route.replace(/\/+$/, "");
    return route || "/";
  }

  function navigate(route) {
    location.hash = "#" + (route.startsWith("/") ? route : "/" + route);
  }

  let renderSeq = 0;

  async function render() {
    const seq = ++renderSeq;
    bindAvatarFallbacks();
    if (!state.me) {
      renderLogin();
      return;
    }
    const route = currentRoute();
    const definition = ROUTES[route];
    if (!definition) {
      navigate("/");
      return;
    }
    if (definition.admin && !isAdmin()) {
      toast("需要站长权限", "error");
      navigate("/");
      return;
    }
    app.innerHTML = appShell(loadingContent());
    bindAvatarFallbacks();
    try {
      const data = await definition.load();
      if (seq !== renderSeq) return;
      app.innerHTML = appShell(definition.render(data));
      bindAvatarFallbacks();
      if (definition.bind) definition.bind(data);
      enhanceMotion();
    } catch (error) {
      if (seq !== renderSeq) return;
      if (error.status === 401) {
        state.me = null;
        renderLogin();
        return;
      }
      app.innerHTML = appShell(errorContent(error));
      bindAvatarFallbacks();
    }
  }

  async function loadDashboard() {
    const me = await api("/api/me");
    state.me = me;
    try {
      const data = await api("/api/notices");
      me.notices = data.notices || [];
      me.unread_notices = data.unread_count || 0;
    } catch {
      me.notices = [];
      me.unread_notices = 0;
    }
    return me;
  }

  async function loadKeys() {
    const [me, keys, models] = await Promise.all([
      api("/api/me"),
      api("/api/keys"),
      api("/api/models"),
    ]);
    state.me = me;
    return { me, keys, models };
  }

  async function loadUsage() {
    const [me, logs] = await Promise.all([
      api("/api/me"),
      api(`/api/usage?page=${ui.usagePage}`),
    ]);
    state.me = me;
    return { me, logs };
  }

  async function loadPlans() {
    const [me, plans] = await Promise.all([api("/api/me"), api("/api/plans")]);
    state.me = me;
    return { me, plans: plans.plans };
  }

  /* ---------- admin shared ---------- */

  function adminTabs(active) {
    const tabs = [
      ["/admin", "概览", "grid"],
      ["/admin/users", "用户", "users"],
      ["/admin/channels", "渠道", "server"],
      ["/admin/applications", "审批", "crown"],
      ["/admin/plans", "套餐", "layers"],
      ["/admin/logs", "日志", "chart"],
      ["/admin/ip-bans", "IP 封禁", "shield"],
      ["/admin/settings", "设置", "settings"],
    ];
    return `<div class="admin-tabs">${tabs
      .map(
        ([href, label, iconName]) =>
          `<a class="admin-tab ${active === href ? "active" : ""}" href="#${href}">${icon(iconName, 14)}${esc(label)}</a>`
      )
      .join("")}</div>`;
  }

  function adminHeader(title, subtitle, actions = "") {
    return `<div class="page-head">
      <div><h1 class="page-title">${esc(title)}</h1><p class="page-sub">${esc(subtitle)}</p></div>
      ${actions ? `<div class="head-actions">${actions}</div>` : ""}
    </div>`;
  }

  function statCard(label, value, foot = "") {
    return `<div class="stat"><div class="stat-label">${esc(label)}</div><div class="stat-value">${value}</div>${foot ? `<div class="stat-foot">${esc(foot)}</div>` : ""}</div>`;
  }

  async function loadAdminOverview() {
    const [me, overview] = await Promise.all([api("/api/me"), api("/api/admin/overview")]);
    state.me = me;
    return { me, overview };
  }

  function renderAdminOverview(data) {
    const overview = data.overview;
    const stats = overview.stats;
    const maxCost = Math.max(...(overview.series || []).map((point) => Number(point.cost_micro) || 0), 1);
    return `
      ${adminTabs("/admin")}
      ${adminHeader("站长概览", "全局用户、渠道、额度与请求状态", `<a class="btn btn-soft btn-sm" href="#/admin/channels">${icon("server", 13)}管理渠道</a>`)}
      <div class="stat-grid stat-grid-4" style="margin-bottom:16px">
        ${statCard("用户总数", Number(stats.users).toLocaleString(), `${stats.active_users} 位 7 天内活跃`)}
        ${statCard("待审批", String(stats.pending_apps), "Pro / Max 候补")}
        ${statCard("今日请求", Number(stats.requests_today).toLocaleString(), `$${(Number(stats.cost_today_micro) / 1e6).toFixed(2)} 消耗`)}
        ${statCard("渠道健康", `${stats.healthy_channels} / ${stats.channels}`, `${stats.active_keys} 个有效 Key`)}
      </div>
      <div class="overview-grid">
        <div class="stack">
          <div class="card">
            <div class="card-head"><div><div class="card-title">最近 7 天全局消耗</div><div class="card-sub">按自然日聚合，单位美元</div></div><span class="badge badge-muted">$${(Number(stats.cost_total_micro) / 1e6).toFixed(2)} 累计</span></div>
            <div class="chart">${(overview.series || [])
              .map((point) => {
                const height = Math.max(3, (Number(point.cost_micro) / maxCost) * 100);
                return `<div class="chart-col" title="${esc(point.day)} · ${point.requests} 次 · ${moneyExact(point.cost_micro)}"><div class="chart-bar-wrap"><div class="chart-bar" style="height:${height.toFixed(1)}%"></div></div><div class="chart-label">${esc(point.day)}</div></div>`;
              })
              .join("")}</div>
          </div>
          <div class="card">
            <div class="card-head"><div><div class="card-title">最近请求</div><div class="card-sub">含成功与拦截记录</div></div><a class="card-link" href="#/admin/logs">全部日志 ${icon("arrowRight", 12)}</a></div>
            ${usageTable(overview.recent, "暂无请求", true)}
          </div>
        </div>
        <div class="stack">
          <div class="card">
            <div class="card-head"><div><div class="card-title">套餐分布</div><div class="card-sub">当前生效档位</div></div></div>
            <div class="mini-list">${(overview.plan_distribution || [])
              .map(
                (row) => `<div class="mini-row"><div class="mini-row-main">${planBadge(row.plan)}</div><span class="mini-row-side">${row.count} 人</span></div>`
              )
              .join("")}</div>
          </div>
          <div class="card">
            <div class="card-head"><div><div class="card-title">渠道健康</div><div class="card-sub">最近一次巡检结果</div></div></div>
            <div class="mini-list">${(overview.channel_health || [])
              .map(
                (row) => `<div class="mini-row"><div class="mini-row-main">${statusDot(row.health)}<strong>${row.health === "healthy" ? "健康" : row.health === "error" ? "异常" : "未知"}</strong></div><span class="mini-row-side">${row.count} 个</span></div>`
              )
              .join("")}</div>
            <div class="divider"></div>
            <div class="mini-row"><span class="muted">总请求</span><span>${Number(stats.requests_total).toLocaleString()}</span></div>
            <div class="mini-row"><span class="muted">总消耗</span><span>$${(Number(stats.cost_total_micro) / 1e6).toFixed(2)}</span></div>
          </div>
          <div class="card">
            <div class="card-head"><div><div class="card-title">卡片发放</div><div class="card-sub">补签卡 / Plus 卡全员发放 · 重置卡发放与兑换码生成</div></div></div>
            <button type="button" class="btn btn-mint btn-block" data-action="issue-makeup-all">全员发放补签卡</button>
            <div class="checkin-note">每人每种卡仅发一张；已有可用补签卡、封禁用户自动跳过。</div>
            <div class="divider"></div>
            <div class="field"><label>Plus 补签卡：一次补齐 7 天内所有漏签</label></div>
            <button type="button" class="btn btn-mint btn-block" data-action="issue-makeup-plus-all">全员发放 Plus 补签卡</button>
            <div class="checkin-note">每人每种卡仅发一张；已有可用 Plus 卡、封禁用户自动跳过，持有普通卡也能领。</div>
            <div class="divider"></div>
            <button type="button" class="btn btn-mint btn-block" data-action="issue-reset-all">全员发放重置卡</button>
            <div class="checkin-note">直接发到每人账户，无需兑换码；长期有效，已有未使用重置卡、封禁用户自动跳过。</div>
            <div class="divider"></div>
            <div class="field"><label>生成重置卡数量（1–100）</label><input class="input" id="reset-code-count" type="number" min="1" max="100" value="10"></div>
            <div class="field"><label>有效期（天，留空为长期有效）</label><input class="input" id="reset-code-days" type="number" min="1" placeholder="例如 30"></div>
            <button type="button" class="btn btn-soft btn-block" data-action="create-reset-codes">生成重置卡</button>
            <div id="reset-codes-result" style="margin-top:10px"></div>
          </div>
        </div>
      </div>`;
  }

  function bindAdminOverview() {}

  /* ---------- admin users ---------- */

  async function loadAdminUsers() {
    const [me, data, plansData] = await Promise.all([
      api("/api/me"),
      api(`/api/admin/users?q=${encodeURIComponent(ui.userQuery)}&page=${ui.userPage}`),
      api("/api/admin/plans"),
    ]);
    state.me = me;
    ui.adminPlans = plansData.plans || [];
    return { me, data };
  }

  function renderAdminUsers({ data }) {
    const users = data.users || [];
    const totalPages = Math.max(1, data.pages || 1);
    return `
      ${adminTabs("/admin/users")}
      ${adminHeader("用户管理", "封禁 / 解封、手动授予或撤销套餐、查看用户用量")}
      <div class="card">
        <form class="filters" id="user-search-form">
          <div class="search-input">${icon("search", 14)}<input class="input" name="q" value="${esc(ui.userQuery)}" placeholder="搜索用户名或昵称…"></div>
          <button class="btn btn-soft btn-sm" type="submit">${icon("search", 13)}搜索</button>
          <span class="faint" style="font-size:11px">共 ${Number(data.total || 0)} 位用户</span>
        </form>
        ${
          users.length
            ? `<div class="table-wrap"><table>
                <thead><tr><th>用户</th><th>档位</th><th>连续签到</th><th class="num">Keys</th><th class="num">30D 消耗</th><th>状态</th><th class="right">操作</th></tr></thead>
                <tbody>${users
                  .map(
                    (user) => `<tr>
                      <td><div class="cell-user">${avatar(user, "avatar-sm")}<div><div class="cell-strong">${esc(user.display_name || user.username)}</div><div class="cell-sub">@${esc(user.username)} · #${user.id} · L${user.trust_level}${user.role === "root" ? " · 站长" : user.role === "admin" ? " · 管理员" : ""}</div></div></div></td>
                      <td>${planBadge(user.plan)}${user.earned_plan !== user.plan ? `<div class="cell-sub">基础 ${esc(user.earned_plan)}</div>` : ""}</td>
                      <td>${user.streak} 天<div class="cell-sub">最长 ${user.longest_streak} 天</div></td>
                      <td class="num">${user.key_count}</td>
                      <td class="num">$${(Number(user.cost_30d_micro) / 1e6).toFixed(4)}</td>
                      <td>${user.is_banned ? `<span class="badge badge-danger">${statusDot("banned")}已封禁</span>` : `<span class="badge badge-success">${statusDot("active")}正常</span>`}${user.pending_apps ? `<div class="cell-sub">${user.pending_apps} 个待审批</div>` : ""}</td>
                      <td><div class="cell-actions">${renderUserActions(user)}</div></td>
                    </tr>`
                  )
                  .join("")}</tbody>
              </table></div>
              <div class="pagination"><span>第 ${data.page} / ${totalPages} 页</span><div class="pager">
                <button type="button" class="btn btn-soft btn-sm" data-action="user-page" data-page="${data.page - 1}" ${data.page <= 1 ? "disabled" : ""}>上一页</button>
                <button type="button" class="btn btn-soft btn-sm" data-action="user-page" data-page="${data.page + 1}" ${data.page >= totalPages ? "disabled" : ""}>下一页</button>
              </div></div>`
            : `<div class="empty">没有找到用户</div>`
        }
      </div>`;
  }

  function renderUserActions(user) {
    if (user.role === "root") {
      return `<span class="faint" style="font-size:11px">站长账号</span>`;
    }
    const buttons = [];
    buttons.push(
      `<button type="button" class="btn btn-sm ${user.is_banned ? "btn-soft" : "btn-danger"}" data-action="ban-user" data-id="${user.id}" data-banned="${user.is_banned ? "1" : "0"}">${user.is_banned ? "解封" : "封禁"}</button>`
    );
    const isRootViewer = state.me && state.me.user && state.me.user.role === "root";
    if (user.role === "admin") {
      buttons.push(`<span class="badge badge-admin">管理员</span>`);
      if (isRootViewer) {
        buttons.push(`<button type="button" class="btn btn-ghost btn-sm" data-action="demote-user" data-id="${user.id}">取消管理员</button>`);
      }
    } else if (isRootViewer) {
      buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="promote-user" data-id="${user.id}">设为管理员</button>`);
    }
    buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="send-notice" data-id="${user.id}" data-username="${esc(user.display_name || user.username)}">发通知</button>`);
    buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="issue-makeup-one" data-id="${user.id}">发补签卡</button>`);
    buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="issue-makeup-one-plus" data-id="${user.id}">发 Plus 卡</button>`);
    buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="issue-reset-one" data-id="${user.id}">发重置卡</button>`);
    // 自定义套餐不在 PLAN_ORDER 里，视为最高档：不显示快捷授予按钮，
    // 改走“授予套餐”弹窗（下拉含全部套餐，后端校验只能授予更高档位）。
    const order = PLAN_ORDER[user.plan] ?? 99;
    buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="grant-plan-modal" data-id="${user.id}">授予套餐</button>`);
    if (order < 5) {
      buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="grant-plan" data-id="${user.id}" data-plan="pro">授予 Pro</button>`);
    }
    if (order < 6) {
      buttons.push(`<button type="button" class="btn btn-soft btn-sm" data-action="grant-plan" data-id="${user.id}" data-plan="max">授予 Max</button>`);
    }
    if (order >= 5) {
      buttons.push(`<button type="button" class="btn btn-ghost btn-sm" data-action="revoke-plan" data-id="${user.id}">撤销</button>`);
    }
    return buttons.join("");
  }

  function bindAdminUsers() {
    const form = document.querySelector("#user-search-form");
    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      ui.userQuery = new FormData(form).get("q")?.toString() || "";
      ui.userPage = 1;
      render();
    });
  }

  /* ---------- admin plans ---------- */

  async function loadAdminPlans() {
    const [me, data] = await Promise.all([api("/api/me"), api("/api/admin/plans")]);
    state.me = me;
    ui.adminPlans = data.plans || [];
    return { me, plans: ui.adminPlans };
  }

  function planLimitsText(plan) {
    return `5H ${money(plan.limits.h5)} · 7D ${money(plan.limits.weekly)} · 30D ${money(plan.limits.monthly)}`;
  }

  function renderAdminPlans({ plans }) {
    return `
      ${adminTabs("/admin/plans")}
      ${adminHeader("套餐管理", "添加自定义套餐、改描述、删除套餐；内置档仅可改描述")}
      <div class="card">
        <div class="card-head"><div><div class="card-title">添加自定义套餐</div><div class="card-sub">自定义套餐只能由管理员手动授予，不参与签到自动升级；额度为空 = 不限</div></div></div>
        <form id="plan-add-form" class="filters" style="margin-bottom:4px">
          <input class="input" name="code" placeholder="code，如 vip2026" required style="width:130px">
          <input class="input" name="name" placeholder="名称，如 VIP" required style="width:120px">
          <input class="input" name="tagline" placeholder="描述（显示给用户看）" required style="min-width:200px">
          <input class="input" name="h5_usd" type="number" step="0.01" min="0" placeholder="5H $（空=不限）" style="width:120px">
          <input class="input" name="weekly_usd" type="number" step="0.01" min="0" placeholder="7D $（空=不限）" style="width:120px">
          <input class="input" name="monthly_usd" type="number" step="0.01" min="0" placeholder="30D $（空=不限）" style="width:130px">
          <input class="input" name="sort_order" type="number" step="1" placeholder="排序（默认 0）" style="width:110px">
          <label class="switch"><input type="checkbox" name="requires_approval"><span class="switch-track"></span><span class="switch-text">需审批</span></label>
          <button class="btn btn-soft btn-sm" type="submit">${icon("plus", 13)}添加套餐</button>
        </form>
        <p class="field-hint">code 只能是小写字母开头的小写字母 / 数字 / 下划线（2–32 字符），不可与内置档重名。新建套餐默认没有任何渠道授权——记得去“渠道”页把档位勾上，否则用户看不到任何模型。</p>
      </div>
      <div class="card mt-24">
        <div class="card-head"><div><div class="card-title">全部套餐</div><div class="card-sub">内置档在前，自定义套餐按排序值排在后面</div></div><span class="badge badge-muted">${plans.length} 个</span></div>
        <div class="mini-list">${plans
          .map(
            (plan) => `<div class="mini-row"><div class="mini-row-main">${planBadge(plan.code, plan.name)}<strong>${esc(plan.name)}</strong><span>${esc(plan.tagline)}${plan.requires_approval ? " · 需站长审批" : ""}</span><span class="faint" style="font-size:11px">${planLimitsText(plan)}${plan.checkin_days ? ` · 连续签到 ${plan.checkin_days} 天` : ""}</span></div>
          <span class="mini-row-side" style="display:inline-flex;align-items:center;gap:8px"><span class="badge ${plan.builtin ? "badge-muted" : "badge-success"}">${plan.builtin ? "内置" : "自定义"}</span><button type="button" class="btn btn-ghost btn-sm" data-action="edit-plan" data-code="${esc(plan.code)}" data-builtin="${plan.builtin ? "1" : "0"}">改描述</button>${plan.builtin ? "" : `<button type="button" class="btn btn-ghost btn-sm" data-action="delete-plan" data-code="${esc(plan.code)}" data-name="${esc(plan.name)}">${icon("trash", 13)}</button>`}</span></div>`
          )
          .join("")}</div>
        <p class="field-hint">删除保护：内置套餐不可删除；有人持有、被渠道或模型档位授权引用、或有待审批申请的自定义套餐也删不掉（先改派用户、取消授权、处理申请）。</p>
      </div>`;
  }

  function bindAdminPlans() {
    const form = document.querySelector("#plan-add-form");
    form?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(form);
      const usd = (key) => {
        const value = String(data.get(key) || "").trim();
        return value === "" ? null : Number(value);
      };
      try {
        await api("/api/admin/plans", {
          method: "POST",
          body: {
            code: String(data.get("code") || "").trim(),
            name: String(data.get("name") || "").trim(),
            tagline: String(data.get("tagline") || "").trim(),
            h5_usd: usd("h5_usd"),
            weekly_usd: usd("weekly_usd"),
            monthly_usd: usd("monthly_usd"),
            sort_order: Math.round(Number(data.get("sort_order") || 0)),
            requires_approval: data.get("requires_approval") === "on",
          },
        });
        toast("套餐已添加", "success");
        render();
      } catch (error) {
        toast(error.message, "error");
      }
    });
  }

  /* ---------- admin channels ---------- */

  async function loadAdminChannels() {
    const [me, data] = await Promise.all([api("/api/me"), api("/api/admin/channels")]);
    state.me = me;
    ui.channels = data.channels || [];
    return { me, channels: ui.channels };
  }

  function renderAdminChannels({ channels }) {
    const selected = ui.selectedChannels;
    return `
      ${adminTabs("/admin/channels")}
      ${adminHeader(
        "渠道与档位授权",
        "不勾选任何档位 = 全部档位可调用；模型级授权优先于渠道级授权",
        `<button type="button" class="btn btn-soft btn-sm" data-action="test-all-channels">${icon("zap", 13)}测试全部渠道</button>
         <button type="button" class="btn btn-soft btn-sm" data-action="bulk-tiers">${icon("layers", 13)}批量授权${selected.size ? ` (${selected.size})` : ""}</button>
         <button type="button" class="btn btn-primary btn-sm" data-action="open-channel-modal">${icon("plus", 13)}添加渠道</button>`
      )}
      <div class="notice" style="margin-bottom:16px">${icon("info", 16)}<div><strong>档位授权规则：</strong>渠道未勾选档位 → 全部档位可调用；渠道勾选了档位 → 仅这些档位；模型单独勾选档位时，以模型授权为准。</div></div>
      <div class="card">
        ${
          channels.length
            ? `<div class="table-wrap"><table>
                <thead><tr><th style="width:28px"><input type="checkbox" id="channel-select-all" ${selected.size === channels.length && channels.length ? "checked" : ""}></th><th>渠道</th><th>健康 / 最近检测</th><th>档位授权</th><th class="num">模型</th><th>状态</th><th class="right">操作</th></tr></thead>
                <tbody>${channels
                  .map(
                    (channel) => `<tr>
                      <td><input type="checkbox" data-channel-select="${channel.id}" ${selected.has(channel.id) ? "checked" : ""}></td>
                      <td><div class="channel-title"><div><div class="cell-strong">${esc(channel.name)}</div><div class="channel-url">${esc(channel.base_url)}</div></div></div><div class="cell-sub">Key ${esc(channel.key_hint || "—")} · 优先级 ${channel.priority} · 权重 ${channel.weight}</div>${channelFeatureBadges(channel)}</td>
                      <td><span class="badge ${channel.health === "healthy" ? "badge-success" : channel.health === "error" ? "badge-danger" : "badge-muted"}">${statusDot(channel.health)}${channel.health === "healthy" ? "健康" : channel.health === "error" ? "异常" : "未知"}</span>${channel.last_health_at ? `<div class="cell-sub">${fmtShortDate(channel.last_health_at)}</div>` : ""}<div style="margin-top:6px">${channelLastTestCell(channel)}</div></td>
                      <td><div class="tier-tags">${tierTags(channel.tiers)}</div>${channel.tiers_all ? "" : `<div class="cell-sub">渠道级授权</div>`}</td>
                      <td class="num">${channel.model_count}</td>
                      <td><button type="button" class="switch ${channel.enabled ? "on" : ""}" data-action="toggle-channel" data-id="${channel.id}" data-enabled="${channel.enabled ? "1" : "0"}" title="切换启用状态"><span class="switch-track sm"></span><span class="switch-text">${channel.enabled ? "启用" : "停用"}</span></button></td>
                      <td><div class="cell-actions">
                        <button type="button" class="btn btn-ghost btn-sm" data-action="open-test-modal" data-id="${channel.id}" title="模型检测" aria-label="模型检测">${icon("zap", 13)}测试</button>
                        <button type="button" class="btn btn-ghost btn-sm" data-action="open-models" data-id="${channel.id}" title="管理模型" aria-label="管理模型">模型</button>
                        <button type="button" class="btn btn-ghost btn-sm" data-action="open-channel-modal" data-id="${channel.id}" title="编辑渠道" aria-label="编辑渠道">${icon("edit", 13)}</button>
                        <button type="button" class="btn btn-ghost btn-sm" data-action="delete-channel" data-id="${channel.id}" title="删除渠道" aria-label="删除渠道">${icon("trash", 13)}</button>
                      </div></td>
                    </tr>`
                  )
                  .join("")}</tbody>
              </table></div>`
            : `<div class="empty"><div class="empty-icon">${icon("server", 20)}</div>还没有渠道<br><button type="button" class="btn btn-primary btn-sm mt-16" data-action="open-channel-modal">${icon("plus", 13)}添加第一个渠道</button></div>`
        }
      </div>`;
  }

  function tierPicker(name, selected = []) {
    return `<div class="tier-picker">${TIERS.map(
      (tier) => `<label class="tier-option"><input type="checkbox" name="${name}" value="${tier}" ${selected.includes(tier) ? "checked" : ""}><span>${tier}</span></label>`
    ).join("")}</div>`;
  }

  function openChannelModal(channelId) {
    const channel = channelId ? (ui.channels || []).find((item) => item.id === Number(channelId)) : null;
    const isEdit = !!channel;
    openModal({
      title: isEdit ? "编辑渠道" : "添加渠道",
      subtitle: isEdit ? "留空 API Key 表示不修改" : "Base URL 建议包含 /v1，例如 https://api.openai.com/v1",
      wide: true,
      body: `
        <form id="channel-form">
          <div class="input-row">
            <div class="field"><label>渠道名称</label><input class="input" name="name" value="${esc(channel?.name || "")}" placeholder="例如 OpenAI 主渠道" required></div>
            <div class="field"><label>Base URL</label><input class="input" name="base_url" value="${esc(channel?.base_url || "")}" placeholder="https://api.openai.com/v1" required></div>
          </div>
          <div class="field"><label>API Key</label><input class="input" name="api_key" type="password" value="" placeholder="${isEdit ? "留空不修改（当前 " + esc(channel.key_hint) + "）" : "sk-..."}" ${isEdit ? "" : "required"} autocomplete="new-password"><div class="field-hint">密钥使用 AES-256-GCM 加密存储，后台只展示尾号。</div></div>
          <div class="input-row-3 input-row">
            <div class="field"><label>优先级</label><input class="input" name="priority" type="number" value="${channel?.priority ?? 0}" step="1"><div class="field-hint">数值越大越优先</div></div>
            <div class="field"><label>权重</label><input class="input" name="weight" type="number" value="${channel?.weight ?? 1}" min="1" max="100"><div class="field-hint">同优先级加权随机</div></div>
            <div class="field"><label>状态</label><label class="switch" style="height:42px"><input type="checkbox" name="enabled" ${channel?.enabled !== false ? "checked" : ""}><span class="switch-track"></span><span class="switch-text">启用</span></label></div>
          </div>
          <div class="field">
            <label>渠道级档位授权</label>
            ${tierPicker("channel-tier", channel?.tiers || [])}
            <div class="field-hint">不勾选任何档位 = 全部档位可调用（含未来新增档位）。模型级授权优先。</div>
          </div>
          <div class="divider"></div>
          <div class="field">
            <label>额外请求头（JSON 对象）</label>
            <textarea class="input mono" name="extra_headers" rows="3" placeholder='{"originator":"codex_cli_rs"}' style="font-size:12px;min-height:72px">${esc(jsonFieldText(channel?.extra_headers))}</textarea>
            <div class="field-hint">转发上游时附加；值必须是字符串。不允许覆盖 Authorization、Host、Content-Length 等头。</div>
          </div>
          <div class="field">
            <label>请求体覆盖（JSON 对象，可选）</label>
            <textarea class="input mono" name="param_override" rows="3" placeholder='{"temperature":0.7,"reasoning_effort":null}' style="font-size:12px;min-height:72px">${esc(jsonFieldText(channel?.param_override))}</textarea>
            <div class="field-hint">顶层字段覆盖请求体同名字段，值为 null 表示删除；model / stream 不可覆盖。</div>
          </div>
          <div class="input-row">
            <div class="field"><label>参数兼容</label><label class="switch" style="height:42px"><input type="checkbox" name="strip_reasoning_with_tools" ${channel?.strip_reasoning_with_tools ? "checked" : ""}><span class="switch-track"></span><span class="switch-text">有 tools 时去掉推理参数</span></label><div class="field-hint">去掉 chat 的 reasoning_effort 与 responses 的 reasoning（上游不支持 tools + 推理参数时开启）。</div></div>
            <div class="field"><label>默认测试模型</label><input class="input" name="test_model" value="${esc(channel?.test_model || "")}" placeholder="留空 = 第一个启用的模型"><div class="field-hint">「测试全部渠道」与自动检测使用。</div></div>
          </div>
          <div class="field"><label>检测接口</label><select class="input" name="test_endpoint"><option value="chat" ${(channel?.test_endpoint || "chat") === "chat" ? "selected" : ""}>/chat/completions（默认）</option><option value="responses" ${channel?.test_endpoint === "responses" ? "selected" : ""}>/responses（上游只支持 Responses 格式时选）</option></select><div class="field-hint">只影响模型检测发出的请求格式，不影响正常转发。</div></div>
          <div class="notice">${icon("info", 16)}<div>保存后可在「模型」中同步上游模型列表，并为单个模型配置价格与档位授权。</div></div>
        </form>`,
      footer: `
        <button type="button" class="btn btn-ghost" data-action="close-modal">取消</button>
        <button type="submit" class="btn btn-primary" form="channel-form">${icon("check", 14)}${isEdit ? "保存修改" : "添加渠道"}</button>`,
      onMount(root) {
        root.querySelector("#channel-form").addEventListener("submit", async (event) => {
          event.preventDefault();
          const form = event.currentTarget;
          const data = new FormData(form);
          const tiers = data.getAll("channel-tier").map(String);
          const payload = {
            name: String(data.get("name") || "").trim(),
            base_url: String(data.get("base_url") || "").trim(),
            priority: Number(data.get("priority") || 0),
            weight: Number(data.get("weight") || 1),
            enabled: data.get("enabled") === "on",
            tiers,
            strip_reasoning_with_tools: data.get("strip_reasoning_with_tools") === "on",
            test_model: String(data.get("test_model") || "").trim(),
            test_endpoint: String(data.get("test_endpoint") || "chat"),
          };
          for (const [field, label] of [["extra_headers", "额外请求头"], ["param_override", "请求体覆盖"]]) {
            const raw = String(data.get(field) || "").trim();
            if (!raw) { payload[field] = {}; continue; }
            try {
              const parsed = JSON.parse(raw);
              if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error("必须是 JSON 对象");
              payload[field] = parsed;
            } catch (err) {
              toast(`${label}不是合法的 JSON 对象：${err.message}`, "error", 5000);
              return;
            }
          }
          if (data.get("api_key")) payload.api_key = String(data.get("api_key")).trim();
          const button = root.querySelector('button[type="submit"]');
          button.disabled = true;
          try {
            if (isEdit) {
              await api(`/api/admin/channels/${channel.id}`, { method: "PATCH", body: payload });
            } else {
              await api("/api/admin/channels", { method: "POST", body: payload });
            }
            closeModal();
            toast(isEdit ? "渠道已更新" : "渠道已添加", "success");
            render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  async function openModelsModal(channelId) {
    const channel = (ui.channels || []).find((item) => item.id === Number(channelId));
    const root = openModal({
      title: `模型 · ${channel?.name || ""}`,
      subtitle: "模型级档位授权优先于渠道级；不勾选 = 继承渠道级",
      xwide: true,
      body: `<div id="models-body"><div class="skeleton" style="height:220px"></div></div>`,
      footer: `<button type="button" class="btn btn-soft" data-action="close-modal">关闭</button>`,
    });
    await refreshModelsBody(channelId, root);
  }

  async function refreshModelsBody(channelId, root) {
    const body = root.querySelector("#models-body");
    try {
      const data = await api(`/api/admin/channels/${channelId}/models`);
      body.innerHTML = renderModelsBody(channelId, data);
      bindModelsBody(channelId, root);
    } catch (error) {
      body.innerHTML = `<div class="notice danger">${icon("alert", 16)}<div>${esc(error.message)}</div></div>`;
    }
  }

  function renderModelsBody(channelId, data) {
    const models = data.models || [];
    return `
      <div class="flex between flex-wrap" style="margin-bottom:14px;gap:8px">
        <div class="flex" style="gap:8px">
          <button type="button" class="btn btn-soft btn-sm" data-action="sync-models" data-id="${channelId}">${icon("refresh", 13)}从上游同步</button>
          <span class="faint" style="font-size:11px">渠道级档位：${tierTags(data.channel_tiers || [])}</span>
        </div>
        <span class="badge badge-muted">${models.length} 个模型</span>
      </div>
      <form id="model-add-form" class="card card-flat" style="padding:14px;margin-bottom:14px">
        <div class="input-row-3 input-row">
          <div class="field mb-0"><label>模型名</label><input class="input" name="model" placeholder="gpt-4o-mini" required></div>
          <div class="field mb-0"><label>上游模型名（可选）</label><input class="input" name="upstream_model" placeholder="留空 = 同名"></div>
          <div class="field mb-0"><label>操作</label><button type="submit" class="btn btn-primary btn-block">${icon("plus", 13)}添加模型</button></div>
        </div>
        <div class="input-row-3 input-row mt-8">
          <div class="field mb-0"><label>输入价 $/1M（可选）</label><input class="input" name="input_price" type="number" step="0.0001" min="0" placeholder="跟随全局价格"></div>
          <div class="field mb-0"><label>输出价 $/1M（可选）</label><input class="input" name="output_price" type="number" step="0.0001" min="0" placeholder="跟随全局价格"></div>
          <div class="field mb-0"><label>档位（不选继承渠道）</label><div class="tier-picker">${TIERS.map((tier) => `<label class="tier-option"><input type="checkbox" name="model-tier" value="${tier}"><span>${tier}</span></label>`).join("")}</div></div>
        </div>
      </form>
      ${
        models.length
          ? `<div class="table-wrap"><table>
              <thead><tr><th>模型</th><th>上游</th><th class="num">输入 / 输出</th><th>档位</th><th>状态</th><th class="right">操作</th></tr></thead>
              <tbody>${models
                .map(
                  (model) => `<tr>
                    <td><span class="cell-strong mono" style="font-size:11px">${esc(model.model)}</span></td>
                    <td><span class="faint mono" style="font-size:10.5px">${esc(model.upstream_model || "同名")}</span></td>
                    <td class="num"><div class="price-table"><input class="input" data-price-input="${model.id}" data-field="input" type="number" step="0.0001" min="0" value="${model.input_micro === null ? "" : (model.input_micro / 1e6)}" placeholder="全局"><input class="input" data-price-input="${model.id}" data-field="output" type="number" step="0.0001" min="0" value="${model.output_micro === null ? "" : (model.output_micro / 1e6)}" placeholder="全局"></div></td>
                    <td><div class="tier-tags">${tierTags(model.tiers, "继承渠道")}</div></td>
                    <td><button type="button" class="switch ${model.enabled ? "on" : ""}" data-action="toggle-model" data-id="${model.id}" data-enabled="${model.enabled ? "1" : "0"}"><span class="switch-track xs"></span><span class="switch-text">${model.enabled ? "启用" : "停用"}</span></button></td>
                    <td><div class="cell-actions">
                      <button type="button" class="btn btn-ghost btn-sm" data-action="save-model-price" data-id="${model.id}">保存价格</button>
                      <button type="button" class="btn btn-ghost btn-sm" data-action="model-tiers" data-id="${model.id}" data-name="${esc(model.model)}">档位</button>
                      <button type="button" class="btn btn-ghost btn-sm" data-action="delete-model" data-id="${model.id}">${icon("trash", 13)}</button>
                    </div></td>
                  </tr>`
                )
                .join("")}</tbody>
            </table></div>`
          : `<div class="empty">还没有模型，点击「从上游同步」或手动添加</div>`
      }`;
  }

  function bindModelsBody(channelId, root) {
    const form = root.querySelector("#model-add-form");
    form?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(form);
      const toMicro = (value) => {
        if (value === null || String(value).trim() === "") return null;
        return Math.round(Number(value) * 1e6);
      };
      const payload = {
        model: String(data.get("model") || "").trim(),
        upstream_model: String(data.get("upstream_model") || "").trim() || null,
        input_micro: toMicro(data.get("input_price")),
        output_micro: toMicro(data.get("output_price")),
        tiers: data.getAll("model-tier").map(String),
      };
      const button = form.querySelector('button[type="submit"]');
      button.disabled = true;
      try {
        await api(`/api/admin/channels/${channelId}/models`, { method: "POST", body: payload });
        toast("模型已添加", "success");
        await refreshModelsBody(channelId, root);
      } catch (error) {
        button.disabled = false;
        toast(error.message, "error");
      }
    });
  }

  function openModelTiersModal(modelId, modelName, currentTiers, channelId) {
    openModal({
      title: `模型档位 · ${modelName}`,
      subtitle: "勾选允许调用该模型的套餐档位；不勾选 = 继承渠道级授权",
      body: `<div class="field"><label>套餐档位</label>${tierPicker("modal-model-tier", currentTiers || [])}</div>
        <div class="notice">${icon("info", 16)}<div>用户套餐档位与这里的授权取交集。未授权档位调用会返回 403，并写入用量审计。</div></div>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn btn-primary" id="save-model-tiers">保存授权</button>`,
      onMount(root) {
        root.querySelector("#save-model-tiers").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            const tiers = Array.from(root.querySelectorAll('input[name="modal-model-tier"]:checked')).map((item) => item.value);
            await api(`/api/admin/models/${modelId}/tiers`, { method: "PUT", body: { tiers } });
            closeModal();
            toast("模型档位授权已更新", "success");
            await render();
            if (channelId) window.setTimeout(() => openModelsModal(channelId), 220);
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function bindAdminChannels() {
    document.querySelectorAll("[data-channel-select]").forEach((checkbox) => {
      checkbox.addEventListener("change", () => {
        const id = Number(checkbox.dataset.channelSelect);
        if (checkbox.checked) ui.selectedChannels.add(id);
        else ui.selectedChannels.delete(id);
        render();
      });
    });
    const selectAll = document.querySelector("#channel-select-all");
    selectAll?.addEventListener("change", () => {
      if (selectAll.checked) {
        (ui.channels || []).forEach((channel) => ui.selectedChannels.add(channel.id));
      } else {
        ui.selectedChannels.clear();
      }
      render();
    });
  }

  /* ---------- admin applications ---------- */

  async function loadAdminApplications() {
    const [me, data] = await Promise.all([
      api("/api/me"),
      api(`/api/admin/applications?status=${encodeURIComponent(ui.appStatus)}`),
    ]);
    state.me = me;
    return { me, data };
  }

  function renderAdminApplications({ data }) {
    const apps = data.applications || [];
    return `
      ${adminTabs("/admin/applications")}
      ${adminHeader("晋升审批", "Pro / Max 候补池；审批结果决定最终档位")}
      <div class="admin-tabs" style="margin-bottom:16px">
        ${["pending", "approved", "rejected", "all"]
          .map(
            (status) => `<a class="admin-tab ${ui.appStatus === status ? "active" : ""}" href="#" data-action="app-status" data-status="${status}">${status === "pending" ? "待审批" : status === "approved" ? "已通过" : status === "rejected" ? "已驳回" : "全部"}</a>`
          )
          .join("")}
      </div>
      ${
        apps.length
          ? `<div class="app-list">${apps
              .map(
                (app) => `<div class="app-card">
                  <div class="app-card-main">
                    <div class="app-card-title">${avatar({ username: app.username, display_name: app.display_name, avatar_url: app.avatar_url }, "avatar-sm")}${esc(app.display_name || app.username)}${planBadge(app.current_plan)}<span class="faint" style="font-weight:400">→</span>${planBadge(app.target_plan)}</div>
                    <div class="app-card-meta">连续签到 ${app.consecutive_days} 天 · 最长 ${app.longest_streak} 天 · 申请于 ${fmtDate(app.created_at)}${app.review_note ? ` · ${esc(app.review_note)}` : ""}</div>
                  </div>
                  ${app.status === "pending" ? `<div class="cell-actions"><button type="button" class="btn btn-primary btn-sm" data-action="approve-app" data-id="${app.id}" data-name="${esc(app.display_name || app.username)}" data-plan="${esc(app.target_plan)}">通过</button><button type="button" class="btn btn-ghost btn-sm" data-action="reject-app" data-id="${app.id}" data-name="${esc(app.display_name || app.username)}" data-plan="${esc(app.target_plan)}">驳回</button></div>` : `<span class="badge ${app.status === "approved" ? "badge-success" : "badge-muted"}">${app.status === "approved" ? "已通过" : "已驳回"}</span>`}
                </div>`
              )
              .join("")}</div>`
          : `<div class="card"><div class="empty"><div class="empty-icon">${icon("crown", 20)}</div>${ui.appStatus === "pending" ? "暂无待审批的晋升申请" : "暂无记录"}</div></div>`
      }`;
  }

  function bindAdminApplications() {}

  /* ---------- admin logs ---------- */

  async function loadAdminLogs() {
    const query = new URLSearchParams();
    if (ui.logFilters.user_id) query.set("user_id", ui.logFilters.user_id);
    if (ui.logFilters.model) query.set("model", ui.logFilters.model);
    if (ui.logFilters.status) query.set("status", ui.logFilters.status);
    query.set("page", String(ui.logPage));
    const [me, data] = await Promise.all([api("/api/me"), api(`/api/admin/logs?${query}`)]);
    state.me = me;
    return { me, data };
  }

  function renderAdminLogs({ data }) {
    const logs = data.logs || [];
    const totalPages = Math.max(1, data.pages || 1);
    return `
      ${adminTabs("/admin/logs")}
      ${adminHeader("全局日志", "所有用户请求、档位拦截、额度拒绝与上游错误")}
      <div class="card">
        <form class="filters" id="log-filter-form">
          <input class="input" name="user_id" type="number" placeholder="用户 ID" value="${esc(ui.logFilters.user_id)}">
          <input class="input" name="model" placeholder="模型名" value="${esc(ui.logFilters.model)}">
          <select class="select" name="status">
            <option value="">全部状态</option>
            <option value="200" ${ui.logFilters.status === "200" ? "selected" : ""}>200 成功</option>
            <option value="403" ${ui.logFilters.status === "403" ? "selected" : ""}>403 无权限</option>
            <option value="429" ${ui.logFilters.status === "429" ? "selected" : ""}>429 超额</option>
            <option value="500" ${ui.logFilters.status === "500" ? "selected" : ""}>500 错误</option>
          </select>
          <button class="btn btn-soft btn-sm" type="submit">${icon("filter", 13)}筛选</button>
          <span class="faint" style="font-size:11px">共 ${Number(data.total || 0)} 条</span>
        </form>
        ${
          logs.length
            ? `<div class="table-wrap"><table>
                <thead><tr><th>用户</th><th>模型</th><th>渠道</th><th class="num">Token</th><th class="num">费用</th><th>状态</th><th>时间</th></tr></thead>
                <tbody>${logs
                  .map(
                    (log) => `<tr>
                      <td><span class="cell-strong">${esc(log.user_name || "—")}</span><div class="cell-sub">${esc(log.key_prefix || "")}</div></td>
                      <td><span class="mono" style="font-size:11px">${esc(log.model || "—")}</span>${log.error_code ? `<div class="cell-sub text-danger">${esc(log.error_code)}</div>` : ""}</td>
                      <td><span class="muted">${esc(log.channel_name || "—")}</span></td>
                      <td class="num">${Number(log.total_tokens || 0).toLocaleString()}</td>
                      <td class="num">${moneyExact(log.cost_micro)}</td>
                      <td><span class="badge ${log.status < 400 ? "badge-success" : "badge-danger"}">${log.status}</span></td>
                      <td><span class="faint">${fmtShortDate(log.created_at)}</span></td>
                    </tr>`
                  )
                  .join("")}</tbody>
              </table></div>
              <div class="pagination"><span>第 ${data.page} / ${totalPages} 页</span><div class="pager">
                <button type="button" class="btn btn-soft btn-sm" data-action="log-page" data-page="${data.page - 1}" ${data.page <= 1 ? "disabled" : ""}>上一页</button>
                <button type="button" class="btn btn-soft btn-sm" data-action="log-page" data-page="${data.page + 1}" ${data.page >= totalPages ? "disabled" : ""}>下一页</button>
              </div></div>`
            : `<div class="empty">没有匹配的日志</div>`
        }
      </div>`;
  }

  function bindAdminLogs() {
    const form = document.querySelector("#log-filter-form");
    form?.addEventListener("submit", (event) => {
      event.preventDefault();
      const data = new FormData(form);
      ui.logFilters = {
        user_id: String(data.get("user_id") || "").trim(),
        model: String(data.get("model") || "").trim(),
        status: String(data.get("status") || "").trim(),
      };
      ui.logPage = 1;
      render();
    });
  }

  /* ---------- admin ip bans ---------- */

  async function loadAdminIpBans() {
    const [me, data] = await Promise.all([api("/api/me"), api("/api/admin/ip-bans")]);
    state.me = me;
    return { me, data };
  }

  function renderAdminIpBans({ data }) {
    const bans = data.ip_bans || data.bans || [];
    const perSec = Number(data.per_sec || 0);
    return `
      ${adminTabs("/admin/ip-bans")}
      ${adminHeader("IP 封禁", perSec > 0 ? `单个地址每秒超过 ${perSec} 次请求会被自动封禁，并连带封禁在该地址出现过的普通账号` : "自动封禁已关闭（IP_FLOOD_PER_SEC=0），仍可手动封禁")}
      <div class="card">
        <form class="filters" id="ip-ban-form">
          <input class="input" name="ip" placeholder="IP 地址，如 1.2.3.4" required>
          <input class="input" name="reason" placeholder="原因（可选）" maxlength="200">
          <label class="faint" style="font-size:12px;display:inline-flex;align-items:center;gap:6px"><input type="checkbox" name="ban_users" checked>连带封禁账号</label>
          <button class="btn btn-danger btn-sm" type="submit">${icon("shield", 13)}封禁</button>
          <span class="faint" style="font-size:11px">共 ${bans.length} 个地址</span>
        </form>
        ${
          bans.length
            ? `<div class="table-wrap"><table>
                <thead><tr><th>地址</th><th>原因</th><th class="num">触发次数</th><th class="num">关联账号</th><th>封禁时间</th><th></th></tr></thead>
                <tbody>${bans
                  .map(
                    (ban) => `<tr>
                      <td><span class="mono cell-strong">${esc(ban.ip)}</span></td>
                      <td><span class="muted">${esc(ban.reason || "—")}</span></td>
                      <td class="num">${Number(ban.hits || 0).toLocaleString()}</td>
                      <td class="num">${Number(ban.linked_users || 0)}</td>
                      <td><span class="faint">${fmtShortDate(ban.created_at)}</span></td>
                      <td class="num"><button type="button" class="btn btn-soft btn-sm" data-action="unban-ip" data-ip="${esc(ban.ip)}">解封</button></td>
                    </tr>`
                  )
                  .join("")}</tbody>
              </table></div>`
            : `<div class="empty">目前没有被封禁的地址</div>`
        }
      </div>`;
  }

  function bindAdminIpBans() {
    const form = document.querySelector("#ip-ban-form");
    form?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(form);
      const ip = String(data.get("ip") || "").trim();
      const reason = String(data.get("reason") || "").trim();
      const banUsers = data.get("ban_users") === "on";
      confirmModal({
        title: "封禁地址",
        message: banUsers ? `封禁 ${ip}，并封禁在该地址登录或调用过的普通账号？` : `封禁 ${ip}？`,
        confirmText: "封禁",
        danger: true,
        async onConfirm() {
          const res = await api("/api/admin/ip-bans", { method: "POST", body: { ip, reason: reason || null, ban_users: banUsers } });
          toast(`已封禁 ${res.ip}${res.banned_users ? `，连带封禁 ${res.banned_users} 个账号` : ""}`, "success");
          render();
        },
      });
    });
  }

  function handleUnbanIp(ip) {
    confirmModal({
      title: "解除封禁",
      message: `解除对 ${ip} 的封禁，并恢复因它被连带封禁的账号？`,
      confirmText: "解封",
      async onConfirm() {
        const res = await api(`/api/admin/ip-bans/${encodeURIComponent(ip)}?unban_users=true`, { method: "DELETE" });
        toast(`已解封 ${res.ip}${res.unbanned_users ? `，恢复 ${res.unbanned_users} 个账号` : ""}`, "success");
        render();
      },
    });
  }

  /* ---------- channel model test (New-API style) ---------- */

  function jsonFieldText(value) {
    if (!value || typeof value !== "object" || !Object.keys(value).length) return "";
    return JSON.stringify(value, null, 2);
  }

  function fmtLatency(ms) {
    const n = Number(ms || 0);
    return n >= 1000 ? `${(n / 1000).toFixed(2)} 秒` : `${n} 毫秒`;
  }

  function channelFeatureBadges(channel) {
    const tags = [];
    const headerCount = channel.extra_headers ? Object.keys(channel.extra_headers).length : 0;
    if (headerCount) tags.push(`<span class="badge badge-muted" title="${esc(Object.keys(channel.extra_headers).join(", "))}">+${headerCount} 请求头</span>`);
    if (channel.strip_reasoning_with_tools) tags.push(`<span class="badge badge-muted" title="有 tools 时去掉 reasoning_effort / reasoning">tools 去推理参数</span>`);
    const overCount = channel.param_override ? Object.keys(channel.param_override).length : 0;
    if (channel.test_endpoint === "responses") tags.push(`<span class="badge badge-muted" title="模型检测走 /responses">检测 /responses</span>`);
    if (overCount) tags.push(`<span class="badge badge-muted" title="${esc(JSON.stringify(channel.param_override))}">参数覆盖 ${overCount}</span>`);
    return tags.length ? `<div class="tier-tags" style="margin-top:4px">${tags.join("")}</div>` : "";
  }

  function channelLastTestCell(channel) {
    if (channel.last_test_ok === null || channel.last_test_ok === undefined) return "";
    const ok = !!channel.last_test_ok;
    const title = ok ? `${channel.last_test_model || ""}` : `${channel.last_test_model || ""}：${channel.last_error || "失败"}`;
    return `<span class="badge ${ok ? "badge-success" : "badge-danger"}" title="${esc(title)}${channel.last_test_at ? " · " + esc(fmtShortDate(channel.last_test_at)) : ""}">${icon("zap", 11)}${ok ? fmtLatency(channel.last_test_ms) : "检测失败"}</span>`;
  }

  function testStatusBadge(row) {
    switch (row.status) {
      case "testing": return `<span class="badge badge-muted">${icon("loader", 12)}测试中</span>`;
      case "ok": return `<span class="badge badge-success">成功</span>`;
      case "fail": return `<span class="badge badge-danger">失败</span>`;
      default: return `<span class="badge badge-muted">未测试</span>`;
    }
  }

  function testResultCell(row) {
    if (row.status === "ok") return `<span class="mono" title="${esc(row.reply ? "回复：" + row.reply : "")}">${fmtLatency(row.latency_ms)}</span>`;
    if (row.status === "fail") {
      const full = row.error_detail || row.error || "失败";
      return `<span class="test-err" title="${esc(full)}" style="color:var(--danger,#d93025);font-size:12px;display:inline-block;max-width:320px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;vertical-align:middle">${esc(row.error || "失败")}</span>`;
    }
    if (row.status === "testing") return `<span class="faint" style="font-size:12px">请求中…</span>`;
    return `<span class="faint" style="font-size:12px">—</span>`;
  }

  async function runWithConcurrency(items, limit, worker) {
    let index = 0;
    const runners = Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (index < items.length) {
        const item = items[index++];
        await worker(item);
      }
    });
    await Promise.all(runners);
  }

  async function openChannelTestModal(channelId) {
    const channel = (ui.channels || []).find((item) => item.id === Number(channelId));
    if (!channel) return;
    const rows = (channel.models || []).map((m) => ({
      model: m.model,
      upstream: m.upstream_model,
      enabled: m.enabled,
      status: m.last_test_ok === null || m.last_test_ok === undefined ? "idle" : m.last_test_ok ? "ok" : "fail",
      latency_ms: m.last_test_ms,
      error: m.last_test_ok === false ? (m.last_test_error || "").split("\n")[0].slice(0, 200) : null,
      error_detail: m.last_test_error,
      checked: false,
      stale: m.last_test_ok !== null && m.last_test_ok !== undefined,
    }));
    let busy = 0;
    let dirty = false;
    const root = openModal({
      title: `模型检测 · ${channel.name}`,
      subtitle: `用渠道 Key 发送一条最小 ${channel.test_endpoint === "responses" ? "/responses" : "/chat/completions"} 请求（"hi"，非流式，超时 60 秒），不计入用户用量`,
      xwide: true,
      body: `
        <div class="filters" style="margin-bottom:12px">
          <input class="input" id="test-custom-model" placeholder="临时检测其他模型名（不会改动渠道状态）" style="min-width:260px">
          <button type="button" class="btn btn-soft btn-sm" id="test-custom-btn">${icon("zap", 13)}检测</button>
          <span class="faint" id="test-summary" style="font-size:12px"></span>
        </div>
        <div class="table-wrap" style="max-height:52vh;overflow:auto"><table>
          <thead><tr><th style="width:28px"><input type="checkbox" id="test-select-all"></th><th>模型</th><th>状态</th><th>结果</th><th class="right">操作</th></tr></thead>
          <tbody id="test-rows"></tbody>
        </table></div>`,
      footer: `
        <button type="button" class="btn btn-ghost" id="test-connectivity">${icon("server", 13)}连通性（/models）</button>
        <button type="button" class="btn btn-soft" data-action="close-modal">关闭</button>
        <button type="button" class="btn btn-primary" id="test-selected">${icon("zap", 14)}批量测试所选</button>`,
    });

    const tbody = root.querySelector("#test-rows");
    const summary = root.querySelector("#test-summary");
    const selectAll = root.querySelector("#test-select-all");

    function paint() {
      if (!rows.length) {
        tbody.innerHTML = `<tr><td colspan="5"><div class="empty">该渠道还没有模型，可在「模型」中同步或用上方输入框临时检测</div></td></tr>`;
      } else {
        tbody.innerHTML = rows
          .map(
            (row, i) => `<tr>
              <td><input type="checkbox" data-test-pick="${i}" ${row.checked ? "checked" : ""}></td>
              <td><div class="cell-strong mono" style="font-size:12px">${esc(row.model)}</div>${row.upstream && row.upstream !== row.model ? `<div class="cell-sub mono">→ ${esc(row.upstream)}</div>` : ""}${row.enabled === false ? `<div class="cell-sub">已停用</div>` : ""}${row.adhoc ? `<div class="cell-sub">临时检测</div>` : ""}</td>
              <td>${testStatusBadge(row)}${row.stale && row.status !== "testing" ? `<div class="cell-sub">上次结果</div>` : ""}</td>
              <td>${testResultCell(row)}</td>
              <td><div class="cell-actions"><button type="button" class="btn btn-ghost btn-sm" data-test-one="${i}" title="测试该模型" aria-label="测试该模型" ${row.status === "testing" ? "disabled" : ""}>${icon("zap", 13)}</button></div></td>
            </tr>`
          )
          .join("");
      }
      const done = rows.filter((r) => r.status === "ok" || r.status === "fail");
      const ok = rows.filter((r) => r.status === "ok").length;
      summary.textContent = rows.length ? `${ok} 成功 / ${done.length - ok} 失败 / 共 ${rows.length}${busy ? ` · 进行中 ${busy}` : ""}` : "";
      selectAll.checked = rows.length > 0 && rows.every((r) => r.checked);
    }

    async function testRow(row) {
      row.status = "testing";
      row.stale = false;
      busy += 1;
      paint();
      try {
        const res = await api(`/api/admin/channels/${channel.id}/test`, { method: "POST", body: { model: row.model } });
        row.status = res.ok ? "ok" : "fail";
        row.latency_ms = res.latency_ms;
        row.error = res.error;
        row.error_detail = res.error_detail || res.error;
        row.reply = res.reply;
        if (res.configured) dirty = true;
      } catch (error) {
        row.status = "fail";
        row.error = error.message;
        row.error_detail = error.message;
      } finally {
        busy -= 1;
        if (root.isConnected && root.querySelector("#test-rows")) paint();
      }
    }

    tbody.addEventListener("click", (event) => {
      const one = event.target.closest("[data-test-one]");
      if (one) testRow(rows[Number(one.dataset.testOne)]);
    });
    tbody.addEventListener("change", (event) => {
      const pick = event.target.closest("[data-test-pick]");
      if (pick) {
        rows[Number(pick.dataset.testPick)].checked = pick.checked;
        paint();
      }
    });
    selectAll.addEventListener("change", () => {
      rows.forEach((r) => (r.checked = selectAll.checked));
      paint();
    });
    root.querySelector("#test-selected").addEventListener("click", async (event) => {
      const picked = rows.filter((r) => r.checked && r.status !== "testing");
      if (!picked.length) {
        toast("请先勾选要测试的模型", "info");
        return;
      }
      const btn = event.currentTarget;
      btn.disabled = true;
      await runWithConcurrency(picked, 3, testRow);
      btn.disabled = false;
      const ok = picked.filter((r) => r.status === "ok").length;
      toast(`批量测试完成：${ok} 成功，${picked.length - ok} 失败`, ok === picked.length ? "success" : "error", 4500);
    });
    root.querySelector("#test-custom-btn").addEventListener("click", () => {
      const input = root.querySelector("#test-custom-model");
      const name = input.value.trim();
      if (!name) return;
      let row = rows.find((r) => r.model === name);
      if (!row) {
        row = { model: name, status: "idle", checked: false, adhoc: true };
        rows.unshift(row);
      }
      input.value = "";
      testRow(row);
    });
    root.querySelector("#test-connectivity").addEventListener("click", async (event) => {
      const btn = event.currentTarget;
      btn.disabled = true;
      try {
        const result = await api(`/api/admin/channels/${channel.id}/test`, { method: "POST", body: {} });
        if (result.ok) toast(`渠道连通，上游 /models 返回 ${result.model_count} 个模型（${result.latency_ms}ms）`, "success", 4500);
        else toast(result.error || "连通性检查失败", "error", 6000);
        dirty = true;
      } catch (error) {
        toast(error.message, "error");
      } finally {
        btn.disabled = false;
      }
    });
    // 关闭弹窗后刷新渠道列表，显示最新检测结果
    const obs = new MutationObserver(() => {
      if (!modalRoot.firstElementChild) {
        obs.disconnect();
        if (dirty && currentRoute && location.hash.startsWith("#/admin/channels")) render();
      }
    });
    obs.observe(modalRoot, { childList: true });
    paint();
  }

  async function handleTestAllChannels(button) {
    if (button) {
      button.disabled = true;
      button.innerHTML = `${icon("loader", 13)}测试中…`;
    }
    try {
      const res = await api("/api/admin/channels/test-all", { method: "POST", body: {} });
      const failed = (res.results || []).filter((r) => !r.ok);
      const names = new Map((ui.channels || []).map((c) => [c.id, c.name]));
      if (!failed.length) toast(`全部 ${res.total} 个渠道检测成功`, "success", 4500);
      else toast(`${res.ok} 成功，${res.failed} 失败：${failed.slice(0, 3).map((r) => `${names.get(r.channel_id) || r.channel_id}（${r.error || "失败"}）`).join("；")}`, "error", 8000);
    } catch (error) {
      toast(error.message, "error");
    }
    render();
  }

  /* ---------- admin settings ---------- */

  async function loadAdminSettings() {
    const [me, settings, prices, audit, caps] = await Promise.all([
      api("/api/me"),
      api("/api/admin/settings"),
      api("/api/admin/prices"),
      api("/api/admin/audit"),
      api("/api/admin/free-token-caps"),
    ]);
    state.me = me;
    return { me, settings, prices: prices.prices || [], audit: audit.audit || [], caps: caps || { default_cap: 1000000000, overrides: [] } };
  }

  const AUDIT_LABELS = {
    "user.ban": "封禁用户",
    "user.unban": "解封用户",
    "user.grant": "手动授予档位",
    "user.revoke": "撤销 Pro / Max",
    "plan.create": "添加套餐",
    "plan.update": "修改套餐",
    "plan.delete": "删除套餐",
    "application.approve": "通过晋升申请",
    "application.reject": "驳回晋升申请",
    "channel.create": "添加渠道",
    "channel.update": "编辑渠道",
    "channel.delete": "删除渠道",
    "channel.set_tiers": "设置渠道档位",
    "channel.bulk_tiers": "批量设置渠道档位",
    "channel.sync_models": "同步渠道模型",
    "channel_model.create": "添加模型",
    "channel_model.update": "编辑模型",
    "channel_model.delete": "删除模型",
    "channel_model.set_tiers": "设置模型档位",
    "settings.registration": "修改注册开关",
    "settings.channel_auto_test": "修改渠道自动检测间隔",
    "settings.channel_auto_disable": "修改失败自动禁用",
    "channel.auto_disable": "检测失败自动禁用渠道",
    "ip.ban": "封禁 IP",
    "ip.release": "解除 IP 封禁",
    "price.save": "保存模型价格",
    "price.delete": "删除模型价格",
    "key.distinct_ip_alert": "Key 多 IP 告警",
    "key.distinct_ip_block": "Key 多 IP 限流",
    "free_token_cap.set": "设置免费模型上限",
    "free_token_cap.delete": "取消免费模型上限",
  };

  function auditLabel(action) {
    return AUDIT_LABELS[action] || action;
  }

  function renderAdminSettings({ settings, prices, audit, caps }) {
    return `
      ${adminTabs("/admin/settings")}
      ${adminHeader("站点设置", "注册开关、应用信息、模型价格、免费模型上限与审计日志")}
      <div class="overview-grid">
        <div class="card">
          <div class="card-head"><div><div class="card-title">注册开关</div><div class="card-sub">关闭后仅已有账号可登录</div></div><span class="badge ${settings.registration_open ? "badge-success" : "badge-warning"}">${settings.registration_open ? "开放注册" : "关闭注册"}</span></div>
          <form id="settings-form" class="flex between flex-wrap">
            <label class="switch"><input type="checkbox" name="registration_open" ${settings.registration_open ? "checked" : ""}><span class="switch-track"></span><span class="switch-text">允许新用户通过 LINUX DO 注册</span></label>
            <button class="btn btn-primary btn-sm" type="submit">保存</button>
          </form>
          <div class="divider"></div>
          <div class="mini-list">
            <div class="mini-row"><span class="muted">应用主页</span><span class="mono" style="font-size:11px">https://plan.pmcat.top</span></div>
            <div class="mini-row"><span class="muted">回调地址</span><span class="mono" style="font-size:11px">${esc(settings.callback_url)}</span></div>
            <div class="mini-row"><span class="muted">LINUX DO 等级</span><span>Level 0</span></div>
          </div>
          <div class="notice mt-16">${icon("info", 16)}<div>回调地址必须与 LINUX DO Connect 应用中配置的地址完全一致，否则 OAuth 会失败。</div></div>
        </div>
        <div class="card">
          <div class="card-head"><div><div class="card-title">审计日志</div><div class="card-sub">最近 200 条站长操作</div></div></div>
          <div class="mini-list audit-scroll">${audit.length
            ? audit
                .slice(0, 80)
                .map(
                  (row) => `<div class="mini-row"><div class="mini-row-main"><strong>${esc(auditLabel(row.action))}</strong><span>${esc(row.actor_name || "系统")} · ${esc(row.target_type || "")}</span></div><span class="mini-row-side">${fmtShortDate(row.created_at)}</span></div>`
                )
                .join("")
            : `<div class="faint" style="font-size:12px">暂无审计记录</div>`}</div>
        </div>
      </div>
      <div class="card mt-24">
        <div class="card-head"><div><div class="card-title">渠道自动检测</div><div class="card-sub">定时用每个启用渠道的默认测试模型发一次最小请求（不计入用户用量）</div></div><span class="badge ${Number(settings.channel_auto_test_minutes || 0) > 0 ? "badge-success" : "badge-muted"}">${Number(settings.channel_auto_test_minutes || 0) > 0 ? `每 ${Number(settings.channel_auto_test_minutes)} 分钟` : "已关闭"}</span></div>
        <form id="auto-test-form" class="filters">
          <label class="faint" style="font-size:12px;display:inline-flex;align-items:center;gap:6px">间隔（分钟，0 = 关闭）<input class="input" name="channel_auto_test_minutes" type="number" min="0" max="10080" step="1" value="${Number(settings.channel_auto_test_minutes || 0)}" style="width:110px"></label>
          <label class="switch"><input type="checkbox" name="channel_auto_disable" ${settings.channel_auto_disable ? "checked" : ""}><span class="switch-track"></span><span class="switch-text">检测失败时自动禁用渠道</span></label>
          <button class="btn btn-primary btn-sm" type="submit">保存</button>
        </form>
      </div>
      <div class="card mt-24">
        <div class="card-head"><div><div class="card-title">模型价格</div><div class="card-sub">单位：美元 / 1M tokens；渠道模型可单独覆盖</div></div><span class="badge badge-muted">${prices.length} 条</span></div>
        <form id="price-add-form" class="filters" style="margin-bottom:12px">
          <input class="input" name="model" placeholder="模型名，如 gpt-4o" required style="min-width:180px">
          <input class="input" name="input_price" type="number" step="0.0001" min="0" placeholder="输入 $/1M" required style="width:120px">
          <input class="input" name="output_price" type="number" step="0.0001" min="0" placeholder="输出 $/1M" required style="width:120px">
          <button class="btn btn-soft btn-sm" type="submit">${icon("plus", 13)}添加</button>
        </form>
        <div class="table-wrap price-scroll"><table class="price-table">
          <thead><tr><th>模型</th><th class="num">输入 $/1M</th><th class="num">输出 $/1M</th><th class="right">操作</th></tr></thead>
          <tbody>${prices
            .map(
              (price) => `<tr>
                <td><span class="mono" style="font-size:11px">${esc(price.model)}</span></td>
                <td class="num"><input class="input" data-price-row="${esc(price.model)}" data-field="input" type="number" step="0.0001" min="0" value="${(price.input_micro / 1e6).toFixed(4)}"></td>
                <td class="num"><input class="input" data-price-row="${esc(price.model)}" data-field="output" type="number" step="0.0001" min="0" value="${(price.output_micro / 1e6).toFixed(4)}"></td>
                <td><div class="cell-actions"><button type="button" class="btn btn-ghost btn-sm" data-action="save-price" data-model="${esc(price.model)}">保存</button><button type="button" class="btn btn-ghost btn-sm" data-action="delete-price" data-model="${esc(price.model)}">${icon("trash", 13)}</button></div></td>
              </tr>`
            )
            .join("")}</tbody>
        </table></div>
      </div>
      <div class="card mt-24">
        <div class="card-head"><div><div class="card-title">免费模型日上限</div><div class="card-sub">默认每天 ${Number(caps.default_cap || 0).toLocaleString()} tokens（Key 与用户各算）；单独设置优先于默认值，重启不丢</div></div><span class="badge badge-muted">默认 ${Number(caps.default_cap || 0).toLocaleString()}</span></div>
        <form id="cap-add-form" class="filters" style="margin-bottom:12px">
          <select class="input" name="target_type" style="width:110px"><option value="key">指定 Key</option><option value="user">指定用户</option></select>
          <input class="input" name="target_id" type="number" min="1" step="1" placeholder="Key ID / 用户 ID" required style="width:150px">
          <input class="input" name="cap_tokens" type="number" min="1" step="1" placeholder="上限 tokens，如 5000000000" required style="min-width:190px">
          <input class="input" name="note" placeholder="备注（可选）" style="min-width:140px">
          <button class="btn btn-soft btn-sm" type="submit">保存设置</button>
        </form>
        <div class="mini-list">${caps.overrides && caps.overrides.length
          ? caps.overrides
              .map(
                (row) => `<div class="mini-row"><div class="mini-row-main"><strong>${row.key_id ? `Key #${row.key_id}${row.key_prefix ? `（${esc(row.key_prefix)}）` : ""}` : `用户 #${row.user_id}${row.username ? `（${esc(row.username)}）` : ""}`}</strong><span>${row.note ? esc(row.note) + " · " : ""}${fmtShortDate(row.updated_at)}</span></div><span class="mini-row-side" style="display:inline-flex;align-items:center;gap:8px"><span>${Number(row.cap_tokens).toLocaleString()} tokens/天</span><button type="button" class="btn btn-ghost btn-sm" data-action="delete-cap" data-id="${row.id}">删除</button></span></div>`
              )
              .join("")
          : `<div class="faint" style="font-size:12px">暂无单独设置，全部走默认值</div>`}</div>
      </div>`;
  }

  function bindAdminSettings() {
    const form = document.querySelector("#settings-form");
    form?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const open = new FormData(form).get("registration_open") === "on";
      try {
        await api("/api/admin/settings", { method: "PATCH", body: { registration_open: open } });
        toast(open ? "已开放注册" : "已关闭注册", "success");
        render();
      } catch (error) {
        toast(error.message, "error");
      }
    });
    const autoForm = document.querySelector("#auto-test-form");
    autoForm?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(autoForm);
      const minutes = Math.max(0, Math.round(Number(data.get("channel_auto_test_minutes") || 0)));
      try {
        await api("/api/admin/settings", {
          method: "PATCH",
          body: { channel_auto_test_minutes: minutes, channel_auto_disable: data.get("channel_auto_disable") === "on" },
        });
        toast(minutes > 0 ? `已开启自动检测（每 ${minutes} 分钟）` : "已关闭自动检测", "success");
        render();
      } catch (error) {
        toast(error.message, "error");
      }
    });
    const addForm = document.querySelector("#price-add-form");
    addForm?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(addForm);
      try {
        await api("/api/admin/prices", {
          method: "POST",
          body: {
            model: String(data.get("model") || "").trim(),
            input_micro: Math.round(Number(data.get("input_price")) * 1e6),
            output_micro: Math.round(Number(data.get("output_price")) * 1e6),
          },
        });
        toast("价格已保存", "success");
        render();
      } catch (error) {
        toast(error.message, "error");
      }
    });
    const capForm = document.querySelector("#cap-add-form");
    capForm?.addEventListener("submit", async (event) => {
      event.preventDefault();
      const data = new FormData(capForm);
      const targetType = String(data.get("target_type") || "key");
      const targetId = Math.round(Number(data.get("target_id") || 0));
      const capTokens = Math.round(Number(data.get("cap_tokens") || 0));
      if (!targetId || !capTokens || capTokens <= 0) {
        toast("请填写正确的目标 ID 与上限", "error");
        return;
      }
      try {
        await api("/api/admin/free-token-caps", {
          method: "POST",
          body: {
            ...(targetType === "user" ? { user_id: targetId } : { key_id: targetId }),
            cap_tokens: capTokens,
            note: String(data.get("note") || "").trim(),
          },
        });
        toast("单独上限已保存", "success");
        render();
      } catch (error) {
        toast(error.message, "error");
      }
    });
  }

  /* ---------- global actions ---------- */

  async function handleCheckin(button) {
    button.disabled = true;
    try {
      const cap = await runCheckinCaptcha();
      if (!cap) {
        button.disabled = false;
        return;
      }
      const result = await api("/api/checkin", { method: "POST", body: { captcha_token: cap.token, captcha_provider: cap.provider } });
      const card = document.querySelector(".checkin-card");
      if (card) card.classList.add("success");
      toast(result.message, "success", 4500);
      ui.usagePage = 1;
      await render();
    } catch (error) {
      button.disabled = false;
      toast(error.message, "error");
    }
  }

  // 签到人机验证: 双通道 (hCaptcha 为主, Turnstile 备用), 通过后返回 {token, provider}; 取消返回 null
  // 关键: 立即弹出弹窗(先 loading), 再异步加载脚本/取通道 —— 避免"只发请求不出弹窗"
  // hCaptcha 强制验证: 显式 size:"normal" 渲染可见复选框(绝不用隐形模式);
  // 是否弹题、题目难度由 hCaptcha 后台该 sitekey 的 Behavior(须为 Always Challenge)
  // 与难度档位决定, 代码里改不了 —— 免费版支持 Always Challenge + Easy/Medium/Difficult/Auto,
  // 只有 Passive / 99.9% Passive 才是付费版; Pro 试用默认就是 99.9% Passive。
  let hcaptchaScriptPromise = null;
  let turnstileScriptPromise = null;
  function loadHcaptchaScript() {
    if (window.hcaptcha) return Promise.resolve();
    if (!hcaptchaScriptPromise) {
      hcaptchaScriptPromise = new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = "https://js.hcaptcha.com/1/api.js?render=explicit&hl=zh-CN";
        s.async = true;
        s.defer = true;
        s.onload = () => resolve();
        s.onerror = () => {
          hcaptchaScriptPromise = null;
          reject(new Error("验证组件加载失败，请检查网络后重试"));
        };
        document.head.appendChild(s);
      });
    }
    return hcaptchaScriptPromise;
  }
  function loadTurnstileScript() {
    if (window.turnstile) return Promise.resolve();
    if (!turnstileScriptPromise) {
      turnstileScriptPromise = new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
        s.async = true;
        s.defer = true;
        s.onload = () => resolve();
        s.onerror = () => {
          turnstileScriptPromise = null;
          reject(new Error("备用验证组件加载失败，请检查网络后重试"));
        };
        document.head.appendChild(s);
      });
    }
    return turnstileScriptPromise;
  }
  // 每次验证都现取当前通道(通道可能刚切换过, 不缓存); 返回 {provider, sitekey, fallback}
  async function getCaptchaProvider() {
    const data = await api("/api/captcha/provider");
    if (!data || !data.provider || !data.sitekey) throw new Error("人机验证未配置");
    return data;
  }
  function runCheckinCaptcha() {
    return new Promise((resolve) => {
      let done = false;
      let widgetId = null;
      let activeProvider = null;
      const finish = (val) => { if (!done) { done = true; resolve(val); } };

      openModal({
        title: "签到人机验证",
        subtitle: "请完成下方验证，通过后自动继续",
        wide: true,
        body: `
          <div class="captcha-widget-wrap" id="captcha-widget-wrap" style="display:flex;justify-content:center;padding:8px 0 2px;min-height:78px">
            <div class="captcha-loading" id="captcha-loading">${icon("loader", 20)}<span>正在加载验证…</span></div>
          </div>
          <div id="captcha-widget-tip" class="muted" style="margin-top:8px;font-size:12px;min-height:16px;text-align:center"></div>`,
        onMount: function () {
          const wrap = document.getElementById("captcha-widget-wrap");
          const loading = document.getElementById("captcha-loading");
          const tip = document.getElementById("captcha-widget-tip");
          const theme = ui.theme === "dark" ? "dark" : "light";
          const showError = (msg) => {
            if (loading) loading.outerHTML = `<div class="muted" style="text-align:center;padding:20px 0">${esc(msg)}</div>`;
            if (tip) tip.textContent = "请关闭后重试，或稍后再试";
          };
          const resetWidget = () => {
            try {
              if (activeProvider === "hcaptcha" && window.hcaptcha) window.hcaptcha.reset(widgetId);
              else if (activeProvider === "turnstile" && window.turnstile) window.turnstile.reset(widgetId);
            } catch (e) { /* ignore */ }
          };
          // 渲染指定通道; true=控件已就绪, false=加载/渲染失败(调用方可切备用通道)
          const renderChannel = async (channel) => {
            if (done) return false;
            try {
              if (channel.provider === "turnstile") {
                await loadTurnstileScript();
                if (!window.turnstile || done) return false;
              } else {
                await loadHcaptchaScript();
                if (!window.hcaptcha || done) return false;
              }
            } catch (e) {
              return false;
            }
            if (loading) loading.remove();
            wrap.innerHTML = "";
            activeProvider = channel.provider;
            try {
              if (channel.provider === "turnstile") {
                // Turnstile 显式渲染(不用 .cf-turnstile 自动渲染, 避免 SPA 弹窗里重复渲染);
                // action 须为 checkin, 后端 siteverify 会校验 action 与 hostname
                widgetId = window.turnstile.render(wrap, {
                  sitekey: channel.sitekey,
                  action: "checkin",
                  theme,
                  callback: (token) => {
                    if (token) {
                      const p = activeProvider;
                      try { window.turnstile.reset(widgetId); } catch (e) { /* ignore */ }
                      closeModal();
                      finish({ token, provider: p });
                    }
                  },
                  "expired-callback": () => {
                    if (tip) tip.textContent = "验证已过期，请重新验证";
                    resetWidget();
                  },
                  "error-callback": () => {
                    if (tip) tip.textContent = "验证加载失败，请关闭后重试";
                  },
                });
              } else {
                widgetId = window.hcaptcha.render(wrap, {
                  sitekey: channel.sitekey,
                  theme,
                  size: "normal", // 可见复选框(强制验证); 难度与是否弹题由 hCaptcha 后台决定
                  callback: (token) => {
                    if (token) {
                      closeModal();
                      finish({ token, provider: "hcaptcha" });
                    }
                  },
                  "expired-callback": () => {
                    if (tip) tip.textContent = "验证已过期，请重新验证";
                    resetWidget();
                  },
                  "error-callback": () => {
                    if (tip) tip.textContent = "验证加载失败，请关闭后重试";
                  },
                });
              }
            } catch (e) {
              return false;
            }
            return true;
          };
          (async () => {
            try {
              const info = await getCaptchaProvider();
              const ok = await renderChannel({ provider: info.provider, sitekey: info.sitekey });
              if (done) return;
              if (!ok && info.fallback) {
                // 主通道脚本加载/渲染失败(例如 hCaptcha 被拦截或额度用尽导致控件不可用):
                // 就地切到备用通道, 本次验证走备用通道, 后端按 token 实际通道校验
                if (tip) tip.textContent = "主验证加载失败，已切换到备用验证";
                const ok2 = await renderChannel(info.fallback);
                if (!ok2 && !done) showError("验证组件加载失败，请检查网络后重试");
              } else if (!ok) {
                showError("验证组件加载失败，请检查网络后重试");
              }
            } catch (e) {
              showError(e.message || "验证加载失败，请重试");
            }
          })();
        },
      });
      // 关闭弹窗时若无结果, resolve(null)
      const obs = new MutationObserver(() => {
        if (!modalRoot.firstElementChild && !done) {
          finish(null);
          obs.disconnect();
        }
      });
      obs.observe(modalRoot, { childList: true });
    });
  }

  function openEditPlanModal(code, builtin) {
    const plan = (ui.adminPlans || []).find((item) => item.code === code);
    const isCustom = !builtin;
    openModal({
      title: `改描述 · ${plan ? plan.name : code}`,
      subtitle: builtin ? "内置套餐仅可修改描述，档位与额度不可改" : "自定义套餐可改描述、名称、额度与排序",
      body: `
        <div class="field"><label>描述（显示给用户看）</label><input class="input" id="edit-plan-tagline" maxlength="200" value="${esc(plan?.tagline || "")}"></div>
        ${isCustom ? `
        <div class="field"><label>名称</label><input class="input" id="edit-plan-name" maxlength="50" value="${esc(plan?.name || "")}"></div>
        <div class="field"><label>5H 额度（美元，留空 = 不限）</label><input class="input" id="edit-plan-h5" type="number" step="0.01" min="0" value="${plan?.limits.h5 == null ? "" : plan.limits.h5 / 1e6}"></div>
        <div class="field"><label>7D 额度（美元，留空 = 不限）</label><input class="input" id="edit-plan-weekly" type="number" step="0.01" min="0" value="${plan?.limits.weekly == null ? "" : plan.limits.weekly / 1e6}"></div>
        <div class="field"><label>30D 额度（美元，留空 = 不限）</label><input class="input" id="edit-plan-monthly" type="number" step="0.01" min="0" value="${plan?.limits.monthly == null ? "" : plan.limits.monthly / 1e6}"></div>
        <div class="field"><label>排序</label><input class="input" id="edit-plan-sort" type="number" step="1" value="${plan?.sort_order ?? 0}"></div>` : ""}`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn btn-primary" id="save-plan-edit">保存</button>`,
      onMount(root) {
        root.querySelector("#save-plan-edit").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            const tagline = root.querySelector("#edit-plan-tagline").value.trim();
            if (!tagline) throw new Error("描述不能为空");
            const body = { tagline };
            if (isCustom) {
              const name = root.querySelector("#edit-plan-name").value.trim();
              if (!name) throw new Error("名称不能为空");
              body.name = name;
              const usd = (id) => {
                const value = root.querySelector(id).value.trim();
                return value === "" ? null : Number(value);
              };
              body.h5_usd = usd("#edit-plan-h5");
              body.weekly_usd = usd("#edit-plan-weekly");
              body.monthly_usd = usd("#edit-plan-monthly");
              body.sort_order = Math.round(Number(root.querySelector("#edit-plan-sort").value || 0));
            }
            await api(`/api/admin/plans/${encodeURIComponent(code)}`, { method: "PATCH", body });
            closeModal();
            toast("套餐已更新", "success");
            await render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function openGrantPlanModal(userId) {
    const plans = ui.adminPlans || [];
    if (!plans.length) {
      toast("套餐列表尚未加载，请刷新页面重试", "error");
      return;
    }
    openModal({
      title: "手动授予套餐",
      subtitle: "选择要授予的套餐；只能授予比用户当前档位更高的套餐，后端会校验。",
      body: `<div class="field"><label>授予套餐</label><select class="input" id="grant-plan-select">${plans
        .map((plan) => `<option value="${esc(plan.code)}">${esc(plan.name)}（${esc(plan.code)}）${plan.builtin ? "" : " · 自定义"}</option>`)
        .join("")}</select></div>
      <p class="field-hint">自定义套餐只能这样手动授予，不参与签到自动升级。</p>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn btn-primary" id="confirm-grant-plan">确认授予</button>`,
      onMount(root) {
        root.querySelector("#confirm-grant-plan").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            const plan = root.querySelector("#grant-plan-select").value;
            await api(`/api/admin/users/${userId}/grant`, { method: "POST", body: { plan } });
            closeModal();
            toast(`已授予 ${plan.toUpperCase()}`, "success");
            await render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function openSendNoticeModal(userId, username) {
    openModal({
      title: `给 ${username} 发站内通知`,
      subtitle: "用户下次登录后会在首页顶部看到这条通知。",
      body: `<div class="faint" style="font-size:12px;margin-bottom:6px">标题</div>
        <input class="input" id="notice-title" maxlength="100" value="API Key 使用异常警告" style="width:100%">
        <div class="faint" style="font-size:12px;margin:12px 0 6px">内容</div>
        <textarea class="input" id="notice-body" rows="6" style="width:100%">你的 API Key 近期出现异常使用（多个不同 IP 高频调用），涉嫌分发或共享给他人使用。请立即停止分发/共享行为，否则将吊销 Key、封禁账号。</textarea>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button>
        <button type="button" class="btn btn-primary" id="notice-send-btn">发送</button>`,
      onMount(root) {
        root.querySelector("#notice-send-btn").addEventListener("click", async () => {
          const title = root.querySelector("#notice-title").value.trim();
          const body = root.querySelector("#notice-body").value.trim();
          if (!title || !body) {
            toast("标题和内容不能为空", "error");
            return;
          }
          await api("/api/admin/notices", { method: "POST", body: { user_id: userId, title, body, kind: "warning" } });
          closeModal();
          toast("站内通知已发送", "success");
        });
      },
    });
  }

  function handleBan(userId, banned) {
    const user = (document.querySelector(`[data-action="ban-user"][data-id="${userId}"]`) || {});
    openModal({
      title: banned ? "解封用户" : "封禁用户",
      subtitle: "封禁后立即失效所有会话，API Key 也会被拒绝",
      body: banned
        ? `<p class="muted" style="margin:0">确定要解封该用户吗？解封后用户可重新登录。</p>`
        : `<div class="field"><label>封禁原因（可选）</label><textarea class="textarea" id="ban-reason" placeholder="例如：滥用额度 / 违反使用规则"></textarea></div>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn ${banned ? "btn-primary" : "btn-danger"}" id="ban-confirm">${banned ? "确认解封" : "确认封禁"}</button>`,
      onMount(root) {
        root.querySelector("#ban-confirm").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            await api(`/api/admin/users/${userId}/ban`, {
              method: "POST",
              body: { banned: !banned, reason: root.querySelector("#ban-reason")?.value || null },
            });
            closeModal();
            toast(banned ? "已解封" : "已封禁", "success");
            render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function handleAppReview(appId, targetPlan, approve, userName) {
    openModal({
      title: approve ? `通过 ${targetPlan.toUpperCase()} 申请` : `驳回 ${targetPlan.toUpperCase()} 申请`,
      subtitle: userName,
      body: `<div class="field"><label>审批备注（可选）</label><textarea class="textarea" id="review-note" placeholder="${approve ? "例如：签到记录符合要求" : "例如：签到记录不足 / 暂不开放"}"></textarea></div>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn ${approve ? "btn-primary" : "btn-danger"}" id="review-confirm">${approve ? "确认通过" : "确认驳回"}</button>`,
      onMount(root) {
        root.querySelector("#review-confirm").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            await api(`/api/admin/applications/${appId}/${approve ? "approve" : "reject"}`, {
              method: "POST",
              body: { note: root.querySelector("#review-note").value || null },
            });
            closeModal();
            toast(approve ? "已通过申请" : "已驳回申请", "success");
            render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  function openBulkTiers() {
    if (ui.selectedChannels.size === 0) {
      toast("请先选择至少一个渠道", "error");
      return;
    }
    openModal({
      title: `批量修改档位授权（${ui.selectedChannels.size} 个渠道）`,
      subtitle: "不勾选 = 全部档位可调用",
      body: `<div class="field"><label>允许调用的套餐档位</label>${tierPicker("bulk-tier", [])}<div class="field-hint">将覆盖所选渠道的渠道级授权；模型级授权不会被修改。</div></div>`,
      footer: `<button type="button" class="btn btn-ghost" data-action="close-modal">取消</button><button type="button" class="btn btn-primary" id="bulk-save">保存授权</button>`,
      onMount(root) {
        root.querySelector("#bulk-save").addEventListener("click", async (event) => {
          const button = event.currentTarget;
          button.disabled = true;
          try {
            const tiers = Array.from(root.querySelectorAll('input[name="bulk-tier"]:checked')).map((item) => item.value);
            await api("/api/admin/channels/bulk-tiers", {
              method: "POST",
              body: { channel_ids: Array.from(ui.selectedChannels), tiers },
            });
            closeModal();
            ui.selectedChannels.clear();
            toast("批量授权已更新", "success");
            render();
          } catch (error) {
            button.disabled = false;
            toast(error.message, "error");
          }
        });
      },
    });
  }

  async function handleDeleteChannel(id) {
    const channel = (ui.channels || []).find((item) => item.id === Number(id));
    confirmModal({
      title: "删除渠道",
      message: `确定删除「${channel?.name || id}」吗？该渠道下的模型、档位授权会一并删除，历史用量记录保留。`,
      confirmText: "删除",
      danger: true,
      async onConfirm() {
        await api(`/api/admin/channels/${id}`, { method: "DELETE" });
        toast("渠道已删除", "success");
        render();
      },
    });
  }

  async function handleTestChannel(id) {
    const button = document.querySelector(`[data-action="test-channel"][data-id="${id}"]`);
    if (button) button.disabled = true;
    try {
      const result = await api(`/api/admin/channels/${id}/test`, { method: "POST", body: {} });
      if (result.ok) {
        toast(`渠道连通，发现 ${result.model_count} 个模型（${result.latency_ms}ms）`, "success", 4500);
      } else {
        toast(result.error || "渠道测试失败", "error", 5000);
      }
      render();
    } catch (error) {
      toast(error.message, "error");
      if (button) button.disabled = false;
    }
  }

  async function handleSyncModels(channelId, root) {
    try {
      const result = await api(`/api/admin/channels/${channelId}/sync-models`, { method: "POST", body: {} });
      toast(`同步完成：新增 ${result.added} 个，共 ${result.total} 个模型`, "success", 4500);
      await refreshModelsBody(channelId, root);
      render();
    } catch (error) {
      toast(error.message, "error");
    }
  }

  async function handleDeleteModel(modelId, root, channelId) {
    confirmModal({
      title: "删除模型",
      message: "确定删除该模型及其档位授权吗？",
      confirmText: "删除",
      danger: true,
      async onConfirm() {
        await api(`/api/admin/models/${modelId}`, { method: "DELETE" });
        toast("模型已删除", "success");
        await render();
      },
    });
  }

  async function handleSaveModelPrice(modelId, root) {
    const inputs = root.querySelectorAll(`[data-price-input="${modelId}"]`);
    const values = {};
    inputs.forEach((input) => (values[input.dataset.field] = input.value));
    const toMicro = (value) => (String(value).trim() === "" ? null : Math.round(Number(value) * 1e6));
    try {
      await api(`/api/admin/models/${modelId}`, {
        method: "PATCH",
        body: { input_micro: toMicro(values.input), output_micro: toMicro(values.output) },
      });
      toast("模型价格已更新", "success");
    } catch (error) {
      toast(error.message, "error");
    }
  }

  async function handleSavePrice(model) {
    const input = document.querySelector(`[data-price-row="${CSS.escape(model)}"][data-field="input"]`);
    const output = document.querySelector(`[data-price-row="${CSS.escape(model)}"][data-field="output"]`);
    try {
      await api(`/api/admin/prices/${encodeURIComponent(model)}`, {
        method: "PUT",
        body: {
          input_micro: Math.round(Number(input.value) * 1e6),
          output_micro: Math.round(Number(output.value) * 1e6),
        },
      });
      toast("全局价格已更新", "success");
    } catch (error) {
      toast(error.message, "error");
    }
  }

  async function handleDeletePrice(model) {
    confirmModal({
      title: "删除模型价格",
      message: `确定删除 ${model} 的全局价格吗？未单独配置价格的渠道模型将回退到默认价格。`,
      confirmText: "删除",
      danger: true,
      async onConfirm() {
        await api(`/api/admin/prices/${encodeURIComponent(model)}`, { method: "DELETE" });
        toast("价格已删除", "success");
        render();
      },
    });
  }

  async function handleDeleteCap(id) {
    confirmModal({
      title: "取消单独上限",
      message: "确定删除这条单独上限设置吗？该 Key / 用户将恢复默认日上限。",
      confirmText: "删除",
      danger: true,
      async onConfirm() {
        await api(`/api/admin/free-token-caps/${id}`, { method: "DELETE" });
        toast("单独上限已取消", "success");
        render();
      },
    });
  }

  document.addEventListener("click", async (event) => {
    const target = event.target.closest("[data-action]");
    if (!target) return;
    const action = target.dataset.action;
    try {
      switch (action) {
        case "toggle-theme": {
          ui.theme = ui.theme === "dark" ? "light" : "dark";
          document.documentElement.dataset.theme = ui.theme;
          localStorage.setItem("pmplan-theme", ui.theme);
          await render();
          break;
        }
        case "reload-page":
          location.reload();
          break;
        case "close-modal":
          closeModal();
          break;
        case "checkin":
          await handleCheckin(target);
          break;
        case "use-makeup": {
          const cardId = target.dataset.id;
          const select = document.querySelector("#makeup-date");
          const date = select ? select.value : "";
          if (!date) {
            toast("请选择要补签的日期", "error");
            break;
          }
          target.disabled = true;
          try {
            const cap = await runCheckinCaptcha();
            if (!cap) {
              target.disabled = false;
              break;
            }
            const result = await api(`/api/makeup-cards/${cardId}/use`, {
              method: "POST",
              body: { date, captcha_token: cap.token, captcha_provider: cap.provider },
            });
            toast(result.message, "success", 4500);
            await render();
          } catch (error) {
            target.disabled = false;
            toast(error.message, "error");
          }
          break;
        }
        case "use-makeup-plus": {
          const cardId = target.dataset.id;
          const missed = (target.dataset.missed || "").split(",").filter(Boolean);
          const date = missed[0] || "";
          if (!date) {
            toast("最近 7 天没有漏签", "error");
            break;
          }
          target.disabled = true;
          try {
            const cap = await runCheckinCaptcha();
            if (!cap) {
              target.disabled = false;
              break;
            }
            const result = await api(`/api/makeup-cards/${cardId}/use`, {
              method: "POST",
              body: { date, captcha_token: cap.token, captcha_provider: cap.provider },
            });
            toast(result.message, "success", 4500);
            await render();
          } catch (error) {
            target.disabled = false;
            toast(error.message, "error");
          }
          break;
        }
        case "redeem-reset": {
          const input = document.querySelector("#reset-code-input");
          const code = input ? input.value.trim() : "";
          if (!code) {
            toast("请先输入重置卡兑换码", "error");
            break;
          }
          target.disabled = true;
          try {
            await api("/api/reset-cards/redeem", { method: "POST", body: { code } });
            toast("重置卡已兑换到你的账户", "success");
            await render();
          } catch (error) {
            target.disabled = false;
            toast(error.message, "error");
          }
          break;
        }
        case "use-reset": {
          const id = target.dataset.id;
          confirmModal({
            title: "使用重置卡",
            message: "使用后 5 小时 / 7 天 / 30 天已用额度全部清零，且无法撤销。确定使用吗？",
            confirmText: "确认使用",
            danger: true,
            async onConfirm() {
              await api(`/api/reset-cards/${id}/use`, { method: "POST" });
              toast("额度已清零", "success");
              render();
            },
          });
          break;
        }
        case "issue-makeup-all": {
          confirmModal({
            title: "全员发放补签卡",
            message: "给所有未封禁且当前没有可用补签卡的用户各发一张（30 天有效）。确定发放吗？",
            confirmText: "发放",
            async onConfirm() {
              const result = await api("/api/admin/makeup-cards/issue-all", { method: "POST", body: {} });
              toast(`已发放 ${result.issued} 张补签卡`, "success", 4500);
              render();
            },
          });
          break;
        }
        case "issue-makeup-one": {
          target.disabled = true;
          try {
            await api("/api/admin/makeup-cards/issue", { method: "POST", body: { user_id: Number(target.dataset.id) } });
            toast("已发放补签卡", "success");
          } catch (error) {
            toast(error.message, "error");
          } finally {
            target.disabled = false;
          }
          break;
        }
        case "issue-makeup-one-plus": {
          target.disabled = true;
          try {
            await api("/api/admin/makeup-cards/issue", { method: "POST", body: { user_id: Number(target.dataset.id), kind: "plus" } });
            toast("已发放 Plus 补签卡", "success");
          } catch (error) {
            toast(error.message, "error");
          } finally {
            target.disabled = false;
          }
          break;
        }
        case "issue-reset-one": {
          target.disabled = true;
          try {
            await api("/api/admin/reset-codes/issue", { method: "POST", body: { user_id: Number(target.dataset.id) } });
            toast("已发放重置卡", "success");
          } catch (error) {
            toast(error.message, "error");
          } finally {
            target.disabled = false;
          }
          break;
        }
        case "promote-user": {
          confirmModal({
            title: "设为管理员",
            message: "确定将该用户提升为管理员吗？管理员可以使用全部管理功能（发卡、封禁、审批、渠道与价格管理等），但不能提拔其他管理员。",
            confirmText: "设为管理员",
            async onConfirm() {
              await api(`/api/admin/users/${target.dataset.id}/promote`, { method: "POST" });
              toast("已设为管理员", "success");
              render();
            },
          });
          break;
        }
        case "demote-user": {
          confirmModal({
            title: "取消管理员",
            message: "确定取消该用户的管理员身份吗？取消后立即失去全部管理权限，恢复为普通用户。",
            confirmText: "取消管理员",
            danger: true,
            async onConfirm() {
              await api(`/api/admin/users/${target.dataset.id}/demote`, { method: "POST" });
              toast("已取消管理员", "success");
              render();
            },
          });
          break;
        }
        case "issue-makeup-plus-all": {
          confirmModal({
            title: "全员发放 Plus 补签卡",
            message: "给所有未封禁且当前没有可用 Plus 补签卡的用户各发一张 Plus 卡（30 天有效），一次补齐 7 天内所有漏签。确定发放吗？",
            confirmText: "发放",
            async onConfirm() {
              const result = await api("/api/admin/makeup-cards/issue-all", { method: "POST", body: { kind: "plus" } });
              toast(`已发放 ${result.issued} 张 Plus 补签卡`, "success", 4500);
              render();
            },
          });
          break;
        }
        case "issue-reset-all": {
          confirmModal({
            title: "全员发放重置卡",
            message: "给所有未封禁且当前没有未使用重置卡的用户各直接发一张重置卡（长期有效，无需兑换码）。确定发放吗？",
            confirmText: "发放",
            async onConfirm() {
              const result = await api("/api/admin/reset-codes/issue-all", { method: "POST" });
              toast(`已发放 ${result.issued} 张重置卡`, "success", 4500);
              render();
            },
          });
          break;
        }
        case "create-reset-codes": {
          const countInput = document.querySelector("#reset-code-count");
          const daysInput = document.querySelector("#reset-code-days");
          const count = countInput ? Number(countInput.value) : 0;
          const days = daysInput && daysInput.value ? Number(daysInput.value) : null;
          if (!count || count < 1 || count > 100) {
            toast("数量需在 1–100 之间", "error");
            break;
          }
          target.disabled = true;
          try {
            const result = await api("/api/admin/reset-codes", {
              method: "POST",
              body: { count, expires_days: days },
            });
            const box = document.querySelector("#reset-codes-result");
            if (box) {
              box.innerHTML = `<div class="checkin-note">兑换码只显示一次，请复制保存：</div><div class="mini-list">${result.codes
                .map(
                  (code) =>
                    `<div class="mini-row"><code>${esc(code)}</code><button type="button" class="btn btn-soft btn-sm" data-action="copy-secret" data-copy="${esc(code)}">复制</button></div>`
                )
                .join("")}</div>`;
            }
            toast(`已生成 ${result.codes.length} 张重置卡`, "success");
          } catch (error) {
            toast(error.message, "error");
          } finally {
            target.disabled = false;
          }
          break;
        }
        case "open-key-modal":
          if (state.me) openKeyModal(await api("/api/models"));
          break;
        case "revoke-key": {
          const id = target.dataset.id;
          const prefix = target.dataset.prefix;
          confirmModal({
            title: "撤销 Key",
            message: `确定撤销 ${prefix} 吗？撤销后使用该 Key 的请求会立即被拒绝。`,
            confirmText: "撤销",
            danger: true,
            async onConfirm() {
              await api(`/api/keys/${id}`, { method: "DELETE" });
              toast("Key 已撤销", "success");
              render();
            },
          });
          break;
        }
        case "copy-secret": {
          const value = target.dataset.copy;
          try {
            await navigator.clipboard.writeText(value);
            toast("已复制到剪贴板", "success");
          } catch {
            const textarea = document.createElement("textarea");
            textarea.value = value;
            document.body.appendChild(textarea);
            textarea.select();
            document.execCommand("copy");
            textarea.remove();
            toast("已复制到剪贴板", "success");
          }
          pulseElement(target);
          break;
        }
        case "usage-page":
          ui.usagePage = Number(target.dataset.page || 1);
          await render();
          break;
        case "user-page":
          ui.userPage = Number(target.dataset.page || 1);
          await render();
          break;
        case "log-page":
          ui.logPage = Number(target.dataset.page || 1);
          await render();
          break;
        case "app-status":
          event.preventDefault();
          ui.appStatus = target.dataset.status || "pending";
          await render();
          break;
        case "ban-user":
          handleBan(Number(target.dataset.id), target.dataset.banned === "1");
          break;
        case "send-notice":
          openSendNoticeModal(Number(target.dataset.id), target.dataset.username || `#${target.dataset.id}`);
          break;
        case "notice-read": {
          const id = Number(target.dataset.id);
          await api(`/api/notices/${id}/read`, { method: "POST", body: {} });
          toast("已标记为已读", "success");
          await render();
          break;
        }
        case "edit-plan":
          openEditPlanModal(target.dataset.code, target.dataset.builtin === "1");
          break;
        case "delete-plan": {
          const code = target.dataset.code;
          confirmModal({
            title: `删除套餐 ${target.dataset.name || code}`,
            message: "删除后该套餐彻底消失。内置套餐、有人持有、被渠道或模型授权引用、有待审批申请的套餐会被后端拒绝删除。",
            confirmText: "确认删除",
            danger: true,
            async onConfirm() {
              await api(`/api/admin/plans/${encodeURIComponent(code)}`, { method: "DELETE" });
              toast("套餐已删除", "success");
              await render();
            },
          });
          break;
        }
        case "grant-plan-modal":
          openGrantPlanModal(Number(target.dataset.id));
          break;
        case "grant-plan": {
          const id = Number(target.dataset.id);
          const plan = target.dataset.plan;
          confirmModal({
            title: `手动授予 ${plan.toUpperCase()}`,
            message: `确认将该用户的套餐直接提升为 ${plan.toUpperCase()} 吗？该操作会记录到审计日志。`,
            confirmText: "确认授予",
            async onConfirm() {
              await api(`/api/admin/users/${id}/grant`, { method: "POST", body: { plan } });
              toast(`已授予 ${plan.toUpperCase()}`, "success");
              await render();
            },
          });
          break;
        }
        case "revoke-plan": {
          const id = Number(target.dataset.id);
          confirmModal({
            title: "撤销 Pro / Max",
            message: "撤销后将回退到用户已获得的最高自动套餐（通常为 Standard）。确定继续吗？",
            confirmText: "撤销",
            danger: true,
            async onConfirm() {
              await api(`/api/admin/users/${id}/revoke`, { method: "POST", body: {} });
              toast("已撤销高阶档位", "success");
              await render();
            },
          });
          break;
        }
        case "approve-app":
          handleAppReview(Number(target.dataset.id), target.dataset.plan, true, target.dataset.name);
          break;
        case "reject-app":
          handleAppReview(Number(target.dataset.id), target.dataset.plan, false, target.dataset.name);
          break;
        case "open-channel-modal":
          openChannelModal(target.dataset.id ? Number(target.dataset.id) : null);
          break;
        case "delete-channel":
          await handleDeleteChannel(Number(target.dataset.id));
          break;
        case "test-channel":
          await handleTestChannel(Number(target.dataset.id));
          break;
        case "open-test-modal":
          openChannelTestModal(Number(target.dataset.id));
          break;
        case "test-all-channels":
          await handleTestAllChannels(target);
          break;
        case "unban-ip":
          handleUnbanIp(target.dataset.ip);
          break;
        case "open-models":
          await openModelsModal(Number(target.dataset.id));
          break;
        case "sync-models":
          await handleSyncModels(Number(target.dataset.id), modalRoot);
          break;
        case "bulk-tiers":
          openBulkTiers();
          break;
        case "toggle-channel": {
          const id = Number(target.dataset.id);
          const enabled = target.dataset.enabled === "1";
          await api(`/api/admin/channels/${id}`, { method: "PATCH", body: { enabled: !enabled } });
          toast(enabled ? "渠道已停用" : "渠道已启用", "success");
          await render();
          break;
        }
        case "toggle-model": {
          const id = Number(target.dataset.id);
          const enabled = target.dataset.enabled === "1";
          await api(`/api/admin/models/${id}`, { method: "PATCH", body: { enabled: !enabled } });
          toast(enabled ? "模型已停用" : "模型已启用", "success");
          const modelsRoot = modalRoot.querySelector(".modal");
          if (modelsRoot && ui.channels) {
            const channelId = target.closest(".modal-body")?.querySelector("[data-action='sync-models']")?.dataset.id;
            if (channelId) await refreshModelsBody(Number(channelId), modalRoot);
          }
          break;
        }
        case "model-tiers": {
          const id = Number(target.dataset.id);
          const name = target.dataset.name || "";
          const channelId = target.closest(".modal-body")?.querySelector("[data-action='sync-models']")?.dataset.id;
          const row = target.closest("tr");
          const tiers = Array.from(row?.querySelectorAll(".tier-tag") || []).map((tag) => tag.textContent.trim().toLowerCase());
          openModelTiersModal(id, name, tiers.filter((tier) => TIERS.includes(tier)), Number(channelId));
          break;
        }
        case "delete-model": {
          const id = Number(target.dataset.id);
          const channelId = target.closest(".modal-body")?.querySelector("[data-action='sync-models']")?.dataset.id;
          await handleDeleteModel(id, modalRoot, Number(channelId));
          break;
        }
        case "save-model-price":
          await handleSaveModelPrice(Number(target.dataset.id), modalRoot);
          break;
        case "save-price":
          await handleSavePrice(target.dataset.model);
          break;
        case "delete-price":
          await handleDeletePrice(target.dataset.model);
          break;
        case "delete-cap":
          await handleDeleteCap(Number(target.dataset.id));
          break;
        default:
          break;
      }
    } catch (error) {
      if (error.status === 401) {
        state.me = null;
        renderLogin();
      }
      toast(error.message, "error");
    }
  });

  window.addEventListener("hashchange", () => {
    window.scrollTo({ top: 0, behavior: "auto" });
    render();
  });

  async function boot() {
    try {
      state.bootstrap = await api("/api/bootstrap");
    } catch (error) {
      state.bootError = error;
    }
    const params = new URLSearchParams(location.search);
    if (params.has("oauth_error")) {
      toast(params.get("oauth_error") || "登录失败", "error", 6000);
      const clean = location.pathname + location.hash;
      history.replaceState(null, "", clean);
    }
    try {
      state.me = await api("/api/me");
    } catch (error) {
      if (error.status !== 401) {
        toast(error.message, "error");
      }
      state.me = null;
    }
    await render();
  }

  boot();
})();
