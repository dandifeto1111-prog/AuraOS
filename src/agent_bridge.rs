//! AuraOS v5.0 AI Desktop Bridge (`agent-desktop` + `AuraAgent` Native JSON UI Tree Exposure)
//! - Exposes Direct2D interactive `Hit` elements from every AuraOS window to
//!   `%LocalAppData%\AuraShell\ui_trees\<app>.json` with screen-space coordinates (`screen_x`, `screen_y`),
//!   enabling `AuraAgent.exe` and external AI agents (`agent-desktop`) to inspect and click
//!   elements in the background (`PostMessageW`) or with the animated AI Ghost Cursor!

use crate::models::app_data_dir;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsWindow, PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElementNode {
    pub index: usize,
    pub id: String,
    pub role: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub screen_x: i32,
    pub screen_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTreeSnapshot {
    pub app: String,
    pub hwnd: isize,
    pub win_x: i32,
    pub win_y: i32,
    pub win_w: i32,
    pub win_h: i32,
    pub updated_unix_ms: u64,
    pub element_count: usize,
    pub elements: Vec<UiElementNode>,
}

static LAST_PUBLISH: Mutex<Option<(String, Instant, usize)>> = Mutex::new(None);

pub fn ui_trees_dir() -> PathBuf {
    let dir = app_data_dir().join("ui_trees");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Publishes a structured JSON snapshot of an AuraOS window's interactive regions (throttled to <= 4 Hz
/// when element count is unchanged so rendering stays 100% zero-overhead at 144 FPS!).
pub fn publish_ui_tree(app_name: &str, hwnd: HWND, raw_nodes: &[(String, String, f32, f32, f32, f32)]) {
    if hwnd.0.is_null() {
        return;
    }
    let now = Instant::now();
    if let Ok(mut guard) = LAST_PUBLISH.lock() {
        if let Some((ref prev_app, prev_t, prev_len)) = *guard {
            if prev_app == app_name
                && prev_len == raw_nodes.len()
                && now.duration_since(prev_t) < Duration::from_millis(320)
            {
                return;
            }
        }
        *guard = Some((app_name.to_string(), now, raw_nodes.len()));
    }

    let mut wr = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut wr);
    }
    let wx = wr.left;
    let wy = wr.top;
    let ww = (wr.right - wr.left).max(0);
    let wh = (wr.bottom - wr.top).max(0);

    let mut elements = Vec::with_capacity(raw_nodes.len());
    for (idx, (id, label, x, y, w, h)) in raw_nodes.iter().enumerate() {
        let ix = x.round() as i32;
        let iy = y.round() as i32;
        let iw = w.round().max(1.0) as i32;
        let ih = h.round().max(1.0) as i32;
        elements.push(UiElementNode {
            index: idx,
            id: id.clone(),
            role: infer_role(id),
            label: if label.is_empty() { id.clone() } else { label.clone() },
            x: ix,
            y: iy,
            w: iw,
            h: ih,
            screen_x: wx + ix + iw / 2,
            screen_y: wy + iy + ih / 2,
        });
    }

    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let snap = UiTreeSnapshot {
        app: app_name.to_string(),
        hwnd: hwnd.0 as isize,
        win_x: wx,
        win_y: wy,
        win_w: ww,
        win_h: wh,
        updated_unix_ms: unix_ms,
        element_count: elements.len(),
        elements,
    };

    if let Ok(json) = serde_json::to_string_pretty(&snap) {
        let out = ui_trees_dir().join(format!("{}.json", app_name));
        let _ = fs::write(out, json);
    }
}

fn infer_role(id: &str) -> String {
    let low = id.to_lowercase();
    if low.contains("slider") || low.contains("scrub") || low.contains("vol") {
        "slider".into()
    } else if low.contains("tab") || low.contains("nav") {
        "tab".into()
    } else if low.contains("search") || low.contains("input") || low.contains("edit") {
        "textbox".into()
    } else if low.contains("dock") {
        "dock-item".into()
    } else {
        "button".into()
    }
}

pub fn read_ui_tree(app_name: &str) -> Option<UiTreeSnapshot> {
    let p = ui_trees_dir().join(format!("{}.json", app_name));
    let txt = fs::read_to_string(p).ok()?;
    serde_json::from_str(&txt).ok()
}

pub fn list_published_trees() -> Vec<UiTreeSnapshot> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(ui_trees_dir()) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(txt) = fs::read_to_string(&p) {
                    if let Ok(snap) = serde_json::from_str::<UiTreeSnapshot>(&txt) {
                        let alive = unsafe { IsWindow(HWND(snap.hwnd as *mut _)).as_bool() };
                        if alive {
                            out.push(snap);
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.app.cmp(&b.app));
    out
}

/// Performs a zero-focus-stealing background click on `hwnd` at client coordinates `(local_x, local_y)`
/// (same non-intrusive philosophy as `vercel-labs/agent-desktop`!).
pub fn send_background_click(hwnd_raw: isize, local_x: i32, local_y: i32) -> bool {
    let hwnd = HWND(hwnd_raw as *mut _);
    unsafe {
        if !IsWindow(hwnd).as_bool() {
            return false;
        }
        let lp = LPARAM(((local_y & 0xFFFF) << 16 | (local_x & 0xFFFF)) as isize);
        let _ = PostMessageW(hwnd, WM_MOUSEMOVE, WPARAM(0), lp);
        let ok1 = PostMessageW(hwnd, WM_LBUTTONDOWN, WPARAM(1), lp).is_ok();
        let ok2 = PostMessageW(hwnd, WM_LBUTTONUP, WPARAM(0), lp).is_ok();
        ok1 && ok2
    }
}
