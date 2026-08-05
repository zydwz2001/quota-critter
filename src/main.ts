import "./styles.css";
import lumoSprite from "./assets/lumo-sprite.png";
import { bridge } from "./bridge";
import {
  defaultSettings,
  demoSnapshot,
  formatCountdown,
  formatReset,
  formatSynced,
  formatWindowDuration,
  quotaState,
  type AppServerState,
  type AppSettings,
  type AuthState,
  type QuotaSnapshot,
  type QuotaWindow,
  type UserFacingError,
} from "./model";

const appRoot = document.querySelector<HTMLElement>("#app");
if (!appRoot) throw new Error("Quota Critter root element is missing");
const root: HTMLElement = appRoot;

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

let refreshTimer: number | undefined;
let windowHeight = 152;

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

function lumoMarkup(status: ReturnType<typeof quotaState>): string {
  return `<img class="lumo lumo--${status}" src="${lumoSprite}" alt="Lumo ${status}" />`;
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
    return `<span class="${className}" style="--angle:${angle}deg"></span>`;
  }).join("");
  return `<div class="orbit" aria-label="Quota shown by orbit activity">
    <div class="orbit-arc"></div>${nodes}
  </div>`;
}

function widgetState(snapshot: QuotaSnapshot | null): ReturnType<typeof quotaState> {
  return quotaState(primaryWindow(snapshot), snapshot?.stale ?? false);
}

function errorMarkup(error: UserFacingError | null): string {
  if (!error) return "";
  const action = error.action === "login"
    ? `<button class="error-action" data-action="login">Sign in</button>`
    : error.action === "retry"
      ? `<button class="error-action" data-action="retry">Retry</button>`
      : "";
  return `<div class="error-banner" role="alert"><span class="error-dot"></span><span>${safe(error.message)}</span>${action}</div>`;
}

function amount(value: number | undefined): string {
  return value == null ? "" : Math.round(value).toLocaleString();
}

function quotaPanel(window: QuotaWindow | undefined, snapshot: QuotaSnapshot | null): string {
  if (!window) return `<div class="empty-state">Waiting for quota</div>`;
  const status = quotaState(window, snapshot?.stale ?? false);
  const usage = window.limitAmount != null && window.usedAmount != null
    ? `Used ${amount(window.usedAmount)} / ${amount(window.limitAmount)}`
    : formatWindowDuration(window.windowDurationMins);
  return `<div class="quota-panel quota-panel--${status}">
    <div class="quota-panel__top"><strong>${safe(window.remainingPercent)}<small>% left</small></strong><span>${safe(usage)}</span></div>
    <div class="quota-panel__track"><span style="width:${window.remainingPercent}%"></span></div>
    <div class="quota-panel__bottom"><span>Resets ${safe(formatCountdown(window.resetsAt))}</span><span>${safe(formatReset(window.resetsAt))}</span></div>
  </div>`;
}

function settingsMarkup(): string {
  const motion = state.settings.reducedMotion;
  return `<div class="settings-list">
    <label class="setting-row"><span><strong>Always on top</strong><small>Keep Lumo above other windows</small></span><input type="checkbox" data-setting="alwaysOnTop" ${state.settings.alwaysOnTop ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>Lock position</strong><small>Disable accidental dragging</small></span><input type="checkbox" data-setting="locked" ${state.settings.locked ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>Launch at login</strong><small>Start with your desktop</small></span><input type="checkbox" data-setting="launchAtLogin" ${state.settings.launchAtLogin ? "checked" : ""}></label>
    <label class="setting-row"><span><strong>Motion</strong><small>Respect system reduced motion</small></span><select data-setting="reducedMotion"><option value="system" ${motion === "system" ? "selected" : ""}>System</option><option value="on" ${motion === "on" ? "selected" : ""}>On</option><option value="off" ${motion === "off" ? "selected" : ""}>Off</option></select></label>
  </div><div class="panel-footer"><span>Quota Critter 0.1.0</span><button class="text-button" data-action="toggle-settings">← Back</button></div>`;
}

function render(): void {
  const snapshot = state.snapshot ?? (state.preview ? demoSnapshot : null);
  const mainWindow = primaryWindow(snapshot);
  const status = widgetState(snapshot);
  const staleClass = snapshot?.stale ? " widget--stale" : "";
  const motionClass = state.settings.reducedMotion === "on" ? " reduce-motion" : "";
  const expandedPanel = state.settingsOpen
    ? `<div class="expanded-panel expanded-panel--settings"><div class="panel-heading"><span>SETTINGS</span><button class="icon-button" data-action="toggle-settings" aria-label="Close settings">×</button></div>${settingsMarkup()}</div>`
    : `<div class="expanded-panel"><div class="panel-heading"><span>ACCOUNT QUOTA</span><button class="icon-button" data-action="toggle-details" aria-label="Collapse quota">×</button></div>${quotaPanel(mainWindow, snapshot)}<div class="panel-meta"><span>${snapshot?.stale ? "Showing last known quota" : "Updates automatically"}</span>${snapshot ? ` <span>Updated ${safe(formatSynced(snapshot.fetchedAt))}</span>` : ""}</div></div>`;

  root.innerHTML = `<main class="widget-shell${motionClass}" data-status="${status}">
    <section class="floating-widget${staleClass}" aria-label="Quota Critter">
      <div class="floating-stage" data-drag>
        <button class="lumo-trigger" data-action="toggle-details" aria-label="${state.expanded ? "Collapse quota details" : "Show quota details"}" aria-expanded="${state.expanded}">${lumoMarkup(status)}</button>
        ${orbitMarkup(mainWindow)}
      </div>
      ${state.expanded ? expandedPanel : ""}
      ${errorMarkup(state.error)}
    </section>
    ${state.preview ? `<div class="preview-badge">browser preview · click Lumo</div>` : ""}
  </main>`;

  if (bridge.isTauri()) {
    const nextHeight = state.expanded ? (state.settingsOpen ? 380 : 292) : 152;
    if (nextHeight !== windowHeight) {
      windowHeight = nextHeight;
      void bridge.setWidgetHeight(nextHeight);
    }
  }
}

async function persistSettings(next: AppSettings): Promise<void> {
  state.settings = next;
  render();
  if (!bridge.isTauri()) return;
  try {
    await bridge.setSettings(next);
    await bridge.setAlwaysOnTop(next.alwaysOnTop);
  } catch (error) {
    state.error = { code: "SETTINGS_WRITE_FAILED", message: "Could not save settings. Try again.", action: "retry" };
    console.warn(error);
    render();
  }
}

async function refresh(): Promise<void> {
  if (state.preview) {
    state.snapshot = { ...demoSnapshot, fetchedAt: Date.now(), windows: demoSnapshot.windows.map((item) => ({ ...item })) };
    state.error = null;
    render();
    return;
  }
  state.syncing = true;
  render();
  try {
    state.snapshot = await bridge.refresh();
    state.server = "ready";
    state.auth = "chatgpt";
    state.error = null;
  } catch (error) {
    const code = String(error);
    state.error = code.includes("AUTH_REQUIRED")
      ? { code: "AUTH_REQUIRED", message: "Sign in with ChatGPT to see your Codex quota.", action: "login" }
      : { code: "REQUEST_FAILED", message: "Offline — showing last known quota.", action: "retry" };
    if (state.snapshot) state.snapshot = { ...state.snapshot, source: "cache", stale: true };
  } finally {
    state.syncing = false;
    render();
  }
}

function scheduleRefresh(): void {
  if (refreshTimer) window.clearInterval(refreshTimer);
  refreshTimer = window.setInterval(() => void refresh(), state.settings.refreshSecondsVisible * 1000);
}

function wireEvents(): void {
  root.addEventListener("pointerdown", (event) => {
    const target = event.target as HTMLElement;
    if (state.settings.locked || target.closest("button, input, select")) return;
    if (target.closest("[data-drag]")) void bridge.startDragging();
  });
  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
    if (!action) return;
    if (action === "toggle-details") {
      state.expanded = !state.expanded;
      if (!state.expanded) state.settingsOpen = false;
      render();
    } else if (action === "toggle-settings") {
      state.settingsOpen = !state.settingsOpen;
      state.expanded = true;
      render();
    } else if (action === "retry") {
      void refresh();
    } else if (action === "login") {
      void bridge.login().catch(() => {
        state.error = { code: "LOGIN_FAILED", message: "Could not open the ChatGPT login flow. Try again from the tray menu.", action: "retry" };
        render();
      });
    }
  });
  root.addEventListener("change", (event) => {
    const input = event.target as HTMLInputElement | HTMLSelectElement;
    const key = input.dataset.setting as keyof AppSettings | undefined;
    if (!key) return;
    const next = { ...state.settings };
    if (input instanceof HTMLInputElement) {
      (next[key] as boolean) = input.checked;
    } else if (key === "reducedMotion") {
      next.reducedMotion = input.value as AppSettings["reducedMotion"];
    }
    void persistSettings(next);
  });
}

async function start(): Promise<void> {
  wireEvents();
  window.setInterval(() => render(), 1000);
  if (!bridge.isTauri()) {
    state.preview = true;
    state.server = "ready";
    state.auth = "chatgpt";
    state.snapshot = demoSnapshot;
    render();
    scheduleRefresh();
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
        state.error = { code: "CODEX_NOT_FOUND", message: "Codex CLI unavailable. Install or sign in to Codex, then retry.", action: "retry" };
      }
      render();
    }),
    bridge.listenAuth((auth) => {
      state.auth = auth as AuthState;
      render();
    }),
    bridge.listenOpenSettings(() => {
      state.expanded = true;
      state.settingsOpen = true;
      render();
    }),
  ]);
  scheduleRefresh();
  await refresh();
}

void start();
