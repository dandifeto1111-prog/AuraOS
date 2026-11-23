#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

mod audio;
mod models;
mod motion;
mod radios;
mod renderer;
mod system;
mod telemetry_worker;
mod theme;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WinKind {
    TopBar,
    Dock,
    DockNotch,
    ControlCenter,
    PowerMenu,
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
    startup_rx: Option<Receiver<StartupCatalog>>,
    weather_rx: Option<Receiver<WeatherInfo>>,
    radio_rx: Option<Receiver<RadioStates>>,
    file_pick_rx: Option<Receiver<(FilePickKind, Option<String>)>>,

    hwnd_topbar: HWND,
    hwnd_dock: HWND,
    hwnd_notch: HWND,
    hwnd_cc: HWND,
    hwnd_power: HWND,
    hwnd_launchpad: HWND,
    hwnd_overview: HWND,

    surf_topbar: WindowSurface,
    surf_dock: WindowSurface,
    surf_notch: WindowSurface,
    surf_cc: WindowSurface,
    surf_power: WindowSurface,
    surf_launchpad: WindowSurface,
    surf_overview: WindowSurface,

    widget_wins: Vec<DesktopWidgetWin>,
    dragging_widget: Option<(WidgetKind, i32, i32)>, // (kind, grab_offset_x, grab_offset_y)

    hits_topbar: Vec<HitRegion>,
    hits_dock: Vec<HitRegion>,
    hits_cc: Vec<HitRegion>,
    hits_power: Vec<HitRegion>,
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
    last_dock_hover_exit: Instant,
    mouse_in_dock: bool,
    mouse_in_notch: bool,
    dock_hwnd_visible: bool,
    notch_visible: bool,
    dock_geo: (i32, i32, i32, i32),
    slow_tick_count: u32,
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

    // Initialize integrated lightweight Wallpaper Engine (behind desktop icons!)
    let mut wp_engine = wallpaper::WallpaperEngine::new(&gpu, sw, sh);
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
    let (dx, dy, dw, dh) = ui_dock::dock_tight_geometry(&state, sw, sh, is_left);
    let hwnd_dock = create_layered_window(&cls_name, "AuraDock", dx, dy, dw, dh, true, true)?;
    let (nx, ny, nw, nh) = ui_dock::notch_geometry(sw, sh, is_left);
    let hwnd_notch = create_layered_window(&cls_name, "AuraDockNotch", nx, ny, nw, nh, true, true)?;
    let cc_h = (sh - top_h - 18).min(CC_MAX_H as i32);
    let hwnd_cc = create_layered_window(&cls_name, "AuraControlCenter", sw - CC_WIDTH as i32 - 10, top_h + 6, CC_WIDTH as i32, cc_h, true, false)?;
    let hwnd_power = create_layered_window(&cls_name, "AuraPowerMenu", sw - POWER_MENU_W as i32 - 10, top_h + 6, POWER_MENU_W as i32, POWER_MENU_H as i32, true, false)?;
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
        let surf = WindowSurface::new(&gpu, hw, ww as u32, wh as u32)?;
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

    // Global hotkeys (Alt+Space, Alt+E, Ctrl+Alt+W, Ctrl+Alt+L for AuraLock!)
    unsafe {
        let _ = RegisterHotKey(hwnd_topbar, HK_OVERVIEW, MOD_ALT, VK_SPACE.0 as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_FILES, MOD_ALT, 'E' as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_RESTORE, MOD_CONTROL | MOD_ALT, 'W' as u32);
        let _ = RegisterHotKey(hwnd_topbar, HK_LOCK, MOD_CONTROL | MOD_ALT, 'L' as u32);
    }

    let surf_topbar = WindowSurface::new(&gpu, hwnd_topbar, sw as u32, top_h as u32)?;
    let surf_dock = WindowSurface::new(&gpu, hwnd_dock, dw as u32, dh as u32)?;
    let surf_notch = WindowSurface::new(&gpu, hwnd_notch, nw as u32, nh as u32)?;
    let surf_cc = WindowSurface::new(&gpu, hwnd_cc, CC_WIDTH as u32, cc_h as u32)?;
    let surf_power = WindowSurface::new(&gpu, hwnd_power, POWER_MENU_W as u32, POWER_MENU_H as u32)?;
    let surf_launchpad = WindowSurface::new(&gpu, hwnd_launchpad, sw as u32, body_h as u32)?;
    let surf_overview = WindowSurface::new(&gpu, hwnd_overview, sw as u32, body_h as u32)?;

    let wm_act = unsafe {
        let m = win32_shell::to_wide("WM_AURASHELL_V3_ACTIVATE");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };
    let wm_cfg = unsafe {
        let m = win32_shell::to_wide("WM_AURAOS_SETTINGS_CHANGED");
        RegisterWindowMessageW(PCWSTR(m.as_ptr()))
    };

    let upd_settings = state.settings.clone();
    let mut rt = ShellRuntime {
        gpu,
        state,
        cpu: system::CpuTracker::new(),
        boost: system::GameBoost::new(),
        wallpaper: wp_engine,
        startup_rx: Some(cat_rx),
        weather_rx: Some(w_rx),
        radio_rx: Some(r_rx),
        file_pick_rx: None,
        hwnd_topbar,
        hwnd_dock,
        hwnd_notch,
        hwnd_cc,
        hwnd_power,
        hwnd_launchpad,
        hwnd_overview,
        surf_topbar,
        surf_dock,
        surf_notch,
        surf_cc,
        surf_power,
        surf_launchpad,
        surf_overview,
        widget_wins,
        dragging_widget: None,
        hits_topbar: Vec::new(),
        hits_dock: Vec::new(),
        hits_cc: Vec::new(),
        hits_power: Vec::new(),
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
        last_dock_hover_exit: Instant::now(),
        mouse_in_dock: false,
        mouse_in_notch: false,
        dock_hwnd_visible: true,
        notch_visible: false,
        dock_geo: (dx, dy, dw, dh),
        slow_tick_count: 0,
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
    }

    with_rt(|rt| rt.wallpaper.shutdown());
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
            WinKind::DockNotch => &[],
            WinKind::ControlCenter => &self.hits_cc,
            WinKind::PowerMenu => &self.hits_power,
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
    }

    fn redraw_dock(&mut self) {
        let w = self.surf_dock.width as f32;
        let h = self.surf_dock.height as f32;
        let is_left = !self.state.settings.dock_position.eq_ignore_ascii_case("Bottom");
        let state = &self.state;
        let hits = &mut self.hits_dock;
        self.surf_dock.draw(&mut self.gpu, |ctx| ui_dock::render_dock(ctx, state, w, h, is_left, hits));
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
        let state = &self.state;
        if let Some(w) = self.widget_wins.iter_mut().find(|w| w.kind == kind) {
            let hov = w.hovered;
            let hits = &mut w.hits;
            w.surf.draw(&mut self.gpu, |ctx| {
                ui_widgets::render_desktop_widget(ctx, state, kind, tier, hov, hits);
            });
        }
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
                w.surf.resize(&self.gpu, ww as u32, wh as u32);
                unsafe {
                    if global_on && cfg.enabled {
                        let _ = SetWindowPos(w.hwnd, HWND_BOTTOM, cfg.x, cfg.y, ww, wh, SWP_NOACTIVATE);
                        let _ = ShowWindow(w.hwnd, SW_SHOWNOACTIVATE);
                    } else {
                        let _ = ShowWindow(w.hwnd, SW_HIDE);
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
        self.close_launchpad();
    }

    fn open_overview(&mut self, settings_view: bool) {
        self.close_cc();
        self.close_power();
        self.close_launchpad();
        let was_closed = !self.state.overview_open;
        self.state.overview_open = true;
        self.state.overview_settings_view = settings_view;
        self.state.add_modal.open = false;
        self.state.search_query.clear();
        self.state.search_selected = 0;
        self.state.open_windows = win32_shell::get_open_user_windows();
        let _ = self.telemetry.tx.send(TelemetryCmd::RefreshOverview);
        if was_closed {
            self.state.overview_anim.snap(0.01);
            self.state.overview_anim.animate_to(1.0, 170.0, CubicBezier::EASE_OUT);
            self.state.tab_content_anim.snap(1.0);
        }
        self.redraw_overview();
        self.redraw_topbar();
        win32_shell::bring_to_front(self.hwnd_overview);
        win32_shell::ensure_topmost(self.hwnd_topbar);
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
        self.close_launchpad();
        // 0ms UI-THREAD OPEN: renders immediately from warm MTA background cache!
        // Zero COM / WinRT / process-snapshot blocking even during heavy games or Spotify!
        self.state.keyboard_lang = win32_shell::get_keyboard_layout_short();
        self.state.active_keyboard_hkl = win32_shell::get_active_keyboard_hkl();
        self.state.keyboard_layouts = win32_shell::get_installed_keyboard_layouts();
        self.state.cc_keyboard_menu_open = false;
        self.state.cc_scroll = 0.0;
        self.state.cc_open = true;

        // Trigger fresh background MTA update (arrives silently in ~25ms without blocking frame 1!)
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
        self.state.cc_last_closed = Some(Instant::now());
        self.state.cc_anim.animate_to(0.0, 135.0, CubicBezier::EASE_OUT);
        self.redraw_topbar();
    }

    fn open_power(&mut self) {
        self.close_overview();
        self.close_cc();
        self.close_launchpad();
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
                    ShellMode::Studio => 3.0,
                };
                let target_idx = match m {
                    ShellMode::PC => 0.0,
                    ShellMode::Dev => 1.0,
                    ShellMode::Gamer => 2.0,
                    ShellMode::Studio => 3.0,
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
                window_styler::minimize_active_window(self.state.active_user_hwnd);
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
            Action::ToggleMasterMute => {
                self.state.master_muted = audio::toggle_master_mute();
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
            }
            Action::SetMasterVolume(p) => {
                let p = p.clamp(0, 100);
                audio::set_master_volume(p);
                self.state.master_vol = p;
                self.state.master_muted = false;
                self.redraw_topbar();
                if self.state.cc_open { self.redraw_cc(); }
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
                self.close_all_overlays();
                win32_shell::toggle_or_focus_window(hwnd);
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
                if self.state.overview_open { self.redraw_overview(); }
            }
            Action::AddCustomWallpaperFile => {
                self.spawn_file_picker(
                    FilePickKind::CustomWallpaper,
                    "Seleccionar video, imagen o web para Fondo de Pantalla",
                    "Fondos (*.mp4;*.webm;*.jpg;*.png;*.webp;*.html)",
                    "*.mp4;*.webm;*.mkv;*.jpg;*.jpeg;*.png;*.webp;*.html",
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
            self.gpu.prewarm_icons(self.state.all_apps.iter().map(|a| a.icon_path.clone()));
            self.gpu.prewarm_icons(self.state.games.iter().map(|g| g.icon_path.clone()));
            self.gpu.prewarm_images(self.state.wallpapers.iter().filter_map(|w| w.preview_path.clone()));
            self.gpu.prewarm_images(self.state.games.iter().filter_map(|g| g.cover_path.clone()));
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
                        || self.state.now_playing.active != now_playing.active;
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
                        let prefix = if low.ends_with(".mp4") || low.ends_with(".webm") || low.ends_with(".mkv") || low.ends_with(".mov") {
                            "video"
                        } else if low.ends_with(".html") || low.ends_with(".htm") {
                            "web"
                        } else {
                            "image"
                        };
                        let wp_id = format!("{}:{}", prefix, path);
                        self.state.custom_items.custom_wallpapers.push(CustomEntry {
                            id: format!("wp_{}", ts_id),
                            name,
                            target: path,
                            subtitle: "Fondo personalizado".into(),
                            category: "Wallpaper".into(),
                            cover_path: None,
                        });
                        save_custom_items(&self.state.custom_items);
                        self.state.wallpapers = wallpaper::scan_wallpapers(&self.state.custom_items);
                        self.state.settings.wallpaper_id = wp_id.clone();
                        save_settings(&self.state.settings);
                        self.wallpaper.set_wallpaper(&mut self.gpu, &wp_id);
                        self.state.mode = ShellMode::Studio;
                        self.state.overview_tab_pill_x.snap(3.0);
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
            } else {
                self.redraw_cc();
            }
        }

        // 5. Power Menu Apple Circular Reveal
        if self.state.power_anim.is_moving() {
            let moving = self.state.power_anim.step(dt_ms);
            if !moving && !self.state.power_open && self.state.power_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_power, SW_HIDE); }
            } else {
                self.redraw_power();
            }
        }

        // 6. Launchpad Apple Expanding Circle Reveal + Butter-Smooth Momentum Scrolling!
        let mut lp_dirty = false;
        if self.state.launchpad_anim.is_moving() {
            let moving = self.state.launchpad_anim.step(dt_ms);
            if !moving && !self.state.launchpad_open && self.state.launchpad_anim.value <= 0.02 {
                unsafe { let _ = ShowWindow(self.hwnd_launchpad, SW_HIDE); }
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

        // 8. Animated Desktop Music Visualizer (paused while Launchpad/Overview covers desktop!)
        let music_enabled = self.state.settings.show_desktop_widgets
            && self.state.widgets.get(WidgetKind::Music).map(|w| w.enabled).unwrap_or(false);
        let fullscreen_overlay = self.state.launchpad_open || self.state.overview_open;
        if music_enabled && !self.desktop_covered && !self.hidden_for_fullscreen && !fullscreen_overlay {
            let target_energy = if self.state.now_playing.is_playing { 1.0 } else { 0.0 };
            let diff = target_energy - self.state.music_energy;
            if self.state.now_playing.is_playing || diff.abs() > 0.015 {
                self.state.music_energy += diff * (dt_sec * 6.0).min(1.0);
                self.state.music_wave_phase += dt_sec * 1.8;
                if self.state.music_wave_phase > 1000.0 {
                    self.state.music_wave_phase -= 1000.0;
                }
                self.redraw_widget(WidgetKind::Music);
            }
        }

        // 9. Integrated Wallpaper Engine (Video/Web/GPU Presets)
        // Keeps live preview running inside Studio tab, pauses behind Launchpad to give 100% GPU to scrolling!
        if !self.state.launchpad_open {
            self.wallpaper.tick(&mut self.gpu, dt_sec);
        }
    }

    fn tick_fast_120ms(&mut self) {
        if self.state.settings.hide_windows_taskbar {
            win32_shell::enforce_taskbar_suppressed();
        }

        let any_overlay = self.state.overview_open || self.state.cc_open || self.state.power_open || self.state.launchpad_open;
        let is_fs = !any_overlay && win32_shell::is_fullscreen_app_active();

        // Auto-pause Wallpaper Engine & Music Visualizer whenever a maximized/fullscreen window covers the desktop!
        let covered = !any_overlay && win32_shell::is_any_maximized_window_visible();
        self.desktop_covered = covered;
        if self.state.settings.wallpaper_auto_pause {
            self.wallpaper.set_paused(covered || is_fs);
        }

        if is_fs && !self.hidden_for_fullscreen {
            self.hidden_for_fullscreen = true;
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
        if self.hidden_for_fullscreen {
            return;
        }

        // Zero-Interference Dock Auto-Hide + iPhone/Android Muesquita Trigger!
        // ONLY reveals when hovering the exact compact Notch rect (never random screen edges!)
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

        let want_hidden = if !self.state.settings.dock_auto_hide || any_overlay {
            false
        } else if in_notch || in_dock_tight || self.mouse_in_dock || self.mouse_in_notch {
            self.last_dock_hover_exit = Instant::now();
            false
        } else if !self.state.dock_hidden && self.last_dock_hover_exit.elapsed().as_millis() < 340 {
            false
        } else {
            // Whenever Auto-Hide is active and the user isn't hovering the Dock or Notch,
            // fully hide hwnd_dock (SW_HIDE = 0 blocked clicks!) leaving ONLY the minimal muesquita!
            true
        };

        if want_hidden != self.state.dock_hidden {
            self.state.dock_hidden = want_hidden;
            if !want_hidden {
                self.show_dock_hide_notch();
                self.state.dock_reveal.set_target(1.0);
            } else {
                self.state.dock_reveal.set_target(0.0);
            }
        } else if want_hidden && self.dock_hwnd_visible && self.state.dock_reveal.value <= 0.03 {
            self.hide_dock_show_notch();
        }
    }

    fn tick_slow_1s(&mut self) {
        if let Some(rx) = &self.weather_rx {
            if let Ok(w) = rx.try_recv() {
                self.state.weather = w;
                self.weather_rx = None;
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

        let new_wins = win32_shell::get_open_user_windows();
        let dock_changed = new_wins.len() != self.state.open_windows.len()
            || new_wins.iter().zip(self.state.open_windows.iter()).any(|(a, b)| a.hwnd != b.hwnd || a.is_active != b.is_active);
        self.state.open_windows = new_wins;

        // Continuously apply AuraOS Drool Dark Mode DWM styling to ALL Windows applications (Edge, etc.)!
        if self.state.settings.style_windows_apps {
            window_styler::apply_aura_style_to_windows(&self.state.open_windows);
        }

        if self.state.dock_item_scales.len() < self.state.open_windows.len() + 6 {
            self.state.dock_item_scales.resize(self.state.open_windows.len() + 6, Spring::dock_hover(1.0));
        }

        if !self.hidden_for_fullscreen {
            self.redraw_topbar();
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
        if self.state.settings.show_desktop_widgets && !self.desktop_covered {
            self.redraw_widget(WidgetKind::Clock);
            self.redraw_widget(WidgetKind::Weather);
            self.redraw_widget(WidgetKind::Monitor);
            if !self.state.now_playing.is_playing {
                self.redraw_widget(WidgetKind::Music);
            }
            for w in &self.widget_wins {
                win32_shell::send_to_desktop_bottom(w.hwnd);
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
            SliderDragKind::AppVolume { x, w, pids } => {
                let pct = (((px - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as i32;
                self.execute(Action::SetAppVolume(pids, pct));
            }
            SliderDragKind::GlassOpacity { x, w } => {
                let frac = ((px - x) / w.max(1.0)).clamp(0.0, 1.0);
                let val = 0.50 + frac * 0.46;
                self.execute(Action::SetGlassOpacity(val));
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
            rt.state.settings = load_settings();
            rt.state.widgets = load_widgets(rt.screen_w, rt.screen_h);
            if rt.state.settings.hide_windows_taskbar != prev_hide {
                win32_shell::set_taskbar_visibility(!rt.state.settings.hide_windows_taskbar);
                win32_shell::apply_full_work_area(TOPBAR_HEIGHT as i32, rt.state.settings.hide_windows_taskbar);
            }
            if rt.state.settings.wallpaper_id != prev_wp {
                let next_wp = rt.state.settings.wallpaper_id.clone();
                rt.wallpaper.set_wallpaper(&mut rt.gpu, &next_wp);
            }
            rt.sync_dock_tight_bounds();
            rt.sync_widget_windows_visibility();
            rt.redraw_topbar();
            rt.redraw_dock();
            rt.redraw_all_widgets();
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
                match wparam.0 as i32 {
                    HK_OVERVIEW => rt.execute(Action::ToggleOverview),
                    HK_FILES => rt.execute(Action::LaunchAuraFiles(None)),
                    HK_RESTORE => rt.execute(Action::RestoreWindowsAndExit),
                    HK_LOCK => rt.execute(Action::LaunchAuraLock),
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

                // Live Desktop Widget Dragging!
                if let Some((dwk, off_x, off_y)) = rt.dragging_widget {
                    let (cx, cy) = win32_shell::cursor_pos();
                    let nx = (cx - off_x).clamp(8, rt.screen_w - 80);
                    let ny = (cy - off_y).clamp(TOPBAR_HEIGHT as i32 + 6, rt.screen_h - 60);
                    if let Some(cfg) = rt.state.widgets.get_mut(dwk) {
                        cfg.x = nx;
                        cfg.y = ny;
                    }
                    if let Some(ww) = rt.widget_wins.iter().find(|w| w.kind == dwk) {
                        let _ = SetWindowPos(ww.hwnd, HWND_BOTTOM, nx, ny, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                    return true;
                }

                if kind == WinKind::DockNotch {
                    // Instant zero-latency reveal the moment cursor touches the sleek iPhone/Android muesquita!
                    rt.mouse_in_notch = true;
                    rt.last_dock_hover_exit = Instant::now();
                    if rt.state.dock_hidden {
                        rt.state.dock_hidden = false;
                        rt.show_dock_hide_notch();
                        rt.state.dock_reveal.set_target(1.0);
                    }
                    return true;
                }
                if kind == WinKind::Dock {
                    rt.mouse_in_dock = true;
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

                let new_hov = rt
                    .hits_for(kind)
                    .iter()
                    .rev()
                    .find(|h| h.rect.contains(px, py))
                    .map(|h| h.id.clone());

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
                    rt.last_dock_hover_exit = Instant::now();
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
                if rt.dragging_widget.is_some() {
                    rt.dragging_widget = None;
                    let _ = ReleaseCapture();
                    save_widgets(&rt.state.widgets);
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
                        let max_s = (rt.lp_content_h - (rt.surf_launchpad.height as f32 - 180.0)).max(0.0);
                        rt.state.launchpad_scroll_target = (rt.state.launchpad_scroll_target + step * 2.2).clamp(0.0, max_s);
                    }
                    WinKind::Overview => {
                        if rt.state.mode == ShellMode::Studio {
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
                        if ov_active && rt.state.add_modal.open {
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
    state.overview_tab_pill_x.snap(3.0);
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
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Clock, 1, false, &mut h1);
        ctx.set_transform_scale_origin(1.0, 0.0, 0.0, 602.0, 390.0);
        ui_widgets::render_desktop_widget(ctx, &state, WidgetKind::Music, 1, true, &mut h2);
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
}
