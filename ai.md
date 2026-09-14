# no-sleep-agent-tray 交接

> 本文件是给 AI 的工作交接文档，请先阅读此文档再继续对本仓库进行操作。

## 一、还没做完的事 & 动手前注意

### 尚未完成 / 待办
- [ ] 左键双击托盘图标直接打开设置窗（目前只能右键菜单进，`OnContextMenu` 已有，缺左键 `OnTrayNotification`/`OnClick` 处理）
- [ ] 发 GitHub Release（目前二进制直接 `git push` 进 `dist/`，体积会越滚越大，后续建议转 Release 附件）
- [ ] `keepawake.go` 是 v1 时代的纯控制台单文件版，已过时（托盘版只有 Rust），要么删除要么标注废弃
- [ ] 验证清单里“取消按钮自动化点击测试”曾被 PowerShell 5.1 的 delegate 封送 quirks 卡住（见注意事项 5），如重做 UI 测试建议换 pytest + `pywinauto` 或只做单枚举脚本
- [ ] TODO: 在真笔记本上验证“电源方案”调光路（当前只在台式机/虚拟机上验证了 WMI 路）

### 动手前注意事项
1. 用户已明确要求 `git push` 到 `https://github.com/fun3588/no-sleep-agent-tray.git`，本次允许 push；后续无明确指示时不要擅自 push。
2. exe 必须是 **GUI 子系统**（`#![windows_subsystem = "windows"]`），否则双击会弹黑窗；改完用脚本确认 PE 的 subsystem 字段 = 2。
3. 链接 `comctl32` 的 `GetWindowSubclass`（nwg 依赖）需要 **app manifest 开启 Common-Controls v6**，否则启动即 `0xC0000139` 崩溃。manifest 通过 `build.rs` + `embed-resource` 嵌入（纯 Rust，无需 Windows SDK），`keepawake.rc` / `keepawake.manifest` 别删。
4. `nwg::Icon::source_bin` 需要开 `image-decoder` feature；`MenuItem` **没有** `set_text`，所以托盘菜单是永久双语写法，不要试图运行时改名。
5. 事件绑定必须在**托盘的 MessageWindow 和设置 Window 上各绑一次**（`full_bind_event_handler`），否则设置窗里的按钮点了没反应——之前“取消没用”的 bug 就是这么来的。
6. WMI 调亮度 `WmiSetBrightness(Timeout, Brightness)` 的 Timeout 必须传 **0**（=不恢复）；传 1 会在 1 秒后弹回原亮度，看起来像功能失效。
7. 验证亮度必须“记录原值 → 设新值 → 回读”（`WmiMonitorBrightness.CurrentBrightness`），原值恰好是 50 时“设 50 读 50”证明不了任何事——之前踩过这个坑。
8. PowerShell 5.1 注意：不支持 `0u` 字面量（用 `[uint32]0`）；`*.ps1` 无 BOM 会被按 GBK 解码，中文全乱码——测试脚本只用 ASCII + `[char]` 码点；同一个进程里第二次 `EnumChildWindows` 可能返回空，UI 自动化脚本请“一个进程只枚举一次”。
9. `dist/` 下的 exe 是故意提交的（用户要求），`.gitignore` 只忽略 `target/`，不要把 `dist/` 加进去。
10. 不要提交密钥、token；`%APPDATA%\keepawake\config.ini` 是用户本地配置，不要进仓库。

## 二、历史记录（我们做了什么、怎么做的）

- 2026-09-14：创建 Rust 零依赖控制台版 `src/main.rs`（`SetThreadExecutionState` + F15 心跳），`cargo build --release` 验证；附带 `keepawake.go` 单文件版。文件：`Cargo.toml`、`src/main.rs`、`keepawake.go`。
- 2026-09-14：升级 2.0 托盘版，引入 `native-windows-gui`（托盘图标+右键菜单+设置窗复选框）与 `winreg`（开机自启），配置存 `%APPDATA%\keepawake\config.ini`。踩坑：`comctl32!GetWindowSubclass` 缺 v6 导致 `0xC0000139`，用自写 `pe_imports.py` 定位，加 `build.rs` + `embed-resource` 嵌 manifest 解决；`source_bin` 需补 `image-decoder` feature。
- 2026-09-14：改 GUI 子系统去黑窗（`#![windows_subsystem="windows"]`），`--help/--no-tray/--console` 用 `AttachConsole` 回挂父终端；GUI 致命错误改弹窗。PE 确认 subsystem=2。
- 2026-09-14：修“设置页取消没用”——根因是事件只绑了 MessageWindow（见注意事项 5），改双绑；取消/X 顺带丢弃未保存修改。托盘右键加“亮度调到一半”。
- 2026-09-14：修“亮度不能用”——根因是 WMI Timeout=1 会弹回（改 0），并新增电源方案调光路（`powrprof` FFI），菜单加到“亮度 50% / 15%”两档，`--brightness-50/15` 可命令行验证。严格验证：原值 50 → 设 15 回读 15 → 设回 50 回读 50。
- 2026-09-14：设置窗美化（标题+分区加粗字体 `Segoe UI`、三段式分区布局）与中英双语（`Lang::Cn/En`、`tr()` 字符串表、`英文界面 / English UI` 即时切换、`--lang`），托盘菜单永久双语。自动化验证：CN/EN 各 dump 19 个控件、`BM_CLICK` 点英文框后整窗变英文且 `config.ini` 写入 `lang=en`。
- 2026-09-14：定仓名 `no-sleep-agent-tray`；代码搬到 `C:\code\no-sleep-agent-tray`；写 `README.md`（中英）与本 `ai.md`；`cargo build --release --target x86_64-pc-windows-msvc` 与 `--target aarch64-pc-windows-msvc` 双架构编译，产物进 `dist/` 并随仓 push。关键命令：`git init`、`git add -A`、`git commit -m "..."`、`git remote add origin https://github.com/fun3588/no-sleep-agent-tray.git`、`git push -u origin main`。

当前仓库状态：

```text
no-sleep-agent-tray/
├── Cargo.toml / Cargo.lock
├── build.rs  (+ keepawake.rc / keepawake.manifest → Common-Controls v6)
├── src/main.rs            # 主程序（托盘 + 设置窗 + 保活 + 调光，约1400行）
├── keepawake.go           # v1 纯控制台版（已过时，待处理）
├── assets/icon_on.ico / icon_off.ico   # 托盘绿/灰圆点（内嵌）
├── README.md              # 中英双语说明
├── ai.md                  # 本文件
└── dist/
    ├── keepawake-x64.exe    # x86_64-pc-windows-msvc release
    └── keepawake-arm64.exe  # aarch64-pc-windows-msvc release
```

## 三、为什么做这件事

- 目标：让 Windows 电脑在跑 AI Agent 时不锁屏、不睡眠、不断网，Agent 能整夜稳定干活；顺手解决公司 GPO 空闲锁屏、笔记本合盖断网、临时调亮度等痛点。
- 价值：单文件 exe、双击即用、无黑窗、常驻托盘，一次配置到处用；Rust 编写，体积小（~370KB）、无运行时依赖。
- 复用方式：`src/main.rs` 里的三段式保活（ExecutionState + F15 + AwayMode）、三路调光（DDC/CI + WMI + 电源方案）、nwg 托盘+双语设置窗都是可直接抄走的模板；`ai.md` 模板本身也可复用到其他仓库做 AI 交接。
