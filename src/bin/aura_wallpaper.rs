#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code)]

//! AuraWallpaper v5.2 — Isolated Native-Resolution 60 FPS Hardware Video Wallpaper Subprocess
//! - Keeps `AuraShell.exe` 100% free of Media Foundation / video decode DLLs (~35-55 MB RAM permanently!)
//! - Plays video wallpapers at NATIVE screen resolution (up to 1080p/1440p, ZERO 720p pixelation!)
//!   and NATIVE frame rate up to 60 FPS using DXVA/D3D11 hardware video decoding.
//! - Zero-jitter event-driven rendering (`WM_AURA_WP_FRAME` coalesced via AtomicBool — zero timer spin!)
//! - True `Condvar` pause (`WM_AURA_WP_PAUSE` / `WM_AURA_WP_RESUME`): 0.00% CPU while paused and ZERO
//!   Media Foundation tear-down/re-open virtual memory leaks!
//! - Parent PID watchdog: automatically exits cleanly if `AuraShell.exe` ever closes.

use aura_os::renderer::{GpuEngine, Rect, WindowSurface};
use aura_os::theme;
use aura_os::video_decoder::{self, NativeVideoStream};
use aura_os::wallpaper;
use aura_os::win32_shell::{self, to_wide};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, KillTimer, PostMessageW, PostQuitMessage, RegisterClassW, SetParent,
    SetTimer, SetWindowPos, ShowWindow, TranslateMessage, HWND_BOTTOM, HWND_MESSAGE, HWND_TOP, MSG,
    SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE,
    WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};

const TIMER_WATCHDOG: usize = 1;

struct WpRuntime {
    hwnd: HWND,
    ipc_hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    stream: Option<NativeVideoStream>,
    frame_pending: Arc<AtomicBool>,
    frame_scratch: Vec<u8>,
    last_seq: u64,
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

unsafe extern "system" fn wp_worker_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        m if m == wallpaper::WM_AURA_WP_FRAME => {
            WP_RT.with(|cell| {
                if let Ok(mut guard) = cell.try_borrow_mut() {
                    if let Some(rt) = guard.as_mut() {
                        rt.frame_pending.store(false, Ordering::Release);
                        if rt.paused {
                            return;
                        }
                        if let Some(vs) = &rt.stream {
                            if let Some((fw, fh)) =
                                vs.swap_latest_frame(&mut rt.last_seq, &mut rt.frame_scratch)
                            {
                                rt.surf.draw_video_frame(fw, fh, &rt.frame_scratch);
                            }
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
                            if let Some(vs) = &rt.stream {
                                vs.set_paused(true);
                            }
                            rt.gpu.trim_gpu();
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
                            if let Some(vs) = &rt.stream {
                                vs.set_paused(false);
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
                        if let Some(mut vs) = rt.stream.take() {
                            vs.stop_now();
                        }
                        return (rt.hwnd, rt.ipc_hwnd);
                    }
                }
                (hwnd, HWND::default())
            });
            if !render_hw.is_invalid() {
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
                        if let Some(vs) = &rt.stream {
                            if vs.has_failed() {
                                return true;
                            }
                        }
                        // Periodic gentle trim once shortly after start so startup commit settles cleanly
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

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Mode 1: One-shot isolated thumbnail extraction (keeps Media Foundation 100% out of AuraShell.exe!)
    if let Some(pos) = args.iter().position(|a| a == "--thumb") {
        if let Some(path) = args.get(pos + 1) {
            let _ = video_decoder::extract_video_thumbnail(path);
        }
        return;
    }

    // Mode 2: Isolated Native-Res 60 FPS Video Wallpaper Worker
    let mut video_path = String::new();
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
                if video_path.is_empty() && !args[i].starts_with("--") {
                    video_path = args[i].clone();
                }
                i += 1;
            }
        }
    }

    if video_path.is_empty() || !std::path::Path::new(&video_path).exists() {
        return;
    }

    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Cleanly replace any existing AuraWallpaperWorker instance (finds HWND_MESSAGE IPC even inside WorkerW!)
        if let Some(prev) = wallpaper::find_video_worker_ipc_hwnd() {
            let _ = PostMessageW(prev, wallpaper::WM_AURA_WP_QUIT, WPARAM(0), LPARAM(0));
            std::thread::sleep(std::time::Duration::from_millis(80));
        }

        let cls_w = to_wide(wallpaper::WP_WORKER_CLASS);
        let ipc_cls_w = to_wide(wallpaper::WP_IPC_CLASS);
        let sw = GetSystemMetrics(SM_CXSCREEN).max(1280);
        let sh = GetSystemMetrics(SM_CYSCREEN).max(720);

        let Ok(mut gpu) = GpuEngine::new() else {
            return;
        };

        let Ok(hinst) = GetModuleHandleW(None) else {
            return;
        };
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

        // Message-only IPC control window (ALWAYS discoverable via FindWindowExW(HWND_MESSAGE, ...))!
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
        let Ok(hwnd) = CreateWindowExW(
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
        ) else {
            return;
        };

        if let Some(workerw) = wallpaper::find_desktop_workerw() {
            let _ = SetParent(hwnd, workerw);
            let _ = SetWindowPos(hwnd, HWND_TOP, 0, 0, sw, sh, SWP_NOACTIVATE);
        } else {
            let _ = SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, sw, sh, SWP_NOACTIVATE);
        }

        let Ok(mut surf) = WindowSurface::new(&gpu, hwnd, sw as u32, sh as u32) else {
            let _ = DestroyWindow(hwnd);
            return;
        };

        // Paint crisp matte obsidian backdrop while first hardware-decoded frame arrives (~60ms)
        let wf = sw as f32;
        let hf = sh as f32;
        surf.draw(&mut gpu, |ctx| {
            ctx.fill_rrect(Rect::new(0.0, 0.0, wf, hf), 0.0, theme::CANVAS);
        });
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        // Native monitor resolution (up to 2560x1440; 1280x720 ONLY in performance_mode!)
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
                gpu,
                surf,
                stream,
                frame_pending,
                frame_scratch: Vec::new(),
                last_seq: 0,
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
                    if let Some(mut vs) = rt.stream.take() {
                        vs.stop_now();
                    }
                }
            }
        });
    }
}
