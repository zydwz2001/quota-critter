// 极简 i18n：根据 settings.locale 解析成 "en" / "zh-CN"，渲染时由 t(key) 取文案。
// 翻译 key 用点分命名空间（settings.* / error.* / panel.* / action.* / state.* / preview.*）。
// 文案缺失时回退英文，再缺失则回退 key 本身。

export type LocaleSetting = "system" | "en" | "zh-CN";
export type ResolvedLocale = "en" | "zh-CN";

type Dict = Record<string, string>;

const en: Dict = {
  "status.heading": "ACCOUNT QUOTA",
  "status.stale": "Showing last known quota",
  "status.updates": "Updates automatically",

  "panel.empty": "Waiting for quota",
  "panel.used": "Used {used} / {limit}",
  "panel.used.noLimit": "Resets",
  "panel.leftUnit": "% left",
  "panel.resetsAt": "Resets on {time}",
  "panel.updated": "Updated {time}",
  "panel.window.daily": "{n}h window",
  "panel.window.weekly": "Weekly",
  "panel.window.days": "{n}d window",
  "panel.window.minutes": "{n}m window",
  "panel.window.usage": "Usage limit",

  "settings.title": "SETTINGS",
  "settings.alwaysOnTop": "Always on top",
  "settings.alwaysOnTop.desc": "Keep Lumo above other windows",
  "settings.locked": "Lock position",
  "settings.locked.desc": "Disable accidental dragging",
  "settings.launchAtLogin": "Launch at login",
  "settings.launchAtLogin.desc": "Start with your desktop",
  "settings.quotaReminder": "Low-quota reminder",
  "settings.quotaReminder.desc": "Notify when remaining drops below 20%",
  "settings.motion": "Motion",
  "settings.motion.system": "System",
  "settings.motion.on": "On",
  "settings.motion.off": "Off",
  "settings.refreshVisible": "Refresh when visible",
  "settings.refreshVisible.desc": "Interval in seconds while widget is on screen",
  "settings.refreshHidden": "Refresh when hidden",
  "settings.refreshHidden.desc": "Interval in seconds while widget is hidden",
  "settings.seconds": "s",
  "settings.locale": "Language",
  "settings.locale.system": "System",
  "settings.locale.en": "English",
  "settings.locale.zh-CN": "简体中文",
  "settings.widgetMode": "Collapsed by default",
  "settings.widgetMode.desc": "Hide numbers and details until you click Lumo",
  "settings.codexPath": "Codex CLI path",
  "settings.codexPath.desc": "Leave empty to detect Codex desktop, CLI, or IDE extensions automatically.",
  "settings.back": "← Back",

  "action.retry": "Retry",
  "action.login": "Sign in",
  "action.install": "Get Codex",
  "action.setPath": "Set path…",

  "error.authRequired": "Sign in with ChatGPT to see your Codex quota.",
  "error.authUnsupported": "Your account type isn't supported yet.",
  "error.codexNotFound": "No local Codex runtime found. Install and sign in to Codex desktop, CLI, or the VS Code extension.",
  "error.codexConfigFoundButCliMissing": "Your Codex login was found, but no client or IDE runtime is available. Install Codex or its VS Code extension.",
  "error.codexConfiguredPathMissing": "The saved Codex path no longer exists, and automatic detection found no replacement.",
  "error.rateLimitsEmpty": "No quota data available.",
  "error.requestTimeout": "Codex is taking too long. Try again.",
  "error.requestFailed": "Offline — showing last known quota.",
  "error.authUrlMissing": "Login service returned no URL. Try again.",
  "error.unsafeUrl": "Refused to open a non-https URL.",
  "error.openBrowserFailed": "Could not open the install page.",
  "error.loginFailed": "Could not open the ChatGPT login flow. Try again from the tray menu.",
  "error.settingsWriteFailed": "Could not save settings. Try again.",

  "preview.badge": "browser preview · click Lumo",

  "state.full": "Healthy",
  "state.steady": "Steady",
  "state.low": "Low quota",
  "state.exhausted": "Quota exhausted",
  "state.stale": "Stale",
  "state.unknown": "Unknown",
};

const zhCN: Dict = {
  "status.heading": "账户额度",
  "status.stale": "显示最近一次额度",
  "status.updates": "自动更新",

  "panel.empty": "等待额度数据",
  "panel.used": "已用 {used} / {limit}",
  "panel.used.noLimit": "重置于",
  "panel.leftUnit": "% 剩余",
  "panel.resetsAt": "将于 {time} 重置",
  "panel.updated": "更新于 {time}",
  "panel.window.daily": "{n} 小时窗口",
  "panel.window.weekly": "周窗口",
  "panel.window.days": "{n} 天窗口",
  "panel.window.minutes": "{n} 分钟窗口",
  "panel.window.usage": "用量上限",

  "settings.title": "设置",
  "settings.alwaysOnTop": "始终置顶",
  "settings.alwaysOnTop.desc": "让 Lumo 永远显示在窗口之上",
  "settings.locked": "锁定位置",
  "settings.locked.desc": "防止误拖动",
  "settings.launchAtLogin": "开机启动",
  "settings.launchAtLogin.desc": "随系统一起启动",
  "settings.quotaReminder": "额度提醒",
  "settings.quotaReminder.desc": "额度低于 20% 时提醒",
  "settings.motion": "动效",
  "settings.motion.system": "跟随系统",
  "settings.motion.on": "开启",
  "settings.motion.off": "关闭",
  "settings.refreshVisible": "前台刷新（秒）",
  "settings.refreshVisible.desc": "窗口可见时的刷新间隔（秒）",
  "settings.refreshHidden": "后台刷新（秒）",
  "settings.refreshHidden.desc": "窗口隐藏时的刷新间隔（秒）",
  "settings.seconds": "秒",
  "settings.locale": "语言",
  "settings.locale.system": "跟随系统",
  "settings.locale.en": "English",
  "settings.locale.zh-CN": "简体中文",
  "settings.widgetMode": "默认收起",
  "settings.widgetMode.desc": "收起时不显示数字和重置时间，点击 Lumo 展开",
  "settings.codexPath": "Codex CLI 路径",
  "settings.codexPath.desc": "留空时自动查找 Codex 客户端、CLI 或 VS Code 插件。",
  "settings.back": "← 返回",

  "action.retry": "重试",
  "action.login": "登录",
  "action.install": "获取 Codex",
  "action.setPath": "设置路径…",

  "error.authRequired": "请用 ChatGPT 登录以查看你的 Codex 额度。",
  "error.authUnsupported": "当前账号类型暂不支持。",
  "error.codexNotFound": "找不到本机 Codex 运行组件，请安装并登录 Codex 客户端、CLI 或 VS Code 插件。",
  "error.codexConfigFoundButCliMissing": "检测到 Codex 登录态，但找不到客户端或 IDE 插件中的运行组件。请安装 Codex 或 VS Code 插件。",
  "error.codexConfiguredPathMissing": "已保存的 Codex 路径失效，自动查找也没有找到可用组件。",
  "error.rateLimitsEmpty": "暂无额度数据。",
  "error.requestTimeout": "Codex 响应超时，请重试。",
  "error.requestFailed": "离线中 —— 显示最近一次额度。",
  "error.authUrlMissing": "登录服务没有返回 URL，请重试。",
  "error.unsafeUrl": "拒绝打开非 https 的链接。",
  "error.openBrowserFailed": "无法打开安装页面。",
  "error.loginFailed": "无法打开 ChatGPT 登录流程，请从托盘菜单重试。",
  "error.settingsWriteFailed": "无法保存设置，请重试。",

  "preview.badge": "浏览器预览 · 点击 Lumo",

  "state.full": "充足",
  "state.steady": "正常",
  "state.low": "额度偏低",
  "state.exhausted": "额度已耗尽",
  "state.stale": "已过期",
  "state.unknown": "未知",
};

const dictionaries: Record<ResolvedLocale, Dict> = { en, "zh-CN": zhCN };

export function detectSystemLocale(): ResolvedLocale {
  if (typeof navigator !== "undefined" && navigator.language) {
    return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
  }
  return "en";
}

export function resolveLocale(setting: LocaleSetting): ResolvedLocale {
  if (setting === "system") return detectSystemLocale();
  return setting;
}

export type Translator = (key: string, vars?: Record<string, string | number>) => string;

export function makeTranslator(locale: ResolvedLocale): Translator {
  const primary = dictionaries[locale] ?? en;
  return (key, vars) => {
    let template = primary[key] ?? en[key] ?? key;
    if (vars) {
      for (const [k, v] of Object.entries(vars)) {
        template = template.split(`{${k}}`).join(String(v));
      }
    }
    return template;
  };
}
