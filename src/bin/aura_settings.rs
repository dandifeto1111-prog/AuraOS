#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

//! AuraSettings v4.3 — Native Lightweight Pure-Rust Direct2D Replacement for Windows Settings!
//! - Replaces heavy WinUI/XAML Windows Settings with an instant ~15 MB GPU settings hub
//! - 8 Real Live Categories:
//!   1. Sistema y Energía (CPU/RAM/Discos, Plan de energía real powercfg, Animaciones Windows, Limpieza %TEMP%)
//!   2. Pantalla (Resolución y Hz reales, Cambio de Hz en vivo ChangeDisplaySettingsW, Modo Oscuro, Transparencia)
//!   3. Sonido y Mezclador (Volumen maestro, Salida, Micrófono, Mezclador real por aplicación)
//!   4. Red y Bluetooth (Ethernet/Wi-Fi señal, Radios WinRT ON/OFF, DNS Cloudflare/Google/DHCP, Flush DNS)
//!   5. Personalización AuraOS (Dock, Widgets, Opacidad Glass, Barra Windows — aplica en vivo sin reiniciar!)
//!   6. Fondos y AuraLock (Galería de fondos para Escritorio y AuraLock, PIN SHA-256 o contraseña Windows)
//!   7. Apps e Inicio (Gestor real de apps en HKCU\Run, Suite AuraOS, APBOptimizer)
//!   8. Actualizaciones y GitHub (Auto-actualización, URL repo GitHub, Emergencia restaurar Windows)
//! - Unified macOS Traffic Lights (× − +), Re-entrant safe fullscreen (`deferred_max`), Clean `WM_CLOSE`!

use aura_os::audio;
use aura_os::gradient_waves;
use aura_os::holo_mosaic;
use aura_os::aura_window::{self, SnapZone};
use aura_os::models::{
    app_data_dir, hash_lock_pin, load_custom_items, load_settings, load_widgets, save_custom_items,
    save_settings, save_widgets, AudioAppSession, AudioDevice, CustomEntry, DriveInfo,
    ShellSettings, WallpaperItem, WidgetKind, WidgetsStore,
};
use aura_os::radios::{self, RadioStates};
use aura_os::renderer::{DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::system;
use aura_os::theme::{self, VectorIcon};
use aura_os::video_decoder;
use aura_os::wallpaper;
use aura_os::win32_shell;
use std::cell::RefCell;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    ChangeDisplaySettingsW, EnumDisplaySettingsW, CDS_UPDATEREGISTRY, DEVMODEW, DM_DISPLAYFREQUENCY,
    ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_MODE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD, REG_SZ,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_BACK, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    IsZoomed, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer,
    ShowWindow, SystemParametersInfoW, TranslateMessage, HTCAPTION, IDC_ARROW, MSG,
    SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETDRAGFULLWINDOWS, SPI_SETDRAGFULLWINDOWS, SW_HIDE,
    SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, SW_SHOW, WM_CHAR, WM_CLOSE, WM_DESTROY,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN, WM_RBUTTONDOWN, WM_SIZE,
    WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TOP_H: f32 = 46.0;
const SB_W: f32 = 226.0;
const TIMER_POLL: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    System,
    Display,
    Sound,
    Network,
    Personalize,
    LockAndWallpapers,
    StartupAndApps,
    Updates,
}

impl Tab {
    const ALL: [Tab; 8] = [
        Tab::System,
        Tab::Display,
        Tab::Sound,
        Tab::Network,
        Tab::Personalize,
        Tab::LockAndWallpapers,
        Tab::StartupAndApps,
        Tab::Updates,
    ];

    fn label(self) -> &'static str {
        match self {
            Tab::System => "Sistema y Energía",
            Tab::Display => "Pantalla",
            Tab::Sound => "Sonido y Mezclador",
            Tab::Network => "Red y Bluetooth",
            Tab::Personalize => "Personalización",
            Tab::LockAndWallpapers => "Fondos y AuraLock",
            Tab::StartupAndApps => "Apps e Inicio",
            Tab::Updates => "Actualizaciones",
        }
    }

    fn icon(self) -> VectorIcon {
        match self {
            Tab::System => VectorIcon::Settings,
            Tab::Display => VectorIcon::Sun,
            Tab::Sound => VectorIcon::Volume,
            Tab::Network => VectorIcon::Wifi,
            Tab::Personalize => VectorIcon::Sparkle,
            Tab::LockAndWallpapers => VectorIcon::Lock,
            Tab::StartupAndApps => VectorIcon::AppGrid,
            Tab::Updates => VectorIcon::Refresh,
        }
    }

    fn from_arg(s: &str) -> Option<Tab> {
        match s.to_lowercase().as_str() {
            "system" | "sistema" => Some(Tab::System),
            "display" | "pantalla" => Some(Tab::Display),
            "sound" | "sonido" | "audio" => Some(Tab::Sound),
            "network" | "red" => Some(Tab::Network),
            "personalize" | "personalizacion" => Some(Tab::Personalize),
            "lock" | "auralock" | "fondos" => Some(Tab::LockAndWallpapers),
            "apps" | "startup" | "inicio" => Some(Tab::StartupAndApps),
            "updates" | "github" => Some(Tab::Updates),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum SetAct {
    WinClose,
    WinMin,
    WinMax,
    SelectTab(Tab),
    // System
    SetPowerPlan(&'static str),
    ToggleWinAnimations,
    CleanTempFiles,
    OpenDiskAnalyzer,
    // Display
    SetRefreshHz(u32),
    ToggleWinDarkMode,
    ToggleWinTransparency,
    SetGlassOpacity(f32),
    // Sound
    ToggleMasterMute,
    SetMasterVol(i32),
    SelectOutput(String),
    SelectMic(String),
    ToggleAppMute(Vec<u32>, bool),
    SetAppVol(Vec<u32>, i32),
    RefreshAudio,
    // Network
    ToggleWifi,
    ToggleBluetooth,
    ToggleAirplaneMode,
    ToggleHotspot,
    FocusHotspotSsid,
    FocusHotspotPass,
    SaveHotspotConfig,
    ScanWifiNetworks,
    ConnectWifiSaved(String),
    PromptWifiPassword(String),
    FocusWifiPassInput,
    SubmitWifiPassword,
    CancelWifiPassword,
    DisconnectWifi,
    ForgetWifi(String),
    ToggleVpn(String, bool),
    TestPing,
    RenewIp,
    FlushDns,
    SetDnsPreset(&'static str),
    OpenWinNetworkSettings,
    // Personalize
    ToggleShellSetting(&'static str),
    SetThemeScheme(&'static str),
    SetBarStyle(&'static str),
    SetClockStyle(&'static str),
    ToggleWidget(WidgetKind),
    FocusCityInput,
    SaveWeatherCity,
    ClearWeatherCity,
    // Wallpapers & AuraLock
    SetDesktopWallpaper(String),
    SetLockWallpaper(String),
    SetWavesCustomColor(String),
    SetHoloFilter(String),
    PickLocalLockWallpaper,
    PickLocalDesktopWallpaper,
    PickLockAvatar,
    ClearLockAvatar,
    FocusPinInput,
    SaveLockPin,
    ClearLockPin,
    TestAuraLock,
    // Startup & Apps
    RemoveStartupEntry(String),
    UninstallProgram(String),
    LaunchSuiteExe(&'static str),
    LaunchApbOptimizer,
    // Updates
    FocusGithubUrl,
    SaveGithubUrl,
    LaunchInstaller,
    OpenClassicWinSettings,
    RestoreWindowsTaskbar,
}

#[derive(Clone)]
enum SliderKind {
    MasterVol { x: f32, w: f32 },
    AppVol { x: f32, w: f32, pids: Vec<u32> },
    GlassOpacity { x: f32, w: f32 },
    WavesSpeed { x: f32, w: f32 },
    WavesRelief { x: f32, w: f32 },
    HoloSpeed { x: f32, w: f32 },
    HoloSaturation { x: f32, w: f32 },
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: SetAct,
    slider: Option<SliderKind>,
}

#[derive(Clone)]
struct StartupEntry {
    name: String,
    cmd: String,
}

enum BgUpdate {
    Audio {
        master_vol: i32,
        master_muted: bool,
        outputs: Vec<AudioDevice>,
        mics: Vec<AudioDevice>,
        apps: Vec<AudioAppSession>,
    },
    Radios(RadioStates),
    NetworkFull {
        radios: RadioStates,
        details: radios::NetworkDetails,
        wifis: Vec<radios::WifiNetwork>,
        vpns: Vec<radios::VpnProfile>,
    },
    PingMs(Option<u32>),
    PowerPlan(String),
    TempBytes(u64),
    Toast(String, bool),
}

// ==================== REAL WINDOWS SYSTEM QUERIES & CONTROLS ====================

fn get_display_info() -> (u32, u32, u32, Vec<u32>) {
    let mut cur = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    let mut cur_w = 1920u32;
    let mut cur_h = 1080u32;
    let mut cur_hz = 60u32;
    let mut rates: Vec<u32> = Vec::new();
    unsafe {
        if EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut cur).as_bool() {
            cur_w = cur.dmPelsWidth;
            cur_h = cur.dmPelsHeight;
            cur_hz = cur.dmDisplayFrequency;
        }
        let mut idx = 0u32;
        loop {
            let mut dm = DEVMODEW {
                dmSize: std::mem::size_of::<DEVMODEW>() as u16,
                ..Default::default()
            };
            if !EnumDisplaySettingsW(PCWSTR::null(), ENUM_DISPLAY_SETTINGS_MODE(idx), &mut dm).as_bool() {
                break;
            }
            if dm.dmPelsWidth == cur_w && dm.dmPelsHeight == cur_h && dm.dmDisplayFrequency >= 50 {
                if !rates.contains(&dm.dmDisplayFrequency) {
                    rates.push(dm.dmDisplayFrequency);
                }
            }
            idx += 1;
            if idx > 1024 {
                break;
            }
        }
    }
    rates.sort();
    (cur_w, cur_h, cur_hz, rates)
}

fn apply_refresh_hz(hz: u32) -> bool {
    let mut dm = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    unsafe {
        if !EnumDisplaySettingsW(PCWSTR::null(), ENUM_CURRENT_SETTINGS, &mut dm).as_bool() {
            return false;
        }
        dm.dmDisplayFrequency = hz;
        dm.dmFields = DM_DISPLAYFREQUENCY;
        ChangeDisplaySettingsW(Some(&dm), CDS_UPDATEREGISTRY).0 == 0
    }
}

fn read_hkcu_dword(subkey: &str, name: &str, default: u32) -> u32 {
    let sk = win32_shell::to_wide(subkey);
    let vn = win32_shell::to_wide(name);
    let mut hk = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, KEY_READ, &mut hk).is_ok() {
            let mut val = 0u32;
            let mut sz = 4u32;
            let ok = RegQueryValueExW(
                hk,
                PCWSTR(vn.as_ptr()),
                None,
                None,
                Some(&mut val as *mut u32 as *mut u8),
                Some(&mut sz),
            )
            .is_ok();
            let _ = RegCloseKey(hk);
            if ok {
                return val;
            }
        }
    }
    default
}

fn write_hkcu_dword(subkey: &str, name: &str, val: u32) -> bool {
    let sk = win32_shell::to_wide(subkey);
    let vn = win32_shell::to_wide(name);
    let mut hk = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, KEY_WRITE, &mut hk).is_ok() {
            let bytes = val.to_le_bytes();
            let ok = RegSetValueExW(hk, PCWSTR(vn.as_ptr()), 0, REG_DWORD, Some(&bytes)).is_ok();
            let _ = RegCloseKey(hk);
            return ok;
        }
    }
    false
}

fn get_win_drag_animations() -> bool {
    let mut val = 1i32;
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETDRAGFULLWINDOWS,
            0,
            Some(&mut val as *mut i32 as *mut _),
            windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    val != 0
}

fn set_win_drag_animations(enable: bool) {
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_SETDRAGFULLWINDOWS,
            if enable { 1 } else { 0 },
            None,
            SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
        );
    }
    let _ = write_hkcu_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
        "TaskbarAnimations",
        if enable { 1 } else { 0 },
    );
}

fn list_hkcu_run_entries() -> Vec<StartupEntry> {
    let mut out = Vec::new();
    let sk = win32_shell::to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let mut hk = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, KEY_READ, &mut hk).is_ok() {
            let mut idx = 0u32;
            loop {
                let mut name_buf = [0u16; 256];
                let mut name_len = name_buf.len() as u32;
                let mut data_buf = [0u8; 1024];
                let mut data_len = data_buf.len() as u32;
                let mut typ = 0u32;
                if RegEnumValueW(
                    hk,
                    idx,
                    windows::core::PWSTR(name_buf.as_mut_ptr()),
                    &mut name_len,
                    None,
                    Some(&mut typ),
                    Some(data_buf.as_mut_ptr()),
                    Some(&mut data_len),
                )
                .is_err()
                {
                    break;
                }
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let cmd = if typ == REG_SZ.0 && data_len >= 2 {
                    let u16s: Vec<u16> = data_buf[..data_len as usize]
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take_while(|&v| v != 0)
                        .collect();
                    String::from_utf16_lossy(&u16s)
                } else {
                    String::new()
                };
                if !name.is_empty() {
                    out.push(StartupEntry { name, cmd });
                }
                idx += 1;
                if idx > 128 {
                    break;
                }
            }
            let _ = RegCloseKey(hk);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn delete_hkcu_run_entry(name: &str) -> bool {
    let sk = win32_shell::to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let vn = win32_shell::to_wide(name);
    let mut hk = HKEY::default();
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sk.as_ptr()), 0, KEY_WRITE, &mut hk).is_ok() {
            let ok = RegDeleteValueW(hk, PCWSTR(vn.as_ptr())).is_ok();
            let _ = RegCloseKey(hk);
            return ok;
        }
    }
    false
}

fn query_active_power_plan() -> String {
    if let Ok(out) = Command::new("powercfg")
        .arg("/getactivescheme")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let txt = String::from_utf8_lossy(&out.stdout).to_lowercase();
        if txt.contains("8c5e7fda") || txt.contains("high") || txt.contains("alto") {
            return "high".to_string();
        }
        if txt.contains("a1841308") || txt.contains("saver") || txt.contains("economizador") {
            return "saver".to_string();
        }
    }
    "balanced".to_string()
}

fn set_power_plan(kind: &str) -> bool {
    let guid = match kind {
        "high" => "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
        "saver" => "a1841308-3541-4fab-bc81-f71556f20b4a",
        _ => "381b4222-f694-41f0-9685-ff5bb260df2e",
    };
    Command::new("powercfg")
        .args(["/setactive", guid])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn measure_temp_bytes() -> u64 {
    let mut sum = 0u64;
    if let Ok(tmp) = std::env::var("TEMP") {
        if let Ok(rd) = std::fs::read_dir(&tmp) {
            for e in rd.flatten().take(400) {
                if let Ok(m) = e.metadata() {
                    if m.is_file() {
                        sum = sum.saturating_add(m.len());
                    }
                }
            }
        }
    }
    sum
}

fn clean_temp_safe() -> (usize, u64) {
    let mut count = 0usize;
    let mut freed = 0u64;
    if let Ok(tmp) = std::env::var("TEMP") {
        if let Ok(rd) = std::fs::read_dir(&tmp) {
            for e in rd.flatten() {
                let p = e.path();
                if let Ok(m) = e.metadata() {
                    if m.is_file() {
                        let len = m.len();
                        if std::fs::remove_file(&p).is_ok() {
                            count += 1;
                            freed = freed.saturating_add(len);
                        }
                    }
                }
            }
        }
    }
    (count, freed)
}

fn format_bytes(b: u64) -> String {
    if b >= 1_073_741_824 {
        format!("{:.2} GB", b as f64 / 1_073_741_824.0)
    } else if b >= 1_048_576 {
        format!("{:.1} MB", b as f64 / 1_048_576.0)
    } else {
        format!("{:.0} KB", b as f64 / 1024.0)
    }
}

// ==================== RUNTIME STATE ====================

struct SettingsRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    maximized: bool,
    active_tab: Tab,
    scroll: f32,
    settings: ShellSettings,
    widgets: WidgetsStore,
    wallpapers: Vec<WallpaperItem>,
    drives: Vec<DriveInfo>,
    startup_entries: Vec<StartupEntry>,
    installed_programs: Vec<system::InstalledProgramEntry>,
    disp_w: u32,
    disp_h: u32,
    disp_hz: u32,
    disp_rates: Vec<u32>,
    win_dark_mode: bool,
    win_transparency: bool,
    win_animations: bool,
    power_plan: String,
    temp_bytes: u64,
    master_vol: i32,
    master_muted: bool,
    outputs: Vec<AudioDevice>,
    mics: Vec<AudioDevice>,
    app_sessions: Vec<AudioAppSession>,
    radios: RadioStates,
    net_details: radios::NetworkDetails,
    wifi_networks: Vec<radios::WifiNetwork>,
    wifi_scanning: bool,
    vpn_profiles: Vec<radios::VpnProfile>,
    ping_ms: Option<u32>,
    hs_ssid_draft: String,
    hs_pass_draft: String,
    hs_ssid_focused: bool,
    hs_pass_focused: bool,
    wifi_pwd_target: Option<String>,
    wifi_pwd_draft: String,
    wifi_pwd_focused: bool,
    pin_draft: String,
    pin_focused: bool,
    city_draft: String,
    city_focused: bool,
    github_draft: String,
    github_focused: bool,
    active_slider: Option<SliderKind>,
    toast: Option<(String, bool, Instant)>,
    hits: Vec<Hit>,
    hovered_id: Option<String>,
    bg_tx: Sender<BgUpdate>,
    bg_rx: Receiver<BgUpdate>,
    ram_used: f64,
    ram_total: f64,
    ram_pct: i32,
    hostname: String,
    username: String,
    cpu_cores: usize,
    wm_palette_changed: u32,
}

thread_local! {
    static SET_RT: RefCell<Option<SettingsRuntime>> = RefCell::new(None);
}

impl SettingsRuntime {
    fn show_toast(&mut self, msg: impl Into<String>, ok: bool) {
        self.toast = Some((msg.into(), ok, Instant::now()));
        self.redraw();
    }

    fn save_and_broadcast(&self) {
        save_settings(&self.settings);
        save_widgets(&self.widgets);
        win32_shell::notify_shell_settings_changed();
    }

    fn register_custom_wallpaper(&mut self, path: &str, is_vid: bool) {
        let mut custom = load_custom_items();
        if !custom.custom_wallpapers.iter().any(|w| w.target.eq_ignore_ascii_case(path)) {
            let name = Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Fondo personalizado")
                .to_string();
            let cover_path = if is_vid {
                video_decoder::extract_video_thumbnail(path)
            } else {
                None
            };
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            custom.custom_wallpapers.push(CustomEntry {
                id: format!("wp_{}", ts),
                name,
                target: path.to_string(),
                subtitle: "Fondo personalizado".into(),
                category: "Wallpaper".into(),
                cover_path,
            });
            save_custom_items(&custom);
        }
        self.wallpapers = wallpaper::scan_wallpapers(&custom);
    }

    fn spawn_audio_refresh(&self) {
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                );
            }
            let (mv, mm) = audio::get_master_volume();
            let outputs = audio::get_audio_devices(false);
            let mics = audio::get_audio_devices(true);
            let apps = audio::get_app_audio_sessions();
            let _ = tx.send(BgUpdate::Audio {
                master_vol: mv,
                master_muted: mm,
                outputs,
                mics,
                apps,
            });
        });
    }

    fn spawn_radios_refresh(&mut self) {
        self.wifi_scanning = true;
        let tx = self.bg_tx.clone();
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                );
            }
            let r = radios::query_radio_states();
            let details = radios::query_network_details();
            let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
            let vpns = radios::list_vpn_profiles();
            let _ = tx.send(BgUpdate::NetworkFull {
                radios: r,
                details,
                wifis,
                vpns,
            });
        });
    }

    fn unfocus_all_inputs(&mut self) {
        self.pin_focused = false;
        self.city_focused = false;
        self.github_focused = false;
        self.hs_ssid_focused = false;
        self.hs_pass_focused = false;
        self.wifi_pwd_focused = false;
    }

    fn poll_bg(&mut self) {
        let mut dirty = false;
        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                BgUpdate::Audio {
                    master_vol,
                    master_muted,
                    outputs,
                    mics,
                    apps,
                } => {
                    self.master_vol = master_vol;
                    self.master_muted = master_muted;
                    self.outputs = outputs;
                    self.mics = mics;
                    self.app_sessions = apps;
                    dirty = true;
                }
                BgUpdate::Radios(r) => {
                    if !self.hs_ssid_focused && !r.hotspot.ssid.is_empty() {
                        self.hs_ssid_draft = r.hotspot.ssid.clone();
                    }
                    if !self.hs_pass_focused && !r.hotspot.passphrase.is_empty() {
                        self.hs_pass_draft = r.hotspot.passphrase.clone();
                    }
                    self.radios = r;
                    dirty = true;
                }
                BgUpdate::NetworkFull { radios: r, details, wifis, vpns } => {
                    if !self.hs_ssid_focused && !r.hotspot.ssid.is_empty() {
                        self.hs_ssid_draft = r.hotspot.ssid.clone();
                    }
                    if !self.hs_pass_focused && !r.hotspot.passphrase.is_empty() {
                        self.hs_pass_draft = r.hotspot.passphrase.clone();
                    }
                    self.radios = r;
                    self.net_details = details;
                    self.wifi_networks = wifis;
                    self.vpn_profiles = vpns;
                    self.wifi_scanning = false;
                    dirty = true;
                }
                BgUpdate::PingMs(ms) => {
                    self.ping_ms = ms;
                    let msg = match ms {
                        Some(v) => format!("Latencia a Internet: {} ms (Cloudflare 1.1.1.1)", v),
                        None => "Sin respuesta de Internet (verifica tu conexión)".into(),
                    };
                    self.toast = Some((msg, ms.is_some(), Instant::now()));
                    dirty = true;
                }
                BgUpdate::PowerPlan(p) => {
                    self.power_plan = p;
                    dirty = true;
                }
                BgUpdate::TempBytes(b) => {
                    self.temp_bytes = b;
                    dirty = true;
                }
                BgUpdate::Toast(t, ok) => {
                    self.toast = Some((t, ok, Instant::now()));
                    dirty = true;
                }
            }
        }
        if self.gpu.drain_decoded_assets() {
            dirty = true;
        }
        if let Some((_, _, at)) = &self.toast {
            if at.elapsed().as_secs_f32() > 3.2 {
                self.toast = None;
                dirty = true;
            }
        }
        if dirty {
            self.redraw();
        }
    }

    fn apply_slider_drag(&mut self, px: f32) {
        let Some(sl) = self.active_slider.clone() else { return };
        match sl {
            SliderKind::MasterVol { x, w } => {
                let pct = (((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as i32;
                audio::set_master_volume(pct);
                self.master_vol = pct;
                self.master_muted = false;
                self.redraw();
            }
            SliderKind::AppVol { x, w, pids } => {
                let pct = (((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as i32;
                audio::set_app_session_volume(&pids, pct);
                for s in &mut self.app_sessions {
                    if s.pids == pids {
                        s.volume_percent = pct;
                        s.is_muted = false;
                    }
                }
                self.redraw();
            }
            SliderKind::GlassOpacity { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                self.settings.glass_opacity = 0.50 + frac * 0.46;
                self.save_and_broadcast();
                self.redraw();
            }
            SliderKind::WavesSpeed { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let v = gradient_waves::SPEED_MIN + frac * (gradient_waves::SPEED_MAX - gradient_waves::SPEED_MIN);
                self.settings.waves_speed = (v * 20.0).round() / 20.0;
                gradient_waves::set_tuning(self.settings.waves_speed, self.settings.waves_relief);
                self.redraw();
            }
            SliderKind::WavesRelief { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let v = gradient_waves::RELIEF_MIN + frac * (gradient_waves::RELIEF_MAX - gradient_waves::RELIEF_MIN);
                self.settings.waves_relief = (v * 20.0).round() / 20.0;
                gradient_waves::set_tuning(self.settings.waves_speed, self.settings.waves_relief);
                self.redraw();
            }
            SliderKind::HoloSpeed { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let v = holo_mosaic::SPEED_MIN + frac * (holo_mosaic::SPEED_MAX - holo_mosaic::SPEED_MIN);
                self.settings.holo_speed = (v * 20.0).round() / 20.0;
                self.redraw();
            }
            SliderKind::HoloSaturation { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let v = holo_mosaic::SAT_MIN + frac * (holo_mosaic::SAT_MAX - holo_mosaic::SAT_MIN);
                self.settings.holo_saturation = (v * 20.0).round() / 20.0;
                self.redraw();
            }
        }
    }

    fn execute(&mut self, act: SetAct) {
        match act {
            SetAct::WinClose => unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                PostQuitMessage(0);
            },
            SetAct::WinMin | SetAct::WinMax => {} // handled deferred outside RefCell borrow!
            SetAct::SelectTab(t) => {
                self.active_tab = t;
                self.scroll = 0.0;
                self.unfocus_all_inputs();
                if t == Tab::Sound {
                    self.spawn_audio_refresh();
                } else if t == Tab::Network {
                    self.spawn_radios_refresh();
                } else if t == Tab::StartupAndApps {
                    self.startup_entries = list_hkcu_run_entries();
                }
                self.redraw();
            }
            SetAct::SetPowerPlan(plan) => {
                self.power_plan = plan.to_string();
                let tx = self.bg_tx.clone();
                let p_owned = plan.to_string();
                std::thread::spawn(move || {
                    let ok = set_power_plan(&p_owned);
                    let lbl = match p_owned.as_str() {
                        "high" => "Alto rendimiento activado",
                        "saver" => "Economizador activado",
                        _ => "Plan Equilibrado activado",
                    };
                    let _ = tx.send(BgUpdate::Toast(
                        if ok { lbl.into() } else { "No se pudo cambiar el plan de energía".into() },
                        ok,
                    ));
                });
                self.redraw();
            }
            SetAct::ToggleWinAnimations => {
                self.win_animations = !self.win_animations;
                set_win_drag_animations(self.win_animations);
                self.show_toast(
                    if self.win_animations {
                        "Animaciones fluidas de ventanas activadas"
                    } else {
                        "Animaciones de ventanas desactivadas"
                    },
                    true,
                );
            }
            SetAct::CleanTempFiles => {
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    let (n, freed) = clean_temp_safe();
                    let rem = measure_temp_bytes();
                    let _ = tx.send(BgUpdate::TempBytes(rem));
                    let _ = tx.send(BgUpdate::Toast(
                        format!("Limpieza lista: {} archivos ({} liberados)", n, format_bytes(freed)),
                        true,
                    ));
                });
                self.show_toast("Limpiando archivos temporales...", true);
            }
            SetAct::OpenDiskAnalyzer => {
                win32_shell::launch_aurafiles(None);
            }
            SetAct::SetRefreshHz(hz) => {
                if apply_refresh_hz(hz) {
                    self.disp_hz = hz;
                    self.show_toast(format!("Frecuencia aplicada: {} Hz", hz), true);
                } else {
                    self.show_toast("El monitor rechazó esa frecuencia", false);
                }
            }
            SetAct::ToggleWinDarkMode => {
                self.win_dark_mode = !self.win_dark_mode;
                let v = if self.win_dark_mode { 0 } else { 1 };
                let _ = write_hkcu_dword(
                    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "AppsUseLightTheme",
                    v,
                );
                let _ = write_hkcu_dword(
                    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "SystemUsesLightTheme",
                    v,
                );
                self.show_toast(
                    if self.win_dark_mode { "Modo oscuro de Windows activado" } else { "Modo claro de Windows activado" },
                    true,
                );
            }
            SetAct::ToggleWinTransparency => {
                self.win_transparency = !self.win_transparency;
                let _ = write_hkcu_dword(
                    r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                    "EnableTransparency",
                    if self.win_transparency { 1 } else { 0 },
                );
                self.show_toast("Transparencia del sistema actualizada", true);
            }
            SetAct::SetGlassOpacity(v) => {
                self.settings.glass_opacity = v.clamp(0.50, 0.96);
                self.save_and_broadcast();
                self.redraw();
            }
            SetAct::ToggleMasterMute => {
                self.master_muted = audio::toggle_master_mute();
                self.redraw();
            }
            SetAct::SetMasterVol(p) => {
                let p = p.clamp(0, 100);
                audio::set_master_volume(p);
                self.master_vol = p;
                self.master_muted = false;
                self.redraw();
            }
            SetAct::SelectOutput(id) => {
                if audio::set_default_audio_device(&id) {
                    self.spawn_audio_refresh();
                    self.show_toast("Dispositivo de salida actualizado", true);
                }
            }
            SetAct::SelectMic(id) => {
                if audio::set_default_audio_device(&id) {
                    self.spawn_audio_refresh();
                    self.show_toast("Micrófono predeterminado actualizado", true);
                }
            }
            SetAct::ToggleAppMute(pids, muted) => {
                audio::set_app_session_mute(&pids, muted);
                for s in &mut self.app_sessions {
                    if s.pids == pids {
                        s.is_muted = muted;
                    }
                }
                self.redraw();
            }
            SetAct::SetAppVol(pids, pct) => {
                audio::set_app_session_volume(&pids, pct);
                self.redraw();
            }
            SetAct::RefreshAudio => {
                self.spawn_audio_refresh();
                self.show_toast("Mezclador de audio actualizado", true);
            }
            SetAct::ToggleWifi => {
                self.radios.wifi_on = !self.radios.wifi_on;
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let _ = radios::toggle_wifi();
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                });
                self.redraw();
            }
            SetAct::ToggleBluetooth => {
                self.radios.bt_on = !self.radios.bt_on;
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let _ = radios::toggle_bluetooth();
                    let _ = tx.send(BgUpdate::Radios(radios::query_radio_states()));
                });
                self.redraw();
            }
            SetAct::ToggleAirplaneMode => {
                let next = !self.radios.airplane_mode;
                self.radios.airplane_mode = next;
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let ok = radios::set_airplane_mode(next);
                    let r = radios::query_radio_states();
                    let _ = tx.send(BgUpdate::Radios(r));
                    let _ = tx.send(BgUpdate::Toast(
                        if !ok {
                            "Sin radios inalámbricas detectadas".into()
                        } else if next {
                            "Modo Avión activado".into()
                        } else {
                            "Modo Avión desactivado".into()
                        },
                        ok,
                    ));
                });
                self.redraw();
            }
            SetAct::ToggleHotspot => {
                let tx = self.bg_tx.clone();
                self.show_toast("Actualizando Compartir Internet (Hotspot)...", true);
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    let res = radios::toggle_hotspot();
                    let r = radios::query_radio_states();
                    let _ = tx.send(BgUpdate::Radios(r));
                    match res {
                        Ok(true) => { let _ = tx.send(BgUpdate::Toast("Zona Wi-Fi (Compartir Internet) ACTIVADA".into(), true)); }
                        Ok(false) => { let _ = tx.send(BgUpdate::Toast("Zona Wi-Fi (Compartir Internet) desactivada".into(), true)); }
                        Err(e) => { let _ = tx.send(BgUpdate::Toast(e, false)); }
                    }
                });
            }
            SetAct::FocusHotspotSsid => {
                self.unfocus_all_inputs();
                self.hs_ssid_focused = true;
                self.redraw();
            }
            SetAct::FocusHotspotPass => {
                self.unfocus_all_inputs();
                self.hs_pass_focused = true;
                self.redraw();
            }
            SetAct::SaveHotspotConfig => {
                let ssid = self.hs_ssid_draft.trim().to_string();
                let pass = self.hs_pass_draft.trim().to_string();
                if let Err(e) = radios::validate_hotspot_credentials(&ssid, &pass) {
                    self.show_toast(e, false);
                    return;
                }
                self.hs_ssid_focused = false;
                self.hs_pass_focused = false;
                let tx = self.bg_tx.clone();
                self.show_toast("Guardando nombre y contraseña del Hotspot...", true);
                std::thread::spawn(move || {
                    unsafe {
                        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    }
                    match radios::configure_hotspot(&ssid, &pass) {
                        Ok(()) => {
                            let _ = tx.send(BgUpdate::Radios(radios::query_radio_states()));
                            let _ = tx.send(BgUpdate::Toast(format!("Hotspot '{}' configurado correctamente", ssid), true));
                        }
                        Err(e) => {
                            let _ = tx.send(BgUpdate::Toast(e, false));
                        }
                    }
                });
            }
            SetAct::ScanWifiNetworks => {
                self.spawn_radios_refresh();
                self.show_toast("Escaneando redes Wi-Fi y adaptadores...", true);
            }
            SetAct::ConnectWifiSaved(ssid) => {
                let tx = self.bg_tx.clone();
                self.show_toast(format!("Conectando a '{}'...", ssid), true);
                std::thread::spawn(move || {
                    let res = radios::connect_wifi(&ssid, None);
                    std::thread::sleep(std::time::Duration::from_millis(900));
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    match res {
                        Ok(m) => { let _ = tx.send(BgUpdate::Toast(m, true)); }
                        Err(e) => { let _ = tx.send(BgUpdate::Toast(e, false)); }
                    }
                });
            }
            SetAct::PromptWifiPassword(ssid) => {
                self.unfocus_all_inputs();
                self.wifi_pwd_target = Some(ssid);
                self.wifi_pwd_draft.clear();
                self.wifi_pwd_focused = true;
                self.redraw();
            }
            SetAct::FocusWifiPassInput => {
                self.unfocus_all_inputs();
                self.wifi_pwd_focused = true;
                self.redraw();
            }
            SetAct::CancelWifiPassword => {
                self.wifi_pwd_target = None;
                self.wifi_pwd_draft.clear();
                self.wifi_pwd_focused = false;
                self.redraw();
            }
            SetAct::SubmitWifiPassword => {
                let Some(ssid) = self.wifi_pwd_target.take() else { return };
                let pwd = std::mem::take(&mut self.wifi_pwd_draft);
                self.wifi_pwd_focused = false;
                let tx = self.bg_tx.clone();
                self.show_toast(format!("Conectando a '{}'...", ssid), true);
                std::thread::spawn(move || {
                    let res = radios::connect_wifi(&ssid, Some(&pwd));
                    std::thread::sleep(std::time::Duration::from_millis(1200));
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    match res {
                        Ok(m) => { let _ = tx.send(BgUpdate::Toast(m, true)); }
                        Err(e) => { let _ = tx.send(BgUpdate::Toast(e, false)); }
                    }
                });
            }
            SetAct::DisconnectWifi => {
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    let ok = radios::disconnect_wifi();
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    let _ = tx.send(BgUpdate::Toast(if ok { "Wi-Fi desconectado".into() } else { "Error al desconectar Wi-Fi".into() }, ok));
                });
            }
            SetAct::ForgetWifi(ssid) => {
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    let ok = radios::forget_wifi(&ssid);
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    let _ = tx.send(BgUpdate::Toast(
                        if ok { format!("Red '{}' olvidada", ssid) } else { "No se pudo olvidar la red".into() },
                        ok,
                    ));
                });
            }
            SetAct::ToggleVpn(name, is_conn) => {
                let tx = self.bg_tx.clone();
                self.show_toast(format!("{} VPN '{}'...", if is_conn { "Desconectando" } else { "Conectando" }, name), true);
                std::thread::spawn(move || {
                    let res = radios::toggle_vpn(&name, is_conn);
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    match res {
                        Ok(m) => { let _ = tx.send(BgUpdate::Toast(m, true)); }
                        Err(e) => { let _ = tx.send(BgUpdate::Toast(e, false)); }
                    }
                });
            }
            SetAct::TestPing => {
                let tx = self.bg_tx.clone();
                self.show_toast("Midiendo latencia a Internet...", true);
                std::thread::spawn(move || {
                    let ms = radios::measure_internet_latency_ms();
                    let _ = tx.send(BgUpdate::PingMs(ms));
                });
            }
            SetAct::RenewIp => {
                let tx = self.bg_tx.clone();
                self.show_toast("Renovando dirección IP y DNS...", true);
                std::thread::spawn(move || {
                    let ok = radios::renew_ip_address();
                    let r = radios::query_radio_states();
                    let details = radios::query_network_details();
                    let wifis = radios::scan_wifi_networks(&r.wifi_ssid);
                    let vpns = radios::list_vpn_profiles();
                    let _ = tx.send(BgUpdate::NetworkFull { radios: r, details, wifis, vpns });
                    let _ = tx.send(BgUpdate::Toast(
                        if ok { "IP renovada correctamente".into() } else { "No se pudo renovar la IP".into() },
                        ok,
                    ));
                });
            }
            SetAct::FlushDns => {
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    let ok = Command::new("ipconfig")
                        .arg("/flushdns")
                        .creation_flags(CREATE_NO_WINDOW)
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false);
                    let _ = tx.send(BgUpdate::Toast(
                        if ok { "Caché DNS limpiada correctamente".into() } else { "Error limpiando DNS".into() },
                        ok,
                    ));
                });
            }
            SetAct::SetDnsPreset(preset) => {
                let script = match preset {
                    "cf" => "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses ('1.1.1.1','1.0.0.1')",
                    "google" => "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses ('8.8.8.8','8.8.4.4')",
                    "quad9" => "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ServerAddresses ('9.9.9.9','149.112.112.112')",
                    _ => "Get-NetAdapter | Where-Object Status -eq 'Up' | Set-DnsClientServerAddress -ResetServerAddresses",
                };
                win32_shell::run_as_admin("powershell.exe", &format!("-NoProfile -WindowStyle Hidden -Command \"{}\"", script));
                self.show_toast("Aplicando DNS (acepta UAC si aparece)...", true);
            }
            SetAct::OpenWinNetworkSettings => {
                win32_shell::launch_target("ms-settings:network", "");
            }
            SetAct::ToggleShellSetting(k) => {
                match k {
                    "hide_taskbar" => self.settings.hide_windows_taskbar = !self.settings.hide_windows_taskbar,
                    "dock_auto_hide" => self.settings.dock_auto_hide = !self.settings.dock_auto_hide,
                    "dock_left" => {
                        self.settings.dock_position = if self.settings.dock_position.eq_ignore_ascii_case("Bottom") {
                            "Left".to_string()
                        } else {
                            "Bottom".to_string()
                        };
                    }
                    "style_windows_apps" => self.settings.style_windows_apps = !self.settings.style_windows_apps,
                    "show_widgets" => self.settings.show_desktop_widgets = !self.settings.show_desktop_widgets,
                    "wp_autopause" => self.settings.wallpaper_auto_pause = !self.settings.wallpaper_auto_pause,
                    "auto_boost" => self.settings.auto_game_boost = !self.settings.auto_game_boost,
                    "auto_updates" => self.settings.auto_apply_updates = !self.settings.auto_apply_updates,
                    "lock_win_pwd" => self.settings.lock_use_windows_pwd = !self.settings.lock_use_windows_pwd,
                    "lock_startup" => self.settings.lock_on_startup = !self.settings.lock_on_startup,
                    "lock_media" => self.settings.lock_show_media = !self.settings.lock_show_media,
                    "perf_mode" => self.settings.performance_mode = !self.settings.performance_mode,
                    "win_hotkeys" => self.settings.intercept_win_hotkeys = !self.settings.intercept_win_hotkeys,
                    _ => {}
                }
                self.save_and_broadcast();
                self.show_toast("Ajuste aplicado en vivo en AuraOS", true);
            }
            SetAct::SetThemeScheme(scheme) => {
                self.settings.theme_scheme = scheme.to_string();
                theme::apply_theme_scheme(scheme, theme::pal().seed_rgb);
                self.save_and_broadcast();
                self.show_toast(format!("Paleta Material You '{}' aplicada en vivo", scheme), true);
            }
            SetAct::SetBarStyle(style) => {
                self.settings.bar_style = style.to_string();
                self.save_and_broadcast();
                self.show_toast(format!("Barra superior estilo '{}' aplicada", style), true);
            }
            SetAct::SetClockStyle(style) => {
                self.settings.clock_widget_style = style.to_string();
                self.save_and_broadcast();
                self.show_toast(format!("Estilo de reloj '{}' aplicado en vivo", style), true);
            }
            SetAct::FocusCityInput => {
                self.city_focused = true;
                self.pin_focused = false;
                self.github_focused = false;
                self.redraw();
            }
            SetAct::SaveWeatherCity => {
                self.settings.weather_city = self.city_draft.trim().to_string();
                self.city_focused = false;
                self.save_and_broadcast();
                self.show_toast(
                    if self.settings.weather_city.is_empty() {
                        "Clima configurado en detección automática de ubicación"
                    } else {
                        "Ciudad del clima guardada y actualizando en vivo..."
                    },
                    true,
                );
            }
            SetAct::ClearWeatherCity => {
                self.city_draft.clear();
                self.settings.weather_city.clear();
                self.city_focused = false;
                self.save_and_broadcast();
                self.show_toast("Ubicación automática por IP restaurada", true);
            }
            SetAct::ToggleWidget(wk) => {
                if let Some(cfg) = self.widgets.get_mut(wk) {
                    cfg.enabled = !cfg.enabled;
                    if cfg.enabled {
                        self.settings.show_desktop_widgets = true;
                    }
                }
                self.save_and_broadcast();
                self.redraw();
            }
            SetAct::SetDesktopWallpaper(id) => {
                self.settings.wallpaper_id = id.clone();
                if id.starts_with("preset:waves_") {
                    self.settings.waves_custom_color.clear();
                    gradient_waves::set_custom_color("");
                    if let Some(rgb) = id.strip_prefix("preset:").and_then(gradient_waves::effective_color) {
                        theme::apply_theme_scheme(&self.settings.theme_scheme, rgb);
                    }
                } else if id.starts_with("preset:holo_") {
                    self.settings.holo_saturation = -1.0;
                    if let Some(rgb) = id.strip_prefix("preset:").and_then(holo_mosaic::effective_accent_rgb) {
                        theme::apply_theme_scheme(&self.settings.theme_scheme, rgb);
                    }
                } else if let Some(wp) = self.wallpapers.iter().find(|w| w.id == id) {
                    theme::apply_theme_scheme(&self.settings.theme_scheme, wp.accent_rgb);
                }
                self.save_and_broadcast();
                self.show_toast("Fondo del escritorio y colores adaptados en vivo", true);
            }
            SetAct::SetHoloFilter(filter) => {
                let on_holo = self
                    .settings
                    .wallpaper_id
                    .strip_prefix("preset:")
                    .map(holo_mosaic::is_holo_key)
                    .unwrap_or(false);
                if !on_holo {
                    self.settings.wallpaper_id = holo_mosaic::DEFAULT_PRESET_ID.to_string();
                }
                self.settings.holo_filter = filter;
                self.save_and_broadcast();
                self.show_toast("Filtro de Holo Mosaic aplicado en vivo", true);
            }
            SetAct::SetWavesCustomColor(hex) => {
                let on_waves = self
                    .settings
                    .wallpaper_id
                    .strip_prefix("preset:")
                    .map(gradient_waves::is_waves_key)
                    .unwrap_or(false);
                if !on_waves {
                    self.settings.wallpaper_id = gradient_waves::DEFAULT_PRESET_ID.to_string();
                }
                self.settings.waves_custom_color = hex.clone();
                self.settings.theme_scheme = "Auto".to_string();
                gradient_waves::set_custom_color(&hex);
                if let Some(rgb) = self
                    .settings
                    .wallpaper_id
                    .strip_prefix("preset:")
                    .and_then(gradient_waves::effective_color)
                {
                    theme::apply_theme_scheme("Auto", rgb);
                }
                self.save_and_broadcast();
                self.show_toast("Color de Gradient Waves y tema de todo AuraOS actualizados", true);
            }
            SetAct::SetLockWallpaper(id) => {
                self.settings.lock_wallpaper_id = id;
                self.save_and_broadcast();
                self.show_toast("Fondo de AuraLock guardado", true);
            }
            SetAct::PickLocalLockWallpaper => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Elegir fondo para AuraLock (Imagen o Video)",
                    "Fondos (*.png;*.jpg;*.jpeg;*.webp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi)",
                    "*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi",
                ) {
                    let is_vid = video_decoder::is_video_ext(&p);
                    let id = if is_vid { format!("video:{}", p) } else { format!("image:{}", p) };
                    self.register_custom_wallpaper(&p, is_vid);
                    self.settings.lock_wallpaper_id = id;
                    self.save_and_broadcast();
                    self.show_toast("Fondo personalizado aplicado a AuraLock", true);
                }
            }
            SetAct::PickLocalDesktopWallpaper => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Elegir fondo para el Escritorio (Imagen o Video)",
                    "Fondos (*.png;*.jpg;*.jpeg;*.webp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi)",
                    "*.png;*.jpg;*.jpeg;*.webp;*.bmp;*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi",
                ) {
                    let is_vid = video_decoder::is_video_ext(&p);
                    let id = if is_vid { format!("video:{}", p) } else { format!("image:{}", p) };
                    self.register_custom_wallpaper(&p, is_vid);
                    self.settings.wallpaper_id = id;
                    self.save_and_broadcast();
                    self.show_toast("Fondo personalizado aplicado al escritorio en vivo", true);
                }
            }
            SetAct::PickLockAvatar => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Elegir foto de perfil para AuraLock",
                    "Imágenes (*.png;*.jpg;*.jpeg;*.webp;*.bmp)",
                    "*.png;*.jpg;*.jpeg;*.webp;*.bmp",
                ) {
                    // Copy locally into AuraShell data dir so moving/deleting original doesn't break avatar
                    let dst_dir = app_data_dir();
                    let _ = std::fs::create_dir_all(&dst_dir);
                    let ext = Path::new(&p).extension().and_then(|e| e.to_str()).unwrap_or("png");
                    let dst = dst_dir.join(format!("lock_avatar.{}", ext));
                    let final_path = if std::fs::copy(&p, &dst).is_ok() {
                        dst.to_string_lossy().to_string()
                    } else {
                        p
                    };
                    self.settings.lock_avatar_path = final_path;
                    self.save_and_broadcast();
                    self.show_toast("Foto de perfil actualizada en AuraLock", true);
                }
            }
            SetAct::ClearLockAvatar => {
                self.settings.lock_avatar_path.clear();
                self.save_and_broadcast();
                self.show_toast("Foto de perfil eliminada", true);
            }
            SetAct::FocusPinInput => {
                self.pin_focused = true;
                self.city_focused = false;
                self.github_focused = false;
                self.redraw();
            }
            SetAct::SaveLockPin => {
                if self.pin_draft.trim().len() < 4 {
                    self.show_toast("La contraseña o PIN debe tener al menos 4 caracteres", false);
                } else {
                    self.settings.lock_pin_hash = hash_lock_pin(&self.pin_draft);
                    self.pin_draft.clear();
                    self.pin_focused = false;
                    self.save_and_broadcast();
                    self.show_toast("Contraseña de AuraLock guardada con cifrado seguro", true);
                }
            }
            SetAct::ClearLockPin => {
                self.settings.lock_pin_hash.clear();
                self.settings.lock_use_windows_pwd = false;
                self.pin_draft.clear();
                self.pin_focused = false;
                self.save_and_broadcast();
                self.show_toast("Contraseña de AuraLock eliminada (desbloqueo rápido)", true);
            }
            SetAct::TestAuraLock => {
                win32_shell::launch_aura_lock();
            }
            SetAct::RemoveStartupEntry(name) => {
                if name.eq_ignore_ascii_case("AuraShell") {
                    self.show_toast("Desactiva inicio de AuraOS desde el instalador", false);
                } else if delete_hkcu_run_entry(&name) {
                    self.startup_entries = list_hkcu_run_entries();
                    self.show_toast(format!("'{}' quitado del inicio de Windows", name), true);
                }
            }
            SetAct::UninstallProgram(cmd) => {
                let _ = std::process::Command::new("cmd.exe")
                    .args(["/C", &cmd])
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn();
                self.show_toast("Desinstalador iniciado en segundo plano", true);
            }
            SetAct::LaunchSuiteExe(exe) => {
                if let Some(p) = win32_shell::resolve_aura_app_path(exe) {
                    win32_shell::launch_target(&p.to_string_lossy(), "");
                } else {
                    self.show_toast(format!("No se encontró {}", exe), false);
                }
            }
            SetAct::LaunchApbOptimizer => {
                let p = app_data_dir().join("APBOptimizer.ps1");
                if p.exists() {
                    win32_shell::launch_target(
                        "powershell.exe",
                        &format!("-ExecutionPolicy Bypass -NoProfile -File \"{}\"", p.to_string_lossy()),
                    );
                } else {
                    self.show_toast("APBOptimizer no está copiado aún", false);
                }
            }
            SetAct::FocusGithubUrl => {
                self.github_focused = true;
                self.pin_focused = false;
                self.redraw();
            }
            SetAct::SaveGithubUrl => {
                self.settings.update_manifest_url = self.github_draft.trim().to_string();
                self.github_focused = false;
                self.save_and_broadcast();
                self.show_toast("URL de actualizaciones GitHub guardada", true);
            }
            SetAct::LaunchInstaller => {
                let p = app_data_dir().join("AuraShell-Setup.exe");
                if p.exists() {
                    win32_shell::launch_target(&p.to_string_lossy(), "");
                }
            }
            SetAct::OpenClassicWinSettings => {
                win32_shell::launch_target("ms-settings:", "");
            }
            SetAct::RestoreWindowsTaskbar => {
                win32_shell::restore_windows_safely();
                self.show_toast("Barra clásica de Windows restaurada", true);
            }
        }
    }

    fn redraw(&mut self) {
        let w = self.width;
        let h = self.height;
        let mut hits = Vec::new();
        let snap = SettingsSnap {
            w,
            h,
            maximized: self.maximized,
            active_tab: self.active_tab,
            scroll: self.scroll,
            settings: &self.settings,
            widgets: &self.widgets,
            wallpapers: &self.wallpapers,
            drives: &self.drives,
            startup_entries: &self.startup_entries,
            installed_programs: &self.installed_programs,
            disp_w: self.disp_w,
            disp_h: self.disp_h,
            disp_hz: self.disp_hz,
            disp_rates: &self.disp_rates,
            win_dark_mode: self.win_dark_mode,
            win_transparency: self.win_transparency,
            win_animations: self.win_animations,
            power_plan: &self.power_plan,
            temp_bytes: self.temp_bytes,
            master_vol: self.master_vol,
            master_muted: self.master_muted,
            outputs: &self.outputs,
            mics: &self.mics,
            app_sessions: &self.app_sessions,
            radios: &self.radios,
            net_details: &self.net_details,
            wifi_networks: &self.wifi_networks,
            wifi_scanning: self.wifi_scanning,
            vpn_profiles: &self.vpn_profiles,
            ping_ms: self.ping_ms,
            hs_ssid_draft: &self.hs_ssid_draft,
            hs_pass_draft: &self.hs_pass_draft,
            hs_ssid_focused: self.hs_ssid_focused,
            hs_pass_focused: self.hs_pass_focused,
            wifi_pwd_target: self.wifi_pwd_target.as_deref(),
            wifi_pwd_draft: &self.wifi_pwd_draft,
            wifi_pwd_focused: self.wifi_pwd_focused,
            pin_draft: &self.pin_draft,
            pin_focused: self.pin_focused,
            city_draft: &self.city_draft,
            city_focused: self.city_focused,
            github_draft: &self.github_draft,
            github_focused: self.github_focused,
            toast: self.toast.as_ref().map(|(s, ok, _)| (s.as_str(), *ok)),
            hovered: self.hovered_id.as_deref(),
            ram_used: self.ram_used,
            ram_total: self.ram_total,
            ram_pct: self.ram_pct,
            hostname: &self.hostname,
            username: &self.username,
            cpu_cores: self.cpu_cores,
        };
        self.surf.draw(&mut self.gpu, |ctx| {
            render_settings_ui(ctx, &snap, &mut hits);
        });
        let nodes: Vec<(String, String, f32, f32, f32, f32)> = hits
            .iter()
            .take(120)
            .map(|h| (h.id.clone(), h.id.clone(), h.rect.x, h.rect.y, h.rect.w, h.rect.h))
            .collect();
        aura_os::agent_bridge::publish_ui_tree("AuraSettings", self.hwnd, &nodes);
        self.hits = hits;
    }
}

struct SettingsSnap<'a> {
    w: f32,
    h: f32,
    maximized: bool,
    active_tab: Tab,
    scroll: f32,
    settings: &'a ShellSettings,
    widgets: &'a WidgetsStore,
    wallpapers: &'a [WallpaperItem],
    drives: &'a [DriveInfo],
    startup_entries: &'a [StartupEntry],
    installed_programs: &'a [system::InstalledProgramEntry],
    disp_w: u32,
    disp_h: u32,
    disp_hz: u32,
    disp_rates: &'a [u32],
    win_dark_mode: bool,
    win_transparency: bool,
    win_animations: bool,
    power_plan: &'a str,
    temp_bytes: u64,
    master_vol: i32,
    master_muted: bool,
    outputs: &'a [AudioDevice],
    mics: &'a [AudioDevice],
    app_sessions: &'a [AudioAppSession],
    radios: &'a RadioStates,
    net_details: &'a radios::NetworkDetails,
    wifi_networks: &'a [radios::WifiNetwork],
    wifi_scanning: bool,
    vpn_profiles: &'a [radios::VpnProfile],
    ping_ms: Option<u32>,
    hs_ssid_draft: &'a str,
    hs_pass_draft: &'a str,
    hs_ssid_focused: bool,
    hs_pass_focused: bool,
    wifi_pwd_target: Option<&'a str>,
    wifi_pwd_draft: &'a str,
    wifi_pwd_focused: bool,
    pin_draft: &'a str,
    pin_focused: bool,
    city_draft: &'a str,
    city_focused: bool,
    github_draft: &'a str,
    github_focused: bool,
    toast: Option<(&'a str, bool)>,
    hovered: Option<&'a str>,
    ram_used: f64,
    ram_total: f64,
    ram_pct: i32,
    hostname: &'a str,
    username: &'a str,
    cpu_cores: usize,
}

fn draw_toggle_row(
    ctx: &mut DrawCtx<'_>,
    r: Rect,
    id: &str,
    title: &str,
    sub: &str,
    on: bool,
    hovered: Option<&str>,
    act: SetAct,
    hits: &mut Vec<Hit>,
) {
    let hov = hovered == Some(id);
    ctx.fill_card(
        r,
        13.0,
        if hov { theme::ELEVATED_HOVER } else { theme::CARD_BG },
        if hov { theme::accent_border() } else { theme::BORDER },
    );
    ctx.draw_text(title, Rect::new(r.x + 14.0, r.y + 8.0, r.w - 86.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text(sub, Rect::new(r.x + 14.0, r.y + 27.0, r.w - 86.0, 15.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);
    let sw = Rect::new(r.x + r.w - 56.0, r.y + (r.h - 22.0) * 0.5, 42.0, 22.0);
    ctx.fill_rrect(sw, 11.0, if on { theme::accent() } else { theme::ELEVATED });
    ctx.fill_circle(if on { sw.x + 31.0 } else { sw.x + 11.0 }, sw.y + 11.0, 8.0, theme::CRISP_WHITE);
    hits.push(Hit { id: id.into(), rect: r, act, slider: None });
}

fn render_settings_ui(ctx: &mut DrawCtx<'_>, s: &SettingsSnap<'_>, hits: &mut Vec<Hit>) {
    let w = s.w;
    let h = s.h;
    let corner = if s.maximized { 0.0 } else { 15.0 };
    let bounds = Rect::new(0.0, 0.0, w, h);
    let _clip = if corner > 0.0 {
        ctx.push_rrect_mask_layer(bounds, bounds, corner, 1.0)
    } else {
        None
    };

    ctx.fill_rrect(bounds, 0.0, theme::CANVAS);

    // 1. TOP TITLE BAR + UNIFIED macOS TRAFFIC LIGHTS (× − +)
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_H, w, TOP_H, theme::BORDER, 1.0);

    let lights = [
        ("st_close", 20.0f32, theme::CORAL, "×", SetAct::WinClose),
        ("st_min", 40.0f32, theme::AMBER, "−", SetAct::WinMin),
        ("st_max", 60.0f32, theme::EMERALD, "+", SetAct::WinMax),
    ];
    let any_tl = matches!(s.hovered, Some("st_close") | Some("st_min") | Some("st_max"));
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 23.0, if any_tl { 6.3 } else { 5.8 }, col);
        if any_tl {
            ctx.draw_text_center(glyph, Rect::new(cx - 6.0, 16.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit { id: id.into(), rect: Rect::new(cx - 9.0, 13.0, 18.0, 20.0), act, slider: None });
    }

    ctx.draw_text("Configuración de AuraOS", Rect::new(82.0, 0.0, 240.0, TOP_H), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
    ctx.draw_text_right(
        &format!("{} · {} · v{} Rust", s.username, s.hostname, env!("CARGO_PKG_VERSION")),
        Rect::new(w - 340.0, 0.0, 322.0, TOP_H),
        TextStyle::Small10Med,
        theme::TEXT_TERTIARY,
    );

    // 2. LEFT NAVIGATION SIDEBAR
    let body_y = TOP_H;
    let body_h = (h - body_y).max(200.0);
    ctx.fill_rrect(Rect::new(0.0, body_y, SB_W, body_h), 0.0, theme::SURFACE);
    ctx.draw_line(SB_W, body_y, SB_W, h, theme::BORDER, 1.0);

    // User profile badge in sidebar (shows custom AuraLock avatar if configured!)
    let prof_r = Rect::new(12.0, body_y + 14.0, SB_W - 24.0, 52.0);
    ctx.fill_card(prof_r, 14.0, theme::CARD_BG, theme::BORDER);
    let av_r = Rect::new(prof_r.x + 9.0, prof_r.y + 11.0, 30.0, 30.0);
    let mut drew_av = false;
    if !s.settings.lock_avatar_path.trim().is_empty() && Path::new(&s.settings.lock_avatar_path).exists() {
        let _l = ctx.push_rrect_mask_layer(av_r, av_r, 15.0, 1.0);
        drew_av = ctx.draw_image_file(&s.settings.lock_avatar_path, av_r, 1.0);
        if _l.is_some() { ctx.pop_layer(); }
    }
    if !drew_av {
        ctx.fill_circle(prof_r.x + 24.0, prof_r.y + 26.0, 15.0, theme::accent());
        let init = s.username.chars().next().unwrap_or('A').to_uppercase().to_string();
        ctx.draw_text_center(&init, av_r, TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    }
    ctx.draw_text(s.username, Rect::new(prof_r.x + 48.0, prof_r.y + 9.0, prof_r.w - 56.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
    ctx.draw_text("AuraOS sobre Windows 11", Rect::new(prof_r.x + 48.0, prof_r.y + 27.0, prof_r.w - 56.0, 15.0), TextStyle::Small9Reg, theme::accent_light());

    let mut sy = prof_r.y + prof_r.h + 16.0;
    for tab in Tab::ALL {
        let tr = Rect::new(12.0, sy, SB_W - 24.0, 36.0);
        let active = s.active_tab == tab;
        let id = format!("tab_{:?}", tab);
        let hov = s.hovered == Some(id.as_str());
        ctx.fill_rrect(
            tr,
            11.0,
            if active { theme::accent() } else if hov { theme::ELEVATED } else { theme::rgba(0, 0, 0, 0.0) },
        );
        ctx.draw_vector_icon(
            tab.icon(),
            tr.x + 18.0,
            tr.y + 18.0,
            13.0,
            if active { theme::TEXT_PRIMARY } else { theme::accent_light() },
        );
        ctx.draw_text(
            tab.label(),
            Rect::new(tr.x + 34.0, tr.y, tr.w - 40.0, 36.0),
            if active { TextStyle::Body11Med } else { TextStyle::Body11Reg },
            if active { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
        );
        hits.push(Hit { id, rect: tr, act: SetAct::SelectTab(tab), slider: None });
        sy += 41.0;
    }

    // Bottom quick button: Test AuraLock
    let lk_btn = Rect::new(12.0, h - 50.0, SB_W - 24.0, 36.0);
    ctx.fill_card(lk_btn, 12.0, theme::accent_tint(), theme::accent_border());
    ctx.draw_vector_icon(VectorIcon::Lock, lk_btn.x + 18.0, lk_btn.y + 18.0, 12.5, theme::accent_light());
    ctx.draw_text("Bloquear (AuraLock)", Rect::new(lk_btn.x + 34.0, lk_btn.y, lk_btn.w - 40.0, 36.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(Hit { id: "sb_auralock".into(), rect: lk_btn, act: SetAct::TestAuraLock, slider: None });

    // 3. MAIN SCROLLABLE CONTENT PANEL
    let cx = SB_W + 24.0;
    let cw = (w - SB_W - 48.0).min(880.0).max(400.0);
    let view_r = Rect::new(SB_W + 4.0, body_y + 4.0, w - SB_W - 8.0, body_h - 8.0);
    ctx.push_clip(view_r);

    let mut y = body_y + 22.0 - s.scroll;
    ctx.draw_text(s.active_tab.label(), Rect::new(cx, y, cw, 28.0), TextStyle::Title22Bold, theme::TEXT_PRIMARY);
    y += 38.0;

    match s.active_tab {
        Tab::System => {
            // Hardware Hero Bento
            let hc = Rect::new(cx, y, cw, 92.0);
            ctx.fill_card(hc, 16.0, theme::SURFACE, theme::accent_border());
            let col_w = hc.w / 3.0;
            let specs = [
                ("PROCESADOR", format!("{} hilos lógicos", s.cpu_cores), "Arquitectura x64".to_string()),
                ("MEMORIA RAM", format!("{:.1} / {:.1} GB", s.ram_used, s.ram_total), format!("{}% en uso", s.ram_pct)),
                ("ENTORNO", format!("AuraOS v{}", env!("CARGO_PKG_VERSION")), "Motor Pure-Rust Direct2D".to_string()),
            ];
            for (i, (k, v, sub)) in specs.into_iter().enumerate() {
                let bx = hc.x + 18.0 + i as f32 * col_w;
                ctx.draw_text(k, Rect::new(bx, hc.y + 14.0, col_w - 24.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
                ctx.draw_text(&v, Rect::new(bx, hc.y + 32.0, col_w - 24.0, 22.0), TextStyle::Title16Semi, theme::TEXT_PRIMARY);
                ctx.draw_text(&sub, Rect::new(bx, hc.y + 58.0, col_w - 24.0, 16.0), TextStyle::Small10Med, theme::accent_light());
            }
            y += 108.0;

            // Real Windows Power Plan Switcher!
            ctx.draw_text("PLAN DE ENERGÍA DE WINDOWS (REAL)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let plans = [
                ("pwr_high", "Alto rendimiento", "Máximo FPS y respuesta", "high"),
                ("pwr_bal", "Equilibrado", "Balance ideal diario", "balanced"),
                ("pwr_save", "Economizador", "Menor consumo y calor", "saver"),
            ];
            let pw = (cw - 20.0) / 3.0;
            for (i, (id, title, sub, key)) in plans.into_iter().enumerate() {
                let pr = Rect::new(cx + i as f32 * (pw + 10.0), y, pw, 64.0);
                let active = s.power_plan == key;
                ctx.fill_card(pr, 13.0, if active { theme::accent_tint() } else { theme::CARD_BG }, if active { theme::accent_light() } else { theme::BORDER });
                ctx.draw_text(title, Rect::new(pr.x + 12.0, pr.y + 12.0, pr.w - 24.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
                ctx.draw_text(sub, Rect::new(pr.x + 12.0, pr.y + 34.0, pr.w - 24.0, 15.0), TextStyle::Small9Reg, if active { theme::accent_light() } else { theme::TEXT_TERTIARY });
                hits.push(Hit { id: id.into(), rect: pr, act: SetAct::SetPowerPlan(key), slider: None });
            }
            y += 80.0;

            draw_toggle_row(
                ctx,
                Rect::new(cx, y, cw, 52.0),
                "sys_anim",
                "Animaciones fluidas al arrastrar ventanas",
                "Evita el recuadro blanco al mover ventanas (DragFullWindows)",
                s.win_animations,
                s.hovered,
                SetAct::ToggleWinAnimations,
                hits,
            );
            y += 66.0;

            // Storage & Safe Temp Cleaner
            ctx.draw_text("ALMACENAMIENTO Y LIMPIEZA SEGURA", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            for d in s.drives.iter().take(4) {
                let dr = Rect::new(cx, y, cw, 46.0);
                ctx.fill_card(dr, 12.0, theme::CARD_BG, theme::BORDER);
                ctx.draw_text(&format!("Disco {}  ·  {:.0} GB libres de {:.0} GB", d.letter, d.free_gb, d.total_gb), Rect::new(dr.x + 14.0, dr.y + 7.0, dr.w - 140.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                ctx.draw_text_right(&format!("{:.0}% usado", d.used_pct), Rect::new(dr.x + dr.w - 110.0, dr.y + 7.0, 96.0, 18.0), TextStyle::Mono11Med, if d.used_pct > 80.0 { theme::CORAL } else { theme::accent_light() });
                let br = Rect::new(dr.x + 14.0, dr.y + 31.0, dr.w - 28.0, 6.0);
                ctx.fill_rrect(br, 3.0, theme::ELEVATED);
                ctx.fill_rrect(Rect::new(br.x, br.y, (d.used_pct / 100.0).clamp(0.03, 1.0) * br.w, 6.0), 3.0, if d.used_pct > 80.0 { theme::CORAL } else { theme::accent() });
                y += 54.0;
            }
            let b_tmp = Rect::new(cx, y + 4.0, 250.0, 36.0);
            ctx.fill_rrect(b_tmp, 11.0, theme::accent());
            ctx.draw_text_center(&format!("Limpiar %TEMP% ({})", format_bytes(s.temp_bytes)), b_tmp, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "sys_clean_tmp".into(), rect: b_tmp, act: SetAct::CleanTempFiles, slider: None });

            let b_an = Rect::new(cx + 262.0, y + 4.0, 220.0, 36.0);
            ctx.fill_card(b_an, 11.0, theme::ELEVATED, theme::accent_border());
            ctx.draw_text_center("Abrir Mapa de Espacio (AuraFiles)", b_an, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "sys_analyzer".into(), rect: b_an, act: SetAct::OpenDiskAnalyzer, slider: None });
        }
        Tab::Display => {
            let dc = Rect::new(cx, y, cw, 74.0);
            ctx.fill_card(dc, 16.0, theme::SURFACE, theme::accent_border());
            ctx.draw_text(&format!("Monitor Principal  ·  {} × {}", s.disp_w, s.disp_h), Rect::new(dc.x + 16.0, dc.y + 14.0, dc.w - 180.0, 22.0), TextStyle::Title16Semi, theme::TEXT_PRIMARY);
            ctx.draw_text(&format!("Frecuencia activa: {} Hz · DirectComposition Hardware", s.disp_hz), Rect::new(dc.x + 16.0, dc.y + 40.0, dc.w - 180.0, 18.0), TextStyle::Body11Med, theme::accent_light());
            y += 90.0;

            ctx.draw_text("TASA DE REFRESCO DEL MONITOR (CAMBIO REAL EN VIVO)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 24.0;
            let mut rx = cx;
            for &hz in s.disp_rates {
                let br = Rect::new(rx, y, 86.0, 36.0);
                let active = hz == s.disp_hz;
                ctx.fill_card(br, 11.0, if active { theme::accent() } else { theme::CARD_BG }, if active { theme::accent_light() } else { theme::BORDER });
                ctx.draw_text_center(&format!("{} Hz", hz), br, TextStyle::Mono12Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: format!("hz_{}", hz), rect: br, act: SetAct::SetRefreshHz(hz), slider: None });
                rx += 96.0;
                if rx + 86.0 > cx + cw {
                    rx = cx;
                    y += 44.0;
                }
            }
            y += 54.0;

            draw_toggle_row(ctx, Rect::new(cx, y, cw, 52.0), "disp_dark", "Modo Oscuro Nativo en todo Windows", "Aplica AppsUseLightTheme=0 y SystemUsesLightTheme=0", s.win_dark_mode, s.hovered, SetAct::ToggleWinDarkMode, hits);
            y += 60.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 52.0), "disp_trans", "Efectos de Transparencia de Windows", "EnableTransparency para paneles modernos y DWM", s.win_transparency, s.hovered, SetAct::ToggleWinTransparency, hits);
            y += 66.0;

            // Glass opacity slider
            let gc = Rect::new(cx, y, cw, 68.0);
            ctx.fill_card(gc, 14.0, theme::CARD_BG, theme::BORDER);
            ctx.draw_text(&format!("Opacidad Glass de AuraOS ({:.0}%)", s.settings.glass_opacity * 100.0), Rect::new(gc.x + 14.0, gc.y + 10.0, 340.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
            let tr = Rect::new(gc.x + 14.0, gc.y + 38.0, gc.w - 28.0, 12.0);
            ctx.fill_hatched_track(tr, 6.0);
            let frac = ((s.settings.glass_opacity - 0.50) / 0.46).clamp(0.0, 1.0);
            ctx.fill_rrect(Rect::new(tr.x, tr.y, (frac * tr.w).max(12.0), 12.0), 6.0, theme::accent());
            hits.push(Hit {
                id: "sl_glass".into(),
                rect: Rect::new(tr.x, tr.y - 6.0, tr.w, 24.0),
                act: SetAct::SetGlassOpacity(s.settings.glass_opacity),
                slider: Some(SliderKind::GlassOpacity { x: tr.x, w: tr.w }),
            });
        }
        Tab::Sound => {
            // Master volume hero card
            let mc = Rect::new(cx, y, cw, 74.0);
            ctx.fill_card(mc, 16.0, theme::SURFACE, theme::accent_border());
            let mute_r = Rect::new(mc.x + 14.0, mc.y + 19.0, 36.0, 36.0);
            ctx.fill_rrect(mute_r, 10.0, if s.master_muted { theme::CORAL_BG } else { theme::accent_tint() });
            ctx.draw_vector_icon(if s.master_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume }, mute_r.x + 18.0, mute_r.y + 18.0, 15.0, if s.master_muted { theme::CORAL } else { theme::accent_light() });
            hits.push(Hit { id: "snd_mute".into(), rect: mute_r, act: SetAct::ToggleMasterMute, slider: None });

            ctx.draw_text("Volumen Maestro del Sistema", Rect::new(mc.x + 62.0, mc.y + 12.0, 260.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
            ctx.draw_text_right(&format!("{}%", s.master_vol), Rect::new(mc.x + mc.w - 86.0, mc.y + 12.0, 70.0, 18.0), TextStyle::Mono12Bold, theme::accent_light());
            let sl_r = Rect::new(mc.x + 62.0, mc.y + 40.0, mc.w - 78.0, 12.0);
            ctx.fill_hatched_track(sl_r, 6.0);
            ctx.fill_rrect(Rect::new(sl_r.x, sl_r.y, (s.master_vol as f32 / 100.0 * sl_r.w).max(8.0), 12.0), 6.0, if s.master_muted { theme::CORAL } else { theme::accent() });
            hits.push(Hit {
                id: "sl_master".into(),
                rect: Rect::new(sl_r.x, sl_r.y - 6.0, sl_r.w, 24.0),
                act: SetAct::SetMasterVol(s.master_vol),
                slider: Some(SliderKind::MasterVol { x: sl_r.x, w: sl_r.w }),
            });
            y += 90.0;

            // Output & Input Devices side by side
            let col_w = (cw - 14.0) * 0.5;
            ctx.draw_text("DISPOSITIVOS DE SALIDA", Rect::new(cx, y, col_w, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            ctx.draw_text("MICRÓFONOS (ENTRADA)", Rect::new(cx + col_w + 14.0, y, col_w, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;

            let max_dev = s.outputs.len().max(s.mics.len()).min(5).max(1);
            for i in 0..max_dev {
                if let Some(d) = s.outputs.get(i) {
                    let dr = Rect::new(cx, y + i as f32 * 44.0, col_w, 38.0);
                    ctx.fill_card(dr, 11.0, if d.is_default { theme::accent_tint() } else { theme::CARD_BG }, if d.is_default { theme::accent_light() } else { theme::BORDER });
                    ctx.draw_text(&d.short_name, Rect::new(dr.x + 12.0, dr.y, dr.w - 24.0, 38.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                    hits.push(Hit { id: format!("out_{}", i), rect: dr, act: SetAct::SelectOutput(d.id.clone()), slider: None });
                }
                if let Some(m) = s.mics.get(i) {
                    let mr = Rect::new(cx + col_w + 14.0, y + i as f32 * 44.0, col_w, 38.0);
                    ctx.fill_card(mr, 11.0, if m.is_default { theme::accent_tint() } else { theme::CARD_BG }, if m.is_default { theme::accent_light() } else { theme::BORDER });
                    ctx.draw_text(&m.short_name, Rect::new(mr.x + 12.0, mr.y, mr.w - 24.0, 38.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                    hits.push(Hit { id: format!("mic_{}", i), rect: mr, act: SetAct::SelectMic(m.id.clone()), slider: None });
                }
            }
            y += max_dev as f32 * 44.0 + 14.0;

            // Per-App Volume Mixer!
            ctx.draw_text("MEZCLADOR DE VOLUMEN POR APLICACIÓN", Rect::new(cx, y, cw - 120.0, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            let ref_r = Rect::new(cx + cw - 96.0, y - 4.0, 96.0, 24.0);
            ctx.fill_card(ref_r, 8.0, theme::ELEVATED, theme::BORDER);
            ctx.draw_text_center("Actualizar", ref_r, TextStyle::Small10Med, theme::accent_light());
            hits.push(Hit { id: "snd_ref".into(), rect: ref_r, act: SetAct::RefreshAudio, slider: None });
            y += 26.0;

            if s.app_sessions.is_empty() {
                ctx.draw_text("No hay aplicaciones reproduciendo audio ahora mismo.", Rect::new(cx, y, cw, 24.0), TextStyle::Body11Reg, theme::TEXT_TERTIARY);
            } else {
                for (i, app) in s.app_sessions.iter().enumerate() {
                    let ar = Rect::new(cx, y, cw, 46.0);
                    ctx.fill_card(ar, 12.0, theme::CARD_BG, theme::BORDER);
                    let mb = Rect::new(ar.x + 10.0, ar.y + 9.0, 28.0, 28.0);
                    ctx.fill_rrect(mb, 8.0, if app.is_muted { theme::CORAL_BG } else { theme::ELEVATED });
                    ctx.draw_vector_icon(if app.is_muted { VectorIcon::VolumeMute } else { VectorIcon::Volume }, mb.x + 14.0, mb.y + 14.0, 11.5, if app.is_muted { theme::CORAL } else { theme::TEXT_PRIMARY });
                    hits.push(Hit { id: format!("amute_{}", i), rect: mb, act: SetAct::ToggleAppMute(app.pids.clone(), !app.is_muted), slider: None });

                    ctx.draw_text(&app.display_name, Rect::new(ar.x + 48.0, ar.y, 178.0, 46.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                    let tr = Rect::new(ar.x + 234.0, ar.y + 18.0, (ar.w - 306.0).max(120.0), 10.0);
                    ctx.fill_hatched_track(tr, 5.0);
                    ctx.fill_rrect(Rect::new(tr.x, tr.y, (app.volume_percent as f32 / 100.0 * tr.w).max(6.0), 10.0), 5.0, if app.is_muted { theme::CORAL } else { theme::accent() });
                    hits.push(Hit {
                        id: format!("asl_{}", i),
                        rect: Rect::new(tr.x, tr.y - 8.0, tr.w, 26.0),
                        act: SetAct::SetAppVol(app.pids.clone(), app.volume_percent),
                        slider: Some(SliderKind::AppVol { x: tr.x, w: tr.w, pids: app.pids.clone() }),
                    });
                    ctx.draw_text_right(&format!("{}%", app.volume_percent), Rect::new(ar.x + ar.w - 58.0, ar.y, 44.0, 46.0), TextStyle::Mono11Med, theme::TEXT_SECONDARY);
                    y += 52.0;
                }
            }
        }
        Tab::Network => {
            // 1. HERO CONNECTION STATUS & LIVE ADAPTER TELEMETRY (IPv4, Gateway, DNS, MAC, Link Speed, Ping!)
            let nc = Rect::new(cx, y, cw, 114.0);
            ctx.fill_card(nc, 16.0, theme::SURFACE, theme::accent_border());
            let net_icon = match s.radios.net_kind {
                radios::NetKind::Ethernet => VectorIcon::Ethernet,
                radios::NetKind::Wifi => VectorIcon::Wifi,
                radios::NetKind::Offline => VectorIcon::NoInternet,
            };
            let net_col = match s.radios.net_kind {
                radios::NetKind::Ethernet => theme::EMERALD,
                radios::NetKind::Wifi => theme::accent_light(),
                radios::NetKind::Offline => theme::CORAL,
            };
            ctx.draw_vector_icon(net_icon, nc.x + 32.0, nc.y + 34.0, 22.0, net_col);
            ctx.draw_text(
                &format!("{}  ·  {}", s.radios.net_label, if s.net_details.adapter_name.is_empty() { "Red" } else { &s.net_details.adapter_name }),
                Rect::new(nc.x + 62.0, nc.y + 12.0, cw - 208.0, 22.0),
                TextStyle::Title16Semi,
                theme::TEXT_PRIMARY,
            );
            let ip_line = if !s.net_details.ipv4.is_empty() {
                format!(
                    "IPv4: {}   ·   Puerta de enlace: {}   ·   Enlace: {} Mbps",
                    s.net_details.ipv4,
                    if s.net_details.gateway.is_empty() { "—" } else { &s.net_details.gateway },
                    s.net_details.link_speed_mbps
                )
            } else {
                "Sin dirección IPv4 asignada actualmente".to_string()
            };
            ctx.draw_text(&ip_line, Rect::new(nc.x + 62.0, nc.y + 38.0, cw - 80.0, 18.0), TextStyle::Mono11Med, theme::TEXT_SECONDARY);

            let dns_str = if s.net_details.dns_servers.is_empty() {
                "Automático".to_string()
            } else {
                s.net_details.dns_servers.join(", ")
            };
            let proxy_str = if s.net_details.proxy_enabled {
                format!("Proxy ON ({})", s.net_details.proxy_server)
            } else {
                "Proxy: Directo".to_string()
            };
            let mac_line = format!(
                "DNS: {}   ·   MAC: {}   ·   {}",
                dns_str,
                if s.net_details.mac.is_empty() { "—" } else { &s.net_details.mac },
                proxy_str
            );
            ctx.draw_text(&mac_line, Rect::new(nc.x + 62.0, nc.y + 60.0, cw - 80.0, 16.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);

            let ping_lbl = match s.ping_ms {
                Some(ms) => format!("Ping: {} ms", ms),
                None => "Test Latencia".to_string(),
            };
            let p_btn = Rect::new(nc.x + nc.w - 134.0, nc.y + 14.0, 120.0, 30.0);
            ctx.fill_card(p_btn, 10.0, if s.hovered == Some("net_ping") { theme::accent() } else { theme::ELEVATED }, theme::accent_border());
            ctx.draw_text_center(&ping_lbl, p_btn, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "net_ping".into(), rect: p_btn, act: SetAct::TestPing, slider: None });

            let ref_net = Rect::new(nc.x + nc.w - 134.0, nc.y + 78.0, 120.0, 26.0);
            ctx.fill_card(ref_net, 9.0, theme::CARD_BG, theme::BORDER);
            ctx.draw_text_center(if s.wifi_scanning { "Escaneando..." } else { "Actualizar Red" }, ref_net, TextStyle::Small9Reg, theme::accent_light());
            hits.push(Hit { id: "net_rescan_top".into(), rect: ref_net, act: SetAct::ScanWifiNetworks, slider: None });
            y += 128.0;

            // 2. ZONA CON COBERTURA INALÁMBRICA MÓVIL (COMPARTIR INTERNET / MOBILE HOTSPOT!)
            ctx.draw_text("COMPARTIR INTERNET (ZONA WI-FI MÓVIL DE WINDOWS)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let hs = &s.radios.hotspot;
            let hs_h = if hs.supported { 138.0 } else { 66.0 };
            let hs_card = Rect::new(cx, y, cw, hs_h);
            ctx.fill_card(hs_card, 15.0, theme::CARD_BG, if hs.is_on { theme::EMERALD } else { theme::BORDER });
            ctx.draw_text("Zona con cobertura inalámbrica móvil", Rect::new(hs_card.x + 14.0, hs_card.y + 10.0, cw - 150.0, 20.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
            ctx.draw_text(
                &hs.status_reason,
                Rect::new(hs_card.x + 14.0, hs_card.y + 32.0, cw - 150.0, 16.0),
                TextStyle::Small10Med,
                if hs.is_on { theme::EMERALD } else { theme::TEXT_TERTIARY },
            );

            let hs_sw = Rect::new(hs_card.x + hs_card.w - 60.0, hs_card.y + 16.0, 44.0, 24.0);
            ctx.fill_rrect(hs_sw, 12.0, if hs.is_on { theme::EMERALD } else { theme::ELEVATED });
            ctx.fill_circle(if hs.is_on { hs_sw.x + 32.0 } else { hs_sw.x + 12.0 }, hs_sw.y + 12.0, 8.5, theme::CRISP_WHITE);
            hits.push(Hit { id: "net_hs_toggle".into(), rect: Rect::new(hs_card.x, hs_card.y, cw, 56.0), act: SetAct::ToggleHotspot, slider: None });

            if hs.supported {
                ctx.draw_line(hs_card.x + 14.0, hs_card.y + 58.0, hs_card.x + hs_card.w - 14.0, hs_card.y + 58.0, theme::BORDER, 1.0);
                ctx.draw_text("NOMBRE DE RED (SSID)", Rect::new(hs_card.x + 14.0, hs_card.y + 66.0, 240.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
                ctx.draw_text("CONTRASEÑA (MÍN. 8 CARACTERES)", Rect::new(hs_card.x + 266.0, hs_card.y + 66.0, 260.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

                let ssid_r = Rect::new(hs_card.x + 14.0, hs_card.y + 84.0, 240.0, 36.0);
                ctx.fill_card(ssid_r, 10.0, theme::SURFACE, if s.hs_ssid_focused { theme::accent_light() } else { theme::BORDER });
                let ssid_disp = format!("{}{}", s.hs_ssid_draft, if s.hs_ssid_focused { "|" } else { "" });
                ctx.draw_text(
                    if ssid_disp.is_empty() { "Nombre de red..." } else { &ssid_disp },
                    Rect::new(ssid_r.x + 10.0, ssid_r.y, ssid_r.w - 20.0, 36.0),
                    TextStyle::Mono11Med,
                    theme::TEXT_PRIMARY,
                );
                hits.push(Hit { id: "hs_ssid_in".into(), rect: ssid_r, act: SetAct::FocusHotspotSsid, slider: None });

                let pass_r = Rect::new(hs_card.x + 266.0, hs_card.y + 84.0, (cw - 422.0).max(160.0), 36.0);
                ctx.fill_card(pass_r, 10.0, theme::SURFACE, if s.hs_pass_focused { theme::accent_light() } else { theme::BORDER });
                let pass_disp = format!("{}{}", s.hs_pass_draft, if s.hs_pass_focused { "|" } else { "" });
                ctx.draw_text(
                    if pass_disp.is_empty() { "Contraseña Wi-Fi..." } else { &pass_disp },
                    Rect::new(pass_r.x + 10.0, pass_r.y, pass_r.w - 20.0, 36.0),
                    TextStyle::Mono11Med,
                    theme::TEXT_PRIMARY,
                );
                hits.push(Hit { id: "hs_pass_in".into(), rect: pass_r, act: SetAct::FocusHotspotPass, slider: None });

                let save_hs = Rect::new(hs_card.x + hs_card.w - 142.0, hs_card.y + 84.0, 128.0, 36.0);
                ctx.fill_rrect(save_hs, 10.0, if s.hovered == Some("hs_save") { theme::accent_light() } else { theme::accent() });
                ctx.draw_text_center("Guardar Hotspot", save_hs, TextStyle::Small10Bold, theme::CRISP_WHITE);
                hits.push(Hit { id: "hs_save".into(), rect: save_hs, act: SetAct::SaveHotspotConfig, slider: None });
            }
            y += hs_h + 16.0;

            // 3. RADIOS DE HARDWARE (WI-FI, BLUETOOTH, MODO AVIÓN)
            ctx.draw_text("RADIOS INALÁMBRICAS Y MODO AVIÓN", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 50.0), "net_wifi", "Radio Wi-Fi", if s.radios.wifi_available { "Activar o apagar el adaptador Wi-Fi al instante" } else { "Sin adaptador Wi-Fi detectado (PC por Ethernet)" }, s.radios.wifi_on, s.hovered, SetAct::ToggleWifi, hits);
            y += 56.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 50.0), "net_bt", "Bluetooth", if s.radios.bt_available { "Activar o apagar radio Bluetooth de Windows" } else { "Sin adaptador Bluetooth detectado" }, s.radios.bt_on, s.hovered, SetAct::ToggleBluetooth, hits);
            y += 56.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 50.0), "net_air", "Modo Avión", "Apaga todas las radios inalámbricas (Wi-Fi y Bluetooth) al instante", s.radios.airplane_mode, s.hovered, SetAct::ToggleAirplaneMode, hits);
            y += 66.0;

            // 4. REDES WI-FI CERCANAS (ESCANEAR, CONECTAR CON CONTRASEÑA, DESCONECTAR, OLVIDAR)
            ctx.draw_text("REDES WI-FI DISPONIBLES", Rect::new(cx, y, cw - 220.0, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            let scan_b = Rect::new(cx + cw - 118.0, y - 5.0, 118.0, 26.0);
            ctx.fill_card(scan_b, 9.0, theme::ELEVATED, theme::accent_border());
            ctx.draw_text_center(if s.wifi_scanning { "Buscando..." } else { "Escanear Wi-Fi" }, scan_b, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "wifi_scan_btn".into(), rect: scan_b, act: SetAct::ScanWifiNetworks, slider: None });
            y += 26.0;

            if let Some(target_ssid) = s.wifi_pwd_target {
                let pr = Rect::new(cx, y, cw, 78.0);
                ctx.fill_card(pr, 13.0, theme::SURFACE, theme::accent_light());
                ctx.draw_text(&format!("Contraseña para conectar a '{}'", target_ssid), Rect::new(pr.x + 14.0, pr.y + 8.0, cw - 28.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                let in_r = Rect::new(pr.x + 14.0, pr.y + 32.0, cw - 230.0, 34.0);
                ctx.fill_card(in_r, 9.0, theme::CARD_BG, if s.wifi_pwd_focused { theme::accent_light() } else { theme::BORDER });
                let masked = format!("{}{}", "•".repeat(s.wifi_pwd_draft.chars().count()), if s.wifi_pwd_focused { "|" } else { "" });
                ctx.draw_text(if masked.is_empty() { "Escribe la clave Wi-Fi..." } else { &masked }, Rect::new(in_r.x + 10.0, in_r.y, in_r.w - 20.0, 34.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                hits.push(Hit { id: "wifi_pwd_in".into(), rect: in_r, act: SetAct::FocusWifiPassInput, slider: None });

                let ok_r = Rect::new(in_r.x + in_r.w + 8.0, pr.y + 32.0, 96.0, 34.0);
                ctx.fill_rrect(ok_r, 9.0, theme::accent());
                ctx.draw_text_center("Conectar", ok_r, TextStyle::Small10Bold, theme::CRISP_WHITE);
                hits.push(Hit { id: "wifi_pwd_ok".into(), rect: ok_r, act: SetAct::SubmitWifiPassword, slider: None });

                let ca_r = Rect::new(ok_r.x + 102.0, pr.y + 32.0, 96.0, 34.0);
                ctx.fill_card(ca_r, 9.0, theme::ELEVATED, theme::BORDER);
                ctx.draw_text_center("Cancelar", ca_r, TextStyle::Small10Bold, theme::TEXT_SECONDARY);
                hits.push(Hit { id: "wifi_pwd_cancel".into(), rect: ca_r, act: SetAct::CancelWifiPassword, slider: None });
                y += 88.0;
            }

            if s.wifi_networks.is_empty() {
                let empty_r = Rect::new(cx, y, cw, 44.0);
                ctx.fill_card(empty_r, 12.0, theme::CARD_BG, theme::BORDER);
                let msg = if !s.radios.wifi_available {
                    "No se detectó adaptador Wi-Fi activo en este equipo."
                } else if !s.radios.wifi_on {
                    "Wi-Fi está apagado. Enciéndelo arriba para ver redes disponibles."
                } else {
                    "No se encontraron redes Wi-Fi cercanas todavía. Pulsa 'Escanear Wi-Fi'."
                };
                ctx.draw_text(msg, Rect::new(empty_r.x + 14.0, empty_r.y, cw - 28.0, 44.0), TextStyle::Body11Reg, theme::TEXT_TERTIARY);
                y += 56.0;
            } else {
                for (i, net) in s.wifi_networks.iter().take(8).enumerate() {
                    let wr = Rect::new(cx, y, cw, 44.0);
                    ctx.fill_card(wr, 12.0, if net.connected { theme::accent_tint() } else { theme::CARD_BG }, if net.connected { theme::accent_light() } else { theme::BORDER });
                    ctx.draw_wifi_signal(wr.x + 20.0, wr.y + 22.0, 14.0, net.bars.max(1), if net.connected { theme::EMERALD } else { theme::accent_light() }, theme::rgba(255, 255, 255, 0.16));
                    ctx.draw_text(&net.ssid, Rect::new(wr.x + 42.0, wr.y + 5.0, cw - 260.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
                    let meta = format!("{}% señal  ·  {}{}", net.signal_pct, net.auth, if net.saved { "  ·  Guardada" } else { "" });
                    ctx.draw_text(&meta, Rect::new(wr.x + 42.0, wr.y + 23.0, cw - 260.0, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

                    if net.saved {
                        let fg_r = Rect::new(wr.x + wr.w - 200.0, wr.y + 8.0, 78.0, 28.0);
                        ctx.fill_card(fg_r, 8.0, theme::ELEVATED, theme::BORDER);
                        ctx.draw_text_center("Olvidar", fg_r, TextStyle::Small9Reg, theme::CORAL);
                        hits.push(Hit { id: format!("wf_fg_{}", i), rect: fg_r, act: SetAct::ForgetWifi(net.ssid.clone()), slider: None });
                    }
                    let act_r = Rect::new(wr.x + wr.w - 114.0, wr.y + 8.0, 102.0, 28.0);
                    ctx.fill_rrect(act_r, 8.0, if net.connected { theme::CORAL_BG } else { theme::accent() });
                    ctx.draw_text_center(if net.connected { "Desconectar" } else { "Conectar" }, act_r, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                    let act = if net.connected {
                        SetAct::DisconnectWifi
                    } else if net.saved || !net.secured {
                        SetAct::ConnectWifiSaved(net.ssid.clone())
                    } else {
                        SetAct::PromptWifiPassword(net.ssid.clone())
                    };
                    hits.push(Hit { id: format!("wf_c_{}", i), rect: act_r, act, slider: None });
                    y += 50.0;
                }
                y += 10.0;
            }

            // 5. PERFILES VPN DE WINDOWS
            if !s.vpn_profiles.is_empty() {
                ctx.draw_text("PERFILES VPN DE WINDOWS", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
                y += 22.0;
                for (i, vpn) in s.vpn_profiles.iter().take(6).enumerate() {
                    let vr = Rect::new(cx, y, cw, 44.0);
                    ctx.fill_card(vr, 12.0, if vpn.connected { theme::accent_tint() } else { theme::CARD_BG }, if vpn.connected { theme::EMERALD } else { theme::BORDER });
                    ctx.draw_text(&vpn.name, Rect::new(vr.x + 14.0, vr.y, cw - 150.0, 44.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
                    let vb = Rect::new(vr.x + vr.w - 118.0, vr.y + 8.0, 104.0, 28.0);
                    ctx.fill_rrect(vb, 8.0, if vpn.connected { theme::CORAL_BG } else { theme::accent() });
                    ctx.draw_text_center(if vpn.connected { "Desconectar" } else { "Conectar" }, vb, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                    hits.push(Hit { id: format!("vpn_b_{}", i), rect: vb, act: SetAct::ToggleVpn(vpn.name.clone(), vpn.connected), slider: None });
                    y += 50.0;
                }
                y += 10.0;
            }

            // 6. ACELERACIÓN DNS Y HERRAMIENTAS DE RED
            ctx.draw_text("ACELERACIÓN DNS Y MANTENIMIENTO DE RED", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 24.0;
            let dns_btns = [
                ("dns_cf", "Cloudflare 1.1.1.1", SetAct::SetDnsPreset("cf")),
                ("dns_gg", "Google 8.8.8.8", SetAct::SetDnsPreset("google")),
                ("dns_q9", "Quad9 9.9.9.9", SetAct::SetDnsPreset("quad9")),
                ("dns_auto", "DHCP Automático", SetAct::SetDnsPreset("dhcp")),
            ];
            let bw = (cw - 30.0) / 4.0;
            for (i, (id, lbl, act)) in dns_btns.into_iter().enumerate() {
                let br = Rect::new(cx + i as f32 * (bw + 10.0), y, bw, 38.0);
                let hov = s.hovered == Some(id);
                ctx.fill_card(br, 11.0, if hov { theme::accent() } else { theme::CARD_BG }, theme::accent_border());
                ctx.draw_text_center(lbl, br, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: br, act, slider: None });
            }
            y += 48.0;

            let tool_btns = [
                ("dns_flush", "Limpiar caché DNS", SetAct::FlushDns),
                ("net_renew", "Renovar IP (DHCP)", SetAct::RenewIp),
                ("net_win", "Ajustes avanzados de Windows", SetAct::OpenWinNetworkSettings),
            ];
            let bw3 = (cw - 20.0) / 3.0;
            for (i, (id, lbl, act)) in tool_btns.into_iter().enumerate() {
                let br = Rect::new(cx + i as f32 * (bw3 + 10.0), y, bw3, 38.0);
                let hov = s.hovered == Some(id);
                ctx.fill_card(br, 11.0, if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
                ctx.draw_text_center(lbl, br, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: br, act, slider: None });
            }
        }
        Tab::Personalize => {
            // v5.1 Material You 3 Theme Scheme Selector!
            ctx.draw_text("TEMA MATERIAL YOU 3 (ADAPTATIVO AL FONDO DE PANTALLA)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let schemes = [
                ("th_auto", "Auto (Fondo)", "Auto", theme::accent_light()),
                ("th_violet", "Obsidian Violet", "Violet", theme::VIOLET),
                ("th_sage", "Sage Forest", "Sage", theme::rgb(148, 194, 120)),
                ("th_apricot", "Warm Apricot", "Apricot", theme::rgb(244, 162, 108)),
                ("th_mono", "Monochrome", "Mono", theme::rgb(188, 194, 208)),
            ];
            let sbw = (cw - 32.0) / 5.0;
            for (i, (id, lbl, key, dot)) in schemes.into_iter().enumerate() {
                let r = Rect::new(cx + i as f32 * (sbw + 8.0), y, sbw, 38.0);
                let active = s.settings.theme_scheme.eq_ignore_ascii_case(key);
                ctx.fill_card(r, 11.0, if active { theme::accent_tint() } else { theme::CARD_BG }, if active { theme::accent_light() } else { theme::BORDER });
                ctx.fill_circle(r.x + 14.0, r.y + 19.0, 5.5, dot);
                ctx.draw_text(lbl, Rect::new(r.x + 25.0, r.y, r.w - 30.0, 38.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: r, act: SetAct::SetThemeScheme(key), slider: None });
            }
            y += 50.0;

            // TopBar Style + Desktop Clock Style side-by-side selectors!
            let half_w = (cw - 14.0) * 0.5;
            ctx.draw_text("ESTILO BARRA SUPERIOR", Rect::new(cx, y, half_w, 15.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            ctx.draw_text("ESTILO RELOJ DEL ESCRITORIO", Rect::new(cx + half_w + 14.0, y, half_w, 15.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 20.0;
            let bar_opts = [("bar_hug", "Hug", "Hug"), ("bar_flt", "Flotante", "Float"), ("bar_rct", "Recta", "Rect")];
            let b3w = (half_w - 12.0) / 3.0;
            for (i, (id, lbl, key)) in bar_opts.into_iter().enumerate() {
                let r = Rect::new(cx + i as f32 * (b3w + 6.0), y, b3w, 32.0);
                let act = s.settings.bar_style.eq_ignore_ascii_case(key);
                ctx.fill_card(r, 10.0, if act { theme::accent() } else { theme::CARD_BG }, if act { theme::accent_light() } else { theme::BORDER });
                ctx.draw_text_center(lbl, r, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: r, act: SetAct::SetBarStyle(key), slider: None });
            }
            let clk_opts = [("clk_vert", "Vertical", "Vertical"), ("clk_cook", "Cookie", "Cookie"), ("clk_horz", "Clásico", "Horizontal")];
            for (i, (id, lbl, key)) in clk_opts.into_iter().enumerate() {
                let r = Rect::new(cx + half_w + 14.0 + i as f32 * (b3w + 6.0), y, b3w, 32.0);
                let act = s.settings.clock_widget_style.eq_ignore_ascii_case(key);
                ctx.fill_card(r, 10.0, if act { theme::accent() } else { theme::CARD_BG }, if act { theme::accent_light() } else { theme::BORDER });
                ctx.draw_text_center(lbl, r, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: r, act: SetAct::SetClockStyle(key), slider: None });
            }
            y += 46.0;

            let rows = [
                ("p_tb", "Ocultar barra clásica de Windows", "Reemplaza la barra inferior por AuraOS (Ctrl+Alt+W restaura)", s.settings.hide_windows_taskbar, "hide_taskbar"),
                ("p_hotkeys", "Atajos de Windows integrados con AuraOS", "Tecla Win → Launchpad · Ctrl+Shift+Esc → AuraTasks · Win+E/I/S", s.settings.intercept_win_hotkeys, "win_hotkeys"),
                ("p_perf", "Modo Rendimiento (PC Modesto / Ultra Ligero)", "Reduce animaciones GPU continuas para máxima fluidez en equipos ligeros", s.settings.performance_mode, "perf_mode"),
                ("p_dock_pos", "Dock a la izquierda estilo Ubuntu", "OFF = Dock inferior centrado estilo macOS", !s.settings.dock_position.eq_ignore_ascii_case("Bottom"), "dock_left"),
                ("p_dock_hide", "Auto-ocultar Dock solo al abrir apps (visible en escritorio)", "Siempre visible en tu escritorio; se esconde automáticamente al usar programas", s.settings.dock_auto_hide, "dock_auto_hide"),
                ("p_style", "Estilo AuraOS en ventanas de Windows", "Aplica modo oscuro y bordes redondeados nativos DWM", s.settings.style_windows_apps, "style_windows_apps"),
                ("p_wp_pause", "Auto-pausar fondo animado al jugar o maximizar", "0% consumo de CPU/GPU mientras juegas", s.settings.wallpaper_auto_pause, "wp_autopause"),
                ("p_boost", "Game Boost automático en modo Gamer", "Prioriza tus juegos instalados al instante", s.settings.auto_game_boost, "auto_boost"),
                ("p_widgets", "Mostrar widgets flotantes en el escritorio", "Activa o apaga todos los widgets de escritorio", s.settings.show_desktop_widgets, "show_widgets"),
            ];
            for (id, title, sub, on, key) in rows {
                draw_toggle_row(ctx, Rect::new(cx, y, cw, 48.0), id, title, sub, on, s.hovered, SetAct::ToggleShellSetting(key), hits);
                y += 54.0;
            }
            y += 8.0;

            // Weather City Auto-Geolocation / Manual Override Card
            let wc = Rect::new(cx, y, cw, 82.0);
            ctx.fill_card(wc, 14.0, theme::SURFACE, theme::BORDER);
            let city_mode = if s.settings.weather_city.trim().is_empty() {
                "Ubicación automática activa (detecta tu ciudad actual por IP)".to_string()
            } else {
                format!("Ciudad fijada manualmente: {}", s.settings.weather_city)
            };
            ctx.draw_text("UBICACIÓN DEL CLIMA Y RELOJ", Rect::new(wc.x + 14.0, wc.y + 10.0, 260.0, 14.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            ctx.draw_text(&city_mode, Rect::new(wc.x + 220.0, wc.y + 10.0, wc.w - 236.0, 14.0), TextStyle::Small10Med, theme::accent_light());

            let ci_r = Rect::new(wc.x + 14.0, wc.y + 34.0, 260.0, 34.0);
            ctx.fill_card(ci_r, 10.0, theme::CARD_BG, if s.city_focused { theme::accent_light() } else { theme::BORDER });
            let ci_txt = if s.city_draft.is_empty() {
                if s.city_focused { "|".to_string() } else { "Automático (o escribe: Bogotá, Madrid...)".to_string() }
            } else {
                format!("{}{}", s.city_draft, if s.city_focused { "|" } else { "" })
            };
            ctx.draw_text(&ci_txt, Rect::new(ci_r.x + 10.0, ci_r.y, ci_r.w - 20.0, 34.0), TextStyle::Body11Med, if s.city_draft.is_empty() && !s.city_focused { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
            hits.push(Hit { id: "city_in".into(), rect: ci_r, act: SetAct::FocusCityInput, slider: None });

            let c_save = Rect::new(ci_r.x + ci_r.w + 10.0, wc.y + 34.0, 118.0, 34.0);
            ctx.fill_rrect(c_save, 10.0, theme::accent());
            ctx.draw_text_center("Guardar ciudad", c_save, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "city_save".into(), rect: c_save, act: SetAct::SaveWeatherCity, slider: None });

            let c_auto = Rect::new(c_save.x + c_save.w + 10.0, wc.y + 34.0, 132.0, 34.0);
            ctx.fill_card(c_auto, 10.0, theme::ELEVATED, theme::BORDER);
            ctx.draw_text_center("Automático (IP)", c_auto, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "city_auto".into(), rect: c_auto, act: SetAct::ClearWeatherCity, slider: None });
            y += 94.0;

            ctx.draw_text("WIDGETS DEL ESCRITORIO (TODOS CON SEMÁFORO MACOS)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let wgs = [
                ("wg_c", "Reloj y Fecha", WidgetKind::Clock),
                ("wg_m", "Música SMTC", WidgetKind::Music),
                ("wg_w", "Clima en vivo", WidgetKind::Weather),
                ("wg_s", "Monitor CPU/RAM", WidgetKind::Monitor),
            ];
            let ww = (cw - 30.0) / 4.0;
            for (i, (id, lbl, wk)) in wgs.into_iter().enumerate() {
                let on = s.widgets.get(wk).map(|w| w.enabled).unwrap_or(false);
                let wr = Rect::new(cx + i as f32 * (ww + 10.0), y, ww, 42.0);
                ctx.fill_card(wr, 12.0, if on { theme::accent_tint() } else { theme::CARD_BG }, if on { theme::accent_light() } else { theme::BORDER });
                ctx.draw_text_center(lbl, wr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: wr, act: SetAct::ToggleWidget(wk), slider: None });
            }
        }
        Tab::LockAndWallpapers => {
            // ===== Gradient Waves (React Bits) — Default animated AuraOS wallpaper =====
            gradient_waves::set_tuning(s.settings.waves_speed, s.settings.waves_relief);
            gradient_waves::set_custom_color(&s.settings.waves_custom_color);
            let active_wave = s
                .settings
                .wallpaper_id
                .strip_prefix("preset:")
                .and_then(gradient_waves::preset_by_key);
            let shown = active_wave.unwrap_or(&gradient_waves::PRESETS[0]);
            let gc = Rect::new(cx, y, cw, 258.0);
            ctx.fill_card(gc, 16.0, theme::pal().surface, if active_wave.is_some() { theme::accent_light() } else { theme::accent_border() });

            let prev_r = Rect::new(gc.x + 14.0, gc.y + 14.0, 290.0, gc.h - 28.0);
            let _pl = ctx.push_rrect_mask_layer(prev_r, prev_r, 12.0, 1.0);
            gradient_waves::draw_waves_ex(ctx, shown.key, prev_r, 4.2, true);
            if _pl.is_some() {
                ctx.pop_layer();
            }
            ctx.stroke_rrect(prev_r, 12.0, theme::rgba(255, 255, 255, 0.12), 1.0);

            let tx = prev_r.x + prev_r.w + 18.0;
            let tw = gc.x + gc.w - tx - 14.0;
            ctx.draw_text("Gradient Waves · Fondo animado principal", Rect::new(tx, gc.y + 12.0, tw, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
            let active_hex = gradient_waves::effective_color(shown.key)
                .map(gradient_waves::format_hex_rgb)
                .unwrap_or_else(|| "#38BDF8".into());
            let status = if active_wave.is_some() {
                format!("Activo: {} ({}) · Todo AuraOS adapta su paleta al color del fondo", shown.title, active_hex)
            } else {
                "Elige un estilo o color para activarlo · la paleta de todo AuraOS se adapta sola".to_string()
            };
            ctx.draw_text(&status, Rect::new(tx, gc.y + 34.0, tw, 16.0), TextStyle::Small10Med, theme::accent_light());

            // 5 React Bits preset chips (Ocean · Dune · Dusk · Silk · Aurora)
            let n = gradient_waves::PRESETS.len() as f32;
            let chip_w = (tw - 6.0 * (n - 1.0)) / n;
            for (i, p) in gradient_waves::PRESETS.iter().enumerate() {
                let r = Rect::new(tx + i as f32 * (chip_w + 6.0), gc.y + 58.0, chip_w, 32.0);
                let id = format!("gw_p_{}", i);
                let is_on = active_wave.map(|a| a.key == p.key).unwrap_or(false) && s.settings.waves_custom_color.trim().is_empty();
                let hov = s.hovered == Some(id.as_str());
                ctx.fill_card(
                    r,
                    10.0,
                    if is_on { theme::accent_tint() } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
                    if is_on { theme::accent_light() } else { theme::BORDER },
                );
                let (pr, pg, pb) = p.wave_color;
                ctx.fill_circle(r.x + 14.0, r.y + 16.0, 5.2, theme::rgb(pr, pg, pb));
                ctx.draw_text(p.title, Rect::new(r.x + 24.0, r.y + 8.0, r.w - 28.0, 16.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id, rect: r, act: SetAct::SetDesktopWallpaper(format!("preset:{}", p.key)), slider: None });
            }

            // 8 Adaptive Color Swatches (change waveColor & entire OS theme in 1 click!)
            ctx.draw_text("COLOR ADAPTATIVO (CAMBIA LAS ONDAS Y TODO AURAOS)", Rect::new(tx, gc.y + 98.0, tw, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
            let sn = gradient_waves::COLOR_SWATCHES.len() as f32;
            let sw_w = (tw - 5.0 * (sn - 1.0)) / sn;
            let cur_hex_up = s.settings.waves_custom_color.trim().to_ascii_uppercase();
            for (i, (name, hex, (cr, cg, cb))) in gradient_waves::COLOR_SWATCHES.iter().enumerate() {
                let sr = Rect::new(tx + i as f32 * (sw_w + 5.0), gc.y + 116.0, sw_w, 28.0);
                let id = format!("gw_c_{}", i);
                let is_sel = cur_hex_up == *hex || (cur_hex_up.is_empty() && shown.wave_color == (*cr, *cg, *cb) && active_wave.is_some());
                let hov = s.hovered == Some(id.as_str());
                ctx.fill_card(
                    sr,
                    8.0,
                    if is_sel { theme::accent_tint() } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
                    if is_sel { theme::accent_light() } else { theme::BORDER },
                );
                ctx.fill_circle(sr.x + 11.0, sr.y + 14.0, 4.8, theme::rgb(*cr, *cg, *cb));
                if sw_w >= 48.0 {
                    ctx.draw_text(name, Rect::new(sr.x + 19.0, sr.y + 7.0, sr.w - 21.0, 14.0), TextStyle::Small9Reg, theme::TEXT_PRIMARY);
                }
                hits.push(Hit { id, rect: sr, act: SetAct::SetWavesCustomColor((*hex).to_string()), slider: None });
            }

            // Live tuning sliders (speed & relief multipliers)
            let sliders = [
                ("sl_gw_speed", "Velocidad", s.settings.waves_speed, gradient_waves::SPEED_MIN, gradient_waves::SPEED_MAX, gc.y + 156.0),
                ("sl_gw_relief", "Relieve", s.settings.waves_relief, gradient_waves::RELIEF_MIN, gradient_waves::RELIEF_MAX, gc.y + 202.0),
            ];
            for (id, lbl, val, mn, mx, sy) in sliders {
                let label = if id == "sl_gw_speed" && val <= 0.001 { format!("{} (Pausado)", lbl) } else { format!("{} (x{:.2})", lbl, val) };
                ctx.draw_text(&label, Rect::new(tx, sy, tw - 40.0, 16.0), TextStyle::Small10Bold, theme::TEXT_SECONDARY);
                let tr = Rect::new(tx, sy + 22.0, tw, 10.0);
                ctx.fill_hatched_track(tr, 5.0);
                let frac = ((val - mn) / (mx - mn)).clamp(0.0, 1.0);
                ctx.fill_rrect(Rect::new(tr.x, tr.y, (frac * tr.w).max(10.0), 10.0), 5.0, theme::accent());
                ctx.fill_circle(tr.x + (frac * tr.w).clamp(5.0, tr.w - 5.0), tr.y + 5.0, 7.0, theme::TEXT_PRIMARY);
                let slider = if id == "sl_gw_speed" { SliderKind::WavesSpeed { x: tr.x, w: tr.w } } else { SliderKind::WavesRelief { x: tr.x, w: tr.w } };
                hits.push(Hit { id: id.into(), rect: Rect::new(tr.x - 6.0, tr.y - 9.0, tr.w + 12.0, 28.0), act: SetAct::SetGlassOpacity(s.settings.glass_opacity), slider: Some(slider) });
            }
            y += gc.h + 16.0;

            // ===== Holo Mosaic (React Bits 1:1 GPU Port) =====
            let active_holo = s
                .settings
                .wallpaper_id
                .strip_prefix("preset:")
                .and_then(holo_mosaic::preset_by_key);
            let shown_holo = active_holo.unwrap_or(&holo_mosaic::PRESETS[0]);
            let hc = Rect::new(cx, y, cw, 248.0);
            ctx.fill_card(hc, 16.0, theme::pal().surface, if active_holo.is_some() { theme::accent_light() } else { theme::accent_border() });

            let hprev = Rect::new(hc.x + 14.0, hc.y + 14.0, 290.0, hc.h - 28.0);
            let _hpl = ctx.push_rrect_mask_layer(hprev, hprev, 12.0, 1.0);
            holo_mosaic::draw_holo_preview(ctx, shown_holo.key, hprev, 2.4);
            if _hpl.is_some() {
                ctx.pop_layer();
            }
            ctx.stroke_rrect(hprev, 12.0, theme::rgba(255, 255, 255, 0.14), 1.0);

            let htx = hprev.x + hprev.w + 18.0;
            let htw = hc.x + hc.w - htx - 14.0;
            ctx.draw_text("Holo Mosaic · Mosaico Holográfico (React Bits)", Rect::new(htx, hc.y + 12.0, htw, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
            let hstatus = if active_holo.is_some() {
                format!("Activo: {} · Glitter 3D + inclinación al cursor + volteo al clic (GPU aislada)", shown_holo.title)
            } else {
                "Elige un espectro holográfico · +16 fondos React Bits extra disponibles abajo".to_string()
            };
            ctx.draw_text(&hstatus, Rect::new(htx, hc.y + 34.0, htw, 16.0), TextStyle::Small10Med, theme::accent_light());

            let hn = holo_mosaic::PRESETS.len() as f32;
            let hchip_w = (htw - 6.0 * (hn - 1.0)) / hn;
            for (i, p) in holo_mosaic::PRESETS.iter().enumerate() {
                let r = Rect::new(htx + i as f32 * (hchip_w + 6.0), hc.y + 58.0, hchip_w, 32.0);
                let id = format!("hm_p_{}", i);
                let is_on = active_holo.map(|a| a.key == p.key).unwrap_or(false);
                let hov = s.hovered == Some(id.as_str());
                ctx.fill_card(
                    r,
                    10.0,
                    if is_on { theme::accent_tint() } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
                    if is_on { theme::accent_light() } else { theme::BORDER },
                );
                let (ar, ag, ab) = p.accent_rgb;
                ctx.fill_circle(r.x + 14.0, r.y + 16.0, 5.2, theme::rgb(ar, ag, ab));
                ctx.draw_text(p.title, Rect::new(r.x + 24.0, r.y + 8.0, r.w - 28.0, 16.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id, rect: r, act: SetAct::SetDesktopWallpaper(format!("preset:{}", p.key)), slider: None });
            }

            // 3 Filter chips (Normal · Dither · ASCII)
            ctx.draw_text("FILTRO EN TIEMPO REAL", Rect::new(htx, hc.y + 98.0, htw, 14.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
            let hfilters = [("none", "Normal"), ("dither", "Dither Bayer"), ("ascii", "ASCII Matrix")];
            let f_w = (htw - 12.0) / 3.0;
            let cur_flt = s.settings.holo_filter.trim().to_ascii_lowercase();
            for (i, (fkey, flbl)) in hfilters.iter().enumerate() {
                let fr = Rect::new(htx + i as f32 * (f_w + 6.0), hc.y + 116.0, f_w, 26.0);
                let id = format!("hm_f_{}", i);
                let is_sel = cur_flt == *fkey || (*fkey == "none" && cur_flt.is_empty());
                let hov = s.hovered == Some(id.as_str());
                ctx.fill_card(
                    fr,
                    8.0,
                    if is_sel { theme::accent_tint() } else if hov { theme::ELEVATED_HOVER } else { theme::ELEVATED },
                    if is_sel { theme::accent_light() } else { theme::BORDER },
                );
                ctx.draw_text_center(flbl, fr, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
                hits.push(Hit { id, rect: fr, act: SetAct::SetHoloFilter((*fkey).to_string()), slider: None });
            }

            let eff_sat = if s.settings.holo_saturation >= 0.0 { s.settings.holo_saturation } else { shown_holo.saturation };
            let hsliders = [
                ("sl_hm_speed", "Velocidad", s.settings.holo_speed, holo_mosaic::SPEED_MIN, holo_mosaic::SPEED_MAX, hc.y + 152.0),
                ("sl_hm_sat", "Saturación", eff_sat, holo_mosaic::SAT_MIN, holo_mosaic::SAT_MAX, hc.y + 196.0),
            ];
            for (id, lbl, val, mn, mx, sy) in hsliders {
                let label = format!("{} ({:.2})", lbl, val);
                ctx.draw_text(&label, Rect::new(htx, sy, htw - 40.0, 16.0), TextStyle::Small10Bold, theme::TEXT_SECONDARY);
                let tr = Rect::new(htx, sy + 20.0, htw, 10.0);
                ctx.fill_hatched_track(tr, 5.0);
                let frac = ((val - mn) / (mx - mn)).clamp(0.0, 1.0);
                ctx.fill_rrect(Rect::new(tr.x, tr.y, (frac * tr.w).max(10.0), 10.0), 5.0, theme::accent());
                ctx.fill_circle(tr.x + (frac * tr.w).clamp(5.0, tr.w - 5.0), tr.y + 5.0, 7.0, theme::TEXT_PRIMARY);
                let slider = if id == "sl_hm_speed" { SliderKind::HoloSpeed { x: tr.x, w: tr.w } } else { SliderKind::HoloSaturation { x: tr.x, w: tr.w } };
                hits.push(Hit { id: id.into(), rect: Rect::new(tr.x - 6.0, tr.y - 9.0, tr.w + 12.0, 28.0), act: SetAct::SetGlassOpacity(s.settings.glass_opacity), slider: Some(slider) });
            }
            y += hc.h + 16.0;

            // AuraLock Profile Photo + Security Password Card
            let lc = Rect::new(cx, y, cw, 184.0);
            ctx.fill_card(lc, 16.0, theme::SURFACE, theme::accent_border());

            // Live Circular Profile Avatar Preview (left side of card)
            let av_r = Rect::new(lc.x + 18.0, lc.y + 18.0, 62.0, 62.0);
            let mut drew_av = false;
            if !s.settings.lock_avatar_path.trim().is_empty() && Path::new(&s.settings.lock_avatar_path).exists() {
                let _l = ctx.push_rrect_mask_layer(av_r, av_r, 31.0, 1.0);
                drew_av = ctx.draw_image_file(&s.settings.lock_avatar_path, av_r, 1.0);
                if _l.is_some() { ctx.pop_layer(); }
            }
            if !drew_av {
                ctx.fill_circle(av_r.x + 31.0, av_r.y + 31.0, 31.0, theme::accent());
                let init = s.username.chars().next().unwrap_or('A').to_uppercase().to_string();
                ctx.draw_text_center(&init, av_r, TextStyle::Title22Bold, theme::TEXT_PRIMARY);
            }
            ctx.stroke_circle(av_r.x + 31.0, av_r.y + 31.0, 31.5, theme::rgba(255, 255, 255, 0.35), 1.5);

            let pin_status = if !s.settings.lock_pin_hash.is_empty() {
                "Protegido con contraseña segura de AuraLock"
            } else if s.settings.lock_use_windows_pwd {
                "Protegido con tu contraseña real de Windows"
            } else {
                "Desbloqueo rápido sin contraseña (puedes agregar una abajo)"
            };
            ctx.draw_text("Perfil y Seguridad de AuraLock (Ctrl + Alt + L)", Rect::new(lc.x + 94.0, lc.y + 16.0, 380.0, 20.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
            ctx.draw_text(pin_status, Rect::new(lc.x + 94.0, lc.y + 38.0, cw - 290.0, 16.0), TextStyle::Small10Med, theme::accent_light());

            // Profile photo buttons
            let av_pick = Rect::new(lc.x + 94.0, lc.y + 60.0, 152.0, 28.0);
            ctx.fill_card(av_pick, 9.0, theme::ELEVATED, theme::accent_border());
            ctx.draw_text_center("+ Foto de perfil", av_pick, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "lk_av_pick".into(), rect: av_pick, act: SetAct::PickLockAvatar, slider: None });

            if !s.settings.lock_avatar_path.trim().is_empty() {
                let av_clr = Rect::new(av_pick.x + av_pick.w + 8.0, lc.y + 60.0, 96.0, 28.0);
                ctx.fill_card(av_clr, 9.0, theme::CORAL_BG, theme::CORAL);
                ctx.draw_text_center("Quitar foto", av_clr, TextStyle::Small10Bold, theme::CORAL);
                hits.push(Hit { id: "lk_av_clr".into(), rect: av_clr, act: SetAct::ClearLockAvatar, slider: None });
            }

            let test_r = Rect::new(lc.x + lc.w - 178.0, lc.y + 18.0, 162.0, 34.0);
            ctx.fill_rrect(test_r, 11.0, theme::accent());
            ctx.draw_text_center("Probar AuraLock ahora", test_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "lk_test".into(), rect: test_r, act: SetAct::TestAuraLock, slider: None });

            ctx.draw_line(lc.x + 16.0, lc.y + 104.0, lc.x + lc.w - 16.0, lc.y + 104.0, theme::BORDER, 1.0);
            ctx.draw_text("CONTRASEÑA O PIN DE SEGURIDAD PARA AURALOCK", Rect::new(lc.x + 16.0, lc.y + 112.0, 380.0, 14.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);

            // Password/PIN Input Box + Save / Clear buttons
            let pin_r = Rect::new(lc.x + 16.0, lc.y + 132.0, 280.0, 36.0);
            ctx.fill_card(pin_r, 10.0, theme::CARD_BG, if s.pin_focused { theme::accent_light() } else { theme::BORDER });
            let pin_txt = if s.pin_draft.is_empty() {
                if s.pin_focused { "|".to_string() } else { "Nueva contraseña o PIN (mín. 4)...".to_string() }
            } else {
                format!("{}{}", "●".repeat(s.pin_draft.len()), if s.pin_focused { "|" } else { "" })
            };
            ctx.draw_text(&pin_txt, Rect::new(pin_r.x + 12.0, pin_r.y, pin_r.w - 24.0, 36.0), TextStyle::Body11Med, if s.pin_draft.is_empty() && !s.pin_focused { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
            hits.push(Hit { id: "lk_pin_in".into(), rect: pin_r, act: SetAct::FocusPinInput, slider: None });

            let save_pin = Rect::new(pin_r.x + pin_r.w + 10.0, lc.y + 132.0, 146.0, 36.0);
            ctx.fill_rrect(save_pin, 10.0, theme::accent());
            ctx.draw_text_center("Guardar contraseña", save_pin, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "lk_pin_save".into(), rect: save_pin, act: SetAct::SaveLockPin, slider: None });

            let clr_pin = Rect::new(save_pin.x + save_pin.w + 10.0, lc.y + 132.0, 128.0, 36.0);
            ctx.fill_card(clr_pin, 10.0, theme::CORAL_BG, theme::CORAL);
            ctx.draw_text_center("Quitar contraseña", clr_pin, TextStyle::Body11Med, theme::CORAL);
            hits.push(Hit { id: "lk_pin_clr".into(), rect: clr_pin, act: SetAct::ClearLockPin, slider: None });
            y += 198.0;

            draw_toggle_row(ctx, Rect::new(cx, y, cw, 48.0), "lk_wpwd", "Verificar con contraseña real de Windows (LogonUserW)", "Usa tu contraseña de cuenta local de Windows para desbloquear AuraLock", s.settings.lock_use_windows_pwd, s.hovered, SetAct::ToggleShellSetting("lock_win_pwd"), hits);
            y += 55.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 48.0), "lk_boot", "Mostrar AuraLock automáticamente al iniciar AuraShell", "Abre la pantalla de bloqueo GPU justo al cargar AuraOS", s.settings.lock_on_startup, s.hovered, SetAct::ToggleShellSetting("lock_startup"), hits);
            y += 55.0;
            draw_toggle_row(ctx, Rect::new(cx, y, cw, 48.0), "lk_med", "Mostrar controles de música en pantalla de bloqueo", "Mini reproductor flotante con Spotify/YouTube", s.settings.lock_show_media, s.hovered, SetAct::ToggleShellSetting("lock_media"), hits);
            y += 66.0;

            // Custom Wallpapers Picker Header
            ctx.draw_text("FONDOS PARA ESCRITORIO Y AURALOCK (IMAGEN, VIDEO O WORKSHOP)", Rect::new(cx, y, cw - 330.0, 18.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            let b_lwp = Rect::new(cx + cw - 178.0, y - 5.0, 178.0, 28.0);
            ctx.fill_rrect(b_lwp, 9.0, theme::accent());
            ctx.draw_text_center("+ Archivo para AuraLock", b_lwp, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "wp_pick_lock".into(), rect: b_lwp, act: SetAct::PickLocalLockWallpaper, slider: None });

            let b_dwp = Rect::new(b_lwp.x - 168.0, y - 5.0, 160.0, 28.0);
            ctx.fill_card(b_dwp, 9.0, theme::ELEVATED, theme::accent_border());
            ctx.draw_text_center("+ Archivo Escritorio", b_dwp, TextStyle::Small10Bold, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "wp_pick_desk".into(), rect: b_dwp, act: SetAct::PickLocalDesktopWallpaper, slider: None });
            y += 32.0;

            let cols = 3usize;
            let gw = (cw - 20.0) / cols as f32;
            let gh = 148.0;
            for (i, wp) in s.wallpapers.iter().filter(|w| w.id != "external").take(18).enumerate() {
                let col = i % cols;
                let row = i / cols;
                let cr = Rect::new(cx + col as f32 * (gw + 10.0), y + row as f32 * (gh + 12.0), gw, gh);
                let is_desk = s.settings.wallpaper_id == wp.id;
                let is_lock = s.settings.lock_wallpaper_id == wp.id;
                ctx.fill_card(cr, 14.0, theme::pal().card_bg, if is_lock || is_desk { theme::accent_light() } else { theme::pal().border });
                let prev_r = Rect::new(cr.x + 5.0, cr.y + 5.0, cr.w - 10.0, 84.0);
                let _pclip = ctx.push_rrect_mask_layer(prev_r, prev_r, 9.0, 1.0);
                let drew = wp.preview_path.as_deref().map(|p| ctx.draw_image_file(p, prev_r, 1.0)).unwrap_or(false);
                if !drew {
                    if wp.kind == aura_os::models::WallpaperKind::Preset {
                        wallpaper::draw_wallpaper_preview_in_rect(ctx, &wp.file_path, prev_r);
                    } else {
                        ctx.fill_gradient_rrect(prev_r, 9.0, theme::accent_deep(), theme::pal().surface);
                    }
                }
                if _pclip.is_some() {
                    ctx.pop_layer();
                }
                ctx.draw_text(&wp.title, Rect::new(cr.x + 10.0, cr.y + 92.0, cr.w - 20.0, 16.0), TextStyle::Small10Bold, theme::TEXT_PRIMARY);

                let bw = (cr.w - 22.0) * 0.5;
                let b1 = Rect::new(cr.x + 8.0, cr.y + 114.0, bw, 24.0);
                let b2 = Rect::new(b1.x + bw + 6.0, cr.y + 114.0, bw, 24.0);
                ctx.fill_rrect(b1, 8.0, if is_desk { theme::accent() } else { theme::ELEVATED });
                ctx.draw_text_center(if is_desk { "Escritorio ✓" } else { "Escritorio" }, b1, TextStyle::Small9Reg, theme::TEXT_PRIMARY);
                ctx.fill_rrect(b2, 8.0, if is_lock { theme::EMERALD } else { theme::ELEVATED });
                ctx.draw_text_center(if is_lock { "AuraLock ✓" } else { "AuraLock" }, b2, TextStyle::Small9Reg, if is_lock { theme::DARK_INK } else { theme::TEXT_PRIMARY });
                hits.push(Hit { id: format!("wp_d_{}", i), rect: b1, act: SetAct::SetDesktopWallpaper(wp.id.clone()), slider: None });
                hits.push(Hit { id: format!("wp_l_{}", i), rect: b2, act: SetAct::SetLockWallpaper(wp.id.clone()), slider: None });
            }
        }
        Tab::StartupAndApps => {
            ctx.draw_text("SUITE NATIVA AURAOS v5.2 (100% RUST + GPU)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let suite = [
                ("st_af", "AuraFiles", "AuraFiles.exe"),
                ("st_pdf", "AuraPDF Studio", "AuraPDF.exe"),
                ("st_ap", "AuraPhotos Studio", "AuraPhotos.exe"),
                ("st_sh", "AuraShot Capturas", "AuraShot.exe"),
                ("st_at", "AuraTasks Monitor", "AuraTasks.exe"),
                ("st_av", "AuraPlayer", "AuraPlayer.exe"),
                ("st_an", "AuraNotes", "AuraNotes.exe"),
                ("st_al", "AuraLock", "AuraLock.exe"),
            ];
            let cols = 4usize;
            let sw = (cw - 30.0) / cols as f32;
            for (i, (id, lbl, exe)) in suite.into_iter().enumerate() {
                let col_i = (i % cols) as f32;
                let row_i = (i / cols) as f32;
                let r = Rect::new(cx + col_i * (sw + 10.0), y + row_i * 46.0, sw, 38.0);
                let hov = s.hovered == Some(id);
                ctx.fill_card(r, 11.0, if hov { theme::accent() } else { theme::CARD_BG }, theme::accent_border());
                ctx.draw_text_center(lbl, r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
                hits.push(Hit { id: id.into(), rect: r, act: SetAct::LaunchSuiteExe(exe), slider: None });
            }
            y += 2.0 * 46.0 + 10.0;

            let apb_r = Rect::new(cx, y, cw, 46.0);
            ctx.fill_card(apb_r, 13.0, theme::accent_tint(), theme::accent_light());
            ctx.draw_text("Optimizador de Windows (APBOptimizer · OptiJuegos)", Rect::new(apb_r.x + 14.0, apb_r.y + 6.0, apb_r.w - 130.0, 18.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);
            ctx.draw_text("Reduce servicios innecesarios y mejora latencia en juegos", Rect::new(apb_r.x + 14.0, apb_r.y + 24.0, apb_r.w - 130.0, 15.0), TextStyle::Small9Reg, theme::TEXT_SECONDARY);
            ctx.draw_text_right("Ejecutar →", Rect::new(apb_r.x + apb_r.w - 108.0, apb_r.y, 92.0, 46.0), TextStyle::Body11Med, theme::accent_light());
            hits.push(Hit { id: "st_apb".into(), rect: apb_r, act: SetAct::LaunchApbOptimizer, slider: None });
            y += 60.0;

            ctx.draw_text("APLICACIONES AL INICIO DE WINDOWS (HKCU\\Run)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            for (i, entry) in s.startup_entries.iter().enumerate() {
                let er = Rect::new(cx, y, cw, 44.0);
                ctx.fill_card(er, 12.0, theme::CARD_BG, theme::BORDER);
                ctx.draw_text(&entry.name, Rect::new(er.x + 14.0, er.y + 5.0, er.w - 120.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                ctx.draw_text(&entry.cmd, Rect::new(er.x + 14.0, er.y + 23.0, er.w - 120.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);
                if !entry.name.eq_ignore_ascii_case("AuraShell") {
                    let rm = Rect::new(er.x + er.w - 92.0, er.y + 9.0, 80.0, 26.0);
                    ctx.fill_rrect(rm, 8.0, theme::CORAL_BG);
                    ctx.draw_text_center("Quitar", rm, TextStyle::Small10Bold, theme::CORAL);
                    hits.push(Hit { id: format!("rm_run_{}", i), rect: rm, act: SetAct::RemoveStartupEntry(entry.name.clone()), slider: None });
                } else {
                    ctx.draw_text_right("Activo ✓", Rect::new(er.x + er.w - 92.0, er.y, 78.0, 44.0), TextStyle::Small10Bold, theme::EMERALD);
                }
                y += 50.0;
            }
            y += 12.0;

            // Fast Native Installed Windows Programs Uninstaller (replaces slow `appwiz.cpl`!)
            ctx.draw_text(
                &format!("PROGRAMAS INSTALADOS EN WINDOWS ({} DETECTADOS · DESINSTALADOR RÁPIDO)", s.installed_programs.len()),
                Rect::new(cx, y, cw, 16.0),
                TextStyle::Small10Bold,
                theme::TEXT_TERTIARY,
            );
            y += 22.0;
            for (pi, prog) in s.installed_programs.iter().take(45).enumerate() {
                let pr = Rect::new(cx, y, cw, 46.0);
                ctx.fill_card(pr, 12.0, theme::CARD_BG, theme::BORDER);
                let ic_r = Rect::new(pr.x + 10.0, pr.y + 11.0, 24.0, 24.0);
                if prog.icon_path.is_empty() || !ctx.draw_app_icon(&prog.icon_path, ic_r) {
                    ctx.draw_vector_icon(VectorIcon::AppGrid, ic_r.x + 12.0, ic_r.y + 12.0, 13.0, theme::accent_light());
                }
                ctx.draw_text(&prog.name, Rect::new(pr.x + 42.0, pr.y + 6.0, pr.w - 180.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
                let sub = format!(
                    "{}{}{}",
                    if prog.publisher.is_empty() { "Aplicación Windows" } else { &prog.publisher },
                    if prog.version.is_empty() { String::new() } else { format!(" · v{}", prog.version) },
                    prog.size_mb.map(|m| format!(" · {:.1} MB", m)).unwrap_or_default()
                );
                ctx.draw_text(&sub, Rect::new(pr.x + 42.0, pr.y + 24.0, pr.w - 180.0, 15.0), TextStyle::Small9Reg, theme::TEXT_TERTIARY);

                let ub = Rect::new(pr.x + pr.w - 108.0, pr.y + 9.0, 96.0, 28.0);
                let uhov = s.hovered == Some(&format!("uninst_{}", pi));
                ctx.fill_card(ub, 8.0, if uhov { theme::CORAL } else { theme::CORAL_BG }, theme::CORAL);
                ctx.draw_text_center("Desinstalar", ub, TextStyle::Small10Bold, if uhov { theme::DARK_INK } else { theme::CORAL });
                hits.push(Hit {
                    id: format!("uninst_{}", pi),
                    rect: ub,
                    act: SetAct::UninstallProgram(prog.uninstall_cmd.clone()),
                    slider: None,
                });
                y += 52.0;
            }
        }
        Tab::Updates => {
            let uc = Rect::new(cx, y, cw, 92.0);
            ctx.fill_card(uc, 16.0, theme::SURFACE, theme::accent_border());
            ctx.draw_text(
                &format!("AuraOS v{} (Aura Studio · AuraBeats · Motor 60 FPS)", env!("CARGO_PKG_VERSION")),
                Rect::new(uc.x + 16.0, uc.y + 14.0, 560.0, 24.0),
                TextStyle::Title16Semi,
                theme::TEXT_PRIMARY,
            );
            ctx.draw_text("AuraShell · AuraWallpaper · AuraBeats DAW · AuraPDF · AuraPhotos · AuraShot · AuraTasks · AuraFiles", Rect::new(uc.x + 16.0, uc.y + 42.0, cw - 32.0, 18.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);
            ctx.draw_text("Actualizaciones automáticas silenciosas compatibles con GitHub Releases y Discord", Rect::new(uc.x + 16.0, uc.y + 63.0, cw - 32.0, 16.0), TextStyle::Small10Med, theme::accent_light());
            y += 108.0;

            draw_toggle_row(ctx, Rect::new(cx, y, cw, 52.0), "upd_auto", "Aplicar actualizaciones automáticamente en segundo plano", "Mantiene AuraOS al día sin reinstalar manualmente", s.settings.auto_apply_updates, s.hovered, SetAct::ToggleShellSetting("auto_updates"), hits);
            y += 66.0;

            ctx.draw_text("MANIFIESTO GITHUB RELEASES (URL DE VERSIONES)", Rect::new(cx, y, cw, 16.0), TextStyle::Small10Bold, theme::TEXT_TERTIARY);
            y += 22.0;
            let ur = Rect::new(cx, y, cw - 128.0, 38.0);
            ctx.fill_card(ur, 11.0, theme::CARD_BG, if s.github_focused { theme::accent_light() } else { theme::BORDER });
            let g_disp = if s.github_draft.is_empty() {
                if s.github_focused { "|".to_string() } else { "https://api.github.com/repos/TU_USUARIO/AuraOS/releases/latest".to_string() }
            } else if s.github_focused {
                format!("{}|", s.github_draft)
            } else {
                s.github_draft.to_string()
            };
            ctx.draw_text(&g_disp, Rect::new(ur.x + 12.0, ur.y, ur.w - 24.0, 38.0), TextStyle::Mono11Med, if s.github_draft.is_empty() && !s.github_focused { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY });
            hits.push(Hit { id: "upd_gh_in".into(), rect: ur, act: SetAct::FocusGithubUrl, slider: None });

            let sg = Rect::new(ur.x + ur.w + 10.0, y, 118.0, 38.0);
            ctx.fill_rrect(sg, 11.0, theme::accent());
            ctx.draw_text_center("Guardar URL", sg, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "upd_gh_save".into(), rect: sg, act: SetAct::SaveGithubUrl, slider: None });
            y += 56.0;

            let b1 = Rect::new(cx, y, 220.0, 38.0);
            ctx.fill_rrect(b1, 11.0, theme::accent());
            ctx.draw_text_center("Abrir Gestor / Instalador", b1, TextStyle::Body11Med, theme::TEXT_PRIMARY);
            hits.push(Hit { id: "upd_inst".into(), rect: b1, act: SetAct::LaunchInstaller, slider: None });

            let b2 = Rect::new(b1.x + 232.0, y, 220.0, 38.0);
            ctx.fill_card(b2, 11.0, theme::ELEVATED, theme::BORDER);
            ctx.draw_text_center("Configuración clásica Windows", b2, TextStyle::Body11Med, theme::TEXT_SECONDARY);
            hits.push(Hit { id: "upd_win".into(), rect: b2, act: SetAct::OpenClassicWinSettings, slider: None });

            let b3 = Rect::new(b2.x + 232.0, y, 210.0, 38.0);
            ctx.fill_card(b3, 11.0, theme::CORAL_BG, theme::CORAL);
            ctx.draw_text_center("Restaurar barra Windows", b3, TextStyle::Body11Med, theme::CORAL);
            hits.push(Hit { id: "upd_res".into(), rect: b3, act: SetAct::RestoreWindowsTaskbar, slider: None });
        }
    }

    ctx.pop_clip();

    if let Some((txt, ok)) = s.toast {
        let tw = (txt.len() as f32 * 6.8 + 38.0).clamp(210.0, 480.0);
        let tr = Rect::new(SB_W + ((w - SB_W - tw) * 0.5), h - 48.0, tw, 34.0);
        ctx.fill_card(tr, 12.0, theme::ELEVATED, if ok { theme::EMERALD } else { theme::CORAL });
        ctx.draw_text_center(txt, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }

    if _clip.is_some() {
        ctx.stroke_rrect(bounds.inset(0.5, 0.5), corner, theme::BORDER_STRONG, 1.0);
        ctx.pop_layer();
    }
}

unsafe extern "system" fn settings_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        let _ = ShowWindow(hwnd, SW_HIDE);
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_NCCALCSIZE {
        if let Some(res) = aura_window::handle_nccalcsize(hwnd, wparam, lparam) {
            return res;
        }
    }
    if msg == WM_GETMINMAXINFO {
        return aura_window::handle_getminmaxinfo(hwnd, lparam, 680, 480);
    }
    if msg == WM_NCHITTEST {
        return aura_window::handle_nchittest(hwnd, lparam, TOP_H, |lx, ly| {
            SET_RT.with(|c| {
                c.try_borrow()
                    .ok()
                    .and_then(|g| g.as_ref().map(|r| r.hits.iter().any(|h| h.rect.contains(lx, ly))))
                    .unwrap_or(false)
            })
        });
    }
    if msg == WM_SYSKEYDOWN {
        let vk = wparam.0 as u16;
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_LEFT.0 { aura_window::snap_window(hwnd, SnapZone::LeftHalf); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_RIGHT.0 { aura_window::snap_window(hwnd, SnapZone::RightHalf); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_UP.0 { aura_window::snap_window(hwnd, SnapZone::Maximize); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0 { aura_window::snap_window(hwnd, SnapZone::Center); return LRESULT(0); }
    }

    let mut start_title_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;
    let mut deferred_snap: Option<SnapZone> = None;

    let handled = SET_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        if rt.wm_palette_changed != 0 && msg == rt.wm_palette_changed {
            let _ = theme::load_active_palette();
            rt.settings = load_settings();
            gradient_waves::set_tuning(rt.settings.waves_speed, rt.settings.waves_relief);
            gradient_waves::set_custom_color(&rt.settings.waves_custom_color);
            rt.redraw();
            return true;
        }
        match msg {
            WM_TIMER if wparam.0 == TIMER_POLL => {
                rt.poll_bg();
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let raw_w = rc.right - rc.left;
                    let raw_h = rc.bottom - rc.top;
                    if raw_w > 0 && raw_h > 0 {
                        let nw = raw_w.max(680) as u32;
                        let nh = raw_h.max(460) as u32;
                        rt.maximized = IsZoomed(hwnd).as_bool();
                        rt.width = nw as f32;
                        rt.height = nh as f32;
                        rt.surf.resize(&rt.gpu, nw, nh);
                        rt.redraw();
                    }
                }
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if rt.active_slider.is_some() {
                    rt.apply_slider_drag(px);
                    return true;
                }
                let nh = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).map(|h| h.id.clone());
                if nh != rt.hovered_id {
                    rt.hovered_id = nh;
                    rt.redraw();
                }
                true
            }
            WM_LBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).cloned() {
                    if let Some(sl) = hit.slider {
                        rt.active_slider = Some(sl);
                        SetCapture(hwnd);
                        rt.apply_slider_drag(px);
                    } else {
                        match hit.act {
                            SetAct::WinMax => {
                                rt.maximized = !rt.maximized;
                                deferred_max = Some(rt.maximized);
                            }
                            SetAct::WinMin => {
                                deferred_min = true;
                            }
                            other => rt.execute(other),
                        }
                    }
                    return true;
                }
                if py < TOP_H {
                    start_title_drag = true;
                }
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)) {
                    if matches!(hit.act, SetAct::WinMax) {
                        deferred_snap = Some(SnapZone::LeftHalf);
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if let Some(sl) = rt.active_slider.take() {
                    let _ = ReleaseCapture();
                    if matches!(
                        sl,
                        SliderKind::WavesSpeed { .. }
                            | SliderKind::WavesRelief { .. }
                            | SliderKind::HoloSpeed { .. }
                            | SliderKind::HoloSaturation { .. }
                    ) {
                        rt.save_and_broadcast();
                    }
                }
                true
            }
            0x0215 | 0x001F => {
                // WM_CAPTURECHANGED / WM_CANCELMODE
                rt.active_slider = None;
                true
            }
            WM_LBUTTONDBLCLK => {
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if py < TOP_H {
                    rt.maximized = !rt.maximized;
                    deferred_max = Some(rt.maximized);
                }
                true
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) as i16) as f32;
                let step = if delta > 0.0 { -64.0 } else { 64.0 };
                rt.scroll = (rt.scroll + step).clamp(0.0, 1600.0);
                rt.redraw();
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_ESCAPE.0 {
                    rt.unfocus_all_inputs();
                    rt.wifi_pwd_target = None;
                    rt.redraw();
                } else if vk == VK_BACK.0 {
                    if rt.pin_focused {
                        rt.pin_draft.pop();
                        rt.redraw();
                    } else if rt.city_focused {
                        rt.city_draft.pop();
                        rt.redraw();
                    } else if rt.github_focused {
                        rt.github_draft.pop();
                        rt.redraw();
                    } else if rt.hs_ssid_focused {
                        rt.hs_ssid_draft.pop();
                        rt.redraw();
                    } else if rt.hs_pass_focused {
                        rt.hs_pass_draft.pop();
                        rt.redraw();
                    } else if rt.wifi_pwd_focused {
                        rt.wifi_pwd_draft.pop();
                        rt.redraw();
                    }
                } else if vk == VK_RETURN.0 {
                    if rt.pin_focused {
                        rt.execute(SetAct::SaveLockPin);
                    } else if rt.city_focused {
                        rt.execute(SetAct::SaveWeatherCity);
                    } else if rt.github_focused {
                        rt.execute(SetAct::SaveGithubUrl);
                    } else if rt.hs_ssid_focused || rt.hs_pass_focused {
                        rt.execute(SetAct::SaveHotspotConfig);
                    } else if rt.wifi_pwd_focused {
                        rt.execute(SetAct::SubmitWifiPassword);
                    }
                }
                true
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam.0 as u32) {
                    if !ch.is_control() {
                        if rt.pin_focused && rt.pin_draft.len() < 32 {
                            rt.pin_draft.push(ch);
                            rt.redraw();
                        } else if rt.city_focused && rt.city_draft.len() < 48 {
                            rt.city_draft.push(ch);
                            rt.redraw();
                        } else if rt.github_focused && rt.github_draft.len() < 160 {
                            rt.github_draft.push(ch);
                            rt.redraw();
                        } else if rt.hs_ssid_focused && rt.hs_ssid_draft.len() < 32 {
                            rt.hs_ssid_draft.push(ch);
                            rt.redraw();
                        } else if rt.hs_pass_focused && rt.hs_pass_draft.len() < 63 {
                            rt.hs_pass_draft.push(ch);
                            rt.redraw();
                        } else if rt.wifi_pwd_focused && rt.wifi_pwd_draft.len() < 64 {
                            rt.wifi_pwd_draft.push(ch);
                            rt.redraw();
                        }
                    }
                }
                true
            }
            _ => false,
        }
    });

    if start_title_drag {
        let _ = ReleaseCapture();
        let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, WPARAM(HTCAPTION as usize), LPARAM(0));
    } else if deferred_min {
        if !win32_shell::request_genie_minimize(hwnd) {
            let _ = ShowWindow(hwnd, SW_MINIMIZE);
        }
    } else if deferred_snap.is_some() {
        aura_window::cycle_snap_window(hwnd);
    } else if let Some(max) = deferred_max {
        let _ = ShowWindow(hwnd, if max { SW_MAXIMIZE } else { SW_RESTORE });
    }

    if handled {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

fn capture_settings_png(out_png: &Path, tab: Tab) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let _ = theme::load_active_palette();
    let settings = load_settings();
    gradient_waves::set_tuning(settings.waves_speed, settings.waves_relief);
    gradient_waves::set_custom_color(&settings.waves_custom_color);
    let (sw, sh) = win32_shell::screen_size();
    let widgets = load_widgets(sw, sh);
    let custom = load_custom_items();
    let wallpapers = wallpaper::scan_wallpapers(&custom);
    let drives = system::get_local_drives();
    let startup_entries = list_hkcu_run_entries();
    let installed_programs = system::scan_installed_programs();
    let (dw, dh, dhz, drates) = get_display_info();
    let (ru, rt_gb, rp) = system::get_ram_stats();
    let (mv, mm) = audio::get_master_volume();
    let outputs = audio::get_audio_devices(false);
    let mics = audio::get_audio_devices(true);
    let apps = audio::get_app_audio_sessions();
    let radios = radios::query_radio_states();
    let net_details = radios::query_network_details();
    let wifi_networks = radios::scan_wifi_networks(&radios.wifi_ssid);
    let vpn_profiles = radios::list_vpn_profiles();
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "Usuario".into());
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "AuraPC".into());
    let snap = SettingsSnap {
        w: 1180.0,
        h: 740.0,
        maximized: false,
        active_tab: tab,
        scroll: 0.0,
        settings: &settings,
        widgets: &widgets,
        wallpapers: &wallpapers,
        drives: &drives,
        startup_entries: &startup_entries,
        installed_programs: &installed_programs,
        disp_w: dw,
        disp_h: dh,
        disp_hz: dhz,
        disp_rates: &drates,
        win_dark_mode: true,
        win_transparency: true,
        win_animations: true,
        power_plan: "high",
        temp_bytes: 128_000_000,
        master_vol: mv,
        master_muted: mm,
        outputs: &outputs,
        mics: &mics,
        app_sessions: &apps,
        radios: &radios,
        net_details: &net_details,
        wifi_networks: &wifi_networks,
        wifi_scanning: false,
        vpn_profiles: &vpn_profiles,
        ping_ms: Some(14),
        hs_ssid_draft: &radios.hotspot.ssid,
        hs_pass_draft: &radios.hotspot.passphrase,
        hs_ssid_focused: false,
        hs_pass_focused: false,
        wifi_pwd_target: None,
        wifi_pwd_draft: "",
        wifi_pwd_focused: false,
        pin_draft: "",
        pin_focused: false,
        city_draft: &settings.weather_city,
        city_focused: false,
        github_draft: &settings.update_manifest_url,
        github_focused: false,
        toast: None,
        hovered: None,
        ram_used: ru,
        ram_total: rt_gb,
        ram_pct: rp,
        hostname: &hostname,
        username: &username,
        cpu_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8),
    };
    let _ = gpu.render_to_png(1180, 740, out_png, |ctx| {
        let mut h = Vec::new();
        render_settings_ui(ctx, &snap, &mut h);
    });
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        let tab = args.get(3).and_then(|s| Tab::from_arg(s)).unwrap_or(Tab::System);
        capture_settings_png(Path::new(&args[2]), tab);
        return;
    }

    let mut init_tab = Tab::System;
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--tab") {
        if let Some(t) = Tab::from_arg(&args[2]) {
            init_tab = t;
        }
    }

    let Ok(mut gpu) = GpuEngine::new() else { return };
    gpu.enable_async_loader();
    let (sw, sh) = win32_shell::screen_size();
    let w = (sw - 220).clamp(960, 1220);
    let h = (sh - 140).clamp(640, 780);
    let x = (sw - w) / 2;
    let y = ((sh - h) / 2).max(40);

    let hinst = unsafe { GetModuleHandleW(None).unwrap_or_default() };
    let cls_name = win32_shell::to_wide("AuraSettings_GPU_v43_Wnd");
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(settings_wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(cls_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }

    let title = win32_shell::to_wide("Configuración de AuraOS");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            PCWSTR(cls_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            aura_window::AURA_APP_STYLE,
            x,
            y,
            w,
            h,
            HWND::default(),
            None,
            hinst,
            None,
        )
        .unwrap_or_default()
    };
    if hwnd.is_invalid() {
        return;
    }
    aura_window::configure_frameless_dwm(hwnd);

    let Ok(surf) = WindowSurface::new(&gpu, hwnd, w as u32, h as u32) else { return };

    let _ = theme::load_active_palette();
    let settings = load_settings();
    gradient_waves::set_tuning(settings.waves_speed, settings.waves_relief);
    gradient_waves::set_custom_color(&settings.waves_custom_color);
    let widgets = load_widgets(sw, sh);
    let custom = load_custom_items();
    let wallpapers = wallpaper::scan_wallpapers(&custom);
    gpu.prewarm_images(wallpapers.iter().filter_map(|wp| wp.preview_path.clone()));
    let drives = system::get_local_drives();
    let startup_entries = list_hkcu_run_entries();
    let installed_programs = system::scan_installed_programs();
    let (disp_w, disp_h, disp_hz, disp_rates) = get_display_info();
    let (ram_used, ram_total, ram_pct) = system::get_ram_stats();
    let win_dark_mode = read_hkcu_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "AppsUseLightTheme", 0) == 0;
    let win_transparency = read_hkcu_dword(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 1) != 0;
    let win_animations = get_win_drag_animations();

    let (bg_tx, bg_rx) = mpsc::channel();
    let tx_init = bg_tx.clone();
    std::thread::spawn(move || {
        let _ = tx_init.send(BgUpdate::PowerPlan(query_active_power_plan()));
        let _ = tx_init.send(BgUpdate::TempBytes(measure_temp_bytes()));
    });

    let github_draft = settings.update_manifest_url.clone();
    let city_draft = settings.weather_city.clone();
    let mut rt = SettingsRuntime {
        hwnd,
        gpu,
        surf,
        width: w as f32,
        height: h as f32,
        maximized: false,
        active_tab: init_tab,
        scroll: 0.0,
        settings,
        widgets,
        wallpapers,
        drives,
        startup_entries,
        installed_programs,
        disp_w,
        disp_h,
        disp_hz,
        disp_rates,
        win_dark_mode,
        win_transparency,
        win_animations,
        power_plan: "balanced".into(),
        temp_bytes: 0,
        master_vol: 50,
        master_muted: false,
        outputs: Vec::new(),
        mics: Vec::new(),
        app_sessions: Vec::new(),
        radios: RadioStates::default(),
        net_details: radios::NetworkDetails::default(),
        wifi_networks: Vec::new(),
        wifi_scanning: false,
        vpn_profiles: Vec::new(),
        ping_ms: None,
        hs_ssid_draft: String::new(),
        hs_pass_draft: String::new(),
        hs_ssid_focused: false,
        hs_pass_focused: false,
        wifi_pwd_target: None,
        wifi_pwd_draft: String::new(),
        wifi_pwd_focused: false,
        pin_draft: String::new(),
        pin_focused: false,
        city_draft,
        city_focused: false,
        github_draft,
        github_focused: false,
        active_slider: None,
        toast: None,
        hits: Vec::new(),
        hovered_id: None,
        bg_tx,
        bg_rx,
        ram_used,
        ram_total,
        ram_pct,
        hostname: std::env::var("COMPUTERNAME").unwrap_or_else(|_| "AuraPC".into()),
        username: std::env::var("USERNAME").unwrap_or_else(|_| "Usuario".into()),
        cpu_cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8),
        wm_palette_changed: win32_shell::register_palette_changed_msg(),
    };
    rt.spawn_audio_refresh();
    rt.spawn_radios_refresh();
    rt.redraw();

    SET_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, TIMER_POLL, 60, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = KillTimer(hwnd, TIMER_POLL);
    }

    SET_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            *g = None;
        }
    });
}
