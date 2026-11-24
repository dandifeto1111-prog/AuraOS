//! AuraClip (`Win + V`) Clipboard History Manager + macOS Floating Volume/Mute OSD Pill
//! - Zero-overhead clipboard sequence polling (`GetClipboardSequenceNumber`)
//! - Stores up to 25 recent copied text items, supports Pin (`★`), Delete (`×`), Clear All, and
//!   1-click Copy + Auto-Paste (`Ctrl + V`) into the active user window!

use crate::renderer::{DrawCtx, Rect, TextStyle};
use crate::theme::{self, VectorIcon};
use crate::ui_state::{Action, AppState, HitRegion};

pub const CLIP_WIDTH: f32 = 416.0;
pub const CLIP_HEIGHT: f32 = 472.0;

pub const OSD_WIDTH: f32 = 248.0;
pub const OSD_HEIGHT: f32 = 54.0;

#[derive(Debug, Clone)]
pub struct ClipItem {
    pub text: String,
    pub pinned: bool,
    pub chars_len: usize,
}

pub fn get_clipboard_seq() -> u32 {
    #[link(name = "user32")]
    extern "system" {
        fn GetClipboardSequenceNumber() -> u32;
    }
    unsafe { GetClipboardSequenceNumber() }
}

pub fn render_clipboard(ctx: &mut DrawCtx<'_>, state: &AppState, hits: &mut Vec<HitRegion>) {
    hits.clear();
    let t = state.clipboard_anim.value.clamp(0.0, 1.0);
    if t <= 0.01 {
        return;
    }

    let ox = CLIP_WIDTH * 0.5;
    let oy = CLIP_HEIGHT * 0.5;
    let scale = 0.96 + 0.04 * t;
    ctx.set_transform_scale_origin(scale, ox, oy, 0.0, (1.0 - t) * 8.0);
    let _layer = if t < 0.99 {
        ctx.push_opacity_layer(Rect::new(0.0, 0.0, CLIP_WIDTH, CLIP_HEIGHT), t)
    } else {
        None
    };

    let outer = Rect::new(6.0, 6.0, CLIP_WIDTH - 12.0, CLIP_HEIGHT - 12.0);
    ctx.fill_card(outer, 22.0, theme::SURFACE, theme::VIOLET_BORDER);

    let lx = 20.0;
    let cw = CLIP_WIDTH - 40.0;
    let mut y = 18.0;

    ctx.draw_vector_icon(VectorIcon::Sparkle, lx + 10.0, y + 12.0, 14.0, theme::VIOLET_LIGHT);
    ctx.draw_text("Portapapeles AuraClip (Win+V)", Rect::new(lx + 26.0, y + 2.0, 240.0, 22.0), TextStyle::Title14Semi, theme::CRISP_WHITE);

    if !state.clipboard_items.is_empty() {
        let clr = Rect::new(lx + cw - 82.0, y, 82.0, 26.0);
        ctx.fill_card(clr, 8.0, theme::CORAL_BG, theme::BORDER);
        ctx.draw_text_center("Limpiar", clr, TextStyle::Small10Bold, theme::CORAL);
        hits.push(HitRegion { id: "clip_clear".into(), rect: clr, action: Action::ClearClips, right_action: None, tooltip: None, slider: None });
    }
    y += 34.0;
    ctx.draw_line(lx, y, lx + cw, y, theme::BORDER, 1.0);
    y += 10.0;

    let list_r = Rect::new(lx, y, cw, CLIP_HEIGHT - y - 18.0);
    ctx.push_clip(list_r);

    if state.clipboard_items.is_empty() {
        ctx.draw_text_center(
            "Tu historial está vacío.\nCualquier texto que copies (Ctrl+C) aparecerá aquí al instante.",
            Rect::new(lx + 16.0, y + 120.0, cw - 32.0, 80.0),
            TextStyle::Body11Reg,
            theme::TEXT_TERTIARY,
        );
    } else {
        let mut ry = y;
        for (idx, item) in state.clipboard_items.iter().take(8).enumerate() {
            let row = Rect::new(lx, ry, cw, 46.0);
            let id = format!("clip_item_{}", idx);
            let hov = state.hovered_id.as_deref() == Some(id.as_str());
            ctx.fill_card(
                row,
                11.0,
                if hov { theme::VIOLET_TINT } else { theme::CARD_BG },
                if item.pinned { theme::AMBER } else if hov { theme::VIOLET_LIGHT } else { theme::BORDER },
            );

            let one_line: String = item
                .text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ↵ ");
            let preview = if one_line.chars().count() > 46 {
                let s: String = one_line.chars().take(46).collect();
                format!("{}…", s)
            } else {
                one_line
            };

            ctx.draw_text(&preview, Rect::new(row.x + 12.0, row.y + 6.0, row.w - 88.0, 18.0), TextStyle::Body11Med, theme::TEXT_PRIMARY);
            ctx.draw_text(
                &format!("{} caracteres · Clic para pegar", item.chars_len),
                Rect::new(row.x + 12.0, row.y + 24.0, row.w - 88.0, 15.0),
                TextStyle::Small9Reg,
                theme::TEXT_TERTIARY,
            );

            let pin_r = Rect::new(row.x + row.w - 62.0, row.y + 9.0, 26.0, 28.0);
            ctx.fill_rrect(pin_r, 7.0, if item.pinned { theme::rgba(245, 183, 49, 0.22) } else { theme::ELEVATED });
            ctx.draw_text_center("★", pin_r, TextStyle::Body11Med, if item.pinned { theme::AMBER } else { theme::TEXT_TERTIARY });

            let del_r = Rect::new(row.x + row.w - 32.0, row.y + 9.0, 24.0, 28.0);
            ctx.fill_rrect(del_r, 7.0, theme::ELEVATED);
            ctx.draw_text_center("×", del_r, TextStyle::Body11Med, theme::CORAL);

            // Push main row first, then pin & delete on top so `rev()` hit-test hits pin/delete cleanly!
            hits.push(HitRegion { id, rect: row, action: Action::PasteClip(idx), right_action: None, tooltip: None, slider: None });
            hits.push(HitRegion { id: format!("clip_pin_{}", idx), rect: pin_r, action: Action::PinClip(idx), right_action: None, tooltip: None, slider: None });
            hits.push(HitRegion { id: format!("clip_del_{}", idx), rect: del_r, action: Action::DeleteClip(idx), right_action: None, tooltip: None, slider: None });

            ry += 51.0;
        }
    }

    ctx.pop_clip();
    if _layer.is_some() {
        ctx.pop_layer();
    }
    ctx.reset_transform();
}

/// Sleek macOS / iOS Floating Volume & Mute OSD Pill!
pub fn render_volume_osd(ctx: &mut DrawCtx<'_>, state: &AppState) {
    let pill = Rect::new(4.0, 4.0, OSD_WIDTH - 8.0, OSD_HEIGHT - 8.0);
    ctx.fill_card(pill, 22.0, theme::rgba(16, 16, 24, 0.95), theme::VIOLET_BORDER);

    let ic = if state.master_muted || state.master_vol == 0 {
        VectorIcon::VolumeMute
    } else {
        VectorIcon::Volume
    };
    ctx.draw_vector_icon(
        ic,
        28.0,
        OSD_HEIGHT * 0.5,
        15.0,
        if state.master_muted { theme::CORAL } else { theme::VIOLET_LIGHT },
    );

    let bar = Rect::new(48.0, OSD_HEIGHT * 0.5 - 4.0, 138.0, 8.0);
    ctx.fill_rrect(bar, 4.0, theme::ELEVATED);
    if !state.master_muted && state.master_vol > 0 {
        let fw = (state.master_vol as f32 / 100.0 * bar.w).clamp(4.0, bar.w);
        ctx.fill_rrect(Rect::new(bar.x, bar.y, fw, 8.0), 4.0, theme::VIOLET_LIGHT);
    }

    let txt = if state.master_muted {
        "MUTE".to_string()
    } else {
        format!("{}%", state.master_vol)
    };
    ctx.draw_text_right(&txt, Rect::new(190.0, 14.0, 44.0, 26.0), TextStyle::Mono11Med, theme::CRISP_WHITE);
}
