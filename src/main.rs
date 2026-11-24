#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod agent_bridge;
mod audio;
mod aura_window;
mod dev_hub;
mod dock_preview;
mod genie;
mod hotkey_hook;
mod math_eval;
mod models;
mod motion;
mod radios;
mod renderer;
mod system;
mod telemetry_worker;
mod theme;
mod ui_calendar;
mod ui_clipboard;
mod ui_control_center;
mod ui_dock;
mod ui_launchpad;
mod ui_overview;
mod ui_power_menu;
mod ui_state;
mod ui_topbar;
mod ui_widgets;
mod updater;
mod wallpaper;
mod win32_shell;
mod window_styler;

use models::*;
use motion::{CubicBezier, Spring};
use radios::RadioStates;
use renderer::{GpuEngine, Rect, WindowSurface};
use telemetry_worker::{TelemetryCmd, TelemetryUpdate, TelemetryWorker};
use updater::UpdateInfo;
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;
use ui_calendar::{CAL_HEIGHT, CAL_WIDTH};
use ui_clipboard::{ClipItem, CLIP_HEIGHT, CLIP_WIDTH, OSD_HEIGHT, OSD_WIDTH};
use ui_control_center::{CC_MAX_H, CC_WIDTH};
use ui_dock::DOCK_THICKNESS;
use ui_power_menu::{POWER_MENU_H, POWER_MENU_W};
use ui_state::{Action, AppState, HitRegion, SliderDragKind};
use ui_topbar::TOPBAR_HEIGHT;
use ui_widgets::widget_window_dims;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, ReleaseCapture, SetCapture, TrackMouseEvent, UnregisterHotKey, MOD_ALT,
    MOD_CONTROL, TME_LEAVE, TRACKMOUSEEVENT, VK_BACK, VK_DOWN, VK_ESCAPE, VK_RETURN, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, KillTimer, LoadCursorW,
    PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SetTimer, SetWindowPos, ShowWindow,
    TranslateMessage, HWND_BOTTOM, HWND_TOPMOST, IDC_ARROW, MA_NOACTIVATE, MSG, SWP_NOACTIVATE,
    SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_CLOSE,
    WM_DESTROY, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const TIMER_ANIM: usize = 1;
const TIMER_FAST: usize = 2;
const TIMER_SLOW: usize = 3;

const HK_OVERVIEW: i32 = 1001;
const HK_FILES: i32 = 1002;
const HK_RESTORE: i32 = 1003;
const HK_LOCK: i32 = 1004;
const HK_TASKS: i32 = 1005;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinKind {
    TopBar,
    Dock,
    DockNotch,
    ControlCenter,
    PowerMenu,
    Calendar,
    Clipboard,
    VolumeOsd,
    Launchpad,
    Overview,
    Widget(WidgetKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilePickKind {
    DevLocal,
    CustomGame,
    CustomWallpaper,
}

struct DesktopWidgetWin {
    kind: WidgetKind,
    hwnd: HWND,
    surf: WindowSurface,
    hits: Vec<HitRegion>,
    hovered: bool,
}

struct StartupCatalog {
    all_apps: Vec<AppEntry>,
    dev_ai_apps: Vec<AppEntry>,
    games: Vec<GameEntry>,
    wallpapers: Vec<WallpaperItem>,
    drives: Vec<DriveInfo>,
}

struct ShellRuntime {
    gpu: GpuEngine,
    state: AppState,
    cpu: system::CpuTracker,
    boost: system::GameBoost,
    wallpaper: wallpaper::WallpaperEngine,
    dock_preview: dock_preview::DockPreviewPopup,
    genie: genie::GenieAnimator,
    startup_rx: Option<Receiver<StartupCatalog>>,
    weather_rx: Option<Receiver<WeatherInfo>>,
    radio_rx: Option<Receiver<RadioStates>>,
    dev_hub_rx: Option<Receiver<dev_hub::DevHubSnapshot>>,
    file_pick_rx: Option<Receiver<(FilePickKind, Option<String>)>>,

    hwnd_topbar: HWND,
    hwnd_dock: HWND,
    hwnd_notch: HWND,
    hwnd_cc: HWND,
    hwnd_power: HWND,
    hwnd_calendar: HWND,
    hwnd_clipboard: HWND,
    hwnd_osd: HWND,
    hwnd_launchpad: HWND,
    hwnd_overview: HWND,

    surf_topbar: WindowSurface,
    surf_dock: WindowSurface,
    surf_notch: WindowSurface,
    surf_cc: WindowSurface,
    surf_power: WindowSurface,
    surf_calendar: WindowSurface,
    surf_clipboard: WindowSurface,
    surf_osd: WindowSurface,
    surf_launchpad: WindowSurface,
    surf_overview: WindowSurface,

    widget_wins: Vec<DesktopWidgetWin>,
    dragging_widget: Option<(WidgetKind, i32, i32)>, // (kind, grab_offset_x, grab_offset_y)
    snapped_widget: Option<WidgetKind>,              // Currently magnetically docked widget during drag!

    hits_topbar: Vec<HitRegion>,
    hits_dock: Vec<HitRegion>,
    hits_cc: Vec<HitRegion>,
    hits_power: Vec<HitRegion>,
    hits_calendar: Vec<HitRegion>,
    hits_clipboard: Vec<HitRegion>,
    hits_launchpad: Vec<HitRegion>,
    hits_overview: Vec<HitRegion>,

    cc_content_h: f32,
    lp_content_h: f32,
    screen_w: i32,
    screen_h: i32,
    hidden_for_fullscreen: bool,
    desktop_covered: bool,
    last_frame: Instant,
    wm_activate_existing: u32,
    wm_settings_changed: u32,
    wm_aura_shortcut: u32,
    wm_dock_preview_click: u32,
    wm_genie_minimize: u32,
    last_dock_hover_exit: Instant,
    mouse_in_dock: bool,
    mouse_in_notch: bool,
    dock_hwnd_visible: bool,
    notch_visible: bool,
    dock_geo: (i32, i32, i32, i32),
    slow_tick_count: u32,
    fast_tick_count: u32,
    anim_interval_ms: u32,
    music_anim_accum: f32,
    last_clock_min: u32,
    last_fg_hwnd_seen: isize,
    last_topbar_sig: String,
    telemetry: TelemetryWorker,
    update_rx: Receiver<UpdateInfo>,
    auto_update_triggered: bool,
}

thread_local! {
    static RUNTIME: RefCell<Option<ShellRuntime>> = RefCell::new(None);
}

fn with_rt<R, F: FnOnce(&mut ShellRuntime) -> R>(f: F) -> Option<R> {
    RUNTIME.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return None;
        };
        guard.as_mut().map(f)
    })
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    std::panic::set_hook(Box::new(|info| {
        let _ = std::fs::write(
            app_data_dir().join("AuraShell_v3.log"),
            format!("Panic: {}\n", info),
        );
        win32_shell::restore_windows_safely();
    }));

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        run_visual_capture(&PathBuf::from(&args[2]));
        return;
    }

    if !win32_shell::acquire_single_instance_mutex("AuraShell_SingleInstance_Mutex_v3") {
        return;
    }

    if let Err(e) = run_shell() {
        win32_shell::restore_windows_safely();
        let _ = std::fs::write(app_data_dir().join("AuraShell_v3.log"), format!("Error: {:?}", e));
    }
    win32_shell::restore_windows_safely();
}

fn run_shell() -> windows::core::Result<()> {
    let mut gpu = GpuEngine::new()?;
    let (sw, sh) = win32_shell::screen_size();
    let mut state = AppState::new(sw, sh);

    // INSTANT TAKEOVER (<120ms first paint!): hide classic Windows taskbar immediately at logon
    // BEFORE running any slow filesystem scans for apps, games, or wallpapers!
    if state.settings.hide_windows_taskbar {
        win32_shell::set_taskbar_visibility(false);
    }

    // Enable non-blocking background STA icon & thumbnail decoder immediately
    gpu.enable_async_loader();

    // Move heavy catalog scans (135+ apps, Steam/Epic/Riot games, Workshop wallpapers, drives)
    // completely OFF the startup critical path so AuraTopBar + AuraDock + Wallpaper paint in <120ms!
    let custom_snapshot = state.custom_items.clone();
    let (cat_tx, cat_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let all_apps = system::scan_all_apps(&custom_snapshot);
        let dev_ai_apps = system::build_dev_ai_entries(&all_apps, &custom_snapshot);
        let games = system::scan_all_games(&custom_snapshot);
        let wallpapers = wallpaper::scan_wallpapers(&custom_snapshot);
        let drives = system::get_local_drives();
        let _ = cat_tx.send(StartupCatalog {
            all_apps,
            dev_ai_apps,
            games,
            wallpapers,
            drives,
        });
    });

    // Only ultra-fast (<2ms) local Win32 queries before first paint — ZERO blocking COM/WinRT calls!
    state.open_windows = win32_shell::get_open_user_windows();
    state.active_window_label = win32_shell::get_active_window_label();
    state.active_user_hwnd = win32_shell::get_active_user_hwnd();
    state.keyboard_lang = win32_shell::get_keyboard_layout_short();
    state.active_keyboard_hkl = win32_shell::get_active_keyboard_hkl();
    state.keyboard_layouts = win32_shell::get_installed_keyboard_layouts();
    let (ru, rt_gb, rp) = system::get_ram_stats();
    state.ram_used_gb = ru;
    state.ram_total_gb = rt_gb;
    state.ram_pct = rp;
    state.update_available_ver = check_pending_update();

    // Async Open-Meteo weather + WinRT Radio states on background threads!
    let (w_tx, w_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = w_tx.send(system::fetch_weather_blocking());
    });
    let (r_tx, r_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = r_tx.send(radios::query_radio_states());
    });

    // Apply v5.1 Material You 3 palette, display brightness/night-light & keep-awake immediately!
    let _ = theme::load_active_palette();
    if state.settings.brightness_pct < 100 || state.settings.night_light {
        win32_shell::apply_display_gamma(state.settings.brightness_pct, state.settings.night_light);
    }
    if state.settings.keep_awake {
        win32_shell::set_keep_awake(true);
    }
    let dh_cfg = state.dev_hub_cfg.clone();
    let (dh_tx, dh_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = dh_tx.send(dev_hub::fetch_dev_hub_snapshot(&dh_cfg));
    });

    // Initialize integrated lightweight Wallpaper Engine (behind desktop icons!)
    let mut wp_engine = wallpaper::WallpaperEngine::new(&gpu, sw, sh);
    wp_engine.set_performance_mode(state.settings.performance_mode);
    let init_wp = state.settings.wallpaper_id.clone();
    wp_engine.set_wallpaper(&mut gpu, &init_wp);

    // Register shared Win32 window class
    let hinst = unsafe { GetModuleHandleW(None)? };
    let cls_name = win32_shell::to_wide("AuraOS_Rust_v3_Wnd");
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(cls_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }

    let is_left = !state.settings.dock_position.eq_ignore_ascii_case("Bottom");
    let top_h = TOPBAR_HEIGHT as i32;
    let body_h = (sh - top_h).max(400);

    let hwnd_topbar = create_layered_window(&cls_name, "AuraTopBar", 0, 0, sw, top_h, true, true)?;
    aura_window::apply_app_icon(hwnd_topbar);
    let (dx, dy, dw, dh) = ui_dock::dock_tight_geometry(&state, sw, sh, is_left);
    let hwnd_dock = create_layered_window(&cls_name, "AuraDock", dx, dy, dw, dh, true, true)?;
    let (nx, ny, nw, nh) = ui_dock::notch_geometry(sw, sh, is_left);
    let hwnd_notch = create_layered_window(&cls_name, "AuraDockNotch", nx, ny, nw, nh, true, true)?;
    let cc_h = (sh - top_h - 18).min(CC_MAX_H as i32);
    let hwnd_cc = create_layered_window(&cls_name, "AuraControlCenter", sw - CC_WIDTH as i32 - 10, top_h + 6, CC_WIDTH as i32, cc_h, true, false)?;
    let hwnd_power = create_layered_window(&cls_name, "AuraPowerMenu", sw - POWER_MENU_W as i32 - 10, top_h + 6, POWER_MENU_W as i32, POWER_MENU_H as i32, true, false)?;
    let cal_x = (sw - CAL_WIDTH as i32) / 2;
    let hwnd_calendar = create_layered_window(&cls_name, "AuraCalendar", cal_x, top_h + 6, CAL_WIDTH as i32, CAL_HEIGHT as i32, true, false)?;
    let clip_x = (sw - CLIP_WIDTH as i32) / 2;
    let clip_y = ((sh - CLIP_HEIGHT as i32) / 2).max(top_h + 16);
    let hwnd_clipboard = create_layered_window(&cls_name, "AuraClipboard", clip_x, clip_y, CLIP_WIDTH as i32, CLIP_HEIGHT as i32, true, false)?;
    let osd_x = (sw - OSD_WIDTH as i32) / 2;
    let osd_y = sh - 138;
    let hwnd_osd = create_layered_window(&cls_name, "AuraOSD", osd_x, osd_y, OSD_WIDTH as i32, OSD_HEIGHT as i32, true, true)?;
    let hwnd_launchpad = create_layered_window(&cls_name, "AuraLaunchpad", 0, top_h, sw, body_h, true, false)?;
    let hwnd_overview = create_layered_window(&cls_name, "AuraOverview", 0, top_h, sw, body_h, true, false)?;

    // Create 4 modular interactive draggable Desktop Widget windows (HWND_BOTTOM)!
    let widget_specs = [
        (WidgetKind::Clock, "AuraWidgetClock"),
        (WidgetKind::Music, "AuraWidgetMusic"),
        (WidgetKind::Weather, "AuraWidgetWeather"),
        (WidgetKind::Monitor, "AuraWidgetMonitor"),
    ];
    let mut widget_wins = Vec::new();
    for (wk, title) in widget_specs {
        let cfg = state.widgets.get(wk).cloned().unwrap_or(WidgetConfig {
            kind: wk,
            enabled: false,
            x: 100,
            y: 100,
            size_tier: 1,
        });
        let (ww, wh) = widget_window_dims(wk, cfg.size_tier);
        let hw = create_layered_window(&cls_name, title, cfg.x, cfg.y, ww, wh, false, true)?;
        let (init_w, init_h) = if state.settings.show_desktop_widgets && cfg.enabled {
            (ww as u32, wh as u32)
        } else {
            (2, 2)
        };
        let surf = WindowSurface::new(&gpu, hw, init_w, init_h)?;
        widget_wins.push(DesktopWidgetWin {
            kind: wk,
            hwnd: hw,
            surf,
            hits: Vec::new(),
            hovered: false,
        });
    }

    // Configure real Windows AppBar + WorkArea reclaiming full bottom 48px!
    if state.settings.hide_windows_taskbar {
        win32_shell::set_taskbar_visibility(false);
    }
    let _rc = win32_shell::register_top_appbar(hwnd_topbar, sw, top_h);
    win32_shell::apply_full_work_area(top_h, state.settings.hide_windows_taskbar);
    if state.settings.hide_windows_taskbar {
        win32_shell::refresh_maximized_windows();
    }

    // Global hotkeys (Alt+Space, Alt+E, Ctrl+Alt+W, Ctrl+Alt+L for AuraLock, Ctrl+Alt+T for AuraTasks!)
    unsafe {
        let _ = RegisterHotKey(hwnd_topbar, HK_OVERVIEW, MOD_ALT, VK_SPACE.0 as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_FILES, MOD_ALT, 'E' as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_RESTORE, MOD_CONTROL | MOD_ALT, 'W' as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_LOCK, MOD_CONTROL | MOD_ALT, 'L' as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_TASKS, MOD_CONTROL | MOD_ALT, 'T' as u32);
    }

    // v5.1 LAZY SWAPCHAIN TRIMMING: Hidden overlays start at 2x2 (~0 MB GPU RAM!) and expand on demand!
    let surf_topbar = WindowSurface::new(&gpu, hwnd_topbar, sw as u32, top_h as u32)?;
    let surf_dock = WindowSurface::new(&gpu, hwnd_dock, dw as u32, dh as u32)?;
    let surf_notch = WindowSurface::new(&gpu, hwnd_notch, nw as u32, nh as u32)?;
    let surf_cc = WindowSurface::new(&gpu, hwnd_cc, 2, 2)?;
    let surf_power = WindowSurface::new(&gpu, hwnd_power, 2, 2)?;
    let surf_calendar = WindowSurface::new(&gpu, hwnd_calendar, 2, 2)?;
    let surf_clipboard = WindowSurface::new(&gpu, hwnd_clipboard, 2, 2)?;
    let surf_osd = WindowSurface::new(&gpu, hwnd_osd, 2, 2)?;
    let surf_launchpad = WindowSurface::new(&gpu, hwnd_launchpad, 2, 2)?;
    let surf_overview = WindowSurface::new(&gpu, hwnd_overview, 2, 2)?;

    let wm_act = unsafe {
        let m = win32_shell::to_wide("WM_AURASHELL_V3_ACTIVATE");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };
    let wm_cfg = unsafe {
        let m = win32_shell::to_wide("WM_AURAOS_SETTINGS_CHANGED");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };
    let wm_shortcut = unsafe {
        let m = win32_shell::to_wide("WM_AURAOS_SHORTCUT_V45");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };
    let wm_preview_click = unsafe {
        let m = win32_shell::to_wide("WM_AURAOS_DOCK_PREVIEW_CLICK_V45");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };
    let wm_genie_min = unsafe {
        let m = win32_shell::to_wide("WM_AURAOS_GENIE_MINIMIZE");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };

    // Install lightweight WH_KEYBOARD_LL shortcut bridge (solo Win -> Launchpad, Ctrl+Shift+Esc -> AuraTasks, Win+E/I/S!)
    hotkey_hook::install(hwnd_topbar, wm_shortcut, state.settings.intercept_win_hotkeys);

    let dock_preview = dock_preview::DockPreviewPopup::new(hwnd_topbar, wm_preview_click);
    let genie = genie::GenieAnimator::new(&gpu, sw, sh);

    let upd_settings = state.settings.clone();
    let mut rt = ShellRuntime {
        gpu,
        state,
        cpu: system::CpuTracker::new(),
        boost: system::GameBoost::new(),
        wallpaper: wp_engine,
        dock_preview,
        genie,
        startup_rx: Some(cat_rx),
        weather_rx: Some(w_rx),
        radio_rx: Some(r_rx),
        dev_hub_rx: Some(dh_rx),
        file_pick_rx: None,
        hwnd_topbar,
        hwnd_dock,
        hwnd_notch,
        hwnd_cc,
        hwnd_power,
        hwnd_calendar,
        hwnd_clipboard,
        hwnd_osd,
        hwnd_launchpad,
        hwnd_overview,
        surf_topbar,
        surf_dock,
        surf_notch,
        surf_cc,
        surf_power,
        surf_calendar,
        surf_clipboard,
        surf_osd,
        surf_launchpad,
        surf_overview,
        widget_wins,
        dragging_widget: None,
        snapped_widget: None,
        hits_topbar: Vec::new(),
        hits_dock: Vec::new(),
        hits_cc: Vec::new(),
        hits_power: Vec::new(),
        hits_calendar: Vec::new(),
        hits_clipboard: Vec::new(),
        hits_launchpad: Vec::new(),
        hits_overview: Vec::new(),
        cc_content_h: 600.0,
        lp_content_h: 600.0,
        screen_w: sw,
        screen_h: sh,
        hidden_for_fullscreen: false,
        desktop_covered: false,
        last_frame: Instant::now(),
        wm_activate_existing: wm_act,
        wm_settings_changed: wm_cfg,
        wm_aura_shortcut: wm_shortcut,
        wm_dock_preview_click: wm_preview_click,
        wm_genie_minimize: wm_genie_min,
        last_dock_hover_exit: Instant::now(),
        mouse_in_dock: false,
        mouse_in_notch: false,
        dock_hwnd_visible: true,
        notch_visible: false,
        dock_geo: (dx, dy, dw, dh),
        slow_tick_count: 0,
        fast_tick_count: 0,
        anim_interval_ms: 16,
        music_anim_accum: 0.0,
        last_clock_min: u32::MAX,
        last_fg_hwnd_seen: 0,
        last_topbar_sig: String::new(),
        telemetry: TelemetryWorker::spawn(),
        update_rx: updater::spawn_update_watcher(upd_settings),
        auto_update_triggered: false,
    };

    // Initial render & show TopBar, tight Dock, pre-rendered Notch (hidden until auto-hide), and Widgets!
    rt.redraw_topbar();
    rt.redraw_dock();
    rt.redraw_notch();
    rt.sync_widget_windows_visibility();
    rt.redraw_all_widgets();
    unsafe {
        let _ = ShowWindow(hwnd_topbar, SW_SHOWNOACTIVATE);
        let _ = ShowWindow(hwnd_dock, SW_SHOWNOACTIVATE);
        SetTimer(hwnd_topbar, TIMER_ANIM, 16, None);
        SetTimer(hwnd_topbar, TIMER_FAST, 120, None);
        SetTimer(hwnd_topbar, TIMER_SLOW, 1000, None);
    }

    let launch_lock_now = rt.state.settings.lock_on_startup;
    RUNTIME.with(|cell| *cell.borrow_mut() = Some(rt));
    if launch_lock_now {
        win32_shell::launch_aura_lock();
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = KillTimer(hwnd_topbar, TIMER_ANIM);
        let _ = KillTimer(hwnd_topbar, TIMER_FAST);
        let _ = KillTimer(hwnd_topbar, TIMER_SLOW);
        let _ = UnregisterHotKey(hwnd_topbar, HK_OVERVIEW);
        let _ = UnregisterHotKey(hwnd_topbar, HK_FILES);
        let _ = UnregisterHotKey(hwnd_topbar, HK_RESTORE);
        let _ = UnregisterHotKey(hwnd_topbar, HK_LOCK);
        let _ = UnregisterHotKey(hwnd_topbar, HK_TASKS);
    }

    hotkey_hook::uninstall();
    with_rt(|rt| {
        rt.dock_preview.shutdown();
        rt.genie.shutdown();
        rt.wallpaper.shutdown();
    });
    RUNTIME.with(|cell| {
        if let Ok(mut g) = cell.try_borrow_mut() {
            *g = None;
        }
    });
    Ok(())
}

fn create_layered_window(
    cls: &[u16],
    title: &str,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    topmost: bool,
    no_activate: bool,
) -> windows::core::Result<HWND> {
    let wtitle = win32_shell::to_wide(title);
    let mut ex = WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW;
    if topmost {
        ex |= WS_EX_TOPMOST;
    }
    if no_activate {
        ex |= WS_EX_NOACTIVATE;
    }
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        CreateWindowExW(
            ex,
            PCWSTR(cls.as_ptr()),
            PCWSTR(wtitle.as_ptr()),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            hinst,
            None,
        )
    }
}

fn check_pending_update() -> Option<String> {
    let local_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()))?;
    let local_m: VersionManifest = std::fs::read_to_string(local_dir.join("version.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    let src_dir = if !local_m.source_dir.is_empty() {
        PathBuf::from(&local_m.source_dir)
    } else {
        PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(r".gemini\antigravity\scratch\AuraOS-Rust")
    };
    let avail_m: VersionManifest = std::fs::read_to_string(src_dir.join("version.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    if avail_m.build_number > local_m.build_number {
        Some(avail_m.version)
    } else {
        None
    }
}

impl ShellRuntime {
    fn kind_of(&self, hwnd: HWND) -> Option<WinKind> {
        if hwnd == self.hwnd_topbar { return Some(WinKind::TopBar); }
        if hwnd == self.hwnd_dock { return Some(WinKind::Dock); }
        if hwnd == self.hwnd_notch { return Some(WinKind::DockNotch); }
        if hwnd == self.hwnd_cc { return Some(WinKind::ControlCenter); }
        if hwnd == self.hwnd_power { return Some(WinKind::PowerMenu); }
        if hwnd == self.hwnd_calendar { return Some(WinKind::Calendar); }
        if hwnd == self.hwnd_clipboard { return Some(WinKind::Clipboard); }
        if hwnd == self.hwnd_osd { return Some(WinKind::VolumeOsd); }
        if hwnd == self.hwnd_launchpad { return Some(WinKind::Launchpad); }
        if hwnd == self.hwnd_overview { return Some(WinKind::Overview); }
        for w in &self.widget_wins {
            if w.hwnd == hwnd {
                return Some(WinKind::Widget(w.kind));
            }
        }
        None
    }

    fn hits_for(&self, kind: WinKind) -> &[HitRegion] {
        match kind {
            WinKind::TopBar => &self.hits_topbar,
            WinKind::Dock => &self.hits_dock,
            WinKind::DockNotch | WinKind::VolumeOsd => &[],
            WinKind::ControlCenter => &self.hits_cc,
            WinKind::PowerMenu => &self.hits_power,
            WinKind::Calendar => &self.hits_calendar,
            WinKind::Clipboard => &self.hits_clipboard,
            WinKind::Launchpad => &self.hits_launchpad,
            WinKind::Overview => &self.hits_overview,
            WinKind::Widget(wk) => self
                .widget_wins
                .iter()
                .find(|w| w.kind == wk)
                .map(|w| w.hits.as_slice())
                .unwrap_or(&[]),
        }
    }

    fn redraw(&mut self, kind: WinKind) {
        match kind {
            WinKind::TopBar => self.redraw_topbar(),
            WinKind::Dock => self.redraw_dock(),
            WinKind::DockNotch => self.redraw_notch(),
            WinKind::ControlCenter => self.redraw_cc(),
            WinKind::PowerMenu => self.redraw_power(),
            WinKind::Calendar => self.redraw_calendar(),
            WinKind::Clipboard => self.redraw_clipboard(),
            WinKind::VolumeOsd => self.redraw_osd(),
            WinKind::Launchpad => self.redraw_launchpad(),
            WinKind::Overview => self.redraw_overview(),
            WinKind::Widget(wk) => self.redraw_widget(wk),
        }
    }

    fn redraw_topbar(&mut self) {
        let w = self.surf_topbar.width as f32;
        let state = &self.state;
        let hits = &mut self.hits_topbar;
        self.surf_topbar.draw(&mut self.gpu, |ctx| ui_topbar::render_topbar(ctx, state, w, hits));
        let nodes: Vec<(String, String, f32, f32, f32, f32)> = self
            .hits_topbar
            .iter()
            .map(|h| {
                (
                    h.id.clone(),
                    h.tooltip.clone().unwrap_or_else(|| h.id.clone()),
                    h.rect.x,
                    h.rect.y,
                    h.rect.w,
                    h.rect.h,
                )
            })
            .collect();
        agent_bridge::publish_ui_tree("AuraTopBar", self.hwnd_topbar, &nodes);
    }

    fn redraw_dock(&mut self) {
        let w = self.surf_dock.width as f32;
        let h = self.surf_dock.height as f32;
        let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
        let state = &self.state;
        let hits = &mut self.hits_dock;
        self.surf_dock.draw(&mut self.gpu, |ctx| ui_dock::render_dock(ctx, state, w, h, is_left, hits));
        let nodes: Vec<(String, String, f32, f32, f32, f32)> = self
            .hits_dock
            .iter()
            .map(|h| {
                (
                    h.id.clone(),
                    h.tooltip.clone().unwrap_or_else(|| h.id.clone()),
                    h.rect.x,
                    h.rect.y,
                    h.rect.w,
                    h.rect.h,
                )
            })
            .collect();
        agent_bridge::publish_ui_tree("AuraDock", self.hwnd_dock, &nodes);
    }

    fn redraw_notch(&mut self) {
        let w = self.surf_notch.width as f32;
        let h = self.surf_notch.height as f32;
        let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
        let hov = self.mouse_in_notch;
        self.surf_notch.draw(&mut self.gpu, |ctx| ui_dock::render_dock_notch(ctx, w, h, is_left, hov));
    }

    /// Shrinks/fits `hwnd_dock` to the EXACT tight capsule bounding box so ZERO transparent pixels block clicks!
    fn sync_dock_tight_bounds(&mut self) {
        let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
        let geo = ui_dock::dock_tight_geometry(&self.state, self.screen_w, self.screen_h, is_left);
        if geo != self.dock_geo {
            self.dock_geo = geo;
            let (dx, dy, dw, dh) = geo;
            unsafe {
                let _ = SetWindowPos(self.hwnd_dock, HWND_TOPMOST, dx, dy, dw, dh, SWP_NOACTIVATE);
            }
            self.surf_dock.resize(&self.gpu, dw as u32, dh as u32);
        }
    }

    fn show_dock_hide_notch(&mut self) {
        if self.notch_visible {
            unsafe { let _ = ShowWindow(self.hwnd_notch, SW_HIDE); }
            self.notch_visible = false;
            self.mouse_in_notch = false;
        }
        if !self.dock_hwnd_visible && !self.hidden_for_fullscreen {
            self.sync_dock_tight_bounds();
            self.redraw_dock();
            unsafe { let _ = ShowWindow(self.hwnd_dock, SW_SHOWNOACTIVATE); }
            win32_shell::ensure_topmost(self.hwnd_dock);
            self.dock_hwnd_visible = true;
        }
    }

    fn hide_dock_show_notch(&mut self) {
        self.dock_preview.hide();
        self.state.dock_mouse_axis = None;
        if self.dock_hwnd_visible {
            unsafe { let _ = ShowWindow(self.hwnd_dock, SW_HIDE); }
            self.dock_hwnd_visible = false;
            self.mouse_in_dock = false;
        }
        if !self.notch_visible && !self.hidden_for_fullscreen {
            self.redraw_notch();
            unsafe { let _ = ShowWindow(self.hwnd_notch, SW_SHOWNOACTIVATE); }
            win32_shell::ensure_topmost(self.hwnd_notch);
            self.notch_visible = true;
        }
    }

    fn redraw_cc(&mut self) {
        let h = self.surf_cc.height as f32;
        let state = &self.state;
        let hits = &mut self.hits_cc;
        let mut total_h = 600.0;
        self.surf_cc.draw(&mut self.gpu, |ctx| {
            total_h = ui_control_center::render_control_center(ctx, state, h, hits);
        });
        self.cc_content_h = total_h;
    }

    fn redraw_power(&mut self) {
        let state = &self.state;
        let hits = &mut self.hits_power;
        self.surf_power.draw(&mut self.gpu, |ctx| ui_power_menu::render_power_menu(ctx, state, hits));
    }

    fn redraw_calendar(&mut self) {
        let state = &self.state;
        let hits = &mut self.hits_calendar;
        self.surf_calendar.draw(&mut self.gpu, |ctx| ui_calendar::render_calendar(ctx, state, hits));
    }

    fn redraw_clipboard(&mut self) {
        let state = &self.state;
        let hits = &mut self.hits_clipboard;
        self.surf_clipboard.draw(&mut self.gpu, |ctx| ui_clipboard::render_clipboard(ctx, state, hits));
    }

    fn redraw_osd(&mut self) {
        let state = &self.state;
        self.surf_osd.draw(&mut self.gpu, |ctx| ui_clipboard::render_volume_osd(ctx, state));
    }

    fn trigger_volume_osd(&mut self) {
        self.surf_osd.ensure_size(&self.gpu, OSD_WIDTH as u32, OSD_HEIGHT as u32);
        self.state.osd_until = Some(Instant::now() + std::time::Duration::from_millis(1450));
        self.redraw_osd();
        unsafe {
            let _ = ShowWindow(self.hwnd_osd, SW_SHOWNOACTIVATE);
        }
        win32_shell::ensure_topmost(self.hwnd_osd);
    }

    fn sync_material_you_palette(&mut self) {
        let wp_id = self.state.settings.wallpaper_id.clone();
        let mut seed = (124u8, 92u8, 255u8);
        if let Some(item) = self.state.wallpapers.iter().find(|w| w.id == wp_id) {
            seed = item.accent_rgb;
            if let Some(prev) = &item.preview_path {
                if let Some(rgb) = theme::extract_seed_rgb_from_image(prev) {
                    seed = rgb;
                }
            } else if item.kind == WallpaperKind::Image {
                if let Some(rgb) = theme::extract_seed_rgb_from_image(&item.file_path) {
                    seed = rgb;
                }
            }
        } else if let Some(img_path) = wp_id.strip_prefix("image:") {
            if let Some(rgb) = theme::extract_seed_rgb_from_image(img_path) {
                seed = rgb;
            }
        } else if let Some(vid_path) = wp_id.strip_prefix("video:") {
            if let Some(thumb) = wallpaper::extract_video_thumbnail_isolated(vid_path) {
                if let Some(rgb) = theme::extract_seed_rgb_from_image(&thumb) {
                    seed = rgb;
                }
            }
        }
        theme::apply_theme_scheme(&self.state.settings.theme_scheme, seed);
        window_styler::invalidate_styled_cache();
        if self.state.settings.style_windows_apps {
            window_styler::apply_aura_style_to_windows(&self.state.open_windows);
        }
    }

    fn spawn_dev_hub_refresh(&mut self) {
        let cfg = self.state.dev_hub_cfg.clone();
        let (tx, rx) = mpsc::channel();
        self.dev_hub_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(dev_hub::fetch_dev_hub_snapshot(&cfg));
        });
    }

    fn redraw_launchpad(&mut self) {
        let w = self.surf_launchpad.width as f32;
        let h = self.surf_launchpad.height as f32;
        let state = &self.state;
        let hits = &mut self.hits_launchpad;
        let mut total_h = 600.0;
        self.surf_launchpad.draw(&mut self.gpu, |ctx| {
            total_h = ui_launchpad::render_launchpad(ctx, state, w, h, hits);
        });
        self.lp_content_h = total_h;
    }

    fn redraw_overview(&mut self) {
        let w = self.surf_overview.width as f32;
        let h = self.surf_overview.height as f32;
        let state = &self.state;
        let hits = &mut self.hits_overview;
        self.surf_overview.draw(&mut self.gpu, |ctx| ui_overview::render_overview(ctx, state, w, h, hits));
    }

    fn redraw_widget(&mut self, kind: WidgetKind) {
        let Some(cfg) = self.state.widgets.get(kind) else { return };
        if !self.state.settings.show_desktop_widgets || !cfg.enabled {
            return;
        }
        let tier = cfg.size_tier;
        let mag = self.snapped_widget == Some(kind);
        let state = &self.state;
        if let Some(w) = self.widget_wins.iter_mut().find(|w| w.kind == kind) {
            let hov = w.hovered;
            let hits = &mut w.hits;
            w.surf.draw(&mut self.gpu, |ctx| {
                ui_widgets::render_desktop_widget(ctx, state, kind, tier, hov, mag, hits);
            });
        }
    }

    /// Magnetic Widget Grid Engine:
    /// - Snaps dragged widget cleanly above/below (stacking) or left/right (row) of nearby widgets with uniform 12px gap
    /// - Auto-aligns edges & centers when docked so widgets form a crisp automatic grid
    /// - Releases smoothly when dragged >26px away for completely free positioning!
    fn compute_magnetic_widget_pos(&self, kind: WidgetKind, raw_x: i32, raw_y: i32) -> (i32, i32, bool) {
        const SNAP_DIST: i32 = 26;
        const ALIGN_DIST: i32 = 28;
        const GRID_GAP: i32 = 12;
        const EDGE_PAD: i32 = 18;

        let tier = self.state.widgets.get(kind).map(|w| w.size_tier).unwrap_or(1);
        let (aw, ah) = widget_window_dims(kind, tier);
        let min_x = EDGE_PAD;
        let max_x = (self.screen_w - aw - EDGE_PAD).max(min_x);
        let min_y = TOPBAR_HEIGHT as i32 + 12;
        let max_y = (self.screen_h - ah - EDGE_PAD).max(min_y);

        let mut nx = raw_x.clamp(min_x, max_x);
        let mut ny = raw_y.clamp(min_y, max_y);
        let mut snapped_x = false;
        let mut snapped_y = false;
        let mut docked_to_widget = false;

        for other in &self.state.widgets.widgets {
            if other.kind == kind || !other.enabled {
                continue;
            }
            let (bw, bh) = widget_window_dims(other.kind, other.size_tier);
            let bx = other.x;
            let by = other.y;

            let horiz_near = nx < bx + bw + 48 && nx + aw > bx - 48;
            let vert_near = ny < by + bh + 48 && ny + ah > by - 48;

            // 1. Vertical stack docking (uno encima o debajo de otro!)
            if horiz_near && !snapped_y {
                let below_y = by + bh + GRID_GAP;
                let above_y = by - GRID_GAP - ah;
                if (ny - below_y).abs() <= SNAP_DIST {
                    ny = below_y;
                    snapped_y = true;
                    docked_to_widget = true;
                } else if (ny - above_y).abs() <= SNAP_DIST {
                    ny = above_y;
                    snapped_y = true;
                    docked_to_widget = true;
                }
            }

            // 2. Horizontal row docking (uno al lado de otro!)
            if vert_near && !snapped_x {
                let right_x = bx + bw + GRID_GAP;
                let left_x = bx - GRID_GAP - aw;
                if (nx - right_x).abs() <= SNAP_DIST {
                    nx = right_x;
                    snapped_x = true;
                    docked_to_widget = true;
                } else if (nx - left_x).abs() <= SNAP_DIST {
                    nx = left_x;
                    snapped_x = true;
                    docked_to_widget = true;
                }
            }

            // 3. Column & row edge/center alignment when near another widget!
            if horiz_near && !snapped_x {
                let left_align = bx;
                let right_align = bx + bw - aw;
                let center_align = bx + (bw - aw) / 2;
                if (nx - left_align).abs() <= ALIGN_DIST {
                    nx = left_align;
                    snapped_x = true;
                    docked_to_widget = true;
                } else if (nx - right_align).abs() <= ALIGN_DIST {
                    nx = right_align;
                    snapped_x = true;
                    docked_to_widget = true;
                } else if (nx - center_align).abs() <= ALIGN_DIST {
                    nx = center_align;
                    snapped_x = true;
                    docked_to_widget = true;
                }
            }
            if vert_near && !snapped_y {
                let top_align = by;
                let bot_align = by + bh - ah;
                let mid_align = by + (bh - ah) / 2;
                if (ny - top_align).abs() <= ALIGN_DIST {
                    ny = top_align;
                    snapped_y = true;
                    docked_to_widget = true;
                } else if (ny - bot_align).abs() <= ALIGN_DIST {
                    ny = bot_align;
                    snapped_y = true;
                    docked_to_widget = true;
                } else if (ny - mid_align).abs() <= ALIGN_DIST {
                    ny = mid_align;
                    snapped_y = true;
                    docked_to_widget = true;
                }
            }
        }

        // 4. Soft screen edge margin snap when floating freely near screen edges
        if !snapped_x {
            if (nx - min_x).abs() <= 20 {
                nx = min_x;
            } else if (nx - max_x).abs() <= 20 {
                nx = max_x;
            }
        }
        if !snapped_y {
            if (ny - min_y).abs() <= 20 {
                ny = min_y;
            } else if (ny - max_y).abs() <= 20 {
                ny = max_y;
            }
        }

        (nx.clamp(min_x, max_x), ny.clamp(min_y, max_y), docked_to_widget)
    }

    /// Prevents widgets from ending up overlapping on drop by cleanly settling into the closest grid slot!
    fn resolve_widget_overlap_on_drop(&mut self, kind: WidgetKind) {
        const GRID_GAP: i32 = 12;
        const EDGE_PAD: i32 = 18;
        let Some(me) = self.state.widgets.get(kind).cloned() else { return };
        let (aw, ah) = widget_window_dims(kind, me.size_tier);
        let min_x = EDGE_PAD;
        let max_x = (self.screen_w - aw - EDGE_PAD).max(min_x);
        let min_y = TOPBAR_HEIGHT as i32 + 12;
        let max_y = (self.screen_h - ah - EDGE_PAD).max(min_y);

        let mut nx = me.x;
        let mut ny = me.y;
        let mut moved = false;

        for other in &self.state.widgets.widgets {
            if other.kind == kind || !other.enabled {
                continue;
            }
            let (bw, bh) = widget_window_dims(other.kind, other.size_tier);
            let ox1 = nx.max(other.x);
            let oy1 = ny.max(other.y);
            let ox2 = (nx + aw).min(other.x + bw);
            let oy2 = (ny + ah).min(other.y + bh);
            if ox2 - ox1 > 8 && oy2 - oy1 > 8 {
                let candidates = [
                    (other.x, other.y + bh + GRID_GAP),
                    (other.x, other.y - ah - GRID_GAP),
                    (other.x + bw + GRID_GAP, other.y),
                    (other.x - aw - GRID_GAP, other.y),
                ];
                if let Some(&(cx, cy)) = candidates
                    .iter()
                    .filter(|&&(cx, cy)| cx >= min_x && cx <= max_x && cy >= min_y && cy <= max_y)
                    .min_by_key(|&&(cx, cy)| (cx - nx).abs() + (cy - ny).abs())
                {
                    nx = cx;
                    ny = cy;
                    moved = true;
                }
            }
        }
        if moved {
            if let Some(cfg) = self.state.widgets.get_mut(kind) {
                cfg.x = nx;
                cfg.y = ny;
            }
            if let Some(ww) = self.widget_wins.iter().find(|w| w.kind == kind) {
                unsafe {
                    let _ = SetWindowPos(ww.hwnd, HWND_BOTTOM, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                }
            }
        }
    }

    /// 1-click Smart Grid Auto-Arrange: lines up all active widgets cleanly with uniform 12px spacing!
    fn arrange_widgets_in_grid(&mut self) {
        const GRID_GAP: i32 = 12;
        const EDGE_PAD: i32 = 24;
        let top_start = TOPBAR_HEIGHT as i32 + 24;
        let mut enabled: Vec<(WidgetKind, i32, i32)> = self
            .state
            .widgets
            .widgets
            .iter()
            .filter(|w| w.enabled)
            .map(|w| {
                let (ww, wh) = widget_window_dims(w.kind, w.size_tier);
                (w.kind, ww, wh)
            })
            .collect();
        if enabled.is_empty() {
            return;
        }
        let max_col_w = enabled.iter().map(|(_, w, _)| *w).max().unwrap_or(420);
        let col_x = (self.screen_w - max_col_w - EDGE_PAD).max(EDGE_PAD);
        let mut cy = top_start;
        let mut col_idx = 0;

        for (kind, ww, wh) in enabled.drain(..) {
            if cy + wh > self.screen_h - EDGE_PAD && col_idx == 0 {
                col_idx = 1;
                cy = top_start;
            }
            let x = if col_idx == 0 {
                col_x + (max_col_w - ww) // flush right column alignment!
            } else {
                (col_x - ww - GRID_GAP).max(EDGE_PAD)
            };
            if let Some(cfg) = self.state.widgets.get_mut(kind) {
                cfg.x = x;
                cfg.y = cy;
            }
            cy += wh + GRID_GAP;
        }
        save_widgets(&self.state.widgets);
        self.sync_widget_windows_visibility();
        self.redraw_all_widgets();
    }

    fn redraw_all_widgets(&mut self) {
        for wk in [WidgetKind::Clock, WidgetKind::Music, WidgetKind::Weather, WidgetKind::Monitor] {
            self.redraw_widget(wk);
        }
    }

    fn sync_widget_windows_visibility(&mut self) {
        let global_on = self.state.settings.show_desktop_widgets;
        for w in &mut self.widget_wins {
            if let Some(cfg) = self.state.widgets.get(w.kind) {
                let (ww, wh) = widget_window_dims(w.kind, cfg.size_tier);
                unsafe {
                    if global_on && cfg.enabled {
                        w.surf.ensure_size(&self.gpu, ww as u32, wh as u32);
                        let _ = SetWindowPos(w.hwnd, HWND_BOTTOM, cfg.x, cfg.y, ww, wh, SWP_NOACTIVATE);
                        let _ = ShowWindow(w.hwnd, SW_SHOWNOACTIVATE);
                    } else {
                        let _ = ShowWindow(w.hwnd, SW_HIDE);
                        w.surf.trim_hidden(&self.gpu);
                    }
                }
            }
        }
    }

    // ==================== TRUE TOGGLE OVERLAY ORCHESTRATION ====================

    fn close_all_overlays(&mut self) {
        self.close_overview();
        self.close_cc();
        self.close_power();
        self.close_calendar();
        self.close_clipboard();
        self.close_launchpad();
    }

    fn open_overview(&mut self, settings_view: bool) {
        self.open_overview_ex(settings_view, false);
    }

    fn open_overview_ex(&mut self, settings_view: bool, instant_kbd: bool) {
        self.close_cc();
        self.close_power();
        self.close_calendar();
        self.close_clipboard();
        self.close_launchpad();
        let body_h = (self.screen_h - TOPBAR_HEIGHT as i32).max(400) as u32;
        self.surf_overview.ensure_size(&self.gpu, self.screen_w as u32, body_h);
        let was_closed = !self.state.overview_open;
        self.state.overview_open = true;
        self.state.overview_settings_view = settings_view;
        self.state.add_modal.open = false;
        self.state.gh_input_active = false;
        self.state.search_query.clear();
        self.state.search_selected = 0;
        self.state.open_windows = win32_shell::get_open_user_windows();
        let _ = self.telemetry.tx.send(TelemetryCmd::RefreshOverview);
        if self.state.mode == ShellMode::Dev && self.dev_hub_rx.is_none() {
            self.spawn_dev_hub_refresh();
        }
        if was_closed {
            if instant_kbd || self.state.settings.performance_mode {
                // Emil Kowalski Rule #1: Keyboard actions snap instantaneously (0ms Raycast feel!)
                self.state.overview_anim.snap(1.0);
            } else {
                self.state.overview_anim.snap(0.18);
                self.state.overview_anim.animate_to(1.0, 155.0, CubicBezier::EASE_OUT);
            }
            self.state.tab_content_anim.snap(1.0);
        }
        self.redraw_overview();
        self.redraw_topbar();
        win32_shell::bring_to_front(self.hwnd_overview);
        win32_shell::ensure_topmost(self.hwnd_topbar);
    }

    fn open_calendar(&mut self) {
        self.close_overview();
        self.close_cc();
        self.close_power();
        self.close_clipboard();
        self.close_launchpad();
        self.surf_calendar.ensure_size(&self.gpu, CAL_WIDTH as u32, CAL_HEIGHT as u32);
        let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        if self.state.cal_year == 0 {
            self.state.cal_year = st.wYear as i32;
            self.state.cal_month = st.wMonth as i32;
        }
        self.state.calendar_open = true;
        self.state.calendar_anim.snap(0.15);
        self.state.calendar_anim.animate_to(1.0, 155.0, CubicBezier::EASE_OUT);
        self.redraw_calendar();
        self.redraw_topbar();
        win32_shell::bring_to_front(self.hwnd_calendar);
    }

    fn close_calendar(&mut self) {
        if !self.state.calendar_open {
            return;
        }
        self.state.calendar_open = false;
        self.state.calendar_last_closed = Some(Instant::now());
        self.state.calendar_anim.animate_to(0.0, 115.0, CubicBezier::EASE_OUT);
        self.redraw_topbar();
    }

    fn open_clipboard(&mut self) {
        self.state.clip_target_hwnd = win32_shell::get_active_user_hwnd();
        self.poll_clipboard_history();
        self.close_overview();
        self.close_cc();
        self.close_power();
        self.close_calendar();
        self.close_launchpad();
        self.surf_clipboard.ensure_size(&self.gpu, CLIP_WIDTH as u32, CLIP_HEIGHT as u32);
        self.state.clipboard_open = true;
        self.state.clipboard_anim.snap(1.0); // Keyboard Win+V opens instantaneously!
        self.redraw_clipboard();
        win32_shell::bring_to_front(self.hwnd_clipboard);
    }

    fn close_clipboard(&mut self) {
        if !self.state.clipboard_open {
            return;
        }
        self.state.clipboard_open = false;
        self.state.clipboard_last_closed = Some(Instant::now());
        self.state.clipboard_anim.animate_to(0.0, 110.0, CubicBezier::EASE_OUT);
    }

    fn poll_clipboard_history(&mut self) {
        let seq = ui_clipboard::get_clipboard_seq();
        if seq == self.state.last_clip_seq {
            return;
        }
        self.state.last_clip_seq = seq;
        if let Some(txt) = win32_shell::read_clipboard_text() {
            let trimmed = txt.trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            if let Some(pos) = self.state.clipboard_items.iter().position(|c| c.text == trimmed) {
                let existing = self.state.clipboard_items.remove(pos);
                self.state.clipboard_items.insert(0, existing);
            } else {
                let chars_len = trimmed.chars().count();
                self.state.clipboard_items.insert(
                    0,
                    ClipItem {
                        text: trimmed,
                        pinned: false,
                        chars_len,
                    },
                );
                if self.state.clipboard_items.len() > 25 {
                    if let Some(unpinned) = self.state.clipboard_items.iter().rposition(|c| !c.pinned) {
                        self.state.clipboard_items.remove(unpinned);
                    }
                }
            }
        }
    }

    fn close_overview(&mut self) {
        if !self.state.overview_open {
            return;
        }
        self.state.overview_open = false;
        self.state.overview_settings_view = false;
        self.state.add_modal.open = false;
        self.state.overview_last_closed = Some(Instant::now());
        self.state.overview_anim.animate_to(0.0, 135.0, CubicBezier::EASE_OUT);
        self.redraw_topbar();
    }

    fn open_cc(&mut self) {
        self.close_overview();
        self.close_power();
        self.close_calendar();
        self.close_clipboard();
        self.close_launchpad();
        let cc_h = (self.screen_h - TOPBAR_HEIGHT as i32 - 18).min(CC_MAX_H as i32).max(400) as u32;
        self.surf_cc.ensure_size(&self.gpu, CC_WIDTH as u32, cc_h);
        self.state.uptime_label = win32_shell::get_uptime_short();
        self.state.keyboard_lang = win32_shell::get_keyboard_layout_short();
        self.state.active_keyboard_hkl = win32_shell::get_active_keyboard_hkl();
        self.state.keyboard_layouts = win32_shell::get_installed_keyboard_layouts();
        self.state.cc_keyboard_menu_open = false;
        self.state.todo_input_active = false;
        self.state.cc_scroll = 0.0;
        self.state.cc_open = true;

        // Trigger fresh background MTA update + mark Control Center active (ZERO polling when closed!)
        let _ = self.telemetry.tx.send(TelemetryCmd::SetControlCenterOpen(true));
        let _ = self.telemetry.tx.send(TelemetryCmd::RefreshControlCenter);

        self.state.cc_anim.snap(0.01);
        self.state.cc_anim.animate_to(1.0, 175.0, CubicBezier::EASE_OUT);
        self.redraw_cc();
        self.redraw_topbar();
        win32_shell::bring_to_front(self.hwnd_cc);
    }

    fn close_cc(&mut self) {
        if !self.state.cc_open {
            return;
        }
        self.state.cc_open = false;
        self.state.cc_keyboard_menu_open = false;
        self.state.todo_input_active = false;
        let _ = self.telemetry.tx.send(TelemetryCmd::SetControlCenterOpen(false));
        self.state.cc_last_closed = Some(Instant::now());
        self.state.cc_anim.animate_to(0.0, 135.0, CubicBezier::EASE_OUT);
        self.redraw_topbar();
    }

    fn open_power(&mut self) {
        self.close_overview();
        self.close_cc();
        self.close_calendar();
        self.close_clipboard();
        self.close_launchpad();
        self.surf_power.ensure_size(&self.gpu, POWER_MENU_W as u32, POWER_MENU_H as u32);
        self.state.power_confirm = None;
        self.state.power_open = true;
        self.state.power_anim.snap(0.01);
        self.state.power_anim.animate_to(1.0, 180.0, CubicBezier::EASE_OUT);
        self.redraw_power();
        self.redraw_topbar();
        win32_shell::bring_to_front(self.hwnd_power);
    }

    fn close_power(&mut self) {
        if !self.state.power_open {
            return;
        }
        self.state.power_open = false;
        self.state.power_confirm = None;
        self.state.power_last_closed = Some(Instant::now());
        self.state.power_anim.animate_to(0.0, 120.0, CubicBezier::EASE_OUT);
        self.redraw_topbar();
    }

    fn open_launchpad(&mut self) {
        self.close_overview();
        self.close_cc();
        self.close_power();
        self.close_calendar();
        self.close_clipboard();
        let body_h = (self.screen_h - TOPBAR_HEIGHT as i32).max(400) as u32;
        self.surf_launchpad.ensure_size(&self.gpu, self.screen_w as u32, body_h);
        self.state.launchpad_query.clear();
        self.state.launchpad_scroll = 0.0;
        self.state.launchpad_scroll_target = 0.0;
        self.state.launchpad_open = true;
        // Snappy 165ms zero-layer GPU reveal (butter-smooth even during 100% GPU games!)
        self.state.launchpad_anim.snap(0.01);
        self.state.launchpad_anim.animate_to(1.0, 165.0, CubicBezier::EASE_OUT);
        self.redraw_launchpad();
        self.redraw_dock();
        win32_shell::bring_to_front(self.hwnd_launchpad);
    }

    fn close_launchpad(&mut self) {
        if !self.state.launchpad_open {
            return;
        }
        self.state.launchpad_open = false;
        self.state.launchpad_last_closed = Some(Instant::now());
        self.state.launchpad_anim.animate_to(0.0, 175.0, CubicBezier::EASE_OUT);
        self.redraw_dock();
    }

    // ==================== ACTION DISPATCHER ====================

    fn execute(&mut self, action: Action) {
        match action {
            Action::ToggleOverview => {
                if self.state.overview_open || AppState::was_just_closed(self.state.overview_last_closed) {
                    self.close_overview();
                } else {
                    self.state.overview_origin_x = 80.0;
                    self.open_overview(false);
                }
            }
            Action::ClickModeTab(m) => {
                let prev_idx = match self.state.mode {
                    ShellMode::PC => 0.0,
                    ShellMode::Dev => 1.0,
                    ShellMode::Gamer => 2.0,
                    ShellMode::Creative => 3.0,
                    ShellMode::Studio => 4.0,
                };
                let target_idx = match m {
                    ShellMode::PC => 0.0,
                    ShellMode::Dev => 1.0,
                    ShellMode::Gamer => 2.0,
                    ShellMode::Creative => 3.0,
                    ShellMode::Studio => 4.0,
                };
                let same_mode = self.state.mode == m && !self.state.overview_settings_view;
                if same_mode && self.state.overview_open {
                    self.close_overview();
                    return;
                }
                self.state.mode = m;
                self.state.settings.last_mode = m;
                save_settings(&self.state.settings);
                self.state.overview_tab_pill_x.animate_to(target_idx, 185.0, CubicBezier::EASE_IN_OUT);
                if self.state.overview_open {
                    self.state.overview_settings_view = false;
                    if (target_idx - prev_idx).abs() > 0.1 {
                        self.state.tab_slide_dir = if target_idx > prev_idx { 1.0 } else { -1.0 };
                        self.state.tab_content_anim.snap(0.0);
                        self.state.tab_content_anim.animate_to(1.0, 180.0, CubicBezier::EASE_OUT);
                    }
                    self.redraw_overview();
                    self.redraw_topbar();
                } else {
                    self.state.overview_origin_x = (self.screen_w as f32 * 0.62).clamp(240.0, self.screen_w as f32 - 360.0);
                    self.open_overview(false);
                }
            }
            Action::ToggleControlCenter => {
                if self.state.cc_open || AppState::was_just_closed(self.state.cc_last_closed) {
                    self.close_cc();
                } else {
                    self.open_cc();
                }
            }
            Action::TogglePowerMenu => {
                if self.state.power_open || AppState::was_just_closed(self.state.power_last_closed) {
                    self.close_power();
                } else {
                    self.open_power();
                }
            }
            Action::ToggleLaunchpad => {
                if self.state.launchpad_open || AppState::was_just_closed(self.state.launchpad_last_closed) {
                    self.close_launchpad();
                } else {
                    self.open_launchpad();
                }
            }
            Action::OpenSettings => {
                self.close_all_overlays();
                win32_shell::launch_aura_settings(None);
            }
            Action::LaunchAuraSettings(tab) => {
                self.close_all_overlays();
                win32_shell::launch_aura_settings(tab.as_deref());
            }
            Action::LaunchAuraLock => {
                self.close_all_overlays();
                win32_shell::launch_aura_lock();
            }
            Action::LaunchAuraTasks => {
                self.close_all_overlays();
                win32_shell::launch_aura_tasks();
            }
            Action::LaunchAuraShot => {
                self.close_all_overlays();
                win32_shell::launch_aura_shot();
            }
            Action::LaunchAuraPdf => {
                self.close_all_overlays();
                win32_shell::launch_aura_pdf(None);
            }
            Action::LaunchAuraBeats => {
                self.close_all_overlays();
                if !win32_shell::launch_aurabeats(None) {
                    // Refresh status in case it was installed after shell startup
                    self.state.aurabeats_installed = win32_shell::resolve_aurabeats_path().is_some();
                    self.state.aurabeats_plugin_count = win32_shell::count_aurabeats_plugins();
                }
            }
            Action::LaunchAuraApp(exe) => {
                self.close_all_overlays();
                let _ = win32_shell::launch_aura_app(exe, None);
            }
            Action::ToggleCalendar => {
                if self.state.calendar_open || AppState::was_just_closed(self.state.calendar_last_closed) {
                    self.close_calendar();
                } else {
                    self.open_calendar();
                }
            }
            Action::CalPrevMonth => {
                if self.state.cal_month <= 1 {
                    self.state.cal_month = 12;
                    self.state.cal_year -= 1;
                } else {
                    self.state.cal_month -= 1;
                }
                self.redraw_calendar();
            }
            Action::CalNextMonth => {
                if self.state.cal_month >= 12 {
                    self.state.cal_month = 1;
                    self.state.cal_year += 1;
                } else {
                    self.state.cal_month += 1;
                }
                self.redraw_calendar();
            }
            Action::CalToday => {
                let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
                self.state.cal_year = st.wYear as i32;
                self.state.cal_month = st.wMonth as i32;
                self.redraw_calendar();
            }
            Action::PomodoroToggle => {
                self.state.pomodoro_active = !self.state.pomodoro_active;
                if self.state.pomodoro_secs_left == 0 {
                    self.state.pomodoro_secs_left = if self.state.pomodoro_is_break { 5 * 60 } else { 25 * 60 };
                }
                self.redraw_calendar();
                self.redraw_topbar();
            }
            Action::PomodoroMode(is_brk) => {
                self.state.pomodoro_is_break = is_brk;
                self.state.pomodoro_secs_left = if is_brk { 5 * 60 } else { 25 * 60 };
                self.state.pomodoro_active = false;
                self.redraw_calendar();
                self.redraw_topbar();
            }
            Action::ToggleClipboard => {
                if self.state.clipboard_open || AppState::was_just_closed(self.state.clipboard_last_closed) {
                    self.close_clipboard();
                } else {
                    self.open_clipboard();
                }
            }
            Action::PasteClip(idx) => {
                if let Some(item) = self.state.clipboard_items.get(idx).cloned() {
                    let target_hw = self.state.clip_target_hwnd;
                    self.close_clipboard();
                    win32_shell::copy_text_to_clipboard(&item.text);
                    self.state.last_clip_seq = ui_clipboard::get_clipboard_seq();
                    if target_hw != 0 {
                        win32_shell::bring_to_front(HWND(target_hw as *mut _));
                        // Simulate Ctrl+V into target window after restoring foreground focus!
                        unsafe {
                            use windows::Win32::UI::Input::KeyboardAndMouse::{
                                keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
                            };
                            keybd_event(0x11, 0, KEYBD_EVENT_FLAGS(0), 0); // VK_CONTROL
                            keybd_event(b'V', 0, KEYBD_EVENT_FLAGS(0), 0);
                            keybd_event(b'V', 0, KEYEVENTF_KEYUP, 0);
                            keybd_event(0x11, 0, KEYEVENTF_KEYUP, 0);
                        }
                    }
                }
            }
            Action::PinClip(idx) => {
                if let Some(item) = self.state.clipboard_items.get_mut(idx) {
                    item.pinned = !item.pinned;
                    self.redraw_clipboard();
                }
            }
            Action::DeleteClip(idx) => {
                if idx < self.state.clipboard_items.len() {
                    self.state.clipboard_items.remove(idx);
                    self.redraw_clipboard();
                }
            }
            Action::ClearClips => {
                self.state.clipboard_items.retain(|c| c.pinned);
                self.redraw_clipboard();
            }
            Action::CopyTextAndClose(txt) => {
                win32_shell::copy_text_to_clipboard(&txt);
                self.state.last_clip_seq = ui_clipboard::get_clipboard_seq();
                self.close_all_overlays();
            }
            Action::RunShellCommand(cmd) => {
                self.close_all_overlays();
                let _ = std::process::Command::new("cmd.exe")
                    .args(["/C", "start", "", &cmd])
                    .spawn();
            }
            Action::CloseAllOverlays => self.close_all_overlays(),
            Action::FocusSearchBar => {
                if self.state.overview_open {
                    win32_shell::bring_to_front(self.hwnd_overview);
                    self.redraw_overview();
                } else if self.state.launchpad_open {
                    win32_shell::bring_to_front(self.hwnd_launchpad);
                    self.redraw_launchpad();
                }
            }
            Action::ClearSearchBar => {
                if self.state.overview_open {
                    self.state.search_query.clear();
                    self.state.search_selected = 0;
                    win32_shell::bring_to_front(self.hwnd_overview);
                    self.redraw_overview();
                } else if self.state.launchpad_open {
                    self.state.launchpad_query.clear();
                    self.state.launchpad_scroll = 0.0;
                    self.state.launchpad_scroll_target = 0.0;
                    win32_shell::bring_to_front(self.hwnd_launchpad);
                    self.redraw_launchpad();
                }
            }
            Action::ActiveWinClose => {
                window_styler::close_active_window(self.state.active_user_hwnd);
            }
            Action::ActiveWinMin => {
                let target = self.state.active_user_hwnd;
                if target != 0 && !self.state.settings.performance_mode {
                    let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                    let (gx, gy) = ui_dock::dock_icon_screen_center(&self.state, self.dock_geo, is_left, target);
                    self.genie.start_minimize(&self.gpu, target, gx, gy, is_left);
                    self.wake_anim_timer();
                } else {
                    window_styler::minimize_active_window(target);
                }
            }
            Action::ActiveWinMaxToggle => {
                window_styler::toggle_maximize_active_window(self.state.active_user_hwnd);
            }
            Action::LaunchAuraFiles(folder) => {
                self.close_all_overlays();
                win32_shell::launch_aurafiles(folder.as_deref());
            }
            Action::MediaPrev => {
                // 100% async detached MTA transport — ZERO WinRT calls on UI thread!
                audio::media_prev();
                if self.state.cc_open { self.redraw_cc(); }
                self.redraw_widget(WidgetKind::Music);
            }
            Action::MediaPlayPause => {
                // 100% async detached MTA transport — safe even when ZERO songs are playing!
                audio::media_play_pause();
                if self.state.now_playing.active {
                    let cur_pos = self.state.now_playing.live_position_sec();
                    self.state.now_playing.position_sec = cur_pos;
                    self.state.now_playing.sampled_at_unix_ms = now_unix_ms();
                    self.state.now_playing.is_playing = !self.state.now_playing.is_playing;
                }
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
                self.redraw_widget(WidgetKind::Music);
            }
            Action::MediaNext => {
                // 100% async detached MTA transport — ZERO WinRT calls on UI thread!
                audio::media_next();
                if self.state.cc_open { self.redraw_cc(); }
                self.redraw_widget(WidgetKind::Music);
            }
            Action::MediaSeek(frac) => {
                let f = frac.clamp(0.0, 1.0);
                if self.state.now_playing.has_timeline && self.state.now_playing.duration_sec > 0.1 {
                    self.state.now_playing.position_sec = self.state.now_playing.duration_sec * f as f64;
                    self.state.now_playing.sampled_at_unix_ms = now_unix_ms();
                    audio::media_seek_fraction(f);
                    if self.state.cc_open { self.redraw_cc(); }
                    self.redraw_widget(WidgetKind::Music);
                }
            }
            Action::ToggleMasterMute => {
                self.state.master_muted = audio::toggle_master_mute();
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
                self.trigger_volume_osd();
            }
            Action::SetMasterVolume(p) => {
                let p = p.clamp(0, 100);
                audio::set_master_volume(p);
                self.state.master_vol = p;
                self.state.master_muted = false;
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
                self.trigger_volume_osd();
            }
            Action::SwitchAudioDeviceTab(mics) => {
                self.state.cc_showing_mics = mics;
                self.redraw_cc();
            }
            Action::SelectAudioDevice(id) => {
                if audio::set_default_audio_device(&id) {
                    self.state.output_devices = audio::get_audio_devices(false);
                    self.state.mic_devices = audio::get_audio_devices(true);
                    let (mv, mm) = audio::get_master_volume();
                    self.state.master_vol = mv;
                    self.state.master_muted = mm;
                    self.state.app_sessions = audio::get_app_audio_sessions();
                    self.redraw_topbar();
                    self.redraw_cc();
                }
            }
            Action::ToggleAppMute(pids, muted) => {
                audio::set_app_session_mute(&pids, muted);
                for s in &mut self.state.app_sessions {
                    if s.pids == pids { s.is_muted = muted; }
                }
                self.redraw_cc();
            }
            Action::SetAppVolume(pids, pct) => {
                let p = pct.clamp(0, 100);
                audio::set_app_session_volume(&pids, p);
                for s in &mut self.state.app_sessions {
                    if s.pids == pids {
                        s.volume_percent = p;
                        s.is_muted = false;
                    }
                }
                self.redraw_cc();
            }
            Action::ToggleWifiRadio => {
                // Optimistic UI feedback + instant background WinRT Radio toggle (0ms UI block!)
                self.state.radios.wifi_on = !self.state.radios.wifi_on;
                self.state.radios.net_label = if self.state.radios.wifi_on {
                    "Activando Wi-Fi...".into()
                } else {
                    "Apagando Wi-Fi...".into()
                };
                if self.state.cc_open { self.redraw_cc(); }
                let (tx, rx) = mpsc::channel();
                self.radio_rx = Some(rx);
                std::thread::spawn(move || {
                    let _ = radios::toggle_wifi();
                    let _ = tx.send(radios::query_radio_states());
                });
            }
            Action::ToggleBluetoothRadio => {
                self.state.radios.bt_on = !self.state.radios.bt_on;
                if self.state.cc_open { self.redraw_cc(); }
                let (tx, rx) = mpsc::channel();
                self.radio_rx = Some(rx);
                std::thread::spawn(move || {
                    let _ = radios::toggle_bluetooth();
                    let _ = tx.send(radios::query_radio_states());
                });
            }
            Action::SetBrightness(pct) => {
                let p = pct.clamp(35, 100);
                self.state.settings.brightness_pct = p;
                win32_shell::apply_display_gamma(p, self.state.settings.night_light);
                save_settings(&self.state.settings);
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::ToggleNightLight => {
                self.state.settings.night_light = !self.state.settings.night_light;
                win32_shell::apply_display_gamma(self.state.settings.brightness_pct, self.state.settings.night_light);
                save_settings(&self.state.settings);
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::ToggleKeepAwake => {
                self.state.settings.keep_awake = !self.state.settings.keep_awake;
                win32_shell::set_keep_awake(self.state.settings.keep_awake);
                save_settings(&self.state.settings);
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::SetCcBottomTab(t) => {
                self.state.cc_bottom_tab = t;
                self.state.todo_input_active = false;
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::ToggleTodo(id) => {
                if let Some(it) = self.state.todos.iter_mut().find(|x| x.id == id) {
                    it.done = !it.done;
                    dev_hub::save_todos(&self.state.todos);
                }
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::DeleteTodo(id) => {
                self.state.todos.retain(|x| x.id != id);
                dev_hub::save_todos(&self.state.todos);
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::StartTodoInput => {
                self.state.todo_input_active = true;
                if self.state.cc_open {
                    win32_shell::bring_to_front(self.hwnd_cc);
                    self.redraw_cc();
                }
            }
            Action::SubmitTodoInput => {
                let txt = self.state.todo_input_text.trim().to_string();
                if !txt.is_empty() {
                    let next_id = self.state.todos.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                    self.state.todos.insert(0, dev_hub::TodoItem { id: next_id, text: txt, done: false });
                    self.state.todo_input_text.clear();
                    dev_hub::save_todos(&self.state.todos);
                }
                self.state.todo_input_active = false;
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::AdjustAiQuota(prov_id, delta) => {
                match prov_id.as_str() {
                    "antigravity" => self.state.dev_hub_cfg.antigravity_pct = (self.state.dev_hub_cfg.antigravity_pct + delta).clamp(0, 100),
                    "gemini" => self.state.dev_hub_cfg.gemini_pct = (self.state.dev_hub_cfg.gemini_pct + delta).clamp(0, 100),
                    "claude" => self.state.dev_hub_cfg.claude_pct = (self.state.dev_hub_cfg.claude_pct + delta).clamp(0, 100),
                    _ => {}
                }
                dev_hub::save_dev_hub_config(&self.state.dev_hub_cfg);
                for p in &mut self.state.dev_hub.providers {
                    if p.id == prov_id {
                        p.pct_remaining = (p.pct_remaining + delta).clamp(0, 100);
                    }
                }
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ResetAiQuota(prov_id) => {
                match prov_id.as_str() {
                    "antigravity" => self.state.dev_hub_cfg.antigravity_pct = 100,
                    "gemini" => self.state.dev_hub_cfg.gemini_pct = 100,
                    "claude" => self.state.dev_hub_cfg.claude_pct = 100,
                    _ => {}
                }
                dev_hub::save_dev_hub_config(&self.state.dev_hub_cfg);
                for p in &mut self.state.dev_hub.providers {
                    if p.id == prov_id {
                        p.pct_remaining = 100;
                    }
                }
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::RefreshDevHub => {
                self.spawn_dev_hub_refresh();
            }
            Action::StartGithubUserEdit => {
                self.state.gh_input_active = true;
                self.state.gh_input_text = self.state.dev_hub.github.username.clone();
                if self.state.overview_open {
                    win32_shell::bring_to_front(self.hwnd_overview);
                    self.redraw_overview();
                }
            }
            Action::SubmitGithubUserEdit => {
                self.state.gh_input_active = false;
                self.state.dev_hub_cfg.github_username = self.state.gh_input_text.trim().trim_start_matches('@').to_string();
                dev_hub::save_dev_hub_config(&self.state.dev_hub_cfg);
                self.spawn_dev_hub_refresh();
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ToggleKeyboardMenu => {
                self.state.keyboard_layouts = win32_shell::get_installed_keyboard_layouts();
                if self.state.keyboard_layouts.len() <= 1 {
                    win32_shell::cycle_keyboard_layout();
                    self.state.keyboard_lang = win32_shell::get_keyboard_layout_short();
                    self.redraw_topbar();
                } else {
                    self.state.cc_keyboard_menu_open = !self.state.cc_keyboard_menu_open;
                }
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::SelectKeyboardLayout(hkl) => {
                win32_shell::select_keyboard_layout(hkl);
                self.state.active_keyboard_hkl = hkl;
                if let Some(found) = self.state.keyboard_layouts.iter().find(|l| l.hkl == hkl) {
                    self.state.keyboard_lang = if found.short_code.starts_with("EN") { "ENG".into() } else { "ESP".into() };
                }
                self.state.cc_keyboard_menu_open = false;
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::CycleKeyboardLayout => {
                win32_shell::cycle_keyboard_layout();
                self.state.keyboard_lang = win32_shell::get_keyboard_layout_short();
                self.state.active_keyboard_hkl = win32_shell::get_active_keyboard_hkl();
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::LaunchTarget(t) => {
                self.close_all_overlays();
                win32_shell::launch_target(&t, "");
            }
            Action::FocusWindow(hwnd) => {
                self.dock_preview.hide();
                self.close_all_overlays();
                let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                let (gx, gy) = ui_dock::dock_icon_screen_center(&self.state, self.dock_geo, is_left, hwnd);
                let hw = HWND(hwnd as *mut _);
                let is_iconic = unsafe { windows::Win32::UI::WindowsAndMessaging::IsIconic(hw).as_bool() };
                let is_fg = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() == hw };

                if !self.state.settings.performance_mode && is_iconic {
                    if self.genie.start_restore(&self.gpu, hwnd, gx, gy, is_left) {
                        self.wake_anim_timer();
                    } else {
                        win32_shell::toggle_or_focus_window(hwnd);
                    }
                } else if !self.state.settings.performance_mode && is_fg {
                    self.genie.start_minimize(&self.gpu, hwnd, gx, gy, is_left);
                    self.wake_anim_timer();
                } else {
                    win32_shell::toggle_or_focus_window(hwnd);
                }
            }
            Action::KillBackgroundApp(key) => {
                system::kill_process_by_name(&key);
                self.state.tray_apps = system::scan_background_tray_apps();
                if self.state.cc_open { self.redraw_cc(); }
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::CloseExternalWallpaperEngine => {
                system::kill_process_by_name("wallpaper64");
                system::kill_process_by_name("wallpaper32");
                self.state.tray_apps = system::scan_background_tray_apps();
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::SelectWallpaper(id) => {
                self.state.settings.wallpaper_id = id.clone();
                save_settings(&self.state.settings);
                self.wallpaper.set_wallpaper(&mut self.gpu, &id);
                self.sync_material_you_palette();
                self.redraw_topbar();
                self.redraw_dock();
                self.redraw_all_widgets();
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::AddCustomWallpaperFile => {
                self.spawn_file_picker(
                    FilePickKind::CustomWallpaper,
                    "Seleccionar video, imagen o web para Fondo de Pantalla",
                    "Fondos (*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi;*.jpg;*.png;*.webp;*.html)",
                    "*.mp4;*.webm;*.mkv;*.mov;*.m4v;*.wmv;*.avi;*.jpg;*.jpeg;*.png;*.webp;*.bmp;*.html",
                );
            }
            Action::ToggleWidget(kind) => {
                if let Some(cfg) = self.state.widgets.get_mut(kind) {
                    cfg.enabled = !cfg.enabled;
                    if cfg.enabled {
                        self.state.settings.show_desktop_widgets = true;
                        save_settings(&self.state.settings);
                    }
                }
                save_widgets(&self.state.widgets);
                self.sync_widget_windows_visibility();
                self.redraw_widget(kind);
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ResizeWidget(kind, delta) => {
                if let Some(cfg) = self.state.widgets.get_mut(kind) {
                    let next = (cfg.size_tier as i8 + delta).clamp(0, 2) as u8;
                    cfg.size_tier = next;
                }
                save_widgets(&self.state.widgets);
                self.sync_widget_windows_visibility();
                self.redraw_widget(kind);
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ArrangeWidgetsGrid => {
                self.arrange_widgets_in_grid();
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ResetWidgetsLayout => {
                let enabled_flags: Vec<(WidgetKind, bool)> =
                    self.state.widgets.widgets.iter().map(|w| (w.kind, w.enabled)).collect();
                self.state.widgets = WidgetsStore::default_for_screen(self.screen_w, self.screen_h);
                for (k, en) in enabled_flags {
                    if let Some(w) = self.state.widgets.get_mut(k) {
                        w.enabled = en;
                    }
                }
                save_widgets(&self.state.widgets);
                self.sync_widget_windows_visibility();
                self.redraw_all_widgets();
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::LaunchUpdater => {
                self.close_all_overlays();
                let setup = app_data_dir().join("AuraShell-Setup.exe");
                if setup.exists() {
                    let arg = if self.state.update_available_ver.is_some() { "--silent-update" } else { "" };
                    win32_shell::launch_target(&setup.to_string_lossy(), arg);
                }
            }
            Action::ApplyPendingUpdate => {
                if let Some(upd) = self.state.pending_update.clone() {
                    self.close_all_overlays();
                    updater::trigger_apply_update(&upd);
                }
            }
            Action::RestoreWindowsAndExit => {
                hotkey_hook::uninstall();
                self.dock_preview.shutdown();
                self.genie.shutdown();
                self.wallpaper.shutdown();
                win32_shell::restore_windows_safely();
                unsafe { PostQuitMessage(0); }
            }
            Action::PowerSleep => {
                self.close_power();
                win32_shell::power_sleep();
            }
            Action::PowerLock => {
                self.close_power();
                win32_shell::power_lock();
            }
            Action::PowerRestartConfirm => {
                if self.state.power_confirm == Some("restart") {
                    self.wallpaper.shutdown();
                    win32_shell::power_restart();
                } else {
                    self.state.power_confirm = Some("restart");
                    self.redraw_power();
                }
            }
            Action::PowerShutdownConfirm => {
                if self.state.power_confirm == Some("shutdown") {
                    self.wallpaper.shutdown();
                    win32_shell::power_shutdown();
                } else {
                    self.state.power_confirm = Some("shutdown");
                    self.redraw_power();
                }
            }
            Action::SetGlassOpacity(v) => {
                self.state.settings.glass_opacity = v.clamp(0.50, 0.96);
                save_settings(&self.state.settings);
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::ToggleGameBoost => {
                self.state.game_boost_status = self.boost.toggle();
                self.state.game_boost_active = self.boost.active;
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::OpenAddDevWebModal => {
                self.state.add_modal = ui_state::AddDevWebModal {
                    open: true,
                    name: String::new(),
                    url: "https://".to_string(),
                    active_field: 0,
                    error: String::new(),
                };
                self.redraw_overview();
            }
            Action::AddDevLocalFile => {
                self.spawn_file_picker(
                    FilePickKind::DevLocal,
                    "Seleccionar aplicación o herramienta IA / Dev",
                    "Aplicaciones (*.exe;*.lnk)",
                    "*.exe;*.lnk",
                );
            }
            Action::AddCustomGameFile => {
                self.spawn_file_picker(
                    FilePickKind::CustomGame,
                    "Seleccionar juego para tu Biblioteca Gamer",
                    "Juegos (*.exe;*.lnk)",
                    "*.exe;*.lnk",
                );
            }
            Action::RemoveCustomDevItem(cid) => {
                self.state.custom_items.dev_ai_items.retain(|x| x.id != cid);
                save_custom_items(&self.state.custom_items);
                self.state.dev_ai_apps = system::build_dev_ai_entries(&self.state.all_apps, &self.state.custom_items);
                self.redraw_overview();
            }
            Action::RemoveCustomGame(cid) => {
                self.state.custom_items.custom_games.retain(|x| x.id != cid);
                save_custom_items(&self.state.custom_items);
                self.state.games = system::scan_all_games(&self.state.custom_items);
                self.redraw_overview();
            }
            Action::FocusModalField(idx) => {
                self.state.add_modal.active_field = idx;
                self.redraw_overview();
            }
            Action::CancelModal => {
                self.state.add_modal.open = false;
                self.redraw_overview();
            }
            Action::SubmitAddDevWebModal => {
                let name = self.state.add_modal.name.trim().to_string();
                let mut url = self.state.add_modal.url.trim().to_string();
                if name.is_empty() || url.len() < 5 {
                    self.state.add_modal.error = "Escribe un nombre y una URL válida (ej. https://claude.ai)".into();
                    self.redraw_overview();
                    return;
                }
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    url = format!("https://{}", url);
                }
                let id = format!("web_{}", self.state.custom_items.dev_ai_items.len() + 100);
                self.state.custom_items.dev_ai_items.push(CustomEntry {
                    id,
                    name,
                    target: url,
                    subtitle: "IA Web personalizada".into(),
                    category: "DevAI".into(),
                    cover_path: None,
                });
                save_custom_items(&self.state.custom_items);
                self.state.dev_ai_apps = system::build_dev_ai_entries(&self.state.all_apps, &self.state.custom_items);
                self.state.add_modal.open = false;
                self.redraw_overview();
            }
            Action::ToggleSetting(key) => {
                match key {
                    "hide_taskbar" => {
                        self.state.settings.hide_windows_taskbar = !self.state.settings.hide_windows_taskbar;
                        win32_shell::set_taskbar_visibility(!self.state.settings.hide_windows_taskbar);
                        win32_shell::apply_full_work_area(TOPBAR_HEIGHT as i32, self.state.settings.hide_windows_taskbar);
                    }
                    "style_windows_apps" => {
                        self.state.settings.style_windows_apps = !self.state.settings.style_windows_apps;
                        if self.state.settings.style_windows_apps {
                            window_styler::apply_aura_style_to_windows(&self.state.open_windows);
                        } else {
                            window_styler::restore_all_styled_windows();
                        }
                    }
                    "dock_auto_hide" => {
                        self.state.settings.dock_auto_hide = !self.state.settings.dock_auto_hide;
                        if !self.state.settings.dock_auto_hide {
                            self.state.dock_hidden = false;
                            self.show_dock_hide_notch();
                            self.state.dock_reveal.set_target(1.0);
                        }
                    }
                    "dock_left" => {
                        let now_left = self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                        self.state.settings.dock_position = if now_left { "Left".into() } else { "Bottom".into() };
                        self.dock_geo = (0, 0, 0, 0); // force geometry sync
                        self.sync_dock_tight_bounds();
                        let (nx, ny, nw, nh) = ui_dock::notch_geometry(self.screen_w, self.screen_h, now_left);
                        unsafe {
                            let _ = SetWindowPos(self.hwnd_notch, HWND_TOPMOST, nx, ny, nw, nh, SWP_NOACTIVATE);
                        }
                        self.surf_notch.resize(&self.gpu, nw as u32, nh as u32);
                        self.redraw_notch();
                        self.redraw_dock();
                    }
                    "desktop_clock" => {
                        self.state.settings.show_desktop_widgets = !self.state.settings.show_desktop_widgets;
                        self.sync_widget_windows_visibility();
                        self.redraw_all_widgets();
                    }
                    "wallpaper_auto_pause" => {
                        self.state.settings.wallpaper_auto_pause = !self.state.settings.wallpaper_auto_pause;
                        if !self.state.settings.wallpaper_auto_pause {
                            self.wallpaper.set_paused(false);
                        }
                    }
                    "auto_boost" => {
                        self.state.settings.auto_game_boost = !self.state.settings.auto_game_boost;
                    }
                    _ => {}
                }
                save_settings(&self.state.settings);
                self.redraw_overview();
            }
        }
    }

    fn spawn_file_picker(&mut self, kind: FilePickKind, title: &'static str, lbl: &'static str, spec: &'static str) {
        if self.file_pick_rx.is_some() {
            return;
        }
        // Hide Topmost Overview BEFORE spawning Windows IFileOpenDialog so it never opens behind AuraOverview!
        self.state.overview_open = false;
        self.state.overview_anim.snap(0.0);
        unsafe { let _ = ShowWindow(self.hwnd_overview, SW_HIDE); }
        self.redraw_topbar();

        let (tx, rx) = mpsc::channel();
        self.file_pick_rx = Some(rx);
        std::thread::spawn(move || {
            let chosen = win32_shell::pick_file_dialog(title, lbl, spec);
            let _ = tx.send((kind, chosen));
        });
    }

    // ==================== ANIMATION & SYSTEM TIMERS ====================

    fn tick_anim(&mut self) {
        let now = Instant::now();
        let dt_sec = now.duration_since(self.last_frame).as_secs_f32().clamp(0.004, 0.033);
        let dt_ms = dt_sec * 1000.0;
        self.last_frame = now;

        // 0a. Drain background startup catalog (apps, dev/ai, games, wallpapers, drives) once ready!
        if let Some(cat) = self.startup_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.startup_rx = None;
            self.state.all_apps = cat.all_apps;
            self.state.dev_ai_apps = cat.dev_ai_apps;
            self.state.games = cat.games;
            self.state.wallpapers = cat.wallpapers;
            self.state.drives = cat.drives;
            self.sync_material_you_palette();
            // Fix live QA Catch #1: ALWAYS re-sync tight dock bounds after all_apps populates pinned icons!
            self.sync_dock_tight_bounds();
            self.redraw_topbar();
            self.redraw_dock();
            self.redraw_all_widgets();
            // Only prewarm tiny icons for open windows (ZERO 100MB eager wallpaper/cover decoding at idle!)
            self.gpu.prewarm_icons(self.state.open_windows.iter().map(|w| w.exe_path.clone()));
            if self.state.settings.style_windows_apps {
                window_styler::apply_aura_style_to_windows(&self.state.open_windows);
            }
            if self.state.launchpad_open {
                self.redraw_launchpad();
            }
            if self.state.overview_open {
                self.redraw_overview();
            }
            if self.state.cc_open {
                self.redraw_cc();
            }
        }

        // 0b. Drain newly decoded icons/thumbnails from background worker thread (ZERO UI thread I/O!)
        let assets_arrived = self.gpu.drain_decoded_assets();
        if assets_arrived {
            if self.state.launchpad_open {
                self.redraw_launchpad();
            }
            if self.state.overview_open {
                self.redraw_overview();
            }
            if self.state.cc_open {
                self.redraw_cc();
            }
            if !self.hidden_for_fullscreen {
                self.redraw_dock();
            }
        }

        // 1. Drain MTA Background Telemetry Worker (ZERO UI thread blocking!)
        let mut tb_dirty = false;
        let mut cc_dirty = false;
        let mut ov_tel_dirty = false;
        while let Ok(msg) = self.telemetry.rx.try_recv() {
            match msg {
                TelemetryUpdate::FastMedia { master_vol, master_muted, now_playing } => {
                    if self.state.master_vol != master_vol || self.state.master_muted != master_muted {
                        self.state.master_vol = master_vol;
                        self.state.master_muted = master_muted;
                        tb_dirty = true;
                        cc_dirty = true;
                    }
                    let np_changed = self.state.now_playing.title != now_playing.title
                        || self.state.now_playing.is_playing != now_playing.is_playing
                        || self.state.now_playing.active != now_playing.active
                        || self.state.now_playing.has_timeline != now_playing.has_timeline
                        || (self.state.now_playing.live_position_sec() - now_playing.position_sec).abs() > 1.8;
                    self.state.now_playing = now_playing;
                    if np_changed {
                        tb_dirty = true;
                        cc_dirty = true;
                        self.redraw_widget(WidgetKind::Music);
                    }
                }
                TelemetryUpdate::Network(r) => {
                    self.state.net_status = r.net_label.clone();
                    self.state.radios = r;
                    tb_dirty = true;
                    cc_dirty = true;
                }
                TelemetryUpdate::ControlCenterData { output_devices, mic_devices, app_sessions, tray_apps } => {
                    self.state.output_devices = output_devices;
                    self.state.mic_devices = mic_devices;
                    self.state.app_sessions = app_sessions;
                    self.state.tray_apps = tray_apps;
                    cc_dirty = true;
                }
                TelemetryUpdate::OverviewData { dev_ports, tray_apps } => {
                    self.state.dev_ports = dev_ports;
                    self.state.tray_apps = tray_apps;
                    ov_tel_dirty = true;
                }
            }
        }

        // Check async Dev Hub snapshot (AI Quotas + GitHub/Git Commits)
        if let Some(snap) = self.dev_hub_rx.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.dev_hub_rx = None;
            self.state.dev_hub = snap;
            if self.state.overview_open && self.state.mode == ShellMode::Dev {
                self.redraw_overview();
            }
        }

        // Check one-shot toggle radio result
        let radio_res = self.radio_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(r) = radio_res {
            self.radio_rx = None;
            self.state.net_status = r.net_label.clone();
            self.state.radios = r;
            tb_dirty = true;
            cc_dirty = true;
        }

        // Check background auto-updater (Local Dev hot-update OR GitHub Releases community update!)
        if let Ok(upd) = self.update_rx.try_recv() {
            let should_hot_apply = upd.is_local_dev && self.state.settings.auto_apply_updates && !self.auto_update_triggered;
            self.state.update_available_ver = Some(upd.version.clone());
            self.state.pending_update = Some(upd.clone());
            tb_dirty = true;
            if should_hot_apply {
                self.auto_update_triggered = true;
                updater::trigger_apply_update(&upd);
            }
        }

        if tb_dirty && !self.hidden_for_fullscreen {
            self.redraw_topbar();
        }
        if cc_dirty && self.state.cc_open {
            self.redraw_cc();
        }
        if ov_tel_dirty && self.state.overview_open {
            self.redraw_overview();
        }

        // 2. Check async background File Picker completion
        let picked = self.file_pick_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some((kind, path_opt)) = picked {
            self.file_pick_rx = None;
            if let Some(path) = path_opt {
                let name = std::path::Path::new(&path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("Elemento")
                    .to_string();
                let ts_id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(1);
                match kind {
                    FilePickKind::CustomGame => {
                        self.state.custom_items.custom_games.push(CustomEntry {
                            id: format!("game_{}", ts_id),
                            name,
                            target: path,
                            subtitle: "Manual".into(),
                            category: "Game".into(),
                            cover_path: None,
                        });
                        save_custom_items(&self.state.custom_items);
                        self.state.games = system::scan_all_games(&self.state.custom_items);
                        self.state.mode = ShellMode::Gamer;
                        self.state.overview_tab_pill_x.snap(2.0);
                    }
                    FilePickKind::DevLocal => {
                        self.state.custom_items.dev_ai_items.push(CustomEntry {
                            id: format!("dev_{}", ts_id),
                            name,
                            target: path,
                            subtitle: "App personalizada".into(),
                            category: "DevAI".into(),
                            cover_path: None,
                        });
                        save_custom_items(&self.state.custom_items);
                        self.state.dev_ai_apps = system::build_dev_ai_entries(&self.state.all_apps, &self.state.custom_items);
                        self.state.mode = ShellMode::Dev;
                        self.state.overview_tab_pill_x.snap(1.0);
                    }
                    FilePickKind::CustomWallpaper => {
                        let low = path.to_lowercase();
                        let is_vid = wallpaper::is_video_ext(&low);
                        let prefix = if is_vid {
                            "video"
                        } else if low.ends_with(".html") || low.ends_with(".htm") {
                            "web"
                        } else {
                            "image"
                        };
                        let wp_id = format!("{}:{}", prefix, path);
                        let cover_path = if is_vid {
                            wallpaper::extract_video_thumbnail_isolated(&path)
                        } else {
                            None
                        };
                        if !self.state.custom_items.custom_wallpapers.iter().any(|w| w.target.eq_ignore_ascii_case(&path)) {
                            self.state.custom_items.custom_wallpapers.push(CustomEntry {
                                id: format!("wp_{}", ts_id),
                                name,
                                target: path,
                                subtitle: "Fondo personalizado".into(),
                                category: "Wallpaper".into(),
                                cover_path,
                            });
                            save_custom_items(&self.state.custom_items);
                        }
                        self.state.wallpapers = wallpaper::scan_wallpapers(&self.state.custom_items);
                        self.state.settings.wallpaper_id = wp_id.clone();
                        save_settings(&self.state.settings);
                        self.wallpaper.set_wallpaper(&mut self.gpu, &wp_id);
                        self.state.mode = ShellMode::Studio;
                        self.state.overview_tab_pill_x.snap(4.0);
                        self.wake_anim_timer();
                    }
                }
            }
            self.open_overview(false);
        }

        // 3. Overview Apple Circular Reveal + Sliding Pill + Directional Content Slide
        let mut ov_dirty = false;
        if self.state.overview_anim.is_moving() {
            let moving = self.state.overview_anim.step(dt_ms);
            if !moving && !self.state.overview_open && self.state.overview_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_overview, SW_HIDE); }
                self.surf_overview.trim_hidden(&self.gpu);
                self.gpu.purge_heavy_image_cache();
                self.gpu.trim_gpu();
                win32_shell::trim_own_working_set();
            } else {
                ov_dirty = true;
            }
        }
        if self.state.overview_tab_pill_x.step(dt_ms) {
            self.redraw_topbar();
            if self.state.overview_open { ov_dirty = true; }
        }
        if self.state.tab_content_anim.is_moving() {
            self.state.tab_content_anim.step(dt_ms);
            if self.state.overview_open { ov_dirty = true; }
        }
        // Smooth Apple inertial scrolling for Studio Wallpaper Gallery!
        if self.state.overview_open && self.state.mode == ShellMode::Studio {
            let sdiff = self.state.wallpaper_scroll_target - self.state.wallpaper_scroll;
            if sdiff.abs() > 0.35 {
                let k = 1.0 - (-18.0 * dt_sec).exp();
                self.state.wallpaper_scroll += sdiff * k;
                ov_dirty = true;
            } else if sdiff != 0.0 {
                self.state.wallpaper_scroll = self.state.wallpaper_scroll_target;
                ov_dirty = true;
            }
        }
        if ov_dirty {
            self.redraw_overview();
        }

        // 4. Control Center Apple Circular Reveal
        if self.state.cc_anim.is_moving() {
            let moving = self.state.cc_anim.step(dt_ms);
            if !moving && !self.state.cc_open && self.state.cc_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_cc, SW_HIDE); }
                self.surf_cc.trim_hidden(&self.gpu);
                self.gpu.trim_gpu();
            } else {
                self.redraw_cc();
            }
        }

        // 5. Power Menu, Calendar & Clipboard Popovers + Volume OSD
        if self.state.power_anim.is_moving() {
            let moving = self.state.power_anim.step(dt_ms);
            if !moving && !self.state.power_open && self.state.power_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_power, SW_HIDE); }
                self.surf_power.trim_hidden(&self.gpu);
            } else {
                self.redraw_power();
            }
        }
        if self.state.calendar_anim.is_moving() {
            let moving = self.state.calendar_anim.step(dt_ms);
            if !moving && !self.state.calendar_open && self.state.calendar_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_calendar, SW_HIDE); }
                self.surf_calendar.trim_hidden(&self.gpu);
            } else {
                self.redraw_calendar();
            }
        }
        if self.state.clipboard_anim.is_moving() {
            let moving = self.state.clipboard_anim.step(dt_ms);
            if !moving && !self.state.clipboard_open && self.state.clipboard_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_clipboard, SW_HIDE); }
                self.surf_clipboard.trim_hidden(&self.gpu);
            } else {
                self.redraw_clipboard();
            }
        }
        if let Some(until) = self.state.osd_until {
            if Instant::now() >= until {
                self.state.osd_until = None;
                unsafe { let _ = ShowWindow(self.hwnd_osd, SW_HIDE); }
                self.surf_osd.trim_hidden(&self.gpu);
            }
        }

        // 6. Launchpad Apple Expanding Circle Reveal + Butter-Smooth Momentum Scrolling!
        let mut lp_dirty = false;
        if self.state.launchpad_anim.is_moving() {
            let moving = self.state.launchpad_anim.step(dt_ms);
            if !moving && !self.state.launchpad_open && self.state.launchpad_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_launchpad, SW_HIDE); }
                self.surf_launchpad.trim_hidden(&self.gpu);
                self.gpu.trim_gpu();
                win32_shell::trim_own_working_set();
            } else {
                lp_dirty = true;
            }
        }
        if self.state.launchpad_open {
            let sdiff = self.state.launchpad_scroll_target - self.state.launchpad_scroll;
            if sdiff.abs() > 0.35 {
                let k = 1.0 - (-18.0 * dt_sec).exp();
                self.state.launchpad_scroll += sdiff * k;
                lp_dirty = true;
            } else if sdiff != 0.0 {
                self.state.launchpad_scroll = self.state.launchpad_scroll_target;
                lp_dirty = true;
            }
        }
        if lp_dirty {
            self.redraw_launchpad();
        }

        // 7. Interruptible Dock Springs + True SW_HIDE to Zero-Interference iPhone/Android Notch!
        let mut dock_dirty = false;
        if self.state.dock_reveal.is_moving() {
            self.state.dock_reveal.step(dt_sec);
            dock_dirty = true;
        }
        for s in &mut self.state.dock_item_scales {
            if s.is_moving() {
                s.step(dt_sec);
                dock_dirty = true;
            }
        }
        if self.state.dock_hidden && self.state.dock_reveal.value <= 0.03 {
            // Completely hide hwnd_dock so ZERO transparent pixels block clicks; show sleek Notch!
            self.hide_dock_show_notch();
        } else if dock_dirty && self.dock_hwnd_visible {
            self.redraw_dock();
        }

        // 8. Animated Desktop Music Visualizer (throttled to 24 FPS; skipped in performance_mode or covered desktop!)
        let music_enabled = self.state.settings.show_desktop_widgets
            && !self.state.settings.performance_mode
            && self.state.widgets.get(WidgetKind::Music).map(|w| w.enabled).unwrap_or(false);
        let fullscreen_overlay = self.state.launchpad_open || self.state.overview_open;
        let mut music_animating = false;
        if music_enabled && !self.desktop_covered && !self.hidden_for_fullscreen && !fullscreen_overlay {
            let target_energy = if self.state.now_playing.is_playing { 1.0 } else { 0.0 };
            let diff = target_energy - self.state.music_energy;
            if self.state.now_playing.is_playing || diff.abs() > 0.015 {
                music_animating = true;
                self.state.music_energy += diff * (dt_sec * 6.0).min(1.0);
                self.state.music_wave_phase += dt_sec * 1.8;
                if self.state.music_wave_phase > 1000.0 {
                    self.state.music_wave_phase -= 1000.0;
                }
                self.music_anim_accum += dt_sec;
                if self.music_anim_accum >= 0.083 {
                    self.music_anim_accum = 0.0;
                    self.redraw_widget(WidgetKind::Music);
                }
            }
        }

        // 9. macOS Genie Effect (36-Strip Curved Suction / Un-Suction!)
        let genie_running = self.genie.tick(&mut self.gpu);

        // 10. Integrated Wallpaper Engine (Video runs isolated in AuraWallpaper.exe -> 0 Shell overhead!)
        if !self.state.launchpad_open {
            self.wallpaper.tick(&mut self.gpu, dt_sec);
        }

        // 11. Dynamic Adaptive Frame Pacing (cuts idle CPU wakeups by >95%!)
        let ui_animating = genie_running
            || self.state.overview_anim.is_moving()
            || self.state.overview_tab_pill_x.is_moving()
            || self.state.tab_content_anim.is_moving()
            || self.state.cc_anim.is_moving()
            || self.state.power_anim.is_moving()
            || self.state.calendar_anim.is_moving()
            || self.state.clipboard_anim.is_moving()
            || self.state.osd_until.is_some()
            || self.state.launchpad_anim.is_moving()
            || self.state.dock_reveal.is_moving()
            || self.state.dock_item_scales.iter().any(|s| s.is_moving())
            || self.state.dock_mouse_axis.is_some()
            || (self.state.launchpad_open && (self.state.launchpad_scroll_target - self.state.launchpad_scroll).abs() > 0.35)
            || (self.state.overview_open && self.state.mode == ShellMode::Studio && (self.state.wallpaper_scroll_target - self.state.wallpaper_scroll).abs() > 0.35);

        let desired_ms = if genie_running {
            8 // Silky 120 FPS pacing during macOS Genie suction/un-suction!
        } else if ui_animating {
            16
        } else if !self.state.launchpad_open && self.wallpaper.needs_fast_tick() {
            48
        } else if music_animating || (self.state.cc_open && self.state.now_playing.is_playing) {
            83 // Ultra-efficient 12 FPS wavy visualizer (half the GPU work, zero CreateLayer allocs!)
        } else {
            220
        };
        if self.state.cc_open && self.state.now_playing.is_playing && !music_animating {
            self.state.music_wave_phase = (self.state.music_wave_phase + dt_sec * 1.8) % 1000.0;
            self.music_anim_accum += dt_sec;
            if self.music_anim_accum >= 0.083 {
                self.music_anim_accum = 0.0;
                self.redraw_cc();
            }
        }
        if desired_ms != self.anim_interval_ms {
            self.anim_interval_ms = desired_ms;
            unsafe {
                SetTimer(self.hwnd_topbar, TIMER_ANIM, desired_ms, None);
            }
        }
    }

    fn wake_anim_timer(&mut self) {
        if self.anim_interval_ms != 16 {
            self.anim_interval_ms = 16;
            unsafe {
                SetTimer(self.hwnd_topbar, TIMER_ANIM, 16, None);
            }
        }
    }

    fn spawn_weather_refresh(&mut self) {
        let city = self.state.settings.weather_city.clone();
        let (w_tx, w_rx) = mpsc::channel();
        self.weather_rx = Some(w_rx);
        std::thread::spawn(move || {
            let _ = w_tx.send(system::fetch_weather_for_city(&city));
        });
    }

    fn tick_fast_120ms(&mut self) {
        self.fast_tick_count = self.fast_tick_count.wrapping_add(1);
        let any_overlay = self.state.overview_open
            || self.state.cc_open
            || self.state.power_open
            || self.state.calendar_open
            || self.state.clipboard_open
            || self.state.launchpad_open;

        // Throttle heavy Win32 EnumWindows scans to every 360ms (-67% CPU overhead on modest PCs!)
        if self.fast_tick_count % 3 == 1 {
            if self.state.settings.hide_windows_taskbar {
                win32_shell::enforce_taskbar_suppressed();
            }

            let is_fs = !any_overlay && win32_shell::is_fullscreen_app_active();
            hotkey_hook::set_fullscreen_bypass(is_fs);

            let covered = !any_overlay && win32_shell::is_any_maximized_window_visible();
            self.desktop_covered = covered;
            if self.state.settings.wallpaper_auto_pause {
                self.wallpaper.set_paused(covered || is_fs);
            }

            if is_fs && !self.hidden_for_fullscreen {
                self.hidden_for_fullscreen = true;
                self.dock_preview.hide();
                unsafe {
                    let _ = ShowWindow(self.hwnd_topbar, SW_HIDE);
                    let _ = ShowWindow(self.hwnd_dock, SW_HIDE);
                    let _ = ShowWindow(self.hwnd_notch, SW_HIDE);
                }
                self.dock_hwnd_visible = false;
                self.notch_visible = false;
                return;
            } else if !is_fs && self.hidden_for_fullscreen {
                self.hidden_for_fullscreen = false;
                unsafe {
                    let _ = ShowWindow(self.hwnd_topbar, SW_SHOWNOACTIVATE);
                }
                win32_shell::ensure_topmost(self.hwnd_topbar);
                if self.state.dock_hidden {
                    self.hide_dock_show_notch();
                } else {
                    self.show_dock_hide_notch();
                }
            }
        }
        if self.hidden_for_fullscreen {
            return;
        }

        // Fast foreground sync so Smart Desktop Dock reacts instantaneously!
        let cur_fg = win32_shell::get_active_user_hwnd();
        if cur_fg != self.state.active_user_hwnd {
            self.state.active_user_hwnd = cur_fg;
            self.state.active_window_label = win32_shell::get_active_window_label();
            self.state.open_windows = win32_shell::get_open_user_windows();
            self.sync_dock_tight_bounds();
            self.redraw_topbar();
            if self.dock_hwnd_visible {
                self.redraw_dock();
            }
        }

        // SMART DESKTOP-AWARE DOCK AUTO-HIDE:
        // • On the Desktop (no visible non-minimized apps OR desktop focused & not covered) -> ALWAYS VISIBLE!
        // • When an application/program is open & focused -> Auto-hides smoothly into the edge notch!
        let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
        let (cx, cy) = win32_shell::cursor_pos();
        let (nx, ny, nw, nh) = ui_dock::notch_geometry(self.screen_w, self.screen_h, is_left);
        let in_notch = cx >= nx && cx <= nx + nw && cy >= ny && cy <= ny + nh;
        let (dx, dy, dw, dh) = self.dock_geo;
        let in_dock_tight = !self.state.dock_hidden
            && self.dock_hwnd_visible
            && cx >= dx
            && cx <= dx + dw
            && cy >= dy
            && cy <= dy + dh;
        let in_preview = self.dock_preview.contains_point(cx, cy);

        // Hide live preview popup if cursor left BOTH the Dock and the preview card
        if self.dock_preview.visible && !in_dock_tight && !in_preview && !self.mouse_in_dock {
            self.dock_preview.hide();
        }

        let has_visible_app = self.state.open_windows.iter().any(|w| !w.is_minimized);
        let on_desktop = !has_visible_app || (self.state.active_user_hwnd == 0 && !self.desktop_covered);

        let want_hidden = if !self.state.settings.dock_auto_hide || any_overlay || on_desktop {
            false
        } else if in_notch || in_dock_tight || in_preview || self.mouse_in_dock || self.mouse_in_notch {
            self.last_dock_hover_exit = Instant::now();
            false
        } else if !self.state.dock_hidden && self.last_dock_hover_exit.elapsed().as_millis() < 340 {
            false
        } else {
            true
        };

        if want_hidden != self.state.dock_hidden {
            self.state.dock_hidden = want_hidden;
            self.wake_anim_timer();
            if !want_hidden {
                self.show_dock_hide_notch();
                self.state.dock_reveal.set_target(1.0);
            } else {
                self.dock_preview.hide();
                self.state.dock_reveal.set_target(0.0);
            }
        } else if want_hidden && self.dock_hwnd_visible && self.state.dock_reveal.value <= 0.03 {
            self.hide_dock_show_notch();
        }
    }

    fn tick_slow_1s(&mut self) {
        let mut weather_changed = false;
        if let Some(rx) = &self.weather_rx {
            if let Ok(w) = rx.try_recv() {
                self.state.weather = w;
                self.weather_rx = None;
                weather_changed = true;
                self.redraw_widget(WidgetKind::Clock);
                self.redraw_widget(WidgetKind::Weather);
            }
        }

        self.state.cpu_pct = self.cpu.sample();
        self.state.cpu_history = self.cpu.history.clone();
        let (ru, rt_gb, rp) = system::get_ram_stats();
        self.state.ram_used_gb = ru;
        self.state.ram_total_gb = rt_gb;
        self.state.ram_pct = rp;
        self.state.active_window_label = win32_shell::get_active_window_label();
        self.state.active_user_hwnd = win32_shell::get_active_user_hwnd();
        self.state.keyboard_lang = win32_shell::get_keyboard_layout_short();
        self.slow_tick_count = self.slow_tick_count.wrapping_add(1);
        if self.slow_tick_count == 4 || self.slow_tick_count % 120 == 0 {
            win32_shell::trim_own_working_set();
        }
        if self.slow_tick_count % 15 == 1 {
            self.state.battery_status = win32_shell::get_battery_status();
            self.state.uptime_label = win32_shell::get_uptime_short();
        }

        // Zero-overhead Clipboard History tracking (`GetClipboardSequenceNumber` < 0.001ms!)
        self.poll_clipboard_history();

        // Live Pomodoro Focus Timer 1s countdown
        let mut pomo_ticked = false;
        if self.state.pomodoro_active {
            if self.state.pomodoro_secs_left > 0 {
                self.state.pomodoro_secs_left -= 1;
                pomo_ticked = true;
            } else {
                self.state.pomodoro_is_break = !self.state.pomodoro_is_break;
                self.state.pomodoro_secs_left = if self.state.pomodoro_is_break { 5 * 60 } else { 25 * 60 };
                pomo_ticked = true;
            }
        }
        if self.state.calendar_open {
            self.redraw_calendar();
        }
        if pomo_ticked && !self.hidden_for_fullscreen {
            self.redraw_topbar();
        }

        // Periodic automatic weather refresh every 15 minutes (900 ticks)
        if self.slow_tick_count > 0 && self.slow_tick_count % 900 == 0 && self.weather_rx.is_none() {
            self.spawn_weather_refresh();
        }

        let new_wins = win32_shell::get_open_user_windows();
        let dock_changed = new_wins.len() != self.state.open_windows.len()
            || new_wins.iter().zip(self.state.open_windows.iter()).any(|(a, b)| a.hwnd != b.hwnd || a.is_active != b.is_active);
        self.state.open_windows = new_wins;

        // Cached DWM styling (only touches newly created or active-changed standard Win32 windows!)
        if self.state.settings.style_windows_apps {
            window_styler::apply_aura_style_to_windows(&self.state.open_windows);
        }

        if self.state.dock_item_scales.len() < self.state.open_windows.len() + 6 {
            self.state.dock_item_scales.resize(self.state.open_windows.len() + 6, Spring::dock_hover(1.0));
        }

        let cur_min = unsafe {
            let st = windows::Win32::System::SystemInformation::GetLocalTime();
            (st.wHour as u32) * 60 + (st.wMinute as u32)
        };
        let min_changed = cur_min != self.last_clock_min;
        if min_changed {
            self.last_clock_min = cur_min;
        }

        if !self.hidden_for_fullscreen {
            let sig = format!(
                "{}|{}|{}|{:.1}|{}|{}|{}|{}|{}|{}",
                cur_min,
                self.state.active_window_label,
                self.state.cpu_pct.round() as i32,
                self.state.ram_used_gb,
                self.state.master_vol,
                self.state.master_muted,
                self.state.keyboard_lang,
                self.state.weather.temp_c,
                self.state.weather.city,
                self.state.now_playing.is_playing
            );
            if sig != self.last_topbar_sig || weather_changed {
                self.last_topbar_sig = sig;
                self.redraw_topbar();
            }
            if dock_changed {
                self.sync_dock_tight_bounds();
                if self.dock_hwnd_visible {
                    self.redraw_dock();
                }
            }
        }
        if self.state.overview_open && self.state.mode == ShellMode::PC && !self.state.overview_settings_view {
            self.redraw_overview();
        }
        // Smart dirty-checked Desktop Widgets (only redraw what ACTUALLY changed -> saves massive GPU on modest PCs!)
        if self.state.settings.show_desktop_widgets && !self.desktop_covered {
            if min_changed || weather_changed {
                self.redraw_widget(WidgetKind::Clock);
            }
            if weather_changed {
                self.redraw_widget(WidgetKind::Weather);
            }
            if !self.state.settings.performance_mode || self.slow_tick_count % 2 == 0 {
                self.redraw_widget(WidgetKind::Monitor);
            }
            // Keep Music Widget M:SS timeline ticking smoothly once/sec even in performance_mode!
            if self.state.now_playing.is_playing && self.state.now_playing.has_timeline {
                self.redraw_widget(WidgetKind::Music);
            }
            if self.state.active_user_hwnd != self.last_fg_hwnd_seen {
                self.last_fg_hwnd_seen = self.state.active_user_hwnd;
                for w in &self.widget_wins {
                    win32_shell::send_to_desktop_bottom(w.hwnd);
                }
            }
        }
    }

    fn apply_slider_drag(&mut self, px: f32) {
        let Some(slider) = self.state.active_slider.clone() else { return };
        match slider {
            SliderDragKind::MasterVolume { x, w } => {
                let pct = (((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as i32;
                self.execute(Action::SetMasterVolume(pct));
            }
            SliderDragKind::Brightness { x, w } => {
                let pct = (35.0 + ((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 65.0).round() as i32;
                self.execute(Action::SetBrightness(pct));
            }
            SliderDragKind::AppVolume { x, w, pids } => {
                let pct = (((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as i32;
                self.execute(Action::SetAppVolume(pids, pct));
            }
            SliderDragKind::GlassOpacity { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let val = 0.50 + frac * 0.46;
                self.execute(Action::SetGlassOpacity(val));
            }
            SliderDragKind::MediaSeek { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                self.execute(Action::MediaSeek(frac));
            }
        }
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_DESTROY {
        return LRESULT(0);
    }
    if msg == WM_CLOSE {
        with_rt(|rt| rt.close_all_overlays());
        return LRESULT(0);
    }
    if msg == WM_MOUSEACTIVATE {
        let is_chrome = with_rt(|rt| {
            matches!(
                rt.kind_of(hwnd),
                Some(WinKind::TopBar)
                    | Some(WinKind::Dock)
                    | Some(WinKind::DockNotch)
                    | Some(WinKind::VolumeOsd)
                    | Some(WinKind::Widget(_))
            )
        })
        .unwrap_or(false);
        if is_chrome {
            return LRESULT(MA_NOACTIVATE as isize);
        }
    }

    let handled = with_rt(|rt| {
        if msg == rt.wm_activate_existing && rt.wm_activate_existing != 0 {
            rt.open_overview(false);
            return true;
        }
        if msg == rt.wm_settings_changed && rt.wm_settings_changed != 0 {
            let prev_wp = rt.state.settings.wallpaper_id.clone();
            let prev_hide = rt.state.settings.hide_windows_taskbar;
            let prev_city = rt.state.settings.weather_city.clone();
            rt.state.settings = load_settings();
            rt.state.custom_items = load_custom_items();
            rt.state.wallpapers = wallpaper::scan_wallpapers(&rt.state.custom_items);
            rt.state.widgets = load_widgets(rt.screen_w, rt.screen_h);
            rt.wallpaper.set_performance_mode(rt.state.settings.performance_mode);
            hotkey_hook::set_enabled(rt.state.settings.intercept_win_hotkeys);
            if rt.state.settings.hide_windows_taskbar != prev_hide {
                win32_shell::set_taskbar_visibility(!rt.state.settings.hide_windows_taskbar);
                win32_shell::apply_full_work_area(TOPBAR_HEIGHT as i32, rt.state.settings.hide_windows_taskbar);
            }
            if rt.state.settings.wallpaper_id != prev_wp {
                let next_wp = rt.state.settings.wallpaper_id.clone();
                rt.wallpaper.set_wallpaper(&mut rt.gpu, &next_wp);
            }
            rt.sync_material_you_palette();
            win32_shell::apply_display_gamma(rt.state.settings.brightness_pct, rt.state.settings.night_light);
            win32_shell::set_keep_awake(rt.state.settings.keep_awake);
            if rt.state.settings.weather_city != prev_city {
                rt.spawn_weather_refresh();
            }
            rt.sync_dock_tight_bounds();
            rt.sync_widget_windows_visibility();
            rt.redraw_topbar();
            rt.redraw_dock();
            rt.redraw_all_widgets();
            rt.wake_anim_timer();
            return true;
        }
        if msg == rt.wm_aura_shortcut && rt.wm_aura_shortcut != 0 {
            rt.wake_anim_timer();
            match wparam.0 {
                hotkey_hook::SHORTCUT_LAUNCHPAD => rt.execute(Action::ToggleLaunchpad),
                hotkey_hook::SHORTCUT_TASKS => rt.execute(Action::LaunchAuraTasks),
                hotkey_hook::SHORTCUT_FILES => rt.execute(Action::LaunchAuraFiles(None)),
                hotkey_hook::SHORTCUT_SETTINGS => rt.execute(Action::LaunchAuraSettings(None)),
                hotkey_hook::SHORTCUT_OVERVIEW => {
                    if rt.state.overview_open {
                        rt.close_overview();
                    } else {
                        rt.open_overview_ex(false, true);
                    }
                }
                hotkey_hook::SHORTCUT_CLIPBOARD => rt.execute(Action::ToggleClipboard),
                hotkey_hook::SHORTCUT_SCREENSHOT => rt.execute(Action::LaunchAuraShot),
                _ => {}
            }
            return true;
        }
        if msg == rt.wm_dock_preview_click && rt.wm_dock_preview_click != 0 {
            let target_hwnd = lparam.0;
            rt.dock_preview.hide();
            if wparam.0 == 1 {
                window_styler::close_active_window(target_hwnd);
            } else {
                rt.execute(Action::FocusWindow(target_hwnd));
            }
            return true;
        }
        if msg == rt.wm_genie_minimize && rt.wm_genie_minimize != 0 {
            let target_hwnd = wparam.0 as isize;
            if target_hwnd != 0 {
                if !rt.state.settings.performance_mode {
                    let is_left = !rt.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                    let (gx, gy) = ui_dock::dock_icon_screen_center(&rt.state, rt.dock_geo, is_left, target_hwnd);
                    rt.genie.start_minimize(&rt.gpu, target_hwnd, gx, gy, is_left);
                    rt.wake_anim_timer();
                } else {
                    window_styler::minimize_active_window(target_hwnd);
                }
            }
            return true;
        }

        let Some(kind) = rt.kind_of(hwnd) else { return false };

        match msg {
            WM_TIMER => {
                match wparam.0 {
                    TIMER_ANIM => rt.tick_anim(),
                    TIMER_FAST => rt.tick_fast_120ms(),
                    TIMER_SLOW => rt.tick_slow_1s(),
                    _ => {}
                }
                true
            }
            WM_HOTKEY => {
                rt.wake_anim_timer();
                match wparam.0 as i32 {
                    HK_OVERVIEW => {
                        if rt.state.overview_open {
                            rt.close_overview();
                        } else {
                            rt.open_overview_ex(false, true);
                        }
                    }
                    HK_FILES => rt.execute(Action::LaunchAuraFiles(None)),
                    HK_RESTORE => rt.execute(Action::RestoreWindowsAndExit),
                    HK_LOCK => rt.execute(Action::LaunchAuraLock),
                    HK_TASKS => rt.execute(Action::LaunchAuraTasks),
                    _ => {}
                }
                true
            }
            WM_ACTIVATE => {
                let active_state = (wparam.0 & 0xFFFF) as u32;
                if active_state == WA_INACTIVE {
                    match kind {
                        WinKind::ControlCenter => rt.close_cc(),
                        WinKind::PowerMenu => rt.close_power(),
                        WinKind::Calendar => rt.close_calendar(),
                        WinKind::Clipboard => rt.close_clipboard(),
                        WinKind::Launchpad => rt.close_launchpad(),
                        WinKind::Overview => {
                            if !rt.state.add_modal.open {
                                rt.close_overview();
                            }
                        }
                        _ => {}
                    }
                }
                true
            }
            WM_MOUSEMOVE => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;

                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                let _ = TrackMouseEvent(&mut tme);

                // Live Magnetic Desktop Widget Dragging & Grid Docking!
                if let Some((dwk, off_x, off_y)) = rt.dragging_widget {
                    let (cx, cy) = win32_shell::cursor_pos();
                    let (nx, ny, snapped) = rt.compute_magnetic_widget_pos(dwk, cx - off_x, cy - off_y);
                    let prev_snap = rt.snapped_widget;
                    rt.snapped_widget = if snapped { Some(dwk) } else { None };
                    if let Some(cfg) = rt.state.widgets.get_mut(dwk) {
                        cfg.x = nx;
                        cfg.y = ny;
                    }
                    if let Some(ww) = rt.widget_wins.iter().find(|w| w.kind == dwk) {
                        let _ = SetWindowPos(ww.hwnd, HWND_BOTTOM, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                    if prev_snap != rt.snapped_widget {
                        rt.redraw_widget(dwk);
                    }
                    return true;
                }

                if kind == WinKind::DockNotch {
                    // Instant zero-latency reveal the moment cursor touches the sleek iPhone/Android muesquita!
                    rt.mouse_in_notch = true;
                    rt.last_dock_hover_exit = Instant::now();
                    rt.wake_anim_timer();
                    if rt.state.dock_hidden {
                        rt.state.dock_hidden = false;
                        rt.show_dock_hide_notch();
                        rt.state.dock_reveal.set_target(1.0);
                    }
                    return true;
                }
                if kind == WinKind::Dock {
                    rt.mouse_in_dock = true;
                    rt.last_dock_hover_exit = Instant::now();
                    let is_left = !rt.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                    let axis = if is_left { py } else { px };
                    rt.state.dock_mouse_axis = Some(axis);
                    rt.wake_anim_timer();
                }
                if let WinKind::Widget(wk) = kind {
                    let mut changed = false;
                    if let Some(w) = rt.widget_wins.iter_mut().find(|w| w.kind == wk) {
                        if !w.hovered {
                            w.hovered = true;
                            changed = true;
                        }
                    }
                    if changed {
                        rt.redraw_widget(wk);
                    }
                }
                if rt.state.active_slider.is_some() {
                    rt.apply_slider_drag(px);
                    return true;
                }

                let new_hov_hit = rt
                    .hits_for(kind)
                    .iter()
                    .rev()
                    .find(|h| h.rect.contains(px, py))
                    .cloned();
                let new_hov = new_hov_hit.as_ref().map(|h| h.id.clone());

                // Live DWM Window Preview Popup when hovering any running app icon in AuraDock!
                if kind == WinKind::Dock {
                    if let Some(hit) = &new_hov_hit {
                        if let Some((target_hw, title)) = ui_dock::dock_preview_target_for_hit(&rt.state, &hit.id) {
                            let is_left = !rt.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
                            let (dx, dy, _dw, _dh) = rt.dock_geo;
                            let (prx, pry) = if is_left {
                                (
                                    dx + 90,
                                    dy + (hit.rect.y + hit.rect.h * 0.5) as i32 - dock_preview::PREVIEW_H / 2,
                                )
                            } else {
                                (
                                    dx + (hit.rect.x + hit.rect.w * 0.5) as i32 - dock_preview::PREVIEW_W / 2,
                                    dy - dock_preview::PREVIEW_H - 6,
                                )
                            };
                            rt.dock_preview.show_for_window(target_hw, &title, prx, pry, rt.screen_w, rt.screen_h);
                        } else {
                            rt.dock_preview.hide();
                        }
                    } else {
                        rt.dock_preview.hide();
                    }
                    rt.state.hovered_id = new_hov;
                    rt.redraw_dock();
                    return true;
                }

                if new_hov != rt.state.hovered_id {
                    rt.state.hovered_id = new_hov;
                    rt.redraw(kind);
                }
                true
            }
            WM_MOUSELEAVE => {
                if kind == WinKind::DockNotch {
                    rt.mouse_in_notch = false;
                    rt.redraw_notch();
                    return true;
                }
                if kind == WinKind::Dock {
                    rt.mouse_in_dock = false;
                    rt.state.dock_mouse_axis = None;
                    rt.last_dock_hover_exit = Instant::now();
                    let (cx, cy) = win32_shell::cursor_pos();
                    if !rt.dock_preview.contains_point(cx, cy) {
                        rt.dock_preview.hide();
                    }
                    rt.state.hovered_id = None;
                    rt.redraw_dock();
                    return true;
                }
                if let WinKind::Widget(wk) = kind {
                    if let Some(w) = rt.widget_wins.iter_mut().find(|w| w.kind == wk) {
                        w.hovered = false;
                    }
                    rt.state.hovered_id = None;
                    rt.redraw_widget(wk);
                    return true;
                }
                if rt.state.hovered_id.is_some() {
                    rt.state.hovered_id = None;
                    rt.redraw(kind);
                }
                true
            }
            WM_LBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                let hit = rt.hits_for(kind).iter().rev().find(|h| h.rect.contains(px, py)).cloned();
                if let Some(h) = hit {
                    rt.wake_anim_timer();
                    rt.state.pressed_id = Some(h.id.clone());
                    if let Some(sl) = h.slider {
                        rt.state.active_slider = Some(sl);
                        SetCapture(hwnd);
                        rt.apply_slider_drag(px);
                    } else {
                        rt.redraw(kind);
                        rt.execute(h.action);
                    }
                } else if let WinKind::Widget(wk) = kind {
                    // Drag anywhere on desktop widget to reposition it smoothly!
                    let (cx, cy) = win32_shell::cursor_pos();
                    if let Some(cfg) = rt.state.widgets.get(wk) {
                        rt.dragging_widget = Some((wk, cx - cfg.x, cy - cfg.y));
                        SetCapture(hwnd);
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if let Some((dwk, _, _)) = rt.dragging_widget.take() {
                    let had_snap = rt.snapped_widget.take().is_some();
                    let _ = ReleaseCapture();
                    rt.resolve_widget_overlap_on_drop(dwk);
                    save_widgets(&rt.state.widgets);
                    if had_snap {
                        rt.redraw_widget(dwk);
                    }
                }
                if rt.state.active_slider.is_some() {
                    rt.state.active_slider = None;
                    let _ = ReleaseCapture();
                }
                if rt.state.pressed_id.is_some() {
                    rt.state.pressed_id = None;
                    rt.redraw(kind);
                }
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                let hit = rt.hits_for(kind).iter().rev().find(|h| h.rect.contains(px, py)).cloned();
                if let Some(h) = hit {
                    if let Some(ra) = h.right_action {
                        rt.execute(ra);
                    }
                }
                true
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) as i16) as f32;
                let step = if delta > 0.0 { -52.0 } else { 52.0 };
                match kind {
                    WinKind::TopBar => {
                        let next = (rt.state.master_vol + if delta > 0.0 { 5 } else { -5 }).clamp(0, 100);
                        rt.execute(Action::SetMasterVolume(next));
                    }
                    WinKind::Dock => {
                        rt.state.dock_scroll = (rt.state.dock_scroll + step).max(0.0);
                        rt.redraw_dock();
                    }
                    WinKind::ControlCenter => {
                        let max_s = (rt.cc_content_h - (rt.surf_cc.height as f32 - 34.0)).max(0.0);
                        rt.state.cc_scroll = (rt.state.cc_scroll + step).clamp(0.0, max_s);
                        rt.redraw_cc();
                    }
                    WinKind::Launchpad => {
                        rt.wake_anim_timer();
                        let max_s = (rt.lp_content_h - (rt.surf_launchpad.height as f32 - 180.0)).max(0.0);
                        rt.state.launchpad_scroll_target = (rt.state.launchpad_scroll_target + step * 2.2).clamp(0.0, max_s);
                    }
                    WinKind::Overview => {
                        if rt.state.mode == ShellMode::Studio {
                            rt.wake_anim_timer();
                            let rows = ((rt.state.wallpapers.len() + 2) / 3) as f32;
                            let max_s = (rows * 164.0 - 440.0).max(0.0);
                            rt.state.wallpaper_scroll_target = (rt.state.wallpaper_scroll_target + step * 1.9).clamp(0.0, max_s);
                        }
                    }
                    _ => {}
                }
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                let ov_active = kind == WinKind::Overview || rt.state.overview_open;
                let lp_active = kind == WinKind::Launchpad || rt.state.launchpad_open;
                let cc_active = kind == WinKind::ControlCenter || rt.state.cc_open;
                if cc_active && rt.state.todo_input_active {
                    if vk == VK_ESCAPE.0 {
                        rt.state.todo_input_active = false;
                        rt.redraw_cc();
                    } else if vk == VK_RETURN.0 {
                        rt.execute(Action::SubmitTodoInput);
                    } else if vk == VK_BACK.0 {
                        rt.state.todo_input_text.pop();
                        rt.redraw_cc();
                    }
                    return true;
                }
                if ov_active && rt.state.gh_input_active {
                    if vk == VK_ESCAPE.0 {
                        rt.state.gh_input_active = false;
                        rt.redraw_overview();
                    } else if vk == VK_RETURN.0 {
                        rt.execute(Action::SubmitGithubUserEdit);
                    } else if vk == VK_BACK.0 {
                        rt.state.gh_input_text.pop();
                        rt.redraw_overview();
                    }
                    return true;
                }
                if vk == VK_ESCAPE.0 {
                    if rt.state.add_modal.open {
                        rt.execute(Action::CancelModal);
                    } else if ov_active && !rt.state.search_query.is_empty() {
                        rt.state.search_query.clear();
                        rt.redraw_overview();
                    } else if lp_active && !rt.state.launchpad_query.is_empty() {
                        rt.state.launchpad_query.clear();
                        rt.redraw_launchpad();
                    } else {
                        rt.close_all_overlays();
                    }
                    return true;
                }
                if ov_active && rt.state.add_modal.open {
                    if vk == VK_TAB.0 {
                        rt.state.add_modal.active_field = 1 - rt.state.add_modal.active_field;
                        rt.redraw_overview();
                    } else if vk == VK_RETURN.0 {
                        rt.execute(Action::SubmitAddDevWebModal);
                    } else if vk == VK_BACK.0 {
                        let target = if rt.state.add_modal.active_field == 0 {
                            &mut rt.state.add_modal.name
                        } else {
                            &mut rt.state.add_modal.url
                        };
                        target.pop();
                        rt.redraw_overview();
                    }
                    return true;
                }
                if ov_active {
                    if vk == VK_BACK.0 {
                        rt.state.search_query.pop();
                        rt.state.search_selected = 0;
                        rt.redraw_overview();
                    } else if vk == VK_DOWN.0 {
                        rt.state.search_selected = (rt.state.search_selected + 1).min(7);
                        rt.redraw_overview();
                    } else if vk == VK_UP.0 {
                        rt.state.search_selected = rt.state.search_selected.saturating_sub(1);
                        rt.redraw_overview();
                    } else if vk == VK_RETURN.0 && !rt.state.search_query.trim().is_empty() {
                        let sel_id = format!("srch_{}", rt.state.search_selected);
                        if let Some(h) = rt.hits_overview.iter().find(|x| x.id == sel_id).cloned() {
                            rt.execute(h.action);
                        }
                    }
                } else if lp_active {
                    if vk == VK_BACK.0 {
                        rt.state.launchpad_query.pop();
                        rt.state.launchpad_scroll = 0.0;
                        rt.state.launchpad_scroll_target = 0.0;
                        rt.redraw_launchpad();
                    } else if vk == VK_RETURN.0 && !rt.state.launchpad_query.trim().is_empty() {
                        if let Some(h) = rt.hits_launchpad.iter().find(|x| x.id == "lp_app_0").cloned() {
                            rt.execute(h.action);
                        }
                    }
                }
                true
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam.0 as u32) {
                    if !ch.is_control() {
                        let ov_active = kind == WinKind::Overview || rt.state.overview_open;
                        let lp_active = kind == WinKind::Launchpad || rt.state.launchpad_open;
                        let cc_active = kind == WinKind::ControlCenter || rt.state.cc_open;
                        if cc_active && rt.state.todo_input_active {
                            if rt.state.todo_input_text.len() < 72 {
                                rt.state.todo_input_text.push(ch);
                                rt.redraw_cc();
                            }
                        } else if ov_active && rt.state.gh_input_active {
                            if rt.state.gh_input_text.len() < 40 && !ch.is_whitespace() {
                                rt.state.gh_input_text.push(ch);
                                rt.redraw_overview();
                            }
                        } else if ov_active && rt.state.add_modal.open {
                            if rt.state.add_modal.active_field == 0 {
                                rt.state.add_modal.name.push(ch);
                            } else {
                                rt.state.add_modal.url.push(ch);
                            }
                            rt.state.add_modal.error.clear();
                            rt.redraw_overview();
                        } else if ov_active {
                            rt.state.search_query.push(ch);
                            rt.state.search_selected = 0;
                            rt.redraw_overview();
                        } else if lp_active {
                            rt.state.launchpad_query.push(ch);
                            rt.state.launchpad_scroll = 0.0;
                            rt.state.launchpad_scroll_target = 0.0;
                            rt.redraw_launchpad();
                        }
                    }
                }
                true
            }
            _ => false,
        }
    });

    if handled == Some(true) {
        LRESULT(0)
    } else {
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

// ==================== VISUAL QA OFFSCREEN PNG CAPTURE (--capture <dir>) ====================

fn run_visual_capture(out_dir: &PathBuf) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let mut state = AppState::new(1600, 900);
    state.all_apps = system::scan_all_apps(&state.custom_items);
    state.dev_ai_apps = system::build_dev_ai_entries(&state.all_apps, &state.custom_items);
    state.games = system::scan_all_games(&state.custom_items);
    state.wallpapers = wallpaper::scan_wallpapers(&state.custom_items);
    state.drives = system::get_local_drives();
    state.open_windows = win32_shell::get_open_user_windows();
    state.active_user_hwnd = 1;
    state.active_window_label = "Microsoft Edge".into();
    state.output_devices = audio::get_audio_devices(false);
    state.mic_devices = audio::get_audio_devices(true);
    state.app_sessions = audio::get_app_audio_sessions();
    state.now_playing = audio::get_now_playing();
    if !state.now_playing.active {
        state.now_playing = NowPlaying {
            title: "UNA CERVEZA".into(),
            artist: "Fuerza Regida".into(),
            source_app: "Spotify".into(),
            is_playing: true,
            active: true,
            position_sec: 84.0,
            duration_sec: 225.0,
            has_timeline: true,
            sampled_at_unix_ms: now_unix_ms(),
        };
    } else {
        state.now_playing.is_playing = true;
    }
    state.tray_apps = system::scan_background_tray_apps();
    state.dev_ports = system::scan_dev_ports();
    state.radios = radios::query_radio_states();
    state.active_keyboard_hkl = win32_shell::get_active_keyboard_hkl();
    state.keyboard_layouts = win32_shell::get_installed_keyboard_layouts();
    state.weather = system::fetch_weather_blocking();
    let (mv, mm) = audio::get_master_volume();
    state.master_vol = mv;
    state.master_muted = mm;
    let (ru, rt_gb, rp) = system::get_ram_stats();
    state.ram_used_gb = ru;
    state.ram_total_gb = rt_gb;
    state.ram_pct = rp;
    state.cpu_pct = 16.0;
    state.cpu_history = vec![14.0, 18.0, 15.0, 22.0, 27.0, 19.0, 16.0, 24.0, 21.0, 17.0, 15.0, 18.0, 14.0, 16.0, 19.0, 16.0];
    state.dock_reveal.snap(1.0);
    state.music_energy = 1.0;
    state.music_wave_phase = 2.4;

    // 1. Capture Fondos & Widgets Studio (Wallpapers + Workshop + Draggable Widgets!)
    state.mode = ShellMode::Studio;
    state.overview_open = true;
    state.overview_anim.snap(1.0);
    state.tab_content_anim.snap(1.0);
    state.overview_tab_pill_x.snap(4.0);
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v31_studio.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_overview::render_overview(ctx, &state, 1600.0, 900.0 - TOPBAR_HEIGHT, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h2);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h3);
    });

    // 2. Capture Desktop with Aura Obsidian Flow Wallpaper + Animated Music Widget + Clock Widget + TopBar!
    state.overview_open = false;
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v31_desktop_widgets.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        let mut h4 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.4);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 590.0, 180.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Clock, 1, false, false, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 602.0, 390.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Music, 1, true, false, &mut h2);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h3);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h4);
    });

    // 3. Capture Control Center (Android Wi-Fi / BT / Inline Keyboard + Full Dynamic Background Tray!)
    state.cc_open = true;
    state.cc_keyboard_menu_open = true;
    state.cc_anim.snap(1.0);
    let _ = gpu.render_to_png(CC_WIDTH as u32, 840, &out_dir.join("v31_control_center.png"), |ctx| {
        let mut h = Vec::new();
        ctx.fill_rrect(Rect::new(0.0, 0.0, CC_WIDTH, 840.0), 0.0, theme::CANVAS);
        ui_control_center::render_control_center(ctx, &state, 840.0, &mut h);
    });

    // 4. Capture Launchpad MID-ANIMATION (Apple Expanding Circle Reveal at t=0.45!)
    state.cc_open = false;
    state.launchpad_open = true;
    state.launchpad_anim.snap(0.45);
    let (_, dy_tight, dw_tight, dh_tight) = ui_dock::dock_tight_geometry(&state, 1600, 900, true);
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v31_launchpad_circle.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, dy_tight as f32);
        ui_dock::render_dock(ctx, &state, dw_tight as f32, dh_tight as f32, true, &mut h2);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_launchpad::render_launchpad(ctx, &state, 1600.0, 900.0 - TOPBAR_HEIGHT, &mut h1);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h3);
    });

    // 5. Capture v3.2 Zero-Interference iPhone/Android Muesquita (Notch) demo!
    state.launchpad_open = false;
    let (nx_b, ny_b, nw_b, nh_b) = ui_dock::notch_geometry(1600, 900, false);
    let (nx_l, ny_l, nw_l, nh_l) = ui_dock::notch_geometry(1600, 900, true);
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v32_dock_notch.png"), |ctx| {
        let mut h_tb = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        // Simulate a maximized app window so the sleek bottom & left notches stand out clearly!
        ctx.fill_card(Rect::new(40.0, 56.0, 1520.0, 816.0), 14.0, theme::SURFACE, theme::VIOLET);
        ctx.draw_text_center(
            "App maximizada — 0 píxeles bloqueados · Pasa el mouse por la muesquita inferior o izquierda para desplegar el Dock",
            Rect::new(40.0, 420.0, 1520.0, 40.0),
            renderer::TextStyle::Title14Semi,
            theme::TEXT_SECONDARY,
        );
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, nx_b as f32, ny_b as f32);
        ui_dock::render_dock_notch(ctx, nw_b as f32, nh_b as f32, false, false);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, nx_l as f32, ny_l as f32);
        ui_dock::render_dock_notch(ctx, nw_l as f32, nh_l as f32, true, true);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h_tb);
    });

    // 6. Capture v5.0 Calendar + Pomodoro Flyout + Volume OSD!
    state.calendar_open = true;
    state.calendar_anim.snap(1.0);
    state.pomodoro_active = true;
    state.pomodoro_secs_left = 24 * 60 + 42;
    let _ = gpu.render_to_png(CAL_WIDTH as u32, CAL_HEIGHT as u32, &out_dir.join("v50_calendar_pomodoro.png"), |ctx| {
        let mut h = Vec::new();
        ctx.fill_rrect(Rect::new(0.0, 0.0, CAL_WIDTH, CAL_HEIGHT), 0.0, theme::CANVAS);
        ui_calendar::render_calendar(ctx, &state, &mut h);
    });

    // 7. Capture v5.0 AuraClip (`Win + V`) Clipboard History Manager!
    state.calendar_open = false;
    state.clipboard_open = true;
    state.clipboard_anim.snap(1.0);
    state.clipboard_items = vec![
        ClipItem { text: "https://github.com/dandifeto1111-prog/AuraOS".into(), pinned: true, chars_len: 44 },
        ClipItem { text: "cargo build --release --bins".into(), pinned: false, chars_len: 28 },
        ClipItem { text: "AuraOS v5.0 — Suite nativa PDF, Fotos, Capturas e IA".into(), pinned: false, chars_len: 52 },
    ];
    let _ = gpu.render_to_png(CLIP_WIDTH as u32, CLIP_HEIGHT as u32, &out_dir.join("v50_clipboard.png"), |ctx| {
        let mut h = Vec::new();
        ctx.fill_rrect(Rect::new(0.0, 0.0, CLIP_WIDTH, CLIP_HEIGHT), 0.0, theme::CANVAS);
        ui_clipboard::render_clipboard(ctx, &state, &mut h);
    });

    // 8. Capture v5.0 Spotlight Math Calculator (`245 * 18` -> `= 4410`)!
    state.clipboard_open = false;
    state.overview_open = true;
    state.overview_anim.snap(1.0);
    state.search_query = "245 * 18".into();
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v50_spotlight_calc.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_overview::render_overview(ctx, &state, 1600.0, 900.0 - TOPBAR_HEIGHT, &mut h1);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h2);
    });

    // 9. Capture v5.1 Dev & AI Hub (Multi-User AI Quotas + GitHub 7d Commits + Local Git Repos!)
    state.search_query.clear();
    state.mode = ShellMode::Dev;
    state.overview_tab_pill_x.snap(1.0);
    state.dev_hub = dev_hub::fetch_dev_hub_snapshot(&state.dev_hub_cfg);
    if state.dev_hub.github.daily_commits_7d.iter().all(|&c| c == 0) {
        state.dev_hub.github.daily_commits_7d = [2, 5, 3, 7, 4, 6, 9];
        state.dev_hub.github.total_recent_commits = 36;
        state.dev_hub.github.fetched_ok = true;
    }
    theme::apply_theme_scheme("Violet", (124, 92, 255));
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v51_dev_hub.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_overview::render_overview(ctx, &state, 1600.0, 900.0 - TOPBAR_HEIGHT, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h2);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h3);
    });

    // 10. Capture v5.1 Material You Sage Forest Unified Right Sidebar (Brightness + Night Light + Wavy Music + To-Do!)
    theme::apply_theme_scheme("Sage", (162, 198, 128));
    state.overview_open = false;
    state.cc_open = true;
    state.cc_anim.snap(1.0);
    state.cc_keyboard_menu_open = false;
    state.cc_bottom_tab = 0;
    state.settings.night_light = true;
    state.settings.keep_awake = true;
    state.settings.brightness_pct = 85;
    let _ = gpu.render_to_png(CC_WIDTH as u32, 860, &out_dir.join("v51_sidebar_sage.png"), |ctx| {
        let mut h = Vec::new();
        let p = theme::pal();
        ctx.fill_rrect(Rect::new(0.0, 0.0, CC_WIDTH, 860.0), 0.0, p.canvas);
        ui_control_center::render_control_center(ctx, &state, 860.0, &mut h);
    });

    // 11. Capture v5.1 Material You Apricot Desktop (Float TopBar + Battery + Vertical Bold Clock + Wavy Music!)
    theme::apply_theme_scheme("Apricot", (244, 162, 108));
    state.cc_open = false;
    state.settings.bar_style = "Float".into();
    state.settings.clock_widget_style = "Vertical".into();
    state.battery_status = Some((88, true));
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v51_desktop_vertical_float.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        let mut h4 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.4);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 590.0, 170.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Clock, 1, false, false, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 602.0, 392.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Music, 1, true, false, &mut h2);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h3);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h4);
    });

    // 12. Capture v5.1 Cookie Scalloped Clock + Sage Forest Material You!
    theme::apply_theme_scheme("Sage", (162, 198, 128));
    state.settings.bar_style = "Hug".into();
    state.settings.clock_widget_style = "Cookie".into();
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v51_desktop_cookie_sage.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        let mut h4 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.4);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 590.0, 170.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Clock, 1, false, false, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 602.0, 392.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Music, 1, false, false, &mut h2);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h3);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h4);
    });

    // 13. Capture v5.3 Aura Studio Creative Suite Hub (AuraBeats DAW + AuraPhotos + Video Slot!)
    theme::apply_theme_scheme("Violet", (124, 92, 255));
    state.overview_open = true;
    state.overview_anim.snap(1.0);
    state.search_query.clear();
    state.mode = ShellMode::Creative;
    state.overview_tab_pill_x.snap(3.0);
    state.aurabeats_installed = true;
    state.aurabeats_plugin_count = state.aurabeats_plugin_count.max(28);
    let _ = gpu.render_to_png(1600, 900, &out_dir.join("v53_aura_studio.png"), |ctx| {
        let mut h1 = Vec::new();
        let mut h2 = Vec::new();
        let mut h3 = Vec::new();
        wallpaper::draw_wallpaper_frame(ctx, WallpaperKind::Preset, "aura_obsidian_flow", 1600.0, 900.0, 2.0);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_overview::render_overview(ctx, &state, 1600.0, 900.0 - TOPBAR_HEIGHT, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 0.0, TOPBAR_HEIGHT);
        ui_dock::render_dock(ctx, &state, DOCK_THICKNESS, 900.0 - TOPBAR_HEIGHT, true, &mut h2);
        ctx.reset_transform();
        ui_topbar::render_topbar(ctx, &state, 1600.0, &mut h3);
    });
}
