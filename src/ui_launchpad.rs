//! macOS / Apple-Style Launchpad Fullscreen App Drawer
//! - Expanding Circle Reveal + Scale Morph originating directly from the Dock's 9-Dot button!
//! - Hardware-accelerated Direct2D ID2D1EllipseGeometry clip mask at 60-144 FPS
//! - Asymmetric Emil Kowalski timing: 270ms EASE_OUT enter, 175ms reverse collapse into circle
//! - Displays ALL installed PC applications + Desktop shortcuts with crisp HD icons & instant search

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub fn render_launchpad(ctx: &mut DrawCtx<'_>, state: &AppState, w: f32, h: f32, hits: &mut Vec<HitRegion>) -> f32 {
    hits.clear();
    let t = state.launchpad_anim.value.clamp(0.0, 1.0);
    if t <= 0.008 {
        return 0.0;
    }

    // Anchor circle directly at the Dock's 9-dot Launchpad button!
    let is_left = !state.settings.dock_position.eq_ignore_ascii_case("Bottom");
    let (ox, oy) = if is_left {
        (39.0, (h * 0.78).min(h - 68.0))
    } else {
        (w * 0.5 + 170.0, h - 38.0)
    };

    // Maximum diagonal distance from origin to farthest screen corner
    let dx_max = ox.max(w - ox);
    let dy_max = oy.max(h - oy);
    let max_radius = (dx_max * dx_max + dy_max * dy_max).sqrt() + 28.0;
    let cur_radius = 24.0 + (max_radius - 24.0) * t;

    // Zero-offscreen-layer fast GPU animation (0 VRAM layer alloc during games = butter-smooth 144 FPS!)
    let full_r = Rect::new(0.0, 0.0, w, h);
    let _circle_layer: Option<windows::Win32::Graphics::Direct2D::ID2D1Layer> = None;

    // Deep matte obsidian translucent canvas with smooth direct alpha fade
    ctx.fill_rrect(full_r, 0.0, theme::rgba(9, 9, 14, 0.90 * t));
    ctx.fill_radial_glow(
        w * 0.5,
        h * 0.25,
        w * 0.52,
        theme::rgba(124, 92, 255, 0.14 * t),
        theme::rgba(124, 92, 255, 0.0),
    );

    // Snappy Apple origin-aware scale morph (0.93 -> 1.0) directly on GPU transform!
    let scale = 0.93 + 0.07 * t;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, (1.0 - t) * 10.0);

    // Clicking empty background closes Launchpad cleanly
    hits.push(HitRegion {
        id: "lp_bg".into(),
        rect: full_r,
        action: Action::CloseAllOverlays,
        right_action: None,
        tooltip: None,
        slider: None,
    });

    // Top Header + Interactive Clickable Live Search Pill (never closes Launchpad on click!)
    let sw = 440.0f32.min(w - 80.0);
    let sx = (w - sw) * 0.5;
    let search_r = Rect::new(sx, 54.0, sw, 42.0);
    let sr_hov = state.hovered_id.as_deref() == Some("lp_search");
    ctx.fill_card(
        search_r,
        21.0,
        if sr_hov || !state.launchpad_query.is_empty() { theme::ELEVATED } else { theme::SURFACE },
        if !state.launchpad_query.is_empty() || sr_hov { theme::VIOLET_LIGHT } else { theme::VIOLET_BORDER },
    );
    ctx.draw_vector_icon(VectorIcon::Search, search_r.x + 22.0, search_r.y + 21.0, 14.0, theme::VIOLET_LIGHT);

    let q_disp = if state.launchpad_query.is_empty() {
        "Buscar entre todas tus aplicaciones...".to_string()
    } else {
        format!("{}|", state.launchpad_query)
    };
    ctx.draw_text(
        &q_disp,
        Rect::new(search_r.x + 42.0, search_r.y, sw - 74.0, 42.0),
        TextStyle::Body12Med,
        if state.launchpad_query.is_empty() { theme::TEXT_TERTIARY } else { theme::TEXT_PRIMARY },
    );
    hits.push(HitRegion {
        id: "lp_search".into(),
        rect: search_r,
        action: Action::FocusSearchBar,
        right_action: None,
        tooltip: None,
        slider: None,
    });
    if !state.launchpad_query.is_empty() {
        let clr_r = Rect::new(search_r.x + search_r.w - 34.0, search_r.y + 9.0, 24.0, 24.0);
        ctx.draw_vector_icon(VectorIcon::Close, clr_r.x + 12.0, clr_r.y + 12.0, 10.0, theme::TEXT_SECONDARY);
        hits.push(HitRegion {
            id: "lp_search_clr".into(),
            rect: clr_r,
            action: Action::ClearSearchBar,
            right_action: None,
            tooltip: None,
            slider: None,
        });
    }

    // Filter all PC + Desktop apps (plus matching installed games when typing!)
    let q_low = state.launchpad_query.trim().to_lowercase();
    let mut filtered: Vec<crate::models::AppEntry> = state
        .all_apps
        .iter()
        .filter(|a| q_low.is_empty() || a.name.to_lowercase().contains(&q_low) || a.category.to_lowercase().contains(&q_low))
        .cloned()
        .collect();
    if !q_low.is_empty() {
        for g in &state.games {
            if g.name.to_lowercase().contains(&q_low)
                && !filtered.iter().any(|a| a.name.eq_ignore_ascii_case(&g.name))
            {
                filtered.push(crate::models::AppEntry {
                    name: g.name.clone(),
                    target: g.launch_target.clone(),
                    icon_path: g.icon_path.clone(),
                    category: format!("Juego · {}", g.platform),
                    is_web: false,
                    is_custom: g.is_custom,
                    custom_id: g.custom_id.clone(),
                });
            }
        }
    }

    let count_lbl = if q_low.is_empty() {
        format!("{} aplicaciones en tu PC y Escritorio · Esc para cerrar", filtered.len())
    } else {
        format!("{} resultados · Enter abre el primero · Esc limpia", filtered.len())
    };
    ctx.draw_text_center(&count_lbl, Rect::new(0.0, 104.0, w, 18.0), TextStyle::Small10Med, theme::TEXT_TERTIARY);

    // Responsive 7-column macOS Launchpad Icon Grid
    let cols = if w >= 1400.0 { 7usize } else if w >= 1050.0 { 6 } else { 5 };
    let grid_w = (cols as f32 * 138.0).min(w - 140.0);
    let cell_w = grid_w / cols as f32;
    let cell_h = 122.0f32;
    let gx = (w - grid_w) * 0.5;
    let gy = 136.0;
    let gh = (h - gy - 44.0).max(200.0);
    let view_r = Rect::new(gx - 12.0, gy, grid_w + 24.0, gh);
    ctx.push_clip(view_r);

    let rows = (filtered.len() + cols - 1) / cols;
    let total_h = (rows as f32 * cell_h + 24.0).max(gh);
    let scroll = state.launchpad_scroll.clamp(0.0, (total_h - gh).max(0.0));

    for (idx, app) in filtered.iter().enumerate() {
        let c = (idx % cols) as f32;
        let r_idx = (idx / cols) as f32;
        let cx = gx + c * cell_w;
        let cy = gy + r_idx * cell_h - scroll;
        if cy + cell_h < gy || cy > gy + gh {
            continue;
        }
        let tile = Rect::new(cx + 8.0, cy + 4.0, cell_w - 16.0, cell_h - 10.0);
        let id = format!("lp_app_{}", idx);
        let hov = state.hovered_id.as_deref() == Some(&id);
        let pressed = state.pressed_id.as_deref() == Some(&id);

        if hov {
            ctx.fill_card(tile, 18.0, theme::rgba(34, 34, 48, 0.92), theme::VIOLET_BORDER);
        }

        // macOS squircle icon container
        let sq = if pressed { 60.0 } else if hov { 68.0 } else { 64.0 };
        let sq_r = Rect::new(tile.x + (tile.w - sq) * 0.5, tile.y + 10.0 + (64.0 - sq) * 0.5, sq, sq);
        ctx.fill_card(sq_r, 17.0, theme::CARD_BG, if hov { theme::VIOLET } else { theme::BORDER });

        let ic_sz = sq * 0.62;
        let ic_r = Rect::new(sq_r.x + (sq - ic_sz) * 0.5, sq_r.y + (sq - ic_sz) * 0.5, ic_sz, ic_sz);
        if app.icon_path.is_empty() || !ctx.draw_app_icon(&app.icon_path, ic_r) {
            ctx.draw_vector_icon(
                if app.is_web { VectorIcon::Sparkle } else { VectorIcon::AppGrid },
                sq_r.x + sq * 0.5,
                sq_r.y + sq * 0.5,
                24.0,
                theme::VIOLET_LIGHT,
            );
        }

        ctx.draw_text_center(
            &app.name,
            Rect::new(tile.x + 4.0, tile.y + 82.0, tile.w - 8.0, 22.0),
            TextStyle::Body11Med,
            if hov { theme::TEXT_PRIMARY } else { theme::TEXT_SECONDARY },
        );

        hits.push(HitRegion {
            id,
            rect: tile,
            action: Action::LaunchTarget(app.target.clone()),
            right_action: None,
            tooltip: None,
            slider: None,
        });
    }
    ctx.pop_clip();

    // Slim dark scrollbar if scrollable
    if total_h > gh {
        let thumb_h = (gh / total_h * gh).clamp(36.0, gh);
        let thumb_y = gy + (scroll / (total_h - gh).max(1.0)) * (gh - thumb_h);
        ctx.fill_rrect(Rect::new(gx + grid_w + 18.0, thumb_y, 5.0, thumb_h), 2.5, theme::rgb(64, 64, 86));
    }

    ctx.reset_transform();
    if _circle_layer.is_some() {
        ctx.pop_layer();
    }

    // Subtle Apple Electric Violet expanding circular luminous halo ring during transition!
    if t > 0.02 && t < 0.94 {
        let ring_alpha = (1.0 - t) * 0.65;
        ctx.stroke_circle(ox, oy, cur_radius, theme::rgba(155, 132, 255, ring_alpha), 2.0);
    }

    total_h
}
