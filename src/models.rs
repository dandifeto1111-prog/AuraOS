use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShellMode {
    PC,
    Dev,
    Gamer,
    Studio, // Fondos de Pantalla & Widgets del Escritorio
}

impl Default for ShellMode {
    fn default() -> Self {
        ShellMode::PC
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
    pub glass_opacity: f32,
    pub hide_windows_taskbar: bool,
    pub start_with_windows: bool,
    pub auto_game_boost: bool,
    pub dock_auto_hide: bool,
    pub show_desktop_widgets: bool,
    pub dock_position: String, // "Left" or "Bottom"
    pub last_mode: ShellMode,
    pub style_windows_apps: bool,
    pub wallpaper_id: String, // "external", "preset:...", "video:<path>", "web:<path>", "image:<path>"
    pub wallpaper_auto_pause: bool,
    pub auto_apply_updates: bool,
    pub update_manifest_url: String, // GitHub Releases API or raw version.json URL for community updates!
    // AuraLock Native Lock Screen Configuration (v4.3+)
    pub lock_wallpaper_id: String, // "" = sync with desktop wallpaper, or "preset:...", "image:<path>", "video:<path>"
    pub lock_pin_hash: String,     // Optional salted hash of user's AuraLock password/PIN (empty = instant unlock)
    pub lock_use_windows_pwd: bool,
    pub lock_on_startup: bool,
    pub lock_show_media: bool,
    pub lock_avatar_path: String,  // v4.5: Custom profile photo for AuraLock
    pub lock_display_name: String, // v4.5: Optional custom profile display name on AuraLock
    // v4.5: Geolocation, Modest-PC Performance Mode & Native Windows Shortcut Interception
    pub weather_city: String,      // "" = Auto IP Geolocation, or custom city name ("Bogotá", "Madrid", etc.)
    pub performance_mode: bool,    // Ultra-low CPU/GPU mode for modest PCs (disables heavy continuous GPU loops)
    pub intercept_win_hotkeys: bool, // Intercept solo Win -> Launchpad, Ctrl+Shift+Esc -> AuraTasks, Win+E/I/S
}

impl Default for ShellSettings {
    fn default() -> Self {
        let low_spec = detect_modest_hardware();
        Self {
            glass_opacity: 0.78,
            hide_windows_taskbar: true,
            start_with_windows: true,
            auto_game_boost: true,
            dock_auto_hide: true,
            show_desktop_widgets: true,
            dock_position: "Left".to_string(),
            last_mode: ShellMode::PC,
            style_windows_apps: true,
            wallpaper_id: if low_spec {
                "preset:matte_grid".to_string()
            } else {
                "preset:aura_obsidian_flow".to_string()
            },
            wallpaper_auto_pause: true,
            auto_apply_updates: true,
            update_manifest_url: "https://api.github.com/repos/dandifeto1111-prog/AuraOS/releases/latest".to_string(),
            lock_wallpaper_id: String::new(),
            lock_pin_hash: String::new(),
            lock_use_windows_pwd: false,
            lock_on_startup: false,
            lock_show_media: true,
            lock_avatar_path: String::new(),
            lock_display_name: String::new(),
            weather_city: String::new(),
            performance_mode: low_spec,
            intercept_win_hotkeys: true,
        }
    }
}

/// Auto-detects modest hardware (<= 4 logical threads or <= 8 GB RAM) so fresh installs run buttery smooth!
pub fn detect_modest_hardware() -> bool {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    if cores <= 4 {
        return true;
    }
    unsafe {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut ms = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut ms).is_ok() {
            let total_gb = ms.ullTotalPhys as f64 / 1_073_741_824.0;
            if total_gb <= 8.5 {
                return true;
            }
        }
    }
    false
}

/// Deterministic salted SHA-256-like 64-hex-char digest for local AuraLock PIN verification (zero extra crates!)
pub fn hash_lock_pin(pin: &str) -> String {
    let trimmed = pin.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x84222325cbf29ce4;
    let mut h3: u64 = 0x9e3779b97f4a7c15;
    let mut h4: u64 = 0x517cc1b727220a95;
    for (i, b) in format!("AuraLock::v4.3::{}", trimmed).as_bytes().iter().enumerate() {
        let v = *b as u64 ^ ((i as u64).wrapping_mul(0x100000001b3));
        h1 = (h1 ^ v).wrapping_mul(0x100000001b3);
        h2 = (h2 ^ v.rotate_left(13)).wrapping_mul(0x9e3779b185ebca87);
        h3 = h3.wrapping_add(v ^ h1).rotate_left(17);
        h4 = (h4 ^ h2.wrapping_add(v)).wrapping_mul(0xc2b2ae3d27d4eb4f);
    }
    format!("{:016x}{:016x}{:016x}{:016x}", h1, h2, h3, h4)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WidgetKind {
    Clock,
    Music,
    Weather,
    Monitor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetConfig {
    pub kind: WidgetKind,
    pub enabled: bool,
    pub x: i32,
    pub y: i32,
    pub size_tier: u8, // 0 = Compacto (0.85x), 1 = Normal (1.0x), 2 = Grande (1.18x)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetsStore {
    pub widgets: Vec<WidgetConfig>,
}

impl Default for WidgetsStore {
    fn default() -> Self {
        Self::default_for_screen(1920, 1080)
    }
}

impl WidgetsStore {
    pub fn default_for_screen(sw: i32, sh: i32) -> Self {
        Self {
            widgets: vec![
                WidgetConfig {
                    kind: WidgetKind::Clock,
                    enabled: true,
                    x: (sw - 420) / 2,
                    y: (sh / 2) - 210,
                    size_tier: 1,
                },
                WidgetConfig {
                    kind: WidgetKind::Music,
                    enabled: true,
                    x: (sw - 390) / 2,
                    y: (sh / 2) + 20,
                    size_tier: 1,
                },
                WidgetConfig {
                    kind: WidgetKind::Weather,
                    enabled: false,
                    x: sw - 340,
                    y: 74,
                    size_tier: 1,
                },
                WidgetConfig {
                    kind: WidgetKind::Monitor,
                    enabled: false,
                    x: sw - 340,
                    y: 238,
                    size_tier: 1,
                },
            ],
        }
    }

    pub fn get(&self, kind: WidgetKind) -> Option<&WidgetConfig> {
        self.widgets.iter().find(|w| w.kind == kind)
    }

    pub fn get_mut(&mut self, kind: WidgetKind) -> Option<&mut WidgetConfig> {
        self.widgets.iter_mut().find(|w| w.kind == kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperKind {
    External,
    Preset,
    Video,
    Web,
    Image,
}

#[derive(Debug, Clone)]
pub struct WallpaperItem {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub kind: WallpaperKind,
    pub file_path: String,
    pub preview_path: Option<String>,
    pub accent_rgb: (u8, u8, u8),
}

#[derive(Debug, Clone)]
pub struct KeyboardLayoutEntry {
    pub hkl: isize,
    pub short_code: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEntry {
    pub id: String,
    pub name: String,
    pub target: String,      // exe path, lnk path, or https:// URL
    pub subtitle: String,    // e.g. "IA Web", "App local", "Juego personalizado"
    pub category: String,    // "DevAI" or "Game"
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CustomItemsStore {
    pub dev_ai_items: Vec<CustomEntry>,
    pub custom_games: Vec<CustomEntry>,
    #[serde(default)]
    pub custom_wallpapers: Vec<CustomEntry>,
}

#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub target: String,
    pub icon_path: String,
    pub category: String, // "App", "Dev", "AI", "Game", "Desktop"
    pub is_web: bool,
    pub is_custom: bool,
    pub custom_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GameEntry {
    pub name: String,
    pub platform: String, // "Steam", "Epic", "Riot", "Local", "Manual"
    pub launch_target: String,
    pub icon_path: String,
    pub cover_path: Option<String>,
    pub is_custom: bool,
    pub custom_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RunningWindow {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
    pub process_name: String,
    pub exe_path: String,
    pub is_active: bool,
    pub is_minimized: bool,
}

#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub full_name: String,
    pub short_name: String,
    pub sub_label: String,
    pub is_default: bool,
    pub is_capture: bool,
}

#[derive(Debug, Clone)]
pub struct AudioAppSession {
    pub display_name: String,
    pub exe_path: String,
    pub volume_percent: i32,
    pub is_muted: bool,
    pub pids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub source_app: String,
    pub is_playing: bool,
    pub active: bool,
}

impl Default for NowPlaying {
    fn default() -> Self {
        Self {
            title: "Sin reproducción activa".to_string(),
            artist: String::new(),
            source_app: "Multimedia".to_string(),
            is_playing: false,
            active: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WeatherInfo {
    pub temp_c: i32,
    pub feels_like_c: i32,
    pub humidity_pct: i32,
    pub wind_kmh: i32,
    pub weather_code: i32,
    pub condition: String,
    pub city: String,
    pub loaded: bool,
}

impl Default for WeatherInfo {
    fn default() -> Self {
        Self {
            temp_c: 0,
            feels_like_c: 0,
            humidity_pct: 0,
            wind_kmh: 0,
            weather_code: 0,
            condition: "Actualizando...".to_string(),
            city: String::new(),
            loaded: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub letter: String,
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_gb: f64,
    pub used_pct: f32,
}

#[derive(Debug, Clone)]
pub struct DevPort {
    pub port: u16,
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct BackgroundApp {
    pub process_key: String,
    pub display_name: String,
    pub exe_path: String,
    pub fallback_launch: String,
    pub main_hwnd: isize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    #[serde(rename = "Version", default)]
    pub version: String,
    #[serde(rename = "BuildNumber", default)]
    pub build_number: i64,
    #[serde(rename = "BuildTime", default)]
    pub build_time: String,
    #[serde(rename = "SourceDir", default)]
    pub source_dir: String,
    #[serde(rename = "Changelog", default)]
    pub changelog: String,
}

pub fn app_data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(base).join("AuraShell");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn load_settings() -> ShellSettings {
    let p = app_data_dir().join("settings_v3.json");
    if let Ok(text) = std::fs::read_to_string(&p) {
        if let Ok(s) = serde_json::from_str::<ShellSettings>(text.trim_start_matches('\u{FEFF}')) {
            return s;
        }
    }
    ShellSettings::default()
}

pub fn save_settings(settings: &ShellSettings) {
    let p = app_data_dir().join("settings_v3.json");
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(p, json);
    }
}

pub fn load_custom_items() -> CustomItemsStore {
    let p = app_data_dir().join("custom_items.json");
    if let Ok(text) = std::fs::read_to_string(&p) {
        if let Ok(s) = serde_json::from_str::<CustomItemsStore>(&text) {
            return s;
        }
    }
    CustomItemsStore::default()
}

pub fn save_custom_items(store: &CustomItemsStore) {
    let p = app_data_dir().join("custom_items.json");
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(p, json);
    }
}

pub fn load_widgets(sw: i32, sh: i32) -> WidgetsStore {
    let p = app_data_dir().join("widgets_v3.json");
    if let Ok(text) = std::fs::read_to_string(&p) {
        if let Ok(mut s) = serde_json::from_str::<WidgetsStore>(&text) {
            // Ensure all 4 built-in widget kinds exist even when upgrading older configs
            let def = WidgetsStore::default_for_screen(sw, sh);
            for dw in def.widgets {
                if s.get(dw.kind).is_none() {
                    s.widgets.push(dw);
                }
            }
            return s;
        }
    }
    WidgetsStore::default_for_screen(sw, sh)
}

pub fn save_widgets(store: &WidgetsStore) {
    let p = app_data_dir().join("widgets_v3.json");
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(p, json);
    }
}
