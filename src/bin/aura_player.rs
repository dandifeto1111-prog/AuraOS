//! AuraPlayer v4.0 — Pure Rust + Direct2D GPU + FFmpeg + Win32 waveOut Video & Music Player
//! - Plays ALL formats (.mp4, .mkv, .webm, .mov, .avi, .mp3, .wav, .flac, .ogg) in Drool Dark Mode
//! - Zero-allocation 60 FPS GPU Direct2D frame streaming + synchronized 48kHz stereo waveOut audio
//! - Live seekable scrub bar, speed control (0.5x–2.0x), volume/mute, fullscreen, folder playlist,
//!   live PCM audio spectrum visualizer for music files, and 1-click "Usar como Fondo" for AuraOS!

#![windows_subsystem = "windows"]

use aura_os::aura_window::{self, SnapZone};
use aura_os::models::{load_settings, save_settings};
use aura_os::renderer::{DrawCtx, GpuEngine, Rect, TextStyle, WindowSurface};
use aura_os::theme::{self, VectorIcon};
use aura_os::win32_shell;
use std::cell::RefCell;
use std::ffi::c_void;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    IsZoomed, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW, SetTimer, ShowWindow,
    TranslateMessage, HTCAPTION, IDC_ARROW, MSG, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCALCSIZE, WM_NCHITTEST, WM_NCLBUTTONDOWN,
    WM_RBUTTONDOWN, WM_SIZE, WM_SYSKEYDOWN, WM_TIMER, WNDCLASSW, WS_EX_NOREDIRECTIONBITMAP,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;
const TIMER_FRAME: usize = 30;
const TOP_BAR_H: f32 = 44.0;
const CTRL_BAR_H: f32 = 86.0;

// ==================== MINIMAL WIN32 WINMM.DLL WAVEOUT STEREO ENGINE ====================

#[repr(C)]
#[derive(Clone, Copy)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    cb_size: u16,
}

#[repr(C)]
struct WaveHdr {
    lp_data: *mut u8,
    dw_buffer_length: u32,
    dw_bytes_recorded: u32,
    dw_user: usize,
    dw_flags: u32,
    dw_loops: u32,
    lp_next: *mut WaveHdr,
    reserved: usize,
}

const WHDR_DONE: u32 = 0x00000001;
const WAVE_MAPPER: u32 = 0xFFFF_FFFF;

#[link(name = "winmm")]
extern "system" {
    fn waveOutOpen(
        phwo: *mut *mut c_void,
        u_device_id: u32,
        pwfx: *const WaveFormatEx,
        dw_callback: usize,
        dw_instance: usize,
        fdw_open: u32,
    ) -> u32;
    fn waveOutPrepareHeader(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    fn waveOutUnprepareHeader(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    fn waveOutWrite(hwo: *mut c_void, pwh: *mut WaveHdr, cbwh: u32) -> u32;
    fn waveOutPause(hwo: *mut c_void) -> u32;
    fn waveOutRestart(hwo: *mut c_void) -> u32;
    fn waveOutReset(hwo: *mut c_void) -> u32;
    fn waveOutClose(hwo: *mut c_void) -> u32;
    fn waveOutSetVolume(hwo: *mut c_void, dw_volume: u32) -> u32;
}

// ==================== MEDIA PROBE & STREAMING ENGINE ====================

#[derive(Clone, Default)]
struct MediaInfo {
    duration: f64,
    src_w: u32,
    src_h: u32,
    dec_w: u32,
    dec_h: u32,
    fps: f64,
    has_video: bool,
    has_audio: bool,
    codec: String,
}

struct SharedFrame {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
    seq: u64,
    audio_bars: [f32; 28],
    eof: bool,
}

struct PlaybackSession {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    vol_pct: Arc<AtomicU32>,
    muted: Arc<AtomicBool>,
    children: Arc<Mutex<Vec<Child>>>,
    frame: Arc<Mutex<SharedFrame>>,
    base_sec: f64,
    speed: f64,
    started_at: Instant,
    accum_sec: f64,
}

impl PlaybackSession {
    fn current_time(&self, duration: f64) -> f64 {
        let elapsed = if self.paused.load(Ordering::Relaxed) {
            self.accum_sec
        } else {
            self.accum_sec + self.started_at.elapsed().as_secs_f64() * self.speed
        };
        let t = self.base_sec + elapsed;
        if duration > 0.0 { t.clamp(0.0, duration) } else { t.max(0.0) }
    }

    fn set_paused(&mut self, p: bool) {
        let was = self.paused.swap(p, Ordering::Relaxed);
        if !was && p {
            self.accum_sec += self.started_at.elapsed().as_secs_f64() * self.speed;
        } else if was && !p {
            self.started_at = Instant::now();
        }
    }

    fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut list) = self.children.lock() {
            for ch in list.iter_mut() {
                let _ = ch.kill();
                let _ = ch.wait();
            }
            list.clear();
        }
    }
}

impl Drop for PlaybackSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn probe_media(path: &Path) -> MediaInfo {
    let mut info = MediaInfo {
        fps: 30.0,
        ..Default::default()
    };
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let audio_only_ext = win32_shell::is_audio_ext(ext);

    let Some(ffprobe) = win32_shell::find_ffmpeg_tool("ffprobe.exe") else {
        info.has_video = !audio_only_ext;
        info.has_audio = true;
        info.dec_w = 1280;
        info.dec_h = 720;
        return info;
    };

    let out = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,codec_name,width,height,r_frame_rate",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(path)
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(o) = out {
        let txt = String::from_utf8_lossy(&o.stdout);
        let mut cur_type = String::new();
        for line in txt.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("codec_type=") {
                cur_type = v.to_string();
                if v == "video" && !audio_only_ext {
                    info.has_video = true;
                } else if v == "audio" {
                    info.has_audio = true;
                }
            } else if let Some(v) = line.strip_prefix("duration=") {
                if let Ok(d) = v.parse::<f64>() {
                    if d.is_finite() && d > info.duration {
                        info.duration = d;
                    }
                }
            } else if cur_type == "video" && !audio_only_ext {
                if let Some(v) = line.strip_prefix("width=") {
                    if let Ok(w) = v.parse::<u32>() {
                        if w > 0 { info.src_w = w; }
                    }
                } else if let Some(v) = line.strip_prefix("height=") {
                    if let Ok(h) = v.parse::<u32>() {
                        if h > 0 { info.src_h = h; }
                    }
                } else if let Some(v) = line.strip_prefix("codec_name=") {
                    if info.codec.is_empty() {
                        info.codec = v.to_uppercase();
                    }
                } else if let Some(v) = line.strip_prefix("r_frame_rate=") {
                    if let Some((a, b)) = v.split_once('/') {
                        if let (Ok(na), Ok(nb)) = (a.parse::<f64>(), b.parse::<f64>()) {
                            if nb > 0.0 && na > 0.0 {
                                info.fps = (na / nb).clamp(12.0, 60.0);
                            }
                        }
                    }
                }
            } else if cur_type == "audio" && info.codec.is_empty() {
                if let Some(v) = line.strip_prefix("codec_name=") {
                    info.codec = v.to_uppercase();
                }
            }
        }
    }

    if info.has_video {
        let sw = if info.src_w > 0 { info.src_w } else { 1280 };
        let sh = if info.src_h > 0 { info.src_h } else { 720 };
        let scale = (1280.0 / sw as f64).min(720.0 / sh as f64).min(1.0);
        let dw = (((sw as f64 * scale) as u32) / 2 * 2).max(64);
        let dh = (((sh as f64 * scale) as u32) / 2 * 2).max(64);
        info.dec_w = dw;
        info.dec_h = dh;
    } else {
        info.has_audio = true;
    }
    info
}

fn spawn_playback_session(
    path: &Path,
    info: &MediaInfo,
    start_sec: f64,
    speed: f64,
    vol_pct: u32,
    muted: bool,
    start_paused: bool,
) -> Option<PlaybackSession> {
    let stop = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(start_paused));
    let vol_atom = Arc::new(AtomicU32::new(vol_pct));
    let mute_atom = Arc::new(AtomicBool::new(muted));
    let children: Arc<Mutex<Vec<Child>>> = Arc::new(Mutex::new(Vec::new()));
    let frame = Arc::new(Mutex::new(SharedFrame {
        w: info.dec_w,
        h: info.dec_h,
        bgra: Vec::new(),
        seq: 0,
        audio_bars: [0.0; 28],
        eof: false,
    }));

    let Some(ffmpeg) = win32_shell::find_ffmpeg_tool("ffmpeg.exe") else {
        // Zero-dependency Native Windows Media Foundation fallback when ffmpeg.exe is not installed!
        if info.has_video {
            let p_str = path.to_string_lossy().to_string();
            if let Some(mut mf) = aura_os::video_decoder::NativeVideoStream::spawn(&p_str, 1280, 720, 30) {
                let stop_mf = stop.clone();
                let pause_mf = paused.clone();
                let fr_mf = frame.clone();
                std::thread::spawn(move || {
                    let mut seq = 0u64;
                    let mut scratch = Vec::new();
                    while !stop_mf.load(Ordering::Relaxed) {
                        if !pause_mf.load(Ordering::Relaxed) {
                            if let Some((w, h)) = mf.swap_latest_frame(&mut seq, &mut scratch) {
                                if let Ok(mut f) = fr_mf.lock() {
                                    f.w = w;
                                    f.h = h;
                                    std::mem::swap(&mut f.bgra, &mut scratch);
                                    f.seq = f.seq.wrapping_add(1);
                                }
                            }
                        }
                        std::thread::sleep(Duration::from_millis(16));
                    }
                    mf.stop_now();
                });
                return Some(PlaybackSession {
                    stop,
                    paused,
                    vol_pct: vol_atom,
                    muted: mute_atom,
                    children,
                    frame,
                    base_sec: start_sec,
                    speed,
                    started_at: Instant::now(),
                    accum_sec: 0.0,
                });
            }
        }
        return None;
    };

    let seek_str = format!("{:.3}", start_sec.max(0.0));

    // 1. Spawn Video BGRA Decoder Thread
    if info.has_video && info.dec_w > 0 && info.dec_h > 0 {
        let ff_v = ffmpeg.clone();
        let p_v = path.to_path_buf();
        let stop_v = stop.clone();
        let pause_v = paused.clone();
        let ch_v = children.clone();
        let fr_v = frame.clone();
        let dw = info.dec_w;
        let dh = info.dec_h;
        let fps = info.fps.clamp(15.0, 60.0);
        let seek_v = seek_str.clone();
        let spd_v = speed;

        std::thread::spawn(move || {
            let mut cmd = Command::new(&ff_v);
            cmd.args([
                "-hide_banner",
                "-loglevel",
                "quiet",
                "-ss",
                &seek_v,
                "-i",
            ])
            .arg(&p_v)
            .args([
                "-an",
                "-sn",
                "-vf",
                &format!("scale={}:{}:flags=fast_bilinear", dw, dh),
                "-pix_fmt",
                "bgra",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);

            let Ok(mut child) = cmd.spawn() else { return };
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill();
                return;
            };
            if let Ok(mut list) = ch_v.lock() {
                list.push(child);
            }

            let frame_bytes = dw as usize * dh as usize * 4;
            let mut buf = vec![0u8; frame_bytes];
            let frame_dur = Duration::from_secs_f64(1.0 / (fps * spd_v).max(1.0));
            let mut next_due = Instant::now();

            while !stop_v.load(Ordering::Relaxed) {
                while pause_v.load(Ordering::Relaxed) && !stop_v.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(20));
                    next_due = Instant::now();
                }
                if stop_v.load(Ordering::Relaxed) {
                    break;
                }
                if stdout.read_exact(&mut buf).is_err() {
                    if let Ok(mut f) = fr_v.lock() {
                        f.eof = true;
                    }
                    break;
                }
                if let Ok(mut f) = fr_v.lock() {
                    f.w = dw;
                    f.h = dh;
                    if f.bgra.len() != frame_bytes {
                        f.bgra.resize(frame_bytes, 0);
                    }
                    f.bgra.copy_from_slice(&buf);
                    f.seq = f.seq.wrapping_add(1);
                }
                next_due += frame_dur;
                let now = Instant::now();
                if next_due > now {
                    std::thread::sleep(next_due - now);
                } else if now - next_due > Duration::from_millis(250) {
                    next_due = now;
                }
            }
        });
    }

    // 2. Spawn Synchronized Stereo PCM Audio Thread (Win32 waveOut!)
    if info.has_audio {
        let ff_a = ffmpeg;
        let p_a = path.to_path_buf();
        let stop_a = stop.clone();
        let pause_a = paused.clone();
        let vol_a = vol_atom.clone();
        let mute_a = mute_atom.clone();
        let ch_a = children.clone();
        let fr_a = frame.clone();
        let seek_a = seek_str;
        let spd_a = speed;
        let audio_only = !info.has_video;

        std::thread::spawn(move || {
            let mut cmd = Command::new(&ff_a);
            cmd.args(["-hide_banner", "-loglevel", "quiet", "-ss", &seek_a, "-i"])
                .arg(&p_a)
                .args(["-vn", "-sn"]);
            if (spd_a - 1.0).abs() > 0.01 {
                cmd.args(["-af", &format!("atempo={:.2}", spd_a.clamp(0.5, 2.0))]);
            }
            cmd.args(["-ac", "2", "-ar", "48000", "-f", "s16le", "pipe:1"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .creation_flags(CREATE_NO_WINDOW);

            let Ok(mut child) = cmd.spawn() else { return };
            let Some(mut stdout) = child.stdout.take() else {
                let _ = child.kill();
                return;
            };
            if let Ok(mut list) = ch_a.lock() {
                list.push(child);
            }

            let wfx = WaveFormatEx {
                format_tag: 1, // WAVE_FORMAT_PCM
                channels: 2,
                samples_per_sec: 48000,
                avg_bytes_per_sec: 48000 * 4,
                block_align: 4,
                bits_per_sample: 16,
                cb_size: 0,
            };

            let mut hwo: *mut c_void = std::ptr::null_mut();
            let open_res = unsafe { waveOutOpen(&mut hwo, WAVE_MAPPER, &wfx, 0, 0, 0) };
            if open_res != 0 || hwo.is_null() {
                return;
            }

            const SLOTS: usize = 6;
            const CHUNK: usize = 8192; // ~42ms stereo 48kHz
            let mut bufs: Vec<Vec<u8>> = (0..SLOTS).map(|_| vec![0u8; CHUNK]).collect();
            let mut hdrs: Vec<WaveHdr> = (0..SLOTS)
                .map(|_| WaveHdr {
                    lp_data: std::ptr::null_mut(),
                    dw_buffer_length: 0,
                    dw_bytes_recorded: 0,
                    dw_user: 0,
                    dw_flags: WHDR_DONE,
                    dw_loops: 0,
                    lp_next: std::ptr::null_mut(),
                    reserved: 0,
                })
                .collect();
            let mut prepared = [false; SLOTS];
            let mut slot = 0usize;
            let mut was_paused = false;
            let mut last_vol_word = u32::MAX;

            if pause_a.load(Ordering::Relaxed) {
                unsafe { waveOutPause(hwo); }
                was_paused = true;
            }

            let mut tmp = vec![0u8; CHUNK];
            while !stop_a.load(Ordering::Relaxed) {
                // Apply volume/mute instantaneously via waveOutSetVolume
                let is_m = mute_a.load(Ordering::Relaxed);
                let vp = if is_m { 0 } else { vol_a.load(Ordering::Relaxed).min(100) };
                let ch_v = ((vp * 0xFFFF) / 100) as u32;
                let vol_word = ch_v | (ch_v << 16);
                if vol_word != last_vol_word {
                    unsafe { waveOutSetVolume(hwo, vol_word); }
                    last_vol_word = vol_word;
                }

                // Honor Pause/Resume instantly
                let is_p = pause_a.load(Ordering::Relaxed);
                if is_p != was_paused {
                    unsafe {
                        if is_p { waveOutPause(hwo); } else { waveOutRestart(hwo); }
                    }
                    was_paused = is_p;
                }
                if is_p {
                    std::thread::sleep(Duration::from_millis(16));
                    continue;
                }

                // Wait for next ring slot to finish playback
                while (hdrs[slot].dw_flags & WHDR_DONE) == 0 && !stop_a.load(Ordering::Relaxed) {
                    if pause_a.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(6));
                }
                if stop_a.load(Ordering::Relaxed) || pause_a.load(Ordering::Relaxed) {
                    continue;
                }

                if prepared[slot] {
                    unsafe {
                        waveOutUnprepareHeader(hwo, &mut hdrs[slot], std::mem::size_of::<WaveHdr>() as u32);
                    }
                    prepared[slot] = false;
                }

                let mut read_total = 0usize;
                while read_total < CHUNK && !stop_a.load(Ordering::Relaxed) {
                    match stdout.read(&mut tmp[read_total..]) {
                        Ok(0) => break,
                        Ok(n) => read_total += n,
                        Err(_) => break,
                    }
                }
                if read_total == 0 {
                    if audio_only {
                        if let Ok(mut f) = fr_a.lock() {
                            f.eof = true;
                        }
                    }
                    break;
                }

                // Compute live 28-band energy bars directly from PCM for music visualizer!
                if audio_only {
                    let samples = read_total / 4;
                    let step = (samples / 28).max(1);
                    let mut bars = [0.0f32; 28];
                    for (b, out_bar) in bars.iter_mut().enumerate() {
                        let start_s = b * step;
                        let end_s = ((b + 1) * step).min(samples);
                        let mut acc = 0.0f32;
                        let mut cnt = 0;
                        for s_i in start_s..end_s {
                            let off = s_i * 4;
                            let l = i16::from_le_bytes([tmp[off], tmp[off + 1]]) as f32 / 32768.0;
                            let r = i16::from_le_bytes([tmp[off + 2], tmp[off + 3]]) as f32 / 32768.0;
                            acc += (l.abs() + r.abs()) * 0.5;
                            cnt += 1;
                        }
                        *out_bar = if cnt > 0 { (acc / cnt as f32 * 3.4).clamp(0.06, 1.0) } else { 0.08 };
                    }
                    if let Ok(mut f) = fr_a.lock() {
                        for i in 0..28 {
                            f.audio_bars[i] = f.audio_bars[i] * 0.45 + bars[i] * 0.55;
                        }
                    }
                }

                bufs[slot][..read_total].copy_from_slice(&tmp[..read_total]);
                hdrs[slot] = WaveHdr {
                    lp_data: bufs[slot].as_mut_ptr(),
                    dw_buffer_length: read_total as u32,
                    dw_bytes_recorded: 0,
                    dw_user: 0,
                    dw_flags: 0,
                    dw_loops: 0,
                    lp_next: std::ptr::null_mut(),
                    reserved: 0,
                };
                unsafe {
                    waveOutPrepareHeader(hwo, &mut hdrs[slot], std::mem::size_of::<WaveHdr>() as u32);
                    prepared[slot] = true;
                    waveOutWrite(hwo, &mut hdrs[slot], std::mem::size_of::<WaveHdr>() as u32);
                }
                slot = (slot + 1) % SLOTS;
            }

            unsafe {
                waveOutReset(hwo);
                for i in 0..SLOTS {
                    if prepared[i] {
                        waveOutUnprepareHeader(hwo, &mut hdrs[i], std::mem::size_of::<WaveHdr>() as u32);
                    }
                }
                waveOutClose(hwo);
            }
        });
    }

    Some(PlaybackSession {
        stop,
        paused,
        vol_pct: vol_atom,
        muted: mute_atom,
        children,
        frame,
        base_sec: start_sec,
        speed,
        started_at: Instant::now(),
        accum_sec: 0.0,
    })
}

// ==================== UI ACTIONS & RUNTIME ====================

#[derive(Clone)]
enum PlayerAct {
    WinClose,
    WinMin,
    WinMax,
    OpenDialog,
    TogglePlay,
    SeekRelative(f64),
    SeekTrack { x: f32, w: f32 },
    VolTrack { x: f32, w: f32 },
    ToggleMute,
    CycleSpeed,
    ToggleLoop,
    PrevMedia,
    NextMedia,
    SetLiveWallpaper,
}

#[derive(Clone)]
struct Hit {
    id: String,
    rect: Rect,
    act: PlayerAct,
}

struct PlayerRuntime {
    hwnd: HWND,
    gpu: GpuEngine,
    surf: WindowSurface,
    width: f32,
    height: f32,
    maximized: bool,

    playlist: Vec<PathBuf>,
    cur_idx: usize,
    info: MediaInfo,
    session: Option<PlaybackSession>,
    last_frame_seq: u64,

    paused: bool,
    vol_pct: u32,
    muted: bool,
    speed: f64,
    loop_enabled: bool,

    dragging_seek: Option<(f32, f32)>,
    dragging_vol: Option<(f32, f32)>,
    hovered_id: Option<String>,
    hits: Vec<Hit>,
    toast: Option<(String, Instant)>,
    phase: f32,
}

thread_local! {
    static PLAYER_RT: RefCell<Option<PlayerRuntime>> = const { RefCell::new(None) };
}

fn format_time(sec: f64) -> String {
    let s = sec.max(0.0).round() as u64;
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let ss = s % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, ss)
    } else {
        format!("{:02}:{:02}", m, ss)
    }
}

fn scan_sibling_media(path: &Path) -> (Vec<PathBuf>, usize) {
    let Some(parent) = path.parent() else {
        return (vec![path.to_path_buf()], 0);
    };
    let mut list: Vec<PathBuf> = std::fs::read_dir(parent)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| win32_shell::is_video_ext(ext) || win32_shell::is_audio_ext(ext))
                    .unwrap_or(false)
        })
        .collect();
    list.sort_by(|a, b| {
        a.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
            .cmp(&b.file_name().unwrap_or_default().to_string_lossy().to_lowercase())
    });
    let low = path.to_string_lossy().to_lowercase();
    let idx = list.iter().position(|p| p.to_string_lossy().to_lowercase() == low).unwrap_or(0);
    if list.is_empty() {
        (vec![path.to_path_buf()], 0)
    } else {
        (list, idx)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1].eq_ignore_ascii_case("--capture") {
        let out_path = PathBuf::from(&args[2]);
        let sample = args.get(3).map(PathBuf::from).or_else(find_sample_video);
        run_capture(&out_path, sample);
        return;
    }

    let initial_file = args.get(1).map(PathBuf::from).filter(|p| p.exists());

    let gpu = match GpuEngine::new() {
        Ok(g) => g,
        Err(_) => return,
    };

    let (sw, sh) = win32_shell::screen_size();
    let w = (sw as f32 * 0.72).clamp(960.0, 1360.0) as i32;
    let h = (sh as f32 * 0.76).clamp(620.0, 860.0) as i32;
    let x = (sw - w) / 2;
    let y = (sh - h) / 2;

    let cls = win32_shell::to_wide("AuraPlayer_Rust_v4_Wnd");
    let title = win32_shell::to_wide("AuraPlayer");
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(player_wnd_proc),
            hInstance: hinst.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: PCWSTR(cls.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP,
            PCWSTR(cls.as_ptr()),
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

    let Ok(surf) = WindowSurface::new(&gpu, hwnd, w as u32, h as u32) else {
        return;
    };

    let mut rt = PlayerRuntime {
        hwnd,
        gpu,
        surf,
        width: w as f32,
        height: h as f32,
        maximized: false,
        playlist: Vec::new(),
        cur_idx: 0,
        info: MediaInfo::default(),
        session: None,
        last_frame_seq: 0,
        paused: false,
        vol_pct: 85,
        muted: false,
        speed: 1.0,
        loop_enabled: false,
        dragging_seek: None,
        dragging_vol: None,
        hovered_id: None,
        hits: Vec::new(),
        toast: None,
        phase: 0.0,
    };

    if let Some(p) = initial_file {
        rt.open_media(&p);
    }
    rt.redraw();

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        SetTimer(hwnd, TIMER_FRAME, 16, None);
    }

    PLAYER_RT.with(|c| *c.borrow_mut() = Some(rt));

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    PLAYER_RT.with(|c| {
        if let Ok(mut g) = c.try_borrow_mut() {
            if let Some(r) = g.as_mut() {
                r.stop_session();
            }
            *g = None;
        }
    });
}

fn find_sample_video() -> Option<PathBuf> {
    if let Ok(prof) = std::env::var("USERPROFILE") {
        for sub in ["Downloads", "Videos"] {
            let dir = PathBuf::from(&prof).join(sub);
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_file()
                        && p.extension()
                            .and_then(|x| x.to_str())
                            .map(win32_shell::is_video_ext)
                            .unwrap_or(false)
                    {
                        return Some(p);
                    }
                }
            }
        }
    }
    let ws = PathBuf::from(r"D:\SteamLibrary\steamapps\workshop\content\431960");
    if let Ok(rd) = std::fs::read_dir(&ws) {
        for e in rd.flatten() {
            if let Ok(sub) = std::fs::read_dir(e.path()) {
                for f in sub.flatten() {
                    let p = f.path();
                    if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("mp4")).unwrap_or(false) {
                        return Some(p);
                    }
                }
            }
        }
    }
    None
}

fn grab_single_video_frame(path: &Path, info: &MediaInfo) -> Option<Vec<u8>> {
    let ffmpeg = win32_shell::find_ffmpeg_tool("ffmpeg.exe")?;
    if !info.has_video || info.dec_w == 0 || info.dec_h == 0 {
        return None;
    }
    let seek = if info.duration > 3.0 { "2.0" } else { "0.2" };
    let mut child = Command::new(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "quiet", "-ss", seek, "-i"])
        .arg(path)
        .args([
            "-frames:v",
            "1",
            "-vf",
            &format!("scale={}:{}", info.dec_w, info.dec_h),
            "-pix_fmt",
            "bgra",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let expected = info.dec_w as usize * info.dec_h as usize * 4;
    let mut buf = vec![0u8; expected];
    let ok = stdout.read_exact(&mut buf).is_ok();
    let _ = child.kill();
    let _ = child.wait();
    if ok { Some(buf) } else { None }
}

fn run_capture(out_path: &Path, sample: Option<PathBuf>) {
    let Ok(mut gpu) = GpuEngine::new() else { return };
    let (playlist, idx) = sample.as_deref().map(scan_sibling_media).unwrap_or_default();
    let info = playlist.get(idx).map(|p| probe_media(p)).unwrap_or_default();
    let bgra = playlist
        .get(idx)
        .and_then(|p| grab_single_video_frame(p, &info))
        .unwrap_or_default();
    let bars = [0.22; 28];

    let _ = gpu.render_to_png(1240, 760, out_path, |ctx| {
        let mut hits = Vec::new();
        render_player_ui(
            ctx,
            1240.0,
            760.0,
            false,
            &playlist,
            idx,
            &info,
            if bgra.is_empty() { None } else { Some((info.dec_w, info.dec_h, bgra.as_slice())) },
            &bars,
            info.duration * 0.28,
            false,
            85,
            false,
            1.0,
            true,
            None,
            None,
            1.4,
            &mut hits,
        );
    });
}

impl PlayerRuntime {
    fn stop_session(&mut self) {
        if let Some(s) = self.session.take() {
            s.shutdown();
        }
    }

    fn open_media(&mut self, path: &Path) {
        self.stop_session();
        let (pl, idx) = scan_sibling_media(path);
        self.playlist = pl;
        self.cur_idx = idx;
        self.start_current_at(0.0);
    }

    fn start_current_at(&mut self, sec: f64) {
        self.stop_session();
        let Some(p) = self.playlist.get(self.cur_idx).cloned() else { return };
        self.info = probe_media(&p);
        self.paused = false;
        self.last_frame_seq = 0;
        self.session = spawn_playback_session(
            &p,
            &self.info,
            sec.clamp(0.0, self.info.duration.max(0.0)),
            self.speed,
            self.vol_pct,
            self.muted,
            false,
        );
        if self.session.is_none() {
            self.toast = Some(("No se encontró ffmpeg.exe para decodificar el archivo".into(), Instant::now()));
        }
    }

    fn seek_to(&mut self, target_sec: f64) {
        let Some(p) = self.playlist.get(self.cur_idx).cloned() else { return };
        let clamped = if self.info.duration > 0.0 {
            target_sec.clamp(0.0, (self.info.duration - 0.15).max(0.0))
        } else {
            target_sec.max(0.0)
        };
        let was_paused = self.paused;
        self.stop_session();
        self.session = spawn_playback_session(
            &p,
            &self.info,
            clamped,
            self.speed,
            self.vol_pct,
            self.muted,
            was_paused,
        );
    }

    fn execute(&mut self, act: PlayerAct, click_x: f32) {
        match act {
            PlayerAct::WinClose => {
                self.stop_session();
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                    PostQuitMessage(0);
                }
                return;
            }
            PlayerAct::WinMin => unsafe { let _ = ShowWindow(self.hwnd, SW_MINIMIZE); },
            PlayerAct::WinMax => unsafe {
                self.maximized = !self.maximized;
                let _ = ShowWindow(self.hwnd, if self.maximized { SW_MAXIMIZE } else { SW_RESTORE });
            },
            PlayerAct::OpenDialog => {
                if let Some(p) = win32_shell::pick_file_dialog(
                    "Abrir video o audio en AuraPlayer",
                    "Videos y Audio (*.mp4;*.mkv;*.webm;*.mov;*.avi;*.mp3;*.wav;*.flac)",
                    "*.mp4;*.mkv;*.webm;*.mov;*.avi;*.wmv;*.mp3;*.wav;*.flac;*.ogg;*.m4a",
                ) {
                    self.open_media(Path::new(&p));
                }
            }
            PlayerAct::TogglePlay => {
                self.paused = !self.paused;
                if let Some(s) = self.session.as_mut() {
                    s.set_paused(self.paused);
                }
            }
            PlayerAct::SeekRelative(delta) => {
                let cur = self.session.as_ref().map(|s| s.current_time(self.info.duration)).unwrap_or(0.0);
                self.seek_to(cur + delta);
            }
            PlayerAct::SeekTrack { x, w } => {
                let frac = ((click_x - x) / w.max(1.0)).clamp(0.0, 1.0) as f64;
                if self.info.duration > 0.0 {
                    self.seek_to(frac * self.info.duration);
                }
                self.dragging_seek = Some((x, w));
                unsafe { SetCapture(self.hwnd); }
            }
            PlayerAct::VolTrack { x, w } => {
                let pct = (((click_x - x) / w.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as u32;
                self.vol_pct = pct;
                self.muted = pct == 0;
                if let Some(s) = &self.session {
                    s.vol_pct.store(self.vol_pct, Ordering::Relaxed);
                    s.muted.store(self.muted, Ordering::Relaxed);
                }
                self.dragging_vol = Some((x, w));
                unsafe { SetCapture(self.hwnd); }
            }
            PlayerAct::ToggleMute => {
                self.muted = !self.muted;
                if let Some(s) = &self.session {
                    s.muted.store(self.muted, Ordering::Relaxed);
                }
            }
            PlayerAct::CycleSpeed => {
                self.speed = match self.speed {
                    s if s < 0.75 => 1.0,
                    s if s < 1.15 => 1.25,
                    s if s < 1.40 => 1.5,
                    s if s < 1.80 => 2.0,
                    _ => 0.5,
                };
                let cur = self.session.as_ref().map(|s| s.current_time(self.info.duration)).unwrap_or(0.0);
                if self.session.is_some() {
                    self.seek_to(cur);
                }
            }
            PlayerAct::ToggleLoop => {
                self.loop_enabled = !self.loop_enabled;
            }
            PlayerAct::PrevMedia => {
                if !self.playlist.is_empty() {
                    self.cur_idx = if self.cur_idx == 0 { self.playlist.len() - 1 } else { self.cur_idx - 1 };
                    self.start_current_at(0.0);
                }
            }
            PlayerAct::NextMedia => {
                if !self.playlist.is_empty() {
                    self.cur_idx = (self.cur_idx + 1) % self.playlist.len();
                    self.start_current_at(0.0);
                }
            }
            PlayerAct::SetLiveWallpaper => {
                if let Some(p) = self.playlist.get(self.cur_idx) {
                    if self.info.has_video {
                        let mut st = load_settings();
                        st.wallpaper_id = format!("video:{}", p.to_string_lossy());
                        save_settings(&st);
                        self.toast = Some(("Video guardado como fondo animado de AuraOS".into(), Instant::now()));
                    }
                }
            }
        }
        self.redraw();
    }

    fn tick(&mut self) {
        self.phase += 0.05;
        let mut need_redraw = false;
        let mut hit_eof = false;

        if let Some(s) = &self.session {
            if let Ok(f) = s.frame.lock() {
                if f.seq != self.last_frame_seq {
                    self.last_frame_seq = f.seq;
                    need_redraw = true;
                }
                if f.eof {
                    hit_eof = true;
                }
            }
            if !self.info.has_video && !self.paused {
                need_redraw = true;
            }
        }
        if hit_eof {
            if self.loop_enabled {
                self.start_current_at(0.0);
                need_redraw = true;
            } else if self.playlist.len() > 1 && self.cur_idx + 1 < self.playlist.len() {
                self.cur_idx += 1;
                self.start_current_at(0.0);
                need_redraw = true;
            } else {
                self.paused = true;
                need_redraw = true;
            }
        }
        if need_redraw {
            self.redraw();
        }
    }

    fn redraw(&mut self) {
        if let Some((_, ts)) = &self.toast {
            if ts.elapsed().as_secs_f32() > 3.2 {
                self.toast = None;
            }
        }

        let cur_sec = self.session.as_ref().map(|s| s.current_time(self.info.duration)).unwrap_or(0.0);
        let mut frame_copy: Option<(u32, u32, Vec<u8>)> = None;
        let mut bars = [0.08f32; 28];
        if let Some(s) = &self.session {
            if let Ok(f) = s.frame.lock() {
                bars = f.audio_bars;
                if !f.bgra.is_empty() && f.w > 0 && f.h > 0 {
                    frame_copy = Some((f.w, f.h, f.bgra.clone()));
                }
            }
        }

        let w = self.width;
        let h = self.height;
        let maxd = self.maximized;
        let pl = &self.playlist;
        let idx = self.cur_idx;
        let info = &self.info;
        let paused = self.paused;
        let vol = self.vol_pct;
        let muted = self.muted;
        let spd = self.speed;
        let lp = self.loop_enabled;
        let hov = self.hovered_id.as_deref();
        let toast_txt = self.toast.as_ref().map(|(s, _)| s.as_str());
        let phase = self.phase;
        let hits = &mut self.hits;

        self.surf.draw(&mut self.gpu, |ctx| {
            let fref = frame_copy.as_ref().map(|(fw, fh, buf)| (*fw, *fh, buf.as_slice()));
            render_player_ui(
                ctx, w, h, maxd, pl, idx, info, fref, &bars, cur_sec, paused, vol, muted, spd, lp,
                hov, toast_txt, phase, hits,
            );
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn render_player_ui(
    ctx: &mut DrawCtx<'_>,
    w: f32,
    h: f32,
    maximized: bool,
    playlist: &[PathBuf],
    cur_idx: usize,
    info: &MediaInfo,
    frame: Option<(u32, u32, &[u8])>,
    audio_bars: &[f32; 28],
    cur_sec: f64,
    paused: bool,
    vol_pct: u32,
    muted: bool,
    speed: f64,
    loop_enabled: bool,
    hovered_id: Option<&str>,
    toast: Option<&str>,
    phase: f32,
    hits: &mut Vec<Hit>,
) {
    hits.clear();
    let corner = if maximized { 0.0 } else { 18.0 };
    let bounds = Rect::new(0.0, 0.0, w, h);
    let _clip = if corner > 0.0 {
        ctx.push_rrect_mask_layer(bounds, bounds, corner, 1.0)
    } else {
        None
    };

    ctx.fill_rrect(bounds, corner, theme::rgba(8, 8, 12, 1.0));

    // Top Header Bar
    ctx.fill_rrect(Rect::new(0.0, 0.0, w, TOP_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, TOP_BAR_H, w, TOP_BAR_H, theme::BORDER, 1.0);

    let lights = [
        ("pl_close", 20.0, theme::CORAL, "×", PlayerAct::WinClose),
        ("pl_min", 40.0, theme::AMBER, "−", PlayerAct::WinMin),
        ("pl_max", 60.0, theme::EMERALD, "+", PlayerAct::WinMax),
    ];
    let any_light_hov = matches!(hovered_id, Some("pl_close") | Some("pl_min") | Some("pl_max"));
    for (id, cx, col, glyph, act) in lights {
        ctx.fill_circle(cx, 22.0, if any_light_hov { 6.3 } else { 5.8 }, col);
        if any_light_hov {
            ctx.draw_text_center(glyph, Rect::new(cx - 6.0, 15.0, 12.0, 13.0), TextStyle::Small9Reg, theme::DARK_INK);
        }
        hits.push(Hit { id: id.into(), rect: Rect::new(cx - 8.0, 13.0, 16.0, 18.0), act });
    }

    let title = playlist
        .get(cur_idx)
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "AuraPlayer".to_string());
    ctx.draw_vector_icon(
        if info.has_video || playlist.is_empty() { VectorIcon::Film } else { VectorIcon::MusicNote },
        90.0,
        22.0,
        13.0,
        theme::VIOLET_LIGHT,
    );
    ctx.draw_text(&title, Rect::new(104.0, 11.0, (w * 0.42).max(200.0), 22.0), TextStyle::Body12Semi, theme::TEXT_PRIMARY);

    // Right Header Pills: Media Spec + Set Wallpaper + Open
    let mut rx = w - 14.0;
    rx -= 76.0;
    let open_r = Rect::new(rx, 8.0, 76.0, 28.0);
    ctx.fill_card(open_r, 9.0, if hovered_id == Some("pl_open") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_text_center("Abrir...", open_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    hits.push(Hit { id: "pl_open".into(), rect: open_r, act: PlayerAct::OpenDialog });

    if info.has_video && !playlist.is_empty() {
        rx -= 118.0;
        let wp_r = Rect::new(rx, 8.0, 110.0, 28.0);
        let hov = hovered_id == Some("pl_wp");
        ctx.fill_card(wp_r, 9.0, if hov { theme::VIOLET_LIGHT } else { theme::VIOLET }, theme::VIOLET_BORDER);
        ctx.draw_text_center("Usar como fondo", wp_r, TextStyle::Body11Med, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "pl_wp".into(), rect: wp_r, act: PlayerAct::SetLiveWallpaper });
    }

    if !playlist.is_empty() {
        let badge_txt = if info.has_video && info.src_w > 0 {
            format!("{}×{} · {:.0} FPS · {}", info.src_w, info.src_h, info.fps, if info.codec.is_empty() { "VIDEO" } else { &info.codec })
        } else {
            format!("AUDIO · {}", if info.codec.is_empty() { "STEREO 48kHz" } else { &info.codec })
        };
        let bw = (badge_txt.len() as f32 * 6.5 + 22.0).clamp(110.0, 220.0);
        rx -= bw + 8.0;
        let br = Rect::new(rx, 10.0, bw, 24.0);
        ctx.fill_rrect(br, 8.0, theme::ELEVATED);
        ctx.draw_text_center(&badge_txt, br, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    }

    // ==================== VIDEO / VISUALIZER STAGE ====================
    let stage = Rect::new(0.0, TOP_BAR_H, w, (h - TOP_BAR_H - CTRL_BAR_H).max(160.0));
    ctx.push_clip(stage);

    if let Some((fw, fh, bgra)) = frame {
        let scale = (stage.w / fw as f32).min(stage.h / fh as f32);
        let dw = fw as f32 * scale;
        let dh = fh as f32 * scale;
        let dst = Rect::new(stage.x + (stage.w - dw) * 0.5, stage.y + (stage.h - dh) * 0.5, dw, dh);
        ctx.draw_bgra_live("player_live_frame", fw, fh, bgra, dst);
        hits.push(Hit { id: "stage_toggle".into(), rect: stage, act: PlayerAct::TogglePlay });

        if paused {
            let cx = stage.x + stage.w * 0.5;
            let cy = stage.y + stage.h * 0.5;
            ctx.fill_circle(cx, cy, 34.0, theme::rgba(13, 13, 20, 0.72));
            ctx.stroke_circle(cx, cy, 34.0, theme::VIOLET_LIGHT, 2.0);
            ctx.draw_vector_icon(VectorIcon::Play, cx + 2.0, cy, 24.0, theme::TEXT_PRIMARY);
        }
    } else if !playlist.is_empty() && !info.has_video {
        // Lush 60 FPS Audio Spectrum Visualizer!
        let cx = stage.x + stage.w * 0.5;
        let cy = stage.y + stage.h * 0.44;
        ctx.fill_radial_glow(cx, cy, 240.0, theme::rgba(124, 92, 255, 0.24), theme::rgba(124, 92, 255, 0.0));
        let card = Rect::new(cx - 240.0, cy - 110.0, 480.0, 220.0);
        ctx.fill_card(card, 20.0, theme::SURFACE, theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::MusicNote, cx, card.y + 42.0, 28.0, theme::VIOLET_LIGHT);
        ctx.draw_text_center(&title, Rect::new(card.x + 18.0, card.y + 66.0, card.w - 36.0, 24.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
        // 28 animated bars
        let bw = 8.0;
        let gap = 5.0;
        let total = 28.0 * bw + 27.0 * gap;
        let bx0 = cx - total * 0.5;
        let base_y = card.y + 190.0;
        for i in 0..28 {
            let wave = if paused { 0.12 } else { (audio_bars[i] + ((phase + i as f32 * 0.4).sin().abs() * 0.12)).clamp(0.08, 1.0) };
            let bh = 12.0 + wave * 74.0;
            let bar_r = Rect::new(bx0 + i as f32 * (bw + gap), base_y - bh, bw, bh);
            let col = if i % 3 == 0 { theme::VIOLET_LIGHT } else { theme::VIOLET };
            ctx.fill_rrect(bar_r, 4.0, col);
        }
        hits.push(Hit { id: "stage_toggle".into(), rect: stage, act: PlayerAct::TogglePlay });
    } else {
        // Empty Welcome State
        let card = Rect::new((w - 380.0) * 0.5, stage.y + (stage.h - 178.0) * 0.5, 380.0, 178.0);
        ctx.fill_card(card, 18.0, theme::SURFACE, theme::BORDER);
        ctx.draw_vector_icon(VectorIcon::Film, card.x + 190.0, card.y + 44.0, 28.0, theme::VIOLET_LIGHT);
        ctx.draw_text_center("AuraPlayer · Cine & Audio GPU", Rect::new(card.x, card.y + 72.0, card.w, 24.0), TextStyle::Title14Semi, theme::TEXT_PRIMARY);
        ctx.draw_text_center("Reproduce MP4, MKV, WebM, MOV, AVI, MP3, WAV y FLAC", Rect::new(card.x, card.y + 98.0, card.w, 20.0), TextStyle::Body11Reg, theme::TEXT_SECONDARY);
        let btn = Rect::new(card.x + 100.0, card.y + 128.0, 180.0, 32.0);
        ctx.fill_card(btn, 10.0, if hovered_id == Some("empty_open") { theme::VIOLET_LIGHT } else { theme::VIOLET }, theme::VIOLET_BORDER);
        ctx.draw_text_center("Seleccionar archivo", btn, TextStyle::Body12Semi, theme::TEXT_PRIMARY);
        hits.push(Hit { id: "empty_open".into(), rect: btn, act: PlayerAct::OpenDialog });
    }

    if let Some(msg) = toast {
        let tw = (msg.len() as f32 * 6.8 + 36.0).clamp(200.0, w - 80.0);
        let tr = Rect::new((w - tw) * 0.5, stage.y + 14.0, tw, 32.0);
        ctx.fill_card(tr, 10.0, theme::rgba(22, 34, 28, 0.94), theme::EMERALD);
        ctx.draw_text_center(msg, tr, TextStyle::Body11Med, theme::TEXT_PRIMARY);
    }
    ctx.pop_clip();

    // ==================== BOTTOM CINEMA CONTROL DOCK ====================
    let cy = h - CTRL_BAR_H;
    ctx.fill_rrect(Rect::new(0.0, cy, w, CTRL_BAR_H), 0.0, theme::SURFACE);
    ctx.draw_line(0.0, cy, w, cy, theme::BORDER, 1.0);

    // 1. Seekable Scrub Bar + Timestamps
    let t_cur = format_time(cur_sec);
    let t_tot = format_time(info.duration);
    ctx.draw_text(&t_cur, Rect::new(18.0, cy + 10.0, 58.0, 18.0), TextStyle::Mono11Med, theme::TEXT_PRIMARY);
    ctx.draw_text_right(&t_tot, Rect::new(w - 76.0, cy + 10.0, 58.0, 18.0), TextStyle::Mono11Med, theme::TEXT_SECONDARY);

    let bar_x = 80.0;
    let bar_w = (w - 160.0).max(120.0);
    let bar_r = Rect::new(bar_x, cy + 14.0, bar_w, 9.0);
    ctx.fill_hatched_track(bar_r, 4.5);
    let frac = if info.duration > 0.0 { (cur_sec / info.duration).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let fill_w = (bar_w * frac).max(6.0);
    ctx.fill_rrect(Rect::new(bar_x, cy + 14.0, fill_w, 9.0), 4.5, theme::VIOLET);
    ctx.fill_circle(bar_x + fill_w, cy + 18.5, 6.5, theme::TEXT_PRIMARY);
    hits.push(Hit {
        id: "seek_bar".into(),
        rect: Rect::new(bar_x - 4.0, cy + 6.0, bar_w + 8.0, 24.0),
        act: PlayerAct::SeekTrack { x: bar_x, w: bar_w },
    });

    // 2. Bottom Row Controls
    let row_y = cy + 38.0;

    // Left: Speed & Loop pills
    let spd_lbl = format!("{:.2}x", speed).replace(".00x", "x").replace("0x", "x");
    let spd_r = Rect::new(18.0, row_y + 4.0, 52.0, 28.0);
    ctx.fill_card(spd_r, 8.0, if hovered_id == Some("btn_spd") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_text_center(&spd_lbl, spd_r, TextStyle::Mono11Med, theme::TEXT_PRIMARY);
    hits.push(Hit { id: "btn_spd".into(), rect: spd_r, act: PlayerAct::CycleSpeed });

    let loop_r = Rect::new(78.0, row_y + 4.0, 70.0, 28.0);
    ctx.fill_card(
        loop_r,
        8.0,
        if loop_enabled { theme::VIOLET_TINT } else { theme::ELEVATED },
        if loop_enabled { theme::VIOLET_BORDER } else { theme::BORDER },
    );
    ctx.draw_text_center("Repetir", loop_r, TextStyle::Body11Med, if loop_enabled { theme::VIOLET_LIGHT } else { theme::TEXT_SECONDARY });
    hits.push(Hit { id: "btn_loop".into(), rect: loop_r, act: PlayerAct::ToggleLoop });

    // Center: Prev · -10s · HERO PLAY/PAUSE · +10s · Next
    let mid = w * 0.5;
    let play_r = Rect::new(mid - 20.0, row_y, 40.0, 36.0);
    ctx.fill_card(play_r, 18.0, if hovered_id == Some("btn_play") { theme::VIOLET_LIGHT } else { theme::VIOLET }, theme::VIOLET_BORDER);
    ctx.draw_vector_icon(if paused { VectorIcon::Play } else { VectorIcon::Pause }, mid + if paused { 1.5 } else { 0.0 }, row_y + 18.0, 15.0, theme::TEXT_PRIMARY);
    hits.push(Hit { id: "btn_play".into(), rect: play_r, act: PlayerAct::TogglePlay });

    let back10 = Rect::new(mid - 74.0, row_y + 4.0, 46.0, 28.0);
    ctx.fill_card(back10, 8.0, if hovered_id == Some("btn_b10") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_text_center("-10s", back10, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    hits.push(Hit { id: "btn_b10".into(), rect: back10, act: PlayerAct::SeekRelative(-10.0) });

    let fwd10 = Rect::new(mid + 28.0, row_y + 4.0, 46.0, 28.0);
    ctx.fill_card(fwd10, 8.0, if hovered_id == Some("btn_f10") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_text_center("+10s", fwd10, TextStyle::Mono11Med, theme::TEXT_SECONDARY);
    hits.push(Hit { id: "btn_f10".into(), rect: fwd10, act: PlayerAct::SeekRelative(10.0) });

    let prev_b = Rect::new(mid - 112.0, row_y + 4.0, 30.0, 28.0);
    ctx.fill_card(prev_b, 8.0, if hovered_id == Some("btn_prev") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(VectorIcon::Prev, prev_b.x + 15.0, prev_b.y + 14.0, 11.0, theme::TEXT_SECONDARY);
    hits.push(Hit { id: "btn_prev".into(), rect: prev_b, act: PlayerAct::PrevMedia });

    let next_b = Rect::new(mid + 82.0, row_y + 4.0, 30.0, 28.0);
    ctx.fill_card(next_b, 8.0, if hovered_id == Some("btn_next") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(VectorIcon::Next, next_b.x + 15.0, next_b.y + 14.0, 11.0, theme::TEXT_SECONDARY);
    hits.push(Hit { id: "btn_next".into(), rect: next_b, act: PlayerAct::NextMedia });

    // Right: Mute + Volume Slider + Fullscreen
    let fs_r = Rect::new(w - 48.0, row_y + 4.0, 30.0, 28.0);
    ctx.fill_card(fs_r, 8.0, if hovered_id == Some("btn_fs") { theme::ELEVATED_HOVER } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(VectorIcon::Maximize, fs_r.x + 15.0, fs_r.y + 14.0, 12.0, theme::TEXT_SECONDARY);
    hits.push(Hit { id: "btn_fs".into(), rect: fs_r, act: PlayerAct::WinMax });

    let vol_w = 96.0;
    let vol_x = w - 58.0 - vol_w;
    let vol_bar = Rect::new(vol_x, row_y + 14.0, vol_w, 8.0);
    ctx.fill_rrect(vol_bar, 4.0, theme::ELEVATED);
    if !muted && vol_pct > 0 {
        ctx.fill_rrect(Rect::new(vol_x, row_y + 14.0, (vol_w * vol_pct as f32 / 100.0).max(6.0), 8.0), 4.0, theme::VIOLET);
    }
    hits.push(Hit {
        id: "vol_bar".into(),
        rect: Rect::new(vol_x - 4.0, row_y + 6.0, vol_w + 8.0, 24.0),
        act: PlayerAct::VolTrack { x: vol_x, w: vol_w },
    });

    let mute_r = Rect::new(vol_x - 36.0, row_y + 4.0, 28.0, 28.0);
    ctx.fill_card(mute_r, 8.0, if muted { theme::CORAL_BG } else { theme::ELEVATED }, theme::BORDER);
    ctx.draw_vector_icon(
        if muted { VectorIcon::VolumeMute } else { VectorIcon::Volume },
        mute_r.x + 14.0,
        mute_r.y + 14.0,
        12.0,
        if muted { theme::CORAL } else { theme::TEXT_PRIMARY },
    );
    hits.push(Hit { id: "btn_mute".into(), rect: mute_r, act: PlayerAct::ToggleMute });

    if corner > 0.0 {
        ctx.stroke_rrect(bounds.inset(0.5, 0.5), corner, theme::BORDER, 1.0);
        ctx.pop_layer();
    }
}

unsafe extern "system" fn player_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLOSE {
        PLAYER_RT.with(|c| {
            if let Ok(mut g) = c.try_borrow_mut() {
                if let Some(r) = g.as_mut() {
                    r.stop_session();
                }
            }
        });
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
        return aura_window::handle_getminmaxinfo(hwnd, lparam, 520, 360);
    }
    if msg == WM_NCHITTEST {
        return aura_window::handle_nchittest(hwnd, lparam, TOP_BAR_H, |lx, ly| {
            PLAYER_RT.with(|c| {
                c.try_borrow()
                    .ok()
                    .and_then(|g| g.as_ref().map(|r| r.hits.iter().any(|h| h.rect.contains(lx, ly))))
                    .unwrap_or(false)
            })
        });
    }
    if msg == WM_SYSKEYDOWN {
        let vk = wparam.0 as u16;
        if vk == VK_LEFT.0 { aura_window::snap_window(hwnd, SnapZone::LeftHalf); return LRESULT(0); }
        if vk == VK_RIGHT.0 { aura_window::snap_window(hwnd, SnapZone::RightHalf); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_UP.0 { aura_window::snap_window(hwnd, SnapZone::Maximize); return LRESULT(0); }
        if vk == windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0 { aura_window::snap_window(hwnd, SnapZone::Center); return LRESULT(0); }
    }

    let mut start_title_drag = false;
    let mut deferred_max: Option<bool> = None;
    let mut deferred_min = false;
    let mut deferred_snap: Option<SnapZone> = None;

    let handled = PLAYER_RT.with(|cell| {
        let Ok(mut opt) = cell.try_borrow_mut() else { return false };
        let Some(rt) = opt.as_mut() else { return false };

        match msg {
            WM_TIMER if wparam.0 == TIMER_FRAME => {
                rt.tick();
                true
            }
            WM_SIZE => {
                let mut rc = RECT::default();
                if GetClientRect(hwnd, &mut rc).is_ok() {
                    let raw_w = rc.right - rc.left;
                    let raw_h = rc.bottom - rc.top;
                    if raw_w > 0 && raw_h > 0 {
                        let nw = raw_w.max(480) as u32;
                        let nh = raw_h.max(340) as u32;
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
                if let Some((vx, vw)) = rt.dragging_vol {
                    let pct = (((px - vx) / vw.max(1.0)).clamp(0.0, 1.0) * 100.0).round() as u32;
                    rt.vol_pct = pct;
                    rt.muted = pct == 0;
                    if let Some(s) = &rt.session {
                        s.vol_pct.store(rt.vol_pct, Ordering::Relaxed);
                        s.muted.store(rt.muted, Ordering::Relaxed);
                    }
                    rt.redraw();
                    return true;
                }
                let new_hov = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).map(|h| h.id.clone());
                if new_hov != rt.hovered_id {
                    rt.hovered_id = new_hov;
                    rt.redraw();
                }
                true
            }
            WM_LBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)).cloned() {
                    match hit.act {
                        PlayerAct::WinMax => {
                            rt.maximized = !rt.maximized;
                            deferred_max = Some(rt.maximized);
                        }
                        PlayerAct::WinMin => {
                            deferred_min = true;
                        }
                        other => rt.execute(other, px),
                    }
                    return true;
                }
                if py < TOP_BAR_H {
                    start_title_drag = true;
                }
                true
            }
            0x0215 | 0x001F => {
                // WM_CAPTURECHANGED / WM_CANCELMODE: release any stuck volume/seek drag capture!
                rt.dragging_vol = None;
                true
            }
            WM_RBUTTONDOWN => {
                let px = (lparam.0 & 0xFFFF) as i16 as f32;
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if let Some(hit) = rt.hits.iter().rev().find(|h| h.rect.contains(px, py)) {
                    if matches!(hit.act, PlayerAct::WinMax) {
                        deferred_snap = Some(SnapZone::LeftHalf);
                    }
                }
                true
            }
            WM_LBUTTONUP => {
                if let Some((sx, sw)) = rt.dragging_seek.take() {
                    let px = (lparam.0 & 0xFFFF) as i16 as f32;
                    let frac = ((px - sx) / sw.max(1.0)).clamp(0.0, 1.0) as f64;
                    if rt.info.duration > 0.0 {
                        rt.seek_to(frac * rt.info.duration);
                    }
                    let _ = ReleaseCapture();
                    rt.redraw();
                }
                if rt.dragging_vol.take().is_some() {
                    let _ = ReleaseCapture();
                }
                true
            }
            WM_LBUTTONDBLCLK => {
                let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
                if py < rt.height - CTRL_BAR_H {
                    rt.maximized = !rt.maximized;
                    deferred_max = Some(rt.maximized);
                }
                true
            }
            WM_MOUSEWHEEL => {
                let delta = ((wparam.0 >> 16) as i16) as f32;
                let next = if delta > 0.0 { (rt.vol_pct + 5).min(100) } else { rt.vol_pct.saturating_sub(5) };
                rt.vol_pct = next;
                rt.muted = next == 0;
                if let Some(s) = &rt.session {
                    s.vol_pct.store(rt.vol_pct, Ordering::Relaxed);
                    s.muted.store(rt.muted, Ordering::Relaxed);
                }
                rt.redraw();
                true
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_SPACE.0 {
                    rt.execute(PlayerAct::TogglePlay, 0.0);
                } else if vk == VK_LEFT.0 {
                    rt.execute(PlayerAct::SeekRelative(-5.0), 0.0);
                } else if vk == VK_RIGHT.0 {
                    rt.execute(PlayerAct::SeekRelative(5.0), 0.0);
                } else if vk == b'M' as u16 {
                    rt.execute(PlayerAct::ToggleMute, 0.0);
                } else if vk == b'F' as u16 {
                    rt.maximized = !rt.maximized;
                    deferred_max = Some(rt.maximized);
                } else if vk == VK_ESCAPE.0 {
                    if rt.maximized {
                        rt.maximized = false;
                        deferred_max = Some(false);
                    } else {
                        rt.execute(PlayerAct::WinClose, 0.0);
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
