# no-sleep-agent-tray

> Windows 防锁屏、防睡眠、保网络的托盘小工具，让 AI Agent 可以长时间稳定运行。
> A Windows tray tool that prevents lock screen, sleep and network drops, keeping AI agents running stably for long sessions.

![platform](https://img.shields.io/badge/platform-Windows-blue) ![lang](https://img.shields.io/badge/lang-Rust-orange) ![license](https://img.shields.io/badge/license-MIT-green)

## 下载 / Download

| 版本 Version | 文件 File | 说明 Notes |
|---|---|---|
| x64 | `dist/keepawake-x64.exe` | Intel / AMD 64 位，最常见 |
| ARM64 | `dist/keepawake-arm64.exe` | 骁龙 / ARM 版 Windows |

单文件、免安装、无黑窗口，双击即用。
Single file, no install, no console window. Double-click to run.

## 功能 / Features

- [x] **防睡眠 Anti-sleep** — `SetThreadExecutionState` 系统级保持，AI agent 可正常长时间运行
- [x] **显示器常亮 Keep display on** — 可选
- [x] **AwayMode 保网络 Keep-network** — 笔记本不断网（可选）
- [x] **F15 心跳 F15 heartbeat** — 无害按键重置空闲计时，对付公司域 GPO 的“无操作 X 分钟锁屏”
- [x] **右键菜单 Tray menu** — 启用保持 / 设置 / 开机自启 / 亮度 50% / 亮度 15% / 退出（永久双语）
- [x] **设置窗口 Settings dialog** — 分区 + 粗体标题，中英即时切换（`英文界面 / English UI`），配置自动存 `%APPDATA%\keepawake\config.ini`
- [x] **调亮度 Brightness** — DDC/CI（外接显示器）+ WMI（笔记本面板）+ 电源方案三路，附带结果气泡通知
- [x] **开机自启 Start on boot** — 一键写注册表 `HKCU\...\Run`

## 使用 / Usage

1. 双击 `keepawake-*.exe`，图标缩到右下角托盘（绿点=保持中，灰点=已暂停）。
   Double-click the exe; it lives in the system tray (green dot = active, gray = paused).
2. 右键托盘 → `设置... / Settings...` 勾选配置，点`应用 / Apply`即时生效。
   Right-click tray → Settings to configure, Apply takes effect immediately.
3. 命令行仍可用（`--help` 查看）：`--no-tray` 纯控制台模式，`--brightness-50/15`，`--lang en` 等。
   CLI still works (`--help`): `--no-tray`, `--brightness-50/15`, `--lang en`, etc.

## 从源码编译 / Build from source

```powershell
# 默认（本机架构）
cargo build --release

# 双架构
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

需要 Rust 稳定版（MSVC toolchain）。本项目用 `native-windows-gui` 做托盘与原生设置窗。
Requires stable Rust (MSVC toolchain). Tray + native settings dialog via `native-windows-gui`.

## 工作原理 / How it works

1. `SetThreadExecutionState(ES_SYSTEM | ES_DISPLAY | ES_AWAYMODE)` 告诉 Windows“正在使用”，阻止睡眠/熄屏/断网。
   Tells Windows the machine is in use: no sleep, no display off, network stays up.
2. 每隔几分钟发送一次无害的 `F15` 按键，重置系统空闲计时，防 GPO 强制锁屏。
   Sends a harmless `F15` keystroke every few minutes to reset the idle timer (beats GPO idle lock).
3. 亮度走三条路：显示器 DDC/CI → 笔记本 WMI（`WmiSetBrightness` 必须 `Timeout=0` 否则会被弹回）→ 系统电源方案。
   Brightness via DDC/CI → laptop WMI (`WmiSetBrightness` must use `Timeout=0` or it reverts) → power scheme.

## 交接文档 / Handoff

给 AI 看的项目交接文档见 [`ai.md`](ai.md)。
Handoff doc for AI assistants: [`ai.md`](ai.md) (Chinese).

## License

MIT
