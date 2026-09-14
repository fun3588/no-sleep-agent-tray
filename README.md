# no-sleep-agent-tray

> Windows 防锁屏、防睡眠、保网络的托盘小工具，让 AI Agent 可以长时间稳定运行。

![platform](https://img.shields.io/badge/platform-Windows-blue) ![lang](https://img.shields.io/badge/lang-Rust-orange) ![license](https://img.shields.io/badge/license-MIT-green)

[中文](#中文) | [English](#english)

---

<a id="中文"></a>
## 中文

### 下载

| 版本 | 文件 | 说明 |
|---|---|---|
| x64 | `dist/keepawake-x64.exe` | Intel / AMD 64 位，最常见 |
| ARM64 | `dist/keepawake-arm64.exe` | 骁龙 / ARM 版 Windows |

单文件、免安装、无黑窗口，双击即用。

### 功能

- [x] **防睡眠** — `SetThreadExecutionState` 系统级保持，AI agent 可正常长时间运行
- [x] **显示器常亮** — 可选
- [x] **AwayMode 保网络** — 笔记本不断网（可选）
- [x] **F15 心跳** — 无害按键重置空闲计时，对付公司域 GPO 的“无操作 X 分钟锁屏”
- [x] **右键菜单** — 启用保持 / 设置 / 开机自启 / 亮度 50% / 亮度 15% / 退出（永久双语）
- [x] **设置窗口** — 分区 + 粗体标题，中英即时切换（`英文界面 / English UI`），配置自动存 `%APPDATA%\keepawake\config.ini`
- [x] **调亮度** — DDC/CI（外接显示器）+ WMI（笔记本面板）+ 电源方案三路，附带结果气泡通知
- [x] **开机自启** — 一键写注册表 `HKCU\...\Run`

### 使用

1. 双击 `keepawake-*.exe`，图标缩到右下角托盘（绿点=保持中，灰点=已暂停）。
2. 右键托盘 → `设置...` 勾选配置，点`应用`即时生效。
3. 命令行仍可用（`--help` 查看）：`--no-tray` 纯控制台模式，`--brightness-50/15`，`--lang en` 等。

### 从源码编译

```powershell
# 默认（本机架构）
cargo build --release

# 双架构
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

需要 Rust 稳定版（MSVC toolchain）。本项目用 `native-windows-gui` 做托盘与原生设置窗。

### 工作原理

1. `SetThreadExecutionState(ES_SYSTEM | ES_DISPLAY | ES_AWAYMODE)` 告诉 Windows“正在使用”，阻止睡眠/熄屏/断网。
2. 每隔几分钟发送一次无害的 `F15` 按键，重置系统空闲计时，防 GPO 强制锁屏。
3. 亮度走三条路：显示器 DDC/CI → 笔记本 WMI（`WmiSetBrightness` 必须 `Timeout=0` 否则会被弹回）→ 系统电源方案。

### 交接文档

给 AI 看的项目交接文档见 [`ai.md`](ai.md)。

---

<a id="english"></a>
## English

> A Windows tray tool that prevents lock screen, sleep and network drops, keeping AI agents running stably for long sessions.

### Download

| Version | File | Notes |
|---|---|---|
| x64 | `dist/keepawake-x64.exe` | Intel / AMD 64-bit, most common |
| ARM64 | `dist/keepawake-arm64.exe` | Snapdragon / ARM Windows |

Single file, no install, no console window. Double-click to run.

### Features

- [x] **Anti-sleep** — system-level keep-awake via `SetThreadExecutionState`, AI agents run uninterrupted
- [x] **Keep display on** — optional
- [x] **AwayMode keep-network** — no disconnects on laptops (optional)
- [x] **F15 heartbeat** — harmless keystroke resets the idle timer, beats corporate GPO idle lock
- [x] **Tray menu** — Enable / Settings / Start on boot / Brightness 50% / 15% / Exit (permanently bilingual)
- [x] **Settings dialog** — sections with bold headers, instant CN/EN switch (`英文界面 / English UI`), auto-saved to `%APPDATA%\keepawake\config.ini`
- [x] **Brightness** — DDC/CI (external monitors) + WMI (laptop panels) + power scheme, with result balloon
- [x] **Start on boot** — one click writes `HKCU\...\Run`

### Usage

1. Double-click `keepawake-*.exe`; it lives in the system tray (green dot = active, gray = paused).
2. Right-click tray → Settings to configure, Apply takes effect immediately.
3. CLI still works (`--help`): `--no-tray`, `--brightness-50/15`, `--lang en`, etc.

### Build from source

```powershell
# default (host arch)
cargo build --release

# both arches
cargo build --release --target x86_64-pc-windows-msvc
cargo build --release --target aarch64-pc-windows-msvc
```

Requires stable Rust (MSVC toolchain). Tray + native settings dialog via `native-windows-gui`.

### How it works

1. `SetThreadExecutionState(ES_SYSTEM | ES_DISPLAY | ES_AWAYMODE)` tells Windows the machine is in use: no sleep, no display off, network stays up.
2. Sends a harmless `F15` keystroke every few minutes to reset the idle timer (beats GPO idle lock).
3. Brightness via DDC/CI → laptop WMI (`WmiSetBrightness` must use `Timeout=0` or it reverts) → power scheme.

### Handoff

Handoff doc for AI assistants: [`ai.md`](ai.md).

## License

MIT
