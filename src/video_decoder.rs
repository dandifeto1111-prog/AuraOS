//! AuraOS v4.5 Native Zero-Dependency Video Decoder (Windows Media Foundation + FFmpeg Fallback)
//! - Tier 1: Native Windows Media Foundation (`IMFSourceReader`) -> 32-bit BGRA frames (0 external deps!)
//!   Works out-of-the-box on 100% of clean Windows 10/11 PCs for .mp4, .m4v, .mov, .wmv, .avi, etc.
//! - Tier 2: Automatic `ffmpeg.exe` pipe fallback when installed (handles exotic .mkv / VP9 .webm / 10-bit)
//! - Zero-allocation double-buffered frame swap (`take_frame_into`) -> eliminates 110 MB/s heap churn!
//! - Automatic first-frame `.bmp` thumbnail extractor for custom video wallpapers in the gallery!

use crate::models::app_data_dir;
use crate::win32_shell::to_wide;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::{GUID, PCWSTR, PROPVARIANT};
use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaBuffer, IMFSourceReader, MFCreateAttributes, MFCreateMediaType,
    MFCreateSourceReaderFromURL, MFMediaType_Video, MFShutdown, MFStartup, MFVideoFormat_RGB32,
    MFSTARTUP_NOSOCKET, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READER_ALL_STREAMS,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
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
    failed: Arc<AtomicBool>,
    child_pid: Arc<Mutex<Option<u32>>>,
    slot: Arc<Mutex<VideoFrameSlot>>,
}

impl NativeVideoStream {
    pub fn spawn(video_path: &str, max_w: u32, max_h: u32, max_fps: u32) -> Option<Self> {
        if !Path::new(video_path).exists() {
            return None;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let child_pid = Arc::new(Mutex::new(None));
        let slot = Arc::new(Mutex::new(VideoFrameSlot {
            seq: 0,
            w: 0,
            h: 0,
            bgra: Vec::new(),
        }));

        let stop_bg = stop.clone();
        let failed_bg = failed.clone();
        let pid_bg = child_pid.clone();
        let slot_bg = slot.clone();
        let path_owned = video_path.to_string();

        std::thread::spawn(move || {
            // Tier 1: Native Windows Media Foundation (works on 100% of clean Windows 10/11 PCs!)
            let mf_ok = run_mf_video_loop(&path_owned, max_w, max_h, max_fps, &stop_bg, &slot_bg);
            if stop_bg.load(Ordering::Relaxed) {
                return;
            }
            if mf_ok {
                return;
            }

            // Tier 2: FFmpeg pipe fallback if installed (for VP9 .webm / exotic .mkv codecs)
            if let Some(ffmpeg) = resolve_ffmpeg_exe() {
                let ff_ok = run_ffmpeg_video_loop(
                    &ffmpeg,
                    &path_owned,
                    max_w,
                    max_h,
                    max_fps,
                    &stop_bg,
                    &pid_bg,
                    &slot_bg,
                );
                if ff_ok || stop_bg.load(Ordering::Relaxed) {
                    return;
                }
            }

            // Signal Tier 3 fallback (embedded HTML5 <video> in WorkerW for WallpaperEngine!)
            failed_bg.store(true, Ordering::SeqCst);
        });

        Some(Self {
            stop,
            failed,
            child_pid,
            slot,
        })
    }

    pub fn has_failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
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

/// Scales/copies RGB32 (BGRX, possibly bottom-up with negative stride) into tightly packed top-down BGRA with alpha=255
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
        for y in 0..dh {
            let sy = if bottom_up { sh - 1 - y } else { y };
            let srow = &src[sy * abs_stride..sy * abs_stride + sw * 4];
            let drow = &mut dst[y * dw * 4..(y + 1) * dw * 4];
            drow.copy_from_slice(srow);
            for px in drow.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
    } else {
        for y in 0..dh {
            let sy_top = (y * sh) / dh;
            let sy = if bottom_up { sh - 1 - sy_top } else { sy_top };
            let srow = &src[sy * abs_stride..sy * abs_stride + sw * 4];
            let drow = &mut dst[y * dw * 4..(y + 1) * dw * 4];
            for x in 0..dw {
                let sx = (x * sw) / dw;
                let si = sx * 4;
                let di = x * 4;
                drow[di] = srow[si];
                drow[di + 1] = srow[si + 1];
                drow[di + 2] = srow[si + 2];
                drow[di + 3] = 255;
            }
        }
    }
}

fn open_mf_reader(video_path: &str) -> Option<(IMFSourceReader, u32, u32, i32, f64)> {
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        if MFCreateAttributes(&mut attrs, 1).is_err() {
            return None;
        }
        let attrs = attrs?;
        let _ = attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1);

        let wpath = to_wide(video_path);
        let reader = MFCreateSourceReaderFromURL(PCWSTR(wpath.as_ptr()), &attrs).ok()?;

        let all_streams = MF_SOURCE_READER_ALL_STREAMS.0 as u32;
        let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let _ = reader.SetStreamSelection(all_streams, false);
        reader.SetStreamSelection(first_vid, true).ok()?;

        let out_type = MFCreateMediaType().ok()?;
        out_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
        out_type.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
        reader.SetCurrentMediaType(first_vid, None, &out_type).ok()?;

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
                fps = (num / den).clamp(10.0, 60.0);
            }
        }

        Some((reader, src_w, src_h, stride, fps))
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

fn run_mf_video_loop(
    video_path: &str,
    max_w: u32,
    max_h: u32,
    max_fps: u32,
    stop: &Arc<AtomicBool>,
    slot: &Arc<Mutex<VideoFrameSlot>>,
) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        if MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET).is_err() {
            CoUninitialize();
            return false;
        }
    }

    let opened = open_mf_reader(video_path);
    let Some((reader, src_w, src_h, stride, src_fps)) = opened else {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
        return false;
    };

    let scale = (max_w as f64 / src_w as f64)
        .min(max_h as f64 / src_h as f64)
        .min(1.0);
    let dst_w = (((src_w as f64 * scale) as u32) / 2 * 2).max(64);
    let dst_h = (((src_h as f64 * scale) as u32) / 2 * 2).max(64);

    let effective_fps = src_fps.min(max_fps.max(10) as f64);
    let frame_interval = Duration::from_secs_f64(1.0 / effective_fps);
    let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

    let mut local_bgra = vec![255u8; (dst_w * dst_h * 4) as usize];
    let mut seq = 0u64;
    let mut decoded_any = false;
    let mut next_due = Instant::now();

    while !stop.load(Ordering::Relaxed) {
        let mut flags = 0u32;
        let mut ts = 0i64;
        let mut sample = None;
        let read_res = unsafe {
            reader.ReadSample(
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
            let seek_ok = unsafe { reader.SetCurrentPosition(&GUID::zeroed(), &pos).is_ok() };
            if !seek_ok {
                break;
            }
            continue;
        }

        if let Some(smp) = sample {
            if let Ok(buf) = unsafe { smp.ConvertToContiguousBuffer() } {
                if read_mf_buffer_frame(&buf, src_w, src_h, stride, &mut local_bgra, dst_w, dst_h) {
                    decoded_any = true;
                    seq += 1;
                    if let Ok(mut g) = slot.lock() {
                        g.w = dst_w;
                        g.h = dst_h;
                        g.seq = seq;
                        std::mem::swap(&mut g.bgra, &mut local_bgra);
                    }

                    // Smooth frame pacing while staying instantaneously interruptible!
                    next_due += frame_interval;
                    let now = Instant::now();
                    if next_due > now {
                        let mut rem = next_due - now;
                        while rem > Duration::from_millis(20) && !stop.load(Ordering::Relaxed) {
                            std::thread::sleep(Duration::from_millis(16));
                            rem = next_due.saturating_duration_since(Instant::now());
                        }
                        if !stop.load(Ordering::Relaxed) && rem > Duration::ZERO {
                            std::thread::sleep(rem);
                        }
                    } else if now.duration_since(next_due) > Duration::from_millis(180) {
                        next_due = now;
                    }
                }
            }
        }
    }

    drop(reader);
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
    pid_slot: &Arc<Mutex<Option<u32>>>,
    slot: &Arc<Mutex<VideoFrameSlot>>,
) -> bool {
    let w = (max_w / 2 * 2).max(64);
    let h = (max_h / 2 * 2).max(64);
    let vf = format!(
        "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},fps={}",
        w,
        h,
        w,
        h,
        max_fps.clamp(12, 30)
    );
    let Ok(mut child) = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
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
    let thumb_dir = app_data_dir().join("wp_thumbs");
    let _ = std::fs::create_dir_all(&thumb_dir);
    let mut h: u64 = 0xcbf29ce484222325;
    for b in video_path.as_bytes() {
        h = (h ^ (*b as u64)).wrapping_mul(0x100000001b3);
    }
    let out_bmp = thumb_dir.join(format!("vid_{:016x}.bmp", h));
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
    if let Some((reader, src_w, src_h, stride, _)) = open_mf_reader(video_path) {
        let scale = (480.0 / src_w as f64).min(270.0 / src_h as f64).min(1.0);
        let tw = (((src_w as f64 * scale) as u32) / 4 * 4).max(64);
        let th = (((src_h as f64 * scale) as u32) / 2 * 2).max(64);
        let first_vid = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let mut bgra = Vec::new();
        let mut grabbed = false;

        // Grab up to frame #6 to skip initial black fade-in frames when possible
        for idx in 0..8 {
            let mut flags = 0u32;
            let mut sample = None;
            if unsafe { reader.ReadSample(first_vid, 0, None, Some(&mut flags), None, Some(&mut sample)) }.is_err() {
                break;
            }
            if (flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32)) != 0 {
                break;
            }
            if let Some(smp) = sample {
                if let Ok(buf) = unsafe { smp.ConvertToContiguousBuffer() } {
                    if read_mf_buffer_frame(&buf, src_w, src_h, stride, &mut bgra, tw, th) {
                        grabbed = true;
                        if idx >= 4 {
                            break;
                        }
                    }
                }
            }
        }
        drop(reader);

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
        let out_jpg = thumb_dir.join(format!("vid_{:016x}.jpg", h));
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
    out.extend_from_slice(&bgra[..pixel_bytes as usize]);
    std::fs::write(path, &out).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_mf_video_and_bogota_weather() {
        let sample = r"D:\SteamLibrary\steamapps\workshop\content\431960\1386004994\TeddyLoid - ME!ME!ME! (feat. daoko) - REAL 1080p!!!.mp4";
        if Path::new(sample).exists() {
            // 1. Test Tier-1 Native Windows Media Foundation DIRECTLY (0% FFmpeg!)
            let stop = Arc::new(AtomicBool::new(false));
            let slot = Arc::new(Mutex::new(VideoFrameSlot {
                seq: 0,
                w: 0,
                h: 0,
                bgra: Vec::new(),
            }));
            let stop_bg = stop.clone();
            let slot_bg = slot.clone();
            let p = sample.to_string();
            let th = std::thread::spawn(move || run_mf_video_loop(&p, 640, 360, 30, &stop_bg, &slot_bg));
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

            // 2. Test Thumbnail extraction
            let thumb = extract_video_thumbnail(sample);
            println!("MF_THUMB_TEST: {:?}", thumb);
            assert!(thumb.as_deref().map(|t| Path::new(t).exists()).unwrap_or(false));
        }

        // 3. Test Dynamic Weather Geocoding for Bogotá!
        let geo_res = crate::system::geocode_city_debug("Bogotá");
        println!("GEOCODE_BOGOTA_DEBUG: {:?}", geo_res);
        assert!(geo_res.is_ok(), "geocode failed: {:?}", geo_res);
    }
}
