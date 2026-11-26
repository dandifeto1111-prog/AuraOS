//! Shared Application State, True-Toggle Debounce Guard, Hit Regions & Actions (v3.1)

use crate::models::*;
use crate::motion::{Spring, TooltipGroup, Tween};
use crate::radios::RadioStates;
use crate::renderer::Rect;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    // Top Bar & Global Panels
    ToggleOverview,
    ClickModeTab(ShellMode),
    LaunchAuraFiles(Option<String>),
    LaunchAuraSettings(Option<String>),
    LaunchAuraLock,
    LaunchAuraTasks,
    LaunchAuraShot,
    LaunchAuraPdf,
    LaunchAuraBeats,
    LaunchAuraApp(&'static str),
    ToggleCalendar,
    CalPrevMonth,
    CalNextMonth,
    CalToday,
    PomodoroToggle,
    PomodoroMode(bool),
    ToggleClipboard,
    PasteClip(usize),
    PinClip(usize),
    DeleteClip(usize),
    ClearClips,
    CopyTextAndClose(String),
    RunShellCommand(String),
    ToggleControlCenter,
    TogglePowerMenu,
    ToggleLaunchpad,
    OpenSettings,
    CloseAllOverlays,
    FocusSearchBar,
    ClearSearchBar,

    // Active Windows App Top-Bar Traffic Lights
    ActiveWinClose,
    ActiveWinMin,
    ActiveWinMaxToggle,

    // Media & Audio
    MediaPrev,
    MediaPlayPause,
    MediaNext,
    MediaSeek(f32),
    ToggleMasterMute,
    SetMasterVolume(i32),
    SwitchAudioDeviceTab(bool), // false = output, true = mic
    SelectAudioDevice(String),
    ToggleAppMute(Vec<u32>, bool),
    SetAppVolume(Vec<u32>, i32),

    // Native Android/iOS-Style Quick Controls (In-Place!)
    ToggleWifiRadio,
    ToggleBluetoothRadio,
    ToggleHotspot,
    ToggleAirplaneMode,
    ToggleNetworkDrawer,
    ScanWifiNetworks,
    ConnectWifi(String),
    DisconnectWifi,
    ToggleVpnProfile(String, bool),
    SetBrightness(i32),
    ToggleNightLight,
    ToggleKeepAwake,
    SetCcBottomTab(u8), // 0 = To-Do, 1 = Pomodoro, 2 = Apps 2do Plano
    ToggleTodo(u64),
    DeleteTodo(u64),
    StartTodoInput,
    SubmitTodoInput,
    // v5.1 Dev Hub: AI Quotas & GitHub/Git Commits Monitor
    AdjustAiQuota(String, i32),
    ResetAiQuota(String),
    RefreshDevHub,
    StartGithubUserEdit,
    SubmitGithubUserEdit,
    CycleKeyboardLayout,
    SelectKeyboardLayout(isize),
    ToggleKeyboardMenu,
    LaunchTarget(String),
    RunTargetAsAdmin(String),
    FocusWindow(isize),
    MinimizeWindow(isize),
    CloseWindowGraceful(isize),
    ForceKillWindowPid(u32, String),
    OpenDockContextMenu {
        title: String,
        process_name: String,
        hwnd: isize,
        pid: u32,
        exe_path: String,
        target: String,
        is_running: bool,
        is_minimized: bool,
    },
    KillBackgroundApp(String),
    LaunchUpdater,
    ApplyPendingUpdate,
    RestoreWindowsAndExit,

    // Power Menu
    PowerSleep,
    PowerLock,
    PowerRestartConfirm,
    PowerShutdownConfirm,

    // Wallpapers & Desktop Widgets Studio
    SelectWallpaper(String),
    SetWavesCustomColor(String),
    AddCustomWallpaperFile,
    ToggleWidget(WidgetKind),
    ResizeWidget(WidgetKind, i8), // -1 or +1
    ArrangeWidgetsGrid,
    ResetWidgetsLayout,
    CloseExternalWallpaperEngine,

    // Overview / Launcher / Dev / Gamer Manual Additions
    SetGlassOpacity(f32),
    ToggleGameBoost,
    OpenAddDevWebModal,
    AddDevLocalFile,
    AddCustomGameFile,
    RemoveCustomDevItem(String),
    RemoveCustomGame(String),
    SubmitAddDevWebModal,
    CancelModal,
    FocusModalField(usize), // 0 = name, 1 = url
    ToggleSetting(&'static str),
}

#[derive(Debug, Clone)]
pub struct HitRegion {
    pub id: String,
    pub rect: Rect,
    pub action: Action,
    pub right_action: Option<Action>,
    pub tooltip: Option<String>,
    pub slider: Option<SliderDragKind>,
}

#[derive(Debug, Clone)]
pub enum SliderDragKind {
    MasterVolume { x: f32, w: f32 },
    Brightness { x: f32, w: f32 },
    AppVolume { x: f32, w: f32, pids: Vec<u32> },
    GlassOpacity { x: f32, w: f32 },
    MediaSeek { x: f32, w: f32 },
}

#[derive(Debug, Clone)]
pub struct DockContextMenu {
    pub title: String,
    pub process_name: String,
    pub hwnd: isize,
    pub pid: u32,
    pub exe_path: String,
    pub target: String,
    pub is_running: bool,
    pub is_minimized: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AddDevWebModal {
    pub open: bool,
    pub name: String,
    pub url: String,
    pub active_field: usize, // 0 = name, 1 = url
    pub error: String,
}

pub struct AppState {
    pub settings: ShellSettings,
    pub custom_items: CustomItemsStore,
    pub widgets: WidgetsStore,
    pub wallpapers: Vec<WallpaperItem>,
    pub wallpaper_scroll: f32,
    pub wallpaper_scroll_target: f32,
    pub mode: ShellMode,

    // Panel visibility + Apple circular reveal & Emil Kowalski animations + TRUE-TOGGLE debounce!
    pub overview_open: bool,
    pub overview_settings_view: bool,
    pub overview_last_closed: Option<Instant>,
    pub overview_anim: Tween,       // Fluid circular/morph reveal 210ms enter / 135ms exit
    pub overview_origin_x: f32,     // X anchor on Top Bar where Overview expands from!
    pub overview_tab_pill_x: Tween, // Smooth 180ms pill morph across PC / Dev / Gamer / Studio
    pub tab_content_anim: Tween,    // Directional 175ms content slide + fade on mode switch
    pub tab_slide_dir: f32,         // +1.0 (entering from right) or -1.0 (entering from left)
    pub search_query: String,
    pub search_selected: usize,

    pub cc_open: bool,
    pub cc_anim: Tween, // Apple circular reveal from top-right pill (210ms / 135ms)
    pub cc_last_closed: Option<Instant>,
    pub cc_showing_mics: bool,
    pub cc_keyboard_menu_open: bool,
    pub cc_network_drawer_open: bool,
    pub cc_wifi_networks: Vec<crate::radios::WifiNetwork>,
    pub cc_wifi_scanning: bool,
    pub cc_vpn_profiles: Vec<crate::radios::VpnProfile>,
    pub cc_net_details: crate::radios::NetworkDetails,
    pub cc_status_toast: Option<(String, bool, Instant)>,
    pub cc_bottom_tab: u8, // 0 = To-Do, 1 = Pomodoro, 2 = Apps 2º Plano
    pub cc_scroll: f32,
    pub dock_menu: Option<DockContextMenu>,
    pub todos: Vec<crate::dev_hub::TodoItem>,
    pub todo_input_active: bool,
    pub todo_input_text: String,
    pub battery_status: Option<(u8, bool)>,
    pub uptime_label: String,
    pub dev_hub_cfg: crate::dev_hub::DevHubConfig,
    pub dev_hub: crate::dev_hub::DevHubSnapshot,
    pub gh_input_active: bool,
    pub gh_input_text: String,

    pub power_open: bool,
    pub power_anim: Tween, // Apple circular reveal from top-right Power icon (180ms / 120ms)
    pub power_last_closed: Option<Instant>,
    pub power_confirm: Option<&'static str>, // "shutdown" or "restart"

    pub launchpad_open: bool,
    pub launchpad_anim: Tween, // Apple expanding circle reveal from Dock 9-dot button (270ms / 175ms)
    pub launchpad_last_closed: Option<Instant>,
    pub launchpad_query: String,
    pub launchpad_scroll: f32,
    pub launchpad_scroll_target: f32,

    // v5.0 Calendar + Pomodoro Flyout & Win+V AuraClip History & Volume OSD
    pub calendar_open: bool,
    pub calendar_anim: Tween,
    pub calendar_last_closed: Option<Instant>,
    pub cal_year: i32,
    pub cal_month: i32,
    pub pomodoro_active: bool,
    pub pomodoro_secs_left: u32,
    pub pomodoro_is_break: bool,

    pub clipboard_open: bool,
    pub clipboard_anim: Tween,
    pub clipboard_last_closed: Option<Instant>,
    pub clipboard_items: Vec<crate::ui_clipboard::ClipItem>,
    pub last_clip_seq: u32,
    pub clip_target_hwnd: isize,

    pub osd_until: Option<Instant>,

    // Animated Music Widget continuous visualizer phase & smooth play energy
    pub music_wave_phase: f32,
    pub music_energy: f32,

    // Dock state & interruptible Emil springs
    pub dock_hidden: bool,
    pub dock_reveal: Spring, // 1.0 = visible, 0.0 = tucked away
    pub dock_scroll: f32,
    pub dock_hover_idx: Option<usize>,
    pub dock_mouse_axis: Option<f32>,
    pub dock_item_scales: Vec<Spring>,
    pub dock_tooltip: TooltipGroup,

    // Modal for adding custom Web AI / Dev links
    pub add_modal: AddDevWebModal,

    // Live data caches (pre-loaded & updated in background -> 0ms tab switches!)
    pub active_window_label: String,
    pub active_user_hwnd: isize,
    pub open_windows: Vec<RunningWindow>,
    pub all_apps: Vec<AppEntry>,
    pub dev_ai_apps: Vec<AppEntry>,
    pub games: Vec<GameEntry>,
    pub dev_ports: Vec<DevPort>,
    pub tray_apps: Vec<BackgroundApp>,
    pub drives: Vec<DriveInfo>,
    pub weather: WeatherInfo,
    pub now_playing: NowPlaying,
    pub master_vol: i32,
    pub master_muted: bool,
    pub output_devices: Vec<AudioDevice>,
    pub mic_devices: Vec<AudioDevice>,
    pub app_sessions: Vec<AudioAppSession>,
    pub radios: RadioStates,
    pub net_status: String,
    pub keyboard_lang: String,
    pub active_keyboard_hkl: isize,
    pub keyboard_layouts: Vec<KeyboardLayoutEntry>,
    pub cpu_pct: f32,
    pub cpu_history: Vec<f32>,
    pub ram_used_gb: f64,
    pub ram_total_gb: f64,
    pub ram_pct: i32,
    pub game_boost_active: bool,
    pub pending_update: Option<crate::updater::UpdateInfo>,
    pub game_boost_status: String,
    pub update_available_ver: Option<String>,
    pub aurabeats_installed: bool,
    pub aurabeats_plugin_count: usize,
    pub aurabeats_icon_path: Option<String>,

    // Tactile press feedback (Emil Kowalski scale(0.97) on press)
    pub hovered_id: Option<String>,
    pub pressed_id: Option<String>,
    pub active_slider: Option<SliderDragKind>,
    pub overview_scroll: f32,
}

impl AppState {
    pub fn new(sw: i32, sh: i32) -> Self {
        let settings = load_settings();
        let custom_items = load_custom_items();
        let widgets = load_widgets(sw, sh);
        let mode = settings.last_mode;
        let initial_tab_idx = match mode {
            ShellMode::PC => 0.0,
            ShellMode::Dev => 1.0,
            ShellMode::Gamer => 2.0,
            ShellMode::Creative => 3.0,
            ShellMode::Studio => 4.0,
        };
        let aurabeats_installed = crate::win32_shell::resolve_aurabeats_path().is_some();
        let aurabeats_plugin_count = crate::win32_shell::count_aurabeats_plugins();
        let aurabeats_icon_path = crate::win32_shell::resolve_aurabeats_icon_path()
            .map(|p| p.to_string_lossy().to_string());
        Self {
            settings,
            custom_items,
            widgets,
            wallpapers: Vec::new(),
            wallpaper_scroll: 0.0,
            wallpaper_scroll_target: 0.0,
            mode,
            overview_open: false,
            overview_settings_view: false,
            overview_last_closed: None,
            overview_anim: Tween::new(0.0),
            overview_origin_x: sw as f32 * 0.5,
            overview_tab_pill_x: Tween::new(initial_tab_idx),
            tab_content_anim: Tween::new(1.0),
            tab_slide_dir: 1.0,
            search_query: String::new(),
            search_selected: 0,
            cc_open: false,
            cc_anim: Tween::new(0.0),
            cc_last_closed: None,
            cc_showing_mics: false,
            cc_keyboard_menu_open: false,
            cc_network_drawer_open: false,
            cc_wifi_networks: Vec::new(),
            cc_wifi_scanning: false,
            cc_vpn_profiles: Vec::new(),
            cc_net_details: crate::radios::NetworkDetails::default(),
            cc_status_toast: None,
            cc_bottom_tab: 0,
            cc_scroll: 0.0,
            dock_menu: None,
            todos: crate::dev_hub::load_todos(),
            todo_input_active: false,
            todo_input_text: String::new(),
            battery_status: crate::win32_shell::get_battery_status(),
            uptime_label: crate::win32_shell::get_uptime_short(),
            dev_hub_cfg: crate::dev_hub::load_dev_hub_config(),
            dev_hub: crate::dev_hub::DevHubSnapshot::default(),
            gh_input_active: false,
            gh_input_text: String::new(),
            power_open: false,
            power_anim: Tween::new(0.0),
            power_last_closed: None,
            power_confirm: None,
            launchpad_open: false,
            launchpad_anim: Tween::new(0.0),
            launchpad_last_closed: None,
            launchpad_query: String::new(),
            launchpad_scroll: 0.0,
            launchpad_scroll_target: 0.0,
            calendar_open: false,
            calendar_anim: Tween::new(0.0),
            calendar_last_closed: None,
            cal_year: 0,
            cal_month: 0,
            pomodoro_active: false,
            pomodoro_secs_left: 25 * 60,
            pomodoro_is_break: false,
            clipboard_open: false,
            clipboard_anim: Tween::new(0.0),
            clipboard_last_closed: None,
            clipboard_items: Vec::new(),
            last_clip_seq: crate::ui_clipboard::get_clipboard_seq(),
            clip_target_hwnd: 0,
            osd_until: None,
            music_wave_phase: 0.0,
            music_energy: 0.0,
            dock_hidden: false,
            dock_reveal: Spring::drawer(1.0),
            dock_scroll: 0.0,
            dock_hover_idx: None,
            dock_mouse_axis: None,
            dock_item_scales: Vec::new(),
            dock_tooltip: TooltipGroup::default(),
            add_modal: AddDevWebModal::default(),
            active_window_label: "Escritorio".to_string(),
            active_user_hwnd: 0,
            open_windows: Vec::new(),
            all_apps: Vec::new(),
            dev_ai_apps: Vec::new(),
            games: Vec::new(),
            dev_ports: Vec::new(),
            tray_apps: Vec::new(),
            drives: Vec::new(),
            weather: crate::system::load_cached_weather().unwrap_or_default(),
            now_playing: NowPlaying::default(),
            master_vol: 50,
            master_muted: false,
            output_devices: Vec::new(),
            mic_devices: Vec::new(),
            app_sessions: Vec::new(),
            radios: RadioStates::default(),
            net_status: "Ethernet".to_string(),
            keyboard_lang: "ESP".to_string(),
            active_keyboard_hkl: 0,
            keyboard_layouts: Vec::new(),
            cpu_pct: 12.0,
            cpu_history: vec![12.0; 24],
            ram_used_gb: 6.5,
            ram_total_gb: 16.0,
            ram_pct: 41,
            game_boost_active: false,
            pending_update: None,
            game_boost_status: "Listo para optimizar rendimiento".to_string(),
            update_available_ver: None,
            aurabeats_installed,
            aurabeats_plugin_count,
            aurabeats_icon_path,
            hovered_id: None,
            pressed_id: None,
            active_slider: None,
            overview_scroll: 0.0,
        }
    }

    pub fn was_just_closed(stamp: Option<Instant>) -> bool {
        stamp
            .map(|t| Instant::now().duration_since(t).as_millis() < 220)
            .unwrap_or(false)
    }
}
