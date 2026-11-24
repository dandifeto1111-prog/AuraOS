//! AuraOS v5.2 Hardware-Accelerated Native Video Decoder (DXVA/D3D11 Media Foundation + FFmpeg Fallback)
//! - Tier 1: Native Windows Media Foundation (`IMFSourceReader`) with DXGI Hardware Video Decode
//!   (`IMFDXGIDeviceManager` + `MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS` + GPU Video Processor)
//!   with seamless fallback to software MF if a GPU driver declines DXGI acceleration.
//! - Native Resolution (up to 1080p/1440p — ZERO 720p nearest-neighbor pixelation!) + true 60 FPS!
//! - Zero-copy single-slice `memcpy` fast path when stride matches (eliminates 124M pixel alpha writes/sec!)
//! - Crisp fixed-point 2x2 bilinear scaler when downscaling 4K -> 1080p (zero jagged blocks!)
//! - True `Condvar` pause (`0.00%` CPU, ZERO Media Foundation tear-down/re-open virtual memory leaks!)
//! - Tier 2: Automatic `ffmpeg.exe` Lanczos 60 FPS pipe fallback for exotic `.mkv` / VP9 `.webm` codecs.

use crate::models::app_data_dir;
use crate::win32_shell::to_wide;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use windows::core::{GUID, Interface, PCWSTR, PROPVARIANT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11Multithread, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFDXGIDeviceManager, IMFMediaBuffer, IMFSourceReader, MFCreateAttributes,
    MFCreateDXGIDeviceManager, MFCreateMediaType, MFCreateSourceReaderFromURL, MFMediaType_Video,
    MFShutdown, MFStartup, MFVideoFormat_RGB32, MFSTARTUP_NOSOCKET, MF_LOW_LATENCY,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_D3D_MANAGER,
    MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn is_video_ext(path: &str) -> bool {
    let low = path.to_lowercase();
    low.ends_with(".mp4")
        || low.ends_with(".webm")
        || low.ends_with(".mkv")
        || low.ends_with(".mov")
        || low.ends_with(".m4v")
        || low.ends_with(".wmv")
        || low.ends_with(".avi")
}

/// Deterministic thumbnail path under `%LocalAppData%\AuraShell\wp_thumbs`
pub fn video_thumbnail_cache_path(video_path: &str) -> PathBuf {
    let thumb_dir = app_data_dir().join("wp_thumbs");
    let _ = std::fs::create_dir_all(&thumb_dir);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in video_path.as_bytes() {
        h = (h ^ (*b as u64)).wrapping_mul(0x100000001b3);
    }
    thumb_dir.join(format!("vid_{:016x}.bmp", h))
}

/// Resolve ffmpeg.exe if present (beside exe, PATH, WinGet, scoop, C:\ffmpeg)
pub fn resolve_ffmpeg_exe() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let local_ff = dir.join("ffmpeg.exe");
            if local_ff.exists() {
                return Some(local_ff);
            }
        }
    }
    let app_ff = app_data_dir().join("ffmpeg.exe");
    if app_ff.exists() {
        return Some(app_ff);
    }
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in path_var.split(';') {
            let cand = PathBuf::from(dir.trim()).join("ffmpeg.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let link = PathBuf::from(&local).join(r"Microsoft\WinGet\Links\ffmpeg.exe");
        if link.exists() {
            return Some(link);
        }
        let winget = PathBuf::from(&local).join(r"Microsoft\WinGet\Packages");
        if let Ok(rd) = std::fs::read_dir(winget) {
            for e in rd.flatten() {
                let p = e.path();
                if p.file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_lowercase().contains("ffmpeg"))
                    .unwrap_or(false)
                {
                    if let Ok(sub) = std::fs::read_dir(&p) {
                        for se in sub.flatten() {
                            let cand = se.path().join("bin").join("ffmpeg.exe");
                            if cand.exists() {
                                return Some(cand);
                            }
                        }
                    }
                    let direct = p.join("bin").join("ffmpeg.exe");
                    if direct.exists() {
                        return Some(direct);
                    }
                }
            }
        }
    }
    for fixed in [r"C:\ffmpeg\bin\ffmpeg.exe", r"C:\Program Files\ffmpeg\bin\ffmpeg.exe"] {
        let p = PathBuf::from(fixed);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn terminate_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 0);
            let _ = windows::Win32::Foundation::CloseHandle(h);
        }
    }
}

/// Shared zero-allocation double-buffered video frame slot
pub struct VideoFrameSlot {
    pub seq: u64,
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

pub struct NativeVideoStream {
    stop: Arc<AtomicBool>,
    pause_sig: Arc<(Mutex<bool>, Condvar)>,
    failed: Arc<AtomicBool>,
    child_pid: Arc<Mutex<Option<u32>>>,
    slot: Arc<Mutex<VideoFrameSlot>>,
}

impl NativeVideoStream {
    pub fn spawn(video_path: &str, max_w: u32, max_h: u32, max_fps: u32) -> Option<Self> {
        Self::spawn_with_callback(video_path, max_w, max_h, max_fps, None)
    }

    pub fn spawn_with_callback(
        video_path: &str,
        max_w: u32,
        max_h: u32,
        max_fps: u32,
        on_frame: Option<Box<dyn Fn() + Send + 'static>>,
    ) -> Option<Self> {
        if !Path::new(video_path).exists() {
            return None;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let pause_sig = Arc::new((Mutex::new(false), Condvar::new()));
        let failed = Arc::new(AtomicBool::new(false));
        let child_pid = Arc::new(Mutex::new(None));
        let slot = Arc::new(Mutex::new(VideoFrameSlot {
            seq: 0,
            w: 0,
            h: 0,
            bgra: Vec::new(),
        }));

        let stop_bg = stop.clone();
        let pause_bg = pause_sig.clone();
        let failed_bg = failed.clone();
        let pid_bg = child_pid.clone();
        let slot_bg = slot.clone();
        let path_owned = video_path.to_string();

        std::thread::spawn(move || {
            // Tier 1: Native Windows Media Foundation with DXVA/D3D11 Hardware Acceleration
            let mf_ok = run_mf_video_loop(
                &path_owned,
                max_w,
                max_h,
                max_fps,
                &stop_bg,
                &pause_bg,
                &slot_bg,
                on_frame.as_deref(),
            );
            if stop_bg.load(Ordering::Relaxed) || mf_ok {
                return;
            }

            // Tier 2: FFmpeg Lanczos pipe fallback if installed (for VP9 .webm / exotic .mkv codecs)
            if let Some(ffmpeg) = resolve_ffmpeg_exe() {
                let ff_ok = run_ffmpeg_video_loop(
                    &ffmpeg,
                    &path_owned,
                    max_w,
                    max_h,
                    max_fps,
                    &stop_bg,
                    &pause_bg,
                    &pid_bg,
                    &slot_bg,
                    on_frame.as_deref(),
                );
                if ff_ok || stop_bg.load(Ordering::Relaxed) {
                    return;
                }
            }

            // Signal Tier 3 fallback if both decoders fail
            failed_bg.store(true, Ordering::SeqCst);
        });

        Some(Self {
            stop,
            pause_sig,
            failed,
            child_pid,
            slot,
        })
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// True zero-CPU, zero-leak pause: parks decoder thread on a Condvar without destroying Media Foundation!
    pub fn set_paused(&self, paused: bool) {
        let (lock, cvar) = &*self.pause_sig;
        if let Ok(mut g) = lock.lock() {
            if *g != paused {
                *g = paused;
                cvar.notify_all();
            }
        }
    }

    /// Zero-allocation double-buffer swap: swaps latest decoded frame into `dst_buf` if newer than `last_seq`.
    pub fn swap_latest_frame(&self, last_seq: &mut u64, dst_buf: &mut Vec<u8>) -> Option<(u32, u32)> {
        let mut g = self.slot.try_lock().ok()?;
        if g.seq == *last_seq || g.w == 0 || g.h == 0 || g.bgra.is_empty() {
            return None;
        }
        *last_seq = g.seq;
        let dims = (g.w, g.h);
        std::mem::swap(dst_buf, &mut g.bgra);
        Some(dims)
    }

    pub fn stop_now(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let (lock, cvar) = &*self.pause_sig;
        if let Ok(mut g) = lock.lock() {
            *g = false;
            cvar.notify_all();
        }
        if let Ok(mut g) = self.child_pid.lock() {
            if let Some(pid) = g.take() {
                terminate_pid(pid);
            }
        }
    }
}

impl Drop for NativeVideoStream {
    fn drop(&mut self) {
        self.stop_now();
    }
}

/// Copies/scales RGB32 (BGRX, possibly bottom-up with negative stride) into tightly packed top-down BGRA.
/// - Fast Path 1: single contiguous `copy_from_slice` when `sw == dw && sh == dh && top-down && stride == sw*4`
///   (ZERO per-pixel alpha writes — Direct2D `draw_video_frame` uses `D2D1_ALPHA_MODE_IGNORE`!)
/// - Fast Path 2: row-by-row `copy_from_slice` when `sw == dw && sh == dh`
/// - Scale Path: smooth fixed-point 2x2 bilinear interpolation (ZERO blocky nearest-neighbor pixelation!)
fn copy_rgb32_to_bgra_scaled(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    stride: i32,
    dst: &mut Vec<u8>,
    dst_w: u32,
    dst_h: u32,
) {
    let needed = (dst_w * dst_h * 4) as usize;
    if dst.len() != needed {
        dst.resize(needed, 255);
    }
    let abs_stride = stride.unsigned_abs() as usize;
    let bottom_up = stride < 0;
    let sw = src_w as usize;
    let sh = src_h as usize;
    let dw = dst_w as usize;
    let dh = dst_h as usize;
    if abs_stride < sw * 4 || src.len() < abs_stride * sh {
        return;
    }

    if sw == dw && sh == dh {
        let row_bytes = sw * 4;
        if !bottom_up && abs_stride == row_bytes {
            dst.copy_from_slice(&src[..needed]);
        } else {
            for y in 0..dh {
                let sy = if bottom_up { sh - 1 - y } else { y };
                let srow = &src[sy * abs_stride..sy * abs_stride + row_bytes];
                let drow = &mut dst[y * row_bytes..(y + 1) * row_bytes];
                drow.copy_from_slice(srow);
            }
        }
    } else {
        // Crisp 2x2 fixed-point (8-bit frac) bilinear resampling — zero jagged pixelation!
        let max_sx = sw.saturating_sub(1);
        let max_sy = sh.saturating_sub(1);
        let x_step = (((sw as u64) << 8) / dw.max(1) as u64) as usize;
        let y_step = (((sh as u64) << 8) / dh.max(1) as u64) as usize;

        for y in 0..dh {
            let fy = y * y_step;
            let y0 = (fy >> 8).min(max_sy);
            let y1 = (y0 + 1).min(max_sy);
            let wy = fy & 0xFF;
            let wy_inv = 256 - wy;

            let sy0 = if bottom_up { max_sy - y0 } else { y0 };
            let sy1 = if bottom_up { max_sy - y1 } else { y1 };
            let r0 = &src[sy0 * abs_stride..sy0 * abs_stride + sw * 4];
            let r1 = &src[sy1 * abs_stride..sy1 * abs_stride + sw * 4];
            let drow = &mut dst[y * dw * 4..(y + 1) * dw * 4];

            for x in 0..dw {
                let fx = x * x_step;
                let x0 = (fx >> 8).min(max_sx);
                let x1 = (x0 + 1).min(max_sx);
                let wx = fx & 0xFF;
                let wx_inv = 256 - wx;

                let w00 = wx_inv * wy_inv;
                let w10 = wx * wy_inv;
                let w01 = wx_inv * wy;
                let w11 = wx * wy;

                let i0 = x0 * 4;
                let i1 = x1 * 4;
                let di = x * 4;

                let b = (r0[i0] as usize * w00 + r0[i1] as usize * w10 + r1[i0] as usize * w01 + r1[i1] as usize * w11) >> 16;
                let g = (r0[i0 + 1] as usize * w00 + r0[i1 + 1] as usize * w10 + r1[i0 + 1] as usize * w01 + r1[i1 + 1] as usize * w11) >> 16;
                let r = (r0[i0 + 2] as usize * w00 + r0[i1 + 2] as usize * w10 + r1[i0 + 2] as usize * w01 + r1[i1 + 2] as usize * w11) >> 16;

                drow[di] = b as u8;
                drow[di + 1] = g as u8;
                drow[di + 2] = r as u8;
                drow[di + 3] = 255;
            }
        }
    }
}

struct MfReaderBundle {
    reader: IMFSourceReader,
    _dxgi_mgr: Option<IMFDXGIDeviceManager>,
    _d3d_dev: Option<ID3D11Device>,
    src_w: u32,
    src_h: u32,
    stride: i32,
    fps: f64,
}

fn try_create_dxgi_video_manager() -> Option<(ID3D11Device, IMFDXGIDeviceManager)> {
    unsafe {
        let mut dev: Option<ID3D11Device> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            None,
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut dev),
            None,
            None,
        )
        .ok()?;
        let dev = dev?;
        if let Ok(mt) = dev.cast::<ID3D11Multithread>() {
            let _ = mt.SetMultithreadProtected(true);
        }
        let mut reset_token = 0u32;
        let mut mgr: Option<IMFDXGIDeviceManager> = None;
        MFCreateDXGIDeviceManager(&mut reset_token, &mut mgr).ok()?;
        let mgr = mgr?;
        mgr.ResetDevice(&dev, reset_token).ok()?;
        Some((dev, mgr))
    }
}

fn open_mf_reader(video_path: &str, max_w: u32, max_h: u32, allow_hw: bool) -> Option<MfReaderBundle> {
    if allow_hw {
        if let Some(bundle) = open_mf_reader_inner(video_path, max_w, max_h, true) {
            return Some(bundle);
        }
    }
    open_mf_reader_inner(video_path, max_w, max_h, false)
}

fn open_mf_reader_inner(video_path: &str, max_w: u32, max_h: u32, use_dxgi_hw: bool) -> Option<MfReaderBundle> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        if MFCreateAttributes(&mut attrs, 4).is_err() {
            return None;
        }
        let attrs = attrs?;
        let _ = attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1);
        let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);

        let mut d3d_dev: Option<ID3D11Device> = None;
        let mut dxgi_mgr: Option<IMFDXGIDeviceManager> = None;

        if use_dxgi_hw {
            if let Some((dev, mgr)) = try_create_dxgi_video_manager() {
                let _ = attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &mgr);
                let _ = attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1);
                d3d_dev = Some(dev);
                dxgi_mgr = Some(mgr);
            } else {
                let _ = attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
            }
        } else {
            let _ = attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);
        }

        let wpath = to_wide(video_path);
        let reader = MFCreateSourceReaderFromURL(PCWSTR(wpath.as_ptr()), &attrs).ok()?;

        let all_streams = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
        let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let _ = reader.SetStreamSelection(all_streams, false);
        reader.SetStreamSelection(first_vid, true).ok()?;

        // Inspect native video size first so we only ask MF Video Processor to resize if video > target!
        let mut req_resize: Option<(u32, u32)> = None;
        if let Ok(nat) = reader.GetNativeMediaType(first_vid, 0) {
            if let Ok(fsz) = nat.GetUINT64(&MF_MT_FRAME_SIZE) {
                let nw = (fsz >> 32) as u32;
                let nh = (fsz & 0xFFFF_FFFF) as u32;
                if nw > 0 && nh > 0 && (nw > max_w || nh > max_h) {
                    let scale = (max_w as f64 / nw as f64).min(max_h as f64 / nh as f64).min(1.0);
                    let tw = (((nw as f64 * scale) as u32) / 2 * 2).max(64);
                    let th = (((nh as f64 * scale) as u32) / 2 * 2).max(64);
                    req_resize = Some((tw, th));
                }
            }
        }

        let mut set_ok = false;
        if let Some((tw, th)) = req_resize {
            if let Ok(out_type) = MFCreateMediaType() {
                if out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).is_ok()
                    && out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).is_ok()
                    && out_type.SetUINT64(&MF_MT_FRAME_SIZE, ((tw as u64) << 32) | (th as u64)).is_ok()
                    && reader.SetCurrentMediaType(first_vid, None, &out_type).is_ok()
                {
                    set_ok = true;
                }
            }
        }
        if !set_ok {
            let out_type = MFCreateMediaType().ok()?;
            out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
            out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
            reader.SetCurrentMediaType(first_vid, None, &out_type).ok()?;
        }

        let cur_type = reader.GetCurrentMediaType(first_vid).ok()?;
        let frame_sz = cur_type.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
        let src_w = (frame_sz >> 32) as u32;
        let src_h = (frame_sz & 0xFFFF_FFFF) as u32;
        if src_w == 0 || src_h == 0 || src_w > 8192 || src_h > 8192 {
            return None;
        }

        let stride = cur_type
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .map(|v| v as i32)
            .unwrap_or((src_w * 4) as i32);

        let mut fps = 30.0f64;
        if let Ok(fr) = cur_type.GetUINT64(&MF_MT_FRAME_RATE) {
            let num = (fr >> 32) as f64;
            let den = (fr & 0xFFFF_FFFF) as f64;
            if den > 0.0 && num > 0.0 {
                fps = (num / den).clamp(10.0, 120.0);
            }
        }

        Some(MfReaderBundle {
            reader,
            _dxgi_mgr: dxgi_mgr,
            _d3d_dev: d3d_dev,
            src_w,
            src_h,
            stride,
            fps,
        })
    }
}

fn read_mf_buffer_frame(
    buf: &IMFMediaBuffer,
    src_w: u32,
    src_h: u32,
    stride: i32,
    dst: &mut Vec<u8>,
    dst_w: u32,
    dst_h: u32,
) -> bool {
    unsafe {
        let mut ptr: *mut u8 = std::ptr::null_mut();
        let mut cur_len = 0u32;
        if buf.Lock(&mut ptr, None, Some(&mut cur_len)).is_err() || ptr.is_null() {
            return false;
        }
        let slice = std::slice::from_raw_parts(ptr, cur_len as usize);
        copy_rgb32_to_bgra_scaled(slice, src_w, src_h, stride, dst, dst_w, dst_h);
        let _ = buf.Unlock();
        true
    }
}

fn wait_while_paused(pause_sig: &Arc<(Mutex<bool>, Condvar)>, stop: &Arc<AtomicBool>) -> bool {
    let (lock, cvar) = &**pause_sig;
    let Ok(mut g) = lock.lock() else { return false };
    let mut was_paused = false;
    while *g && !stop.load(Ordering::Relaxed) {
        was_paused = true;
        g = match cvar.wait(g) {
            Ok(ng) => ng,
            Err(_) => return was_paused,
        };
    }
    was_paused
}

fn run_mf_video_loop(
    video_path: &str,
    max_w: u32,
    max_h: u32,
    max_fps: u32,
    stop: &Arc<AtomicBool>,
    pause_sig: &Arc<(Mutex<bool>, Condvar)>,
    slot: &Arc<Mutex<VideoFrameSlot>>,
    on_frame: Option<&(dyn Fn() + Send + 'static)>,
) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).is_err() {
            CoUninitialize();
            return false;
        }
    }

    let Some(bundle) = open_mf_reader(video_path, max_w, max_h, true) else {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
        return false;
    };

    let scale = (max_w as f64 / bundle.src_w as f64)
        .min(max_h as f64 / bundle.src_h as f64)
        .min(1.0);
    let dst_w = if scale >= 0.999 {
        bundle.src_w
    } else {
        (((bundle.src_w as f64 * scale) as u32) / 2 * 2).max(64)
    };
    let dst_h = if scale >= 0.999 {
        bundle.src_h
    } else {
        (((bundle.src_h as f64 * scale) as u32) / 2 * 2).max(64)
    };

    let effective_fps = bundle.fps.min(max_fps.clamp(12, 60) as f64);
    let frame_interval = Duration::from_secs_f64(1.0 / effective_fps);
    let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    let mut local_bgra = vec![255u8; (dst_w * dst_h * 4) as usize];
    let mut seq = 0u64;
    let mut decoded_any = false;
    let mut next_due = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        if wait_while_paused(pause_sig, stop) {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            next_due = Instant::now();
        }

        let mut flags = 0u32;
        let mut ts = 0i64;
        let mut sample = None;
        let read_res = unsafe {
            bundle.reader.ReadSample(
                first_vid,
                0,
                None,
                Some(&mut flags),
                Some(&mut ts),
                Some(&mut sample),
            )
        };
        if read_res.is_err() {
            break;
        }
        if (flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32)) != 0 {
            if !decoded_any {
                break;
            }
            let pos = PROPVARIANT::from(0i64);
            let seek_ok = unsafe { bundle.reader.SetCurrentPosition(&GUID::zeroed(), &pos).is_ok() };
            if !seek_ok {
                break;
            }
            continue;
        }

        if let Some(smp) = sample {
            if let Ok(buf) = unsafe { smp.ConvertToContiguousBuffer() } {
                if read_mf_buffer_frame(
                    &buf,
                    bundle.src_w,
                    bundle.src_h,
                    bundle.stride,
                    &mut local_bgra,
                    dst_w,
                    dst_h,
                ) {
                    decoded_any = true;
                    seq += 1;
                    if let Ok(mut g) = slot.lock() {
                        g.w = dst_w;
                        g.h = dst_h;
                        g.seq = seq;
                        std::mem::swap(&mut g.bgra, &mut local_bgra);
                    }
                    if let Some(cb) = on_frame {
                        cb();
                    }

                    // Precise 60 FPS frame pacing while staying instantaneously interruptible!
                    next_due += frame_interval;
                    let now = Instant::now();
                    if next_due > now {
                        let mut rem = next_due - now;
                        while rem > Duration::from_millis(12) && !stop.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(8));
                            rem = next_due.saturating_duration_since(Instant::now());
                        }
                        if !stop.load(Ordering::Relaxed) && rem > Duration::ZERO {
                            std::thread::sleep(rem);
                        }
                    } else if now.duration_since(next_due) > Duration::from_millis(150) {
                        next_due = now;
                    }
                }
            }
        }
    }

    drop(bundle);
    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }
    decoded_any
}

fn run_ffmpeg_video_loop(
    ffmpeg: &Path,
    video_path: &str,
    max_w: u32,
    max_h: u32,
    max_fps: u32,
    stop: &Arc<AtomicBool>,
    pause_sig: &Arc<(Mutex<bool>, Condvar)>,
    pid_slot: &Arc<Mutex<Option<u32>>>,
    slot: &Arc<Mutex<VideoFrameSlot>>,
    on_frame: Option<&(dyn Fn() + Send + 'static)>,
) -> bool {
    let w = (max_w / 2 * 2).max(64);
    let h = (max_h / 2 * 2).max(64);
    let vf = format!(
        "scale={}:{}:flags=lanczos:force_original_aspect_ratio=increase,crop={}:{},fps={}",
        w,
        h,
        w,
        h,
        max_fps.clamp(12, 60)
    );
    let Ok(mut child) = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-hwaccel",
            "auto",
            "-stream_loop",
            "-1",
            "-re",
            "-threads",
            "2",
            "-i",
            video_path,
            "-vf",
            &vf,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgra",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    else {
        return false;
    };

    if let Ok(mut g) = pid_slot.lock() {
        *g = Some(child.id());
    }

    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        return false;
    };

    let frame_bytes = (w * h * 4) as usize;
    let mut local_buf = vec![0u8; frame_bytes];
    let mut seq = 0u64;
    let mut decoded_any = false;

    while !stop.load(Ordering::Relaxed) {
        let _ = wait_while_paused(pause_sig, stop);
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let mut read_total = 0usize;
        let mut eof = false;
        while read_total < frame_bytes {
            if stop.load(Ordering::Relaxed) {
                eof = true;
                break;
            }
            match stdout.read(&mut local_buf[read_total..]) {
                Ok(0) => {
                    eof = true;
                    break;
                }
                Ok(n) => read_total += n,
                Err(_) => {
                    eof = true;
                    break;
                }
            }
        }
        if eof || stop.load(Ordering::Relaxed) {
            break;
        }
        decoded_any = true;
        seq += 1;
        if let Ok(mut g) = slot.lock() {
            g.w = w;
            g.h = h;
            g.seq = seq;
            std::mem::swap(&mut g.bgra, &mut local_buf);
            if local_buf.len() != frame_bytes {
                local_buf.resize(frame_bytes, 0);
            }
        }
        if let Some(cb) = on_frame {
            cb();
        }
    }

    let _ = child.kill();
    let _ = child.wait();
    decoded_any
}

/// Extracts a clean thumbnail preview (.bmp) for any custom video wallpaper using Native Media Foundation!
pub fn extract_video_thumbnail(video_path: &str) -> Option<String> {
    if !Path::new(video_path).exists() {
        return None;
    }
    let out_bmp = video_thumbnail_cache_path(video_path);
    if out_bmp.exists() {
        return Some(out_bmp.to_string_lossy().to_string());
    }

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).is_err() {
            CoUninitialize();
            return None;
        }
    }

    let mut result_path = None;
    if let Some(bundle) = open_mf_reader(video_path, 480, 270, false) {
        let scale = (480.0 / bundle.src_w as f64).min(270.0 / bundle.src_h as f64).min(1.0);
        let tw = (((bundle.src_w as f64 * scale) as u32) / 4 * 4).max(64);
        let th = (((bundle.src_h as f64 * scale) as u32) / 2 * 2).max(64);
        let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let mut bgra = Vec::new();
        let mut grabbed = false;

        // Grab up to frame #6 to skip initial black fade-in frames when possible
        for idx in 0..8 {
            let mut flags = 0u32;
            let mut sample = None;
            if unsafe {
                bundle
                    .reader
                    .ReadSample(first_vid, 0, None, Some(&mut flags), None, Some(&mut sample))
            }
            .is_err()
            {
                break;
            }
            if (flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32)) != 0 {
                break;
            }
            if let Some(smp) = sample {
                if let Ok(buf) = unsafe { smp.ConvertToContiguousBuffer() } {
                    if read_mf_buffer_frame(
                        &buf,
                        bundle.src_w,
                        bundle.src_h,
                        bundle.stride,
                        &mut bgra,
                        tw,
                        th,
                    ) {
                        grabbed = true;
                        if idx >= 4 {
                            break;
                        }
                    }
                }
            }
        }
        drop(bundle);

        if grabbed && write_bgra_as_bmp(&out_bmp, tw, th, &bgra) {
            result_path = Some(out_bmp.to_string_lossy().to_string());
        }
    }

    unsafe {
        let _ = MFShutdown();
        CoUninitialize();
    }

    if result_path.is_some() {
        return result_path;
    }

    // Fallback to ffmpeg thumbnail if MF couldn't open codec (e.g. VP9 .webm)
    if let Some(ff) = resolve_ffmpeg_exe() {
        let out_jpg = out_bmp.with_extension("jpg");
        let ok = Command::new(ff)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-ss",
                "0.3",
                "-i",
                video_path,
                "-frames:v",
                "1",
                "-vf",
                "scale=480:-2",
            ])
            .arg(&out_jpg)
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && out_jpg.exists() {
            return Some(out_jpg.to_string_lossy().to_string());
        }
    }
    None
}

fn write_bgra_as_bmp(path: &Path, w: u32, h: u32, bgra: &[u8]) -> bool {
    if bgra.len() < (w * h * 4) as usize {
        return false;
    }
    let pixel_bytes = w * h * 4;
    let file_size = 54 + pixel_bytes;
    let mut out = Vec::with_capacity(file_size as usize);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&54u32.to_le_bytes());
    // BITMAPINFOHEADER (40 bytes), negative height = top-down 32-bit BGRA
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(-(h as i32)).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&pixel_bytes.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&2835i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for px in bgra[..pixel_bytes as usize].chunks_exact(4) {
        out.push(px[0]);
        out.push(px[1]);
        out.push(px[2]);
        out.push(255);
    }
    std::fs::write(path, &out).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_mf_video_and_bogota_weather() {
        let sample = r"D:\SteamLibrary\steamapps\workshop\content\431960\1386004994\TeddyLoid - ME!ME!ME! (feat. daoko) - REAL 1080p!!!.mp4";
        if Path::new(sample).exists() {
            let stop = Arc::new(AtomicBool::new(false));
            let pause_sig = Arc::new((Mutex::new(false), Condvar::new()));
            let slot = Arc::new(Mutex::new(VideoFrameSlot {
                seq: 0,
                w: 0,
                h: 0,
                bgra: Vec::new(),
            }));
            let stop_bg = stop.clone();
            let pause_bg = pause_sig.clone();
            let slot_bg = slot.clone();
            let p = sample.to_string();
            let th = std::thread::spawn(move || {
                run_mf_video_loop(&p, 1920, 1080, 60, &stop_bg, &pause_bg, &slot_bg, None)
            });
            let mut got_frames = 0u64;
            let mut dims = (0u32, 0u32);
            for _ in 0..120 {
                std::thread::sleep(Duration::from_millis(50));
                if let Ok(g) = slot.lock() {
                    if g.seq >= 2 && !g.bgra.is_empty() {
                        got_frames = g.seq;
                        dims = (g.w, g.h);
                        break;
                    }
                }
            }
            stop.store(true, Ordering::SeqCst);
            let mf_ok = th.join().unwrap_or(false);
            println!("MF_NATIVE_TEST: mf_ok={} frames={} dims={}x{}", mf_ok, got_frames, dims.0, dims.1);
            assert!(mf_ok && got_frames >= 2 && dims.0 > 0 && dims.1 > 0, "Media Foundation native decoder failed!");

            let thumb = extract_video_thumbnail(sample);
            println!("MF_THUMB_TEST: {:?}", thumb);
            assert!(thumb.as_deref().map(|t| Path::new(t).exists()).unwrap_or(false));
        }

        let geo_res = crate::system::geocode_city_debug("Bogotá");
        println!("GEOCODE_BOGOTA_DEBUG: {:?}", geo_res);
        assert!(geo_res.is_ok(), "geocode failed: {:?}", geo_res);
    }
}
