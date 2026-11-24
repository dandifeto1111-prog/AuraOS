//! AuraOS v4.5 macOS-Style Parabolic Magnification Dock + Live Preview Targets + Genie Coordinates
//! - True macOS parabolic magnification wave: hovered icon grows smoothly to 1.42x while
//!   neighboring icons ripple gently upward/outward with zero overlap!
//! - Tight-bounds capsule + minimal iPhone/Android Home-Indicator Notch ("Muesquita") when auto-hidden
//!   (`SW_HIDE` when tucked -> ZERO click-blocking transparent air!).
//! - Crisp embedded squircle icons for ALL 8 AuraOS binaries + native Windows apps!

use crate::models::app_data_dir;
use crate::renderer::{DrawCtx, Rect};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};
use crate::ui_topbar::TOPBAR_HEIGHT;

/// Includes +22px magnification headroom on the inner desktop edge ONLY while visible!
pub const DOCK_THICKNESS: f32 = 96.0;
pub const NOTCH_LONG_BOTTOM: i32 = 132;
pub const NOTCH_LONG_LEFT: i32 = 112;
pub const NOTCH_THICK: i32 = 14;

struct DockEntry {
    id: String,
    tooltip: String,
    icon_path: String,
    fallback: VectorIcon,
    is_running: bool,
    is_active: bool,
    hwnd: isize,
    action: Action,
}

fn resolve_aura_exe(bin_name: &str) -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    for d in [exe_dir, Some(app_data_dir())].into_iter().flatten() {
        let p = d.join(bin_name);
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    String::new()
}

fn aura_fallback_icon(proc_name: &str) -> VectorIcon {
    match proc_name.to_lowercase().as_str() {
        "aurafiles" => VectorIcon::Folder,
        "aurasettings" => VectorIcon::Settings,
        "auraphotos" => VectorIcon::Image,
        "aurapdf" => VectorIcon::FileText,
        "aurashot" => VectorIcon::Maximize,
        "auraplayer" => VectorIcon::Play,
        "auranotes" => VectorIcon::Code,
        "auratasks" => VectorIcon::Sparkle,
        "auralock" => VectorIcon::Lock,
        _ => VectorIcon::Code,
    }
}

fn build_middle_entries(state: &AppState) -> Vec<DockEntry> {
    let mut middle: Vec<DockEntry> = Vec::new();
    for w in &state.open_windows {
        if w.process_name.eq_ignore_ascii_case("AuraFiles") {
            continue;
        }
        let icon_path = if !w.exe_path.is_empty() {
            w.exe_path.clone()
        } else {
            resolve_aura_exe(&format!("{}.exe", w.process_name))
        };
        middle.push(DockEntry {
            id: format!("dock_win_{}", w.hwnd),
            tooltip: format!("{} ({})", w.title, w.process_name),
            icon_path,
            fallback: aura_fallback_icon(&w.process_name),
            is_running: true,
            is_active: w.is_active,
            hwnd: w.hwnd,
            action: Action::FocusWindow(w.hwnd),
        });
    }

    let fav_keys = ["antigravity", "edge", "chrome", "discord", "spotify", "steam"];
    let mut added_pinned = 0usize;
    for app in &state.all_apps {
        if added_pinned >= 4 {
            break;
        }
        let low = app.name.to_lowercase();
        if !fav_keys.iter().any(|k| low.contains(k)) {
            continue;
        }
        let already = middle.iter().any(|m| {
            let mt = m.tooltip.to_lowercase();
            fav_keys.iter().any(|k| low.contains(k) && mt.contains(k))
        });
        if already {
            continue;
        }
        middle.push(DockEntry {
            id: format!("dock_pin_{}", added_pinned),
            tooltip: app.name.clone(),
            icon_path: app.icon_path.clone(),
            fallback: VectorIcon::Sparkle,
            is_running: false,
            is_active: false,
            hwnd: 0,
            action: Action::LaunchTarget(app.target.clone()),
        });
        added_pinned += 1;
    }
    middle
}

/// Computes the exact macOS parabolic magnification scale for a tile centered at `center_axis`
/// given the live cursor coordinate `mouse_axis` along the Dock.
pub fn compute_magnify_scale(center_axis: f32, mouse_axis: Option<f32>, low_spec: bool) -> f32 {
    let Some(m) = mouse_axis else {
        return 1.0;
    };
    let max_boost = if low_spec { 0.16 } else { 0.42 };
    let radius = if low_spec { 72.0 } else { 126.0 };
    let dist = (center_axis - m).abs();
    if dist >= radius {
        return 1.0;
    }
    let u = (dist / radius) * std::f32::consts::FRAC_PI_2;
    let c = u.cos();
    1.0 + max_boost * (c * c)
}

/// Computes the EXACT tight pixel bounding box for `hwnd_dock` so transparent pixels NEVER steal clicks!
pub fn dock_tight_geometry(state: &AppState, sw: i32, sh: i32, is_left: bool) -> (i32, i32, i32, i32) {
    let top_h = TOPBAR_HEIGHT as i32;
    let mid_count = build_middle_entries(state).len();
    let total_items = 2 + mid_count; // AuraFiles + middle + 9-dot Launchpad
    let item_step = 50.0f32;
    let pad = 28.0f32; // extra end-margin for parabolic magnification pop

    if is_left {
        let max_len = (sh - top_h - 18).max(220) as f32;
        let cap_len = (total_items as f32 * item_step + 44.0).clamp(196.0, max_len);
        let win_h = (cap_len + pad).round().min(max_len) as i32;
        let win_w = DOCK_THICKNESS as i32;
        let y = top_h + ((sh - top_h - win_h) / 2).max(4);
        (0, y, win_w, win_h)
    } else {
        let max_len = (sw - 36).min(1220).max(240) as f32;
        let cap_len = (total_items as f32 * item_step + 44.0).clamp(216.0, max_len);
        let win_w = (cap_len + pad).round().min(max_len) as i32;
        let win_h = DOCK_THICKNESS as i32;
        let x = (sw - win_w) / 2;
        (x, sh - win_h, win_w, win_h)
    }
}

/// Returns the screen pixel center (x, y) of a specific window's icon inside the Dock (for macOS Genie Effect!)
pub fn dock_icon_screen_center(
    state: &AppState,
    dock_geo: (i32, i32, i32, i32),
    is_left: bool,
    target_hwnd: isize,
) -> (f32, f32) {
    let (dx, dy, dw, dh) = dock_geo;
    let cap_x = if is_left { 8.0 } else { 14.0 };
    let cap_y = if is_left { 14.0 } else { dh as f32 - 68.0 };

    if state
        .open_windows
        .iter()
        .any(|w| w.hwnd == target_hwnd && w.process_name.eq_ignore_ascii_case("AuraFiles"))
    {
        return (dx as f32 + cap_x + 31.0, dy as f32 + cap_y + 31.0);
    }

    let middle = build_middle_entries(state);
    if let Some(idx) = middle.iter().position(|e| e.hwnd == target_hwnd) {
        let pos = idx as f32 * 50.0 - state.dock_scroll;
        if is_left {
            let cy = (dy as f32 + cap_y + 68.0 + pos + 21.0).clamp(dy as f32 + 24.0, (dy + dh - 24) as f32);
            return (dx as f32 + cap_x + 31.0, cy);
        } else {
            let cx = (dx as f32 + cap_x + 68.0 + pos + 21.0).clamp(dx as f32 + 24.0, (dx + dw - 24) as f32);
            return (cx, dy as f32 + cap_y + 31.0);
        }
    }

    (dx as f32 + dw as f32 * 0.5, dy as f32 + dh as f32 * 0.5)
}

/// Resolves a Dock hit ID to its live running window `(hwnd, title)` for Live DWM Preview!
pub fn dock_preview_target_for_hit(state: &AppState, hit_id: &str) -> Option<(isize, String)> {
    if hit_id == "dock_files" {
        return state
            .open_windows
            .iter()
            .find(|w| w.process_name.eq_ignore_ascii_case("AuraFiles"))
            .map(|w| (w.hwnd, w.title.clone()));
    }
    if let Some(raw) = hit_id.strip_prefix("dock_win_") {
        if let Ok(hw) = raw.parse::<isize>() {
            if let Some(w) = state.open_windows.iter().find(|w| w.hwnd == hw) {
                return Some((w.hwnd, w.title.clone()));
            }
        }
    }
    None
}

/// Exact compact geometry for the sleek iPhone/Android Home-Indicator Notch ("Muesquita")
pub fn notch_geometry(sw: i32, sh: i32, is_left: bool) -> (i32, i32, i32, i32) {
    if is_left {
        let nh = NOTCH_LONG_LEFT;
        let nw = NOTCH_THICK;
        let y = TOPBAR_HEIGHT as i32 + ((sh - TOPBAR_HEIGHT as i32 - nh) / 2);
        (0, y, nw, nh)
    } else {
        let nw = NOTCH_LONG_BOTTOM;
        let nh = NOTCH_THICK;
        ((sw - nw) / 2, sh - nh, nw, nh)
    }
}

/// Renders the minimal iPhone/Android Home-Indicator pill ("Muesquita") at the edge of the screen!
pub fn render_dock_notch(ctx: &mut DrawCtx<'_>, w: f32, h: f32, is_left: bool, hovered: bool) {
    let p = theme::pal();
    let halo_col = if hovered {
        theme::with_alpha(p.accent, 0.36)
    } else {
        theme::with_alpha(p.canvas, 0.62)
    };
    let bar_col = if hovered {
        p.accent_light
    } else {
        theme::rgba(245, 245, 252, 0.62)
    };

    if is_left {
        let halo = Rect::new(1.0, 8.0, 9.0, (h - 16.0).max(40.0));
        ctx.fill_card(halo, 4.5, halo_col, theme::rgba(255, 255, 255, 0.12));
        let pill = Rect::new(3.0, 14.0, 4.5, (h - 28.0).max(28.0));
        ctx.fill_rrect(pill, 2.25, bar_col);
    } else {
        let halo = Rect::new(8.0, h - 10.0, (w - 16.0).max(48.0), 9.0);
        ctx.fill_card(halo, 4.5, halo_col, theme::rgba(255, 255, 255, 0.12));
        let pill = Rect::new(14.0, h - 7.5, (w - 28.0).max(36.0), 4.5);
        ctx.fill_rrect(pill, 2.25, bar_col);
    }
}

pub fn render_dock(
    ctx: &mut DrawCtx<'_>,
    state: &AppState,
    win_w: f32,
    win_h: f32,
    is_left: bool,
    hits: &mut Vec<HitRegion>,
) {
    hits.clear();

    let reveal = state.dock_reveal.value.clamp(0.0, 1.0);
    if reveal <= 0.01 {
        return;
    }

    let middle = build_middle_entries(state);
    let files_win = state
        .open_windows
        .iter()
        .find(|w| w.process_name.eq_ignore_ascii_case("AuraFiles"));

    let item_step = 50.0f32;
    let low_spec = state.settings.performance_mode;
    let (cap_x, cap_y, cap_w, cap_h, slide_dx, slide_dy) = if is_left {
        (8.0, 14.0, 62.0, (win_h - 28.0).max(168.0), (1.0 - reveal) * -96.0, 0.0)
    } else {
        (14.0, win_h - 68.0, (win_w - 28.0).max(188.0), 62.0, 0.0, (1.0 - reveal) * 96.0)
    };

    ctx.set_transform_scale_origin(1.0, 0.0, 0.0, slide_dx, slide_dy);
    let _layer = ctx.push_opacity_layer(Rect::new(0.0, 0.0, win_w, win_h), reveal);

    // Material You Obsidian Dock Capsule
    let p = theme::pal();
    let capsule = Rect::new(cap_x, cap_y, cap_w, cap_h);
    ctx.fill_card(capsule, 20.0, theme::with_alpha(p.surface, 0.96), p.border_strong);

    // 1. TOP/LEFT PINNED: AuraFiles HD
    let f_rect = Rect::new(cap_x + 10.0, cap_y + 10.0, 42.0, 42.0);
    let f_axis = if is_left { f_rect.y + 21.0 } else { f_rect.x + 21.0 };
    let f_scale = compute_magnify_scale(f_axis, state.dock_mouse_axis, low_spec);
    let f_action = if let Some(fw) = files_win {
        Action::FocusWindow(fw.hwnd)
    } else {
        Action::LaunchAuraFiles(None)
    };
    draw_dock_tile(
        ctx,
        state,
        "dock_files",
        f_rect,
        &resolve_aura_exe("AuraFiles.exe"),
        VectorIcon::Folder,
        files_win.is_some(),
        files_win.map(|w| w.is_active).unwrap_or(false),
        is_left,
        f_scale,
    );
    hits.push(HitRegion {
        id: "dock_files".into(),
        rect: f_rect.inset(-4.0, -4.0),
        action: f_action,
        right_action: None,
        tooltip: Some("AuraFiles · Gestor instantáneo (Win+E / Alt+E)".into()),
        slider: None,
    });

    // Divider 1
    if is_left {
        ctx.draw_line(cap_x + 17.0, cap_y + 59.0, cap_x + cap_w - 17.0, cap_y + 59.0, theme::BORDER, 1.0);
    } else {
        ctx.draw_line(cap_x + 59.0, cap_y + 17.0, cap_x + 59.0, cap_y + cap_h - 17.0, theme::BORDER, 1.0);
    }

    // 2. MIDDLE SCROLLABLE VIEWPORT (ALL open apps + pinned with macOS Parabolic Magnification Wave!)
    let mid_rect = if is_left {
        Rect::new(cap_x - 4.0, cap_y + 65.0, win_w - cap_x + 4.0, (cap_h - 130.0).max(46.0))
    } else {
        Rect::new(cap_x + 65.0, 0.0, (cap_w - 130.0).max(46.0), win_h)
    };
    ctx.push_clip(mid_rect);

    let max_scroll = ((middle.len() as f32 * item_step)
        - if is_left { mid_rect.h } else { mid_rect.w })
    .max(0.0);
    let scroll = state.dock_scroll.clamp(0.0, max_scroll);

    // Draw non-hovered items first, then draw the most-magnified hovered tile on top for crisp overlap!
    let mut deferred_top: Option<(usize, Rect, f32)> = None;

    for (idx, item) in middle.iter().enumerate() {
        let base_pos = idx as f32 * item_step - scroll;
        let raw_axis = if is_left {
            cap_y + 68.0 + base_pos + 21.0
        } else {
            cap_x + 68.0 + base_pos + 21.0
        };
        let mag_scale = compute_magnify_scale(raw_axis, state.dock_mouse_axis, low_spec);
        let spring_scale = state
            .dock_item_scales
            .get(idx)
            .map(|s| s.value)
            .unwrap_or(1.0)
            .max(mag_scale);

        // Gentle neighbor displacement away from cursor so magnified icons breathe naturally!
        let nudge = if let Some(m) = state.dock_mouse_axis {
            let delta = raw_axis - m;
            let dist = delta.abs();
            if dist > 4.0 && dist < 130.0 && !low_spec {
                let push = (1.0 - dist / 130.0).powi(2) * 7.5;
                if delta > 0.0 { push } else { -push }
            } else {
                0.0
            }
        } else {
            0.0
        };

        let r = if is_left {
            Rect::new(cap_x + 10.0, cap_y + 68.0 + base_pos + nudge, 42.0, 42.0)
        } else {
            Rect::new(cap_x + 68.0 + base_pos + nudge, cap_y + 10.0, 42.0, 42.0)
        };
        let visible = if is_left {
            r.y + r.h >= cap_y + 56.0 && r.y <= cap_y + cap_h - 56.0
        } else {
            r.x + r.w >= cap_x + 56.0 && r.x <= cap_x + cap_w - 56.0
        };
        if !visible {
            continue;
        }

        hits.push(HitRegion {
            id: item.id.clone(),
            rect: r.inset(-3.0, -3.0),
            action: item.action.clone(),
            right_action: None,
            tooltip: Some(item.tooltip.clone()),
            slider: None,
        });

        if state.hovered_id.as_deref() == Some(item.id.as_str()) {
            deferred_top = Some((idx, r, spring_scale));
        } else {
            draw_dock_tile(
                ctx,
                state,
                &item.id,
                r,
                &item.icon_path,
                item.fallback,
                item.is_running,
                item.is_active,
                is_left,
                spring_scale,
            );
        }
    }

    if let Some((idx, r, scale)) = deferred_top {
        let item = &middle[idx];
        draw_dock_tile(
            ctx,
            state,
            &item.id,
            r,
            &item.icon_path,
            item.fallback,
            item.is_running,
            item.is_active,
            is_left,
            scale,
        );
    }

    ctx.pop_clip();

    // Divider 2
    if is_left {
        ctx.draw_line(cap_x + 17.0, cap_y + cap_h - 59.0, cap_x + cap_w - 17.0, cap_y + cap_h - 59.0, theme::BORDER, 1.0);
    } else {
        ctx.draw_line(cap_x + cap_w - 59.0, cap_y + 17.0, cap_x + cap_w - 59.0, cap_y + cap_h - 17.0, theme::BORDER, 1.0);
    }

    // 3. BOTTOM/RIGHT PINNED: 9-Dot macOS Launchpad App Drawer Trigger!
    let lp_rect = if is_left {
        Rect::new(cap_x + 10.0, cap_y + cap_h - 52.0, 42.0, 42.0)
    } else {
        Rect::new(cap_x + cap_w - 52.0, cap_y + 10.0, 42.0, 42.0)
    };
    let lp_axis = if is_left { lp_rect.y + 21.0 } else { lp_rect.x + 21.0 };
    let lp_scale = compute_magnify_scale(lp_axis, state.dock_mouse_axis, low_spec);
    draw_dock_tile(
        ctx,
        state,
        "dock_launchpad",
        lp_rect,
        "",
        VectorIcon::AppGrid,
        false,
        state.launchpad_open,
        is_left,
        lp_scale,
    );
    hits.push(HitRegion {
        id: "dock_launchpad".into(),
        rect: lp_rect.inset(-4.0, -4.0),
        action: Action::ToggleLaunchpad,
        right_action: None,
        tooltip: Some("Todas las aplicaciones (Tecla Win)".into()),
        slider: None,
    });

    ctx.pop_layer();
    ctx.reset_transform();
}

#[allow(clippy::too_many_arguments)]
fn draw_dock_tile(
    ctx: &mut DrawCtx<'_>,
    state: &AppState,
    id: &str,
    r: Rect,
    icon_path: &str,
    fallback: VectorIcon,
    is_running: bool,
    is_active: bool,
    is_left: bool,
    spring_scale: f32,
) {
    let hov = state.hovered_id.as_deref() == Some(id);
    let pressed = state.pressed_id.as_deref() == Some(id);
    let scale = if pressed {
        0.95
    } else {
        spring_scale.clamp(0.95, 1.46)
    };

    // Pop outward toward desktop center as the icon magnifies (classic macOS Dock bounce!)
    let pop = (scale - 1.0).max(0.0) * 18.0;
    let cx = r.x + r.w * 0.5 + if is_left { pop } else { 0.0 };
    let cy = r.y + r.h * 0.5 - if !is_left { pop } else { 0.0 };
    let sw = r.w * scale;
    let sh = r.h * scale;
    let sr = Rect::new(cx - sw * 0.5, cy - sh * 0.5, sw, sh);

    let p = theme::pal();
    let bg = if is_active {
        theme::CRISP_WHITE
    } else if hov {
        p.accent
    } else {
        p.elevated
    };
    let border = if is_active {
        p.accent
    } else if hov {
        p.accent_light
    } else {
        p.border
    };

    ctx.fill_card(sr, 13.0 * scale.min(1.2), bg, border);

    let ic_sz = 28.0 * scale;
    let ic_rect = Rect::new(cx - ic_sz * 0.5, cy - ic_sz * 0.5, ic_sz, ic_sz);
    if icon_path.is_empty() || !ctx.draw_app_icon(icon_path, ic_rect) {
        let ic_col = if is_active { theme::DARK_INK } else { p.text_primary };
        ctx.draw_vector_icon(fallback, cx, cy, 18.0 * scale, ic_col);
    }

    if is_running {
        let dot_col = if is_active { p.accent } else { p.text_secondary };
        if is_left {
            let dh = if is_active { 14.0 } else { 6.0 };
            ctx.fill_rrect(Rect::new(r.x - 6.0, r.y + r.h * 0.5 - dh * 0.5, 3.5, dh), 1.8, dot_col);
        } else {
            let dw = if is_active { 14.0 } else { 6.0 };
            ctx.fill_rrect(Rect::new(r.x + r.w * 0.5 - dw * 0.5, r.y + r.h + 3.0, dw, 3.5), 1.8, dot_col);
        }
    }
}
