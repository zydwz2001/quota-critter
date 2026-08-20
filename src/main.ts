import "./styles.css";
import lumoSprite from "./assets/lumo-sprite-smooth.png";
import pkg from "../package.json";
import { bridge } from "./bridge";
import {
  defaultSettings,
  demoSnapshot,
  formatReset,
  formatWindowDuration,
  quotaState,
  type AppServerState,
  type AppSettings,
  type AuthState,
  type QuotaSnapshot,
  type QuotaWindow,
  type UserFacingError,
} from "./model";
import { makeTranslator, resolveLocale, type ResolvedLocale, type Translator } from "./i18n";

const appRoot = document.querySelector<HTMLElement>("#app");
if (!appRoot) throw new Error("Quota Critter root element is missing");
const root: HTMLElement = appRoot;
const APP_VERSION: string = pkg.version;

const state: {
  snapshot: QuotaSnapshot | null;
  settings: AppSettings;
  expanded: boolean;
  settingsOpen: boolean;
  syncing: boolean;
  preview: boolean;
  server: AppServerState;
  auth: AuthState;
  error: UserFacingError | null;
} = {
  snapshot: null,
  settings: { ...defaultSettings },
  expanded: false,
  settingsOpen: false,
  syncing: false,
  preview: false,
  server: "starting",
  auth: "unknown",
  error: null,
};

let windowHeight = 152;
let windowWidth = 300;
let resizeFrame: number | null = null;
let suppressClickUntil = 0;

// 错误码 → i18n key 映射
const ERROR_KEY_MAP: Record<string, string> = {
  AUTH_REQUIRED: "error.authRequired",
  AUTH_UNSUPPORTED: "error.authUnsupported",
  CODEX_NOT_FOUND: "error.codexNotFound",
  CODEX_CONFIG_FOUND_BUT_CLI_MISSING: "error.codexConfigFoundButCliMissing",
  CODEX_CONFIGURED_PATH_MISSING: "error.codexConfiguredPathMissing",
  APP_SERVER_START_FAILED: "error.codexNotFound",
  RATE_LIMITS_EMPTY: "error.rateLimitsEmpty",
  REQUEST_TIMEOUT: "error.requestTimeout",
  AUTH_URL_MISSING: "error.authUrlMissing",
  UNSAFE_URL: "error.unsafeUrl",
  OPEN_BROWSER_FAILED: "error.openBrowserFailed",
  LOGIN_FAILED: "error.loginFailed",
  SETTINGS_WRITE_FAILED: "error.settingsWriteFailed",
};

function pickErrorMessage(code: string, t: Translator): string {
  for (const [needle, key] of Object.entries(ERROR_KEY_MAP)) {
    if (code.includes(needle)) return t(key);
  }
  return t("error.requestFailed");
}

function safe(value: string | number | undefined | null): string {
  return String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

function primaryWindow(snapshot: QuotaSnapshot | null): QuotaWindow | undefined {
  return snapshot?.windows.find((item) => item.kind === "individual")
    ?? snapshot?.windows.find((item) => item.kind === "primary")
    ?? snapshot?.windows[0];
}

function lumoFaceMarkup(status: ReturnType<typeof quotaState>): string {
  if (status === "full" || status === "unknown") return "";

  const skinPatches = `<ellipse cx="61" cy="59" rx="5.4" ry="7.6" fill="#a69cf9"/>
      <ellipse cx="84" cy="59" rx="5.4" ry="7.6" fill="#a096f8"/>`;
  const mouthPatch = `<ellipse cx="73" cy="64.5" rx="7.2" ry="5.2" fill="#a69df9"/>`;

  if (status === "steady" || status === "stale") {
    return `<svg class="lumo__face" viewBox="0 0 150 142" aria-hidden="true">
      ${skinPatches}
      <path d="M56.5 58.5 Q61 63 65.5 58.5 M79.5 58.5 Q84 63 88.5 58.5"/>
    </svg>`;
  }

  if (status === "low") {
    return `<svg class="lumo__face" viewBox="0 0 150 142" aria-hidden="true">
      ${mouthPatch}
      <path class="lumo__brows" d="M56 51 Q61 47.2 66 49.2 M79 49.2 Q84 47.2 89 51"/>
      <ellipse class="lumo__open-mouth" cx="73" cy="66" rx="4.3" ry="4.8"/>
    </svg>`;
  }

  return `<svg class="lumo__face" viewBox="0 0 150 142" aria-hidden="true">
    ${skinPatches}${mouthPatch}
    <path d="M56.5 59.5 Q61 56 65.5 59.5 M79.5 59.5 Q84 56 88.5 59.5 M68.5 68 Q73 63.5 77.5 68"/>
  </svg>`;
}

function lumoMarkup(
  status: ReturnType<typeof quotaState>,
  expression = status,
): string {
  return `<span class="lumo lumo--${status}" role="img" aria-label="Lumo ${status}">
    <img class="lumo__sprite" src="${lumoSprite}" alt="" draggable="false" />
    ${lumoFaceMarkup(expression)}
  </span>`;
}

function offlineIconMarkup(): string {
  return `<div class="offline-icon" aria-hidden="true">
    <svg viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
      <path d="M14 42c-4.4 0-8-3.6-8-8 0-3.9 2.8-7.2 6.5-7.9.6-5.4 5.2-9.6 10.7-9.6 4.7 0 8.7 3 10.2 7.2 1-.3 2-.5 3.1-.5 5.5 0 10 4.5 10 10 0 .8-.1 1.5-.3 2.2" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
      <line x1="12" y1="50" x2="52" y2="12" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"/>
    </svg>
  </div>`;
}

function orbitMarkup(window: QuotaWindow | undefined): string {
  const remaining = window?.remainingPercent ?? 0;
  const lit = Math.round(remaining / 10);
  const low = remaining < 20;
  const nodes = Array.from({ length: 10 }, (_, index) => {
    const angle = -118 + index * 27;
    const className = index < lit
      ? low ? "orbit-node orbit-node--low" : "orbit-node orbit-node--lit"
      : "orbit-node";
    return `<span class="${className}" style="--angle:${angle}deg;--node-index:${index}"></span>`;
  }).join("");
  return `<div class="orbit" aria-label="Quota shown by orbit activity">
    <div class="orbit-arc"></div>${nodes}
  </div>`;
}

function widgetState(snapshot: QuotaSnapshot | null): ReturnType<typeof quotaState> {
  return quotaState(primaryWindow(snapshot), snapshot?.stale ?? false);
}

function browserPreviewSnapshot(): QuotaSnapshot {
  const requested = new URLSearchParams(window.location.search).get("quota");
  const parsed = requested == null ? Number.NaN : Number(requested);
  if (!Number.isFinite(parsed)) return demoSnapshot;

  const remainingPercent = Math.min(100, Math.max(0, Math.round(parsed)));
  const primary = demoSnapshot.windows[0];
  return {
    ...demoSnapshot,
    fetchedAt: Date.now(),
    windows: [{
      ...primary,
      usedPercent: 100 - remainingPercent,
      remainingPercent,
    }],
  };
}

function errorMarkup(error: UserFacingError | null, t: Translator): string {
  if (!error) return "";
  let action = "";
  if (error.action === "login") {
    action = `<button class="error-action" data-action="login">${safe(t("action.login"))}</button>`;
  } else if (error.action === "retry") {
    action = `<button class="error-action" data-action="retry">${safe(t("action.retry"))}</button>`;
  } else if (error.action === "install") {
    action = `<button class="error-action" data-action="install">${safe(t("action.install"))}</button><button class="error-action error-action--ghost" data-action="open-settings">${safe(t("action.setPath"))}</button>`;
  }
  return `<div class="error-banner" role="alert"><div class="error-banner__text"><span class="error-dot"></span><span>${safe(error.message || pickErrorMessage(error.code, t))}</span></div><div class="error-banner__actions">${action}</div></div>`;
}

function amount(value: number | undefined): string {
  return value == null ? "" : Math.round(value).toLocaleString();
}

function quotaPanel(window: QuotaWindow | undefined, snapshot: QuotaSnapshot | null, locale: ResolvedLocale, t: Translator): string {
  if (!window) return `<div class="empty-state">${safe(t("panel.empty"))}</div>`;
  const status = quotaState(window, snapshot?.stale ?? false);
  const usage = window.limitAmount != null && window.usedAmount != null
    ? safe(t("panel.used", { used: amount(window.usedAmount) ?? "0", limit: amount(window.limitAmount) ?? "0" }))
    : safe(formatWindowDuration(window.windowDurationMins));
  return `<div class="quota-panel quota-panel--${status}">
    <div class="quota-panel__top"><strong>${safe(window.remainingPercent)}<small>${safe(t("panel.leftUnit"))}</small></strong><span>${usage}</span></div>
    <div class="quota-panel__track"><span style="width:${window.remainingPercent}%"></span></div>
    <div class="quota-panel__bottom"><span>${safe(t("panel.resetsAt", { time: formatReset(window.resetsAt, locale) }))}</span></div>
  </div>`;
}

function settingsMarkup(t: Translator): string {
  const s = state.settings;
  const codexPath = s.codexOverride ?? "";
  return `<div class="settings-list">
    <label class="setting-row"><span><strong>${safe(t("settings.alwaysOnTop"))}</strong><small>${safe(t("settings.alwaysOnTop.desc"))}</small></span><input type="checkbox" data-setting="alwaysOnTop" ${s.alwaysOnTop ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>${safe(t("settings.locked"))}</strong><small>${safe(t("settings.locked.desc"))}</small></span><input type="checkbox" data-setting="locked" ${s.locked ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>${safe(t("settings.launchAtLogin"))}</strong><small>${safe(t("settings.launchAtLogin.desc"))}</small></span><input type="checkbox" data-setting="launchAtLogin" ${s.launchAtLogin ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>${safe(t("settings.quotaReminder"))}</strong><small>${safe(t("settings.quotaReminder.desc"))}</small></span><input type="checkbox" data-setting="quotaReminder" ${s.quotaReminder ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>${safe(t("settings.widgetMode"))}</strong><small>${safe(t("settings.widgetMode.desc"))}</small></span><input type="checkbox" data-setting="widgetModeCollapsed" ${s.widgetMode === "collapsed" ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>${safe(t("settings.motion"))}</strong></span><select data-setting="reducedMotion"><option value="system" ${s.reducedMotion === "system" ? "selected" : ""}>${safe(t("settings.motion.system"))}</option><option value="on" ${s.reducedMotion === "on" ? "selected" : ""}>${safe(t("settings.motion.on"))}</option><option value="off" ${s.reducedMotion === "off" ? "selected" : ""}>${safe(t("settings.motion.off"))}</option></select></label>
    <label class="setting-row setting-row--text"><span><strong>${safe(t("settings.refreshVisible"))}</strong><small>${safe(t("settings.refreshVisible.desc"))}</small></span><input type="number" min="15" max="3600" step="5" data-setting="refreshSecondsVisible" value="${safe(s.refreshSecondsVisible)}"><span class="setting-row__suffix">${safe(t("settings.seconds"))}</span></label>
    <label class="setting-row setting-row--text"><span><strong>${safe(t("settings.refreshHidden"))}</strong><small>${safe(t("settings.refreshHidden.desc"))}</small></span><input type="number" min="30" max="3600" step="5" data-setting="refreshSecondsHidden" value="${safe(s.refreshSecondsHidden)}"><span class="setting-row__suffix">${safe(t("settings.seconds"))}</span></label>
    <label class="setting-row"><span><strong>${safe(t("settings.locale"))}</strong></span><select data-setting="locale"><option value="system" ${s.locale === "system" ? "selected" : ""}>${safe(t("settings.locale.system"))}</option><option value="en" ${s.locale === "en" ? "selected" : ""}>${safe(t("settings.locale.en"))}</option><option value="zh-CN" ${s.locale === "zh-CN" ? "selected" : ""}>${safe(t("settings.locale.zh-CN"))}</option></select></label>
    <label class="setting-row setting-row--text"><span><strong>${safe(t("settings.codexPath"))}</strong><small>${safe(t("settings.codexPath.desc"))}</small></span><input type="text" data-setting="codexOverride" placeholder="e.g. C:\\Users\\you\\AppData\\Roaming\\npm\\codex.cmd" value="${safe(codexPath)}" spellcheck="false"></label>
  </div><div class="panel-footer"><span>Quota Critter ${safe(APP_VERSION)}</span><button class="text-button" data-action="toggle-settings">${safe(t("settings.back"))}</button></div>`;
}

function scheduleWidgetResize(): void {
  if (!bridge.isTauri()) return;
  if (resizeFrame != null) window.cancelAnimationFrame(resizeFrame);

  // Wait for the new DOM to be laid out before measuring it. Fixed fallback
  // heights clipped translated text and the settings panel on scaled displays.
  resizeFrame = window.requestAnimationFrame(() => {
    resizeFrame = window.requestAnimationFrame(() => {
      resizeFrame = null;
      const widget = root.querySelector<HTMLElement>(".floating-widget");
      if (!widget) return;

      const contentBottom = Array.from(widget.children).reduce((bottom, child) => {
        if (!(child instanceof HTMLElement)) return bottom;
        const marginBottom = Number.parseFloat(getComputedStyle(child).marginBottom) || 0;
        return Math.max(bottom, child.offsetTop + child.offsetHeight + marginBottom);
      }, 152);
      const nextHeight = Math.ceil(Math.max(152, widget.scrollHeight, contentBottom) + 8);
      const nextWidth = root.querySelector(".expanded-panel") ? 320 : 300;

      if (nextHeight === windowHeight && nextWidth === windowWidth) return;
      windowHeight = nextHeight;
      windowWidth = nextWidth;
      void bridge.setWidgetSize(nextWidth, nextHeight).catch((error) => {
        // Allow the next render to retry a failed native resize.
        windowHeight = 0;
        windowWidth = 0;
        console.warn("Could not resize widget", error);
      });
    });
  });
}

function render(): void {
  const locale = resolveLocale(state.settings.locale);
  const t = makeTranslator(locale);
  const snapshot = state.snapshot ?? (state.preview ? demoSnapshot : null);
  const mainWindow = primaryWindow(snapshot);
  const status = widgetState(snapshot);
  const expression = quotaState(mainWindow, false);
  const staleClass = snapshot?.stale ? " widget--stale" : "";
  const motionClass = state.settings.reducedMotion === "on" ? " reduce-motion" : "";
  const expandedPanel = state.settingsOpen
    ? `<div class="expanded-panel expanded-panel--settings"><div class="panel-heading"><span>${safe(t("settings.title"))}</span><button class="icon-button" data-action="toggle-settings" aria-label="${safe(t("settings.title"))}">×</button></div>${settingsMarkup(t)}</div>`
    : `<div class="expanded-panel"><div class="panel-heading"><span>${safe(t("status.heading"))}</span><button class="icon-button" data-action="toggle-details" aria-label="${safe(t("settings.back"))}">×</button></div>${quotaPanel(mainWindow, snapshot, locale, t)}</div>`;

  // `widgetMode=collapsed` controls the initial presentation; an explicit
  // click must still be able to open the details panel.
  const showExpanded = state.expanded;

  root.innerHTML = `<main class="widget-shell${motionClass}" data-status="${status}">
    <section class="floating-widget${staleClass}" aria-label="Quota Critter">
      <div class="floating-stage" data-drag>
        <button class="lumo-trigger" data-action="toggle-details" aria-label="${state.expanded ? safe(t("settings.back")) : safe(t("status.heading"))}" aria-expanded="${state.expanded}">${lumoMarkup(status, expression)}</button>
        ${orbitMarkup(mainWindow)}
        ${status === "stale" ? offlineIconMarkup() : ""}
      </div>
      ${showExpanded ? expandedPanel : ""}
      ${errorMarkup(state.error, t)}
    </section>
    ${state.preview ? `<div class="preview-badge">${safe(t("preview.badge"))}</div>` : ""}
  </main>`;
  scheduleWidgetResize();
}

async function persistSettings(next: AppSettings): Promise<void> {
  state.settings = next;
  render();
  if (!bridge.isTauri()) return;
  try {
    await bridge.setSettings(next);
    await bridge.setAlwaysOnTop(next.alwaysOnTop);
  } catch (error) {
    const t = makeTranslator(resolveLocale(state.settings.locale));
    state.error = { code: "SETTINGS_WRITE_FAILED", message: t("error.settingsWriteFailed"), action: "retry" };
    console.warn(error);
    render();
  }
}

async function refresh(): Promise<void> {
  if (state.syncing) return;
  if (state.preview) {
    state.snapshot = { ...demoSnapshot, fetchedAt: Date.now(), windows: demoSnapshot.windows.map((item) => ({ ...item })) };
    state.error = null;
    render();
    return;
  }
  const t = makeTranslator(resolveLocale(state.settings.locale));
  state.syncing = true;
  render();
  try {
    state.snapshot = await bridge.refresh();
    state.server = "ready";
    state.auth = "chatgpt";
    state.error = null;
  } catch (error) {
    const code = String(error);
    let action: UserFacingError["action"] = "retry";
    if (code.includes("AUTH_REQUIRED")) action = "login";
    else if (
      code.includes("CODEX_NOT_FOUND") ||
      code.includes("CODEX_CONFIG_FOUND_BUT_CLI_MISSING") ||
      code.includes("CODEX_CONFIGURED_PATH_MISSING") ||
      code.includes("APP_SERVER_START_FAILED")
    ) action = "install";
    state.error = { code, message: pickErrorMessage(code, t), action };
    if (state.snapshot) state.snapshot = { ...state.snapshot, source: "cache", stale: true };
  } finally {
    state.syncing = false;
    render();
  }
}

function wireEvents(): void {
  // WebView treats <img> as a draggable file by default, which produced the
  // ghost PNG copy instead of moving the widget.
  root.addEventListener("dragstart", (event) => event.preventDefault());

  // 全区域可拖动：除了 button/input/select 等交互控件外，任何位置 pointerdown 都启动 Tauri 拖动
  root.addEventListener("pointerdown", (event) => {
    if (state.settings.locked) return;
    const target = event.target as HTMLElement;
    const lumoTrigger = target.closest<HTMLElement>(".lumo-trigger");
    if (lumoTrigger) {
      event.preventDefault();
      const startX = event.screenX;
      const startY = event.screenY;
      const pointerId = event.pointerId;

      const cleanup = () => {
        window.removeEventListener("pointermove", onPointerMove);
        window.removeEventListener("pointerup", cleanup);
        window.removeEventListener("pointercancel", cleanup);
      };
      const onPointerMove = (moveEvent: PointerEvent) => {
        if (moveEvent.pointerId !== pointerId) return;
        if (Math.hypot(moveEvent.screenX - startX, moveEvent.screenY - startY) < 4) return;
        moveEvent.preventDefault();
        suppressClickUntil = performance.now() + 500;
        cleanup();
        void bridge.startDragging();
      };

      window.addEventListener("pointermove", onPointerMove);
      window.addEventListener("pointerup", cleanup);
      window.addEventListener("pointercancel", cleanup);
      return;
    }
    if (target.closest("button, input, select, a")) return;
    // 阻止后续 click 误触发 toggle（如果实际是拖动）
    event.preventDefault();
    void bridge.startDragging();
  });
  // 拦截原生 click：点 button/input/select 之外的任何位置都切换展开/收起
  root.addEventListener("click", (event) => {
    if (performance.now() < suppressClickUntil) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    const target = event.target as HTMLElement;
    const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
    if (action) {
      // 已有具体 action 的元素：走原逻辑
      if (action === "toggle-details") {
        state.expanded = !state.expanded;
        if (!state.expanded) state.settingsOpen = false;
        render();
      } else if (action === "toggle-settings" || action === "open-settings") {
        state.settingsOpen = true;
        state.expanded = true;
        render();
      } else if (action === "retry") {
        void refresh();
      } else if (action === "login") {
        void bridge.login().catch((error) => {
          const t = makeTranslator(resolveLocale(state.settings.locale));
          state.error = { code: "LOGIN_FAILED", message: pickErrorMessage(String(error), t), action: "retry" };
          render();
        });
      } else if (action === "install") {
        void bridge.openUrl("https://learn.chatgpt.com/docs/codex/ide").catch((error) => {
          const t = makeTranslator(resolveLocale(state.settings.locale));
          state.error = { code: "OPEN_BROWSER_FAILED", message: pickErrorMessage(String(error), t), action: "retry" };
          render();
        });
      }
      return;
    }
    // 没有 data-action 的区域：点空白处也切换展开/收起
    if (target.closest("button, input, select, a")) return;
    state.expanded = !state.expanded;
    if (!state.expanded) state.settingsOpen = false;
    render();
  });
  root.addEventListener("change", (event) => {
    const input = event.target as HTMLInputElement | HTMLSelectElement;
    const key = input.dataset.setting as keyof AppSettings | "widgetModeCollapsed" | undefined;
    if (!key) return;
    const next = { ...state.settings };
    if (input instanceof HTMLInputElement) {
      if (input.type === "checkbox") {
        if (key === "widgetModeCollapsed") {
          next.widgetMode = input.checked ? "collapsed" : "default";
        } else {
          (next as Record<string, unknown>)[key as string] = input.checked;
        }
      } else if (input.type === "text") {
        if (key === "codexOverride") {
          const value = input.value.trim();
          if (value.length === 0) {
            delete next.codexOverride;
          } else {
            next.codexOverride = value;
          }
        }
      } else if (input.type === "number") {
        const num = Math.max(5, Math.min(3600, Number(input.value) || 60));
        (next as Record<string, unknown>)[key as string] = num;
      }
    } else if (input instanceof HTMLSelectElement) {
      if (key === "reducedMotion") {
        next.reducedMotion = input.value as AppSettings["reducedMotion"];
      } else if (key === "locale") {
        next.locale = input.value as AppSettings["locale"];
      }
    }
    void persistSettings(next);
  });
}

async function start(): Promise<void> {
  wireEvents();
  if (!bridge.isTauri()) {
    state.preview = true;
    state.server = "ready";
    state.auth = "chatgpt";
    state.snapshot = browserPreviewSnapshot();
    render();
    return;
  }

  try {
    state.settings = { ...defaultSettings, ...(await bridge.getSettings()) };
  } catch (error) {
    console.warn("Settings unavailable", error);
  }
  try {
    state.snapshot = await bridge.getSnapshot();
  } catch (error) {
    console.warn("Cached quota unavailable", error);
  }
  render();
  await Promise.all([
    bridge.listenSnapshot((snapshot) => {
      state.snapshot = snapshot;
      state.server = "ready";
      state.error = null;
      render();
    }),
    bridge.listenServer((server) => {
      state.server = server;
      if (server === "error" && !state.snapshot) {
        // 启动阶段就 fail，触发一次 refresh 拿到具体错误码（CODEX_NOT_FOUND / CODEX_CONFIG_FOUND_BUT_CLI_MISSING 等）
        void refresh();
      }
      render();
    }),
    bridge.listenAuth((auth) => {
      state.auth = auth as AuthState;
      render();
    }),
    bridge.listenRateLimitsUpdated(() => {
      void refresh();
    }),
    bridge.listenOpenSettings(() => {
      state.expanded = true;
      state.settingsOpen = true;
      render();
    }),
  ]);
  await refresh();
}

void start();
