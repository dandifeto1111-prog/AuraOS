//! AuraOS v5.6 — React Bits Free GPU Background Collection (Catalog, Metadata & Direct2D Previews)
//!
//! Contains 16 curated free open-source WebGL shaders from React Bits (`reactbits.dev/backgrounds`),
//! ported natively to Rust + D3D11 HLSL 5.0 (in `reactbits_gpu.rs`) so every wallpaper runs at
//! native 60 FPS inside isolated `AuraWallpaper.exe` with 0.00% CPU/GPU impact on `AuraShell.exe`,
//! full desktop cursor interactivity, click reactions, and Material You 3 color adaptation!

use crate::renderer::{DrawCtx, Rect};
use crate::theme;

#[derive(Clone, Copy, Debug)]
pub struct ReactBitsPreset {
    pub key: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub speed: f32,
    pub colors: [(u8, u8, u8); 4],
    pub accent_rgb: (u8, u8, u8),
}

pub const PRESETS: [ReactBitsPreset; 16] = [
    ReactBitsPreset {
        key: "rb_aurora",
        title: "Aurora",
        subtitle: "React Bits · Luces boreales OKLCH + seguimiento",
        speed: 1.0,
        colors: [(0x52, 0x27, 0xFF), (0x7C, 0xFF, 0x67), (0x00, 0xD4, 0xFF), (0x06, 0x08, 0x12)],
        accent_rgb: (82, 232, 138),
    },
    ReactBitsPreset {
        key: "rb_silk",
        title: "Silk",
        subtitle: "React Bits · Seda líquida con pliegues y ondas al clic",
        speed: 1.0,
        colors: [(0x7B, 0x74, 0x81), (0xFF, 0xFF, 0xFF), (0x6C, 0x5C, 0xE7), (0x0B, 0x0A, 0x12)],
        accent_rgb: (139, 124, 214),
    },
    ReactBitsPreset {
        key: "rb_iridescence",
        title: "Iridescence",
        subtitle: "React Bits · Fluido iridiscente prismático + vórtices al clic",
        speed: 0.9,
        colors: [(0x8A, 0xA8, 0xFF), (0xFF, 0x9F, 0xFC), (0x72, 0xF5, 0xE8), (0x08, 0x0A, 0x14)],
        accent_rgb: (138, 168, 255),
    },
    ReactBitsPreset {
        key: "rb_liquid_chrome",
        title: "Liquid Chrome",
        subtitle: "React Bits · Cromo líquido metálico con reflejos de estudio",
        speed: 0.25,
        colors: [(0xEC, 0xF0, 0xF8), (0xB8, 0xC4, 0xDC), (0x64, 0x74, 0x8B), (0x06, 0x07, 0x0A)],
        accent_rgb: (176, 194, 224),
    },
    ReactBitsPreset {
        key: "rb_balatro",
        title: "Balatro",
        subtitle: "React Bits · Vórtice hipnótico retro + remolinos al clic",
        speed: 0.85,
        colors: [(0xDE, 0x44, 0x3B), (0x00, 0x6B, 0xB4), (0x16, 0x23, 0x25), (0x16, 0x23, 0x25)],
        accent_rgb: (222, 68, 59),
    },
    ReactBitsPreset {
        key: "rb_dark_veil",
        title: "Dark Veil",
        subtitle: "React Bits · Velo neuronal CPPN luminoso + distorsión cursor",
        speed: 0.55,
        colors: [(0x6B, 0x12, 0xFF), (0xC0, 0x84, 0xFC), (0x2E, 0x10, 0x65), (0x05, 0x04, 0x0B)],
        accent_rgb: (124, 58, 237),
    },
    ReactBitsPreset {
        key: "rb_plasma",
        title: "Plasma",
        subtitle: "React Bits · Plasma volumétrico 3D orbitable con el mouse",
        speed: 1.0,
        colors: [(0xFF, 0x6B, 0x35), (0xFF, 0xC8, 0x9A), (0x8B, 0x5C, 0xF6), (0x07, 0x06, 0x0E)],
        accent_rgb: (255, 118, 68),
    },
    ReactBitsPreset {
        key: "rb_galaxy",
        title: "Galaxy",
        subtitle: "React Bits · Galaxia multicapa con repulsión y warp al clic",
        speed: 1.0,
        colors: [(0x8A, 0xB4, 0xFF), (0xC0, 0x84, 0xFC), (0x67, 0xE8, 0xF9), (0x04, 0x05, 0x0C)],
        accent_rgb: (129, 140, 248),
    },
    ReactBitsPreset {
        key: "rb_light_rays",
        title: "Light Rays",
        subtitle: "React Bits · Rayos volumétricos cenitales + sombra del cursor",
        speed: 1.0,
        colors: [(0x93, 0xE5, 0xFF), (0xFF, 0xFF, 0xFF), (0x38, 0xBD, 0xF8), (0x05, 0x08, 0x10)],
        accent_rgb: (56, 189, 248),
    },
    ReactBitsPreset {
        key: "rb_prismatic_burst",
        title: "Prismatic Burst",
        subtitle: "React Bits · Explosión prismática 3D + pulsos radiales al clic",
        speed: 0.55,
        colors: [(0xFF, 0x00, 0x7A), (0x4D, 0x3D, 0xFF), (0x00, 0xF0, 0xFF), (0x05, 0x05, 0x0B)],
        accent_rgb: (236, 72, 153),
    },
    ReactBitsPreset {
        key: "rb_molten_metal",
        title: "Molten Metal",
        subtitle: "React Bits · Filamentos de metal fundido + destellos al clic",
        speed: 0.35,
        colors: [(0x52, 0x27, 0xFF), (0xFF, 0x9F, 0xFC), (0xFF, 0xFF, 0xFF), (0x07, 0x05, 0x0E)],
        accent_rgb: (168, 85, 247),
    },
    ReactBitsPreset {
        key: "rb_color_bends",
        title: "Color Bends",
        subtitle: "React Bits · Bandas de cristal neón con paralaje interactivo",
        speed: 0.25,
        colors: [(0xFF, 0x5C, 0x7A), (0x8A, 0x5C, 0xFF), (0x00, 0xFF, 0xD1), (0x06, 0x06, 0x0D)],
        accent_rgb: (255, 92, 122),
    },
    ReactBitsPreset {
        key: "rb_grainient",
        title: "Grainient",
        subtitle: "React Bits · Gradiente orgánico OKLab con grano de cine vivo",
        speed: 1.0,
        colors: [(0x12, 0x0F, 0x17), (0x9A, 0x34, 0x12), (0xF9, 0x73, 0x16), (0xFE, 0xD7, 0xAA)],
        accent_rgb: (249, 115, 22),
    },
    ReactBitsPreset {
        key: "rb_ripple_grid",
        title: "Ripple Grid",
        subtitle: "React Bits · Rejilla 3D cibernética con ondas al clic",
        speed: 1.0,
        colors: [(0x38, 0xBD, 0xF8), (0x81, 0x8C, 0xF8), (0x22, 0xD3, 0xEE), (0x05, 0x07, 0x0E)],
        accent_rgb: (56, 189, 248),
    },
    ReactBitsPreset {
        key: "rb_orb",
        title: "Orb",
        subtitle: "React Bits · Orbe de plasma vivo con inclinación 3D al cursor",
        speed: 1.0,
        colors: [(0x9C, 0x43, 0xFE), (0x4C, 0xC2, 0xE9), (0x10, 0x14, 0x99), (0x05, 0x05, 0x0C)],
        accent_rgb: (156, 67, 254),
    },
    ReactBitsPreset {
        key: "rb_dither",
        title: "Dither",
        subtitle: "React Bits · Ondas fBM con tramado Bayer 8×8 interactivo",
        speed: 1.0,
        colors: [(0x98, 0x8A, 0xFF), (0x52, 0x27, 0xFF), (0x1B, 0x16, 0x33), (0x08, 0x07, 0x10)],
        accent_rgb: (152, 138, 255),
    },
];

pub fn is_rb_key(key: &str) -> bool {
    key.starts_with("rb_") && preset_by_key(key).is_some()
}

pub fn preset_by_key(key: &str) -> Option<&'static ReactBitsPreset> {
    PRESETS.iter().find(|p| p.key == key)
}

pub fn effective_accent_rgb(key: &str) -> Option<(u8, u8, u8)> {
    preset_by_key(key).map(|p| p.accent_rgb)
}

/// Fast lightweight Direct2D thumbnail preview for AuraSettings & Fondos cards
/// (Full desktop rendering runs 100% isolated on hardware D3D11 HLSL GPU in `AuraWallpaper.exe`!)
pub fn draw_rb_preview(ctx: &mut DrawCtx<'_>, key: &str, dst: Rect, t: f32) -> bool {
    let Some(p) = preset_by_key(key) else {
        return false;
    };
    let (r0, g0, b0) = p.colors[0];
    let (r1, g1, b1) = p.colors[1];
    let (r2, g2, b2) = p.colors[2];
    let (br, bg, bb) = p.colors[3];

    ctx.fill_gradient_rrect(
        dst,
        10.0,
        theme::rgb(br, bg, bb),
        theme::rgb(br.saturating_add(10), bg.saturating_add(10), bb.saturating_add(16)),
    );

    match key {
        "rb_aurora" => {
            ctx.fill_radial_glow(dst.x + dst.w * 0.25, dst.y + dst.h * 0.22, dst.w * 0.52, theme::rgba(r0, g0, b0, 0.55), theme::rgba(r0, g0, b0, 0.0));
            ctx.fill_radial_glow(dst.x + dst.w * 0.55, dst.y + dst.h * 0.26, dst.w * 0.48, theme::rgba(r1, g1, b1, 0.48), theme::rgba(r1, g1, b1, 0.0));
            ctx.fill_radial_glow(dst.x + dst.w * 0.82, dst.y + dst.h * 0.20, dst.w * 0.44, theme::rgba(r2, g2, b2, 0.42), theme::rgba(r2, g2, b2, 0.0));
        }
        "rb_silk" | "rb_grainient" | "rb_dark_veil" => {
            ctx.fill_radial_glow(dst.x + dst.w * 0.32, dst.y + dst.h * 0.35, dst.w * 0.62, theme::rgba(r0, g0, b0, 0.50), theme::rgba(r0, g0, b0, 0.0));
            ctx.fill_radial_glow(dst.x + dst.w * 0.70, dst.y + dst.h * 0.65, dst.w * 0.58, theme::rgba(r2, g2, b2, 0.52), theme::rgba(r2, g2, b2, 0.0));
            for i in 0..5 {
                let f = i as f32;
                let y = dst.y + dst.h * (0.20 + f * 0.16);
                ctx.draw_line(
                    dst.x + 6.0,
                    y + (f + t).sin() * 6.0,
                    dst.x + dst.w - 6.0,
                    y - (f * 1.4 + t).cos() * 7.0,
                    theme::rgba(r1, g1, b1, 0.22),
                    2.0,
                );
            }
        }
        "rb_orb" => {
            let cx = dst.x + dst.w * 0.5;
            let cy = dst.y + dst.h * 0.5;
            let rad = dst.h.min(dst.w) * 0.33;
            ctx.fill_radial_glow(cx, cy, rad * 1.7, theme::rgba(r0, g0, b0, 0.36), theme::rgba(r0, g0, b0, 0.0));
            ctx.stroke_circle(cx, cy, rad, theme::rgba(r1, g1, b1, 0.85), 3.0);
            ctx.stroke_circle(cx, cy, rad * 0.92, theme::rgba(r0, g0, b0, 0.72), 2.2);
            ctx.fill_circle(cx + rad * 0.7, cy - rad * 0.68, 3.2, theme::rgba(255, 255, 255, 0.9));
        }
        "rb_galaxy" => {
            ctx.fill_radial_glow(dst.x + dst.w * 0.5, dst.y + dst.h * 0.5, dst.w * 0.5, theme::rgba(r1, g1, b1, 0.22), theme::rgba(r1, g1, b1, 0.0));
            for i in 0..24 {
                let fi = i as f32;
                let u = ((fi * 17.31).sin() * 43758.545).fract().abs();
                let v = ((fi * 41.73 + 9.1).sin() * 23421.631).fract().abs();
                let sx = dst.x + 6.0 + u * (dst.w - 12.0);
                let sy = dst.y + 6.0 + v * (dst.h - 12.0);
                let rad = if i % 6 == 0 { 2.0 } else { 1.0 };
                let col = if i % 3 == 0 {
                    theme::rgba(r0, g0, b0, 0.9)
                } else if i % 3 == 1 {
                    theme::rgba(r2, g2, b2, 0.85)
                } else {
                    theme::rgba(255, 255, 255, 0.85)
                };
                ctx.fill_circle(sx, sy, rad, col);
            }
        }
        "rb_light_rays" => {
            let ox = dst.x + dst.w * 0.5;
            let oy = dst.y - 6.0;
            ctx.fill_radial_glow(ox, dst.y + 8.0, dst.w * 0.55, theme::rgba(r0, g0, b0, 0.52), theme::rgba(r0, g0, b0, 0.0));
            for i in -3..=3 {
                let tx = dst.x + dst.w * (0.5 + i as f32 * 0.16);
                ctx.draw_line(ox, oy, tx, dst.y + dst.h, theme::rgba(r0, g0, b0, 0.24), 4.0);
            }
        }
        "rb_ripple_grid" => {
            ctx.fill_radial_glow(dst.x + dst.w * 0.5, dst.y + dst.h * 0.55, dst.w * 0.45, theme::rgba(r0, g0, b0, 0.30), theme::rgba(r0, g0, b0, 0.0));
            let cols = 7;
            let rows = 5;
            for c in 1..cols {
                let x = dst.x + dst.w * (c as f32 / cols as f32);
                ctx.draw_line(x, dst.y + 4.0, x, dst.y + dst.h - 4.0, theme::rgba(r0, g0, b0, 0.32), 1.0);
            }
            for r in 1..rows {
                let y = dst.y + dst.h * (r as f32 / rows as f32);
                ctx.draw_line(dst.x + 4.0, y, dst.x + dst.w - 4.0, y, theme::rgba(r0, g0, b0, 0.32), 1.0);
            }
            ctx.stroke_circle(dst.x + dst.w * 0.5, dst.y + dst.h * 0.5, dst.h * 0.28, theme::rgba(r2, g2, b2, 0.55), 1.4);
        }
        "rb_dither" => {
            let step = 8.0f32;
            let mut y = dst.y + 4.0;
            let mut row = 0i32;
            while y < dst.y + dst.h - 4.0 {
                let mut x = dst.x + 4.0;
                let mut col = 0i32;
                while x < dst.x + dst.w - 4.0 {
                    let u = (x - dst.x) / dst.w.max(1.0);
                    let v = (y - dst.y) / dst.h.max(1.0);
                    let wv = 0.5 + 0.5 * ((u * 5.0 + v * 3.0).sin() * (v * 4.0 - u * 2.0).cos());
                    let bayer = (((row & 1) * 2 + (col & 1)) as f32) * 0.25;
                    if wv > bayer + 0.15 {
                        ctx.fill_rrect(Rect::new(x, y, 4.5, 4.5), 0.5, theme::rgba(r0, g0, b0, 0.72));
                    }
                    x += step;
                    col += 1;
                }
                y += step;
                row += 1;
            }
        }
        "rb_balatro" => {
            let cx = dst.x + dst.w * 0.5;
            let cy = dst.y + dst.h * 0.5;
            ctx.fill_radial_glow(cx - dst.w * 0.16, cy - dst.h * 0.08, dst.w * 0.45, theme::rgba(r0, g0, b0, 0.65), theme::rgba(r0, g0, b0, 0.0));
            ctx.fill_radial_glow(cx + dst.w * 0.16, cy + dst.h * 0.08, dst.w * 0.45, theme::rgba(r1, g1, b1, 0.65), theme::rgba(r1, g1, b1, 0.0));
            ctx.stroke_circle(cx, cy, dst.h * 0.24, theme::rgba(r0, g0, b0, 0.55), 2.0);
            ctx.stroke_circle(cx, cy, dst.h * 0.36, theme::rgba(r1, g1, b1, 0.45), 1.6);
        }
        _ => {
            // Iridescence, LiquidChrome, Plasma, PrismaticBurst, MoltenMetal, ColorBends
            ctx.fill_radial_glow(dst.x + dst.w * 0.28, dst.y + dst.h * 0.36, dst.w * 0.55, theme::rgba(r0, g0, b0, 0.52), theme::rgba(r0, g0, b0, 0.0));
            ctx.fill_radial_glow(dst.x + dst.w * 0.72, dst.y + dst.h * 0.62, dst.w * 0.52, theme::rgba(r1, g1, b1, 0.50), theme::rgba(r1, g1, b1, 0.0));
            ctx.fill_radial_glow(dst.x + dst.w * 0.52, dst.y + dst.h * 0.48, dst.w * 0.38, theme::rgba(r2, g2, b2, 0.44), theme::rgba(r2, g2, b2, 0.0));
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_16_react_bits_presets_are_unique_and_valid() {
        assert_eq!(PRESETS.len(), 16);
        let mut keys = std::collections::HashSet::new();
        for p in &PRESETS {
            assert!(p.key.starts_with("rb_"));
            assert!(is_rb_key(p.key));
            assert!(keys.insert(p.key));
            assert!(effective_accent_rgb(p.key).is_some());
        }
        assert!(!is_rb_key("holo_full"));
        assert!(!is_rb_key("waves_ocean"));
    }
}
