# no-sleep-agent-tray 交接 / Handoff

> 本文件是给 AI 的工作交接文档，请先阅读此文档再继续对本仓库进行操作。
> Handoff doc for AI assistants — read this file before working on this repo. Chinese first, English second.

## 一、还没做完的事 & 动手前注意 / TODO & Cautions

### 尚未完成 / 待办 Todo
- [ ] 左键双击托盘图标直接打开设置窗（目前只能右键菜单进，`OnContextMenu` 已有，缺左键处理）
- [ ] 发第一个 GitHub Release（workflow 已就绪：push `v*` tag 自动双架构编译并挂附件；`dist/` 快照保留，长期建议只发 Release 不再 push 二进制）
  Cut the first GitHub Release (workflow ready: pushing a `v*` tag builds both arches and attaches exes).
- [ ] `keepawake.go` 是 v1 时代的纯控制台单文件版，已过时（托盘版只有 Rust），要么删除要么标注废弃
- [ ] 在真笔记本上验证“电源方案”调光路（当前只在台式机/虚拟机上验证了 WMI 路）
- [ ] 英文界面下 giants 超长字符串的人工目检（目前只做了加宽到 480px + 程序化 dump，缺真机截图确认）

### 动手前注意事项 / Cautions
1. 用户已明确要求 `git push` 到 `https://github.com/fun3588/no-sleep-agent-tray.git`；后续无明确指示时不要擅自 push。
   Push to `https://github.com/fun3588/no-sleep-agent-tray.git` only when the user asks; never push unprompted.
2. exe 必须是 **GUI 子系统**（`#![windows_subsystem = "windows"]`），否则双击会弹黑窗；改完用脚本确认 PE 的 subsystem 字段 = 2。
   The exe must be the **windows subsystem** (no console); verify PE subsystem == 2 after changes.
3. 链接 `comctl32` 的 `GetWindowSubclass`（nwg 依赖）需要 **app manifest 开启 Common-Controls v6**，否则启动即 `0xC0000139` 崩溃。manifest 通过 `build.rs` + `embed-resource` 嵌入（纯 Rust，无需 Windows SDK），`keepawake.rc` / `keepawake.manifest` 别删。
   `GetWindowSubclass` needs Common-Controls v6 via app manifest or the exe dies with `0xC0000139`; embedded by `build.rs` + `embed-resource`, keep those files.
4. `nwg::Icon::source_bin` 需要开 `image-decoder` feature；`MenuItem` **没有** `set_text`，所以托盘菜单是永久双语写法，不要试图运行时改名。
   `source_bin` needs the `image-decoder` feature; `MenuItem` has no `set_text`, so tray menu items are permanently bilingual.
5. 事件绑定必须在**托盘的 MessageWindow 和设置 Window 上各绑一次**（`full_bind_event_handler`），否则设置窗里的按钮点了没反应。
   Bind events on **both** the tray MessageWindow and the settings Window, or settings buttons go dead.
6. WMI 调亮度 `WmiSetBrightness(Timeout, Brightness)` 的 Timeout 必须传 **0**（=不恢复）；传 1 会在 1 秒后弹回原亮度。
   WMI `WmiSetBrightness` Timeout must be **0** (no revert); 1 reverts after a second.
7. 验证亮度必须“记录原值 → 设新值 → 回读”（`WmiMonitorBrightness.CurrentBrightness`），原值恰好是目标值时证明不了任何事。
   Verify brightness as record-before → set → read-back; setting 50 when it already is 50 proves nothing.
8. PowerShell 5.1 注意：不支持 `0u` 字面量（用 `[uint32]0`）；`*.ps1` 无 BOM 会被按 GBK 解码——测试脚本只用 ASCII + `[char]` 码点；同一个进程里第二次 `EnumChildWindows` 可能返回空，UI 自动化脚本请“一个进程只枚举一次”。
   PowerShell 5.1 notes: no `0u` literal; BOM-less scripts decode as GBK (ASCII + `[char]` codes only); second `EnumChildWindows` in one process may return empty (one enum per process).
9. `dist/` 下的 exe 是故意提交的（用户要求），`.gitignore` 只忽略 `target/`。
   Binaries under `dist/` are committed on purpose; `.gitignore` ignores only `target/`.
10. 不要提交密钥、token；`%APPDATA%\keepawake\config.ini` 是用户本地配置，不要进仓库。
    Never commit secrets; `%APPDATA%\keepawake\config.ini` is per-user config, keep it out of the repo.

## 二、历史记录 / History

- 2026-09-14：创建 Rust 零依赖控制台版 `src/main.rs`（`SetThreadExecutionState` + F15 心跳），`cargo build --release` 验证；附带 `keepawake.go` 单文件版。文件：`Cargo.toml`、`src/main.rs`、`keepawake.go`。
  Created the zero-dependency Rust console version (`SetThreadExecutionState` + F15 heartbeat), verified with `cargo build --release`; plus single-file `keepawake.go`.
- 2026-09-14：升级 2.0 托盘版，引入 `native-windows-gui`（托盘图标+右键菜单+设置窗）与 `winreg`（开机自启），配置存 `%APPDATA%\keepawake\config.ini`。踩坑：`comctl32!GetWindowSubclass` 缺 v6 导致 `0xC0000139`，自写 PE 脚本定位，加 `build.rs` + `embed-resource` 嵌 manifest 解决；`source_bin` 需补 `image-decoder`。
  2.0 tray version with `native-windows-gui` + `winreg`; fixed `0xC0000139` with embedded v6 manifest; added `image-decoder` for `source_bin`.
- 2026-09-14：改 GUI 子系统去黑窗，`--help/--no-tray/--console` 用 `AttachConsole` 回挂父终端；GUI 致命错误改弹窗。PE 确认 subsystem=2。
  GUI subsystem (no console); CLI modes reattach via `AttachConsole`; fatal errors show a message box.
- 2026-09-14：修“设置页取消没用”——根因是事件只绑了 MessageWindow（见注意事项 5），改双绑；取消/X 丢弃未保存修改。托盘右键加亮度菜单。
  Fixed dead settings buttons (bound events on both windows); Cancel/X discards unsaved edits; added tray brightness item.
- 2026-09-14：修“亮度不能用”——WMI Timeout 改 0，新增电源方案调光路（`powrprof` FFI），菜单“亮度 50% / 15%”两档。严格验证：原值 50 → 设 15 回读 15 → 设回 50 回读 50。
  Fixed brightness (WMI Timeout=0, new power-scheme path, 50%/15% items); verified 50 → 15 → 50 with read-backs.
- 2026-09-14：设置窗美化（三段式分区+粗体）与中英双语（`tr()` 表、`English UI` 即时切换、`--lang`），托盘菜单永久双语。自动化验证 CN/EN 各 19 控件、点击英文框整窗切换且 `config.ini` 写入 `lang=en`。
  Prettier settings dialog (sections + bold fonts) and CN/EN switch; verified 19 controls in each language plus click-to-switch with config persistence.
- 2026-09-14：定仓名 `no-sleep-agent-tray`，代码搬到 `C:\code\no-sleep-agent-tray`，写 `README.md` + 本 `ai.md`，双架构编译产物进 `dist/` 随仓 push：`git init`、`git remote add origin …`、`git push -u origin main`（首个 commit `7ca3368`）。
  Repo settled, dual-arch builds into `dist/`, first push commit `7ca3368`.
- 2026-09-14：加 GitHub Release 自动化（`.github/workflows/release.yml`：push `v*` tag → Windows runner 双架构编译 → `softprops/action-gh-release` 挂 `keepawake-x64.exe` / `keepawake-arm64.exe` 附件）；README 下载节指向 Releases。
  Added release automation (`v*` tag → dual-arch build → attached exes); README points to Releases.
  Repo settled at `C:\code\no-sleep-agent-tray`; dual-arch builds into `dist/`; first push commit `7ca3368`.
- 2026-09-14：修英文界面字体溢出——设置窗加宽 440→480、状态行加高 46→60、输入框右移；`README.md` / `ai.md` 改全文双语（先中文后英文）；版本 2.1.1，双架构重编 push。
  Fixed EN overflow (dialog 440→480px, taller status row); both MDs rewritten fully bilingual (CN first); v2.1.1 rebuilt for both arches.

当前仓库状态 / Current layout：

```text
no-sleep-agent-tray/
├── Cargo.toml / Cargo.lock
├── build.rs  (+ keepawake.rc / keepawake.manifest → Common-Controls v6)
├── src/main.rs            # 主程序 main program（托盘 tray + 设置窗 settings + 保活 keep-awake + 调光 brightness）
├── keepawake.go           # v1 纯控制台版 legacy console version（已过时 legacy）
├── assets/icon_on.ico / icon_off.ico   # 托盘绿/灰圆点 tray dots (embedded)
├── README.md              # 中英双语说明 bilingual (CN first)
├── ai.md                  # 本文件 this file
└── dist/
    ├── keepawake-x64.exe    # x86_64-pc-windows-msvc release
    └── keepawake-arm64.exe  # aarch64-pc-windows-msvc release
```

## 三、为什么做这件事 / Why

- 目标 / Goal：让 Windows 电脑在跑 AI Agent 时不锁屏、不睡眠、不断网，Agent 能整夜稳定干活；顺手解决公司 GPO 空闲锁屏、笔记本断网、临时调亮度等痛点。
  Keep Windows awake, unlocked and online so AI agents run stably overnight; also beats GPO idle lock, laptop disconnects, ad-hoc brightness.
- 价值 / Value：单文件 exe、双击即用、无黑窗、常驻托盘，一次配置到处用；Rust 编写，体积小（~400KB）、无运行时依赖。
  Single-file exe, zero-install, no console, tray-resident; tiny (~400KB) Rust binary, no runtime deps.
- 复用方式 / Reuse：`src/main.rs` 的三段式保活、`tr()` 双语表、三路调光、nwg 托盘+设置窗都是可直接抄走的模板；本 `ai.md` 模板也可复用到其他仓库做 AI 交接。
  The keep-awake trio, `tr()` i18n table, brightness paths and the nwg tray/settings code are copy-paste templates; this `ai.md` template works for other repos too.
