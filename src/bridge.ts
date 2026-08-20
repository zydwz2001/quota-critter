import type { AppSettings, AppServerState, QuotaSnapshot } from "./model";

type Unlisten = () => void;

let tauriLoaded: Promise<{
  invoke: typeof import("@tauri-apps/api/core").invoke;
  listen: typeof import("@tauri-apps/api/event").listen;
  getCurrentWindow: typeof import("@tauri-apps/api/window").getCurrentWindow;
}> | null = null;

function isTauriRuntime(): boolean {
  const candidate = window as unknown as Record<string, unknown>;
  return Boolean(candidate.__TAURI_INTERNALS__ || candidate.__TAURI__);
}

async function loadTauri() {
  if (!tauriLoaded) {
    tauriLoaded = Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
      import("@tauri-apps/api/window"),
    ]).then(([core, event, windowApi]) => ({
      invoke: core.invoke,
      listen: event.listen,
      getCurrentWindow: windowApi.getCurrentWindow,
    }));
  }
  return tauriLoaded;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) throw new Error("TAURI_UNAVAILABLE");
  const tauri = await loadTauri();
  return tauri.invoke<T>(command, args);
}

async function listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!isTauriRuntime()) return () => undefined;
  const tauri = await loadTauri();
  return tauri.listen<T>(event, (eventPayload) => handler(eventPayload.payload));
}

export const bridge = {
  isTauri: isTauriRuntime,
  getSettings: () => invoke<AppSettings>("get_settings"),
  getSnapshot: () => invoke<QuotaSnapshot | null>("get_quota_snapshot"),
  refresh: () => invoke<QuotaSnapshot>("refresh_quota"),
  login: () => invoke<void>("start_login"),
  setSettings: (settings: AppSettings) => invoke<AppSettings>("set_settings", { settings }),
  setAlwaysOnTop: (value: boolean) => invoke<void>("set_always_on_top", { value }),
  setWidgetSize: (width: number, height: number) => invoke<void>("set_widget_size", { width, height }),
  listenSnapshot: (handler: (snapshot: QuotaSnapshot) => void) => listen("quota://snapshot", handler),
  listenServer: (handler: (state: AppServerState) => void) => listen("app-server://state", handler),
  listenAuth: (handler: (state: string) => void) => listen("account://state", handler),
  listenRateLimitsUpdated: (handler: () => void) => listen("account://rate-limits-updated", handler),
  listenOpenSettings: (handler: () => void) => listen("ui://open-settings", handler),
  startDragging: async () => {
    if (!isTauriRuntime()) return;
    const tauri = await loadTauri();
    await tauri.getCurrentWindow().startDragging();
  },
  openUrl: (url: string) => invoke<void>("open_external_url", { url }),
};
