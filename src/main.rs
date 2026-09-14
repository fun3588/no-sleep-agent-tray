#![windows_subsystem = "windows"]
// keepawake 2.0 - Windows 防锁屏/防睡眠/保持网络小工具（托盘版）
// GUI 子系统：双击运行不弹黑窗口；带 --help / --no-tray / --console 时自动挂靠父终端输出。
//
// 右下角托盘图标显示状态，右键菜单 + 设置窗口可勾选配置：
//   [x] 启用保持（防睡眠，AI agent 可正常运行）
//   [x] 显示器常亮 / AwayMode 保网络 / F15 防锁屏
//   刷新间隔、F15 心跳间隔、开机自启
// 配置自动存 %APPDATA%\keepawake\config.ini
//
// 命令行仍兼容 1.0：--no-tray 回到纯控制台模式；--console 在托盘模式下保留控制台。

use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use native_windows_gui as nwg;

// ---------- Win32: 保活 / 按键 / 藏控制台 ----------
const ES_CONTINUOUS: u32 = 0x8000_0000;
const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;
const ES_DISPLAY_REQUIRED: u32 = 0x0000_0002;
const ES_AWAYMODE_REQUIRED: u32 = 0x0000_0040;
const VK_F15: u8 = 0x7E;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
const MB_ICONERROR: u32 = 0x0000_0010;

#[link(name = "kernel32")]
extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
    fn AttachConsole(process_id: u32) -> i32;
    fn AllocConsole() -> i32;
    fn LocalFree(h: isize) -> isize;
}

#[link(name = "user32")]
extern "system" {
    fn keybd_event(bvk: u8, bscan: u8, dwflags: u32, dwextrainfo: usize);
    fn MessageBoxW(hwnd: isize, text: *const u16, caption: *const u16, typ: u32) -> i32;
}

fn set_awake(display: bool, away: bool) {
    let mut flags = ES_CONTINUOUS | ES_SYSTEM_REQUIRED;
    if display {
        flags |= ES_DISPLAY_REQUIRED;
    }
    if away {
        flags |= ES_AWAYMODE_REQUIRED;
    }
    unsafe {
        SetThreadExecutionState(flags);
    }
}

fn clear_awake() {
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS);
    }
}

fn jiggle_f15() {
    unsafe {
        keybd_event(VK_F15, 0, 0, 0);
        std::thread::sleep(Duration::from_millis(50));
        keybd_event(VK_F15, 0, KEYEVENTF_KEYUP, 0);
    }
}

// ---------- 亮度调到一半（DDC/CI 外接显示器 + WMI 笔记本面板，双管齐下） ----------
#[repr(C)]
struct PhysicalMonitor {
    h: isize,
    desc: [u16; 128],
}

type EnumMonProc = unsafe extern "system" fn(isize, isize, *mut (), isize) -> i32;

#[link(name = "dxva2")]
extern "system" {
    fn GetNumberOfPhysicalMonitorsFromHMONITOR(hmon: isize, n: *mut u32) -> i32;
    fn GetPhysicalMonitorsFromHMONITOR(hmon: isize, n: u32, arr: *mut PhysicalMonitor) -> i32;
    fn SetMonitorBrightness(hmon: isize, brightness: u32) -> i32;
    fn DestroyPhysicalMonitors(n: u32, arr: *mut PhysicalMonitor) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn EnumDisplayMonitors(hdc: isize, clip: *const (), cb: EnumMonProc, data: isize) -> i32;
}

unsafe extern "system" fn collect_mon(hmon: isize, _: isize, _: *mut (), data: isize) -> i32 {
    (*(data as *mut Vec<isize>)).push(hmon);
    1
}

/// 用 DDC/CI 给所有显示器设亮度，返回 (成功数, 总数)
fn set_ddc_brightness(level: u32) -> (usize, usize) {
    let mut mons: Vec<isize> = Vec::new();
    let enumerated =
        unsafe { EnumDisplayMonitors(0, std::ptr::null(), collect_mon, &mut mons as *mut _ as isize) }
            != 0;
    if !enumerated {
        return (0, 0);
    }
    let mut ok = 0usize;
    let mut total = 0usize;
    for hmon in mons {
        let mut n: u32 = 0;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut n) } == 0 || n == 0 {
            continue;
        }
        let mut arr: Vec<PhysicalMonitor> = Vec::with_capacity(n as usize);
        if unsafe { GetPhysicalMonitorsFromHMONITOR(hmon, n, arr.as_mut_ptr()) } == 0 {
            continue;
        }
        unsafe { arr.set_len(n as usize) };
        for pm in &arr {
            total += 1;
            if unsafe { SetMonitorBrightness(pm.h, level) } != 0 {
                ok += 1;
            }
        }
        unsafe { DestroyPhysicalMonitors(n, arr.as_mut_ptr()) };
    }
    (ok, total)
}

/// 用 WMI 给笔记本内置面板设亮度（隐藏 powershell，无黑窗），返回成功实例数
/// 注意：Timeout 必须传 0（=不恢复）；传 N 表示 N 秒后弹回原亮度，之前用 1 就是这么失效的。
fn set_wmi_brightness(level: u32) -> usize {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let script = format!(
        "$m = @(Get-WmiObject -Namespace root/WMI -Class WmiMonitorBrightnessMethods); \
         $m | ForEach-Object {{ $_.WmiSetBrightness(0, {lvl}) | Out-Null }}; $m.Count",
        lvl = level
    );
    match std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().parse().unwrap_or(0),
        _ => 0,
    }
}

// ---------- 第三条路：系统电源方案里的显示亮度（Win7+ 通用，笔记本最稳） ----------
#[repr(C)]
struct WinGuid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

// VIDEO_SUBGROUP {7516b95f-f776-4464-8c53-06167f40cc99}
// VIDEO_BRIGHTNESS {aded5e82-bebe-4f43-8a85-657b3076121c}
const VIDEO_SUBGROUP: WinGuid = WinGuid {
    d1: 0x7516b95f,
    d2: 0xf776,
    d3: 0x4464,
    d4: [0x8c, 0x53, 0x06, 0x16, 0x7f, 0x40, 0xcc, 0x99],
};
const VIDEO_BRIGHTNESS: WinGuid = WinGuid {
    d1: 0xaded5e82,
    d2: 0xbebe,
    d3: 0x4f43,
    d4: [0x8a, 0x85, 0x65, 0x7b, 0x30, 0x76, 0x12, 0x1c],
};

#[link(name = "powrprof")]
extern "system" {
    fn PowerGetActiveScheme(user: isize, scheme: *mut *mut WinGuid) -> u32;
    fn PowerWriteACValueIndex(
        user: isize,
        scheme: *const WinGuid,
        sub: *const WinGuid,
        setting: *const WinGuid,
        value: u32,
    ) -> u32;
    fn PowerWriteDCValueIndex(
        user: isize,
        scheme: *const WinGuid,
        sub: *const WinGuid,
        setting: *const WinGuid,
        value: u32,
    ) -> u32;
    fn PowerSetActiveScheme(user: isize, scheme: *const WinGuid) -> u32;
}

/// 写当前电源方案的交/直流亮度并立即生效，成功返回 true
fn set_scheme_brightness(level: u32) -> bool {
    unsafe {
        let mut scheme: *mut WinGuid = std::ptr::null_mut();
        if PowerGetActiveScheme(0, &mut scheme) != 0 || scheme.is_null() {
            return false;
        }
        let a = PowerWriteACValueIndex(0, scheme, &VIDEO_SUBGROUP, &VIDEO_BRIGHTNESS, level);
        let d = PowerWriteDCValueIndex(0, scheme, &VIDEO_SUBGROUP, &VIDEO_BRIGHTNESS, level);
        let s = PowerSetActiveScheme(0, scheme);
        LocalFree(scheme as isize);
        a == 0 && d == 0 && s == 0
    }
}

fn set_brightness(level: u32, lang: Lang) -> String {
    let (ok, total) = set_ddc_brightness(level);
    let wmi = set_wmi_brightness(level);
    let scheme = set_scheme_brightness(level);
    let mut ways: Vec<String> = vec![];
    if ok > 0 {
        ways.push(match lang {
            Lang::Cn => format!("显示器 DDC {}/{} 台", ok, total),
            Lang::En => format!("{} of {} monitors via DDC", ok, total),
        });
    }
    if wmi > 0 {
        ways.push(tr(lang, "bri_wmi").to_string());
    }
    if scheme {
        ways.push(tr(lang, "bri_scheme").to_string());
    }
    if ways.is_empty() {
        if total > 0 {
            match lang {
                Lang::Cn => "亮度设置失败：显示器拒绝 DDC/CI 指令，请检查显示器菜单是否禁用了 DDC/CI"
                    .to_string(),
                Lang::En => "Brightness failed: monitors rejected DDC/CI, check if DDC/CI is disabled in monitor menu".to_string(),
            }
        } else {
            match lang {
                Lang::Cn => "未找到可调亮度的显示器（台式机请用显示器按键，远程桌面无法调节）".to_string(),
                Lang::En => "No adjustable display found (use monitor buttons on desktops; RDP cannot adjust)".to_string(),
            }
        }
    } else {
        match lang {
            Lang::Cn => format!("亮度已设为 {}%（{}）", level, ways.join(" + ")),
            Lang::En => format!("Brightness set to {}% ({})", level, ways.join(" + ")),
        }
    }
}

/// 命令行模式需要输出时：挂靠父终端，挂靠失败则新建一个控制台窗口。
fn ensure_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            AllocConsole();
        }
    }
    println!();
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// GUI 模式致命错误弹窗（无控制台时也能让用户看到原因）
fn fatal_popup(msg: &str) -> ! {
    unsafe {
        MessageBoxW(
            0,
            to_wide(msg).as_ptr(),
            to_wide("No-Sleep-Agent").as_ptr(),
            MB_ICONERROR,
        );
    }
    std::process::exit(1);
}

fn stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let s = (secs + 8 * 3600) % 86400; // 北京时间显示
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

// ---------- 配置 ----------
#[derive(Clone, Debug)]
struct Config {
    enabled: bool,
    display: bool,
    away: bool,
    jiggle: bool,
    interval: u64,
    jiggle_interval: u64,
    lang: Lang,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            display: true,
            away: true,
            jiggle: true,
            interval: 30,
            jiggle_interval: 240,
            lang: Lang::Cn,
        }
    }
}

// ---------- 中英文 ----------
#[derive(Clone, Copy, Debug, PartialEq)]
enum Lang {
    Cn,
    En,
}

impl Lang {
    fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("en") {
            Lang::En
        } else {
            Lang::Cn
        }
    }
    fn code(self) -> &'static str {
        match self {
            Lang::Cn => "cn",
            Lang::En => "en",
        }
    }
}

/// 界面字符串：tr(lang, key)
fn tr(l: Lang, k: &str) -> &'static str {
    match (l, k) {
        (Lang::Cn, "win_title") => "No-Sleep-Agent 设置",
        (Lang::En, "win_title") => "No-Sleep-Agent Settings",
        (_, "app_title") => "No-Sleep-Agent-Tray",
        (Lang::Cn, "subtitle") => "防锁屏 · 防睡眠 · 保网络，AI Agent 稳定运行",
        (Lang::En, "subtitle") => "Anti-lock · Anti-sleep · Keep-online for AI agents",
        (Lang::Cn, "sec_protection") => "保持策略",
        (Lang::En, "sec_protection") => "Protection",
        (Lang::Cn, "chk_enabled") => "启用保持（防睡眠，AI agent 可正常运行）",
        (Lang::En, "chk_enabled") => "Enable protection (anti-sleep, AI agents stay awake)",
        (Lang::Cn, "chk_display") => "显示器常亮（防熄屏）",
        (Lang::En, "chk_display") => "Keep display on (no dim / off)",
        (Lang::Cn, "chk_away") => "AwayMode 保网络（笔记本不断网）",
        (Lang::En, "chk_away") => "AwayMode keep-network (no disconnects)",
        (Lang::Cn, "chk_jiggle") => "模拟 F15 按键（防公司 GPO 空闲锁屏）",
        (Lang::En, "chk_jiggle") => "Emulate F15 key (beats corporate GPO idle lock)",
        (Lang::Cn, "sec_intervals") => "间隔设置",
        (Lang::En, "sec_intervals") => "Intervals",
        (Lang::Cn, "lbl_interval") => "刷新间隔（秒，5~600）：",
        (Lang::En, "lbl_interval") => "Refresh interval (sec, 5-600):",
        (Lang::Cn, "lbl_jiggle") => "F15 心跳间隔（秒，30~3600）：",
        (Lang::En, "lbl_jiggle") => "F15 heartbeat (sec, 30-3600):",
        (Lang::Cn, "sec_system") => "系统",
        (Lang::En, "sec_system") => "System",
        (Lang::Cn, "chk_autostart") => "开机自启动",
        (Lang::En, "chk_autostart") => "Start on boot",
        (_, "chk_english") => "英文界面 / English UI",
        (Lang::Cn, "btn_apply") => "应用",
        (Lang::En, "btn_apply") => "Apply",
        (Lang::Cn, "btn_ok") => "确定",
        (Lang::En, "btn_ok") => "OK",
        (Lang::Cn, "btn_cancel") => "取消",
        (Lang::En, "btn_cancel") => "Cancel",
        (Lang::Cn, "err_title") => "参数错误",
        (Lang::En, "err_title") => "Invalid input",
        (Lang::Cn, "err_interval") => "刷新间隔必须是 5~600 的数字",
        (Lang::En, "err_interval") => "Refresh interval must be a number 5-600",
        (Lang::Cn, "err_jiggle") => "F15 心跳间隔必须是 30~3600 的数字",
        (Lang::En, "err_jiggle") => "Heartbeat must be a number 30-3600",
        (Lang::Cn, "err_autostart") => "开机自启设置失败（注册表写入被拒）",
        (Lang::En, "err_autostart") => "Autostart failed (registry write denied)",
        (Lang::Cn, "tip_title") => "提示",
        (Lang::En, "tip_title") => "Notice",
        // 状态行与托盘提示
        (Lang::Cn, "p_sleep") => "防睡眠",
        (Lang::En, "p_sleep") => "anti-sleep",
        (Lang::Cn, "p_display") => "常亮",
        (Lang::En, "p_display") => "display-on",
        (Lang::Cn, "p_away") => "保网络",
        (Lang::En, "p_away") => "keep-network",
        (Lang::Cn, "p_jiggle") => "F15防锁屏",
        (Lang::En, "p_jiggle") => "F15 anti-lock",
        // 控制台模式
        (Lang::Cn, "con_title") => "=== No-Sleep-Agent 控制台模式 ===",
        (Lang::En, "con_title") => "=== No-Sleep-Agent console mode ===",
        (Lang::Cn, "con_exit") => "按 Ctrl+C 退出。",
        (Lang::En, "con_exit") => "Press Ctrl+C to exit.",
        (Lang::Cn, "con_beat") => "F15 心跳，保持在线。",
        (Lang::En, "con_beat") => "F15 heartbeat, staying awake.",
        (Lang::Cn, "con_nodisp") => "允许熄屏",
        (Lang::En, "con_nodisp") => "display may sleep",
        (Lang::Cn, "con_disp") => "常亮",
        (Lang::En, "con_disp") => "display on",
        (Lang::Cn, "con_noaway") => "无AwayMode",
        (Lang::En, "con_noaway") => "no AwayMode",
        (Lang::Cn, "con_away") => "保网络",
        (Lang::En, "con_away") => "keep-network",
        (Lang::Cn, "con_nojig") => "无按键",
        (Lang::En, "con_nojig") => "no key",
        // 亮度结果
        (Lang::Cn, "bri_ddc") => "显示器 DDC",
        (Lang::En, "bri_ddc") => "monitor DDC",
        (Lang::Cn, "bri_wmi") => "笔记本面板 WMI",
        (Lang::En, "bri_wmi") => "laptop panel WMI",
        (Lang::Cn, "bri_scheme") => "系统电源方案",
        (Lang::En, "bri_scheme") => "power scheme",
        // 弹窗标题
        (Lang::Cn, "fatal_cap") => "No-Sleep-Agent 启动失败",
        (Lang::En, "fatal_cap") => "No-Sleep-Agent failed to start",
        (Lang::Cn, "fatal_gui") => "GUI 初始化失败：",
        (Lang::En, "fatal_gui") => "GUI init failed: ",
        (Lang::Cn, "fatal_tray") => "托盘 UI 构建失败：",
        (Lang::En, "fatal_tray") => "Tray UI build failed: ",
        (_, _) => "[missing text]",
    }
}

fn config_path() -> std::path::PathBuf {
    let base = env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    std::path::Path::new(&base).join("keepawake").join("config.ini")
}

fn load_config() -> Config {
    let mut cfg = Config::default();
    let Ok(text) = std::fs::read_to_string(config_path()) else {
        return cfg;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let (k, v) = (k.trim(), v.trim());
            match k {
                "enabled" => cfg.enabled = v == "1",
                "display" => cfg.display = v == "1",
                "away" => cfg.away = v == "1",
                "jiggle" => cfg.jiggle = v == "1",
                "interval" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.interval = n.clamp(5, 600);
                    }
                }
                "jiggle_interval" => {
                    if let Ok(n) = v.parse::<u64>() {
                        cfg.jiggle_interval = n.clamp(30, 3600);
                    }
                }
                "lang" => cfg.lang = Lang::parse(v),
                _ => {}
            }
        }
    }
    cfg
}

fn save_config(cfg: &Config) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let text = format!(
        "# no-sleep-agent-tray 配置（设置窗口修改后自动保存）\nenabled={}\ndisplay={}\naway={}\njiggle={}\ninterval={}\njiggle_interval={}\nlang={}\n",
        cfg.enabled as u8,
        cfg.display as u8,
        cfg.away as u8,
        cfg.jiggle as u8,
        cfg.interval,
        cfg.jiggle_interval,
        cfg.lang.code(),
    );
    let _ = std::fs::write(path, text);
}

// ---------- 开机自启（HKCU Run） ----------
const AUTOSTART_NAME: &str = "KeepAwake";

fn autostart_enabled() -> bool {
    use winreg::enums::*;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .ok()
        .and_then(|k| k.get_value::<String, _>(AUTOSTART_NAME).ok())
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

fn set_autostart(on: bool) -> bool {
    use winreg::enums::*;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let Ok(run) = hkcu.open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run",
        KEY_SET_VALUE,
    ) else {
        return false;
    };
    if on {
        let exe = env::current_exe()
            .map(|p| format!("\"{}\"", p.display()))
            .unwrap_or_default();
        run.set_value(AUTOSTART_NAME, &exe).is_ok()
    } else {
        // 不存在也算成功（幂等）
        match run.delete_value(AUTOSTART_NAME) {
            Ok(_) => true,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => false,
        }
    }
}

// ---------- 共享状态 ----------
struct Shared {
    cfg: Config,
    status: String,
    tip: String,
    seq: u64, // 每次状态变化 +1，UI 线程据此刷新
    balloon: Option<(String, String)>, // 待弹的气泡通知（标题，正文）
}

impl Shared {
    fn refresh_text(&mut self) {
        let l = self.cfg.lang;
        if !self.cfg.enabled {
            self.status = match l {
                Lang::Cn => "状态：已暂停（右键托盘 → 启用保持 可恢复）".to_string(),
                Lang::En => "Paused (right-click tray → Enable to resume)".to_string(),
            };
            self.tip = match l {
                Lang::Cn => "no-sleep-agent 已暂停".to_string(),
                Lang::En => "no-sleep-agent paused".to_string(),
            };
            return;
        }
        let mut parts: Vec<&str> = vec![tr(l, "p_sleep")];
        if self.cfg.display {
            parts.push(tr(l, "p_display"));
        }
        if self.cfg.away {
            parts.push(tr(l, "p_away"));
        }
        if self.cfg.jiggle {
            parts.push(tr(l, "p_jiggle"));
        }
        let joined = parts.join("+");
        match l {
            Lang::Cn => {
                self.status = format!(
                    "状态：运行中 [{}]  刷新{}s / 心跳{}s",
                    joined, self.cfg.interval, self.cfg.jiggle_interval
                );
                self.tip = format!("no-sleep-agent 运行中 [{}]", joined);
            }
            Lang::En => {
                self.status = format!(
                    "Running [{}]  refresh {}s / heartbeat {}s",
                    joined, self.cfg.interval, self.cfg.jiggle_interval
                );
                self.tip = format!("no-sleep-agent running [{}]", joined);
            }
        }
    }
}

// ---------- 后台保活线程 ----------
fn spawn_worker(shared: Arc<Mutex<Shared>>, sender: nwg::NoticeSender) {
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        let mut jt: u64 = 0;
        let mut last_cfg: Option<Config> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let snapshot = {
                let s = shared.lock().unwrap();
                s.cfg.clone()
            };
            // 配置变化则重置计数并立即生效
            if last_cfg.as_ref().is_some_and(|c| {
                c.interval != snapshot.interval
                    || c.jiggle_interval != snapshot.jiggle_interval
                    || c.enabled != snapshot.enabled
                    || c.display != snapshot.display
                    || c.away != snapshot.away
                    || c.jiggle != snapshot.jiggle
            }) {
                tick = 0;
                jt = 0;
            }
            last_cfg = Some(snapshot.clone());

            if !snapshot.enabled {
                continue;
            }
            tick += 1;
            let mut changed = false;
            {
                let mut s = shared.lock().unwrap();
                if tick % snapshot.interval == 0 {
                    set_awake(snapshot.display, snapshot.away);
                    s.status = format!("{}  （{} 刷新）", s.status, stamp());
                    s.seq += 1;
                    changed = true;
                }
                drop(s);
            }
            if snapshot.jiggle {
                jt += 1;
                if jt >= snapshot.jiggle_interval {
                    jt = 0;
                    jiggle_f15();
                    let mut s = shared.lock().unwrap();
                    s.status = format!("{}  （{} F15心跳）", s.status, stamp());
                    // 防止状态行无限变长
                    if s.status.len() > 400 {
                        s.refresh_text();
                    }
                    s.seq += 1;
                    changed = true;
                }
            }
            if changed {
                sender.notice();
            }
        }
    });
}

// ---------- UI ----------
static ICON_ON: &[u8] = include_bytes!("../assets/icon_on.ico");
static ICON_OFF: &[u8] = include_bytes!("../assets/icon_off.ico");

#[derive(Default)]
struct App {
    msg_window: nwg::MessageWindow,
    icon_on: nwg::Icon,
    icon_off: nwg::Icon,
    tray: nwg::TrayNotification,
    tray_menu: nwg::Menu,
    mi_enabled: nwg::MenuItem,
    mi_settings: nwg::MenuItem,
    mi_autostart: nwg::MenuItem,
    mi_brightness50: nwg::MenuItem,
    mi_brightness15: nwg::MenuItem,
    mi_sep1: nwg::MenuSeparator,
    mi_sep2: nwg::MenuSeparator,
    mi_exit: nwg::MenuItem,
    notice: nwg::Notice,

    win: nwg::Window,
    font_title: nwg::Font,
    font_section: nwg::Font,
    lbl_title: nwg::Label,
    lbl_subtitle: nwg::Label,
    lbl_sec1: nwg::Label,
    chk_enabled: nwg::CheckBox,
    chk_display: nwg::CheckBox,
    chk_away: nwg::CheckBox,
    chk_jiggle: nwg::CheckBox,
    chk_autostart: nwg::CheckBox,
    chk_english: nwg::CheckBox,
    lbl_interval: nwg::Label,
    txt_interval: nwg::TextInput,
    lbl_jiggle: nwg::Label,
    txt_jiggle: nwg::TextInput,
    lbl_sec2: nwg::Label,
    lbl_sec3: nwg::Label,
    lbl_status: nwg::Label,
    btn_apply: nwg::Button,
    btn_ok: nwg::Button,
    btn_cancel: nwg::Button,

    shared: Arc<Mutex<Shared>>,
    last_seq: std::cell::Cell<u64>,
    handlers: std::cell::RefCell<Vec<nwg::EventHandler>>,
}

impl Default for Shared {
    fn default() -> Self {
        let mut s = Self {
            cfg: Config::default(),
            status: String::new(),
            tip: String::new(),
            seq: 0,
            balloon: None,
        };
        s.refresh_text();
        s
    }
}

fn check_state(b: bool) -> nwg::CheckBoxState {
    if b {
        nwg::CheckBoxState::Checked
    } else {
        nwg::CheckBoxState::Unchecked
    }
}

fn is_checked(c: &nwg::CheckBox) -> bool {
    c.check_state() == nwg::CheckBoxState::Checked
}

impl App {
    fn build(mut self) -> Result<std::rc::Rc<Self>, nwg::NwgError> {
        use std::rc::Rc;
        let cfg = self.shared.lock().unwrap().cfg.clone();

        // 图标：内嵌绿色/灰色圆点，失败则回退系统图标
        if nwg::Icon::builder()
            .source_bin(Some(ICON_ON))
            .build(&mut self.icon_on)
            .is_err()
        {
            nwg::Icon::builder()
                .source_system(Some(nwg::OemIcon::Information))
                .build(&mut self.icon_on)?;
        }
        if nwg::Icon::builder()
            .source_bin(Some(ICON_OFF))
            .build(&mut self.icon_off)
            .is_err()
        {
            nwg::Icon::builder()
                .source_system(Some(nwg::OemIcon::Information))
                .build(&mut self.icon_off)?;
        }

        nwg::MessageWindow::builder().build(&mut self.msg_window)?;

        let tip = self.shared.lock().unwrap().tip.clone();
        nwg::TrayNotification::builder()
            .parent(&self.msg_window)
            .icon(Some(if cfg.enabled {
                &self.icon_on
            } else {
                &self.icon_off
            }))
            .tip(Some(&tip))
            .build(&mut self.tray)?;

        nwg::Menu::builder()
            .popup(true)
            .parent(&self.msg_window)
            .build(&mut self.tray_menu)?;
        // 托盘右键菜单：MenuItem 不支持运行时改名，这里永久双语，一劳永逸
        nwg::MenuItem::builder()
            .text("启用保持 / Enable")
            .check(cfg.enabled)
            .parent(&self.tray_menu)
            .build(&mut self.mi_enabled)?;
        nwg::MenuSeparator::builder()
            .parent(&self.tray_menu)
            .build(&mut self.mi_sep1)?;
        nwg::MenuItem::builder()
            .text("设置... / Settings...")
            .parent(&self.tray_menu)
            .build(&mut self.mi_settings)?;
        nwg::MenuItem::builder()
            .text("开机自启 / Start on boot")
            .check(autostart_enabled())
            .parent(&self.tray_menu)
            .build(&mut self.mi_autostart)?;
        nwg::MenuItem::builder()
            .text("亮度降到一半 / Brightness 50%")
            .parent(&self.tray_menu)
            .build(&mut self.mi_brightness50)?;
        nwg::MenuItem::builder()
            .text("亮度调到 15% / Brightness 15%")
            .parent(&self.tray_menu)
            .build(&mut self.mi_brightness15)?;
        nwg::MenuSeparator::builder()
            .parent(&self.tray_menu)
            .build(&mut self.mi_sep2)?;
        nwg::MenuItem::builder()
            .text("退出 / Exit")
            .parent(&self.tray_menu)
            .build(&mut self.mi_exit)?;

        nwg::Notice::builder()
            .parent(&self.msg_window)
            .build(&mut self.notice)?;

        let lang = cfg.lang;

        // 字体：标题 / 分区头
        nwg::Font::builder()
            .family("Segoe UI")
            .size(17)
            .weight(700)
            .build(&mut self.font_title)?;
        nwg::Font::builder()
            .family("Segoe UI")
            .size(12)
            .weight(700)
            .build(&mut self.font_section)?;

        // 设置窗口（默认隐藏）
        nwg::Window::builder()
            .size((496, 492))
            .position((400, 200))
            .title(tr(lang, "win_title"))
            .flags(nwg::WindowFlags::WINDOW)
            .build(&mut self.win)?;

        // 顶栏：产品标题 + 一句话副标题
        nwg::Label::builder()
            .text(tr(lang, "app_title"))
            .font(Some(&self.font_title))
            .size((464, 30))
            .position((16, 10))
            .parent(&self.win)
            .build(&mut self.lbl_title)?;
        nwg::Label::builder()
            .text(tr(lang, "subtitle"))
            .size((464, 22))
            .position((16, 40))
            .parent(&self.win)
            .build(&mut self.lbl_subtitle)?;

        // 分区一：保持策略
        nwg::Label::builder()
            .text(tr(lang, "sec_protection"))
            .font(Some(&self.font_section))
            .size((464, 24))
            .position((16, 68))
            .parent(&self.win)
            .build(&mut self.lbl_sec1)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_enabled"))
            .check_state(check_state(cfg.enabled))
            .size((440, 24))
            .position((28, 94))
            .parent(&self.win)
            .build(&mut self.chk_enabled)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_display"))
            .check_state(check_state(cfg.display))
            .size((440, 24))
            .position((28, 120))
            .parent(&self.win)
            .build(&mut self.chk_display)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_away"))
            .check_state(check_state(cfg.away))
            .size((440, 24))
            .position((28, 146))
            .parent(&self.win)
            .build(&mut self.chk_away)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_jiggle"))
            .check_state(check_state(cfg.jiggle))
            .size((440, 24))
            .position((28, 172))
            .parent(&self.win)
            .build(&mut self.chk_jiggle)?;

        // 分区二：间隔
        nwg::Label::builder()
            .text(tr(lang, "sec_intervals"))
            .font(Some(&self.font_section))
            .size((464, 24))
            .position((16, 204))
            .parent(&self.win)
            .build(&mut self.lbl_sec2)?;
        nwg::Label::builder()
            .text(tr(lang, "lbl_interval"))
            .h_align(nwg::HTextAlign::Left)
            .size((240, 24))
            .position((28, 230))
            .parent(&self.win)
            .build(&mut self.lbl_interval)?;
        nwg::TextInput::builder()
            .text(&cfg.interval.to_string())
            .size((180, 26))
            .position((272, 230))
            .parent(&self.win)
            .build(&mut self.txt_interval)?;

        nwg::Label::builder()
            .text(tr(lang, "lbl_jiggle"))
            .h_align(nwg::HTextAlign::Left)
            .size((240, 24))
            .position((28, 258))
            .parent(&self.win)
            .build(&mut self.lbl_jiggle)?;
        nwg::TextInput::builder()
            .text(&cfg.jiggle_interval.to_string())
            .size((180, 26))
            .position((272, 258))
            .parent(&self.win)
            .build(&mut self.txt_jiggle)?;

        // 分区三：系统
        nwg::Label::builder()
            .text(tr(lang, "sec_system"))
            .font(Some(&self.font_section))
            .size((464, 24))
            .position((16, 290))
            .parent(&self.win)
            .build(&mut self.lbl_sec3)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_autostart"))
            .check_state(check_state(autostart_enabled()))
            .size((440, 24))
            .position((28, 314))
            .parent(&self.win)
            .build(&mut self.chk_autostart)?;
        nwg::CheckBox::builder()
            .text(tr(lang, "chk_english"))
            .check_state(check_state(cfg.lang == Lang::En))
            .size((440, 24))
            .position((28, 340))
            .parent(&self.win)
            .build(&mut self.chk_english)?;

        // 状态行
        let status = self.shared.lock().unwrap().status.clone();
        nwg::Label::builder()
            .text(&status)
            .h_align(nwg::HTextAlign::Left)
            .size((464, 60))
            .position((16, 370))
            .parent(&self.win)
            .build(&mut self.lbl_status)?;

        // 底部按钮
        nwg::Button::builder()
            .text(tr(lang, "btn_apply"))
            .size((100, 32))
            .position((90, 440))
            .parent(&self.win)
            .build(&mut self.btn_apply)?;
        nwg::Button::builder()
            .text(tr(lang, "btn_ok"))
            .size((100, 32))
            .position((198, 440))
            .parent(&self.win)
            .build(&mut self.btn_ok)?;
        nwg::Button::builder()
            .text(tr(lang, "btn_cancel"))
            .size((100, 32))
            .position((306, 440))
            .parent(&self.win)
            .build(&mut self.btn_cancel)?;

        let rc = Rc::new(self);
        // 关键：设置窗口是独立顶层窗口，按钮事件发往它自己，
        // 必须在两个窗口上都绑定，否则设置页里的按钮/关闭按了没反应。
        let h1 = {
            let weak = Rc::downgrade(&rc);
            nwg::full_bind_event_handler(
                &rc.msg_window.handle,
                move |evt, _data: nwg::EventData, handle: nwg::ControlHandle| {
                    if let Some(app) = weak.upgrade() {
                        app.on_event(evt, handle);
                    }
                },
            )
        };
        let h2 = {
            let weak = Rc::downgrade(&rc);
            nwg::full_bind_event_handler(
                &rc.win.handle,
                move |evt, _data: nwg::EventData, handle: nwg::ControlHandle| {
                    if let Some(app) = weak.upgrade() {
                        app.on_event(evt, handle);
                    }
                },
            )
        };
        rc.handlers.replace(vec![h1, h2]);

        // 后台线程
        spawn_worker(rc.shared.clone(), rc.notice.sender());

        Ok(rc)
    }

    fn sync_tray(&self) {
        let s = self.shared.lock().unwrap();
        self.tray.set_tip(&s.tip);
        if s.cfg.enabled {
            self.tray.set_icon(&self.icon_on);
        } else {
            self.tray.set_icon(&self.icon_off);
        }
        self.mi_enabled.set_checked(s.cfg.enabled);
        self.lbl_status.set_text(&s.status);
    }

    /// 把设置窗口的控件同步为当前配置（打开时调用；取消时调用可丢弃未保存的修改）
    fn sync_controls(&self) {
        let cfg = self.shared.lock().unwrap().cfg.clone();
        self.chk_enabled.set_check_state(check_state(cfg.enabled));
        self.chk_display.set_check_state(check_state(cfg.display));
        self.chk_away.set_check_state(check_state(cfg.away));
        self.chk_jiggle.set_check_state(check_state(cfg.jiggle));
        self.txt_interval.set_text(&cfg.interval.to_string());
        self.txt_jiggle.set_text(&cfg.jiggle_interval.to_string());
        self.chk_autostart
            .set_check_state(check_state(autostart_enabled()));
        self.chk_english
            .set_check_state(check_state(cfg.lang == Lang::En));
        self.lbl_status
            .set_text(&self.shared.lock().unwrap().status);
    }

    /// 整窗语言切换：先重算状态文本，再把所有控件文本按当前语言重写
    fn apply_lang(&self) {
        let lang = {
            let mut s = self.shared.lock().unwrap();
            s.refresh_text();
            s.seq += 1;
            s.cfg.lang
        };
        self.win.set_text(tr(lang, "win_title"));
        self.lbl_title.set_text(tr(lang, "app_title"));
        self.lbl_subtitle.set_text(tr(lang, "subtitle"));
        self.lbl_sec1.set_text(tr(lang, "sec_protection"));
        self.lbl_sec2.set_text(tr(lang, "sec_intervals"));
        self.lbl_sec3.set_text(tr(lang, "sec_system"));
        self.chk_enabled.set_text(tr(lang, "chk_enabled"));
        self.chk_display.set_text(tr(lang, "chk_display"));
        self.chk_away.set_text(tr(lang, "chk_away"));
        self.chk_jiggle.set_text(tr(lang, "chk_jiggle"));
        self.lbl_interval.set_text(tr(lang, "lbl_interval"));
        self.lbl_jiggle.set_text(tr(lang, "lbl_jiggle"));
        self.chk_autostart.set_text(tr(lang, "chk_autostart"));
        self.chk_english.set_text(tr(lang, "chk_english"));
        self.btn_apply.set_text(tr(lang, "btn_apply"));
        self.btn_ok.set_text(tr(lang, "btn_ok"));
        self.btn_cancel.set_text(tr(lang, "btn_cancel"));
        self.sync_tray();
    }

    fn show_settings(&self) {
        self.sync_controls();
        self.win.set_visible(true);
        self.win.set_focus();
    }

    fn apply_settings(&self) -> bool {
        let lang = self.shared.lock().unwrap().cfg.lang;
        let interval: u64 = match self.txt_interval.text().trim().parse() {
            Ok(n) if (5..=600).contains(&n) => n,
            _ => {
                nwg::modal_info_message(
                    &self.msg_window,
                    tr(lang, "err_title"),
                    tr(lang, "err_interval"),
                );
                return false;
            }
        };
        let jiggle_interval: u64 = match self.txt_jiggle.text().trim().parse() {
            Ok(n) if (30..=3600).contains(&n) => n,
            _ => {
                nwg::modal_info_message(
                    &self.msg_window,
                    tr(lang, "err_title"),
                    tr(lang, "err_jiggle"),
                );
                return false;
            }
        };
        let cfg = Config {
            enabled: is_checked(&self.chk_enabled),
            display: is_checked(&self.chk_display),
            away: is_checked(&self.chk_away),
            jiggle: is_checked(&self.chk_jiggle),
            interval,
            jiggle_interval,
            lang, // 语言由专用勾选框即时切换，这里保持不变
        };
        let autostart = is_checked(&self.chk_autostart);
        if !set_autostart(autostart) {
            nwg::modal_info_message(
                &self.msg_window,
                tr(lang, "tip_title"),
                tr(lang, "err_autostart"),
            );
        }
        {
            let mut s = self.shared.lock().unwrap();
            let was_enabled = s.cfg.enabled;
            s.cfg = cfg.clone();
            s.refresh_text();
            s.seq += 1;
            if !cfg.enabled && was_enabled {
                clear_awake();
            } else if cfg.enabled {
                set_awake(cfg.display, cfg.away);
            }
        }
        self.mi_autostart.set_checked(autostart_enabled());
        save_config(&cfg);
        self.sync_tray();
        true
    }

    /// 调亮度可能耗时几百毫秒，放后台线程做，做完弹气泡通知
    fn brightness_async(&self, level: u32) {
        let lang = self.shared.lock().unwrap().cfg.lang;
        let shared = self.shared.clone();
        let sender = self.notice.sender();
        std::thread::spawn(move || {
            let msg = set_brightness(level, lang);
            let title = match lang {
                Lang::Cn => format!("亮度 {}%", level),
                Lang::En => format!("Brightness {}%", level),
            };
            let mut s = shared.lock().unwrap();
            s.balloon = Some((title, msg));
            s.seq += 1;
            sender.notice();
        });
    }

    fn toggle_enabled(&self) {        {
            let mut s = self.shared.lock().unwrap();
            s.cfg.enabled = !s.cfg.enabled;
            s.refresh_text();
            s.seq += 1;
            if s.cfg.enabled {
                set_awake(s.cfg.display, s.cfg.away);
            } else {
                clear_awake();
            }
        }
        save_config(&self.shared.lock().unwrap().cfg);
        self.sync_tray();
    }

    fn on_event(&self, evt: nwg::Event, handle: nwg::ControlHandle) {
        use nwg::Event as E;
        match evt {
            E::OnContextMenu => {
                if &handle == &self.tray {
                    let (x, y) = nwg::GlobalCursor::position();
                    self.tray_menu.popup(x, y);
                }
            }
            E::OnMenuItemSelected => {
                if &handle == &self.mi_enabled {
                    self.toggle_enabled();
                    // 同步设置窗口里的复选框
                    let on = self.shared.lock().unwrap().cfg.enabled;
                    self.chk_enabled.set_check_state(check_state(on));
                } else if &handle == &self.mi_settings {
                    self.show_settings();
                } else if &handle == &self.mi_autostart {
                    let target = !autostart_enabled();
                    if set_autostart(target) {
                        self.mi_autostart.set_checked(target);
                        self.chk_autostart.set_check_state(check_state(target));
                    }
                } else if &handle == &self.mi_brightness50 {
                    self.brightness_async(50);
                } else if &handle == &self.mi_brightness15 {
                    self.brightness_async(15);
                } else if &handle == &self.mi_exit {
                    clear_awake();
                    nwg::stop_thread_dispatch();
                }
            }
            E::OnButtonClick => {
                if &handle == &self.btn_apply {
                    self.apply_settings();
                } else if &handle == &self.btn_ok {
                    if self.apply_settings() {
                        self.win.set_visible(false);
                    }
                } else if &handle == &self.btn_cancel {
                    // 取消：丢弃未保存的修改并隐藏窗口
                    self.sync_controls();
                    self.win.set_visible(false);
                } else if &handle == &self.chk_english {
                    // 语言勾选即时生效并保存
                    let en = is_checked(&self.chk_english);
                    {
                        let mut s = self.shared.lock().unwrap();
                        s.cfg.lang = if en { Lang::En } else { Lang::Cn };
                    }
                    save_config(&self.shared.lock().unwrap().cfg);
                    self.apply_lang();
                }
            }
            E::OnWindowClose => {
                if &handle == &self.win {
                    // 设置窗口点 X 只隐藏（顺带丢弃未保存修改），托盘继续运行
                    self.sync_controls();
                    self.win.set_visible(false);
                }
            }
            E::OnNotice => {
                if &handle == &self.notice {
                    // 先弹气泡通知（亮度调节结果等）
                    let balloon = {
                        let mut s = self.shared.lock().unwrap();
                        s.balloon.take()
                    };
                    if let Some((title, msg)) = balloon {
                        let flags = nwg::TrayNotificationFlags::USER_ICON
                            | nwg::TrayNotificationFlags::LARGE_ICON;
                        self.tray.show(
                            &title,
                            Some(&msg),
                            Some(flags),
                            Some(&self.icon_on),
                        );
                    }
                    let seq = self.shared.lock().unwrap().seq;
                    if seq != self.last_seq.get() {
                        self.last_seq.set(seq);
                        self.sync_tray();
                    }
                }
            }
            _ => {}
        }
    }
}

// ---------- 纯控制台模式（--no-tray，给无桌面/AI agent 服务器用） ----------
fn run_console(cfg: Config) {
    let l = cfg.lang;
    println!("{}", tr(l, "con_title"));
    println!(
        "{}: {} + {} + {}，刷新{}s / refresh {}s",
        if l == Lang::Cn { "策略" } else { "Policy" },
        if cfg.display {
            tr(l, "con_disp")
        } else {
            tr(l, "con_nodisp")
        },
        if cfg.away {
            tr(l, "con_away")
        } else {
            tr(l, "con_noaway")
        },
        if cfg.jiggle {
            format!("F15 {}s", cfg.jiggle_interval)
        } else {
            tr(l, "con_nojig").to_string()
        },
        cfg.interval,
        cfg.interval
    );
    println!("{}", tr(l, "con_exit"));
    set_awake(cfg.display, cfg.away);
    let mut elapsed = 0u64;
    loop {
        std::thread::sleep(Duration::from_secs(cfg.interval));
        if !cfg.enabled {
            continue;
        }
        set_awake(cfg.display, cfg.away);
        elapsed += cfg.interval;
        if cfg.jiggle && elapsed >= cfg.jiggle_interval {
            elapsed = 0;
            jiggle_f15();
            println!("[{}] {}", stamp(), tr(cfg.lang, "con_beat"));
        }
    }
}

fn print_help() {
    println!(
        r#"no-sleep-agent-tray 2.1 - Windows 防锁屏/防睡眠小工具（托盘版）
  No-sleep tray tool: anti-lock screen, anti-sleep, keep network online.

用法 / Usage:
  keepawake.exe [选项 options]

托盘模式（默认）/ Tray mode (default):
  右键托盘 → 设置... / Right-click tray → Settings...
    启用保持 / 显示器常亮 / AwayMode保网络 / F15防锁屏
    刷新间隔 / F15心跳间隔 / 开机自启 / 中英切换
    Enable / Display on / AwayMode / F15 / Intervals / Autostart / CN-EN
  右键菜单 / Tray menu: 启用保持Enable / 设置Settings / 开机自启Autostart / 亮度50%15% / 退出Exit
  配置自动存 %APPDATA%\keepawake\config.ini (含 lang=cn|en)

选项 / Options:
  --no-tray                  纯控制台模式（无桌面环境用）/ console only
  --console                  托盘模式下保留控制台窗口 / keep console
  --brightness-50            直接把亮度降到一半并退出 / set brightness 50%
  --brightness-15            直接把亮度调到 15% 并退出 / set brightness 15%
  --show-settings            启动时直接打开设置窗口 / open settings at start
  --lang <cn|en>             界面语言 / UI language
  --interval <秒>            覆盖刷新间隔（5~600）
  --jiggle-interval <秒>     覆盖心跳间隔（30~3600）
  --no-display / --no-away / --no-jiggle / --paused   覆盖对应开关
  -h, --help                 显示帮助 / show help
"#
    );
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut cfg = load_config();
    let mut no_tray = false;
    let mut console = false;
    let mut show_settings = false;

    let mut i = 1;
    let mut wants_output = false;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                ensure_console();
                print_help();
                return;
            }
            "--no-tray" => {
                no_tray = true;
                wants_output = true;
            }
            "--console" => {
                console = true;
                wants_output = true;
            }
            "--brightness-50" => {
                ensure_console();
                println!("{}", set_brightness(50, cfg.lang));
                return;
            }
            "--brightness-15" => {
                ensure_console();
                println!("{}", set_brightness(15, cfg.lang));
                return;
            }
            "--lang" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    cfg.lang = Lang::parse(v);
                }
            }
            "--show-settings" => {
                show_settings = true;
            }
            "--no-display" => cfg.display = false,
            "--no-away" => cfg.away = false,
            "--no-jiggle" => cfg.jiggle = false,
            "--paused" => cfg.enabled = false,
            "--interval" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse::<u64>().ok()) {
                    cfg.interval = v.clamp(5, 600);
                }
            }
            "--jiggle-interval" => {
                i += 1;
                if let Some(v) = args.get(i).and_then(|s| s.parse::<u64>().ok()) {
                    cfg.jiggle_interval = v.clamp(30, 3600);
                }
            }
            other => {
                eprintln!("未知参数: {}，用 --help 查看", other);
                std::process::exit(2);
            }
        }
        i += 1;
    }

    if no_tray {
        if wants_output {
            ensure_console();
        }
        run_console(cfg);
        return;
    }

    // 首次运行落盘一份默认配置，方便直接改文件
    if !config_path().exists() {
        save_config(&cfg);
    }

    if console {
        ensure_console();
    }

    if let Err(e) = nwg::init() {
        fatal_popup(&format!("{}{}", tr(cfg.lang, "fatal_gui"), e));
    }
    let shared = Arc::new(Mutex::new(Shared {
        status: String::new(),
        tip: String::new(),
        seq: 0,
        balloon: None,
        cfg: cfg.clone(),
    }));
    shared.lock().unwrap().refresh_text();
    if cfg.enabled {
        set_awake(cfg.display, cfg.away);
    }

    let app = match (App {
        shared,
        ..Default::default()
    })
    .build()
    {
        Ok(app) => app,
        Err(e) => fatal_popup(&format!("{}{}", tr(cfg.lang, "fatal_tray"), e)),
    };

    if show_settings {
        app.show_settings();
    }

    nwg::dispatch_thread_events();
    clear_awake();
}
