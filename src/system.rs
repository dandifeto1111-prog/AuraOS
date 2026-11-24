//! Live Telemetry (CPU/RAM/Drives), Native WinHTTP Open-Meteo Weather,
//! Fast App/Game/Tray/DevPort Scanners, and Reversible Game Boost

use crate::models::{
    AppEntry, BackgroundApp, CustomItemsStore, DevPort, DriveInfo, GameEntry, WeatherInfo,
};
use crate::win32_shell::{get_process_info, resolve_shortcut_target, to_wide};
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
    WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_FLAG_SECURE,
};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetSystemTimes, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
};

// ==================== LIVE CPU, RAM & STORAGE ====================

pub struct CpuTracker {
    prev_idle: u64,
    prev_kernel: u64,
    prev_user: u64,
    pub history: Vec<f32>,
}

impl CpuTracker {
    pub fn new() -> Self {
        let (i, k, u) = read_sys_times();
        Self {
            prev_idle: i,
            prev_kernel: k,
            prev_user: u,
            history: vec![12.0; 24],
        }
    }

    pub fn sample(&mut self) -> f32 {
        let (i, k, u) = read_sys_times();
        let di = i.saturating_sub(self.prev_idle);
        let dk = k.saturating_sub(self.prev_kernel);
        let du = u.saturating_sub(self.prev_user);
        self.prev_idle = i;
        self.prev_kernel = k;
        self.prev_user = u;
        let total = dk + du;
        let pct = if total > 0 {
            (((total.saturating_sub(di)) as f64 / total as f64) * 100.0).clamp(0.0, 100.0) as f32
        } else {
            0.0
        };
        if self.history.len() >= 24 {
            self.history.remove(0);
        }
        self.history.push(pct);
        pct
    }
}

fn ft_to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn read_sys_times() -> (u64, u64, u64) {
    unsafe {
        let mut idle = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).is_ok() {
            (ft_to_u64(idle), ft_to_u64(kernel), ft_to_u64(user))
        } else {
            (0, 0, 0)
        }
    }
}

pub fn get_ram_stats() -> (f64, f64, i32) {
    unsafe {
        let mut ms = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut ms).is_ok() {
            let total = (ms.ullTotalPhys as f64) / (1024.0 * 1024.0 * 1024.0);
            let avail = (ms.ullAvailPhys as f64) / (1024.0 * 1024.0 * 1024.0);
            let used = (total - avail).max(0.0);
            return (used, total, ms.dwMemoryLoad as i32);
        }
    }
    (0.0, 16.0, 0)
}

pub fn get_local_drives() -> Vec<DriveInfo> {
    let mut drives = Vec::new();
    unsafe {
        let mask = GetLogicalDrives();
        for idx in 0..26u32 {
            if (mask & (1 << idx)) == 0 {
                continue;
            }
            let letter = format!("{}:", (b'A' + idx as u8) as char);
            let root = to_wide(&format!("{}\\", letter));
            let mut free_avail = 0u64;
            let mut total_bytes = 0u64;
            let mut total_free = 0u64;
            if GetDiskFreeSpaceExW(
                PCWSTR(root.as_ptr()),
                Some(&mut free_avail),
                Some(&mut total_bytes),
                Some(&mut total_free),
            )
            .is_ok()
                && total_bytes > 1_000_000_000
            {
                let total_gb = total_bytes as f64 / 1_073_741_824.0;
                let free_gb = total_free as f64 / 1_073_741_824.0;
                let used_gb = (total_gb - free_gb).max(0.0);
                let used_pct = ((used_gb / total_gb) * 100.0).clamp(0.0, 100.0) as f32;
                drives.push(DriveInfo { letter, used_gb, total_gb, free_gb, used_pct });
            }
        }
    }
    drives.truncate(4);
    drives
}

// ==================== NATIVE WINHTTP AUTO-GEOLOCATION & OPEN-METEO WEATHER ====================

fn winhttp_get(host: &str, port: u16, path: &str, secure: bool) -> Option<String> {
    unsafe {
        let ua = to_wide("AuraOS/4.5");
        let h_sess = WinHttpOpen(
            PCWSTR(ua.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if h_sess.is_null() {
            return None;
        }
        let _ = WinHttpSetTimeouts(h_sess, 5000, 6000, 6000, 7000);
        let whost = to_wide(host);
        let h_conn = WinHttpConnect(h_sess, PCWSTR(whost.as_ptr()), port, 0);
        if h_conn.is_null() {
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }
        let verb = to_wide("GET");
        let wpath = to_wide(path);
        let flags = if secure {
            WINHTTP_FLAG_SECURE
        } else {
            windows::Win32::Networking::WinHttp::WINHTTP_OPEN_REQUEST_FLAGS(0)
        };
        let h_req = WinHttpOpenRequest(
            h_conn,
            PCWSTR(verb.as_ptr()),
            PCWSTR(wpath.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            flags,
        );
        if h_req.is_null() {
            let _ = WinHttpCloseHandle(h_conn);
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }

        let ok = WinHttpSendRequest(h_req, None, None, 0, 0, 0).is_ok()
            && WinHttpReceiveResponse(h_req, std::ptr::null_mut()).is_ok();
        let mut body = Vec::new();
        if ok {
            loop {
                let mut avail = 0u32;
                if WinHttpQueryDataAvailable(h_req, &mut avail).is_err() || avail == 0 {
                    break;
                }
                let mut chunk = vec![0u8; avail as usize];
                let mut read = 0u32;
                if WinHttpReadData(h_req, chunk.as_mut_ptr() as *mut _, avail, &mut read).is_err()
                    || read == 0
                {
                    break;
                }
                body.extend_from_slice(&chunk[..read as usize]);
                if body.len() > 256 * 1024 {
                    break;
                }
            }
        }
        let _ = WinHttpCloseHandle(h_req);
        let _ = WinHttpCloseHandle(h_conn);
        let _ = WinHttpCloseHandle(h_sess);
        if body.is_empty() {
            None
        } else {
            String::from_utf8(body).ok()
        }
    }
}

fn winhttp_get_https(host: &str, path: &str) -> Option<String> {
    winhttp_get(host, 443, path, true)
}

fn url_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        let c = *b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else if c == ' ' {
            out.push_str("%20");
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn parse_json_f64(val: &serde_json::Value, key: &str) -> Option<f64> {
    let v = val.get(key)?;
    if let Some(f) = v.as_f64() {
        Some(f)
    } else if let Some(s) = v.as_str() {
        s.trim().parse::<f64>().ok()
    } else {
        None
    }
}

pub fn geocode_city_debug(city_name: &str) -> Result<(String, f64, f64), String> {
    let q = url_encode_query(city_name.trim());
    let path = format!("/v1/search?name={}&count=1&language=es&format=json", q);
    let body = winhttp_get_https("geocoding-api.open-meteo.com", &path)
        .ok_or_else(|| format!("http_none path={}", path))?;
    let val: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("json_err={} body={}", e, &body[..body.len().min(200)]))?;
    let first = val
        .get("results")
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| format!("no_results body={}", &body[..body.len().min(200)]))?;
    let name = first.get("name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let lat = parse_json_f64(first, "latitude").ok_or("no_lat")?;
    let lon = parse_json_f64(first, "longitude").ok_or("no_lon")?;
    if name.is_empty() || !lat.is_finite() || !lon.is_finite() {
        return Err("invalid_fields".into());
    }
    Ok((name, lat, lon))
}

fn geocode_city(city_name: &str) -> Option<(String, f64, f64)> {
    geocode_city_debug(city_name).ok()
}

fn detect_location_by_ip() -> Option<(String, f64, f64)> {
    // Provider 1: ipwho.is (HTTPS, clean UTF-8 city names worldwide, no API key)
    if let Some(body) = winhttp_get_https("ipwho.is", "/") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if v.get("success").and_then(|x| x.as_bool()).unwrap_or(true) {
                if let (Some(city), Some(lat), Some(lon)) = (
                    v.get("city").and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                    parse_json_f64(&v, "latitude"),
                    parse_json_f64(&v, "longitude"),
                ) {
                    return Some((city, lat, lon));
                }
            }
        }
    }
    // Provider 2: get.geojs.io (HTTPS, very fast, no strict rate limits)
    if let Some(body) = winhttp_get_https("get.geojs.io", "/v1/ip/geo.json") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if let (Some(city), Some(lat), Some(lon)) = (
                v.get("city").and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                parse_json_f64(&v, "latitude"),
                parse_json_f64(&v, "longitude"),
            ) {
                return Some((city, lat, lon));
            }
        }
    }
    // Provider 3: ipapi.co (HTTPS)
    if let Some(body) = winhttp_get_https("ipapi.co", "/json/") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if let (Some(city), Some(lat), Some(lon)) = (
                v.get("city").and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                parse_json_f64(&v, "latitude"),
                parse_json_f64(&v, "longitude"),
            ) {
                return Some((city, lat, lon));
            }
        }
    }
    // Provider 4: ip-api.com (HTTP fallback)
    if let Some(body) = winhttp_get("ip-api.com", 80, "/json/?fields=status,city,lat,lon", false) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
            if v.get("status").and_then(|x| x.as_str()) == Some("success") {
                if let (Some(city), Some(lat), Some(lon)) = (
                    v.get("city").and_then(|x| x.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                    parse_json_f64(&v, "lat"),
                    parse_json_f64(&v, "lon"),
                ) {
                    return Some((city, lat, lon));
                }
            }
        }
    }
    None
}

pub fn load_cached_weather() -> Option<WeatherInfo> {
    let p = crate::models::app_data_dir().join("weather_cache.json");
    let raw = std::fs::read_to_string(p).ok()?;
    let w: WeatherInfo = serde_json::from_str(&raw).ok()?;
    if w.loaded && !w.city.trim().is_empty() {
        Some(w)
    } else {
        None
    }
}

fn save_cached_weather(info: &WeatherInfo) {
    if !info.loaded || info.city.trim().is_empty() {
        return;
    }
    let dir = crate::models::app_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(json) = serde_json::to_string_pretty(info) {
        let _ = std::fs::write(dir.join("weather_cache.json"), json);
    }
}

pub fn fetch_weather_for_city(override_city: &str) -> WeatherInfo {
    let mut info = if override_city.trim().is_empty() {
        load_cached_weather().unwrap_or_default()
    } else {
        WeatherInfo::default()
    };
    let loc = if !override_city.trim().is_empty() {
        geocode_city(override_city).or_else(detect_location_by_ip)
    } else {
        detect_location_by_ip()
    };

    let Some((city, lat, lon)) = loc else {
        return info;
    };

    let path = format!(
        "/v1/forecast?latitude={:.4}&longitude={:.4}&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m&timezone=auto",
        lat, lon
    );
    if let Some(body) = winhttp_get_https("api.open-meteo.com", &path) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(cur) = val.get("current") {
                info.city = city;
                info.temp_c = cur.get("temperature_2m").and_then(|v| v.as_f64()).unwrap_or(20.0).round() as i32;
                info.feels_like_c = cur.get("apparent_temperature").and_then(|v| v.as_f64()).unwrap_or(info.temp_c as f64).round() as i32;
                info.humidity_pct = cur.get("relative_humidity_2m").and_then(|v| v.as_f64()).unwrap_or(65.0).round() as i32;
                info.wind_kmh = cur.get("wind_speed_10m").and_then(|v| v.as_f64()).unwrap_or(10.0).round() as i32;
                info.weather_code = cur.get("weather_code").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                info.condition = wmo_condition(info.weather_code).to_string();
                info.loaded = true;
                save_cached_weather(&info);
            }
        }
    }
    info
}

pub fn fetch_weather_blocking() -> WeatherInfo {
    let settings = crate::models::load_settings();
    fetch_weather_for_city(&settings.weather_city)
}

fn wmo_condition(code: i32) -> &'static str {
    match code {
        0 => "Despejado",
        1..=3 => "Parcial nublado",
        45 | 48 => "Neblina",
        51..=67 => "Lluvia ligera",
        80..=82 => "Chubascos",
        95..=99 => "Tormenta",
        _ => "Nublado",
    }
}

// ==================== ALL PC APPS + DESKTOP + GAMES + DEV AI SCANNERS ====================

pub fn scan_all_apps(custom: &CustomItemsStore) -> Vec<AppEntry> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();

    let mut dirs = Vec::new();
    if let Ok(pd) = std::env::var("ProgramData") {
        dirs.push((PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs"), false));
    }
    if let Ok(ad) = std::env::var("APPDATA") {
        dirs.push((PathBuf::from(ad).join(r"Microsoft\Windows\Start Menu\Programs"), false));
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        dirs.push((PathBuf::from(&up).join("Desktop"), true));
        dirs.push((PathBuf::from(&up).join(r"OneDrive\Desktop"), true));
        dirs.push((PathBuf::from(&up).join(r"OneDrive\Escritorio"), true));
    }
    dirs.push((PathBuf::from(r"C:\Users\Public\Desktop"), true));

    for (dir, is_desktop) in dirs {
        if dir.exists() {
            walk_shortcuts(&dir, is_desktop, 0, &mut seen, &mut apps);
        }
    }

    // Always register ALL native AuraOS v5.0 Rust Suite apps with crisp 256px icons!
    for (display, exe, cat) in [
        ("AuraFiles", "AuraFiles.exe", "AuraOS"),
        ("AuraPDF Studio", "AuraPDF.exe", "AuraOS"),
        ("AuraPhotos Studio", "AuraPhotos.exe", "AuraOS"),
        ("AuraShot Capturas", "AuraShot.exe", "AuraOS"),
        ("AuraTasks Monitor", "AuraTasks.exe", "AuraOS"),
        ("AuraPlayer", "AuraPlayer.exe", "AuraOS"),
        ("AuraNotes", "AuraNotes.exe", "AuraOS"),
        ("AuraSettings", "AuraSettings.exe", "AuraOS"),
        ("AuraLock", "AuraLock.exe", "AuraOS"),
    ] {
        if let Some(p) = crate::win32_shell::resolve_aura_app_path(exe) {
            let p_str = p.to_string_lossy().to_string();
            if seen.insert(display.to_lowercase()) {
                apps.push(AppEntry {
                    name: display.to_string(),
                    target: p_str.clone(),
                    icon_path: p_str,
                    category: cat.to_string(),
                    is_web: false,
                    is_custom: false,
                    custom_id: None,
                });
            }
        }
    }

    // Include custom Dev/AI apps
    for c in &custom.dev_ai_items {
        let key = c.name.to_lowercase();
        if seen.insert(key) {
            apps.push(AppEntry {
                name: c.name.clone(),
                target: c.target.clone(),
                icon_path: if c.target.starts_with("http") { String::new() } else { c.target.clone() },
                category: "AI".to_string(),
                is_web: c.target.starts_with("http"),
                is_custom: true,
                custom_id: Some(c.id.clone()),
            });
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

fn walk_shortcuts(
    dir: &Path,
    is_desktop: bool,
    depth: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<AppEntry>,
) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_shortcuts(&path, is_desktop, depth + 1, seen, out);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext != "lnk" && !(is_desktop && ext == "exe") {
            continue;
        }
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").trim().to_string();
        if name.is_empty() {
            continue;
        }
        let low = name.to_lowercase();
        if low.contains("uninstall")
            || low.contains("desinstalar")
            || low.contains("readme")
            || low.contains("help")
            || low.contains("ayuda ")
            || low.contains("manual")
            || low.contains("documentation")
            || low.contains("release notes")
            || low.contains("install additional")
            || low.contains("activate or trial")
            || low.contains("aurashell")
            || low.contains("actualizar o configurar")
            || low.contains("restaurar barra")
            || low == "administrative tools"
            || low == "component services"
            || low == "computer management"
            || low == "dfrgui"
            || low == "disk cleanup"
            || low == "event viewer"
            || low == "character map"
        {
            continue;
        }
        if !seen.insert(low.clone()) {
            continue;
        }
        let p_str = path.to_string_lossy().to_string();
        let resolved = if ext == "lnk" {
            resolve_shortcut_target(&p_str).unwrap_or_else(|| p_str.clone())
        } else {
            p_str.clone()
        };

        let cat = if low.contains("code")
            || low.contains("antigravity")
            || low.contains("terminal")
            || low.contains("git")
            || low.contains("python")
            || low.contains("node")
            || low.contains("docker")
            || low.contains("postman")
        {
            "Dev"
        } else if low.contains("gemini") || low.contains("chatgpt") || low.contains("claude") || low.contains("copilot") {
            "AI"
        } else if is_desktop {
            "Desktop"
        } else {
            "App"
        };

        out.push(AppEntry {
            name,
            target: p_str,
            icon_path: resolved,
            category: cat.to_string(),
            is_web: false,
            is_custom: false,
            custom_id: None,
        });
    }
}

pub fn build_dev_ai_entries(all_apps: &[AppEntry], custom: &CustomItemsStore) -> Vec<AppEntry> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    // User's manually added AI/Dev apps FIRST so they're always immediately accessible!
    for c in &custom.dev_ai_items {
        seen.insert(c.name.to_lowercase());
        items.push(AppEntry {
            name: c.name.clone(),
            target: c.target.clone(),
            icon_path: if c.target.starts_with("http") { String::new() } else { c.target.clone() },
            category: c.subtitle.clone(),
            is_web: c.target.starts_with("http"),
            is_custom: true,
            custom_id: Some(c.id.clone()),
        });
    }

    // Essential built-in AI Web tools right after custom items so IAs are front and center!
    let web_ais = [
        ("Claude AI", "https://claude.ai"),
        ("ChatGPT", "https://chatgpt.com"),
        ("Gemini", "https://gemini.google.com"),
        ("NotebookLM", "https://notebooklm.google.com"),
        ("Perplexity", "https://www.perplexity.ai"),
        ("GitHub", "https://github.com"),
    ];
    for (name, url) in web_ais {
        if seen.insert(name.to_lowercase()) {
            items.push(AppEntry {
                name: name.to_string(),
                target: url.to_string(),
                icon_path: String::new(),
                category: "IA Web".to_string(),
                is_web: true,
                is_custom: false,
                custom_id: None,
            });
        }
    }

    // Core local Dev/AI apps detected on PC (filtered cleanly)
    for a in all_apps {
        let low = a.name.to_lowercase();
        if low.contains("idle") || low.contains("command prompt") || low.contains("git gui") || low.contains("git cmd") {
            continue;
        }
        if (a.category == "Dev" || a.category == "AI" || low.contains("antigravity") || low.contains("notion") || low.contains("cursor") || low.contains("opencode") || low.contains("comfy"))
            && seen.insert(low)
        {
            items.push(a.clone());
        }
    }
    items
}

pub fn scan_all_games(custom: &CustomItemsStore) -> Vec<GameEntry> {
    let mut games = Vec::new();
    let mut seen = HashSet::new();

    // 1. User's manually added games FIRST!
    for cg in &custom.custom_games {
        seen.insert(cg.name.to_lowercase());
        games.push(GameEntry {
            name: cg.name.clone(),
            platform: "Manual".to_string(),
            launch_target: cg.target.clone(),
            icon_path: cg.target.clone(),
            cover_path: cg.cover_path.clone(),
            is_custom: true,
            custom_id: Some(cg.id.clone()),
        });
    }

    // 2. Steam libraries (C: and D:)
    let steam_libs = [
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps"),
        PathBuf::from(r"D:\SteamLibrary\steamapps"),
    ];
    for lib in &steam_libs {
        if let Ok(entries) = std::fs::read_dir(lib) {
            for e in entries.flatten() {
                let p = e.path();
                let fname = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if !fname.starts_with("appmanifest_") || !fname.ends_with(".acf") {
                    continue;
                }
                if let Ok(txt) = std::fs::read_to_string(&p) {
                    let appid = extract_acf_val(&txt, "appid").unwrap_or_default();
                    let name = extract_acf_val(&txt, "name").unwrap_or_default();
                    let low = name.to_lowercase();
                    if name.is_empty()
                        || low.contains("redistributable")
                        || low.contains("wallpaper engine")
                        || low.contains("proton")
                        || low.contains("steamworks")
                    {
                        continue;
                    }
                    if !seen.insert(low) {
                        continue;
                    }
                    let cover = [
                        format!(r"C:\Program Files (x86)\Steam\appcache\librarycache\{}_library_600x900.jpg", appid),
                        format!(r"C:\Program Files (x86)\Steam\appcache\librarycache\{}\library_600x900.jpg", appid),
                        format!(r"C:\Program Files (x86)\Steam\appcache\librarycache\{}_header.jpg", appid),
                        format!(r"C:\Program Files (x86)\Steam\appcache\librarycache\{}\header.jpg", appid),
                    ]
                    .into_iter()
                    .find(|c| Path::new(c).exists());

                    games.push(GameEntry {
                        name,
                        platform: "Steam".to_string(),
                        launch_target: format!("steam://rungameid/{}", appid),
                        icon_path: r"C:\Program Files (x86)\Steam\steam.exe".to_string(),
                        cover_path: cover,
                        is_custom: false,
                        custom_id: None,
                    });
                }
            }
        }
    }

    // 3. Epic Games manifests
    let epic_dir = PathBuf::from(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    if let Ok(entries) = std::fs::read_dir(epic_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("item") {
                continue;
            }
            if let Ok(txt) = std::fs::read_to_string(&p) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                    let name = v.get("DisplayName").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let app_name = v.get("AppName").and_then(|x| x.as_str()).unwrap_or("");
                    let install_loc = v.get("InstallLocation").and_then(|x| x.as_str()).unwrap_or("");
                    let launch_exe = v.get("LaunchExecutable").and_then(|x| x.as_str()).unwrap_or("");
                    let low = name.to_lowercase();
                    if name.is_empty() || low.contains("contenido de") || low.contains("dlc") || !seen.insert(low) {
                        continue;
                    }
                    let full_exe = if !install_loc.is_empty() && !launch_exe.is_empty() {
                        PathBuf::from(install_loc).join(launch_exe).to_string_lossy().to_string()
                    } else {
                        String::new()
                    };
                    let splash = if !install_loc.is_empty() {
                        let s1 = PathBuf::from(install_loc).join(r"FortniteGame\Content\Splash\Splash.bmp");
                        if s1.exists() { Some(s1.to_string_lossy().to_string()) } else { None }
                    } else {
                        None
                    };
                    games.push(GameEntry {
                        name,
                        platform: "Epic".to_string(),
                        launch_target: format!("com.epicgames.launcher://apps/{}?action=launch&silent=true", app_name),
                        icon_path: full_exe,
                        cover_path: splash,
                        is_custom: false,
                        custom_id: None,
                    });
                }
            }
        }
    }

    // 4. Riot Games (League of Legends, VALORANT, 2XKO, etc.)
    let riot_root = PathBuf::from(r"C:\Riot Games");
    let riot_client = r"C:\Riot Games\Riot Client\RiotClientServices.exe";
    if riot_root.exists() {
        let riot_known = [
            ("League of Legends", r"C:\Riot Games\League of Legends\LeagueClient.exe", "league_of_legends"),
            ("VALORANT", r"C:\Riot Games\VALORANT\live\VALORANT.exe", "valorant"),
            ("2XKO", r"C:\Riot Games\2XKO\live\2XKO.exe", "lion"),
        ];
        for (name, exe, _) in riot_known {
            if Path::new(exe).exists() && seen.insert(name.to_lowercase()) {
                games.push(GameEntry {
                    name: name.to_string(),
                    platform: "Riot".to_string(),
                    launch_target: exe.to_string(),
                    icon_path: exe.to_string(),
                    cover_path: None,
                    is_custom: false,
                    custom_id: None,
                });
            }
        }
        // Fallback if League folder exists with RiotClient
        let lol_dir = riot_root.join("League of Legends");
        if lol_dir.exists() && seen.insert("league of legends".to_string()) {
            let ico = lol_dir.join("LeagueClient.exe");
            games.push(GameEntry {
                name: "League of Legends".to_string(),
                platform: "Riot".to_string(),
                launch_target: if ico.exists() { ico.to_string_lossy().to_string() } else { riot_client.to_string() },
                icon_path: if ico.exists() { ico.to_string_lossy().to_string() } else { riot_client.to_string() },
                cover_path: None,
                is_custom: false,
                custom_id: None,
            });
        }
    }

    // 5. Custom local game folders (D:\juegos) + SKlauncher
    let local_game_dir = PathBuf::from(r"D:\juegos");
    if let Ok(entries) = std::fs::read_dir(local_game_dir) {
        for e in entries.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let folder_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
            if folder_name.is_empty() || !seen.insert(folder_name.to_lowercase()) {
                continue;
            }
            if let Some(exe) = find_main_game_exe(&p, 0) {
                games.push(GameEntry {
                    name: folder_name,
                    platform: "Local".to_string(),
                    launch_target: exe.clone(),
                    icon_path: exe,
                    cover_path: None,
                    is_custom: false,
                    custom_id: None,
                });
            }
        }
    }

    games
}

fn find_main_game_exe(dir: &Path, depth: usize) -> Option<String> {
    if depth > 4 {
        return None;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return None };
    let mut best: Option<(u64, String)> = None;
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            subdirs.push(p);
            continue;
        }
        if p.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("exe")).unwrap_or(false) {
            let low = p.file_name().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            if low.contains("unins") || low.contains("crash") || low.contains("redist") || low.contains("setup") {
                continue;
            }
            let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().map(|b| sz > b.0).unwrap_or(true) {
                best = Some((sz, p.to_string_lossy().to_string()));
            }
        }
    }
    if let Some((_, exe)) = best {
        return Some(exe);
    }
    for s in subdirs {
        if let Some(exe) = find_main_game_exe(&s, depth + 1) {
            return Some(exe);
        }
    }
    None
}

fn extract_acf_val(txt: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    for line in txt.lines() {
        let t = line.trim();
        if t.starts_with(&needle) {
            let rest = t[needle.len()..].trim();
            if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                return Some(rest[1..rest.len() - 1].to_string());
            }
        }
    }
    None
}

pub fn scan_dev_ports() -> Vec<DevPort> {
    let ports = [
        (3000, "Next / React"),
        (5173, "Vite Dev"),
        (5174, "Vite Alt"),
        (8000, "Python / FastAPI"),
        (8080, "HTTP Server"),
        (4200, "Angular"),
        (11434, "Ollama AI"),
        (8384, "Syncthing"),
    ];
    let mut active = Vec::new();
    for (port, label) in ports {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if TcpStream::connect_timeout(&addr, Duration::from_millis(12)).is_ok() {
            active.push(DevPort {
                port,
                label: label.to_string(),
                url: format!("http://localhost:{}", port),
            });
        }
    }
    active
}

// ==================== BACKGROUND TRAY APPS ====================

pub fn scan_background_tray_apps() -> Vec<BackgroundApp> {
    let known = [
        ("wallpaper64", "Wallpaper Eng.", "steam://rungameid/431960"),
        ("Discord", "Discord", ""),
        ("steam", "Steam", "steam://open/main"),
        ("EpicGamesLauncher", "Epic Games", "com.epicgames.launcher://"),
        ("RiotClientServices", "Riot Client", r"C:\Riot Games\Riot Client\RiotClientServices.exe"),
        ("vgtray", "Vanguard", ""),
        ("NVIDIA App", "NVIDIA App", ""),
        ("NVIDIA Overlay", "NVIDIA", ""),
        ("GeminiAppLauncher", "Gemini", ""),
        ("GoogleDriveFS", "Google Drive", ""),
        ("nearby_share", "Nearby Share", ""),
        ("MSPCManager", "PC Manager", ""),
        ("PhoneExperienceHost", "Enlace Móvil", "ms-phone:"),
        ("syncthing", "Syncthing", "http://127.0.0.1:8384"),
        ("Spotify", "Spotify", ""),
        ("WsaClient", "Android WSA", ""),
        ("Notion", "Notion", ""),
        ("obs64", "OBS Studio", ""),
        ("OneDrive", "OneDrive", ""),
        ("PowerToys", "PowerToys", ""),
        ("SecurityHealthSystray", "Seguridad", "windowsdefender:"),
        ("ShareX", "ShareX", ""),
        ("MSIAfterburner", "Afterburner", ""),
        ("RTSS", "RivaTuner", ""),
        ("lghub_agent", "Logitech G", ""),
        ("Telegram", "Telegram", ""),
        ("WhatsApp", "WhatsApp", ""),
        ("qBittorrent", "qBittorrent", ""),
        ("Everything", "Everything", ""),
        ("Tailscale-IPN", "Tailscale", ""),
        ("Cloudflare WARP", "WARP VPN", ""),
    ];

    #[link(name = "kernel32")]
    extern "system" {
        fn ProcessIdToSessionId(dwProcessId: u32, pSessionId: *mut u32) -> i32;
    }

    // Collect ALL running processes in the current user session (pid + stem)
    let my_pid = std::process::id();
    let mut my_session = 0u32;
    unsafe {
        let _ = ProcessIdToSessionId(my_pid, &mut my_session);
    }

    let mut running_procs: HashMap<String, (u32, String)> = HashMap::new(); // stem_lower -> (pid, original_stem)
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut pe = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let pid = pe.th32ProcessID;
                    let mut sess = 0u32;
                    let same_sess = ProcessIdToSessionId(pid, &mut sess) != 0 && sess == my_session;
                    if same_sess && pid != my_pid {
                        let raw = crate::win32_shell::from_wide(&pe.szExeFile);
                        let orig_stem = Path::new(&raw).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        if !orig_stem.is_empty() {
                            running_procs.entry(orig_stem.to_lowercase()).or_insert((pid, orig_stem));
                        }
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = windows::Win32::Foundation::CloseHandle(snap);
        }
    }

    let open_wins = crate::win32_shell::get_open_user_windows();
    let mut out = Vec::new();
    let mut seen_keys = HashSet::new();
    let mut seen_disp = HashSet::new();

    // Pass 1: Known friendly tray apps (with clean curated display names & deep-link fallbacks)
    for (key, disp, fallback) in known {
        let klow = key.to_lowercase();
        if seen_disp.contains(disp) {
            continue;
        }
        if let Some(&(pid, _)) = running_procs.get(&klow) {
            seen_keys.insert(klow);
            seen_disp.insert(disp.to_string());
            let (_, exe_path) = get_process_info(pid);
            let hwnd = open_wins
                .iter()
                .find(|w| w.process_name.eq_ignore_ascii_case(key))
                .map(|w| w.hwnd)
                .unwrap_or(0);
            out.push(BackgroundApp {
                process_key: key.to_string(),
                display_name: disp.to_string(),
                exe_path,
                fallback_launch: fallback.to_string(),
                main_hwnd: hwnd,
            });
        }
    }

    // Pass 2: Dynamic auto-discovery of ALL other running user-session background/tray apps!
    // Ensures NO third-party tray app in Windows' hidden tray (^) is ever missed!
    let ignored_substrings = [
        "crashpad", "crashuploader", "webhelper", "updater", "identity_helper", "helper",
        "broker", "language_server", "mcp", "webview", "aurashell", "aurafiles",
        "conhost", "powershell", "pwsh", "cmd", "node", "python", "git", "cargo", "rustc",
        "svchost", "dllhost", "sihost", "taskhost", "explorer", "smartscreen", "ctfmon",
        "searchhost", "startmenu", "shellexperience", "shellhost", "textinput", "lockapp",
        "widgetboard", "widgetservice", "crossdevice", "startfeed", "monotification",
        "rtkaudu", "nvcontainer", "yourphoneappproxy", "applicationframehost",
        "msedge", "chrome", "firefox", "antigravity", "code", "windowsterminal", "openconsole",
        "backgroundtaskhost", "compattelrunner", "audiodg", "fontdrvhost", "dwm",
        "systemsettings", "gamebar", "xboxpcappft", "gamingservices", "esrv", "igfx",
    ];

    let mut dynamic: Vec<(String, u32, String)> = Vec::new();
    for (klow, (pid, orig_stem)) in &running_procs {
        if seen_keys.contains(klow) {
            continue;
        }
        if ignored_substrings.iter().any(|bad| klow.contains(bad)) {
            continue;
        }
        let (_, exe_path) = get_process_info(*pid);
        if exe_path.is_empty() {
            continue;
        }
        let plow = exe_path.to_lowercase();
        // Exclude pure OS internal executables under C:\Windows\* (except whitelisted tray applets handled in Pass 1)
        if plow.contains("\\windows\\system32\\")
            || plow.contains("\\windows\\syswow64\\")
            || plow.contains("\\windows\\systemapps\\")
            || plow.contains("\\windows\\uus\\")
            || plow.contains("\\windows\\immersivecontrolpanel\\")
        {
            continue;
        }
        // Skip processes that are already shown as regular active desktop windows (unless they are classic tray apps)
        let has_open_win = open_wins.iter().any(|w| w.process_name.eq_ignore_ascii_case(orig_stem));
        if has_open_win {
            continue;
        }
        // Clean pretty display label
        let mut pretty = orig_stem.replace('_', " ").replace('-', " ");
        if let Some(first) = pretty.chars().next() {
            let rest: String = pretty.chars().skip(1).collect();
            pretty = format!("{}{}", first.to_uppercase(), rest);
        }
        if seen_disp.insert(pretty.clone()) {
            dynamic.push((orig_stem.clone(), *pid, exe_path));
        }
    }
    dynamic.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));

    for (orig_stem, _pid, exe_path) in dynamic.into_iter().take(16) {
        let mut pretty = orig_stem.replace('_', " ").replace('-', " ");
        if let Some(first) = pretty.chars().next() {
            let rest: String = pretty.chars().skip(1).collect();
            pretty = format!("{}{}", first.to_uppercase(), rest);
        }
        out.push(BackgroundApp {
            process_key: orig_stem,
            display_name: pretty,
            exe_path,
            fallback_launch: String::new(),
            main_hwnd: 0,
        });
    }

    out
}

pub fn kill_process_by_name(proc_key: &str) {
    let _ = std::process::Command::new("taskkill")
        .args(["/IM", &format!("{}.exe", proc_key), "/F"])
        .output();
}

// ==================== REVERSIBLE GAME BOOST ====================

pub struct GameBoost {
    pub active: bool,
    saved_plan: String,
}

impl GameBoost {
    pub fn new() -> Self {
        Self { active: false, saved_plan: String::new() }
    }

    pub fn toggle(&mut self) -> String {
        if !self.active {
            // Save active power scheme and switch to High Performance
            if let Ok(out) = std::process::Command::new("powercfg").arg("/getactivescheme").output() {
                let s = String::from_utf8_lossy(&out.stdout);
                if let Some(guid) = s.split_whitespace().find(|tok| tok.len() == 36 && tok.contains('-')) {
                    self.saved_plan = guid.to_string();
                }
            }
            let _ = std::process::Command::new("powercfg")
                .args(["/setactive", "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"])
                .output();
            let trimmed = trim_background_memory();
            self.active = true;
            format!("Game Boost ON · Alto Rendimiento · {} procesos optimizados", trimmed)
        } else {
            if !self.saved_plan.is_empty() {
                let _ = std::process::Command::new("powercfg")
                    .args(["/setactive", &self.saved_plan])
                    .output();
            }
            self.active = false;
            "Game Boost OFF · Plan de energía restaurado".to_string()
        }
    }
}

fn trim_background_memory() -> usize {
    let mut count = 0usize;
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut pe = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let raw = crate::win32_shell::from_wide(&pe.szExeFile).to_lowercase();
                    if !raw.contains("antigravity") && !raw.contains("aurashell") && !raw.contains("system") {
                        if let Ok(h) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, false, pe.th32ProcessID) {
                            if EmptyWorkingSet(h).is_ok() {
                                count += 1;
                            }
                            let _ = windows::Win32::Foundation::CloseHandle(h);
                        }
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = windows::Win32::Foundation::CloseHandle(snap);
        }
    }
    count
}

// ==================== INSTALLED WINDOWS PROGRAMS UNINSTALLER REGISTRY SCANNER ====================

#[derive(Debug, Clone)]
pub struct InstalledProgramEntry {
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub size_mb: Option<f64>,
    pub uninstall_cmd: String,
    pub icon_path: String,
}

/// Scans HKLM (64-bit + WOW6432Node) & HKCU `Uninstall` keys in `<12 ms` (replaces slow `appwiz.cpl`!)
pub fn scan_installed_programs() -> Vec<InstalledProgramEntry> {
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    };

    let roots: [(HKEY, &str); 3] = [
        (HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Uninstall"),
    ];

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    unsafe fn read_reg_str(hkey: HKEY, val_name: &str) -> String {
        let wname = crate::win32_shell::to_wide(val_name);
        let mut vtype = REG_VALUE_TYPE::default();
        let mut sz = 0u32;
        if RegQueryValueExW(hkey, PCWSTR(wname.as_ptr()), None, Some(&mut vtype), None, Some(&mut sz)).is_err()
            || sz < 2
            || sz > 8192
            || (vtype != REG_SZ && vtype != REG_EXPAND_SZ)
        {
            return String::new();
        }
        let mut buf = vec![0u16; (sz as usize / 2) + 1];
        if RegQueryValueExW(
            hkey,
            PCWSTR(wname.as_ptr()),
            None,
            Some(&mut vtype),
            Some(buf.as_mut_ptr() as *mut u8),
            Some(&mut sz),
        )
        .is_err()
        {
            return String::new();
        }
        crate::win32_shell::from_wide(&buf).trim().to_string()
    }

    unsafe fn read_reg_dword(hkey: HKEY, val_name: &str) -> Option<u32> {
        let wname = crate::win32_shell::to_wide(val_name);
        let mut vtype = REG_VALUE_TYPE::default();
        let mut val = 0u32;
        let mut sz = 4u32;
        if RegQueryValueExW(
            hkey,
            PCWSTR(wname.as_ptr()),
            None,
            Some(&mut vtype),
            Some(&mut val as *mut u32 as *mut u8),
            Some(&mut sz),
        )
        .is_ok()
            && vtype == REG_DWORD
        {
            Some(val)
        } else {
            None
        }
    }

    unsafe {
        for (root, subkey) in roots {
            let wsub = crate::win32_shell::to_wide(subkey);
            let mut h_parent = HKEY::default();
            if RegOpenKeyExW(root, PCWSTR(wsub.as_ptr()), 0, KEY_READ, &mut h_parent).is_err() {
                continue;
            }
            let mut idx = 0u32;
            loop {
                let mut key_buf = [0u16; 260];
                let mut key_len = 260u32;
                if RegEnumKeyExW(
                    h_parent,
                    idx,
                    windows::core::PWSTR(key_buf.as_mut_ptr()),
                    &mut key_len,
                    None,
                    windows::core::PWSTR::null(),
                    None,
                    None,
                )
                .is_err()
                {
                    break;
                }
                idx += 1;

                let mut h_app = HKEY::default();
                if RegOpenKeyExW(h_parent, PCWSTR(key_buf.as_ptr()), 0, KEY_READ, &mut h_app).is_ok() {
                    let sys_comp = read_reg_dword(h_app, "SystemComponent").unwrap_or(0);
                    let name = read_reg_str(h_app, "DisplayName");
                    let uninst = {
                        let u = read_reg_str(h_app, "UninstallString");
                        if u.is_empty() {
                            read_reg_str(h_app, "QuietUninstallString")
                        } else {
                            u
                        }
                    };
                    if sys_comp == 0 && !name.is_empty() && !uninst.is_empty() {
                        let key_low = name.to_lowercase();
                        if !key_low.starts_with("kb")
                            && !key_low.contains("redistributable")
                            && seen.insert(key_low)
                        {
                            let publisher = read_reg_str(h_app, "Publisher");
                            let version = read_reg_str(h_app, "DisplayVersion");
                            let size_mb = read_reg_dword(h_app, "EstimatedSize")
                                .filter(|&kb| kb > 0)
                                .map(|kb| kb as f64 / 1024.0);
                            let raw_ico = read_reg_str(h_app, "DisplayIcon");
                            let clean_ico = raw_ico
                                .split(',')
                                .next()
                                .unwrap_or("")
                                .trim()
                                .trim_matches('"')
                                .to_string();
                            out.push(InstalledProgramEntry {
                                name,
                                publisher,
                                version,
                                size_mb,
                                uninstall_cmd: uninst,
                                icon_path: clean_ico,
                            });
                        }
                    }
                    let _ = RegCloseKey(h_app);
                }
            }
            let _ = RegCloseKey(h_parent);
        }
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}
