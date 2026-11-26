//! AuraOS v5.6 Hardware D3D11 HLSL HoloMosaic Renderer (1:1 React Bits `<HoloMosaic />` GPU Port)
//!
//! Runs 100% isolated inside `AuraWallpaper.exe` on its own D3D11 device + DirectComposition
//! swapchain (0.00% CPU/GPU load on `AuraShell.exe` — zero lag on Dock, TopBar, Overview or Windows!).
//!
//! Ported line-for-line from React Bits `<HoloMosaic />` (WebGL2 GLSL 300 es -> D3D11 HLSL 5.0):
//! - Procedural rounded beveled metallic tiles with diffraction grating spectral iridescence
//! - Exact damped spring physics for wandering spotlight + cursor tracking & tile hover tilt
//! - Radial intro flip-in animation (`2.4 s`) + up to 6 simultaneous click tile-flip waves (`4.0 s`)
//! - Animated rainbow micro-glitter sparkles, metallic specular sheen, and fine film grain
//! - Full support for `none`, `dither` (4x4 Bayer), and `ascii` (5x5 glyph matrix) filters
//! - Zero extra crate dependencies: compiles HLSL via Windows' built-in `d3dcompiler_47.dll`!

use crate::holo_mosaic::{self, HoloPreset};
use crate::win32_shell::to_wide;
use std::ffi::c_void;
use std::path::Path;
use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND};
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11Texture2D, ID3D11VertexShader,
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ,
    D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CULL_NONE, D3D11_FILL_SOLID,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_MAP_WRITE_DISCARD, D3D11_RASTERIZER_DESC,
    D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC,
    D3D11_USAGE_STAGING, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIDevice, IDXGIDevice3, IDXGIFactory2, IDXGISwapChain1,
    DXGI_CREATE_FACTORY_FLAGS, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

pub const MAX_WAVES: usize = 6;
const TILE_PX: f32 = 52.0;
const EYE: f32 = 1.15;
const INTRO_SECONDS: f32 = 2.4;
const WAVE_SECONDS: f32 = 4.0;

#[repr(C)]
#[derive(Clone, Copy)]
struct HoloConstants {
    // float4 u_res_time: xy=uRes, z=uPixel, w=uTime
    u_res_time: [f32; 4],
    // float4 u_clock_tile_rot: x=uClock, y=uTile, zw=uRot (cos, sin)
    u_clock_tile_rot: [f32; 4],
    // float4 u_geom: x=uGap, y=uRound, z=uBevel, w=uScatter
    u_geom: [f32; 4],
    // float4 u_pattern: x=uTilt, y=uSwirl, z=uRipple, w=uBands
    u_pattern: [f32; 4],
    // float4 u_light_props: x=uSoft, y=uIntensity, z=uSaturation, w=uGlitter
    u_light_props: [f32; 4],
    // float4 u_spectrum_sheen: xyz=uSpectrum, w=uSheen
    u_spectrum_sheen: [f32; 4],
    // float4 u_color_grain: xyz=uColor, w=uGrain
    u_color_grain: [f32; 4],
    // float4 u_metal_fade: xyz=uMetal, w=uFade
    u_metal_fade: [f32; 4],
    // float4 u_light_op: xy=uLight, z=uOpacity, w=uLightMode
    u_light_op: [f32; 4],
    // float4 u_hover: xy=hover pos, z=radius, w=strength
    u_hover: [f32; 4],
    // float4 u_intro: xy=(0,0), z=introTime, w=introLive (1 or 0)
    u_intro: [f32; 4],
    // float4 u_misc: x=uSeed, y=uFrame, z=uFilter, w=uFilterSize
    u_misc: [f32; 4],
    // float4 u_waves[6]: xy=pos, z=startClock, w=active (1 or 0)
    u_waves: [[f32; 4]; MAX_WAVES],
}

const HLSL_SRC: &str = r#"
cbuffer HoloCB : register(b0) {
    float4 u_res_time;       // xy=uRes, z=uPixel, w=uTime
    float4 u_clock_tile_rot; // x=uClock, y=uTile, zw=uRot
    float4 u_geom;           // x=uGap, y=uRound, z=uBevel, w=uScatter
    float4 u_pattern;        // x=uTilt, y=uSwirl, z=uRipple, w=uBands
    float4 u_light_props;    // x=uSoft, y=uIntensity, z=uSaturation, w=uGlitter
    float4 u_spectrum_sheen; // xyz=uSpectrum, w=uSheen
    float4 u_color_grain;    // xyz=uColor, w=uGrain
    float4 u_metal_fade;     // xyz=uMetal, w=uFade
    float4 u_light_op;       // xy=uLight, z=uOpacity, w=uLightMode
    float4 u_hover;          // xy=pos, z=radius, w=strength
    float4 u_intro;          // xy=origin, z=introTime, w=introLive
    float4 u_misc;           // x=uSeed, y=uFrame, z=uFilter, w=uFilterSize
    float4 u_waves[6];       // xy=pos, z=startClock, w=active
};

struct VSOut {
    float4 pos : SV_POSITION;
};

VSOut VSMain(uint vid : SV_VertexID) {
    float2 p = float2((float)((vid << 1) & 2), (float)(vid & 2));
    VSOut o;
    o.pos = float4(p * 2.0 - 1.0, 0.0, 1.0);
    return o;
}

float ss(float e0, float e1, float x) {
    float t = saturate((x - e0) / (e1 - e0));
    return t * t * (3.0 - 2.0 * t);
}

float glsl_mod(float x, float y) {
    return x - y * floor(x / y);
}

float h21(float2 p) {
    float3 q = frac(float3(p.x, p.y, p.x) * float3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return frac((q.x + q.y) * q.z);
}

float h31(float3 p) {
    p = frac(p * float3(0.1031, 0.1030, 0.0973));
    p += dot(p, p.yxz + 33.33);
    return frac((p.x + p.y) * p.z);
}

float vn3(float3 p) {
    float3 i = floor(p);
    float3 f = frac(p);
    f = f * f * (3.0 - 2.0 * f);
    return lerp(
        lerp(
            lerp(h31(i), h31(i + float3(1, 0, 0)), f.x),
            lerp(h31(i + float3(0, 1, 0)), h31(i + float3(1, 1, 0)), f.x),
            f.y
        ),
        lerp(
            lerp(h31(i + float3(0, 0, 1)), h31(i + float3(1, 0, 1)), f.x),
            lerp(h31(i + float3(0, 1, 1)), h31(i + float3(1, 1, 1)), f.x),
            f.y
        ),
        f.z
    );
}

float3 bump3y(float3 x, float3 yo) {
    return clamp(1.0 - x * x - yo, 0.0, 1.0);
}

float3 spectral(float w) {
    float x = saturate((w - 400.0) / 300.0);
    float3 c1 = float3(3.54585104, 2.93225262, 2.41593945);
    float3 x1 = float3(0.69549072, 0.49228336, 0.27699880);
    float3 y1 = float3(0.02312639, 0.15225084, 0.52607955);
    float3 c2 = float3(3.90307140, 3.21182957, 3.96587128);
    float3 x2 = float3(0.11748627, 0.86755042, 0.66077860);
    float3 y2 = float3(0.84897130, 0.88445281, 0.73949448);
    return bump3y(c1 * (x - x1), y1) + bump3y(c2 * (x - x2), y2);
}

float3 band(float lam) {
    float3 uSpectrum = u_spectrum_sheen.xyz;
    float wnd = ss(380.0, 425.0, lam) * (1.0 - ss(680.0, 780.0, lam));
    if (wnd <= 0.0) return float3(0.0, 0.0, 0.0);
    return spectral(lerp(uSpectrum.x, uSpectrum.y, saturate((lam - 380.0) / 400.0))) * wnd;
}

float boxDist(float2 p, float b, float r, out float2 grad) {
    float2 q = abs(p) - b + r;
    float2 m = max(q, 0.0);
    float outside = length(m);
    float2 sg = float2(p.x < 0.0 ? -1.0 : 1.0, p.y < 0.0 ? -1.0 : 1.0);
    grad = outside > 0.0 ? sg * m / outside : sg * (q.x > q.y ? float2(1.0, 0.0) : float2(0.0, 1.0));
    return outside + min(max(q.x, q.y), 0.0) - r;
}

float4 shade(float2 frag, float solid) {
    float2 uRes        = u_res_time.xy;
    float  uPixel      = u_res_time.z;
    float  uTime       = u_res_time.w;
    float  uClock      = u_clock_tile_rot.x;
    float  uTile       = u_clock_tile_rot.y;
    float2 uRot        = u_clock_tile_rot.zw;
    float  uGap        = u_geom.x;
    float  uRound      = u_geom.y;
    float  uBevel      = u_geom.z;
    float  uScatter    = u_geom.w;
    float  uTilt       = u_pattern.x;
    float  uSwirl      = u_pattern.y;
    float  uRipple     = u_pattern.z;
    float  uBands      = u_pattern.w;
    float  uSoft       = u_light_props.x;
    float  uIntensity  = u_light_props.y;
    float  uSaturation = u_light_props.z;
    float  uGlitter    = u_light_props.w;
    float3 uSpectrum   = u_spectrum_sheen.xyz;
    float  uSheen      = u_spectrum_sheen.w;
    float3 uColor      = u_color_grain.xyz;
    float  uGrain      = u_color_grain.w;
    float3 uMetal      = u_metal_fade.xyz;
    float  uFade       = u_metal_fade.w;
    float2 uLight      = u_light_op.xy;
    float  uOpacity    = u_light_op.z;
    float  uLightMode  = u_light_op.w;
    float  uSeed       = u_misc.x;
    float  uFrame      = u_misc.y;

    float2 c = frag - 0.5 * uRes;
    float2 s = c / uRes.y;
    float2 g = float2(uRot.x * c.x + uRot.y * c.y, uRot.x * c.y - uRot.y * c.x) / uTile;
    float2 id = floor(g);
    float2 f = g - id - 0.5;
    float2 gc = (id + 0.5) * uTile;
    float2 cell = float2(uRot.x * gc.x - uRot.y * gc.y, uRot.y * gc.x + uRot.x * gc.y) / uRes.y;
    float2 seed = float2(uSeed * 17.13, uSeed * 31.71);
    float h1 = h21(id + seed);
    float h2 = h21(id + seed + 19.7);
    float h3 = h21(id + seed + 41.3);

    float flip = 0.0;
    float presence = 1.0;
    if (u_intro.w > 0.0) {
        float k = saturate((u_intro.z - length(cell - u_intro.xy) * 0.85 - h1 * 0.22) / 0.75);
        float e = k * k * k * (k * (k * 6.0 - 15.0) + 10.0);
        flip = (1.0 - e) * 1.5707963;
        presence = e;
    }
    [unroll]
    for (int i = 0; i < 6; i++) {
        float4 w = u_waves[i];
        if (w.w <= 0.0) continue;
        float k = saturate(((uClock - w.z) * 0.95 - length(cell - w.xy)) / 0.2);
        flip += k * k * (3.0 - 2.0 * k) * 3.1415927;
    }

    float ca = cos(flip);
    float sa = sin(flip);
    float squash = max(abs(ca), 0.02);
    float half_ = 0.5 - 0.5 * uGap;
    float2 grad;
    float d = boxDist(float2(f.x / squash, f.y), half_, uRound * half_, grad);
    float aa = 1.2 / uTile;
    float aaD = lerp(aa, aa / squash, abs(grad.x));
    float cover = lerp((1.0 - ss(-aaD, aaD, d)) * ss(0.02, 0.08, abs(ca)), 1.0, solid);
    float alpha = cover * uOpacity;
    if (uFade > 0.0) {
        float2 uv = frag / uRes;
        float2 m = saturate(min(uv, 1.0 - uv) / max(uFade * 0.5, 1e-4));
        m = m * m * m * (m * (m * 6.0 - 15.0) + 10.0);
        alpha *= m.x * m.y;
    }
    if (alpha <= 0.002) return float4(0.0, 0.0, 0.0, 0.0);

    float edge = ss(-0.02 - 0.07 * uBevel, 0.0, d) * uBevel;
    float inner = ss(0.0, -0.09, d);
    float3 n = normalize(float3((float2(h2, h3) - 0.5) * (0.5 * uTilt) + grad * edge * 0.55, 1.0));
    n = float3(n.x * ca + n.z * sa, n.y, abs(n.z * ca - n.x * sa));
    float3 N = float3(uRot.x * n.x - uRot.y * n.y, uRot.y * n.x + uRot.x * n.y, n.z);
    float2 hv = cell - u_hover.xy;
    float hw = exp(-dot(hv, hv) / (u_hover.z * u_hover.z)) * u_hover.w;
    N = normalize(N + float3(-hv / u_hover.z * hw * 1.3, 0.0));
    float turn = abs(sin(flip));

    float t = uTime;
    float flow = vn3(float3(s * (1.4 * uSwirl) + seed * 0.013, t * 0.05));
    float2 rc = float2(0.15 + 0.22 * sin(t * 0.07), -0.05 + 0.16 * cos(t * 0.053));
    float rip = sin(length(s - rc) * 26.0 - t * 1.2) * 0.5 + 0.5;
    float phi = 6.2831853 * lerp(flow, h1, uScatter) + uRipple * 0.9 * rip;
    float3 T = float3(cos(phi), sin(phi), 0.0);
    T = normalize(T - dot(T, N) * N);
    float3 V = normalize(float3(-s, 1.15));
    float3 L = normalize(float3(uLight, 1.0));
    float D = 1450.0 * uBands;
    float3 sh = float3(0.0, 0.0, 0.0);
    [unroll]
    for (int k = 0; k < 3; k++) {
        float a = (float)k * 2.39996 + 0.7;
        float3 Lk = normalize(float3(uLight + float2(cos(a), sin(a)) * sqrt(((float)k + 0.5) / 3.0) * uSoft, 1.0));
        float x = abs(dot(Lk + V, T)) * D;
        sh += band(x) + 0.6 * band(x * 0.5);
    }
    sh *= presence * (0.62 + 0.38 * inner) / 3.0;
    float lum = dot(sh, float3(0.2126, 0.7152, 0.0722));
    sh = lerp((float3)lum, sh, uSaturation);
    sh = lerp(sh, (float3)(lum * 1.15 + 0.08 * presence), uSpectrum.z * 0.6);

    float3 H = normalize(L + V);
    float ndh = max(dot(N, H), 0.0);
    float wide = pow(ndh, 34.0);
    float tight = pow(ndh, 700.0);
    float2 spot = uLight * 1.15;
    float pool = pow(max(1.0 - length(s - spot) * 2.1, 0.0), 2.5) * uSheen;

    float spark = 0.0;
    float3 sparkColor = float3(1.0, 1.0, 1.0);
    if (uGlitter > 0.0) {
        float2 gq = frag / (uPixel * 6.0);
        float2 gid = floor(gq) + seed;
        float gh = h21(gid + 3.7);
        if (gh < 0.05 + 0.25 * uGlitter) {
            float2 off = float2(h21(gid + 1.3), h21(gid + 5.9)) - 0.5;
            float r = length(frac(gq) - 0.5 - off * 0.45);
            float3 gn = normalize(float3((float2(h21(gid + 9.1), h21(gid + 2.2)) - 0.5) * 0.7 + N.xy, 1.0));
            float twinkle = 0.55 + 0.45 * sin(t * (2.0 + 5.0 * gh) + gh * 70.0);
            spark = ss(0.991, 0.9995, dot(gn, H)) * (1.0 - ss(0.07, 0.24, r)) * twinkle * presence * uGlitter;
            sparkColor = spectral(420.0 + 260.0 * h21(gid + 7.7));
        }
    }

    float grainNoise = h31(float3(floor(frag / uPixel), uFrame)) - 0.5;
    float3 col;
    if (uLightMode > 0.5) {
        float top = max(max(sh.r, sh.g), sh.b);
        float ink = top * uIntensity;
        float3 hue = sh / max(top, 1e-3);
        float chroma = (top - min(min(sh.r, sh.g), sh.b)) / max(top, 1e-3);
        hue = saturate(lerp((float3)dot(hue, float3(0.2126, 0.7152, 0.0722)), hue, 1.35) * 0.92);
        hue = lerp(uMetal * 0.85, hue, ss(0.08, 0.45, chroma));
        float density = 0.86 * ss(0.0, 0.85, ink);
        float3 paper = uColor * (1.0 - 0.035 * (1.0 - wide) - 0.05 * edge) + uMetal * (0.08 * tight);
        col = lerp(paper, hue * lerp(float3(1.0, 1.0, 1.0), uColor, 0.15), density);
        col = lerp(col, sparkColor * 0.9, min(spark * 1.4, 0.9));
        col -= abs(grainNoise) * uGrain * 0.35 * saturate(density * 1.6 + spark);
        col = lerp(col, uColor, 0.12 * (1.0 - wide) * (1.0 - density) * pool);
    } else {
        float3 baseCol = uColor + float3(0.018, 0.018, 0.024);
        col = baseCol + uMetal * (0.025 + 0.07 * wide + 1.1 * tight + 0.2 * pool + 0.55 * turn * turn) * (0.75 + 0.25 * inner);
        col += sh * (1.65 * uIntensity);
        col += lerp(sparkColor, float3(1.0, 1.0, 1.0), 0.45) * spark * 2.4;
        col = col * (1.0 + col / 5.0) / (1.0 + col * 0.55);
        col += grainNoise * uGrain * 0.24;
    }
    col = saturate(col);
    return float4(col * alpha, alpha);
}

float bayer(float2 p) {
    int i = (int)glsl_mod(p.x, 4.0) + (int)glsl_mod(p.y, 4.0) * 4;
    static const int m[16] = { 0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5 };
    return ((float)m[clamp(i, 0, 15)] + 0.5) / 16.0;
}

float glyph(float level, float2 uv) {
    static const int codes[8] = { 0, 4194304, 131200, 14336, 459200, 145536, 703136, 11512810 };
    float2 g = floor(uv * 7.0) - 1.0;
    if (g.x < 0.0 || g.y < 0.0 || g.x > 4.0 || g.y > 4.0) return 0.0;
    int code = codes[(int)clamp(level * 8.0, 0.0, 7.0)];
    return (float)((code >> ((int)g.x + 5 * (int)(4.0 - g.y))) & 1);
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 uRes        = u_res_time.xy;
    float  uPixel      = u_res_time.z;
    float3 uColor      = u_color_grain.xyz;
    float  uLightMode  = u_light_op.w;
    float  uFilter     = u_misc.z;
    float  uFilterSize = u_misc.w;

    // Flip Y to match WebGL2 gl_FragCoord.xy (+Y up) 100% identically!
    float2 frag = float2(inp.pos.x, uRes.y - inp.pos.y);
    float4 outCol;

    if (uFilter > 1.5) {
        float size = max(4.0, floor(uPixel * 10.0 * uFilterSize + 0.5));
        float2 cellId = floor(frag / size);
        float4 sampled = shade((cellId + 0.5) * size, 1.0);
        if (sampled.a <= 0.002) {
            outCol = float4(0.0, 0.0, 0.0, 0.0);
        } else {
            float3 col = sampled.rgb / sampled.a;
            float ink = pow(saturate(length(col - uColor) * 1.25), 0.85);
            float on = glyph(ink, (frag - cellId * size) / size);
            float top = max(max(col.r, col.g), col.b);
            float3 tone = uLightMode > 0.5 ? col * 0.82 : lerp(col, col / max(top, 0.25), 0.4);
            float alpha = on * sampled.a;
            outCol = float4(saturate(tone) * alpha, alpha);
        }
    } else if (uFilter > 0.5) {
        float size = max(1.0, floor(uPixel * 2.0 * uFilterSize + 0.5));
        float2 cellId = floor(frag / size);
        float4 sampled = shade((cellId + 0.5) * size, 0.0);
        float threshold = bayer(cellId);
        float alpha = sampled.a >= 0.999 ? 1.0 : step(threshold, sampled.a);
        if (alpha <= 0.0) {
            outCol = float4(0.0, 0.0, 0.0, 0.0);
        } else {
            float3 col = sampled.rgb / max(sampled.a, 1e-3);
            float3 levels = saturate(uColor + floor((col - uColor) * 4.0 + threshold) / 4.0);
            outCol = float4(levels * alpha, alpha);
        }
    } else {
        outCol = shade(frag, 0.0);
    }

    // Composite premultiplied tile output over uColor (matching React page background showing through seams!)
    float3 finalRgb = saturate(outCol.rgb + uColor * (1.0 - outCol.a));
    return float4(finalRgb, 1.0);
}
"#;

type D3DCompileFn = unsafe extern "system" fn(
    p_src_data: *const c_void,
    src_data_size: usize,
    p_source_name: PCSTR,
    p_defines: *const c_void,
    p_include: *const c_void,
    p_entrypoint: PCSTR,
    p_target: PCSTR,
    flags1: u32,
    flags2: u32,
    pp_code: *mut Option<ID3DBlob>,
    pp_error_msgs: *mut Option<ID3DBlob>,
) -> windows::core::HRESULT;

fn compile_hlsl(entry: &[u8], target: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let dll_name = to_wide("d3dcompiler_47.dll");
        let hmod = LoadLibraryW(PCWSTR(dll_name.as_ptr())).map_err(|e| format!("LoadLibrary d3dcompiler_47: {e}"))?;
        let proc = GetProcAddress(hmod, PCSTR(b"D3DCompile\0".as_ptr()))
            .ok_or_else(|| "D3DCompile not found in d3dcompiler_47.dll".to_string())?;
        let d3d_compile: D3DCompileFn = std::mem::transmute(proc);

        let mut blob: Option<ID3DBlob> = None;
        let mut errs: Option<ID3DBlob> = None;
        let hr = d3d_compile(
            HLSL_SRC.as_ptr() as *const c_void,
            HLSL_SRC.len(),
            PCSTR(b"holo_mosaic.hlsl\0".as_ptr()),
            std::ptr::null(),
            std::ptr::null(),
            PCSTR(entry.as_ptr()),
            PCSTR(target.as_ptr()),
            1 << 15, // D3DCOMPILE_OPTIMIZATION_LEVEL3
            0,
            &mut blob,
            &mut errs,
        );
        if hr.is_err() || blob.is_none() {
            let msg = if let Some(eb) = errs {
                let ptr = eb.GetBufferPointer() as *const u8;
                let len = eb.GetBufferSize();
                if !ptr.is_null() && len > 0 {
                    String::from_utf8_lossy(std::slice::from_raw_parts(ptr, len)).to_string()
                } else {
                    format!("HRESULT {:?}", hr)
                }
            } else {
                format!("HRESULT {:?}", hr)
            };
            return Err(format!("HLSL compile error: {msg}"));
        }
        let b = blob.unwrap();
        let ptr = b.GetBufferPointer() as *const u8;
        let len = b.GetBufferSize();
        Ok(std::slice::from_raw_parts(ptr, len).to_vec())
    }
}

#[derive(Clone, Copy)]
struct FlipWave {
    x: f32,
    y: f32,
    start: f32,
    active: bool,
}

impl Default for FlipWave {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, start: -10.0, active: false }
    }
}

#[derive(Clone, Copy, Default)]
struct Spring2D {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    placed: bool,
}

impl Spring2D {
    fn step(&mut self, tx: f32, ty: f32, stiffness: f32, damping: f32, dt: f32) {
        self.vx += (stiffness * (tx - self.x) - damping * self.vx) * dt;
        self.vy += (stiffness * (ty - self.y) - damping * self.vy) * dt;
        self.x += self.vx * dt;
        self.y += self.vy * dt;
    }
}

fn rgb_norm(rgb: (u8, u8, u8)) -> [f32; 3] {
    [rgb.0 as f32 / 255.0, rgb.1 as f32 / 255.0, rgb.2 as f32 / 255.0]
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub struct GpuHoloRenderer {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    swap_chain: Option<IDXGISwapChain1>,
    offscreen_tex: Option<ID3D11Texture2D>,
    _dcomp: Option<IDCompositionDevice>,
    _target: Option<IDCompositionTarget>,
    _visual: Option<IDCompositionVisual>,
    rtv: ID3D11RenderTargetView,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    cb: ID3D11Buffer,
    rs: ID3D11RasterizerState,

    pub width: u32,
    pub height: u32,
    pixel_ratio: f32,

    // Smoothly interpolated live preset parameters
    cur_base: [f32; 3],
    target_base: [f32; 3],
    cur_metal: [f32; 3],
    target_metal: [f32; 3],
    cur_spectrum: [f32; 3],
    target_spectrum: [f32; 3],
    cur_sat: f32,
    target_sat: f32,
    cur_intensity: f32,
    target_intensity: f32,
    cur_glitter: f32,
    target_glitter: f32,
    cur_sheen: f32,
    target_sheen: f32,
    cur_speed: f32,
    target_speed: f32,

    preset: HoloPreset,
    filter_code: f32,
    filter_size: f32,

    // Exact React Bits simulation state
    pub time: f32,
    pub clock: f32,
    pub intro: f32,
    frame_counter: u32,
    light: Spring2D,
    hover: Spring2D,
    hover_presence: f32,
    follow: f32,
    waves: [FlipWave; MAX_WAVES],
    next_wave: usize,
}

impl GpuHoloRenderer {
    fn create_device() -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
        unsafe {
            let mut dev = None;
            let mut ctx = None;
            let res = D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut dev),
                None,
                Some(&mut ctx),
            );
            if res.is_err() || dev.is_none() {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut dev),
                    None,
                    Some(&mut ctx),
                )?;
            }
            Ok((dev.unwrap(), ctx.unwrap()))
        }
    }

    pub fn new_for_hwnd(
        hwnd: HWND,
        width: u32,
        height: u32,
        preset: &HoloPreset,
        speed_override: Option<f32>,
        sat_override: Option<f32>,
        filter_name: &str,
    ) -> Result<Self, String> {
        let w = width.max(64);
        let h = height.max(64);
        unsafe {
            let (device, ctx) = Self::create_device().map_err(|e| format!("D3D11CreateDevice: {e}"))?;
            let dxgi_dev: IDXGIDevice = device.cast().map_err(|e| format!("cast IDXGIDevice: {e}"))?;
            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
                .map_err(|e| format!("CreateDXGIFactory2: {e}"))?;
            let sc_desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: w,
                Height: h,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                ..Default::default()
            };
            let swap_chain = factory
                .CreateSwapChainForComposition(&device, &sc_desc, None)
                .map_err(|e| format!("CreateSwapChainForComposition: {e}"))?;
            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_dev)
                .map_err(|e| format!("DCompositionCreateDevice: {e}"))?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true).map_err(|e| format!("CreateTargetForHwnd: {e}"))?;
            let visual = dcomp.CreateVisual().map_err(|e| format!("CreateVisual: {e}"))?;
            visual.SetContent(&swap_chain).map_err(|e| format!("SetContent: {e}"))?;
            target.SetRoot(&visual).map_err(|e| format!("SetRoot: {e}"))?;
            dcomp.Commit().map_err(|e| format!("dcomp.Commit: {e}"))?;

            let backbuf: ID3D11Texture2D = swap_chain.GetBuffer(0).map_err(|e| format!("GetBuffer: {e}"))?;
            let mut rtv = None;
            device.CreateRenderTargetView(&backbuf, None, Some(&mut rtv)).map_err(|e| format!("CreateRTV: {e}"))?;

            Self::finish_init(
                device,
                ctx,
                Some(swap_chain),
                None,
                Some(dcomp),
                Some(target),
                Some(visual),
                rtv.unwrap(),
                w,
                h,
                preset,
                speed_override,
                sat_override,
                filter_name,
            )
        }
    }

    pub fn new_offscreen(
        width: u32,
        height: u32,
        preset: &HoloPreset,
        speed_override: Option<f32>,
        sat_override: Option<f32>,
        filter_name: &str,
    ) -> Result<Self, String> {
        let w = width.max(64);
        let h = height.max(64);
        unsafe {
            let (device, ctx) = Self::create_device().map_err(|e| format!("D3D11CreateDevice: {e}"))?;
            let td = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..Default::default()
            };
            let mut tex = None;
            device.CreateTexture2D(&td, None, Some(&mut tex)).map_err(|e| format!("CreateTexture2D: {e}"))?;
            let tex = tex.unwrap();
            let mut rtv = None;
            device.CreateRenderTargetView(&tex, None, Some(&mut rtv)).map_err(|e| format!("CreateRTV: {e}"))?;
            let mut r = Self::finish_init(
                device,
                ctx,
                None,
                Some(tex),
                None,
                None,
                None,
                rtv.unwrap(),
                w,
                h,
                preset,
                speed_override,
                sat_override,
                filter_name,
            )?;
            r.intro = INTRO_SECONDS;
            r.clock = 5.0;
            r.time = 2.5;
            Ok(r)
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_init(
        device: ID3D11Device,
        ctx: ID3D11DeviceContext,
        swap_chain: Option<IDXGISwapChain1>,
        offscreen_tex: Option<ID3D11Texture2D>,
        dcomp: Option<IDCompositionDevice>,
        target: Option<IDCompositionTarget>,
        visual: Option<IDCompositionVisual>,
        rtv: ID3D11RenderTargetView,
        w: u32,
        h: u32,
        preset: &HoloPreset,
        speed_override: Option<f32>,
        sat_override: Option<f32>,
        filter_name: &str,
    ) -> Result<Self, String> {
        unsafe {
            let vs_bc = compile_hlsl(b"VSMain\0", b"vs_5_0\0")?;
            let ps_bc = compile_hlsl(b"PSMain\0", b"ps_5_0\0")?;
            let mut vs = None;
            device.CreateVertexShader(&vs_bc, None, Some(&mut vs)).map_err(|e| format!("CreateVS: {e}"))?;
            let mut ps = None;
            device.CreatePixelShader(&ps_bc, None, Some(&mut ps)).map_err(|e| format!("CreatePS: {e}"))?;

            let cb_desc = D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<HoloConstants>() as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            let mut cb = None;
            device.CreateBuffer(&cb_desc, None, Some(&mut cb)).map_err(|e| format!("CreateCB: {e}"))?;

            let rs_desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                DepthClipEnable: BOOL(0),
                ..Default::default()
            };
            let mut rs = None;
            device.CreateRasterizerState(&rs_desc, Some(&mut rs)).map_err(|e| format!("CreateRS: {e}"))?;

            let base = rgb_norm(preset.base_color);
            let metal = rgb_norm(preset.metal_color);
            let sat = sat_override.unwrap_or(preset.saturation).clamp(0.0, 1.5);
            let spd = speed_override.unwrap_or(preset.speed).clamp(0.0, 2.0);
            let start_time = 17.0 + (preset.variation % 97.0) * 3.1;
            let aspect = w as f32 / (h.max(1) as f32);
            let home_x = 0.46 * aspect * (start_time * 0.29 + 0.4).cos();
            let home_y = 0.36 * (start_time * 0.41 + 1.1).sin();

            Ok(Self {
                device,
                ctx,
                swap_chain,
                offscreen_tex,
                _dcomp: dcomp,
                _target: target,
                _visual: visual,
                rtv,
                vs: vs.unwrap(),
                ps: ps.unwrap(),
                cb: cb.unwrap(),
                rs: rs.unwrap(),
                width: w,
                height: h,
                // Scale tile density relative to 1080p so tiles feel identical to React Bits at any display scale!
                pixel_ratio: (h as f32 / 900.0).clamp(0.85, 2.0),
                cur_base: base,
                target_base: base,
                cur_metal: metal,
                target_metal: metal,
                cur_spectrum: preset.spectrum,
                target_spectrum: preset.spectrum,
                cur_sat: sat,
                target_sat: sat,
                cur_intensity: preset.intensity,
                target_intensity: preset.intensity,
                cur_glitter: preset.glitter,
                target_glitter: preset.glitter,
                cur_sheen: preset.sheen,
                target_sheen: preset.sheen,
                cur_speed: spd,
                target_speed: spd,
                preset: *preset,
                filter_code: holo_mosaic::filter_code(filter_name),
                filter_size: 1.0,
                time: start_time,
                clock: 0.0,
                intro: 0.0,
                frame_counter: 0,
                light: Spring2D { x: home_x, y: home_y, vx: 0.0, vy: 0.0, placed: true },
                hover: Spring2D::default(),
                hover_presence: 0.0,
                follow: 0.0,
                waves: [FlipWave::default(); MAX_WAVES],
                next_wave: 0,
            })
        }
    }

    pub fn set_target_preset(
        &mut self,
        preset: &HoloPreset,
        speed_override: Option<f32>,
        sat_override: Option<f32>,
        filter_name: &str,
    ) {
        let changed_preset = self.preset.key != preset.key;
        self.preset = *preset;
        self.target_base = rgb_norm(preset.base_color);
        self.target_metal = rgb_norm(preset.metal_color);
        self.target_spectrum = preset.spectrum;
        self.target_sat = sat_override.unwrap_or(preset.saturation).clamp(0.0, 1.5);
        self.target_intensity = preset.intensity;
        self.target_glitter = preset.glitter;
        self.target_sheen = preset.sheen;
        self.target_speed = speed_override.unwrap_or(preset.speed).clamp(0.0, 2.0);
        self.filter_code = holo_mosaic::filter_code(filter_name);
        // Trigger a subtle center flip wave when switching between HoloMosaic presets!
        if changed_preset {
            self.spawn_click_wave(0.5, 0.5);
        }
    }

    fn aspect(&self) -> f32 {
        self.width as f32 / (self.height.max(1) as f32)
    }

    /// Converts screen UV (`0..1`, top-left origin) to React Bits surface coords (centered, `+Y` up).
    fn to_surface(&self, u: f32, v: f32) -> (f32, f32) {
        let a = self.aspect();
        ((u - 0.5) * a, 0.5 - v)
    }

    fn wander(&self) -> (f32, f32) {
        let t = self.time;
        (
            0.46 * self.aspect() * (t * 0.29 + 0.4).cos(),
            0.36 * (t * 0.41 + 1.1).sin(),
        )
    }

    /// Updates cursor light-follow and local tile-tilt springs (1:1 React Bits spring math!).
    pub fn update_mouse(&mut self, u: f32, v: f32, on_desktop: bool, dt: f32) {
        let dt = dt.clamp(0.001, 0.05);
        let active = on_desktop;
        let follow_tau = if active { 0.45 } else { 1.2 };
        self.follow += ((if active { 1.0 } else { 0.0 }) - self.follow) * (1.0 - (-dt / follow_tau).exp());

        let home = self.wander();
        let target = if active { Some(self.to_surface(u, v)) } else { None };
        let goal = match target {
            Some((tx, ty)) => (
                home.0 + (tx - home.0) * self.follow,
                home.1 + (ty - home.1) * self.follow,
            ),
            None => home,
        };
        self.light.step(goal.0, goal.1, 38.0, 11.0, dt);

        if let Some((tx, ty)) = target {
            if !self.hover.placed {
                self.hover = Spring2D { x: tx, y: ty, vx: 0.0, vy: 0.0, placed: true };
            }
            self.hover.step(tx, ty, 90.0, 18.0, dt);
        }
        let h_goal = if target.is_some() { 1.0 } else { 0.0 };
        let h_tau = if h_goal > self.hover_presence { 0.3 } else { 0.6 };
        self.hover_presence += (h_goal - self.hover_presence) * (1.0 - (-dt / h_tau).exp());
        if target.is_none() && self.hover_presence < 0.001 {
            self.hover.placed = false;
        }
    }

    /// Spawns a radial 3D tile-flip wave at screen UV (`0..1`, top-left origin) — 1:1 `flipOnClick`!
    pub fn spawn_click_wave(&mut self, u: f32, v: f32) {
        let (sx, sy) = self.to_surface(u, v);
        self.waves[self.next_wave] = FlipWave {
            x: sx,
            y: sy,
            start: self.clock,
            active: true,
        };
        self.next_wave = (self.next_wave + 1) % MAX_WAVES;
    }

    pub fn render_frame(&mut self, dt: f32, sync_interval: u32) {
        let dt = dt.clamp(0.0, 0.05);
        let k = (dt * 6.5).min(1.0);
        self.cur_base = lerp3(self.cur_base, self.target_base, k);
        self.cur_metal = lerp3(self.cur_metal, self.target_metal, k);
        self.cur_spectrum = lerp3(self.cur_spectrum, self.target_spectrum, k);
        self.cur_sat += (self.target_sat - self.cur_sat) * k;
        self.cur_intensity += (self.target_intensity - self.cur_intensity) * k;
        self.cur_glitter += (self.target_glitter - self.cur_glitter) * k;
        self.cur_sheen += (self.target_sheen - self.cur_sheen) * k;
        self.cur_speed += (self.target_speed - self.cur_speed) * k;

        if self.cur_speed > 0.001 || self.intro < INTRO_SECONDS || self.waves.iter().any(|w| w.active) {
            self.frame_counter = self.frame_counter.wrapping_add(1);
            self.clock += dt;
            if self.intro < INTRO_SECONDS {
                self.intro = (self.intro + dt).min(INTRO_SECONDS);
            }
            self.time += dt * self.cur_speed;
            for w in &mut self.waves {
                if w.active && (self.clock - w.start) >= WAVE_SECONDS {
                    w.active = false;
                }
            }
        }

        let mut wave_data = [[0.0f32; 4]; MAX_WAVES];
        for (i, w) in self.waves.iter().enumerate() {
            if w.active {
                wave_data[i] = [w.x, w.y, w.start, 1.0];
            }
        }

        let rot_rad = self.preset.rotation_deg.to_radians();
        let intro_live = if self.intro < INTRO_SECONDS { 1.0 } else { 0.0 };

        let cb_data = HoloConstants {
            u_res_time: [self.width as f32, self.height as f32, self.pixel_ratio, self.time],
            u_clock_tile_rot: [
                self.clock,
                TILE_PX * self.preset.tile_size * self.pixel_ratio,
                rot_rad.cos(),
                rot_rad.sin(),
            ],
            u_geom: [self.preset.gap, self.preset.roundness, self.preset.bevel, self.preset.scatter],
            u_pattern: [self.preset.tilt, self.preset.swirl, self.preset.ripple, self.preset.bands],
            u_light_props: [
                0.02 + 0.2 * self.preset.softness,
                self.cur_intensity,
                self.cur_sat,
                self.cur_glitter,
            ],
            u_spectrum_sheen: [self.cur_spectrum[0], self.cur_spectrum[1], self.cur_spectrum[2], self.cur_sheen],
            u_color_grain: [self.cur_base[0], self.cur_base[1], self.cur_base[2], self.preset.grain],
            u_metal_fade: [self.cur_metal[0], self.cur_metal[1], self.cur_metal[2], 0.0],
            u_light_op: [
                self.light.x / EYE,
                self.light.y / EYE,
                1.0,
                if self.preset.light_mode { 1.0 } else { 0.0 },
            ],
            u_hover: [self.hover.x, self.hover.y, 0.25, 0.8 * self.hover_presence],
            u_intro: [0.0, 0.0, self.intro, intro_live],
            u_misc: [
                (self.preset.variation % 1000.0) * 0.618,
                (self.frame_counter % 613) as f32,
                self.filter_code,
                self.filter_size,
            ],
            u_waves: wave_data,
        };

        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if self.ctx.Map(&self.cb, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped)).is_ok() {
                std::ptr::copy_nonoverlapping(&cb_data as *const HoloConstants, mapped.pData as *mut HoloConstants, 1);
                self.ctx.Unmap(&self.cb, 0);
            }

            let vp = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.ctx.RSSetViewports(Some(&[vp]));
            self.ctx.RSSetState(&self.rs);
            self.ctx.OMSetRenderTargets(Some(&[Some(self.rtv.clone())]), None);
            self.ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.ctx.VSSetShader(&self.vs, None);
            self.ctx.PSSetShader(&self.ps, None);
            self.ctx.PSSetConstantBuffers(0, Some(&[Some(self.cb.clone())]));
            self.ctx.Draw(3, 0);

            if let Some(sc) = &self.swap_chain {
                let _ = sc.Present(sync_interval, windows::Win32::Graphics::Dxgi::DXGI_PRESENT(0));
            }
        }
    }

    pub fn trim_gpu(&self) {
        unsafe {
            if let Ok(dxgi3) = self.device.cast::<IDXGIDevice3>() {
                dxgi3.Trim();
            }
        }
    }

    /// Reads back the offscreen GPU render target and encodes a lossless PNG for visual QA.
    pub fn save_offscreen_png(&self, out_path: &Path) -> Result<(), String> {
        let Some(src_tex) = &self.offscreen_tex else {
            return Err("renderer is not offscreen".into());
        };
        unsafe {
            let sd = D3D11_TEXTURE2D_DESC {
                Width: self.width,
                Height: self.height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..Default::default()
            };
            let mut staging = None;
            self.device.CreateTexture2D(&sd, None, Some(&mut staging)).map_err(|e| format!("CreateStaging: {e}"))?;
            let staging = staging.unwrap();
            self.ctx.CopyResource(&staging, src_tex);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).map_err(|e| format!("Map staging: {e}"))?;
            let row_bytes = (self.width * 4) as usize;
            let mut bgra = vec![0u8; row_bytes * self.height as usize];
            for y in 0..self.height as usize {
                let src = (mapped.pData as *const u8).add(y * mapped.RowPitch as usize);
                std::ptr::copy_nonoverlapping(src, bgra.as_mut_ptr().add(y * row_bytes), row_bytes);
            }
            self.ctx.Unmap(&staging, 0);

            crate::waves_gpu::save_bgra_png(self.width, self.height, &bgra, out_path)
        }
    }
}
