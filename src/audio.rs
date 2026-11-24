//! Real Windows Core Audio COM + IPolicyConfig (1-Click Output/Mic Switching) + WinRT SMTC Media Player

use crate::models::{now_unix_ms, AudioAppSession, AudioDevice, NowPlaying};
use crate::win32_shell::{get_process_info, to_wide};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use windows::core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR};
use windows::Win32::Foundation::S_OK;
use windows::Foundation::{AsyncStatus, IAsyncOperation};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

const PKEY_DEVICE_FRIENDLY_NAME: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
    pid: 14,
};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2,
    IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
};

const VK_MEDIA_NEXT: u8 = 0xB0;
const VK_MEDIA_PREV: u8 = 0xB1;
const VK_MEDIA_PLAY_PAUSE: u8 = 0xB3;

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[windows::core::interface("F8679F50-850A-41CF-9C72-430F290290C8")]
unsafe trait IPolicyConfig: IUnknown {
    fn get_mix_format(&self, a: PCWSTR, b: *mut std::ffi::c_void) -> HRESULT;
    fn get_device_format(&self, a: PCWSTR, b: i32, c: *mut std::ffi::c_void) -> HRESULT;
    fn reset_device_format(&self, a: PCWSTR) -> HRESULT;
    fn set_device_format(&self, a: PCWSTR, b: *mut std::ffi::c_void, c: *mut std::ffi::c_void) -> HRESULT;
    fn get_processing_period(&self, a: PCWSTR, b: i32, c: *mut i64, d: *mut i64) -> HRESULT;
    fn set_processing_period(&self, a: PCWSTR, b: *mut i64) -> HRESULT;
    fn get_share_mode(&self, a: PCWSTR, b: *mut std::ffi::c_void) -> HRESULT;
    fn set_share_mode(&self, a: PCWSTR, b: *mut std::ffi::c_void) -> HRESULT;
    fn get_property_value(&self, a: PCWSTR, b: *const std::ffi::c_void, c: *mut std::ffi::c_void) -> HRESULT;
    fn set_property_value(&self, a: PCWSTR, b: *const std::ffi::c_void, c: *mut std::ffi::c_void) -> HRESULT;
    fn set_default_endpoint(&self, wsz_device_id: PCWSTR, role: u32) -> HRESULT;
    fn set_endpoint_visibility(&self, a: PCWSTR, b: i32) -> HRESULT;
}

fn ensure_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn tap_media_key(vk: u8) {
    unsafe {
        keybd_event(vk, 0, KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(vk, 0, KEYEVENTF_KEYUP, 0);
    }
}

/// Runs SMTC transport controls on a detached MTA thread with strict 220ms bound + panic shield
/// so clicking Play/Pause/Prev/Next with ZERO songs playing NEVER blocks or crashes AuraShell!
fn dispatch_media_transport(kind: u8) {
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let mut handled = false;
            if let Some(mgr) = await_winrt_bounded(
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
                220,
            ) {
                if let Ok(sess) = mgr.GetCurrentSession() {
                    let op = match kind {
                        0 => sess.TryTogglePlayPauseAsync(),
                        1 => sess.TrySkipNextAsync(),
                        _ => sess.TrySkipPreviousAsync(),
                    };
                    if let Some(ok) = await_winrt_bounded(op, 220) {
                        handled = ok;
                    }
                }
            }
            if !handled {
                let vk = match kind {
                    0 => VK_MEDIA_PLAY_PAUSE,
                    1 => VK_MEDIA_NEXT,
                    _ => VK_MEDIA_PREV,
                };
                tap_media_key(vk);
            }
        });
    });
}

pub fn media_play_pause() {
    dispatch_media_transport(0);
}

pub fn media_next() {
    dispatch_media_transport(1);
}

pub fn media_prev() {
    dispatch_media_transport(2);
}

/// Seeks the active SMTC media session (YouTube, Spotify, Chrome, Edge, VLC...) to `frac` (0.0..=1.0).
/// Runs asynchronously on a detached MTA thread so clicking/dragging the timeline NEVER stalls the UI!
pub fn media_seek_fraction(frac: f32) {
    let frac = frac.clamp(0.0, 1.0) as f64;
    std::thread::spawn(move || {
        let _ = std::panic::catch_unwind(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            if let Some(mgr) = await_winrt_bounded(
                GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
                240,
            ) {
                if let Ok(sess) = mgr.GetCurrentSession() {
                    if let Ok(tl) = sess.GetTimelineProperties() {
                        let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
                        let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
                        if end > start {
                            let target = start + (((end - start) as f64) * frac).round() as i64;
                            let _ = await_winrt_bounded(
                                sess.TryChangePlaybackPositionAsync(target),
                                240,
                            );
                        }
                    }
                }
            }
        });
    });
}

thread_local! {
    static CACHED_SMTC_MGR: RefCell<Option<GlobalSystemMediaTransportControlsSessionManager>> = const { RefCell::new(None) };
}

fn get_cached_smtc_manager() -> Option<GlobalSystemMediaTransportControlsSessionManager> {
    if let Some(existing) = CACHED_SMTC_MGR.with(|c| c.borrow().clone()) {
        return Some(existing);
    }
    let fresh = await_winrt_bounded(
        GlobalSystemMediaTransportControlsSessionManager::RequestAsync(),
        250,
    )?;
    CACHED_SMTC_MGR.with(|c| *c.borrow_mut() = Some(fresh.clone()));
    Some(fresh)
}

fn current_utc_filetime_100ns() -> i64 {
    // 100-nanosecond intervals between 1601-01-01 and 1970-01-01
    const UNIX_EPOCH_FT: i64 = 116_444_736_000_000_000;
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    UNIX_EPOCH_FT + (dur.as_secs() as i64) * 10_000_000 + (dur.subsec_nanos() as i64) / 100
}

/// Non-blocking bounded wait for WinRT IAsyncOperation (prevents STA COM deadlocks if Edge/Spotify stalls!)
fn await_winrt_bounded<T: windows::core::RuntimeType>(
    op: windows::core::Result<IAsyncOperation<T>>,
    max_ms: u32,
) -> Option<T> {
    let op = op.ok()?;
    let steps = (max_ms / 8).max(1);
    for _ in 0..steps {
        match op.Status().ok()? {
            AsyncStatus::Completed => return op.GetResults().ok(),
            AsyncStatus::Started => std::thread::sleep(std::time::Duration::from_millis(8)),
            _ => return None,
        }
    }
    None
}

/// Real Windows SMTC live track title + artist + playback state + EXACT synced timeline (YouTube, Spotify, VLC...)
/// Uses a cached SMTC session manager (ZERO per-second WinRT broker RequestAsync leaks!)
pub fn get_now_playing() -> NowPlaying {
    let Some(mgr) = get_cached_smtc_manager() else {
        return NowPlaying::default();
    };
    let sess = match mgr.GetCurrentSession() {
        Ok(s) => s,
        Err(_) => {
            // Manager may have gone stale after explorer/audio restart; refresh once cleanly
            CACHED_SMTC_MGR.with(|c| *c.borrow_mut() = None);
            let Some(mgr2) = get_cached_smtc_manager() else {
                return NowPlaying::default();
            };
            match mgr2.GetCurrentSession() {
                Ok(s) => s,
                Err(_) => return NowPlaying::default(),
            }
        }
    };

    let is_playing = sess
        .GetPlaybackInfo()
        .and_then(|pi| pi.PlaybackStatus())
        .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        .unwrap_or(false);

    let src_raw = sess.SourceAppUserModelId().map(|s| s.to_string()).unwrap_or_default();
    let source_app = friendly_smtc_source(&src_raw);

    // Query REAL SMTC Timeline (StartTime, EndTime, Position + LastUpdatedTime drift correction!)
    let mut position_sec = 0.0f64;
    let mut duration_sec = 0.0f64;
    let mut has_timeline = false;
    let sampled_at_unix_ms = now_unix_ms();

    if let Ok(tl) = sess.GetTimelineProperties() {
        let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
        let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
        let last_upd = tl.LastUpdatedTime().map(|dt| dt.UniversalTime).unwrap_or(0);
        if end > start {
            let dur_s = (end - start) as f64 / 10_000_000.0;
            if dur_s >= 1.0 && dur_s <= 86_400.0 {
                let mut pos_s = (pos - start).max(0) as f64 / 10_000_000.0;
                if is_playing && last_upd > 0 {
                    let now_ft = current_utc_filetime_100ns();
                    if now_ft > last_upd {
                        let drift_s = (now_ft - last_upd) as f64 / 10_000_000.0;
                        if drift_s > 0.0 && drift_s < dur_s + 5.0 {
                            pos_s += drift_s;
                        }
                    }
                }
                position_sec = pos_s.clamp(0.0, dur_s);
                duration_sec = dur_s;
                has_timeline = true;
            }
        }
    }

    if let Some(props) = await_winrt_bounded(sess.TryGetMediaPropertiesAsync(), 250) {
        let title = props.Title().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
        let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default().trim().to_string();
        if !title.is_empty() {
            return NowPlaying {
                title,
                artist,
                source_app,
                is_playing,
                active: true,
                position_sec,
                duration_sec,
                has_timeline,
                sampled_at_unix_ms,
            };
        }
    }
    NowPlaying::default()
}

fn friendly_smtc_source(raw: &str) -> String {
    let low = raw.to_lowercase();
    if low.contains("spotify") { return "Spotify".to_string(); }
    if low.contains("msedge") || low.contains("edge") { return "Microsoft Edge".to_string(); }
    if low.contains("chrome") { return "Google Chrome".to_string(); }
    if low.contains("firefox") { return "Firefox".to_string(); }
    if low.contains("brave") { return "Brave".to_string(); }
    if low.contains("vlc") { return "VLC".to_string(); }
    if low.contains("discord") { return "Discord".to_string(); }
    "Multimedia".to_string()
}

// ==================== MASTER VOLUME ====================

fn default_endpoint_volume() -> Option<IAudioEndpointVolume> {
    ensure_com();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let dev: IMMDevice = en.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
        dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
    }
}

pub fn get_master_volume() -> (i32, bool) {
    if let Some(ep) = default_endpoint_volume() {
        unsafe {
            let scalar = ep.GetMasterVolumeLevelScalar().unwrap_or(0.5);
            let muted = ep.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            return ((scalar.clamp(0.0, 1.0) * 100.0).round() as i32, muted);
        }
    }
    (50, false)
}

pub fn set_master_volume(percent: i32) {
    if let Some(ep) = default_endpoint_volume() {
        unsafe {
            let scalar = (percent.clamp(0, 100) as f32) / 100.0;
            let _ = ep.SetMute(false, std::ptr::null());
            let _ = ep.SetMasterVolumeLevelScalar(scalar, std::ptr::null());
        }
    }
}

pub fn toggle_master_mute() -> bool {
    if let Some(ep) = default_endpoint_volume() {
        unsafe {
            let cur = ep.GetMute().map(|b| b.as_bool()).unwrap_or(false);
            let _ = ep.SetMute(!cur, std::ptr::null());
            return !cur;
        }
    }
    false
}

// ==================== OUTPUT & MICROPHONE DEVICE SWITCHING ====================

pub fn get_audio_devices(capture: bool) -> Vec<AudioDevice> {
    ensure_com();
    let mut list = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return list;
        };
        let flow = if capture { eCapture } else { eRender };
        let def_id = en
            .GetDefaultAudioEndpoint(flow, eMultimedia)
            .ok()
            .and_then(|d| d.GetId().ok())
            .and_then(|p| {
                let s = p.to_string().ok();
                windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
                s
            })
            .unwrap_or_default();

        let Ok(col) = en.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else { return list };
        let count = col.GetCount().unwrap_or(0);
        for i in 0..count {
            let Ok(dev) = col.Item(i) else { continue };
            let id = match dev.GetId() {
                Ok(p) => {
                    let s = p.to_string().unwrap_or_default();
                    windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
                    s
                }
                Err(_) => continue,
            };

            let mut friendly = id.clone();
            if let Ok(store) = dev.OpenPropertyStore(STGM_READ) {
                if let Ok(pv) = store.GetValue(&PKEY_DEVICE_FRIENDLY_NAME) {
                    let s = pv.to_string();
                    if !s.is_empty() {
                        friendly = s;
                    }
                }
            }

            let (short_name, sub_label) = split_device_name(&friendly);
            list.push(AudioDevice {
                is_default: id.eq_ignore_ascii_case(&def_id),
                id,
                full_name: friendly,
                short_name,
                sub_label,
                is_capture: capture,
            });
        }
    }
    list.sort_by(|a, b| b.is_default.cmp(&a.is_default).then_with(|| a.short_name.to_lowercase().cmp(&b.short_name.to_lowercase())));
    list
}

pub fn set_default_audio_device(device_id: &str) -> bool {
    ensure_com();
    unsafe {
        let Ok(policy) = CoCreateInstance::<_, IPolicyConfig>(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL) else {
            return false;
        };
        let w = to_wide(device_id);
        let r0 = policy.set_default_endpoint(PCWSTR(w.as_ptr()), eConsole.0 as u32);
        let r1 = policy.set_default_endpoint(PCWSTR(w.as_ptr()), eMultimedia.0 as u32);
        let r2 = policy.set_default_endpoint(PCWSTR(w.as_ptr()), 2); // eCommunications
        r0.is_ok() || r1.is_ok() || r2.is_ok()
    }
}

fn split_device_name(full: &str) -> (String, String) {
    if let Some(open) = full.find('(') {
        if full.ends_with(')') && open > 0 {
            let head = full[..open].trim();
            let tail = full[open + 1..full.len() - 1].trim();
            let low = head.to_lowercase();
            if matches!(low.as_str(), "altavoces" | "speakers" | "micrófono" | "microphone" | "auriculares") {
                return (tail.to_string(), head.to_string());
            }
            return (head.to_string(), tail.to_string());
        }
    }
    (full.to_string(), String::new())
}

// ==================== PER-APP GROUPED VOLUME MIXER ====================

fn friendly_app_name(proc_name: &str) -> String {
    match proc_name.to_lowercase().as_str() {
        "msedge" => "Microsoft Edge".to_string(),
        "chrome" => "Google Chrome".to_string(),
        "brave" => "Brave".to_string(),
        "firefox" => "Firefox".to_string(),
        "opera" => "Opera".to_string(),
        "spotify" => "Spotify".to_string(),
        "discord" => "Discord".to_string(),
        "whatsapp" => "WhatsApp".to_string(),
        "telegram" => "Telegram".to_string(),
        "leagueclientux" | "leagueclientuxrender" | "league of legends" => "League of Legends".to_string(),
        "riotclientservices" | "riotclientux" | "riotclientuxrender" => "Riot Client".to_string(),
        "valorant-win64-shipping" | "valorant" => "VALORANT".to_string(),
        "fortniteclient-win64-shipping" => "Fortnite".to_string(),
        "wallpaper64" | "wallpaper32" => "Wallpaper Engine".to_string(),
        "antigravity" => "Antigravity".to_string(),
        "steamwebhelper" | "steam" => "Steam".to_string(),
        "epicgameslauncher" => "Epic Games".to_string(),
        "obs64" => "OBS Studio".to_string(),
        "vlc" => "VLC Media Player".to_string(),
        "code" => "VS Code".to_string(),
        "aurashell" => "AuraOS Fondos".to_string(),
        "" => String::new(),
        other => {
            let mut ch = other.chars();
            match ch.next() {
                Some(first) => first.to_uppercase().collect::<String>() + ch.as_str(),
                None => String::new(),
            }
        }
    }
}

/// Fallback process stem extractor from Core Audio SessionIdentifier (works even for elevated/protected apps!)
fn stem_from_session_id(ctrl2: &IAudioSessionControl2) -> String {
    unsafe {
        if let Ok(p) = ctrl2.GetSessionIdentifier() {
            let s = p.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
            // Format: "...|\Device\HarddiskVolume3\...\msedge.exe%b{guid}"
            let cut = s.split("%b").next().unwrap_or(&s);
            if let Some(pos) = cut.rfind('\\') {
                let file = &cut[pos + 1..];
                let stem = file.strip_suffix(".exe")
                    .or_else(|| file.strip_suffix(".EXE"))
                    .unwrap_or(file);
                if !stem.is_empty() && !stem.contains('{') {
                    return stem.to_string();
                }
            }
        }
        if let Ok(p) = ctrl2.GetDisplayName() {
            let s = p.to_string().unwrap_or_default();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
            let trimmed = s.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('@') {
                return trimmed.to_string();
            }
        }
    }
    String::new()
}

fn all_active_render_devices() -> Vec<IMMDevice> {
    ensure_com();
    let mut devices = Vec::new();
    let mut seen = HashSet::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else {
            return devices;
        };
        if let Ok(def) = en.GetDefaultAudioEndpoint(eRender, eMultimedia) {
            if let Ok(p) = def.GetId() {
                let s = p.to_string().unwrap_or_default().to_lowercase();
                windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
                seen.insert(s);
            }
            devices.push(def);
        }
        if let Ok(col) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            let count = col.GetCount().unwrap_or(0);
            for i in 0..count {
                if let Ok(dev) = col.Item(i) {
                    if let Ok(p) = dev.GetId() {
                        let s = p.to_string().unwrap_or_default().to_lowercase();
                        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
                        if seen.insert(s) {
                            devices.push(dev);
                        }
                    }
                }
            }
        }
    }
    devices
}

pub fn get_app_audio_sessions() -> Vec<AudioAppSession> {
    let devices = all_active_render_devices();
    let mut grouped: HashMap<String, AudioAppSession> = HashMap::new();
    let mut sys_sounds: Option<AudioAppSession> = None;

    unsafe {
        for (d_idx, dev) in devices.iter().enumerate() {
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sess_enum) = mgr.GetSessionEnumerator() else { continue };
            let count = sess_enum.GetCount().unwrap_or(0);
            for i in 0..count {
                let Ok(base) = sess_enum.GetSession(i) else { continue };
                let Ok(ctrl2) = base.cast::<IAudioSessionControl2>() else { continue };
                let Ok(vol) = base.cast::<ISimpleAudioVolume>() else { continue };

                let state = ctrl2.GetState().map(|s| s.0).unwrap_or(2);
                if state == 2 {
                    continue; // Expired
                }
                let level = vol.GetMasterVolume().unwrap_or(1.0);
                let muted = vol.GetMute().map(|b| b.as_bool()).unwrap_or(false);
                let pct = (level.clamp(0.0, 1.0) * 100.0).round() as i32;
                let pid = ctrl2.GetProcessId().unwrap_or(0);

                // CRITICAL COM FIX: IsSystemSoundsSession() returns S_OK (0) for System Sounds
                // and S_FALSE (1) for normal apps! In windows-rs, S_FALSE.is_ok() is TRUE,
                // so we MUST compare strictly with == S_OK!
                let is_sys_session = pid == 0 || ctrl2.IsSystemSoundsSession() == S_OK;
                if is_sys_session {
                    if d_idx == 0 && sys_sounds.is_none() {
                        sys_sounds = Some(AudioAppSession {
                            display_name: "Sonidos del sistema".to_string(),
                            exe_path: String::new(),
                            volume_percent: pct,
                            is_muted: muted,
                            pids: vec![0],
                        });
                    }
                    continue;
                }

                let (mut pn, exe) = get_process_info(pid);
                if pn.is_empty() {
                    pn = stem_from_session_id(&ctrl2);
                }
                let name = friendly_app_name(&pn);
                if name.is_empty() {
                    continue;
                }
                let key = name.to_lowercase();
                if let Some(entry) = grouped.get_mut(&key) {
                    if !entry.pids.contains(&pid) {
                        entry.pids.push(pid);
                    }
                    if entry.exe_path.is_empty() && !exe.is_empty() {
                        entry.exe_path = exe;
                    }
                } else {
                    grouped.insert(
                        key,
                        AudioAppSession {
                            display_name: name,
                            exe_path: exe,
                            volume_percent: pct,
                            is_muted: muted,
                            pids: vec![pid],
                        },
                    );
                }
            }
        }
    }

    let mut out: Vec<AudioAppSession> = grouped.into_values().collect();
    out.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    out.truncate(12);
    if let Some(sys) = sys_sounds {
        out.push(sys);
    }
    out
}

pub fn set_app_session_volume(pids: &[u32], percent: i32) {
    let scalar = (percent.clamp(0, 100) as f32) / 100.0;
    mutate_app_sessions(pids, |vol| unsafe {
        let _ = vol.SetMute(false, std::ptr::null());
        let _ = vol.SetMasterVolume(scalar, std::ptr::null());
    });
}

pub fn set_app_session_mute(pids: &[u32], muted: bool) {
    mutate_app_sessions(pids, |vol| unsafe {
        let _ = vol.SetMute(muted, std::ptr::null());
    });
}

fn mutate_app_sessions<F: Fn(&ISimpleAudioVolume)>(pids: &[u32], f: F) {
    let devices = all_active_render_devices();
    let is_sys = pids.contains(&0);
    unsafe {
        for (d_idx, dev) in devices.iter().enumerate() {
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(sess_enum) = mgr.GetSessionEnumerator() else { continue };
            let count = sess_enum.GetCount().unwrap_or(0);
            for i in 0..count {
                let Ok(base) = sess_enum.GetSession(i) else { continue };
                let Ok(ctrl2) = base.cast::<IAudioSessionControl2>() else { continue };
                let Ok(vol) = base.cast::<ISimpleAudioVolume>() else { continue };
                let pid = ctrl2.GetProcessId().unwrap_or(0);
                if is_sys && d_idx == 0 && (pid == 0 || ctrl2.IsSystemSoundsSession() == S_OK) {
                    f(&vol);
                } else if !is_sys && pid != 0 && ctrl2.IsSystemSoundsSession() != S_OK && pids.contains(&pid) {
                    f(&vol);
                }
            }
        }
    }
}
