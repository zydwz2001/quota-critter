# Quota Critter

> A tiny cross-platform quota companion for Codex.

Quota Critter 是一个常驻系统托盘、可悬浮在桌面上的 Codex 额度提示工具。它通过官方 Codex App Server 获取账户额度，让用户无论在 Codex 桌面端还是 VS Code Codex 扩展中工作，都能随时看到剩余额度和重置时间。

当前项目为 v0.1 预览版，视觉概念图和规划文档已保留在仓库中。

## 安装下载

macOS DMG 和 Windows x64 NSIS 安装包都在 [v0.1.0 Release](https://github.com/zydwz2001/quota-critter/releases/tag/v0.1.0)。首次运行前请先安装并登录 Codex CLI。

## 已确定的产品方向

- 产品名：**Quota Critter**
- 默认角色：**Lumo**，原创紫蓝色像素星灵
- 视觉定位：午夜靛蓝 + 薄荷绿 + 长春花紫 + 珊瑚红
- 主形态：像素伙伴 + 额度轨道
- 收起形态：星灵 + 斜向半弧轨道；点击星灵后展开详情
- 技术栈：Tauri 2、Rust、Vanilla TypeScript/CSS、Codex App Server
- 首发平台：macOS 与 Windows

![Quota Critter Lumo concept](assets/concepts/quota-critter-lumo-v2.png)

最新交互概念（收起态无数字、点击星星展开详情）：[v9 concept](assets/concepts/quota-critter-collapsed-no-number-v9.png)

> 概念图用于确定方向，不是像素级 UI 交付稿。实现时以 `docs/DESIGN_SPEC.md` 中的尺寸、颜色和状态规则为准。

## 文档

- [产品需求文档](docs/PRD.md)
- [视觉与交互规范](docs/DESIGN_SPEC.md)
- [技术方案](docs/TECHNICAL_DESIGN.md)
- [概念图生成 Prompt](assets/concepts/PROMPT.md)

## 开发状态

当前优先实现 M0–M2：可运行的桌面窗口、Codex App Server stdio 协议、认证状态、额度读取、缓存和离线 UI。

## 本地运行

先确认本机已经安装并登录 Codex CLI，然后在项目根目录执行：

```bash
npm install
npm run dev              # 浏览器预览，使用 mock 额度
npm run tauri dev        # 桌面开发模式，连接本机 codex app-server
npm run typecheck
npm run build
```

如果 `codex` 不在 `PATH`，可以临时指定：

```bash
QUOTA_CRITTER_CODEX_PATH=/absolute/path/to/codex npm run tauri dev
```

生成安装包：

```bash
npm run tauri build
```

安装包会出现在 `src-tauri/target/release/bundle/`。Windows 构建需要在 Windows runner 上执行；代码和 Tauri 配置已按 Windows/macOS 双平台设计。

推送带有 `v*` 标签，或在 GitHub Actions 手动运行 `Windows package` workflow，会在 Windows runner 上生成 NSIS 安装包并作为 workflow artifact 上传。

## 当前实现

- Lumo 紫蓝像素星灵、轨道额度指示、主额度与重置倒计时
- Lumo 素材来自概念方向的像素星灵：`src/assets/lumo-sprite.png`
- 优先展示 App Server 的真实 `individualLimit`，避免把模型时间窗误读成账户剩余额度
- 收起态只显示 Lumo 与弧形轨道亮点；点击 Lumo 后才展开百分比、进度条和重置时间
- 自动轮询更新额度；手动刷新位于系统托盘菜单，不占用主视觉
- 展开详情查看 primary/secondary 窗口，低额度、耗尽、过期和离线状态
- 系统托盘：显示/隐藏、刷新、设置、退出；关闭窗口只隐藏到托盘
- ChatGPT 登录入口，使用官方 App Server，不读取或复制本地 Token
- 本地额度快照缓存和原子化设置存储
- 浏览器 mock 预览，便于不安装 Tauri 时快速看 UI

## 官方依赖

- [Codex App Server](https://learn.chatgpt.com/docs/app-server)
- [Codex Authentication](https://learn.chatgpt.com/docs/auth)
- [Tauri 2](https://v2.tauri.app/start/)
- [Tauri System Tray](https://v2.tauri.app/learn/system-tray/)
- [Tauri Distribution](https://v2.tauri.app/distribute/)

Unofficial open-source companion; not affiliated with OpenAI.
