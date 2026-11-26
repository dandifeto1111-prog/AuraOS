#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

//! AuraWallpaper v5.5 — Isolated Hardware GPU Wallpaper Subprocess (Video + React Bits GradientWaves)
//! - Mode 1 (`--thumb <video>`): Isolated video thumbnail extractor (keeps Media Foundation out of AuraShell)
//! - Mode 2 (`--video <path>`): Native-resolution 60 FPS DXVA/D3D11 hardware video wallpaper
//! - Mode 3 (`--waves <preset>`): Hardware D3D11 HLSL 3D GradientWaves shader (React Bits 1:1) with
//!   real-time mouse hover swells & click spikes ("picos del fondo"), smooth live color/preset crossfade
//!   via `WM_AURAOS_SETTINGS_CHANGED`, and 0.00% CPU/GPU impact on `AuraShell.exe`!
//! - Mode 4 (`--capture-waves <out.png> <preset> [--spike]`): Offscreen GPU render capture for QA.
//! - True pause (`WM_AURA_WP_PAUSE` / `WM_AURA_WP_RESUME`): 0.00% CPU & 0.00% GPU when covered/gaming!

use aura_os::gradient_waves;
use aura_os::models::load_settings;
use aura_os::renderer::{GpuEngine, Rect, WindowSurface};
use aura_os::theme;
use aura_os::video_decoder::{self, NativeVideoStream};
use aura_os::wallpaper;
use aura_os::waves_gpu::GpuWavesRenderer;
use aura_os::win32_shell::{self, to_wide};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClassNameW, GetCursorPos,
    GetMessageW, GetSystemMetrics, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetParent, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
    WindowFromPoint, HWND_BOTTOM, HWND_MESSAGE, HWND_TOP, MSG, SM_CXSCREEN, SM_CYSCREEN,
    SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
    WM_DESTROY, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};

const TIMER_WATCHDOG: usize = 1;
const TIMER_WAVES_FRAME: usize = 2;

enum WorkerBackend {
    Video {
        gpu: GpuEngine,
        surf: WindowSurface,
        stream: Option<NativeVideoStream>,
        frame_pending: Arc<AtomicBool>,
        frame_scratch: Vec<u8>,
        last_seq: u64,
    },
    Waves {
        renderer: GpuWavesRenderer,
        last_tick: Instant,
        prev_lbtn_down: bool,
        frame_ms: u32,
        wm_settings_changed: u32,
    },
}

struct WpRuntime {
    hwnd: HWND,
    ipc_hwnd: HWND,
    backend: WorkerBackend,
    parent_pid: u32,
    paused: bool,
    trim_countdown: u8,
}

thread_local! {
    static WP_RT: RefCell<Option<WpRuntime>> = const { RefCell::new(None) };
}

fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return true;
    }
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let wait = WaitForSingleObject(h, 0);
        let _ = CloseHandle(h);
        wait != WAIT_OBJECT_0
    }
}

/// Checks if the screen point `pt` is directly over the desktop / wallpaper layer (not an app window)
fn is_point_on_desktop(pt: POINT) -> bool {
    unsafe {
        let hw = WindowFromPoint(pt);
        if hw.is_invalid() {
            return true;
        }
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hw, &mut buf);
        if n <= 0 {
            return false;
        }
        let cls = String::from_utf16_lossy(&buf[..n as usize]);
        matches!(
            cls.as_str(),
            "SysListView32" | "SHELLDLL_DefView" | "WorkerW" | "Progman" | "FolderView"
        ) || cls == wallpaper::WP_WORKER_CLASS
    }
}

fn sync_waves_from_settings(renderer: &mut GpuWavesRenderer) {
    let settings = load_settings();
    gradient_waves::set_tuning(settings.waves_speed, settings.waves_relief);
    gradient_waves::set_custom_color(&settings.waves_custom_color);
    let key = settings
        .wallpaper_id
        .strip_prefix("preset:")
        .filter(|k| gradient_waves::is_waves_key(k))
        .unwrap_or("waves_ocean");
    if let Some(p) = gradient_waves::preset_by_key(key) {
        let color = gradient_waves::custom_color_rgb().unwrap_or(p.wave_color);
        renderer.set_target_preset(p, color, settings.waves_speed, settings.waves_relief);
    }
}

unsafe extern "system" fn wp_worker_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Handle live settings reload for smooth GPU color/preset crossfade (zero restart!)
    let handled_settings = WP_RT.with(|cell| {
        if let Ok(mut guard) = cell.try_borrow_mut() {
            if let Some(rt) = guard.as_mut() {
                if let WorkerBackend::Waves { renderer, wm_settings_changed, .. } = &mut rt.backend {
                    if msg == wallpaper::WM_AURA_WP_RELOAD_WAVES
                        || (*wm_settings_changed != 0 && msg == *wm_settings_changed)
                    {
                        sync_waves_from_settings(renderer);
                        if !rt.paused {
                            renderer.render_frame(0.016, 0);
                        }
                        return true;
                    }
                }
            }
        }
        false
    });
    if handled_settings {
        return LRESULT(0);
    }

    match msg {
        m if m == wallpaper::WM_AURA_WP_FRAME => {
            WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if let WorkerBackend::Video {
                            surf,
                            stream,
                            frame_pending,
                            frame_scratch,
                            last_seq,
                            ..
                        } = &mut rt.backend
                        {
                            frame_pending.store(false, Ordering::Release);
                            if rt.paused {
                                return;
                            }
                            if let Some(vs) = stream {
                                if let Some((fw, fh)) = vs.swap_latest_frame(last_seq, frame_scratch) {
                                    surf.draw_video_frame(fw, fh, frame_scratch);
                                }
                            }
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        WM_TIMER if wparam.0 == TIMER_WAVES_FRAME => {
            WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if rt.paused {
                            return;
                        }
                        if let WorkerBackend::Waves {
                            renderer,
                            last_tick,
                            prev_lbtn_down,
                            ..
                        } = &mut rt.backend
                        {
                            let now = Instant::now();
                            let dt = now.duration_since(*last_tick).as_secs_f32().clamp(0.004, 0.05);
                            *last_tick = now;

                            // Poll cursor for interactive silk hover swell + desktop click spikes (Vanguard-safe!)
                            let mut pt = POINT::default();
                            if GetCursorPos(&mut pt).is_ok() {
                                let u = (pt.x as f32 / renderer.width.max(1) as f32).clamp(0.0, 1.0);
                                let v = (pt.y as f32 / renderer.height.max(1) as f32).clamp(0.0, 1.0);
                                let on_desk = is_point_on_desktop(pt);
                                renderer.update_mouse(u, v, on_desk, dt);

                                let ldown = (GetAsyncKeyState(0x01) as u16 & 0x8000) != 0;
                                if ldown && !*prev_lbtn_down && on_desk {
                                    renderer.spawn_click_spike(u, v, 1.25);
                                }
                                *prev_lbtn_down = ldown;
                            }

                            renderer.render_frame(dt, 1);
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        m if m == wallpaper::WM_AURA_WP_PAUSE => {
            WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if !rt.paused {
                            rt.paused = true;
                            match &mut rt.backend {
                                WorkerBackend::Video { gpu, stream, .. } => {
                                    if let Some(vs) = stream {
                                        vs.set_paused(true);
                                    }
                                    gpu.trim_gpu();
                                }
                                WorkerBackend::Waves { renderer, .. } => {
                                    let _ = KillTimer(rt.hwnd, TIMER_WAVES_FRAME);
                                    renderer.trim_gpu();
                                }
                            }
                            win32_shell::trim_own_working_set();
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        m if m == wallpaper::WM_AURA_WP_RESUME => {
            WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if rt.paused {
                            rt.paused = false;
                            match &mut rt.backend {
                                WorkerBackend::Video { stream, .. } => {
                                    if let Some(vs) = stream {
                                        vs.set_paused(false);
                                    }
                                }
                                WorkerBackend::Waves { last_tick, frame_ms, .. } => {
                                    *last_tick = Instant::now();
                                    SetTimer(rt.hwnd, TIMER_WAVES_FRAME, *frame_ms, None);
                                }
                            }
                        }
                    }
                }
            });
            return LRESULT(0);
        }
        m if m == wallpaper::WM_AURA_WP_QUIT || m == WM_CLOSE => {
            let (render_hw, ipc_hw) = WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if let WorkerBackend::Video { stream, .. } = &mut rt.backend {
                            if let Some(mut vs) = stream.take() {
                                vs.stop_now();
                            }
                        }
                        return (rt.hwnd, rt.ipc_hwnd);
                    }
                }
                (hwnd, HWND::default())
            });
            if !render_hw.is_invalid() {
                let _ = KillTimer(render_hw, TIMER_WAVES_FRAME);
                let _ = KillTimer(render_hw, TIMER_WATCHDOG);
                let _ = ShowWindow(render_hw, SW_HIDE);
                let _ = SetParent(render_hw, HWND::default());
                let _ = DestroyWindow(render_hw);
            }
            if !ipc_hw.is_invalid() && ipc_hw != render_hw {
                let _ = DestroyWindow(ipc_hw);
            }
            PostQuitMessage(0);
            return LRESULT(0);
        }
        WM_TIMER if wparam.0 == TIMER_WATCHDOG => {
            let should_exit = WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        if rt.parent_pid != 0 && !is_pid_alive(rt.parent_pid) {
                            return true;
                        }
                        if let WorkerBackend::Video { stream, .. } = &rt.backend {
                            if let Some(vs) = stream {
                                if vs.has_failed() {
                                    return true;
                                }
                            }
                        }
                        if rt.trim_countdown > 0 {
                            rt.trim_countdown -= 1;
                            if rt.trim_countdown == 0 {
                                win32_shell::trim_own_working_set();
                            }
                        }
                    }
                }
                false
            });
            if should_exit {
                let _ = PostMessageW(hwnd, wallpaper::WM_AURA_WP_QUIT, WPARAM(0), LPARAM(0));
            }
            return LRESULT(0);
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn create_worker_windows(sw: i32, sh: i32) -> Option<(HWND, HWND)> {
    unsafe {
        if let Some(prev) = wallpaper::find_video_worker_ipc_hwnd() {
            let _ = PostMessageW(prev, wallpaper::WM_AURA_WP_QUIT, WPARAM(0), LPARAM(0));
            std::thread::sleep(std::time::Duration::from_millis(80));
        }

        let cls_w = to_wide(wallpaper::WP_WORKER_CLASS);
        let ipc_cls_w = to_wide(wallpaper::WP_IPC_CLASS);
        let hinst = GetModuleHandleW(None).ok()?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wp_worker_proc),
            hInstance: hinst.into(),
            lpszClassName: PCWSTR(cls_w.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let ipc_wc = WNDCLASSW {
            lpfnWndProc: Some(wp_worker_proc),
            hInstance: hinst.into(),
            lpszClassName: PCWSTR(ipc_cls_w.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&ipc_wc);

        let ipc_title = to_wide("AuraWallpaperIPC");
        let ipc_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(ipc_cls_w.as_ptr()),
            PCWSTR(ipc_title.as_ptr()),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            hinst,
            None,
        )
        .unwrap_or_default();

        let title = to_wide("AuraWallpaperWorker");
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            PCWSTR(cls_w.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            sw,
            sh,
            None,
            None,
            hinst,
            None,
        )
        .ok()?;

        if let Some(workerw) = wallpaper::find_desktop_workerw() {
            let _ = SetParent(hwnd, workerw);
            let _ = SetWindowPos(hwnd, HWND_TOP, 0, 0, sw, sh, SWP_NOACTIVATE);
        } else {
            let _ = SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, sw, sh, SWP_NOACTIVATE);
        }
        Some((hwnd, ipc_hwnd))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Mode 1: One-shot isolated thumbnail extraction (keeps Media Foundation 100% out of AuraShell.exe!)
    if let Some(pos) = args.iter().position(|a| a == "--thumb") {
        if let Some(path) = args.get(pos + 1) {
            let _ = video_decoder::extract_video_thumbnail(path);
        }
        return;
    }

    // Mode 4: Offscreen Hardware GPU GradientWaves PNG capture for visual QA
    if let Some(pos) = args.iter().position(|a| a == "--capture-waves") {
        let out = args.get(pos + 1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("waves_gpu.png"));
        let key = args.get(pos + 2).map(|s| s.as_str()).unwrap_or("waves_aurora");
        let with_spike = args.iter().any(|a| a == "--spike");
        let preset = gradient_waves::preset_by_key(key).unwrap_or(&gradient_waves::PRESETS[4]);
        match GpuWavesRenderer::new_offscreen(1600, 900, preset, preset.wave_color) {
            Ok(mut r) => {
                r.anim_time = 3.2;
                if with_spike {
                    r.spawn_click_spike(0.56, 0.56, 1.45);
                    r.render_frame(0.14, 0);
                } else {
                    r.render_frame(0.016, 0);
                }
                if let Err(e) = r.save_offscreen_png(&out) {
                    eprintln!("capture-waves save error: {e}");
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("capture-waves GPU init error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    let mut video_path = String::new();
    let mut waves_key = String::new();
    let mut max_w_arg: u32 = 0;
    let mut max_h_arg: u32 = 0;
    let mut max_fps: u32 = 60;
    let mut perf_mode = false;
    let mut start_paused = false;
    let mut parent_pid: u32 = 0;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--video" if i + 1 < args.len() => {
                video_path = args[i + 1].clone();
                i += 2;
            }
            "--waves" if i + 1 < args.len() => {
                waves_key = args[i + 1].clone();
                i += 2;
            }
            "--max-w" if i + 1 < args.len() => {
                max_w_arg = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "--max-h" if i + 1 < args.len() => {
                max_h_arg = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            "--max-fps" if i + 1 < args.len() => {
                max_fps = args[i + 1].parse().unwrap_or(60).clamp(15, 60);
                i += 2;
            }
            "--perf" if i + 1 < args.len() => {
                perf_mode = args[i + 1] == "1" || args[i + 1].eq_ignore_ascii_case("true");
                i += 2;
            }
            "--paused" if i + 1 < args.len() => {
                start_paused = args[i + 1] == "1" || args[i + 1].eq_ignore_ascii_case("true");
                i += 2;
            }
            "--parent-pid" if i + 1 < args.len() => {
                parent_pid = args[i + 1].parse().unwrap_or(0);
                i += 2;
            }
            _ => {
                if video_path.is_empty() && waves_key.is_empty() && !args[i].starts_with("--") {
                    video_path = args[i].clone();
                }
                i += 1;
            }
        }
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let sw = GetSystemMetrics(SM_CXSCREEN).max(1280);
        let sh = GetSystemMetrics(SM_CYSCREEN).max(720);

        // Mode 3: Isolated Hardware GPU GradientWaves Shader (React Bits 1:1, 0% AuraShell CPU!)
        if !waves_key.is_empty() {
            let Some((hwnd, ipc_hwnd)) = create_worker_windows(sw, sh) else {
                return;
            };
            let settings = load_settings();
            gradient_waves::set_tuning(settings.waves_speed, settings.waves_relief);
            gradient_waves::set_custom_color(&settings.waves_custom_color);
            let preset = gradient_waves::preset_by_key(&waves_key).unwrap_or(&gradient_waves::PRESETS[0]);
            let color = gradient_waves::custom_color_rgb().unwrap_or(preset.wave_color);

            // Render at native resolution (capped to 1920x1080 normal or 1280x720 perf -> ultra-low GPU!)
            let rw = if perf_mode { (sw as u32).min(1280) } else { (sw as u32).min(1920) };
            let rh = if perf_mode { (sh as u32).min(720) } else { (sh as u32).min(1080) };

            let Ok(mut renderer) = GpuWavesRenderer::new_for_hwnd(hwnd, rw, rh, preset, color) else {
                let _ = DestroyWindow(hwnd);
                if !ipc_hwnd.is_invalid() {
                    let _ = DestroyWindow(ipc_hwnd);
                }
                return;
            };
            renderer.render_frame(0.016, 0);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            let frame_ms = if perf_mode { 33u32 } else { 16u32 };
            let m_set = to_wide("WM_AURAOS_SETTINGS_CHANGED");
            let wm_settings_changed = RegisterWindowMessageW(PCWSTR(m_set.as_ptr()));

            WP_RT.with(|cell| {
                *cell.borrow_mut() = Some(WpRuntime {
                    hwnd,
                    ipc_hwnd,
                    backend: WorkerBackend::Waves {
                        renderer,
                        last_tick: Instant::now(),
                        prev_lbtn_down: false,
                        frame_ms,
                        wm_settings_changed,
                    },
                    parent_pid,
                    paused: start_paused,
                    trim_countdown: 3,
                });
            });

            SetTimer(hwnd, TIMER_WATCHDOG, 1500, None);
            if !start_paused {
                SetTimer(hwnd, TIMER_WAVES_FRAME, frame_ms, None);
            }

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let _ = KillTimer(hwnd, TIMER_WAVES_FRAME);
            let _ = KillTimer(hwnd, TIMER_WATCHDOG);
            return;
        }

        // Mode 2: Isolated Native-Res 60 FPS Video Wallpaper Worker
        if video_path.is_empty() || !std::path::Path::new(&video_path).exists() {
            return;
        }
        let Ok(mut gpu) = GpuEngine::new() else {
            return;
        };
        let Some((hwnd, ipc_hwnd)) = create_worker_windows(sw, sh) else {
            return;
        };
        let Ok(mut surf) = WindowSurface::new(&gpu, hwnd, sw as u32, sh as u32) else {
            let _ = DestroyWindow(hwnd);
            return;
        };

        let wf = sw as f32;
        let hf = sh as f32;
        surf.draw(&mut gpu, |ctx| {
            ctx.fill_rrect(Rect::new(0.0, 0.0, wf, hf), 0.0, theme::CANVAS);
        });
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let target_w = if max_w_arg > 0 {
            max_w_arg
        } else if perf_mode {
            1280
        } else {
            (sw as u32).clamp(1280, 2560)
        };
        let target_h = if max_h_arg > 0 {
            max_h_arg
        } else if perf_mode {
            720
        } else {
            (sh as u32).clamp(720, 1440)
        };
        let target_fps = if perf_mode { max_fps.min(30) } else { max_fps.clamp(24, 60) };

        let frame_pending = Arc::new(AtomicBool::new(false));
        let pending_cb = frame_pending.clone();
        let hwnd_raw = hwnd.0 as isize;
        let on_frame: Box<dyn Fn() + Send + 'static> = Box::new(move || {
            if !pending_cb.swap(true, Ordering::AcqRel) {
                let _ = PostMessageW(
                    HWND(hwnd_raw as *mut std::ffi::c_void),
                    wallpaper::WM_AURA_WP_FRAME,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });

        let stream = NativeVideoStream::spawn_with_callback(
            &video_path,
            target_w,
            target_h,
            target_fps,
            Some(on_frame),
        );
        if start_paused {
            if let Some(vs) = &stream {
                vs.set_paused(true);
            }
        }

        WP_RT.with(|cell| {
            *cell.borrow_mut() = Some(WpRuntime {
                hwnd,
                ipc_hwnd,
                backend: WorkerBackend::Video {
                    gpu,
                    surf,
                    stream,
                    frame_pending,
                    frame_scratch: Vec::new(),
                    last_seq: 0,
                },
                parent_pid,
                paused: start_paused,
                trim_countdown: 3,
            });
        });

        SetTimer(hwnd, TIMER_WATCHDOG, 1500, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = KillTimer(hwnd, TIMER_WATCHDOG);
        WP_RT.with(|cell| {
            if let Ok(mut g) = cell.try_borrow_mut() {
                if let Some(mut rt) = g.take() {
                    if let WorkerBackend::Video { stream, .. } = &mut rt.backend {
                        if let Some(mut vs) = stream.take() {
                            vs.stop_now();
                        }
                    }
                }
            }
        });
    }
}
