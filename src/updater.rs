//! Automatic Seamless Updater for AuraOS v4.1+ (Local Workspace + GitHub Releases / Discord Production)
//! - Watches local SourceDir\version.json for zero-friction instant hot-updates while developing
//! - Also checks optional remote `update_manifest_url` (GitHub Releases API or raw version.json)
//!   so community users on GitHub/Discord automatically receive update notifications & 1-click installs!
//! - Runs 100% off the UI thread (0ms impact on shell FPS!)

use crate::models::{app_data_dir, ShellSettings};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub build_number: i64,
    pub changelog: String,
    pub is_local_dev: bool,
    pub download_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct LocalManifest {
    #[serde(rename = "Version", default)]
    version: String,
    #[serde(rename = "BuildNumber", default)]
    build_number: i64,
    #[serde(rename = "SourceDir", default)]
    source_dir: String,
    #[serde(rename = "Changelog", default)]
    changelog: String,
    #[serde(rename = "DownloadUrl", default)]
    download_url: String,
}

fn read_manifest_file(path: &Path) -> Option<LocalManifest> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<LocalManifest>(raw.trim_start_matches('\u{FEFF}')).ok()
}

fn parse_semver_tuple(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches('v').trim_start_matches('V');
    let mut parts = clean.split('.').map(|s| s.parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn check_local_source_update() -> Option<UpdateInfo> {
    let inst_dir = app_data_dir();
    let installed = read_manifest_file(&inst_dir.join("version.json"))?;
    let src_dir = if !installed.source_dir.trim().is_empty() {
        PathBuf::from(&installed.source_dir)
    } else {
        return None;
    };
    if !src_dir.exists() || src_dir.to_string_lossy().eq_ignore_ascii_case(&inst_dir.to_string_lossy()) {
        return None;
    }
    let avail = read_manifest_file(&src_dir.join("version.json"))?;
    if !src_dir.join("AuraShell.exe").exists() {
        return None;
    }

    let newer_build = avail.build_number > installed.build_number;
    let newer_ver = parse_semver_tuple(&avail.version) > parse_semver_tuple(&installed.version);
    if newer_build || newer_ver {
        return Some(UpdateInfo {
            version: avail.version,
            build_number: avail.build_number,
            changelog: avail.changelog,
            is_local_dev: true,
            download_url: String::new(),
        });
    }
    None
}

/// Fetches a remote JSON manifest (either GitHub Releases API `/releases/latest` or raw `version.json`)
fn fetch_https_text(url: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable,
        WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest, WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
        WINHTTP_FLAG_SECURE,
    };

    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/').map(|(h, p)| (h, format!("/{}", p))).unwrap_or((rest, "/".to_string()));
    let w_agent = crate::win32_shell::to_wide("AuraOS-Updater/4.1");
    let w_host = crate::win32_shell::to_wide(host);
    let w_path = crate::win32_shell::to_wide(&path);
    let w_get = crate::win32_shell::to_wide("GET");

    unsafe {
        let h_sess = WinHttpOpen(PCWSTR(w_agent.as_ptr()), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
        if h_sess.is_null() { return None; }
        let h_conn = WinHttpConnect(h_sess, PCWSTR(w_host.as_ptr()), 443, 0);
        if h_conn.is_null() {
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }
        let h_req = WinHttpOpenRequest(
            h_conn,
            PCWSTR(w_get.as_ptr()),
            PCWSTR(w_path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if h_req.is_null() {
            let _ = WinHttpCloseHandle(h_conn);
            let _ = WinHttpCloseHandle(h_sess);
            return None;
        }

        let mut body = Vec::new();
        if WinHttpSendRequest(h_req, None, None, 0, 0, 0).is_ok()
            && WinHttpReceiveResponse(h_req, std::ptr::null_mut()).is_ok()
        {
            loop {
                let mut avail = 0u32;
                if WinHttpQueryDataAvailable(h_req, &mut avail).is_err() || avail == 0 {
                    break;
                }
                let mut chunk = vec![0u8; avail.min(16384) as usize];
                let mut read = 0u32;
                if WinHttpReadData(h_req, chunk.as_mut_ptr() as *mut _, chunk.len() as u32, &mut read).is_err() || read == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..read as usize]);
                if body.len() > 262_144 {
                    break;
                }
            }
        }
        let _ = WinHttpCloseHandle(h_req);
        let _ = WinHttpCloseHandle(h_conn);
        let _ = WinHttpCloseHandle(h_sess);

        if body.is_empty() { None } else { String::from_utf8(body).ok() }
    }
}

fn check_remote_manifest_update(url: &str) -> Option<UpdateInfo> {
    if url.trim().is_empty() {
        return None;
    }
    let inst_ver = read_manifest_file(&app_data_dir().join("version.json"))
        .map(|m| m.version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let txt = fetch_https_text(url.trim())?;
    let val: serde_json::Value = serde_json::from_str(txt.trim_start_matches('\u{FEFF}')).ok()?;

    // Support BOTH GitHub Releases API (`tag_name`, `html_url`, `body`) AND AuraOS `version.json`!
    let remote_ver = val
        .get("tag_name")
        .or_else(|| val.get("Version"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();
    if remote_ver.is_empty() {
        return None;
    }
    if parse_semver_tuple(&remote_ver) <= parse_semver_tuple(&inst_ver) {
        return None;
    }

    let changelog = val
        .get("body")
        .or_else(|| val.get("Changelog"))
        .and_then(|v| v.as_str())
        .unwrap_or("Nueva versión de AuraOS disponible")
        .chars()
        .take(220)
        .collect();
    let dl_url = val
        .get("html_url")
        .or_else(|| val.get("DownloadUrl"))
        .and_then(|v| v.as_str())
        .unwrap_or(url)
        .to_string();

    Some(UpdateInfo {
        version: remote_ver,
        build_number: val.get("BuildNumber").and_then(|v| v.as_i64()).unwrap_or(0),
        changelog,
        is_local_dev: false,
        download_url: dl_url,
    })
}

/// Spawns a silent background update watcher (checks 6s after start, then every 45s)
pub fn spawn_update_watcher(settings: ShellSettings) -> Receiver<UpdateInfo> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(6));
        loop {
            if let Some(upd) = check_local_source_update() {
                if tx.send(upd).is_err() {
                    break;
                }
            } else if !settings.update_manifest_url.trim().is_empty() {
                if let Some(upd) = check_remote_manifest_update(&settings.update_manifest_url) {
                    if tx.send(upd).is_err() {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(45));
        }
    });
    rx
}

/// Triggers seamless zero-GUI hot-update via AuraShell-Setup.exe --silent-update (or opens GitHub release)
pub fn trigger_apply_update(info: &UpdateInfo) {
    if !info.is_local_dev && !info.download_url.is_empty() {
        crate::win32_shell::launch_target(&info.download_url, "");
        return;
    }
    let inst_dir = app_data_dir();
    let setup = inst_dir.join("AuraShell-Setup.exe");
    if setup.exists() {
        let _ = Command::new(&setup)
            .arg("--silent-update")
            .current_dir(&inst_dir)
            .spawn();
    }
}
