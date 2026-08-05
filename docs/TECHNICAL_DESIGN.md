# Quota Critter 技术方案

| 字段 | 内容 |
| --- | --- |
| 版本 | v0.1-draft |
| 日期 | 2026-08-05 |
| 状态 | 可进入技术验证 |

## 1. 技术结论

采用 **Tauri 2 + Rust 后端 + Vanilla TypeScript/CSS 前端 + 固定版本 Codex App Server sidecar**。

- App Server 官方账户接口读取额度，不抓网页、不解析私有接口；
- stdio JSONL 通信，不开放 TCP/WebSocket 端口；
- 开发阶段优先系统 `codex`，发布包内置固定版本；
- 前端只接收清洗后的领域模型；
- v0.1 不启动线程或模型调用。

> 稳定性边界：官方文档当前把 `app-server` 命令标记为实验性、非受支持的生产接口。Quota Critter 属于开源实验性工具，Release 必须固定 sidecar 与 Schema，Codex 升级必须先通过 Fixture 与双平台冒烟测试。

官方参考：[Codex App Server](https://learn.chatgpt.com/docs/app-server)、[Authentication](https://learn.chatgpt.com/docs/auth)、[Tauri 2](https://v2.tauri.app/start/)、[System Tray](https://v2.tauri.app/learn/system-tray/)、[Updater](https://v2.tauri.app/plugin/updater/)。

## 2. 选型

Tauri 使用系统 WebView，适合轻量托盘应用；Rust 适合子进程、stdio、状态机和窗口。相比 Electron，不捆绑完整 Chromium；相比双原生 UI，维护成本更低。

## 3. Sidecar 策略

开发查找顺序：`QUOTA_CRITTER_CODEX_PATH` → `PATH` 中的 `codex` → 开发配置绝对路径 → 安装指导。

Beta/Release 使用 Tauri `externalBin` 固定 Codex 二进制，每个平台/架构独立构建，保留 Apache-2.0 LICENSE/NOTICE，记录 Quota Critter 版本、Codex 版本和 schema hash。

## 4. 架构

```text
Codex App / VS Code Extension
          ↓ 同一账户额度
Tauri WebView UI ← Tauri events/commands ← Rust Core
                                      ├─ AppServerSupervisor
                                      ├─ JsonRpcClient
                                      ├─ AuthService
                                      ├─ QuotaService
                                      ├─ RefreshScheduler
                                      ├─ Window/Tray Manager
                                      └─ Redacted Logger
                                                    ↓ stdio JSONL
                                             codex app-server
```

## 5. 仓库结构

```text
quota-critter/
├─ src/                         # Vanilla TS/CSS UI
│  ├─ main.ts
│  ├─ bridge.ts
│  ├─ model.ts
│  └─ styles.css
├─ src-tauri/
│  ├─ src/
│  │  ├─ lib.rs
│  │  ├─ app_server.rs
│  │  ├─ quota.rs
│  │  └─ settings.rs
│  ├─ binaries/                 # 发布用 sidecar
│  ├─ capabilities/
│  └─ tauri.conf.json
├─ schemas/
├─ tests/fixtures/app-server/
├─ docs/
├─ assets/concepts/
└─ .github/workflows/
```

## 6. App Server 协议

启动：

```text
codex app-server --listen stdio://
```

stdio 是一行一个 JSON 消息，wire 上省略 `jsonrpc` 字段。Rust 必须分开读取 stdout/stderr，限制单行长度，解析失败时只记录脱敏结构，使用单调请求 ID 和超时。

连接建立后先发送：

```json
{"method":"initialize","id":0,"params":{"clientInfo":{"name":"quota_critter","title":"Quota Critter","version":"0.1.1"}}}
```

成功响应后发送：

```json
{"method":"initialized","params":{}}
```

v0.1 不启用 `experimentalApi`。无需 `thread/start`，不得因获取额度触发模型调用。

### 初始读取顺序

```text
spawn -> initialize -> initialized -> account/read
      -> chatgpt account: account/rateLimits/read
      -> signed out: show login -> completed: read again
```

### 登录

```json
{"method":"account/login/start","id":2,"params":{"type":"chatgpt","useHostedLoginSuccessPage":true,"appBrand":"codex"}}
```

将 `authUrl` 交给系统浏览器，等待 `account/login/completed` 与 `account/updated`，成功后重新读取账户和额度。不得把 URL 写入长期日志，不接受前端传入 access token，不复制 `auth.json`。

### 额度兼容

1. 存在 `individualLimit` 时优先展示账户真实使用限额（`limit`、`used`、`remainingPercent`、`resetsAt`）；
2. 存在 `rateLimitsByLimitId` 时遍历所有桶；
3. 否则使用 `rateLimits` 单桶；
4. 没有 `individualLimit` 时再解析 `primary`/`secondary` 时间窗；
5. 未知 `limitId` 保留，不因名称未知丢弃；
6. 可选字段使用 `Option`。

领域模型：

```ts
type QuotaWindow = {
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

type QuotaSnapshot = {
  windows: QuotaWindow[];
  fetchedAt: number;
  source: "live" | "cache";
  stale: boolean;
};
```

`remainingPercent` 统一由 Rust 计算并夹在 0–100，前端不得重复推导。

### Schema

固定版本后运行：

```text
codex app-server generate-ts --out ./schemas/typescript
codex app-server generate-json-schema --out ./schemas/json
```

Schema 与 sidecar 一起提交，CI 检查生成结果，升级前先跑协议 Fixture。

## 7. 刷新与状态机

- 启动立即读取；
- 可见时每 60 秒；隐藏到托盘时每 5 分钟；
- 收到 `account/rateLimits/updated` 后去抖并完整读取；
- 手动刷新 5 秒内最多一次；
- 睡眠唤醒/网络恢复延迟 1–3 秒随机抖动。

失败退避：`5s -> 15s -> 30s -> 60s -> 5m`，加入 0–20% jitter。

sidecar 状态：`Stopped -> Starting -> Handshaking -> Ready -> RestartBackoff -> FailedPermanent`。意外退出最多重启 3 次，稳定运行 10 分钟后重置计数，主动退出不重启。

## 8. 缓存与设置

只缓存清洗后的额度快照，不缓存 Token、账户 ID、会话正文或原始 RPC。缓存超过 24 小时后 UI 只显示“数据过期”。

```ts
type AppSettings = {
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
```

设置写入必须原子化，新字段提供迁移默认值。

## 9. Tauri 窗口与托盘

- 无装饰、透明、不可调整大小、可置顶、跳过任务栏；
- Windows 使用 `skipTaskbar`；macOS 使用 accessory activation policy 隐藏 Dock，只保留菜单栏托盘；
- 关闭挂件等同隐藏，不等同退出；
- 使用 System Tray 和 single-instance；
- 详情优先同窗口模式切换，设置页可独立窗口；
- 挂件默认不抢焦点；
- 显示器拔出后迁移到主显示器可见区域。

## 10. 前端事件与错误

Rust 只向前端发送：`quota://snapshot`、`quota://sync-state`、`account://state`、`app-server://state`、`settings://updated`。前端不能调用任意 shell。

稳定错误码：`CODEX_NOT_FOUND`、`APP_SERVER_START_FAILED`、`APP_SERVER_INCOMPATIBLE`、`AUTH_REQUIRED`、`AUTH_UNSUPPORTED`、`NETWORK_OFFLINE`、`RATE_LIMITS_EMPTY`、`REQUEST_TIMEOUT`。UI 不展示 Rust 堆栈或原始错误。

## 11. 测试

Rust 单元测试覆盖百分比边界、单/多桶、primary/secondary 缺失、重置判断、退避、脱敏和设置迁移。协议 Fixture 覆盖 ChatGPT 登录、未登录、API Key-only、空额度、通知、错误和 sidecar 退出。前端覆盖所有 Lumo 状态、收起、离线、DPI、reduced motion 和键盘访问。

跨平台手测：macOS Apple Silicon、Windows 10/11 x64、单/多显示器、睡眠唤醒、网络恢复、Codex 桌面端与 VS Code 同时运行、开机启动、卸载和安装警告。

## 12. 发布与风险

GitHub Actions 构建 macOS/Windows，sidecar 校验 SHA-256，上传安装包、校验和、更新清单及第三方许可。正式公开下载应配置 macOS Developer ID/notarization 与 Windows 代码签名，更新器使用签名包。

| 风险 | 应对 |
| --- | --- |
| App Server 实验性且协议可能变化 | 固定 sidecar、Schema、Fixture；上游变化时暂停升级 |
| 认证模式不返回额度 | v0.1 明确只支持 ChatGPT Codex 额度 |
| 登录缓存无法复用 | 使用官方 App Server 登录，不直接读凭据 |
| Windows 透明窗口差异 | M1 先双平台 Spike |
| 高频刷新占资源 | 通知优先、可见/隐藏分级轮询、睡眠感知 |
| 用户误读 used/remaining | Rust 统一转换，UI 永远显示 left/剩余 |

## 13. 开发里程碑

- M0（0.5–1 天）：系统 Codex 启动 App Server，完成握手与额度领域模型；
- M1（1 天）：Tauri 窗口、托盘、拖动、位置保存和假数据 UI；
- M2（1 天）：认证、真实额度、缓存、离线和重试；
- M3（1–2 天）：正式 Sprite、状态动画、详情、设置和测试；
- M4（1–2 天，不含证书等待）：sidecar、CI、安装包、README、签名和更新器。
