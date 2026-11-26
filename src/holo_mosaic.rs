//! AuraOS HoloMosaic — native 1:1 port of React Bits `<HoloMosaic />`
//!
//! Defines the preset catalog (Obsidian [exact default React Bits props], Prism, Warm, Cool, Pearl),
//! filter modes (`none`, `dither`, `ascii`), live tuning parameters, Material You accent seeds,
//! and a fast Direct2D thumbnail preview renderer for AuraSettings & Fondos cards.
//! Full-screen live rendering runs 100% isolated on hardware D3D11 HLSL GPU in `holo_mosaic_gpu.rs`
//! inside `AuraWallpaper.exe` (0.00% CPU/GPU load on `AuraShell.exe`).

use crate::renderer::{DrawCtx, Rect};
use crate::theme;

#[derive(Clone, Copy, Debug)]
pub struct HoloPreset {
    pub key: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub base_color: (u8, u8, u8),
    pub metal_color: (u8, u8, u8),
    /// `[min_nm, max_nm, pearl_mix]` matching React Bits `SPECTRUMS`.
    pub spectrum: [f32; 3],
    pub tile_size: f32,
    pub gap: f32,
    pub roundness: f32,
    pub bevel: f32,
    pub rotation_deg: f32,
    pub scatter: f32,
    pub tilt: f32,
    pub swirl: f32,
    pub ripple: f32,
    pub bands: f32,
    pub softness: f32,
    pub intensity: f32,
    pub saturation: f32,
    pub glitter: f32,
    pub sheen: f32,
    pub grain: f32,
    pub speed: f32,
    pub variation: f32,
    pub light_mode: bool,
    /// Accent seed used by AuraOS Material You 3 palette sync.
    pub accent_rgb: (u8, u8, u8),
}

pub const DEFAULT_PRESET_ID: &str = "preset:holo_full";
pub const SPEED_MIN: f32 = 0.0;
pub const SPEED_MAX: f32 = 2.0;
pub const SAT_MIN: f32 = 0.0;
pub const SAT_MAX: f32 = 1.5;

pub const PRESETS: [HoloPreset; 5] = [
    // Exact 1:1 defaults from the user's React Bits <HoloMosaic /> usage example:
    // color="#120f17", metalColor="#e8ebf1", spectrum="full", tileSize=1, gap=0.08,
    // roundness=0.15, bevel=0.35, scatter=0.3, tilt=0.35, saturation=0.1, intensity=1.25,
    // ripple=0.6, glitter=0.85, sheen=0.8, grain=0.08, speed=0.5
    HoloPreset {
        key: "holo_full",
        title: "Obsidian",
        subtitle: "Mosaico metálico React Bits · glitter + ondas al clic",
        base_color: (0x12, 0x0F, 0x17),
        metal_color: (0xE8, 0xEB, 0xF1),
        spectrum: [400.0, 700.0, 0.0],
        tile_size: 1.0,
        gap: 0.08,
        roundness: 0.15,
        bevel: 0.35,
        rotation_deg: 0.0,
        scatter: 0.30,
        tilt: 0.35,
        swirl: 1.0,
        ripple: 0.60,
        bands: 1.30,
        softness: 0.50,
        intensity: 1.25,
        saturation: 0.10,
        glitter: 0.85,
        sheen: 0.80,
        grain: 0.08,
        speed: 0.50,
        variation: 0.0,
        light_mode: false,
        accent_rgb: (158, 148, 224),
    },
    HoloPreset {
        key: "holo_prism",
        title: "Prism",
        subtitle: "Espectro arcoíris completo · iridiscencia holográfica",
        base_color: (0x10, 0x0D, 0x18),
        metal_color: (0xE8, 0xEB, 0xF1),
        spectrum: [400.0, 700.0, 0.0],
        tile_size: 1.0,
        gap: 0.08,
        roundness: 0.15,
        bevel: 0.38,
        rotation_deg: 0.0,
        scatter: 0.30,
        tilt: 0.35,
        swirl: 1.0,
        ripple: 0.65,
        bands: 1.35,
        softness: 0.45,
        intensity: 1.35,
        saturation: 1.00,
        glitter: 0.90,
        sheen: 0.80,
        grain: 0.08,
        speed: 0.50,
        variation: 3.0,
        light_mode: false,
        accent_rgb: (139, 92, 246),
    },
    HoloPreset {
        key: "holo_warm",
        title: "Warm",
        subtitle: "Espectro cálido · oro, ámbar y cobre metálico",
        base_color: (0x16, 0x0F, 0x0D),
        metal_color: (0xF5, 0xE6, 0xD3),
        spectrum: [565.0, 700.0, 0.0],
        tile_size: 1.0,
        gap: 0.08,
        roundness: 0.15,
        bevel: 0.35,
        rotation_deg: 0.0,
        scatter: 0.30,
        tilt: 0.35,
        swirl: 1.0,
        ripple: 0.60,
        bands: 1.30,
        softness: 0.50,
        intensity: 1.30,
        saturation: 0.85,
        glitter: 0.85,
        sheen: 0.85,
        grain: 0.08,
        speed: 0.50,
        variation: 7.0,
        light_mode: false,
        accent_rgb: (245, 158, 11),
    },
    HoloPreset {
        key: "holo_cool",
        title: "Cool",
        subtitle: "Espectro frío · cian, zafiro y esmeralda",
        base_color: (0x0B, 0x11, 0x18),
        metal_color: (0xDC, 0xEC, 0xF7),
        spectrum: [405.0, 565.0, 0.0],
        tile_size: 1.0,
        gap: 0.08,
        roundness: 0.15,
        bevel: 0.35,
        rotation_deg: 0.0,
        scatter: 0.30,
        tilt: 0.35,
        swirl: 1.0,
        ripple: 0.60,
        bands: 1.30,
        softness: 0.50,
        intensity: 1.30,
        saturation: 0.85,
        glitter: 0.85,
        sheen: 0.85,
        grain: 0.08,
        speed: 0.50,
        variation: 11.0,
        light_mode: false,
        accent_rgb: (56, 189, 248),
    },
    HoloPreset {
        key: "holo_pearl",
        title: "Pearl",
        subtitle: "Nacarado perla suave · reflejos sutiles",
        base_color: (0x13, 0x11, 0x1C),
        metal_color: (0xF0, 0xEE, 0xF8),
        spectrum: [420.0, 690.0, 0.55],
        tile_size: 1.0,
        gap: 0.08,
        roundness: 0.18,
        bevel: 0.38,
        rotation_deg: 0.0,
        scatter: 0.25,
        tilt: 0.32,
        swirl: 1.0,
        ripple: 0.55,
        bands: 1.25,
        softness: 0.55,
        intensity: 1.25,
        saturation: 0.65,
        glitter: 0.80,
        sheen: 0.90,
        grain: 0.07,
        speed: 0.45,
        variation: 19.0,
        light_mode: false,
        accent_rgb: (192, 160, 255),
    },
];

pub fn is_holo_key(key: &str) -> bool {
    key.starts_with("holo_") && preset_by_key(key).is_some()
}

pub fn preset_by_key(key: &str) -> Option<&'static HoloPreset> {
    PRESETS.iter().find(|p| p.key == key)
}

pub fn filter_code(filter_name: &str) -> f32 {
    match filter_name.trim().to_ascii_lowercase().as_str() {
        "dither" => 1.0,
        "ascii" => 2.0,
        _ => 0.0,
    }
}

pub fn effective_accent_rgb(key: &str) -> Option<(u8, u8, u8)> {
    preset_by_key(key).map(|p| p.accent_rgb)
}

fn spectral_rgb(t: f32) -> (u8, u8, u8) {
    let x = t.clamp(0.0, 1.0);
    let bump = |v: f32, yo: f32| (1.0 - v * v - yo).clamp(0.0, 1.0);
    let r = bump(3.54585 * (x - 0.69549), 0.02313) + bump(3.90307 * (x - 0.11749), 0.84897);
    let g = bump(2.93225 * (x - 0.49228), 0.15225) + bump(3.21183 * (x - 0.86755), 0.88445);
    let b = bump(2.41594 * (x - 0.27700), 0.52608) + bump(3.96587 * (x - 0.66078), 0.73949);
    (
        (r.clamp(0.0, 1.0) * 255.0) as u8,
        (g.clamp(0.0, 1.0) * 255.0) as u8,
        (b.clamp(0.0, 1.0) * 255.0) as u8,
    )
}

/// Fast lightweight Direct2D thumbnail preview of a HoloMosaic preset inside `dst`
/// (used in AuraSettings cards and the Fondos gallery — live desktop uses full D3D11 HLSL GPU!).
pub fn draw_holo_preview(ctx: &mut DrawCtx<'_>, key: &str, dst: Rect, t: f32) -> bool {
    let Some(p) = preset_by_key(key) else {
        return false;
    };
    let (br, bg, bb) = p.base_color;
    let (mr, mg, mb) = p.metal_color;
    ctx.fill_rrect(dst, 10.0, theme::rgb(br, bg, bb));

    let cols = ((dst.w / 22.0).round() as i32).clamp(6, 16);
    let rows = ((dst.h / 22.0).round() as i32).clamp(4, 10);
    let cell_w = dst.w / cols as f32;
    let cell_h = dst.h / rows as f32;
    let gap = (cell_w.min(cell_h) * 0.11).clamp(1.2, 2.6);
    let rad = (cell_w.min(cell_h) * 0.16).clamp(2.0, 4.5);

    let lx = dst.x + dst.w * (0.52 + 0.22 * (t * 0.35).cos());
    let ly = dst.y + dst.h * (0.46 + 0.18 * (t * 0.45).sin());

    for ry in 0..rows {
        for cx in 0..cols {
            let rx = dst.x + cx as f32 * cell_w + gap * 0.5;
            let ry_px = dst.y + ry as f32 * cell_h + gap * 0.5;
            let tw = (cell_w - gap).max(2.0);
            let th = (cell_h - gap).max(2.0);
            let tile = Rect::new(rx, ry_px, tw, th);

            let mx = rx + tw * 0.5;
            let my = ry_px + th * 0.5;
            let dx = (mx - lx) / dst.w.max(1.0);
            let dy = (my - ly) / dst.h.max(1.0);
            let dist = (dx * dx + dy * dy).sqrt();
            let pool = (1.0 - dist * 1.9).clamp(0.0, 1.0).powf(2.0);

            let h = (((cx * 73 + ry * 151 + p.variation as i32 * 37) as f32).sin() * 43758.545).fract().abs();
            let spec_t = ((p.spectrum[0] - 400.0) / 300.0
                + ((dx * 2.2 - dy * 1.6 + h * 0.45 + 0.5).fract())
                    * ((p.spectrum[1] - p.spectrum[0]) / 300.0))
                .clamp(0.0, 1.0);
            let (sr, sg, sb) = spectral_rgb(spec_t);

            let lum = (0.2126 * sr as f32 + 0.7152 * sg as f32 + 0.0722 * sb as f32) / 255.0;
            let sat = p.saturation.clamp(0.0, 1.2);
            let dif_r = (lum * 255.0) * (1.0 - sat) + sr as f32 * sat;
            let dif_g = (lum * 255.0) * (1.0 - sat) + sg as f32 * sat;
            let dif_b = (lum * 255.0) * (1.0 - sat) + sb as f32 * sat;

            let spec_boost = (0.08 + 0.34 * pool + 0.14 * h * pool) * p.intensity;
            let fr = (br as f32 + 8.0 + mr as f32 * (0.04 + 0.16 * pool) + dif_r * spec_boost).clamp(0.0, 255.0) as u8;
            let fg = (bg as f32 + 8.0 + mg as f32 * (0.04 + 0.16 * pool) + dif_g * spec_boost).clamp(0.0, 255.0) as u8;
            let fb = (bb as f32 + 10.0 + mb as f32 * (0.04 + 0.16 * pool) + dif_b * spec_boost).clamp(0.0, 255.0) as u8;

            ctx.fill_rrect(tile, rad, theme::rgb(fr, fg, fb));
            if pool > 0.25 {
                ctx.stroke_rrect(tile, rad, theme::rgba(mr, mg, mb, 0.16 * pool), 0.8);
            }
            if h > 0.86 && pool > 0.18 {
                let (gr, gg, gb) = spectral_rgb((h * 3.7).fract());
                ctx.fill_circle(rx + tw * (0.25 + 0.5 * h), ry_px + th * 0.32, 1.4, theme::rgba(gr, gg, gb, 0.85 * pool));
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_lookup_and_filter_codes() {
        assert!(is_holo_key("holo_full"));
        assert!(is_holo_key("holo_prism"));
        assert!(is_holo_key("holo_warm"));
        assert!(is_holo_key("holo_cool"));
        assert!(is_holo_key("holo_pearl"));
        assert!(!is_holo_key("waves_ocean"));
        assert_eq!(filter_code("none"), 0.0);
        assert_eq!(filter_code("dither"), 1.0);
        assert_eq!(filter_code("ascii"), 2.0);
        let p = preset_by_key("holo_full").unwrap();
        assert_eq!(p.base_color, (0x12, 0x0F, 0x17));
        assert_eq!(p.metal_color, (0xE8, 0xEB, 0xF1));
        assert!((p.saturation - 0.10).abs() < 1e-4);
    }
}
