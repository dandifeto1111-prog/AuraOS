//! Zero-Interference Tight-Bounds Spring Dock + iPhone/Android Home-Indicator Notch ("Muesquita")
//! - Window bounds ALWAYS fit the exact visible capsule (ZERO click-blocking transparent air!)
//! - When tucked away behind an open app, `hwnd_dock` is fully hidden (`SW_HIDE`) and replaced by a
//!   minimal translucent iPhone/Android Home-Indicator pill (`AuraDockNotch`: 128x14 bottom or 14x112 left)
//! - Dock ONLY reveals when hovering that exact small notch (zero accidental popups along screen edges!)

use crate::models::app_data_dir;
use crate::renderer::{DrawCtx, Rect};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};
use crate::ui_topbar::TOPBAR_HEIGHT;

pub const DOCK_THICKNESS: f32 = 76.0;
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
    action: Action,
}

fn aurafiles_icon_path() -> String {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    for d in [exe_dir, Some(app_data_dir())].into_iter().flatten() {
        let p = d.join("AuraFiles.exe");
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }
    String::new()
}

fn build_middle_entries(state: &AppState) -> Vec<DockEntry> {
    let mut middle: Vec<DockEntry> = Vec::new();
    for w in &state.open_windows {
        if w.process_name.eq_ignore_ascii_case("AuraFiles") {
            continue;
        }
        middle.push(DockEntry {
            id: format!("dock_win_{}", w.hwnd),
            tooltip: format!("{} ({})", w.title, w.process_name),
            icon_path: w.exe_path.clone(),
            fallback: VectorIcon::Code,
            is_running: true,
            is_active: w.is_active,
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
            action: Action::LaunchTarget(app.target.clone()),
        });
        added_pinned += 1;
    }
    middle
}

/// Computes the EXACT tight pixel bounding box for `hwnd_dock` so transparent pixels NEVER steal clicks!
pub fn dock_tight_geometry(state: &AppState, sw: i32, sh: i32, is_left: bool) -> (i32, i32, i32, i32) {
    let top_h = TOPBAR_HEIGHT as i32;
    let mid_count = build_middle_entries(state).len();
    let total_items = 2 + mid_count; // AuraFiles + middle + 9-dot Launchpad
    let item_step = 50.0f32;
    let pad = 12.0f32; // tiny breathing room for hover scale (1.10x)

    if is_left {
        let max_len = (sh - top_h - 24).max(200) as f32;
        let cap_len = (total_items as f32 * item_step + 34.0).clamp(180.0, max_len);
        let win_h = (cap_len + pad).round() as i32;
        let win_w = DOCK_THICKNESS as i32;
        let y = top_h + ((sh - top_h - win_h) / 2).max(6);
        (0, y, win_w, win_h)
    } else {
        let max_len = (sw - 48).min(1140).max(220) as f32;
        let cap_len = (total_items as f32 * item_step + 34.0).clamp(200.0, max_len);
        let win_w = (cap_len + pad).round() as i32;
        let win_h = DOCK_THICKNESS as i32;
        let x = (sw - win_w) / 2;
        (x, sh - win_h, win_w, win_h)
    }
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
    let halo_col = if hovered {
        theme::rgba(124, 92, 255, 0.36)
    } else {
        theme::rgba(13, 13, 20, 0.58)
    };
    let bar_col = if hovered {
        theme::VIOLET_LIGHT
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
    let (cap_x, cap_y, cap_w, cap_h, slide_dx, slide_dy) = if is_left {
        (8.0, 6.0, 62.0, (win_h - 12.0).max(160.0), (1.0 - reveal) * -78.0, 0.0)
    } else {
        (6.0, win_h - 68.0, (win_w - 12.0).max(180.0), 62.0, 0.0, (1.0 - reveal) * 78.0)
    };

    ctx.set_transform_scale_origin(1.0, 0.0, 0.0, slide_dx, slide_dy);
    let _layer = ctx.push_opacity_layer(Rect::new(0.0, 0.0, win_w, win_h), reveal);

    // Matte Obsidian Dock Capsule
    let capsule = Rect::new(cap_x, cap_y, cap_w, cap_h);
    ctx.fill_card(capsule, 20.0, theme::rgba(18, 18, 25, 0.96), theme::BORDER_STRONG);

    // 1. TOP/LEFT PINNED: AuraFiles HD
    let f_rect = Rect::new(cap_x + 10.0, cap_y + 10.0, 42.0, 42.0);
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
        &aurafiles_icon_path(),
        VectorIcon::Folder,
        files_win.is_some(),
        files_win.map(|w| w.is_active).unwrap_or(false),
        is_left,
        1.0,
    );
    hits.push(HitRegion {
        id: "dock_files".into(),
        rect: f_rect,
        action: f_action,
        right_action: None,
        tooltip: Some("AuraFiles · Gestor instantáneo (Alt+E)".into()),
        slider: None,
    });

    // Divider 1
    if is_left {
        ctx.draw_line(cap_x + 17.0, cap_y + 59.0, cap_x + cap_w - 17.0, cap_y + 59.0, theme::BORDER, 1.0);
    } else {
        ctx.draw_line(cap_x + 59.0, cap_y + 17.0, cap_x + 59.0, cap_y + cap_h - 17.0, theme::BORDER, 1.0);
    }

    // 2. MIDDLE SCROLLABLE VIEWPORT (ALL open apps + pinned!)
    let mid_rect = if is_left {
        Rect::new(cap_x + 2.0, cap_y + 65.0, cap_w - 4.0, (cap_h - 130.0).max(46.0))
    } else {
        Rect::new(cap_x + 65.0, cap_y + 2.0, (cap_w - 130.0).max(46.0), cap_h - 4.0)
    };
    ctx.push_clip(mid_rect);

    let max_scroll = ((middle.len() as f32 * item_step) - if is_left { mid_rect.h } else { mid_rect.w }).max(0.0);
    let scroll = state.dock_scroll.clamp(0.0, max_scroll);

    for (idx, item) in middle.iter().enumerate() {
        let pos = idx as f32 * item_step - scroll;
        let r = if is_left {
            Rect::new(cap_x + 10.0, mid_rect.y + 3.0 + pos, 42.0, 42.0)
        } else {
            Rect::new(mid_rect.x + 3.0 + pos, cap_y + 10.0, 42.0, 42.0)
        };
        let visible = if is_left {
            r.y + r.h >= mid_rect.y && r.y <= mid_rect.y + mid_rect.h
        } else {
            r.x + r.w >= mid_rect.x && r.x <= mid_rect.x + mid_rect.w
        };
        if !visible {
            continue;
        }
        let spring_scale = state.dock_item_scales.get(idx).map(|s| s.value).unwrap_or(1.0);
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
        hits.push(HitRegion {
            id: item.id.clone(),
            rect: r,
            action: item.action.clone(),
            right_action: None,
            tooltip: Some(item.tooltip.clone()),
            slider: None,
        });
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
        1.0,
    );
    hits.push(HitRegion {
        id: "dock_launchpad".into(),
        rect: lp_rect,
        action: Action::ToggleLaunchpad,
        right_action: None,
        tooltip: Some("Todas las aplicaciones (Launchpad)".into()),
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
    } else if hov {
        spring_scale.max(1.10)
    } else {
        spring_scale
    };

    let cx = r.x + r.w * 0.5;
    let cy = r.y + r.h * 0.5;
    let sw = r.w * scale;
    let sh = r.h * scale;
    let sr = Rect::new(cx - sw * 0.5, cy - sh * 0.5, sw, sh);

    let bg = if is_active {
        theme::CRISP_WHITE
    } else if hov {
        theme::VIOLET
    } else {
        theme::ELEVATED
    };
    let border = if is_active {
        theme::VIOLET
    } else if hov {
        theme::VIOLET_LIGHT
    } else {
        theme::BORDER
    };

    ctx.fill_card(sr, 13.0, bg, border);

    let ic_sz = 26.0 * scale;
    let ic_rect = Rect::new(cx - ic_sz * 0.5, cy - ic_sz * 0.5, ic_sz, ic_sz);
    if icon_path.is_empty() || !ctx.draw_app_icon(icon_path, ic_rect) {
        let ic_col = if is_active { theme::DARK_INK } else { theme::TEXT_PRIMARY };
        ctx.draw_vector_icon(fallback, cx, cy, 18.0 * scale, ic_col);
    }

    if is_running {
        let dot_col = if is_active { theme::VIOLET } else { theme::TEXT_SECONDARY };
        if is_left {
            let dh = if is_active { 14.0 } else { 6.0 };
            ctx.fill_rrect(Rect::new(r.x - 6.0, cy - dh * 0.5, 3.5, dh), 1.8, dot_col);
        } else {
            let dw = if is_active { 14.0 } else { 6.0 };
            ctx.fill_rrect(Rect::new(cx - dw * 0.5, r.y + r.h + 3.0, dw, 3.5), 1.8, dot_col);
        }
    }
}
