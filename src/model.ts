export type QuotaState = "full" | "steady" | "low" | "exhausted" | "stale" | "unknown";

export type QuotaWindow = {
  key: string;
  limitId: string;
  limitName?: string;
  kind: "individual" | "primary" | "secondary";
  usedPercent: number;
  remainingPercent: number;
  windowDurationMins: number;
  resetsAt: number;
  planType?: string;
  limitAmount?: number;
  usedAmount?: number;
};

export type QuotaSnapshot = {
  windows: QuotaWindow[];
  fetchedAt: number;
  source: "live" | "cache";
  stale: boolean;
};

export type AppSettings = {
  schemaVersion: 1;
  alwaysOnTop: boolean;
  locked: boolean;
  launchAtLogin: boolean;
  reducedMotion: "system" | "on" | "off";
  widgetMode: "default" | "collapsed";
  refreshSecondsVisible: number;
  refreshSecondsHidden: number;
  locale: "system" | "en" | "zh-CN";
  windowPlacement?: { monitorId?: string; x: number; y: number };
};

export type AppServerState = "starting" | "handshaking" | "ready" | "backoff" | "error";
export type AuthState = "unknown" | "signedOut" | "chatgpt" | "unsupported";

export type UserFacingError = {
  code: string;
  message: string;
  action?: "retry" | "login" | "install";
};

export const defaultSettings: AppSettings = {
  schemaVersion: 1,
  alwaysOnTop: true,
  locked: false,
  launchAtLogin: false,
  reducedMotion: "system",
  widgetMode: "default",
  refreshSecondsVisible: 60,
  refreshSecondsHidden: 300,
  locale: "system",
};

export const demoSnapshot: QuotaSnapshot = {
  fetchedAt: Date.now(),
  source: "live",
  stale: false,
  windows: [
    {
      key: "codex:primary",
      limitId: "codex",
      kind: "primary",
      limitName: "Codex",
      usedPercent: 36,
      remainingPercent: 64,
      windowDurationMins: 300,
      resetsAt: Date.now() + 2 * 60 * 60 * 1000 + 14 * 60 * 1000,
    },
    {
      key: "codex:secondary",
      limitId: "codex",
      kind: "secondary",
      limitName: "Weekly",
      usedPercent: 59,
      remainingPercent: 41,
      windowDurationMins: 10080,
      resetsAt: Date.now() + 4 * 24 * 60 * 60 * 1000 + 8 * 60 * 60 * 1000,
    },
  ],
};

export function clamp(value: number, min = 0, max = 100): number {
  return Math.min(max, Math.max(min, Math.round(value)));
}

export function quotaState(window: QuotaWindow | undefined, stale = false): QuotaState {
  if (!window) return "unknown";
  if (stale) return "stale";
  if (window.remainingPercent <= 0) return "exhausted";
  if (window.remainingPercent < 20) return "low";
  if (window.remainingPercent >= 80) return "full";
  return "steady";
}

export function formatWindowDuration(minutes: number): string {
  if (minutes <= 0) return "Usage limit";
  if (minutes >= 10080) return "Weekly";
  if (minutes >= 1440) return `${Math.round(minutes / 1440)}d window`;
  if (minutes >= 60) return `${Math.round(minutes / 60)}h window`;
  return `${minutes}m window`;
}

export function formatCountdown(timestamp: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor((timestamp - now) / 1000));
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function formatReset(timestamp: number, locale = "en-US"): string {
  return new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }).format(timestamp);
}

export function formatSynced(timestamp: number, now = Date.now()): string {
  const seconds = Math.max(0, Math.floor((now - timestamp) / 1000));
  if (seconds < 10) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  return `${Math.floor(minutes / 60)}h ago`;
}
