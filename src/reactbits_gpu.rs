//! AuraOS v5.6 — Native D3D11 HLSL 5.0 GPU Renderer for All 16 Free React Bits Backgrounds
//!
//! Runs 100% isolated inside `AuraWallpaper.exe` on its own D3D11 device + DirectComposition
//! swapchain (0.00% CPU/GPU load on `AuraShell.exe`). Compiles only the active shader via
//! Windows' built-in `d3dcompiler_47.dll` (~12 ms), supports instant live switching between all
//! 16 shaders via `switch_preset()`, real-time desktop cursor interactivity, up to 6 click
//! waves/flares/twirls, offscreen PNG capture for QA, and `verify_all_shaders()` for CI/QA.

use crate::reactbits_shaders::{self, ReactBitsPreset};
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

pub const MAX_CLICKS: usize = 6;
const CLICK_LIFE: f32 = 3.5;

#[repr(C)]
#[derive(Clone, Copy)]
struct RbConstants {
    /// xy = width, height; z = time (speed-scaled); w = clock (raw seconds)
    u_res_time: [f32; 4],
    /// x = mouse_u (0..1), y = mouse_v (0..1 bottom-up), z = presence (0..1), w = aspect (w/h)
    u_mouse: [f32; 4],
    /// x = vel_x, y = vel_y, z = intro (0..1), w = click_warp_energy
    u_vel_intro: [f32; 4],
    /// xy = click_uv (0..1 bottom-up), z = age_sec, w = active (1 or 0)
    u_clicks: [[f32; 4]; MAX_CLICKS],
    /// Palette colors (0..1 linear/sRGB per shader)
    u_colors: [[f32; 4]; 4],
}

const RB_COMMON_HLSL: &str = r#"
cbuffer RbCB : register(b0) {
    float4 u_res_time;
    float4 u_mouse;
    float4 u_vel_intro;
    float4 u_clicks[6];
    float4 u_colors[4];
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

float sss(float e0, float e1, float x) {
    float t = saturate((x - e0) / (e1 - e0));
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

float glsl_mod(float x, float y) {
    return x - y * floor(x / y);
}

float2 glsl_mod2(float2 x, float y) {
    return x - y * floor(x / y);
}

float3 glsl_mod3(float3 x, float y) {
    return x - y * floor(x / y);
}

float4 glsl_mod4(float4 x, float y) {
    return x - y * floor(x / y);
}

float2 rot2(float2 v, float a) {
    float c = cos(a);
    float s = sin(a);
    return float2(c * v.x - s * v.y, s * v.x + c * v.y);
}

float hash21(float2 p) {
    p = frac(p * float2(123.34, 456.21));
    p += dot(p, p + 45.32);
    return frac(p.x * p.y);
}

float4 mv4(float4 c0, float4 c1, float4 c2, float4 c3, float4 v) {
    return c0 * v.x + c1 * v.y + c2 * v.z + c3 * v.w;
}

float3 oklabToLinear(float3 c) {
    float l = c.x + 0.3963377774 * c.y + 0.2158037573 * c.z;
    float m = c.x - 0.1055613458 * c.y - 0.0638541728 * c.z;
    float s = c.x - 0.0894841775 * c.y - 1.2914855480 * c.z;
    l = l * l * l;
    m = m * m * m;
    s = s * s * s;
    return float3(
         4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
        -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
        -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s
    );
}

float3 toSrgb(float3 c) {
    c = saturate(c);
    float3 hi = 1.055 * pow(max(c, 1e-6), 1.0 / 2.4) - 0.055;
    return lerp(c * 12.92, hi, step(0.0031308, c));
}
"#;

// 1. AURORA (React Bits OKLCH 3-layer Simplex Northern Lights)
const SHADER_AURORA: &str = r#"
float3 permute3(float3 x) { return glsl_mod3(((x * 34.0) + 1.0) * x, 289.0); }

float snoise(float2 v) {
    const float4 C = float4(0.211324865405187, 0.366025403784439, -0.577350269189626, 0.024390243902439);
    float2 i = floor(v + dot(v, C.yy));
    float2 x0 = v - i + dot(i, C.xx);
    float2 i1 = (x0.x > x0.y) ? float2(1.0, 0.0) : float2(0.0, 1.0);
    float4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = glsl_mod2(i, 289.0);
    float3 p = permute3(permute3(i.y + float3(0.0, i1.y, 1.0)) + i.x + float3(0.0, i1.x, 1.0));
    float3 m = max(0.5 - float3(dot(x0, x0), dot(x12.xy, x12.xy), dot(x12.zw, x12.zw)), 0.0);
    m = m * m;
    m = m * m;
    float3 x = 2.0 * frac(p * C.www) - 1.0;
    float3 h = abs(x) - 0.5;
    float3 ox = floor(x + 0.5);
    float3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * (a0 * a0 + h * h);
    float3 g;
    g.x = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}

float3 auroraRamp(float t) {
    // OKLCH stops for React Bits ['#5227FF', '#7cff67', '#00d4ff', '#5227FF'] (unwrapped cleanly!)
    float3 s0 = float3(0.51, 0.29,  4.94);
    float3 s1 = float3(0.88, 0.25,  2.48);
    float3 s2 = float3(0.81, 0.16,  3.75);
    float3 s3 = float3(0.51, 0.29,  4.94);
    float x = saturate(t) * 3.0;
    float3 lch;
    if (x < 1.0) {
        float f = x * x * (3.0 - 2.0 * x);
        lch = lerp(s0, s1, f);
    } else if (x < 2.0) {
        float f = x - 1.0; f = f * f * (3.0 - 2.0 * f);
        lch = lerp(s1, s2, f);
    } else {
        float f = x - 2.0; f = f * f * (3.0 - 2.0 * f);
        lch = lerp(s2, s3, f);
    }
    return toSrgb(oklabToLinear(float3(lch.x, lch.y * cos(lch.z), lch.y * sin(lch.z))));
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float y = 1.0 - frag.y / res.y;
    float u = frag.x / res.x;
    float X = (frag.x - res.x * 0.5) / res.y;
    float t = u_res_time.z;
    float px = (u_mouse.x - 0.5) * u_mouse.w;
    float reach = u_mouse.z * exp(-pow((X - px) / 0.38, 2.0));

    float surge = 0.0;
    [unroll]
    for (int c = 0; c < 6; c++) {
        if (u_clicks[c].w > 0.5) {
            float age = u_clicks[c].z;
            float cx = (u_clicks[c].x - 0.5) * u_mouse.w;
            float dx = abs(X - cx) - age * 0.65;
            surge += exp(-dx * dx * 18.0) * exp(-age * 1.4);
        }
    }

    float3 glow = float3(0.0, 0.0, 0.0);
    float cover = 0.0;
    float reveal = u_vel_intro.z;
    [unroll]
    for (int k = 0; k < 3; k++) {
        float layer = (float)k;
        float seed = layer * 13.7;
        float pace = t * (1.0 - layer * 0.18);
        float warp = snoise(float2(X * 0.45 + pace * 0.02 + seed, pace * 0.05 + seed));
        float wave = snoise(float2(X * 0.75 + warp * 0.35 - pace * 0.03 + seed, pace * 0.07 - seed)) * 0.72
                   + snoise(float2(X * 1.60 + warp * 0.50 + pace * 0.04 - seed, pace * 0.10 + seed * 0.5)) * 0.28;
        float hem = 0.50 + layer * 0.08 + wave * 0.17 + (reach + surge * 0.8) * 0.09 * (k == 0 ? 1.0 : 0.45);
        hem = lerp(-0.25, hem, reveal);

        float soft = 0.12 * (1.0 + layer * 0.85);
        float rise = hem - y;
        float low = -soft - 0.085;
        float high = 0.26 + soft * 0.6;
        float body = pow(max(sss(low, high, rise), 0.0), 1.85);
        float lobes = 0.86 + 0.14 * snoise(float2(X * 0.9 - pace * 0.025 + seed, y * 0.3 + pace * 0.04));
        float alpha = body * lobes * (k == 0 ? 1.0 : 0.36 / layer);
        alpha = saturate(alpha * (1.0 + (reach + surge) * 0.30 * sss(0.9, 0.0, abs(y - hem))));

        float3 tint = auroraRamp(u + 0.05 * sin(pace * 0.08 + layer * 2.3) + layer * 0.1 + max(rise, 0.0) * 0.05);
        glow = 1.0 - (1.0 - glow) * (1.0 - tint * alpha);
        cover = 1.0 - (1.0 - cover) * (1.0 - alpha);
    }

    float3 sky = lerp(float3(0.020, 0.024, 0.048), float3(0.040, 0.045, 0.090), 1.0 - y);
    float dither = (hash21(frag) - 0.5) / 255.0;
    float3 col = saturate(sky * (1.0 - cover * 0.75) + glow * 1.10 + dither);
    return float4(col, 1.0);
}
"#;

// 2. SILK (React Bits Woven Liquid Silk + Specular Sheen + Click Ripples)
const SHADER_SILK: &str = r#"
float silkNoise(float2 tc) {
    const float G = 2.718281828459;
    float2 r = G * sin(G * tc);
    return frac(r.x * r.y * (1.0 + tc.x));
}

float2 silkBend(float2 uv) {
    float2 aspect = float2(u_mouse.w, 1.0);
    float2 off = (uv - u_mouse.xy) * aspect;
    float touch = u_mouse.z * exp(-dot(off, off) / (0.32 * 0.32));
    uv -= float2(u_vel_intro.x, -u_vel_intro.y) * touch * 0.06;
    [unroll]
    for (int i = 0; i < 4; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        float amp = exp(-age / 0.72);
        float2 q = (uv - u_clicks[i].xy) * aspect;
        float dist = length(q);
        float front = dist - age * 0.6;
        float ring = exp(-front * front * 60.0) * amp;
        uv += (q / max(dist, 0.0001)) / aspect * ring * 0.036 * sin(front * 34.0);
    }
    return uv;
}

float silkField(float2 vUv) {
    float2 tex = vUv * 1.15;
    float tOffset = u_res_time.z * 0.5;
    tex.y += 0.03 * sin(8.0 * tex.x - tOffset);
    float unfold = 0.25 + 0.75 * u_vel_intro.z;
    return 0.6 + 0.4 * unfold * sin(
        5.0 * (tex.x + tex.y + cos(3.0 * tex.x + 5.0 * tex.y) + 0.02 * tOffset)
        + sin(20.0 * (tex.x + tex.y - 0.1 * tOffset))
    );
}

float3 softClip(float3 c) {
    float3 over = max(c - 0.75, 0.0);
    return min(c, 0.75) + 0.25 * (1.0 - exp(-over * 4.0));
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 vUv = frag / res;
    float2 coord = silkBend(vUv);
    float pattern = silkField(coord);
    float rnd = silkNoise(frag);
    float n = saturate((pattern - 0.2) / 0.8);

    float2 stepUv = 1.5 / res;
    float gx = silkField(coord + float2(stepUv.x, 0.0)) - pattern;
    float gy = silkField(coord + float2(0.0, stepUv.y)) - pattern;
    float3 normal = normalize(float3(-gx * 26.0, -gy * 26.0, 1.0));
    float2 lightAim = lerp(float2(-0.55, 0.70), (u_mouse.xy - 0.5) * float2(u_mouse.w, 1.0) * 1.6, u_mouse.z * 0.75);
    float3 toLight = normalize(float3(lightAim, 0.85));
    float relief = 1.0 + (dot(normal, toLight) - toLight.z) * 1.8 * 0.55;
    float3 halfway = normalize(toLight + float3(0.0, 0.0, 1.0));
    float gloss = pow(max(dot(normal, halfway), 0.0), 48.0) * ss(0.25, 0.85, n);
    float crest = pow(max(n, 0.0), 5.0);
    float shine = (gloss * 0.85 + crest * 0.42) * 0.65 * (1.0 + u_mouse.z * 0.6);

    float3 baseColor = u_colors[0].rgb;
    float3 sheenCol = u_colors[1].rgb;
    float grain = rnd / 15.0 * 1.25;
    float floorVal = lerp(0.2, 0.03, 0.55);
    float curve = pow(max(n, 0.0), 1.0 + 1.4 * 0.55);
    float shade = floorVal + (1.0 - floorVal) * curve;
    float3 tone = lerp(baseColor, pow(max(baseColor, 1e-4), 1.45), (1.0 - n) * 0.55);
    float3 lit = tone * shade * relief - grain;
    float3 tint = lerp(baseColor, sheenCol, 0.8);
    float3 extra = tint * (shine + pow(max(n, 0.0), 4.0) * 0.16);
    float3 result = saturate(softClip(lit + extra));
    return float4(result, 1.0);
}
"#;

// 3. IRIDESCENCE (React Bits Interference Liquid + Click Vortices + Pointer Sheen)
const SHADER_IRIDESCENCE: &str = r#"
float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 vUv = frag / res;
    float mr = min(res.x, res.y);
    float2 aspect = res / mr;
    float2 st = vUv;

    // Velocity stir around cursor
    float2 md = (st - u_mouse.xy) * aspect;
    float stirW = u_mouse.z * exp(-dot(md, md) / 0.06);
    st -= float2(u_vel_intro.x, -u_vel_intro.y) * stirW * 0.08;

    // 6 Click vortices
    [unroll]
    for (int k = 0; k < 6; k++) {
        if (u_clicks[k].w <= 0.5) continue;
        float age = u_clicks[k].z;
        if (age >= 3.2) continue;
        float radius = 0.21 * (1.0 + 0.4 * age);
        float ease = exp(-age * 1.35) - exp(-age * 10.0);
        float fade = min(1.0, (3.2 - age) / 0.8);
        float strength = 3.0 * ease * fade * pow(0.21 / radius, 2.0);
        float2 delta = (st - u_clicks[k].xy) * aspect;
        float angle = strength * exp(-dot(delta, delta) / (radius * radius));
        float ca = cos(angle);
        float sa = sin(angle);
        st = u_clicks[k].xy + float2(ca * delta.x - sa * delta.y, sa * delta.x + ca * delta.y) / aspect;
    }

    float2 uv = (st * 2.0 - 1.0) * aspect;
    uv += (u_mouse.xy - 0.5) * 0.16 * u_mouse.z;

    float d = -u_res_time.z * 0.5;
    float a = 0.0;
    [unroll]
    for (int i = 0; i < 8; i++) {
        float fi = (float)i;
        a += cos(fi - d - a * uv.x);
        d += sin(uv.y * fi + a);
    }
    d += u_res_time.z * 0.5;

    float3 col = float3(cos(uv * float2(d, a)) * 0.6 + 0.4, cos(a + d) * 0.5 + 0.5);
    col = cos(col * cos(float3(d, a, 2.5)) * 0.5 + 0.5) * float3(0.95, 0.96, 1.0);
    float2 toPtr = (vUv - u_mouse.xy) * aspect;
    col += col * u_mouse.z * 0.35 * exp(-dot(toPtr, toPtr) / 0.09);

    col = (col - 0.5) * 1.05 + 0.5;
    col *= 1.18;
    col += (hash21(frag + u_res_time.w) - 0.5) * 0.015;
    return float4(saturate(col), 1.0);
}
"#;

// 4. LIQUID CHROME (React Bits Multi-Octave Jacobian Curl + Analytical Studio Reflections + ACES)
const SHADER_LIQUID_CHROME: &str = r#"
float lamp(float2 p, float2 center, float2 size, float soft) {
    float2 grown = size + soft;
    float2 q = (p - center) / grown;
    q *= q;
    float d = sqrt(sqrt(dot(q, q)));
    float edge = soft / min(grown.x, grown.y);
    float shape = (1.0 - ss(1.0 - edge, 1.0 + edge, d)) * (1.0 - 0.45 * d * d);
    return shape * size.x * size.y / (grown.x * grown.y);
}

float3 studioEnv(float3 r, float blur, float2 shift) {
    float2 p = r.xy / max(r.z + 1.0, 0.05);
    p += shift * ss(0.3, 1.0, length(p));
    float radius = length(p);
    float soft = 0.008 + blur;
    float2 dir = p / max(radius, 1e-3);
    float key = lamp(p, float2(-0.25, 0.42), float2(0.30, 0.12), soft);
    float side = lamp(p, float2(0.48, 0.12), float2(0.06, 0.30), soft);
    float ground = lamp(p, float2(-0.10, -0.55), float2(0.45, 0.06), soft);
    float rise = ss(-1.0, 1.0, dir.y);
    float width = 0.015 + soft * 2.0;
    float rim = exp(-abs(radius - 1.0) / width) * 0.031 / width * (0.8 + ss(-0.5, 0.8, p.y));
    float band = 1.0 - 0.95 * ss(1.04 - soft, 1.12 + soft, radius);
    float wall = ss(1.25, 1.55, radius) * (0.6 + 0.4 * rise);
    float streak = ss(0.2, 0.8, 0.5 + 0.5 * sin(atan2(dir.y, dir.x) * 3.0 + 0.6));
    float3 color = float3(0.005, 0.006, 0.009) + 0.024 * ss(-0.4, 0.6, p.y);
    color += float3(1.0, 0.97, 0.94) * key * 4.0 + float3(0.90, 0.95, 1.0) * side * 3.0 + ground * 1.2;
    color += float3(1.0, 0.98, 0.95) * rim;
    color *= band;
    return color + lerp(0.45, 1.8, streak) * wall;
}

float3 shadeChrome(float2 slope, float3 v, float blur, float3 f0, float2 shift) {
    float steep = length(slope);
    slope *= 3.0 * tanh(steep / 3.0) / max(steep, 1e-6);
    float3 n = normalize(float3(-slope, 1.0));
    float3 r = reflect(v, n);
    float facing = saturate(dot(n, -v));
    return studioEnv(r, blur, shift) * (f0 + (1.0 - f0) * pow(1.0 - facing, 5.0));
}

float3 aces(float3 x) {
    return saturate((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14));
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float unit = min(res.x, res.y);
    float2 uv = (2.0 * frag - res) / unit;
    float2 p = uv;
    float2 rowX = float2(1.0, 0.0);
    float2 rowY = float2(0.0, 1.0);
    float t = u_res_time.z + 12.0;
    float amp = 0.32 * u_vel_intro.z;

    [unroll]
    for (int ii = 1; ii <= 4; ii++) {
        float i = (float)ii;
        float a = amp / i;
        float soften = exp(0.3 - 0.3 * i);
        float argX = i * 3.0 * p.y + t;
        p.x += a * cos(argX);
        rowX -= a * sin(argX) * i * 3.0 * soften * rowY;
        float argY = i * 3.0 * p.x + t;
        p.y += a * cos(argY);
        rowY -= a * sin(argY) * i * 3.0 * soften * rowX;
    }

    float phase = t - p.x - p.y;
    float2 field = -(rowX + rowY);
    float density = max(length(field), 1e-3);
    float2 across = field / density;
    float k = 1.6667;
    float sn = sin(phase);
    float cs = cos(phase);
    float th = tanh(k * sn);
    float bevel = (1.0 - th * th) / tanh(k);
    float fade = ss(3.0, 9.0, 3.14159 * unit / density);
    float weight = 1.0 * u_vel_intro.z * fade;
    float2 slope = k * cs * bevel * weight * across;

    // Interactive analytical ripples on click + hover wake
    float2 uv01 = frag / res;
    float2 aspect = float2(u_mouse.w, 1.0);
    float2 md = (uv01 - u_mouse.xy) * aspect;
    float mDist = length(md);
    slope += (md / max(mDist, 1e-4)) * u_mouse.z * sin(mDist * 28.0 - u_res_time.w * 6.0) * exp(-mDist * mDist * 18.0) * 0.35;
    [unroll]
    for (int c = 0; c < 6; c++) {
        if (u_clicks[c].w <= 0.5) continue;
        float age = u_clicks[c].z;
        float2 cd = (uv01 - u_clicks[c].xy) * aspect;
        float dist = length(cd);
        float front = dist - age * 0.55;
        float env = exp(-front * front * 70.0) * exp(-age * 1.25);
        slope += (cd / max(dist, 1e-4)) * cos(front * 42.0) * env * 1.45;
    }

    float2 shift = lerp(
        float2(0.22 * sin(t * 1.3), 0.16 * sin(t * 0.9 + 1.7)),
        float2((u_mouse.x * 2.0 - 1.0) * 0.38, (u_mouse.y * 2.0 - 1.0) * 0.30),
        u_mouse.z
    );
    float3 f0 = lerp(float3(1.0, 1.0, 1.0), u_colors[0].rgb, ss(0.05, 0.6, length(slope))) * 0.92;
    float3 v = normalize(float3(uv * 0.08, -1.0));
    float3 light = shadeChrome(slope, v, 0.3 * 0.35, f0, shift);
    float3 col = pow(max(aces(light * 1.05), 0.0), 1.0 / 2.2);
    float grain = frac(52.9829189 * frac(dot(frag, float2(0.06711056, 0.00583715))));
    col = saturate(col + (grain - 0.5) / 255.0);
    return float4(col, 1.0);
}
"#;

// 5. BALATRO (React Bits Hypnotic Swirl Vortex + Click Twirls + Mouse Steer)
const SHADER_BALATRO: &str = r#"
float3 paintBalatro(float2 screenSize, float2 coords, float flow, float spin) {
    float2 uv = (coords - 0.5 * screenSize) / length(screenSize);
    float radius = length(uv);
    float speed = spin + 302.2;
    float swirl = 0.25;
    float angle = atan2(uv.y, uv.x) + speed - 20.0 * (swirl * radius + (1.0 - swirl));
    float2 mid = (screenSize / length(screenSize)) * 0.5;
    uv = float2(radius * cos(angle) + mid.x, radius * sin(angle) + mid.y) - mid;

    uv *= 30.0;
    float2 uv2 = float2(uv.x + uv.y, uv.x + uv.y);
    [unroll]
    for (int i = 0; i < 5; i++) {
        uv2 += sin(max(uv.x, uv.y)) + uv;
        uv += 0.5 * float2(cos(5.1123314 + 0.353 * uv2.y + flow * 0.131121), sin(uv2.x - 0.113 * flow));
        uv -= cos(uv.x + uv.y) - sin(uv.x * 0.711 - uv.y);
    }

    float contrast = 3.5;
    float lighting = 0.4;
    float contrastMod = 0.25 * contrast + 0.5 * swirl + 1.2;
    float amount = min(2.0, max(0.0, length(uv) * 0.035 * contrastMod));
    float c1 = max(0.0, 1.0 - contrastMod * abs(1.0 - amount));
    float c2 = max(0.0, 1.0 - contrastMod * abs(amount));
    float c3 = 1.0 - min(1.0, c1 + c2);
    float light = (lighting - 0.2) * max(c1 * 5.0 - 4.0, 0.0) + lighting * max(c2 * 5.0 - 4.0, 0.0);
    float base = 0.3 / contrast;
    return base * u_colors[0].rgb + (1.0 - base) * (u_colors[0].rgb * c1 + u_colors[1].rgb * c2 + u_colors[2].rgb * c3) + light;
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    // Subtle retro pixel block (1.5 px)
    float block = 1.5;
    float2 p = (floor(frag / block) + 0.5) * block;

    // 4 Click twirls
    [unroll]
    for (int i = 0; i < 4; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        if (age >= 3.2) continue;
        float2 center = u_clicks[i].xy * res;
        float rise = sss(0.0, 0.25, age);
        float fall = 1.0 - sss(0.25, 3.2, age);
        float rad = 160.0 * (1.0 + 0.6 * sss(0.0, 3.2, age));
        float2 rel = p - center;
        float turn = 2.0 * rise * fall * exp(-dot(rel, rel) / (rad * rad));
        p = center + rot2(rel, turn);
    }

    float flow = 14.0 + u_res_time.z * 7.0;
    float steer = (u_mouse.x - 0.5) * 0.45 * u_mouse.z;
    float spin = -0.4 + steer;
    float3 col = paintBalatro(res, p, flow, spin);
    float grain = (frac(52.9829189 * frac(dot(frag, float2(0.06711056, 0.00583715)))) - 0.5) / 255.0;
    return float4(saturate(col + grain), 1.0);
}
"#;

// 6. DARK VEIL (React Bits Exact 8-Layer CPPN Neural Veil + Specular Sheen)
const SHADER_DARK_VEIL: &str = r#"
float4 sigmoid4(float4 x) { return 1.0 / (1.0 + exp(-x)); }

float3 cppn(float2 coordinate, float3 latent) {
    float4 b0, b1, b2, b3, b4, b5, b6, b7;
    b6 = float4(coordinate.x, coordinate.y, latent.x, latent.y);
    b7 = float4(latent.z, length(coordinate), 0.0, 0.0);

    b0 = mv4(float4(6.5404263,-3.6126034,0.7590882,-1.13613), float4(2.4582713,3.1660357,1.2219609,0.06276096), float4(-5.478085,-6.159632,1.8701609,-4.7742867), float4(6.039214,-5.542865,-0.90925294,3.251348), b6)
       + mv4(float4(0.8473259,-5.722911,3.975766,1.6522468), float4(-0.24321538,0.5839259,-1.7661959,-5.350116), float4(0,0,0,0), float4(0,0,0,0), b7)
       + float4(0.21808943,1.1243913,-1.7969975,5.0294676);
    b1 = mv4(float4(-3.3522482,-6.0612736,0.55641043,-4.4719114), float4(0.8631464,1.7432913,5.643898,1.6106541), float4(2.4941394,-3.5012043,1.7184316,6.357333), float4(3.310376,8.209261,1.1355612,-1.165539), b6)
       + mv4(float4(5.24046,-13.034365,0.009859298,15.870829), float4(2.987511,3.129433,-0.89023495,-1.6822904), float4(0,0,0,0), float4(0,0,0,0), b7)
       + float4(-5.9457836,-6.573602,-0.8812491,1.5436668);
    b0 = sigmoid4(b0); b1 = sigmoid4(b1);

    b2 = mv4(float4(-15.219568,8.095543,-2.429353,-1.9381982), float4(-5.951362,4.3115187,2.6393783,1.274315), float4(-7.3145227,6.7297835,5.2473326,5.9411426), float4(5.0796127,8.979051,-1.7278991,-1.158976), b6)
       + mv4(float4(-11.967154,-11.608155,6.1486754,11.237008), float4(2.124141,-6.263192,-1.7050359,-0.7021966), float4(0,0,0,0), float4(0,0,0,0), b7)
       + float4(-4.17164,-3.2281182,-4.576417,-3.6401186);
    b3 = mv4(float4(3.1832156,-13.738922,1.879223,3.233465), float4(0.64300746,12.768129,1.9141049,0.50990224), float4(-0.049295485,4.4807224,1.4733979,1.801449), float4(5.0039253,13.000481,3.3991797,-4.5561905), b6)
       + mv4(float4(-0.1285731,7.720628,-3.1425676,4.742367), float4(0.6393625,3.714393,-0.8108378,-0.39174938), float4(0,0,0,0), float4(0,0,0,0), b7)
       + float4(-1.1811101,-21.621881,0.7851888,1.2329718);
    b2 = sigmoid4(b2); b3 = sigmoid4(b3);

    b4 = mv4(float4(5.214916,-7.183024,2.7228765,2.6592617), float4(-5.601878,-25.3591,4.067988,0.4602802), float4(-10.57759,24.286327,21.102104,37.546658), float4(4.3024497,-1.9625226,2.3458803,-1.372816), b0)
       + mv4(float4(-17.6526,-10.507558,2.2587414,12.462782), float4(6.265566,-502.75443,-12.642513,0.9112289), float4(-10.983244,20.741234,-9.701768,-0.7635988), float4(5.383626,1.4819539,-4.1911616,-4.8444734), b1)
       + mv4(float4(12.785233,-16.345072,-0.39901125,1.7955981), float4(-30.48365,-1.8345358,1.4542528,-1.1118771), float4(19.872723,-7.337935,-42.941723,-98.52709), float4(8.337645,-2.7312303,-2.2927687,-36.142323), b2)
       + mv4(float4(-16.298317,3.5471997,-0.44300047,-9.444417), float4(57.5077,-35.609753,16.163465,-4.1534753), float4(-0.07470326,-3.8656476,-7.0901804,3.1523974), float4(-12.559385,-7.077619,1.490437,-0.8211543), b3)
       + float4(-7.67914,15.927437,1.3207729,-1.6686112);
    b5 = mv4(float4(-1.4109162,-0.372762,-3.770383,-21.367174), float4(-6.2103205,-9.35908,0.92529047,8.82561), float4(11.460242,-22.348068,13.625772,-18.693201), float4(-0.3429052,-3.9905605,-2.4626114,-0.45033523), b0)
       + mv4(float4(7.3481627,-4.3661838,-6.3037653,-3.868115), float4(1.5462853,6.5488915,1.9701879,-0.58291394), float4(6.5858274,-2.2180402,3.7127688,-1.3730392), float4(-5.7973905,10.134961,-2.3395722,-5.965605), b1)
       + mv4(float4(-2.5132585,-6.6685553,-1.4029363,-0.16285264), float4(-0.37908727,0.53738135,4.389061,-1.3024765), float4(-0.70647055,2.0111287,-5.1659346,-3.728635), float4(-13.562562,10.487719,-0.9173751,-2.6487076), b2)
       + mv4(float4(-8.645013,6.5546675,-6.3944063,-5.5933375), float4(-0.57783127,-1.077275,36.91025,5.736769), float4(14.283112,3.7146652,7.1452246,-4.5958776), float4(2.7192075,3.6021907,-4.366337,-2.3653464), b3)
       + float4(-5.9000807,-4.329569,1.2427121,8.59503);
    b4 = sigmoid4(b4); b5 = sigmoid4(b5);

    b6 = mv4(float4(-1.61102,0.7970257,1.4675229,0.20917463), float4(-28.793737,-7.1390953,1.5025433,4.656581), float4(-10.94861,39.66238,0.74318546,-10.095605), float4(-0.7229728,-1.5483948,0.7301322,2.1687684), b0)
       + mv4(float4(3.2547753,21.489103,-1.0194173,-3.3100595), float4(-3.7316632,-3.3792162,-7.223193,-0.23685838), float4(13.1804495,0.7916005,5.338587,5.687114), float4(-4.167605,-17.798311,-6.815736,-1.6451967), b1)
       + mv4(float4(0.604885,-7.800309,-7.213122,-2.741014), float4(-3.522382,-0.12359311,-0.5258442,0.43852118), float4(9.6752825,-22.853785,2.062431,0.099892326), float4(-4.3196306,-17.730087,2.5184598,5.30267), b2)
       + mv4(float4(-6.545563,-15.790176,-6.0438633,-5.415399), float4(-43.591583,28.551912,-16.00161,18.84728), float4(4.212382,8.394307,3.0958717,8.657522), float4(-5.0237565,-4.450633,-4.4768,-5.5010443), b3)
       + mv4(float4(1.6985557,-67.05806,6.897715,1.9004834), float4(1.8680354,2.3915145,2.5231109,4.081538), float4(11.158006,1.7294737,2.0738268,7.386411), float4(-4.256034,-306.24686,8.258898,-17.132736), b4)
       + mv4(float4(1.6889864,-4.5852966,3.8534803,-6.3482175), float4(1.3543309,-1.2640043,9.932754,2.9079645), float4(-5.2770967,0.07150358,-0.13962056,3.3269649), float4(28.34703,-4.918278,6.1044083,4.085355), b5)
       + float4(6.6818056,12.522166,-3.7075126,-4.104386);
    b7 = mv4(float4(-8.265602,-4.7027016,5.098234,0.7509808), float4(8.6507845,-17.15949,16.51939,-8.884479), float4(-4.036479,-2.3946867,-2.6055532,-1.9866527), float4(-2.2167742,-1.8135649,-5.9759874,4.8846445), b0)
       + mv4(float4(6.7790847,3.5076547,-2.8191125,-2.7028968), float4(-5.743024,-0.27844876,1.4958696,-5.0517144), float4(13.122226,15.735168,-2.9397483,-4.101023), float4(-14.375265,-5.030483,-6.2599335,2.9848232), b1)
       + mv4(float4(4.0950394,-0.94011575,-5.674733,4.755022), float4(4.3809423,4.8310084,1.7425908,-3.437416), float4(2.117492,0.16342592,-104.56341,16.949184), float4(-5.22543,-2.994248,3.8350096,-1.9364246), b2)
       + mv4(float4(-5.900337,1.7946124,-13.604192,-3.8060522), float4(6.6583457,31.911177,25.164474,91.81147), float4(11.840538,4.1503043,-0.7314397,6.768467), float4(-6.3967767,4.034772,6.1714606,-0.32874924), b3)
       + mv4(float4(3.4992442,-196.91893,-8.923708,2.8142626), float4(3.4806502,-3.1846354,5.1725626,5.1804223), float4(-2.4009497,15.585794,1.2863957,2.0252278), float4(-71.25271,-62.441242,-8.138444,0.50670296), b4)
       + mv4(float4(-12.291733,-11.176166,-7.3474145,4.390294), float4(10.805477,5.6337385,-0.9385842,-4.7348723), float4(-12.869276,-7.039391,5.3029537,7.5436664), float4(1.4593618,8.91898,3.5101583,5.840625), b5)
       + float4(2.2415268,-6.705987,-0.98861027,-2.117676);
    b6 = sigmoid4(b6); b7 = sigmoid4(b7);

    b0 = mv4(float4(1.6794263,1.3817469,2.9625452,0), float4(-1.8834411,-1.4806935,-3.5924516,0), float4(-1.3279216,-1.0918057,-2.3124623,0), float4(0.2662234,0.23235129,0.44178495,0), b0)
       + mv4(float4(-0.6299101,-0.5945583,-0.9125601,0), float4(0.17828953,0.18300213,0.18182953,0), float4(-2.96544,-2.5819945,-4.9001055,0), float4(1.4195864,1.1868085,2.5176322,0), b1)
       + mv4(float4(-1.2584374,-1.0552157,-2.1688404,0), float4(-0.7200217,-0.52666044,-1.438251,0), float4(0.15345335,0.15196142,0.272854,0), float4(0.945728,0.8861938,1.2766753,0), b2)
       + mv4(float4(-2.4218085,-1.968602,-4.35166,0), float4(-22.683098,-18.0544,-41.954372,0), float4(0.63792,0.5470648,1.1078634,0), float4(-1.5489894,-1.3075932,-2.6444845,0), b3)
       + mv4(float4(-0.49252132,-0.39877754,-0.91366625,0), float4(0.95609266,0.7923952,1.640221,0), float4(0.30616966,0.15693925,0.8639857,0), float4(1.1825981,0.94504964,2.176963,0), b4)
       + mv4(float4(0.35446745,0.3293795,0.59547555,0), float4(-0.58784515,-0.48177817,-1.0614829,0), float4(2.5271258,1.9991658,4.6846647,0), float4(0.13042648,0.08864098,0.30187556,0), b5)
       + mv4(float4(-1.7718065,-1.4033192,-3.3355875,0), float4(3.1664357,2.638297,5.378702,0), float4(-3.1724713,-2.6107926,-5.549295,0), float4(-2.851368,-2.249092,-5.3013067,0), b6)
       + mv4(float4(1.5203838,1.2212278,2.8404984,0), float4(1.5210563,1.2651345,2.683903,0), float4(2.9789467,2.4364579,5.2347264,0), float4(2.2270417,1.8825914,3.8028636,0), b7)
       + float4(-1.5468478,-3.6171484,0.24762098,0);
    return sigmoid4(b0).xyz;
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 uv = (frag * 2.0 - res) / res.y;
    uv.y = -uv.y;
    float2 anchor = float2(res.x / res.y, -1.0);

    float2 ptr = float2(u_mouse.x * 2.0 - 1.0, (1.0 - u_mouse.y) * 2.0 - 1.0);
    ptr.x *= u_mouse.w;
    float2 off = uv - ptr;
    float touch = u_mouse.z * 0.9 * exp(-dot(off, off) / (0.42 * 0.42));
    uv = ptr + rot2(off, -touch * 0.6);

    float t = u_res_time.z;
    uv = rot2(uv, -0.06 * sin(t * 0.17));
    uv += 0.05 * float2(sin(t * 0.21), cos(t * 0.13));
    uv = anchor + uv * 2.0;

    float3 latent = float3(0.3948333, 0.36, 0.14) + 0.1 * float3(sin(0.3 * t), sin(0.69 * t), sin(0.44 * t));
    latent += touch * float3(0.12, -0.10, 0.16);
    float base = cppn(uv, latent).b;
    float glow = saturate(base * 0.92);

    float3 tint = pow(max(u_colors[0].rgb, 1e-4), 2.2);
    float energy = pow(max(base, 0.0), 2.1) * 1.45 * (1.0 + touch * 0.35) + pow(glow, 2.2) * 0.45;
    float3 light = tint * energy + tint * energy * energy * 0.35;
    float3 color = 1.0 - exp(-light);
    color = pow(max(color, 0.0), 1.0 / 2.2);
    float3 bg = float3(0.020, 0.015, 0.042);
    float grain = (hash21(frag + frac(u_res_time.w * 7.31) * 113.0) - 0.5) * 0.028;
    return float4(saturate(bg + color + grain * max(color.r, max(color.g, color.b))), 1.0);
}
"#;

// 7. PLASMA (React Bits 3D Raymarched Twisting Volumetric Plasma + Cursor Orbit)
const SHADER_PLASMA: &str = r#"
float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 c = frag - 0.5 * res;
    float3 rd = normalize(float3(c, res.y));
    float3 ro = float3(0.0, 0.0, -4.0);

    float yaw = (u_mouse.x - 0.5) * 0.9 * u_mouse.z;
    float pitch = (0.5 - u_mouse.y) * 0.18 * u_mouse.z;
    float cy = cos(yaw), sy = sin(yaw);
    float cx = cos(pitch), sx = sin(pitch);
    // Apply pitch (X) then yaw (Y)
    float3 ro1 = float3(ro.x, cx * ro.y - sx * ro.z, sx * ro.y + cx * ro.z);
    ro = float3(cy * ro1.x + sy * ro1.z, ro1.y, -sy * ro1.x + cy * ro1.z);
    float3 rd1 = float3(rd.x, cx * rd.y - sx * rd.z, sx * rd.y + cx * rd.z);
    rd = float3(cy * rd1.x + sy * rd1.z, rd1.y, -sy * rd1.x + cy * rd1.z);

    float t = u_res_time.z * 0.4;
    float z = 0.0;
    float density = 0.0;
    float band = 0.0;
    [loop]
    for (int i = 0; i < 48; i++) {
        float3 p = ro + rd * z;
        float3 s = p;
        float d = p.y - t;
        p.x += 0.4 * (1.0 + p.y) * sin(d + p.x * 0.1) * cos(0.34 * d + p.x * 0.05);
        float ang = p.y - t;
        float2 q = float2(
            p.x * cos(ang) + p.z * cos(ang + 33.0),
            p.x * cos(ang + 11.0) + p.z * cos(ang)
        );
        p.xz = q;
        float shell = abs(sqrt(length(q * q)) - 0.25 * (5.0 + s.y)) / 3.0;
        float stepLen = shell + 0.0012;
        z += stepLen;
        float phase = s.y + p.z * 0.5 + s.z - length(s - p);
        float weight = (1.0 + sin(phase + 8.0)) * exp(-shell * 55.0) * stepLen;
        density += weight;
        band += weight * (0.5 + 0.5 * sin(phase * 0.5 + 1.0));
    }

    float2 mOff = (frag / res - u_mouse.xy) * float2(u_mouse.w, 1.0);
    float heat = u_mouse.z * exp(-dot(mOff, mOff) / 0.05);
    float clickBoost = 0.0;
    [unroll]
    for (int k = 0; k < 6; k++) {
        if (u_clicks[k].w <= 0.5) continue;
        float2 cd = (frag / res - u_clicks[k].xy) * float2(u_mouse.w, 1.0);
        clickBoost += exp(-dot(cd, cd) / 0.08) * exp(-u_clicks[k].z * 1.8);
    }

    float dens = density * 4.8 * u_vel_intro.z * (1.0 + heat * 0.4 + clickBoost * 0.9);
    float b = saturate(band / max(density, 1e-5));
    float3 baseCol = pow(max(u_colors[0].rgb, 1e-4), 2.2);
    float3 hiCol = lerp(baseCol, float3(1.0, 1.0, 1.0), 0.85);
    float3 tint = lerp(baseCol, hiCol, b * 0.35);
    float3 hdr = tint * dens + hiCol * dens * dens * 0.025;
    float peak = max(hdr.r, max(hdr.g, hdr.b));
    float level = 1.0 - exp(-peak);
    float hot = ss(1.0, 3.0, peak);
    float3 hue = hdr / max(peak, 1e-4);
    float3 col = lerp(hue * level, float3(level, level, level), hot * 0.6);
    col = pow(max(col, 0.0), 1.0 / 2.2);
    float grain = (hash21(frag + frac(u_res_time.w * 7.31) * 113.0) - 0.5) * 0.035;
    float3 bg = float3(0.026, 0.022, 0.050);
    return float4(saturate(bg + col + grain * level), 1.0);
}
"#;

// 8. GALAXY (React Bits 6-Layer Starfield + Cursor Repulsion + Click Warp Burst)
const SHADER_GALAXY: &str = r#"
float tri(float x) { return abs(frac(x) * 2.0 - 1.0); }
float tris(float x) { float t = frac(x); return 1.0 - ss(0.0, 1.0, abs(2.0 * t - 1.0)); }
float trisn(float x) { float t = frac(x); return 2.0 * (1.0 - ss(0.0, 1.0, abs(2.0 * t - 1.0))) - 1.0; }

float3 hsv2rgb(float3 c) {
    float4 K = float4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    float3 p = abs(frac(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * lerp(K.xxx, saturate(p - K.xxx), c.y);
}

float Star(float2 uv, float flare, float2 radial, float invReach, float squash, float warpAmt) {
    float along = dot(uv, radial);
    float d = sqrt(max(dot(uv, uv) - squash * along * along, 1e-6));
    float m = 0.055 / d;
    float rays = ss(0.0, 1.0, 1.0 - abs(uv.x * uv.y * 1000.0));
    m += rays * flare;
    float2 uv45 = float2(0.7071 * (uv.x - uv.y), 0.7071 * (uv.x + uv.y));
    rays = ss(0.0, 1.0, 1.0 - abs(uv45.x * uv45.y * 1000.0));
    m += rays * 0.3 * flare;
    m *= ss(1.0, 0.2, d * invReach);
    m *= lerp(1.0, ss(1.0, 0.55, length(uv) * invReach), min(warpAmt * 4.0, 1.0));
    return m;
}

float3 StarLayer(float2 uv, float2 radial, float2 repelAt, float pushCells, float spread, float squash, float warpAmt) {
    float3 col = float3(0.0, 0.0, 0.0);
    float2 gv = frac(uv) - 0.5;
    float2 id = floor(uv);
    float t = u_res_time.z;
    [unroll]
    for (int y = -1; y <= 1; y++) {
        [unroll]
        for (int x = -1; x <= 1; x++) {
            float2 offs = float2((float)x, (float)y);
            float2 si = id + offs;
            float seed = hash21(si);
            float size = frac(seed * 345.32);
            float glossLocal = tri((t * 0.25) / (3.0 * seed + 1.0));
            float flareSize = ss(0.9, 1.0, size) * glossLocal;

            float red = ss(0.2, 1.0, hash21(si + 1.0)) + 0.2;
            float blu = ss(0.2, 1.0, hash21(si + 3.0)) + 0.2;
            float grn = min(red, blu) * seed;
            float3 base = float3(red, grn, blu);
            float hue = atan2(base.g - base.r, base.b - base.r) / 6.28318 + 0.5;
            hue = frac(hue + 140.0 / 360.0);
            float sat = length(base - dot(base, float3(0.299, 0.587, 0.114))) * 0.55;
            float val = max(max(base.r, base.g), base.b);
            base = hsv2rgb(float3(hue, sat, val));

            float2 pad = float2(tris(seed * 34.0 + t * 0.1), tris(seed * 38.0 + t * 0.033)) - 0.5;
            float2 away = si + 0.5 + pad - repelAt;
            float dist = length(away);
            float q = dist * spread;
            float cells = pushCells * exp(-q * q);
            float moved = cells * rsqrt(1.0 + cells * cells * 2.78);
            float2 shift = away * (moved / max(dist, 0.0001));
            float invReach = 1.0 / max(1.0 - moved, 0.05);

            float star = Star(gv - offs - pad - shift, flareSize, radial, invReach, squash, warpAmt);
            float twinkle = lerp(1.0, trisn(t + seed * 6.2831) * 0.5 + 1.0, 0.35);
            col += star * twinkle * size * base;
        }
    }
    return col;
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 uv = (frag - 0.5 * res) / res.y;
    float2 repelAt = (u_mouse.xy * res - 0.5 * res) / res.y;
    float push = 2.0 * 0.05 * u_mouse.z;
    float radius = 0.24;

    float2 radial = length(uv) > 0.0001 ? normalize(uv) : float2(1.0, 0.0);
    float ang = u_res_time.z * 0.06;
    uv = rot2(uv, ang);
    radial = rot2(radial, ang);
    repelAt = rot2(repelAt, ang);

    float warpAmt = saturate(u_vel_intro.w);
    float squash = warpAmt * 0.92;
    float depthBase = u_res_time.z * 0.06 + warpAmt * 0.25;

    float3 col = float3(0.0, 0.0, 0.0);
    [unroll]
    for (int k = 0; k < 5; k++) {
        float i = (float)k / 5.0;
        float depth = frac(i + depthBase);
        float scale = lerp(18.0, 0.6, depth);
        float fade = depth * ss(1.0, 0.9, depth);
        float2 off = float2(i * 453.32, i * 453.32);
        col += StarLayer(uv * scale + off, radial, repelAt * scale + off, push * scale, 1.0 / (scale * radius), squash, warpAmt) * fade;
    }

    float3 bg = float3(0.015, 0.018, 0.042);
    return float4(saturate(bg + col * u_vel_intro.z), 1.0);
}
"#;

// 9. LIGHT RAYS (React Bits Volumetric Spotlight Rays + Fog Haze + Cursor Shadow + Click Pulses)
const SHADER_LIGHT_RAYS: &str = r#"
float valNoise(float2 p) {
    float2 i = floor(p);
    float2 f = p - i;
    float2 w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    return lerp(
        lerp(hash21(i), hash21(i + float2(1.0, 0.0)), w.x),
        lerp(hash21(i + float2(0.0, 1.0)), hash21(i + float2(1.0, 1.0)), w.x),
        w.y
    );
}

float rayShape(float strength, float width) {
    float sharp = 0.35;
    float edge = max(lerp(0.5, 0.08, sharp), width);
    return lerp(strength, ss(0.62 - edge, 0.62 + edge, strength), sharp);
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    // LightRays uses top-down pixel coordinates
    float2 coord = inp.pos.xy;
    float2 source = float2(res.x * 0.5, -res.y * 0.2);
    float2 cursorPx = float2(u_mouse.x * res.x, (1.0 - u_mouse.y) * res.y);
    float2 toM = normalize(cursorPx - source);
    float2 aim = normalize(lerp(float2(0.0, 1.0), toM, u_mouse.z * 0.30));

    float reach = max(res.x, res.y);
    float2 delta = coord - source;
    float dist = length(delta);
    float2 dir = delta / max(dist, 1e-4);
    float cosAngle = dot(dir, aim);
    float t = u_res_time.z;
    float angle = cosAngle + 0.04 * sin(t * 2.0 + dist * 0.015);
    float slope = fwidth(angle);
    float depth = dist / reach;

    float srcGlow = exp(-depth * depth * 9.0) * 0.9 + exp(-depth * 3.2) * 0.25;
    float glowLight = 0.65 * srcGlow * ss(-0.2, 0.6, cosAngle);
    float spread = pow(max(angle, 0.0), 1.0 / 0.55);
    float lenFall = saturate(1.0 - dist / (reach * 2.6));
    float fadeFall = clamp(1.0 - dist / (reach * 1.15), 0.5, 1.0);
    float env = spread * lenFall * fadeFall;

    float pA = angle * 36.2214 + t * 1.5;
    float pB = -angle * 21.11349 + t * 1.5;
    float pC = angle * 22.3991 + t * 1.1;
    float pD = -angle * 18.0234 + t * 1.1;
    float sA = saturate(0.75 + 0.15 * sin(pA) + 0.20 * cos(pB));
    float sB = saturate(0.75 + 0.15 * sin(pC) + 0.20 * cos(pD));
    float wA = abs(0.15 * 36.2214 * cos(pA) + 0.20 * 21.11349 * sin(pB)) * slope;
    float wB = abs(0.15 * 22.3991 * cos(pC) + 0.20 * 18.0234 * sin(pD)) * slope;
    float rayLight = env * (0.5 * rayShape(sA, wA) + 0.4 * rayShape(sB, wB));

    float around = atan2(aim.x * dir.y - aim.y * dir.x, cosAngle);
    float2 drift = float2(around * 2.2 + t * 0.035, depth * 2.6 - t * 0.11);
    float fog = 0.65 * valNoise(drift * float2(3.0, 1.0)) + 0.35 * valNoise(drift * float2(7.0, 2.3) + 4.1);
    rayLight *= lerp(1.0, 0.25 + 1.2 * fog, 0.28);

    // Cursor shadow occlusion
    if (u_mouse.z > 0.01) {
        float2 toCursor = cursorPx - source;
        float cDist = length(toCursor);
        float along = dot(delta, toCursor) / max(cDist * cDist, 1e-4);
        float side = abs(delta.x * toCursor.y - delta.y * toCursor.x) / max(cDist, 1e-4);
        float trail = max(along - 1.0, 0.0);
        float shW = 42.0 * along + 0.22 * trail * cDist;
        float behind = ss(0.97, 1.25, along) * exp(-trail * 1.1);
        float shade = (1.0 - ss(shW * 0.3, shW * 1.6, side)) * behind;
        rayLight *= 1.0 - u_mouse.z * 0.58 * shade;
    }

    float boost = 0.0;
    [unroll]
    for (int i = 0; i < 4; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        float x = (depth - age * 0.95) / (0.14 + age * 0.10);
        boost += 1.4 * exp(-age * 1.6) * exp(-x * x);
    }

    float light = (rayLight * (1.0 + boost) + glowLight) * 1.15 * u_vel_intro.z;
    float near = saturate(1.2 - dist / res.y);
    float3 tint = float3(0.1, 0.3, 0.5) + near * float3(0.8, 0.6, 0.5);
    float3 hue = max(tint, 0.0) * u_colors[0].rgb;
    float level = light < 0.75 ? light : 0.75 + 0.25 * (1.0 - exp(-(light - 0.75) / 0.25));
    float top = max(hue.r, max(hue.g, hue.b));
    hue = lerp(hue, float3(top, top, top), ss(0.8, 1.0, level) * 0.45);
    float3 bg = float3(0.018, 0.026, 0.052);
    float grain = (frac(52.9829189 * frac(dot(inp.pos.xy, float2(0.06711056, 0.00583715)))) - 0.5) / 255.0;
    return float4(saturate(bg + hue * level * 1.1 + grain), 1.0);
}
"#;

// 10. PRISMATIC BURST (React Bits 3D Volumetric Spectral Raymarch + Click Shockwaves)
const SHADER_PRISMATIC_BURST: &str = r#"
float pbHash(float2 p) {
    p = floor(p);
    return frac(52.9829189 * frac(dot(p, float2(0.065, 0.005))));
}

float pbNoise(float2 fragPx, float t) {
    float2 p = glsl_mod2(fragPx + float2(t * 30.0, -t * 21.0), 1024.0);
    float2 q = float2(0.8 * p.x - 0.5 * p.y, 0.5 * p.x + 0.8 * p.y);
    return 0.40 * pbHash(q)
         + 0.25 * pbHash(q * 2.0 + 17.0)
         + 0.20 * pbHash(q * 4.0 + 47.0)
         + 0.15 * pbHash(q * 8.0 + 113.0);
}

float3 rot3D(float3 v, float ax, float ay, float az) {
    float cx = cos(ax), sx = sin(ax);
    v = float3(v.x, cx * v.y - sx * v.z, sx * v.y + cx * v.z);
    float cy = cos(ay), sy = sin(ay);
    v = float3(cy * v.x + sy * v.z, v.y, -sy * v.x + cy * v.z);
    float cz = cos(az), sz = sin(az);
    return float3(cz * v.x - sz * v.y, sz * v.x + cz * v.y, v.z);
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float t = u_res_time.z;
    float2 mOff = (u_mouse.xy - 0.5) * 0.06 * u_mouse.z * min(res.x, res.y);
    float2 centered = 2.0 * (frag - mOff) - res;
    float3 dir = normalize(float3(centered, res.y));

    float marchT = 0.0;
    float3 col = float3(0.0, 0.0, 0.0);
    float n = pbNoise(frag, u_res_time.w);
    float ax = t * 0.31 + (0.5 - u_mouse.y) * 0.3 * u_mouse.z;
    float ay = t * 0.21 + (u_mouse.x - 0.5) * 0.3 * u_mouse.z;
    float az = t * 0.17;

    [loop]
    for (int i = 0; i < 40; i++) {
        float3 P = marchT * dir;
        P.z -= 2.0;
        float rad = length(P);
        float3 Pl = P * (10.0 / max(rad, 1e-6));
        Pl = rot3D(Pl, ax, ay, az);
        float stepLen = min(rad - 0.3, n * 0.08) + 0.11;
        float rayPattern = ss(0.5, 0.7, sin(Pl.x + cos(Pl.y) * cos(Pl.z)) * sin(Pl.z + sin(Pl.y) * cos(Pl.x + t)));
        float3 spectral = 1.0 + cos(marchT * 3.0 + float3(0.0, 1.0, 2.0));
        col += (0.055 / (0.4 + stepLen)) * ss(5.0, 0.0, rad) * spectral * rayPattern;
        marchT += stepLen;
    }

    float unit = 0.5 * min(res.x, res.y);
    float radius = length(frag - 0.5 * res - mOff) / unit;
    float ef = saturate(radius);
    ef = ef * ef * ef * (ef * (ef * 6.0 - 15.0) + 10.0) * 0.5;
    ef = pow(max(ef, 0.0), 1.5);
    col *= ef * 2.0;

    float lift = 1.0;
    [unroll]
    for (int k = 0; k < 4; k++) {
        if (u_clicks[k].w <= 0.5) continue;
        float age = u_clicks[k].z;
        if (age > 1.6) continue;
        float d = (radius - age * 1.7) / (0.2 + age * 0.16);
        lift += 1.6 * pow(max(1.0 - age / 1.6, 0.0), 1.6) * exp(-d * d);
    }
    float2 pm = (frag - u_mouse.xy * res) / unit;
    lift += 0.55 * u_mouse.z * exp(-dot(pm, pm) / 0.20);

    float3 bg = float3(0.018, 0.018, 0.036);
    col = saturate(bg + col * lift * u_vel_intro.z);
    return float4(col, 1.0);
}
"#;

// 11. MOLTEN METAL (React Bits Folded Caustic Metallic Filaments + Click Flares)
const SHADER_MOLTEN_METAL: &str = r#"
float3 moltenPalette(float g) {
    float t = saturate(g);
    if (t < 0.5) {
        return lerp(u_colors[0].rgb, u_colors[1].rgb, ss(0.0, 0.5, t));
    }
    return lerp(u_colors[1].rgb, u_colors[2].rgb, ss(0.5, 1.0, t));
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 uv = (frag - 0.5 * res) / res.y;
    float2 p = 4.0 * uv - 0.5;

    float2 mUv = (u_mouse.xy * res - 0.5 * res) / res.y;
    float2 md = (uv - mUv) / 0.22;
    float heat = u_mouse.z * 0.65 * exp(-dot(md, md));

    float flare = 0.0;
    [unroll]
    for (int k = 0; k < 6; k++) {
        if (u_clicks[k].w <= 0.5) continue;
        float age = u_clicks[k].z;
        if (age >= 1.8) continue;
        float life = saturate(age / 1.8);
        float rise = 1.0 - exp(-age / 0.07);
        float fall = exp(-3.0 * life) * (1.0 - life) * (1.0 - life);
        float2 cUv = (u_clicks[k].xy * res - 0.5 * res) / res.y;
        float2 cd = (uv - cUv) / (0.18 * (0.9 + 1.6 * sqrt(life)));
        flare += rise * fall * exp(-dot(cd, cd));
    }

    float time = u_res_time.z + 2.5;
    float power = 1.0 + (heat + flare) * 0.45;
    float2 i = p;
    float c = 0.0;
    float r = length(p + float2(sin(time), sin(time * 0.3 + 5.0)) * 0.5);
    float d = length(p);
    float rot = d + time + p.x;
    float cosRot = cos(rot);
    float m00 = cos(rot - sin(time * 0.2)) * -0.2;
    float m10 = sin(rot) * -0.2;
    float m01 = -sin(cosRot - time) * -0.2;
    float m11 = cosRot * -0.2;

    [unroll]
    for (int n = 0; n < 3; n++) {
        p = float2(p.x * m00 + p.y * m10, p.x * m01 + p.y * m11);
        float t = r - time / ((float)n + 3.0);
        i -= p + float2(cos(t - i.x - r) + sin(t + i.y), sin(t - i.y) + cos(t + i.x) + r);
        float l = length(float2(sin(i.x + t), cos(i.y + t)));
        float fp = 0.5 * length(float2(ddx(l), ddy(l)));
        l = sqrt(l * l + fp * fp);
        c += 0.16 / pow(max(l, 1e-5), power);
    }

    c /= 6.0;
    c *= 1.0 + heat * 0.25 + flare * 0.75;
    float g = saturate(max(c - 0.05, 0.0) * 1.3);
    float3 col = moltenPalette(g);
    float noise = hash21(frag + frac(u_res_time.w) * 97.0) - 0.5;
    float a = saturate(g + noise * 0.05) * u_vel_intro.z;
    float3 bg = u_colors[3].rgb;
    return float4(saturate(lerp(bg, col, a)), 1.0);
}
"#;

// 12. COLOR BENDS (React Bits Swirling Neon Glass Color Bands + Parallax)
const SHADER_COLOR_BENDS: &str = r#"
float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 vUv = frag / res;
    float t = u_res_time.z;
    float2 ptr = (u_mouse.xy * 2.0 - 1.0) * u_mouse.z;
    float2 p = vUv * 2.0 - 1.0 + ptr * 0.05;
    float2 rp = float2(-p.y, p.x); // 90 deg rotation matching React Bits default
    float2 q = float2(rp.x * (res.x / res.y), rp.y);
    q /= 0.5 + 0.2 * dot(q, q);
    q += 0.2 * cos(t) - 7.56;
    q += (ptr - rp) * 0.20;

    // Click ripple displacement
    [unroll]
    for (int c = 0; c < 4; c++) {
        if (u_clicks[c].w <= 0.5) continue;
        float2 cp = u_clicks[c].xy * 2.0 - 1.0;
        float2 d = p - cp;
        float dist = length(d);
        float age = u_clicks[c].z;
        q += (d / max(dist, 1e-4)) * sin((dist - age * 1.1) * 14.0) * exp(-pow((dist - age * 1.1) * 4.0, 2.0)) * exp(-age * 1.5) * 0.35;
    }

    float2 s = q;
    float3 sumCol = float3(0.0, 0.0, 0.0);
    [unroll]
    for (int i = 0; i < 3; i++) {
        s -= 0.01;
        float2 r = sin(1.5 * s.yx + 2.0 * cos(s));
        float m = length(r + sin(5.0 * r.y - 3.0 * t + (float)i) * 0.25);
        float w = 1.0 - exp(-6.0 / exp(6.0 * m));
        sumCol += u_colors[i].rgb * w;
    }

    float3 col = saturate(sumCol) * 1.35;
    float n = frac(sin(dot(frag + u_res_time.w, float2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * 0.06;
    float3 bg = u_colors[3].rgb;
    return float4(saturate(bg + col * u_vel_intro.z), 1.0);
}
"#;

// 13. GRAINIENT (React Bits OKLab Domain-Warped Sunset Gradient + Silk Shading + Live Film Grain)
const SHADER_GRAINIENT: &str = r#"
float2 gHash2(float2 p) {
    float3 q = frac(float3(p.x, p.y, p.x) * float3(0.1031, 0.1030, 0.0973));
    q += dot(q, q.yzx + 33.33);
    return frac((q.xx + q.yz) * q.zy) * 2.0 - 1.0;
}

float gNoise(float2 p) {
    float2 i = floor(p);
    float2 f = frac(p);
    float2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float a = dot(gHash2(i), f);
    float b = dot(gHash2(i + float2(1.0, 0.0)), f - float2(1.0, 0.0));
    float c = dot(gHash2(i + float2(0.0, 1.0)), f - float2(0.0, 1.0));
    float d = dot(gHash2(i + float2(1.0, 1.0)), f - float2(1.0, 1.0));
    return 0.5 + 0.5 * lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y);
}

float2 gDrift(float2 p, float t) {
    float2 w1 = float2(
        gNoise(p * 0.85 + float2(t * 0.11, t * 0.07)),
        gNoise(p * 0.85 + float2(4.1, 1.7) - float2(t * 0.08, t * 0.12))
    );
    float2 w2 = float2(
        gNoise(p * 1.55 + w1 * 1.8 + float2(1.3, 7.1) + t * 0.05),
        gNoise(p * 1.55 + w1 * 1.8 + float2(6.4, 2.2) - t * 0.06)
    );
    return (w1 - 0.5) * 1.1 + (w2 - 0.5) * 0.55;
}

float gCoord(float2 p, float t, float2 dir, float extent) {
    float2 q = p + float2(-dir.y, dir.x) * t * 0.45;
    float2 w = p + gDrift(q, t) * 0.85;
    return dot(w, dir) / extent + 0.5;
}

float gHash1(float2 p) {
    float3 q = frac(float3(p.x, p.y, p.x) * 0.1031);
    q += dot(q, q.yzx + 33.33);
    return frac((q.x + q.y) * q.z);
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 vUv = frag / res;
    float aspect = u_mouse.w;
    float2 p = (vUv - 0.5) * float2(aspect, 1.0);

    // Cursor stir
    float2 mp = (u_mouse.xy - 0.5) * float2(aspect, 1.0);
    float2 md = p - mp;
    p -= float2(u_vel_intro.x, -u_vel_intro.y) * u_mouse.z * exp(-dot(md, md) / 0.045) * 0.12;

    // 4 Click twirls
    [unroll]
    for (int i = 0; i < 4; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        if (age >= 2.6) continue;
        float2 cp = (u_clicks[i].xy - 0.5) * float2(aspect, 1.0);
        float k = age / 2.6;
        float env = min(1.0, age / 0.18) * (1.0 - k) * (1.0 - k);
        float rad = 0.16 + 0.10 * min(1.0, age / 0.5);
        float2 d = p - cp;
        float a = 2.4 * env * exp(-dot(d, d) / (rad * rad));
        p = cp + rot2(d, a);
    }

    float t = u_res_time.z * 0.16;
    float2 dir = float2(sin(0.96), cos(0.96)); // 55 deg
    float extent = aspect * abs(dir.x) + abs(dir.y);
    float raw = gCoord(p, t, dir, extent);
    float s = saturate(raw);

    // OKLab ramp matching React Bits default ['#120f17', '#2a0f0a', '#9a3412', '#f97316', '#fed7aa']
    float3 r0 = float3(0.174,  0.011, -0.017);
    float3 r1 = float3(0.205,  0.045,  0.028);
    float3 r2 = float3(0.470,  0.135,  0.095);
    float3 r3 = float3(0.705,  0.165,  0.140);
    float3 r4 = float3(0.901,  0.045,  0.072);
    float scaled = s * 4.0;
    float3 lab;
    if (scaled < 1.0) lab = lerp(r0, r1, ss(0.0, 1.0, scaled));
    else if (scaled < 2.0) lab = lerp(r1, r2, ss(0.0, 1.0, scaled - 1.0));
    else if (scaled < 3.0) lab = lerp(r2, r3, ss(0.0, 1.0, scaled - 2.0));
    else lab = lerp(r3, r4, ss(0.0, 1.0, scaled - 3.0));

    // Silk relief shading
    float2 grad = float2(gCoord(p + float2(0.004, 0.0), t, dir, extent) - raw, gCoord(p + float2(0.0, 0.004), t, dir, extent) - raw) / 0.004;
    float2 slope = grad * sin(3.14159265 * s) * 0.35 * 4.0;
    float3 normal = normalize(float3(-slope, 1.0));
    float3 light = normalize(float3(-dir.y, dir.x, 1.2));
    float diff = dot(normal, light) - light.z;
    lab = r0 + (lab - r0) * (1.0 + (diff * 0.35 + max(diff, 0.0) * diff * 0.6) * 0.35);

    float3 col = toSrgb(oklabToLinear(lab));
    float frameSeed = floor(u_res_time.w * 24.0) % 97.0;
    float2 pix = frag + frameSeed * float2(17.0, 59.0);
    float fine = gHash1(pix) + gHash1(pix + 7.13) - 1.0;
    float luma = dot(col, float3(0.2126, 0.7152, 0.0722));
    col += fine * 0.075 * (0.45 + 2.2 * luma * (1.0 - luma));
    return float4(saturate(col), 1.0);
}
"#;

// 14. RIPPLE GRID (React Bits 3D Tilted Cyber Grid + Auto/Click Wave Packets + Cursor Halo)
const SHADER_RIPPLE_GRID: &str = r#"
float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 screen = float2(frag.x - res.x * 0.5, res.y * 0.5 - frag.y);
    float tilt = 0.52; // ~30 deg perspective
    float focal = max(res.x, res.y) * 0.85;
    float depth = focal + screen.y * tan(tilt);
    float horizon = ss(0.0, focal * 0.35, depth);
    float sc = focal / max(depth, 0.001);
    float2 plane = float2(screen.x * sc, screen.y * sc / cos(tilt));

    float cell = 48.0;
    float waveLen = 140.0;
    float waveSpd = 260.0;
    float2 push = float2(0.0, 0.0);
    float lift = 0.0;

    // Periodic center auto-ripple
    float autoAge = glsl_mod(u_res_time.z, 2.6);
    {
        float dist = length(plane);
        float x = (dist - autoAge * waveSpd) / waveLen;
        float w = sin(x * 6.28318) * exp(-x * x * 1.8) * (1.0 / (1.0 + dist / (waveLen * 5.0))) * ss(0.0, waveLen * 0.9, dist) * exp(-autoAge * 0.65);
        push += (dist > 0.001 ? plane / dist : float2(0.0, 0.0)) * w;
        lift += w;
    }

    // 6 Click ripples projected onto tilted grid plane
    [unroll]
    for (int i = 0; i < 6; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        float2 cs = float2((u_clicks[i].x - 0.5) * res.x, (0.5 - u_clicks[i].y) * res.y);
        float cDepth = focal + cs.y * tan(tilt);
        float cSc = focal / max(cDepth, 0.001);
        float2 cPlane = float2(cs.x * cSc, cs.y * cSc / cos(tilt));
        float2 delta = plane - cPlane;
        float dist = length(delta);
        float x = (dist - age * waveSpd * 1.2) / waveLen;
        float w = sin(x * 6.28318) * exp(-x * x * 1.8) * (1.0 / (1.0 + dist / (waveLen * 5.0))) * ss(0.0, waveLen * 0.7, dist) * exp(-age * 0.95) * 1.35;
        push += (dist > 0.001 ? delta / dist : float2(0.0, 0.0)) * w;
        lift += w;
    }

    float2 bent = plane + push * cell * 0.26;
    float2 grid = bent / cell;
    float2 fw = max(fwidth(grid), float2(1e-5, 1e-5));
    float2 offset = abs(frac(grid - 0.5) - 0.5);
    float2 dist = offset / fw;
    float2 cover = 1.0 - ss(0.15, 1.15, dist);
    float shape = max(cover.x, cover.y);
    float edge = min(dist.x, dist.y);
    float halo = exp(-edge / 2.6) * 0.70;
    float far = 1.0 - ss(0.18, 0.50, max(fw.x, fw.y));

    float2 ms = float2((u_mouse.x - 0.5) * res.x, (0.5 - u_mouse.y) * res.y);
    float mDepth = focal + ms.y * tan(tilt);
    float mSc = focal / max(mDepth, 0.001);
    float2 mPlane = float2(ms.x * mSc, ms.y * mSc / cos(tilt));
    float2 toPtr = (plane - mPlane) / (cell * 3.2);
    float near = u_mouse.z * exp(-dot(toPtr, toPtr));
    float shine = max(1.0 + lift * 3.4, 0.25) * (1.0 + near * 1.8);

    float2 uv01 = frag / res;
    float mask = sss(0.0, 0.22, uv01.x) * sss(0.0, 0.22, 1.0 - uv01.x)
               * sss(0.0, 0.22, uv01.y) * sss(0.0, 0.22, 1.0 - uv01.y);
    mask *= horizon * far * u_vel_intro.z;

    float level = (shape * 0.42 + halo * 0.18) * shine * mask;
    float3 gridCol = lerp(u_colors[0].rgb, float3(1.0, 1.0, 1.0), saturate(level - 0.55));
    float3 bg = u_colors[3].rgb;
    return float4(saturate(bg + gridCol * level), 1.0);
}
"#;

// 15. ORB (React Bits Glowing 3D Plasma Ring + Orbiting Comet + 3D Cursor Tilt + Click Echo)
const SHADER_ORB: &str = r#"
float3 hash33(float3 p3) {
    p3 = frac(p3 * float3(0.1031, 0.11369, 0.13787));
    p3 += dot(p3, p3.yxz + 19.19);
    return -1.0 + 2.0 * frac(float3(p3.x + p3.y, p3.x + p3.z, p3.y + p3.z) * p3.zyx);
}

float snoise3(float3 p) {
    const float K1 = 0.333333333;
    const float K2 = 0.166666667;
    float3 i = floor(p + (p.x + p.y + p.z) * K1);
    float3 d0 = p - (i - (i.x + i.y + i.z) * K2);
    float3 e = step(0.0, d0 - d0.yzx);
    float3 i1 = e * (1.0 - e.zxy);
    float3 i2 = 1.0 - e.zxy * (1.0 - e);
    float3 d1 = d0 - (i1 - K2);
    float3 d2 = d0 - (i2 - K1);
    float3 d3 = d0 - 0.5;
    float4 h = max(0.6 - float4(dot(d0, d0), dot(d1, d1), dot(d2, d2), dot(d3, d3)), 0.0);
    float4 n = h * h * h * h * float4(dot(d0, hash33(i)), dot(d1, hash33(i + i1)), dot(d2, hash33(i + i2)), dot(d3, hash33(i + 1.0)));
    return dot(float4(31.316, 31.316, 31.316, 31.316), n);
}

float light1(float i, float a, float d) { return i / (1.0 + d * a); }
float light2(float i, float a, float d) { return i / (1.0 + d * d * a); }

float2 toPlane(float2 p, float2 tilt, out float hit) {
    float cx = cos(tilt.x), sx = sin(tilt.x);
    float cy = cos(tilt.y), sy = sin(tilt.y);
    float3 ex = float3(cy, 0.0, -sy);
    float3 ey = float3(sy * sx, cx, cy * sx);
    float3 normal = float3(sy * cx, -sx, cy * cx);
    float3 origin = float3(0.0, 0.0, 3.2);
    float3 ray = float3(p, -3.2);
    float denom = dot(normal, ray);
    float t = -dot(normal, origin) / (abs(denom) > 1e-5 ? denom : -1e-5);
    hit = t > 0.0 ? 1.0 : 0.0;
    float3 q = origin + ray * t;
    return float2(dot(q, ex), dot(q, ey));
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float2 uv = (frag - res * 0.5) / min(res.x, res.y) * 2.15;
    float2 tilt = float2((0.5 - u_mouse.y) * 0.22 * u_mouse.z, (u_mouse.x - 0.5) * 0.28 * u_mouse.z);
    float hit = 1.0;
    if (abs(tilt.x) + abs(tilt.y) > 0.0001) uv = toPlane(uv, tilt, hit);

    float t = u_res_time.z;
    float rawLen = length(uv);
    float2 dir = rawLen > 0.0001 ? uv / rawLen : float2(1.0, 0.0);
    float2 reachVec = (u_mouse.xy - 0.5) * 0.14 * u_mouse.z;
    float reachAmt = length(reachVec);
    float2 reachDir = reachAmt > 1e-5 ? reachVec / reachAmt : float2(1.0, 0.0);
    float facing = exp((dot(dir, reachDir) - 1.0) * 3.2);
    float len = max(rawLen - reachAmt * facing * ss(0.25, 0.65, rawLen), 0.0);
    uv = dir * len;
    float ang = atan2(dir.y, dir.x);
    float invLen = len > 0.0 ? 1.0 / len : 0.0;

    float n0 = snoise3(float3(uv * 0.65, t * 0.5)) * 0.5 + 0.5;
    float r0 = lerp(0.76, 0.84, n0);
    float d0 = distance(uv, (r0 * invLen) * uv);
    float v0 = light1(1.0, 10.0, d0) * ss(r0 * 1.05, r0, len);
    v0 *= ss(r0 * 0.80, r0 * 0.95, len);
    float body = ss(0.60, 0.80, len);
    float cl = cos(ang + t * 0.8) * 0.5 + 0.5;

    float orbit = t * 1.1;
    float2 head = float2(cos(orbit), sin(orbit));
    float d = distance(uv, head * r0);
    float lag = atan2(head.x * dir.y - head.y * dir.x, dot(dir, head));
    float trail = ss(0.0, 0.35, lag) * exp(-max(lag, 0.0) * 2.6);
    float v1 = (light2(1.5, 5.0, d) + trail * 0.6) * light1(1.0, 50.0, d0);
    float v2 = ss(1.0, lerp(0.6, 1.0, n0 * 0.5), len);
    float band = v0 * v2 * body;
    float heat = pow(saturate(band / 0.82), 3.0);
    float bloom = light2(1.0, 160.0, max(len - r0, 0.0)) * ss(0.4, 0.0, len - r0) * step(r0, len);

    // Click echo wave
    float echo = 0.0;
    [unroll]
    for (int k = 0; k < 4; k++) {
        if (u_clicks[k].w <= 0.5) continue;
        float age = u_clicks[k].z;
        float ed = abs(len - r0 - age * 0.45);
        echo += light1(1.0, 60.0, ed) * ss(0.14, 0.0, ed) * exp(-age * 2.2);
    }

    float3 colBase = lerp(u_colors[0].rgb, u_colors[1].rgb, cl);
    float3 orbCol = lerp(u_colors[2].rgb, colBase, v0);
    orbCol = (orbCol + v1) * v2 * body * (1.0 + u_mouse.z * (0.18 + facing * 0.35)) + lerp(colBase, float3(1.0, 1.0, 1.0), 0.25) * heat * 0.35;
    orbCol = saturate(orbCol + colBase * (bloom * 0.14 + echo * 0.8));

    float3 bg = u_colors[3].rgb;
    float dither = (hash21(frag) - 0.5) / 255.0;
    return float4(saturate(bg + orbCol * hit * u_vel_intro.z + dither), 1.0);
}
"#;

// 16. DITHER (React Bits 4-Octave Perlin fBM Waves + 8x8 Bayer Ordered Dithering + Click Ripples)
const SHADER_DITHER: &str = r#"
float4 mod289_4(float4 x) { return x - floor(x * (1.0 / 289.0)) * 289.0; }
float4 permute4(float4 x) { return mod289_4(((x * 34.0) + 1.0) * x); }
float4 taylorInvSqrt4(float4 r) { return 1.79284291400159 - 0.85373472095314 * r; }
float2 fade2(float2 t) { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }

float cnoise(float2 P) {
    float4 Pi = floor(P.xyxy) + float4(0.0, 0.0, 1.0, 1.0);
    float4 Pf = frac(P.xyxy) - float4(0.0, 0.0, 1.0, 1.0);
    Pi = mod289_4(Pi);
    float4 ix = Pi.xzxz;
    float4 iy = Pi.yyww;
    float4 fx = Pf.xzxz;
    float4 fy = Pf.yyww;
    float4 i = permute4(permute4(ix) + iy);
    float4 gx = frac(i * (1.0 / 41.0)) * 2.0 - 1.0;
    float4 gy = abs(gx) - 0.5;
    float4 tx = floor(gx + 0.5);
    gx -= tx;
    float2 g00 = float2(gx.x, gy.x);
    float2 g10 = float2(gx.y, gy.y);
    float2 g01 = float2(gx.z, gy.z);
    float2 g11 = float2(gx.w, gy.w);
    float4 norm = taylorInvSqrt4(float4(dot(g00, g00), dot(g01, g01), dot(g10, g10), dot(g11, g11)));
    g00 *= norm.x; g01 *= norm.y; g10 *= norm.z; g11 *= norm.w;
    float n00 = dot(g00, float2(fx.x, fy.x));
    float n10 = dot(g10, float2(fx.y, fy.y));
    float n01 = dot(g01, float2(fx.z, fy.z));
    float n11 = dot(g11, float2(fx.w, fy.w));
    float2 fadeXY = fade2(Pf.xy);
    float2 nx = lerp(float2(n00, n01), float2(n10, n11), fadeXY.x);
    return 2.3 * lerp(nx.x, nx.y, fadeXY.y);
}

float dFbm(float2 p) {
    float v = 0.0;
    float a = 1.0;
    [unroll]
    for (int i = 0; i < 4; i++) {
        v += a * abs(cnoise(p));
        p *= 3.0;
        a *= 0.30;
    }
    return v;
}

float bayer8(int2 cell) {
    static const float B[64] = {
         0.0/64.0, 48.0/64.0, 12.0/64.0, 60.0/64.0,  3.0/64.0, 51.0/64.0, 15.0/64.0, 63.0/64.0,
        32.0/64.0, 16.0/64.0, 44.0/64.0, 28.0/64.0, 35.0/64.0, 19.0/64.0, 47.0/64.0, 31.0/64.0,
         8.0/64.0, 56.0/64.0,  4.0/64.0, 52.0/64.0, 11.0/64.0, 59.0/64.0,  7.0/64.0, 55.0/64.0,
        40.0/64.0, 24.0/64.0, 36.0/64.0, 20.0/64.0, 43.0/64.0, 27.0/64.0, 39.0/64.0, 23.0/64.0,
         2.0/64.0, 50.0/64.0, 14.0/64.0, 62.0/64.0,  1.0/64.0, 49.0/64.0, 13.0/64.0, 61.0/64.0,
        34.0/64.0, 18.0/64.0, 46.0/64.0, 30.0/64.0, 33.0/64.0, 17.0/64.0, 45.0/64.0, 29.0/64.0,
        10.0/64.0, 58.0/64.0,  6.0/64.0, 54.0/64.0,  9.0/64.0, 57.0/64.0,  5.0/64.0, 53.0/64.0,
        42.0/64.0, 26.0/64.0, 38.0/64.0, 22.0/64.0, 41.0/64.0, 25.0/64.0, 37.0/64.0, 21.0/64.0
    };
    int cx = cell.x & 7;
    int cy = cell.y & 7;
    return B[cy * 8 + cx];
}

float4 PSMain(VSOut inp) : SV_TARGET {
    float2 res = u_res_time.xy;
    float2 frag = float2(inp.pos.x, res.y - inp.pos.y);
    float cellPx = 3.0;
    float2 cell = floor(frag / cellPx);
    float2 corner = (cell + 0.5) * cellPx;
    float2 uv = corner / res - 0.5;
    uv.x *= u_mouse.w;

    float2 mUv = (u_mouse.xy - 0.5) * float2(u_mouse.w, 1.0);
    float2 mDelta = uv - mUv;
    float mDist = length(mDelta);
    float touch = u_mouse.z * (1.0 - ss(0.0, 0.35, mDist));
    float2 warp = float2(0.0, 0.0);
    if (touch > 0.0) {
        warp += (mDist > 1e-4 ? mDelta / mDist : float2(0.0, 0.0)) * sin(mDist * 32.0 - u_res_time.z * 7.0) * 0.03 * touch;
    }

    float ring = 0.0;
    [unroll]
    for (int i = 0; i < 4; i++) {
        if (u_clicks[i].w <= 0.5) continue;
        float age = u_clicks[i].z;
        if (age >= 1.6) continue;
        float2 cUv = (u_clicks[i].xy - 0.5) * float2(u_mouse.w, 1.0);
        float2 delta = uv - cUv;
        float dist = length(delta);
        float life = saturate(age / 1.6);
        float w = exp(-pow((dist - age * 0.9) / 0.07, 2.0)) * (1.0 - life) * (1.0 - life);
        warp += (dist > 1e-4 ? delta / dist : float2(0.0, 0.0)) * w * 0.04;
        ring += w;
    }

    float2 p = uv + warp;
    float f = dFbm(p + dFbm(p - u_res_time.z * 0.05));
    f += ring * 0.55 + touch * 0.16;
    f = saturate(f * 0.85) * u_vel_intro.z;

    float t = bayer8(int2((int)cell.x, (int)cell.y)) - 0.25;
    float levels = 3.0;
    float v = saturate(f + t / levels);
    v = saturate(v - lerp(0.18, 0.0, ss(0.45, 0.80, v)));
    float q = floor(v * levels + 0.5) / levels;
    float3 col = lerp(u_colors[3].rgb, lerp(u_colors[1].rgb, u_colors[0].rgb, q), q);
    return float4(saturate(col), 1.0);
}
"#;

fn shader_body_for_key(key: &str) -> &'static str {
    match key {
        "rb_aurora" => SHADER_AURORA,
        "rb_silk" => SHADER_SILK,
        "rb_iridescence" => SHADER_IRIDESCENCE,
        "rb_liquid_chrome" => SHADER_LIQUID_CHROME,
        "rb_balatro" => SHADER_BALATRO,
        "rb_dark_veil" => SHADER_DARK_VEIL,
        "rb_plasma" => SHADER_PLASMA,
        "rb_galaxy" => SHADER_GALAXY,
        "rb_light_rays" => SHADER_LIGHT_RAYS,
        "rb_prismatic_burst" => SHADER_PRISMATIC_BURST,
        "rb_molten_metal" => SHADER_MOLTEN_METAL,
        "rb_color_bends" => SHADER_COLOR_BENDS,
        "rb_grainient" => SHADER_GRAINIENT,
        "rb_ripple_grid" => SHADER_RIPPLE_GRID,
        "rb_orb" => SHADER_ORB,
        "rb_dither" => SHADER_DITHER,
        _ => SHADER_AURORA,
    }
}

type D3DCompileFn = unsafe extern "system" fn(
    p_src_data: *const c_void,
    src_data_size: usize,
    p_source_name: PCSTR,
    p_defines: *const c_void,
    p_include: *const c_void,
    p_entry_point: PCSTR,
    p_target: PCSTR,
    flags1: u32,
    flags2: u32,
    pp_code: *mut Option<ID3DBlob>,
    pp_error_msgs: *mut Option<ID3DBlob>,
) -> windows::core::HRESULT;

fn compile_hlsl(src: &str, entry: &str, target: &str) -> Result<Vec<u8>, String> {
    unsafe {
        let dll_name = to_wide("d3dcompiler_47.dll");
        let hmod = LoadLibraryW(PCWSTR(dll_name.as_ptr())).map_err(|e| format!("LoadLibrary d3dcompiler_47: {e}"))?;
        let proc = GetProcAddress(hmod, windows::core::s!("D3DCompile"))
            .ok_or_else(|| "D3DCompile not found".to_string())?;
        let d3d_compile: D3DCompileFn = std::mem::transmute(proc);

        let entry_c = std::ffi::CString::new(entry).unwrap();
        let target_c = std::ffi::CString::new(target).unwrap();
        let mut blob: Option<ID3DBlob> = None;
        let mut err_blob: Option<ID3DBlob> = None;
        let hr = d3d_compile(
            src.as_ptr() as *const c_void,
            src.len(),
            PCSTR::null(),
            std::ptr::null(),
            std::ptr::null(),
            PCSTR(entry_c.as_ptr() as *const u8),
            PCSTR(target_c.as_ptr() as *const u8),
            1 << 15, // D3DCOMPILE_OPTIMIZATION_LEVEL3
            0,
            &mut blob,
            &mut err_blob,
        );
        if hr.is_err() {
            let msg = err_blob
                .map(|b| {
                    let slice = std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize());
                    String::from_utf8_lossy(slice).into_owned()
                })
                .unwrap_or_else(|| format!("HRESULT 0x{:08X}", hr.0 as u32));
            return Err(format!("HLSL compile {entry}/{target} failed: {msg}"));
        }
        let b = blob.ok_or_else(|| "empty shader blob".to_string())?;
        let slice = std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize());
        Ok(slice.to_vec())
    }
}

/// Compiles ALL 16 React Bits HLSL pixel shaders (plus shared vertex shader) via `d3dcompiler_47.dll`.
/// Used by `AuraWallpaper.exe --verify-all-shaders` to guarantee 0 HLSL compile errors across the entire catalog!
pub fn verify_all_shaders() -> Result<usize, String> {
    let vs_src = format!("{}\n{}", RB_COMMON_HLSL, SHADER_AURORA);
    let _ = compile_hlsl(&vs_src, "VSMain", "vs_5_0")?;
    let mut ok = 0usize;
    for preset in &reactbits_shaders::PRESETS {
        let full = format!("{}\n{}", RB_COMMON_HLSL, shader_body_for_key(preset.key));
        compile_hlsl(&full, "PSMain", "ps_5_0")
            .map_err(|e| format!("Shader '{}' failed: {e}", preset.key))?;
        ok += 1;
    }
    Ok(ok)
}

#[derive(Clone, Copy, Default)]
struct ClickImpulse {
    x: f32,
    y: f32,
    age: f32,
    active: bool,
}

fn rgb_norm(c: (u8, u8, u8)) -> [f32; 4] {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0, 1.0]
}

pub struct GpuReactBitsRenderer {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    cb: ID3D11Buffer,
    rs: ID3D11RasterizerState,
    swap_chain: Option<IDXGISwapChain1>,
    _dcomp_dev: Option<IDCompositionDevice>,
    _dcomp_target: Option<IDCompositionTarget>,
    _dcomp_visual: Option<IDCompositionVisual>,
    offscreen_tex: Option<ID3D11Texture2D>,
    rtv: ID3D11RenderTargetView,

    pub width: u32,
    pub height: u32,
    pub preset: ReactBitsPreset,
    pub time: f32,
    pub clock: f32,
    pub intro: f32,
    mx: f32,
    my: f32,
    vx: f32,
    vy: f32,
    presence: f32,
    warp_energy: f32,
    clicks: [ClickImpulse; MAX_CLICKS],
    next_click: usize,
}

impl GpuReactBitsRenderer {
    fn create_device() -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
        unsafe {
            let mut dev = None;
            let mut ctx = None;
            let mut hr = D3D11CreateDevice(
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
            if hr.is_err() {
                hr = D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut dev),
                    None,
                    Some(&mut ctx),
                );
            }
            hr.map_err(|e| format!("D3D11CreateDevice: {e}"))?;
            Ok((dev.unwrap(), ctx.unwrap()))
        }
    }

    fn build_pipeline(
        device: &ID3D11Device,
        key: &str,
    ) -> Result<(ID3D11VertexShader, ID3D11PixelShader, ID3D11Buffer, ID3D11RasterizerState), String> {
        let full_src = format!("{}\n{}", RB_COMMON_HLSL, shader_body_for_key(key));
        let vs_bytes = compile_hlsl(&full_src, "VSMain", "vs_5_0")?;
        let ps_bytes = compile_hlsl(&full_src, "PSMain", "ps_5_0")?;
        unsafe {
            let mut vs = None;
            device.CreateVertexShader(&vs_bytes, None, Some(&mut vs)).map_err(|e| format!("VS: {e}"))?;
            let mut ps = None;
            device.CreatePixelShader(&ps_bytes, None, Some(&mut ps)).map_err(|e| format!("PS: {e}"))?;

            let cb_desc = D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<RbConstants>() as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            let mut cb = None;
            device.CreateBuffer(&cb_desc, None, Some(&mut cb)).map_err(|e| format!("CB: {e}"))?;

            let rs_desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                ..Default::default()
            };
            let mut rs = None;
            device.CreateRasterizerState(&rs_desc, Some(&mut rs)).map_err(|e| format!("RS: {e}"))?;
            Ok((vs.unwrap(), ps.unwrap(), cb.unwrap(), rs.unwrap()))
        }
    }

    pub fn new_for_hwnd(hwnd: HWND, width: u32, height: u32, preset: &ReactBitsPreset) -> Result<Self, String> {
        let (device, ctx) = Self::create_device()?;
        let (vs, ps, cb, rs) = Self::build_pipeline(&device, preset.key)?;
        let w = width.max(64);
        let h = height.max(64);
        unsafe {
            let dxgi_dev: IDXGIDevice = device.cast().map_err(|e| format!("IDXGIDevice: {e}"))?;
            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))
                .map_err(|e| format!("CreateDXGIFactory2: {e}"))?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: w,
                Height: h,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: BOOL(0),
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: 0,
            };
            let sc = factory
                .CreateSwapChainForComposition(&device, &desc, None)
                .map_err(|e| format!("CreateSwapChainForComposition: {e}"))?;
            let dcomp_dev: IDCompositionDevice =
                DCompositionCreateDevice(&dxgi_dev).map_err(|e| format!("DCompositionCreateDevice: {e}"))?;
            let dcomp_target = dcomp_dev
                .CreateTargetForHwnd(hwnd, true)
                .map_err(|e| format!("CreateTargetForHwnd: {e}"))?;
            let dcomp_visual = dcomp_dev.CreateVisual().map_err(|e| format!("CreateVisual: {e}"))?;
            dcomp_visual.SetContent(&sc).map_err(|e| format!("SetContent: {e}"))?;
            dcomp_target.SetRoot(&dcomp_visual).map_err(|e| format!("SetRoot: {e}"))?;
            dcomp_dev.Commit().map_err(|e| format!("Commit: {e}"))?;

            let bb: ID3D11Texture2D = sc.GetBuffer(0).map_err(|e| format!("GetBuffer: {e}"))?;
            let mut rtv = None;
            device.CreateRenderTargetView(&bb, None, Some(&mut rtv)).map_err(|e| format!("RTV: {e}"))?;

            Ok(Self {
                device,
                ctx,
                vs,
                ps,
                cb,
                rs,
                swap_chain: Some(sc),
                _dcomp_dev: Some(dcomp_dev),
                _dcomp_target: Some(dcomp_target),
                _dcomp_visual: Some(dcomp_visual),
                offscreen_tex: None,
                rtv: rtv.unwrap(),
                width: w,
                height: h,
                preset: *preset,
                time: 2.0,
                clock: 0.0,
                intro: 0.0,
                mx: 0.5,
                my: 0.5,
                vx: 0.0,
                vy: 0.0,
                presence: 0.0,
                warp_energy: 0.0,
                clicks: [ClickImpulse::default(); MAX_CLICKS],
                next_click: 0,
            })
        }
    }

    pub fn new_offscreen(width: u32, height: u32, preset: &ReactBitsPreset) -> Result<Self, String> {
        let (device, ctx) = Self::create_device()?;
        let (vs, ps, cb, rs) = Self::build_pipeline(&device, preset.key)?;
        let w = width.max(64);
        let h = height.max(64);
        unsafe {
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
            device.CreateTexture2D(&td, None, Some(&mut tex)).map_err(|e| format!("Tex2D: {e}"))?;
            let tex = tex.unwrap();
            let mut rtv = None;
            device.CreateRenderTargetView(&tex, None, Some(&mut rtv)).map_err(|e| format!("RTV: {e}"))?;

            Ok(Self {
                device,
                ctx,
                vs,
                ps,
                cb,
                rs,
                swap_chain: None,
                _dcomp_dev: None,
                _dcomp_target: None,
                _dcomp_visual: None,
                offscreen_tex: Some(tex),
                rtv: rtv.unwrap(),
                width: w,
                height: h,
                preset: *preset,
                time: 4.5,
                clock: 4.5,
                intro: 1.0,
                mx: 0.55,
                my: 0.52,
                vx: 0.0,
                vy: 0.0,
                presence: 0.85,
                warp_energy: 0.0,
                clicks: [ClickImpulse::default(); MAX_CLICKS],
                next_click: 0,
            })
        }
    }

    /// Hot-swaps the active React Bits HLSL pixel shader inside the running worker (~12 ms, zero restart!).
    pub fn switch_preset(&mut self, preset: &ReactBitsPreset) -> Result<(), String> {
        if self.preset.key == preset.key {
            return Ok(());
        }
        let full_src = format!("{}\n{}", RB_COMMON_HLSL, shader_body_for_key(preset.key));
        let ps_bytes = compile_hlsl(&full_src, "PSMain", "ps_5_0")?;
        unsafe {
            let mut ps = None;
            self.device
                .CreatePixelShader(&ps_bytes, None, Some(&mut ps))
                .map_err(|e| format!("PS swap: {e}"))?;
            self.ps = ps.unwrap();
        }
        self.preset = *preset;
        self.intro = 0.0;
        self.clicks = [ClickImpulse::default(); MAX_CLICKS];
        self.warp_energy = 0.0;
        Ok(())
    }

    pub fn update_mouse(&mut self, u: f32, v: f32, on_desktop: bool, dt: f32) {
        let dt = dt.clamp(0.001, 0.05);
        let target_x = u.clamp(0.0, 1.0);
        let target_y = (1.0 - v).clamp(0.0, 1.0); // bottom-up WebGL convention
        let follow = 1.0 - (-dt * 10.0).exp();
        let prev_x = self.mx;
        let prev_y = self.my;
        self.mx += (target_x - self.mx) * follow;
        self.my += (target_y - self.my) * follow;
        let raw_vx = ((self.mx - prev_x) / dt).clamp(-4.0, 4.0);
        let raw_vy = ((self.my - prev_y) / dt).clamp(-4.0, 4.0);
        let v_blend = 1.0 - (-dt * 6.0).exp();
        self.vx += (raw_vx - self.vx) * v_blend;
        self.vy += (raw_vy - self.vy) * v_blend;
        let goal = if on_desktop { 1.0 } else { 0.0 };
        let tau = if on_desktop { 0.22 } else { 0.55 };
        self.presence += (goal - self.presence) * (1.0 - (-dt / tau).exp());
    }

    pub fn spawn_click(&mut self, u: f32, v: f32) {
        self.clicks[self.next_click] = ClickImpulse {
            x: u.clamp(0.0, 1.0),
            y: (1.0 - v).clamp(0.0, 1.0),
            age: 0.0,
            active: true,
        };
        self.next_click = (self.next_click + 1) % MAX_CLICKS;
        self.warp_energy = (self.warp_energy + 0.85).min(1.0);
    }

    pub fn render_frame(&mut self, dt: f32, sync_interval: u32) {
        let dt = dt.clamp(0.0, 0.05);
        self.clock += dt;
        self.time += dt * self.preset.speed;
        self.intro = (self.intro + dt / 1.4).min(1.0);
        self.warp_energy *= (-dt * 2.2).exp();
        if self.warp_energy < 0.002 {
            self.warp_energy = 0.0;
        }
        let mut clicks_cb = [[0.0f32; 4]; MAX_CLICKS];
        for (i, c) in self.clicks.iter_mut().enumerate() {
            if c.active {
                c.age += dt;
                if c.age >= CLICK_LIFE {
                    c.active = false;
                } else {
                    clicks_cb[i] = [c.x, c.y, c.age, 1.0];
                }
            }
        }
        let intro_eased = self.intro * self.intro * (3.0 - 2.0 * self.intro);
        let cb_data = RbConstants {
            u_res_time: [self.width as f32, self.height as f32, self.time, self.clock],
            u_mouse: [
                self.mx,
                self.my,
                self.presence,
                self.width as f32 / (self.height.max(1) as f32),
            ],
            u_vel_intro: [self.vx, self.vy, intro_eased, self.warp_energy],
            u_clicks: clicks_cb,
            u_colors: [
                rgb_norm(self.preset.colors[0]),
                rgb_norm(self.preset.colors[1]),
                rgb_norm(self.preset.colors[2]),
                rgb_norm(self.preset.colors[3]),
            ],
        };

        unsafe {
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            if self.ctx.Map(&self.cb, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped)).is_ok() {
                std::ptr::copy_nonoverlapping(&cb_data as *const RbConstants, mapped.pData as *mut RbConstants, 1);
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
            self.device.CreateTexture2D(&sd, None, Some(&mut staging)).map_err(|e| format!("Staging: {e}"))?;
            let staging = staging.unwrap();
            self.ctx.CopyResource(&staging, src_tex);

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).map_err(|e| format!("Map: {e}"))?;
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
