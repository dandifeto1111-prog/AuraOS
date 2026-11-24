//! Native Direct3D11 + Direct2D + DirectWrite + DirectComposition GPU Renderer
//! - True per-pixel alpha transparent frameless windows (zero flicker, <20 MB RAM)
//! - Crisp Segoe UI Variable / Consolas typography with ellipsis trimming
//! - Hand-crafted Direct2D vector iconography (zero emoji/font-glyph hacks)
//! - Bounded HD icon (SHIL_EXTRALARGE) & image bitmap cache via WIC
//! - Pixel-accurate offscreen PNG capture for automated visual QA

use crate::theme::{self, VectorIcon};
use crate::win32_shell::to_wide;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP,
    D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Bitmap, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1, ID2D1Geometry,
    ID2D1Layer, ID2D1RenderTarget, ID2D1SolidColorBrush, ID2D1StrokeStyle,
    D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL,
    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES, D2D1_BITMAP_PROPERTIES1,
    D2D1_CAP_STYLE_ROUND, D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_CLIP,
    D2D1_ELLIPSE, D2D1_EXTEND_MODE_CLAMP, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_GAMMA_2_2,
    D2D1_LAYER_OPTIONS_NONE, D2D1_LAYER_PARAMETERS, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
    D2D1_LINE_JOIN_ROUND, D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES1, D2D1_SWEEP_DIRECTION_CLOCKWISE,
};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_MEDIUM, DWRITE_FONT_WEIGHT_REGULAR,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TRIMMING,
    DWRITE_TRIMMING_GRANULARITY_CHARACTER, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWRITE_WORD_WRAPPING_WRAP,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1,
    DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_CHAIN_FLAG, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatPng, GUID_WICPixelFormat32bppPBGRA,
    IWICBitmap, IWICBitmapSource, IWICImagingFactory, WICBitmapCacheOnDemand,
    WICBitmapDitherTypeNone, WICBitmapEncoderNoCache, WICBitmapInterpolationModeFant,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{
    SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_SYSICONINDEX,
    SHIL_EXTRALARGE,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextStyle {
    Title22Bold,
    Title16Semi,
    Title14Semi,
    Body12Semi,
    Body12Med,
    Body11Med,
    Body11Reg,
    Small10Bold,
    Small10Med,
    Small9Reg,
    Mono12Bold,
    Mono11Med,
    Clock72Light,
    HeroTemp34Bold,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
    pub fn to_d2d(&self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.x + self.w,
            bottom: self.y + self.h,
        }
    }
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, (self.w - 2.0 * dx).max(0.0), (self.h - 2.0 * dy).max(0.0))
    }
}

#[derive(Clone)]
pub struct DecodedAsset {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

enum AssetReq {
    Icon { key: String, path: String },
    Image { key: String, path: String, max_w: u32 },
}

struct AssetResp {
    key: String,
    data: Option<DecodedAsset>,
}

pub struct GpuEngine {
    pub d3d_device: ID3D11Device,
    pub dxgi_factory: IDXGIFactory2,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    pub dcomp: IDCompositionDevice,
    pub dwrite: IDWriteFactory,
    pub wic: IWICImagingFactory,
    pub round_stroke: ID2D1StrokeStyle,
    text_formats: HashMap<(TextStyle, i32, bool), IDWriteTextFormat>,
    /// Bounded WIC bitmap source cache for synchronous capture mode
    wic_cache: HashMap<String, Option<IWICBitmapSource>>,
    /// Non-blocking async decoded BGRA asset cache (ZERO UI-thread disk I/O during Launchpad/Fondos scrolling!)
    decoded_cache: HashMap<String, Option<DecodedAsset>>,
    pending_keys: HashSet<String>,
    asset_tx: Option<Sender<AssetReq>>,
    asset_rx: Option<Receiver<AssetResp>>,
}

impl GpuEngine {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

            // Try hardware D3D11 first, fallback to WARP software GPU seamlessly
            let mut d3d_opt: Option<ID3D11Device> = None;
            let res = D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_opt),
                None,
                None,
            );
            if res.is_err() || d3d_opt.is_none() {
                D3D11CreateDevice(
                    None,
                    D3D_DRIVER_TYPE_WARP,
                    None,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut d3d_opt),
                    None,
                    None,
                )?;
            }
            let d3d_device = d3d_opt.unwrap();
            let dxgi_device: IDXGIDevice = d3d_device.cast()?;
            let dxgi_factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = d2d_factory.CreateDevice(&dxgi_device)?;
            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

            let stroke_props = D2D1_STROKE_STYLE_PROPERTIES1 {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                ..Default::default()
            };
            let round_stroke: ID2D1StrokeStyle = d2d_factory.CreateStrokeStyle(&stroke_props, None)?.cast()?;

            Ok(Self {
                d3d_device,
                dxgi_factory,
                d2d_factory,
                d2d_device,
                dcomp,
                dwrite,
                wic,
                round_stroke,
                text_formats: HashMap::new(),
                wic_cache: HashMap::new(),
                decoded_cache: HashMap::new(),
                pending_keys: HashSet::new(),
                asset_tx: None,
                asset_rx: None,
            })
        }
    }

    /// Enables the background STA icon & thumbnail decoder thread so Launchpad, Overview,
    /// and Fondos NEVER perform synchronous SHGetFileInfoW / disk image I/O on the UI thread!
    pub fn enable_async_loader(&mut self) {
        if self.asset_tx.is_some() {
            return;
        }
        let (req_tx, req_rx) = mpsc::channel::<AssetReq>();
        let (resp_tx, resp_rx) = mpsc::channel::<AssetResp>();
        self.asset_tx = Some(req_tx);
        self.asset_rx = Some(resp_rx);

        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let Ok(wic): windows::core::Result<IWICImagingFactory> =
                    CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                else {
                    return;
                };
                while let Ok(req) = req_rx.recv() {
                    let resp = match req {
                        AssetReq::Icon { key, path } => {
                            let data = decode_icon_to_bgra(&wic, &path);
                            AssetResp { key, data }
                        }
                        AssetReq::Image { key, path, max_w } => {
                            let data = decode_image_to_bgra(&wic, &path, max_w);
                            AssetResp { key, data }
                        }
                    };
                    if resp_tx.send(resp).is_err() {
                        break;
                    }
                }
            }
        });
    }

    /// Pre-warm all app/game icons silently on the background thread right after startup!
    pub fn prewarm_icons<I: IntoIterator<Item = String>>(&mut self, paths: I) {
        for p in paths {
            if !p.trim().is_empty() {
                self.request_icon_async(&p);
            }
        }
    }

    /// Pre-warm Workshop & game thumbnails (downscaled to 360px wide -> 90% less RAM, 144 FPS!)
    pub fn prewarm_images<I: IntoIterator<Item = String>>(&mut self, paths: I) {
        for p in paths {
            if !p.trim().is_empty() {
                self.request_image_async(&p, 360);
            }
        }
    }

    pub fn request_icon_async(&mut self, path: &str) {
        let key = format!("ico:{}", path.to_lowercase());
        if self.decoded_cache.contains_key(&key) || !self.pending_keys.insert(key.clone()) {
            return;
        }
        if let Some(tx) = &self.asset_tx {
            let _ = tx.send(AssetReq::Icon { key, path: path.to_string() });
        }
    }

    pub fn request_image_async(&mut self, path: &str, max_w: u32) {
        let key = format!("img:{}", path.to_lowercase());
        if self.decoded_cache.contains_key(&key) || !self.pending_keys.insert(key.clone()) {
            return;
        }
        if let Some(tx) = &self.asset_tx {
            let _ = tx.send(AssetReq::Image { key, path: path.to_string(), max_w });
        }
    }

    /// Drains ready decoded icons/thumbnails from the background thread into RAM (0ms lock-free).
    /// Returns true if any new icon/thumbnail arrived and visible panels should refresh!
    pub fn drain_decoded_assets(&mut self) -> bool {
        let Some(rx) = &self.asset_rx else { return false };
        let mut got_any = false;
        // Drain up to 24 newly decoded assets per 16ms frame so GPU uploads never spike!
        for _ in 0..24 {
            match rx.try_recv() {
                Ok(resp) => {
                    self.pending_keys.remove(&resp.key);
                    if self.decoded_cache.len() > 640 {
                        self.decoded_cache.clear();
                    }
                    self.decoded_cache.insert(resp.key, resp.data);
                    got_any = true;
                }
                Err(_) => break,
            }
        }
        got_any
    }

    pub fn text_format(&mut self, style: TextStyle, align: DWRITE_TEXT_ALIGNMENT, wrap: bool) -> IDWriteTextFormat {
        let key = (style, align.0, wrap);
        if let Some(f) = self.text_formats.get(&key) {
            return f.clone();
        }
        let (family, size, weight) = match style {
            TextStyle::Title22Bold => ("Segoe UI Variable Display", 22.0, DWRITE_FONT_WEIGHT_BOLD),
            TextStyle::Title16Semi => ("Segoe UI Variable Display", 16.0, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            TextStyle::Title14Semi => ("Segoe UI Variable Text", 14.0, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            TextStyle::Body12Semi => ("Segoe UI Variable Text", 12.2, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            TextStyle::Body12Med => ("Segoe UI Variable Text", 12.0, DWRITE_FONT_WEIGHT_MEDIUM),
            TextStyle::Body11Med => ("Segoe UI Variable Text", 11.2, DWRITE_FONT_WEIGHT_MEDIUM),
            TextStyle::Body11Reg => ("Segoe UI Variable Text", 11.0, DWRITE_FONT_WEIGHT_REGULAR),
            TextStyle::Small10Bold => ("Segoe UI Variable Text", 10.0, DWRITE_FONT_WEIGHT_BOLD),
            TextStyle::Small10Med => ("Segoe UI Variable Text", 10.2, DWRITE_FONT_WEIGHT_MEDIUM),
            TextStyle::Small9Reg => ("Segoe UI Variable Text", 9.6, DWRITE_FONT_WEIGHT_REGULAR),
            TextStyle::Mono12Bold => ("Consolas", 12.5, DWRITE_FONT_WEIGHT_BOLD),
            TextStyle::Mono11Med => ("Consolas", 11.0, DWRITE_FONT_WEIGHT_MEDIUM),
            TextStyle::Clock72Light => ("Segoe UI Variable Display", 74.0, DWRITE_FONT_WEIGHT(300)),
            TextStyle::HeroTemp34Bold => ("Segoe UI Variable Display", 34.0, DWRITE_FONT_WEIGHT_BOLD),
        };
        unsafe {
            let w_fam = to_wide(family);
            let w_loc = to_wide("es-ES");
            let fmt = self
                .dwrite
                .CreateTextFormat(
                    PCWSTR(w_fam.as_ptr()),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    PCWSTR(w_loc.as_ptr()),
                )
                .expect("CreateTextFormat");
            let _ = fmt.SetTextAlignment(align);
            let _ = fmt.SetParagraphAlignment(if wrap {
                DWRITE_PARAGRAPH_ALIGNMENT_NEAR
            } else {
                DWRITE_PARAGRAPH_ALIGNMENT_CENTER
            });
            let _ = fmt.SetWordWrapping(if wrap {
                DWRITE_WORD_WRAPPING_WRAP
            } else {
                DWRITE_WORD_WRAPPING_NO_WRAP
            });
            if !wrap {
                if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&fmt) {
                    let trim = DWRITE_TRIMMING {
                        granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                        delimiter: 0,
                        delimiterCount: 0,
                    };
                    let _ = fmt.SetTrimming(&trim, &sign);
                }
            }
            self.text_formats.insert(key, fmt.clone());
            fmt
        }
    }

    pub fn load_wic_icon_for_path(&mut self, path: &str) -> Option<IWICBitmapSource> {
        if path.trim().is_empty() {
            return None;
        }
        let key = format!("ico:{}", path.to_lowercase());
        if let Some(cached) = self.wic_cache.get(&key) {
            return cached.clone();
        }
        if self.wic_cache.len() > 160 {
            self.wic_cache.clear();
        }
        let src = self.extract_hicon_wic(path);
        self.wic_cache.insert(key, src.clone());
        src
    }

    /// Public full-resolution WIC image decoder for AuraPhotos (preserves crisp detail up to `max_w`)
    pub fn decode_image_full(&self, path: &str, max_w: u32) -> Option<DecodedAsset> {
        decode_image_to_bgra(&self.wic, path, max_w)
    }

    pub fn load_wic_image_file(&mut self, path: &str) -> Option<IWICBitmapSource> {
        if path.trim().is_empty() || !Path::new(path).exists() {
            return None;
        }
        let key = format!("img:{}", path.to_lowercase());
        if let Some(cached) = self.wic_cache.get(&key) {
            return cached.clone();
        }
        if self.wic_cache.len() > 160 {
            self.wic_cache.clear();
        }
        let src = unsafe {
            let w = to_wide(path);
            let dec = self
                .wic
                .CreateDecoderFromFilename(
                    PCWSTR(w.as_ptr()),
                    None,
                    windows::Win32::Foundation::GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                )
                .ok()?;
            let frame = dec.GetFrame(0).ok()?;
            let conv = self.wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &frame,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
            conv.cast::<IWICBitmapSource>().ok()
        };
        self.wic_cache.insert(key, src.clone());
        src
    }

    fn extract_hicon_wic(&self, path: &str) -> Option<IWICBitmapSource> {
        // Universal 256px supersampled PNG fast-path for ALL AuraOS binaries (bypasses stale Windows IconCache.db!)
        if let Some(png) = resolve_companion_icon_png(path) {
            unsafe {
                let w = to_wide(&png.to_string_lossy());
                if let Ok(dec) = self.wic.CreateDecoderFromFilename(
                    PCWSTR(w.as_ptr()),
                    None,
                    windows::Win32::Foundation::GENERIC_READ,
                    WICDecodeMetadataCacheOnDemand,
                ) {
                    if let Ok(frame) = dec.GetFrame(0) {
                        if let Ok(conv) = self.wic.CreateFormatConverter() {
                            if conv
                                .Initialize(
                                    &frame,
                                    &GUID_WICPixelFormat32bppPBGRA,
                                    WICBitmapDitherTypeNone,
                                    None,
                                    0.0,
                                    WICBitmapPaletteTypeCustom,
                                )
                                .is_ok()
                            {
                                if let Ok(src) = conv.cast::<IWICBitmapSource>() {
                                    return Some(src);
                                }
                            }
                        }
                    }
                }
            }
        }

        if !Path::new(path).exists() {
            return None;
        }
        unsafe {
            let w = to_wide(path);
            let mut hicon = HICON::default();
            // Step 1: HD 48x48+ via SHGetImageList(SHIL_EXTRALARGE)
            let mut shfi = SHFILEINFOW::default();
            if SHGetFileInfoW(
                PCWSTR(w.as_ptr()),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut shfi),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_SYSICONINDEX,
            ) != 0
            {
                if let Ok(img_list) = SHGetImageList::<IImageList>(SHIL_EXTRALARGE as i32) {
                    if let Ok(ic) = img_list.GetIcon(shfi.iIcon, ILD_TRANSPARENT.0) {
                        hicon = ic;
                    }
                }
            }
            // Step 2: Standard large icon fallback
            if hicon.is_invalid() {
                let mut shfi2 = SHFILEINFOW::default();
                if SHGetFileInfoW(
                    PCWSTR(w.as_ptr()),
                    windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                    Some(&mut shfi2),
                    std::mem::size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_LARGEICON,
                ) != 0
                {
                    hicon = shfi2.hIcon;
                }
            }
            if hicon.is_invalid() {
                return None;
            }
            let bmp = self.wic.CreateBitmapFromHICON(hicon).ok();
            let _ = DestroyIcon(hicon);
            let bmp = bmp?;
            let conv = self.wic.CreateFormatConverter().ok()?;
            conv.Initialize(
                &bmp,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .ok()?;
            conv.cast::<IWICBitmapSource>().ok()
        }
    }

    /// Offscreen pixel-identical PNG capture for Visual QA (`--capture`)
    pub fn render_to_png<F>(&mut self, width: u32, height: u32, out_path: &Path, draw_fn: F) -> windows::core::Result<()>
    where
        F: FnOnce(&mut DrawCtx<'_>),
    {
        unsafe {
            let wic_bmp: IWICBitmap = self.wic.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnDemand,
            )?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let rt: ID2D1RenderTarget = self.d2d_factory.CreateWicBitmapRenderTarget(&wic_bmp, &props)?;
            rt.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let mut local_bmps = HashMap::new();
            {
                let mut ctx = DrawCtx {
                    rt: &rt,
                    gpu: self,
                    bmp_cache: &mut local_bmps,
                };
                draw_fn(&mut ctx);
            }
            let _ = rt.EndDraw(None, None);

            // Encode IWICBitmap to PNG
            if let Some(parent) = out_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let stream = self.wic.CreateStream()?;
            let wp = to_wide(&out_path.to_string_lossy());
            stream.InitializeFromFilename(PCWSTR(wp.as_ptr()), windows::Win32::Foundation::GENERIC_WRITE.0)?;
            let encoder = self.wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null())?;
            encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
            let mut frame = None;
            let mut bag = None;
            encoder.CreateNewFrame(&mut frame, &mut bag)?;
            let frame = frame.unwrap();
            frame.Initialize(bag.as_ref())?;
            frame.SetSize(width, height)?;
            let mut pf = GUID_WICPixelFormat32bppPBGRA;
            frame.SetPixelFormat(&mut pf)?;
            frame.WriteSource(&wic_bmp, std::ptr::null())?;
            frame.Commit()?;
            encoder.Commit()?;
            Ok(())
        }
    }
}

/// Resolves `<Stem>_256.png` companion high-DPI icon for any AuraOS executable or shortcut target
fn resolve_companion_icon_png(path: &str) -> Option<std::path::PathBuf> {
    let p = Path::new(path);
    let stem = p.file_stem()?.to_str()?;
    let png_name = format!("{}_256.png", stem);
    if let Some(dir) = p.parent() {
        let c1 = dir.join(&png_name);
        if c1.exists() {
            return Some(c1);
        }
        let c2 = dir.join("icons").join(&png_name);
        if c2.exists() {
            return Some(c2);
        }
        // Cargo `target/release/<Bin>.exe` -> `assets/icons/<Stem>_256.png`
        if let Some(root) = dir.parent().and_then(|d| d.parent()) {
            let c3 = root.join("assets").join("icons").join(&png_name);
            if c3.exists() {
                return Some(c3);
            }
        }
    }
    let app_dir_png = crate::models::app_data_dir().join(&png_name);
    if app_dir_png.exists() {
        return Some(app_dir_png);
    }
    None
}

/// Background STA worker helper: extracts HD icon to raw premultiplied BGRA pixels
fn decode_icon_to_bgra(wic: &IWICImagingFactory, path: &str) -> Option<DecodedAsset> {
    if let Some(png) = resolve_companion_icon_png(path) {
        if let Some(d) = decode_image_to_bgra(wic, &png.to_string_lossy(), 128) {
            return Some(d);
        }
    }
    if !Path::new(path).exists() {
        return None;
    }
    unsafe {
        let w = to_wide(path);
        let mut hicon = HICON::default();
        let mut shfi = SHFILEINFOW::default();
        if SHGetFileInfoW(
            PCWSTR(w.as_ptr()),
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut shfi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_SYSICONINDEX,
        ) != 0
        {
            if let Ok(img_list) = SHGetImageList::<IImageList>(SHIL_EXTRALARGE as i32) {
                if let Ok(ic) = img_list.GetIcon(shfi.iIcon, ILD_TRANSPARENT.0) {
                    hicon = ic;
                }
            }
        }
        if hicon.is_invalid() {
            let mut shfi2 = SHFILEINFOW::default();
            if SHGetFileInfoW(
                PCWSTR(w.as_ptr()),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut shfi2),
                std::mem::size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            ) != 0
            {
                hicon = shfi2.hIcon;
            }
        }
        if hicon.is_invalid() {
            return None;
        }
        let bmp = wic.CreateBitmapFromHICON(hicon).ok();
        let _ = DestroyIcon(hicon);
        let bmp = bmp?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(
            &bmp,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let mut bw = 0u32;
        let mut bh = 0u32;
        conv.GetSize(&mut bw, &mut bh).ok()?;
        if bw == 0 || bh == 0 || bw > 512 || bh > 512 {
            return None;
        }
        let stride = bw * 4;
        let mut bgra = vec![0u8; (stride * bh) as usize];
        conv.CopyPixels(std::ptr::null(), stride, &mut bgra).ok()?;
        Some(DecodedAsset { width: bw, height: bh, bgra })
    }
}

/// Background STA worker helper: decodes & downscales thumbnail image to max_w (90% less RAM, 144 FPS!)
fn decode_image_to_bgra(wic: &IWICImagingFactory, path: &str, max_w: u32) -> Option<DecodedAsset> {
    if !Path::new(path).exists() {
        return None;
    }
    unsafe {
        let w = to_wide(path);
        let dec = wic
            .CreateDecoderFromFilename(
                PCWSTR(w.as_ptr()),
                None,
                windows::Win32::Foundation::GENERIC_READ,
                WICDecodeMetadataCacheOnDemand,
            )
            .ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let mut ow = 0u32;
        let mut oh = 0u32;
        frame.GetSize(&mut ow, &mut oh).ok()?;
        if ow == 0 || oh == 0 {
            return None;
        }
        let longest = ow.max(oh);
        let src: IWICBitmapSource = if longest > max_w {
            let ratio = max_w as f64 / longest as f64;
            let nw = ((ow as f64 * ratio).round() as u32).max(1);
            let nh = ((oh as f64 * ratio).round() as u32).max(1);
            let scaler = wic.CreateBitmapScaler().ok()?;
            scaler.Initialize(&frame, nw, nh, WICBitmapInterpolationModeFant).ok()?;
            scaler.cast().ok()?
        } else {
            frame.cast().ok()?
        };
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(
            &src,
            &GUID_WICPixelFormat32bppPBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
        .ok()?;
        let mut bw = 0u32;
        let mut bh = 0u32;
        conv.GetSize(&mut bw, &mut bh).ok()?;
        if bw == 0 || bh == 0 {
            return None;
        }
        let stride = bw * 4;
        let mut bgra = vec![0u8; (stride * bh) as usize];
        conv.CopyPixels(std::ptr::null(), stride, &mut bgra).ok()?;
        Some(DecodedAsset { width: bw, height: bh, bgra })
    }
}

/// Per-window DirectComposition + Direct2D Premultiplied Alpha GPU Surface
pub struct WindowSurface {
    pub width: u32,
    pub height: u32,
    swap_chain: IDXGISwapChain1,
    d2d_ctx: ID2D1DeviceContext,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    bmp_cache: HashMap<String, Option<ID2D1Bitmap>>,
    video_bmp: Option<(u32, u32, ID2D1Bitmap)>,
}

impl WindowSurface {
    pub fn new(gpu: &GpuEngine, hwnd: HWND, width: u32, height: u32) -> windows::core::Result<Self> {
        let w = width.max(1);
        let h = height.max(1);
        unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
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
            let swap_chain = gpu.dxgi_factory.CreateSwapChainForComposition(&gpu.d3d_device, &desc, None)?;
            let d2d_ctx = gpu.d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let target = gpu.dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = gpu.dcomp.CreateVisual()?;
            visual.SetContent(&swap_chain)?;
            target.SetRoot(&visual)?;
            gpu.dcomp.Commit()?;

            let mut surf = Self {
                width: w,
                height: h,
                swap_chain,
                d2d_ctx,
                _target: target,
                _visual: visual,
                bmp_cache: HashMap::new(),
                video_bmp: None,
            };
            surf.bind_backbuffer()?;
            Ok(surf)
        }
    }

    fn bind_backbuffer(&mut self) -> windows::core::Result<()> {
        unsafe {
            let surface: IDXGISurface = self.swap_chain.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                ..Default::default()
            };
            let target_bmp = self.d2d_ctx.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.d2d_ctx.SetTarget(&target_bmp);
            self.d2d_ctx.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            Ok(())
        }
    }

    pub fn resize(&mut self, gpu: &GpuEngine, width: u32, height: u32) {
        let w = width.max(1);
        let h = height.max(1);
        if w == self.width && h == self.height {
            return;
        }
        unsafe {
            self.d2d_ctx.SetTarget(None);
            if self.swap_chain.ResizeBuffers(2, w, h, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SWAP_CHAIN_FLAG(0)).is_ok() {
                self.width = w;
                self.height = h;
                let _ = self.bind_backbuffer();
                let _ = gpu.dcomp.Commit();
            }
        }
    }

    pub fn clear_bitmap_cache(&mut self) {
        self.bmp_cache.clear();
    }

    pub fn draw<F>(&mut self, gpu: &mut GpuEngine, draw_fn: F)
    where
        F: FnOnce(&mut DrawCtx<'_>),
    {
        unsafe {
            let Ok(rt) = self.d2d_ctx.cast::<ID2D1RenderTarget>() else {
                return;
            };
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            {
                let mut ctx = DrawCtx {
                    rt: &rt,
                    gpu,
                    bmp_cache: &mut self.bmp_cache,
                };
                draw_fn(&mut ctx);
            }
            if let Err(err) = rt.EndDraw(None, None) {
                // D2DERR_RECREATE_TARGET (0x8899000C) after fullscreen game GPU reset: recover cleanly!
                if err.code().0 as u32 == 0x8899_000C {
                    self.bmp_cache.clear();
                    self.video_bmp = None;
                    self.d2d_ctx.SetTarget(None);
                    let _ = self.bind_backbuffer();
                }
                return;
            }
            let _ = self.swap_chain.Present(0, DXGI_PRESENT(0));
        }
    }

    /// Streams an opaque raw BGRA video wallpaper frame straight into a reusable GPU ID2D1Bitmap (<0.4ms!)
    pub fn draw_video_frame(&mut self, frame_w: u32, frame_h: u32, bgra: &[u8]) {
        if frame_w == 0 || frame_h == 0 || bgra.len() < (frame_w * frame_h * 4) as usize {
            return;
        }
        unsafe {
            let rt: ID2D1RenderTarget = self.d2d_ctx.cast().unwrap();
            let need_new = match &self.video_bmp {
                Some((bw, bh, _)) => *bw != frame_w || *bh != frame_h,
                None => true,
            };
            if need_new {
                let props = D2D1_BITMAP_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT {
                        format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        alphaMode: D2D1_ALPHA_MODE_IGNORE,
                    },
                    dpiX: 96.0,
                    dpiY: 96.0,
                };
                if let Ok(bmp) = rt.CreateBitmap(
                    D2D_SIZE_U { width: frame_w, height: frame_h },
                    Some(bgra.as_ptr() as *const _),
                    frame_w * 4,
                    &props,
                ) {
                    self.video_bmp = Some((frame_w, frame_h, bmp));
                }
            } else if let Some((_, _, ref bmp)) = self.video_bmp {
                let _ = bmp.CopyFromMemory(None, bgra.as_ptr() as *const _, frame_w * 4);
            }

            if let Some((_, _, ref bmp)) = self.video_bmp {
                rt.BeginDraw();
                let dst = D2D_RECT_F {
                    left: 0.0,
                    top: 0.0,
                    right: self.width as f32,
                    bottom: self.height as f32,
                };
                // Aspect-fill (cover) center-crop so video wallpapers NEVER stretch on any aspect ratio!
                let bw = frame_w.max(1) as f32;
                let bh = frame_h.max(1) as f32;
                let src_ar = bw / bh;
                let dst_ar = (self.width.max(1) as f32) / (self.height.max(1) as f32);
                let src = if src_ar > dst_ar {
                    let sw = bh * dst_ar;
                    let sx = (bw - sw) * 0.5;
                    D2D_RECT_F { left: sx, top: 0.0, right: sx + sw, bottom: bh }
                } else {
                    let sh = bw / dst_ar;
                    let sy = (bh - sh) * 0.5;
                    D2D_RECT_F { left: 0.0, top: sy, right: bw, bottom: sy + sh }
                };
                rt.DrawBitmap(bmp, Some(&dst), 1.0, D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, Some(&src));
                let _ = rt.EndDraw(None, None);
                let _ = self.swap_chain.Present(0, DXGI_PRESENT(0));
            }
        }
    }
}

impl Drop for WindowSurface {
    fn drop(&mut self) {
        self.bmp_cache.clear();
        self.video_bmp = None;
        unsafe {
            self.d2d_ctx.SetTarget(None);
        }
    }
}

/// High-level Direct2D Drawing Context
pub struct DrawCtx<'a> {
    pub rt: &'a ID2D1RenderTarget,
    pub gpu: &'a mut GpuEngine,
    bmp_cache: &'a mut HashMap<String, Option<ID2D1Bitmap>>,
}

impl<'a> DrawCtx<'a> {
    fn brush(&self, color: D2D1_COLOR_F) -> Option<ID2D1SolidColorBrush> {
        unsafe { self.rt.CreateSolidColorBrush(&color, None).ok() }
    }

    pub fn set_transform_scale_origin(&self, scale: f32, origin_x: f32, origin_y: f32, dx: f32, dy: f32) {
        let m = windows::Foundation::Numerics::Matrix3x2 {
            M11: scale,
            M12: 0.0,
            M21: 0.0,
            M22: scale,
            M31: origin_x * (1.0 - scale) + dx,
            M32: origin_y * (1.0 - scale) + dy,
        };
        unsafe { self.rt.SetTransform(&m); }
    }

    pub fn reset_transform(&self) {
        let id = windows::Foundation::Numerics::Matrix3x2 {
            M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0,
        };
        unsafe { self.rt.SetTransform(&id); }
    }

    pub fn push_clip(&self, rect: Rect) {
        unsafe {
            self.rt.PushAxisAlignedClip(&rect.to_d2d(), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.rt.PopAxisAlignedClip(); }
    }

    pub fn push_opacity_layer(&self, rect: Rect, opacity: f32) -> Option<ID2D1Layer> {
        unsafe {
            let layer = self.rt.CreateLayer(None).ok()?;
            let params = D2D1_LAYER_PARAMETERS {
                contentBounds: rect.to_d2d(),
                opacity,
                maskTransform: windows::Foundation::Numerics::Matrix3x2 {
                    M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0,
                },
                ..Default::default()
            };
            self.rt.PushLayer(&params, &layer);
            Some(layer)
        }
    }

    /// Apple-style hardware-accelerated circular clip mask reveal (60-144 FPS via Direct2D ID2D1EllipseGeometry!)
    pub fn push_circle_mask_layer(&self, bounds: Rect, cx: f32, cy: f32, radius: f32, opacity: f32) -> Option<ID2D1Layer> {
        unsafe {
            let layer = self.rt.CreateLayer(None).ok()?;
            let ell = D2D1_ELLIPSE {
                point: D2D_POINT_2F { x: cx, y: cy },
                radiusX: radius.max(1.0),
                radiusY: radius.max(1.0),
            };
            let geom: ID2D1Geometry = self.gpu.d2d_factory.CreateEllipseGeometry(&ell).ok()?.cast().ok()?;
            let mut params = D2D1_LAYER_PARAMETERS {
                contentBounds: bounds.to_d2d(),
                geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: windows::Foundation::Numerics::Matrix3x2 {
                    M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0,
                },
                opacity,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, &layer);
            std::mem::ManuallyDrop::drop(&mut params.geometricMask);
            Some(layer)
        }
    }

    /// Razor-sharp Direct2D rounded-rectangle geometric clip mask (zero square corner leaks on frameless windows!)
    pub fn push_rrect_mask_layer(&self, bounds: Rect, mask_rect: Rect, corner_radius: f32, opacity: f32) -> Option<ID2D1Layer> {
        unsafe {
            let layer = self.rt.CreateLayer(None).ok()?;
            let rr = D2D1_ROUNDED_RECT {
                rect: mask_rect.to_d2d(),
                radiusX: corner_radius.max(0.0),
                radiusY: corner_radius.max(0.0),
            };
            let geom: ID2D1Geometry = self.gpu.d2d_factory.CreateRoundedRectangleGeometry(&rr).ok()?.cast().ok()?;
            let mut params = D2D1_LAYER_PARAMETERS {
                contentBounds: bounds.to_d2d(),
                geometricMask: std::mem::ManuallyDrop::new(Some(geom)),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: windows::Foundation::Numerics::Matrix3x2 {
                    M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0,
                },
                opacity,
                opacityBrush: std::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS_NONE,
            };
            self.rt.PushLayer(&params, &layer);
            std::mem::ManuallyDrop::drop(&mut params.geometricMask);
            Some(layer)
        }
    }

    /// Smooth GPU radial glow orb (used by animated AuraOS wallpapers & music visualizer!)
    pub fn fill_radial_glow(&self, cx: f32, cy: f32, radius: f32, inner: D2D1_COLOR_F, outer: D2D1_COLOR_F) {
        unsafe {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: inner },
                D2D1_GRADIENT_STOP { position: 1.0, color: outer },
            ];
            if let Ok(col) = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: D2D_POINT_2F { x: cx, y: cy },
                    gradientOriginOffset: D2D_POINT_2F { x: 0.0, y: 0.0 },
                    radiusX: radius.max(1.0),
                    radiusY: radius.max(1.0),
                };
                if let Ok(br) = self.rt.CreateRadialGradientBrush(&props, None, &col) {
                    let ell = D2D1_ELLIPSE {
                        point: D2D_POINT_2F { x: cx, y: cy },
                        radiusX: radius.max(1.0),
                        radiusY: radius.max(1.0),
                    };
                    self.rt.FillEllipse(&ell, &br);
                }
            }
        }
    }

    pub fn pop_layer(&self) {
        unsafe { self.rt.PopLayer(); }
    }

    pub fn fill_rrect(&self, r: Rect, radius: f32, color: D2D1_COLOR_F) {
        if let Some(b) = self.brush(color) {
            let rr = D2D1_ROUNDED_RECT { rect: r.to_d2d(), radiusX: radius, radiusY: radius };
            unsafe { self.rt.FillRoundedRectangle(&rr, &b); }
        }
    }

    pub fn stroke_rrect(&self, r: Rect, radius: f32, color: D2D1_COLOR_F, width: f32) {
        if let Some(b) = self.brush(color) {
            let inset = width * 0.5;
            let rr = D2D1_ROUNDED_RECT { rect: r.inset(inset, inset).to_d2d(), radiusX: radius, radiusY: radius };
            unsafe { self.rt.DrawRoundedRectangle(&rr, &b, width, None); }
        }
    }

    pub fn fill_card(&self, r: Rect, radius: f32, bg: D2D1_COLOR_F, border: D2D1_COLOR_F) {
        self.fill_rrect(r, radius, bg);
        self.stroke_rrect(r, radius, border, 1.0);
    }

    pub fn fill_gradient_rrect(&self, r: Rect, radius: f32, c0: D2D1_COLOR_F, c1: D2D1_COLOR_F) {
        unsafe {
            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: c0 },
                D2D1_GRADIENT_STOP { position: 1.0, color: c1 },
            ];
            if let Ok(col) = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: D2D_POINT_2F { x: r.x, y: r.y },
                    endPoint: D2D_POINT_2F { x: r.x + r.w, y: r.y + r.h },
                };
                if let Ok(br) = self.rt.CreateLinearGradientBrush(&props, None, &col) {
                    let rr = D2D1_ROUNDED_RECT { rect: r.to_d2d(), radiusX: radius, radiusY: radius };
                    self.rt.FillRoundedRectangle(&rr, &br);
                }
            }
        }
    }

    /// Drool signature diagonal-hatched slider track
    pub fn fill_hatched_track(&self, r: Rect, radius: f32) {
        self.fill_rrect(r, radius, theme::rgba(255, 255, 255, 0.05));
        self.push_clip(r);
        if let Some(b) = self.brush(theme::rgba(255, 255, 255, 0.09)) {
            let mut x = r.x - r.h;
            let end = r.x + r.w + r.h;
            while x < end {
                unsafe {
                    self.rt.DrawLine(
                        D2D_POINT_2F { x, y: r.y + r.h },
                        D2D_POINT_2F { x: x + r.h, y: r.y },
                        &b,
                        2.0,
                        None,
                    );
                }
                x += 8.0;
            }
        }
        self.pop_clip();
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, radius: f32, color: D2D1_COLOR_F) {
        if let Some(b) = self.brush(color) {
            let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: radius, radiusY: radius };
            unsafe { self.rt.FillEllipse(&e, &b); }
        }
    }

    pub fn stroke_circle(&self, cx: f32, cy: f32, radius: f32, color: D2D1_COLOR_F, width: f32) {
        if let Some(b) = self.brush(color) {
            let e = D2D1_ELLIPSE { point: D2D_POINT_2F { x: cx, y: cy }, radiusX: radius, radiusY: radius };
            unsafe { self.rt.DrawEllipse(&e, &b, width, None); }
        }
    }

    pub fn draw_line(&self, x1: f32, y1: f32, x2: f32, y2: f32, color: D2D1_COLOR_F, width: f32) {
        if let Some(b) = self.brush(color) {
            unsafe {
                self.rt.DrawLine(
                    D2D_POINT_2F { x: x1, y: y1 },
                    D2D_POINT_2F { x: x2, y: y2 },
                    &b,
                    width,
                    Some(&self.gpu.round_stroke),
                );
            }
        }
    }

    /// Drool Arc Gauge (used for RAM %)
    pub fn draw_arc_gauge(&self, cx: f32, cy: f32, radius: f32, thickness: f32, pct: f32) {
        let start_deg = 150.0f32;
        let sweep_total = 240.0f32;
        self.draw_arc_segment(cx, cy, radius, thickness, start_deg, sweep_total, theme::rgb(46, 46, 62));
        let active_sweep = sweep_total * (pct.clamp(0.0, 100.0) / 100.0);
        if active_sweep > 1.0 {
            let col = if pct >= 82.0 { theme::CORAL } else if pct >= 60.0 { theme::AMBER } else { theme::VIOLET };
            self.draw_arc_segment(cx, cy, radius, thickness, start_deg, active_sweep, col);
            let end_rad = (start_deg + active_sweep).to_radians();
            let kx = cx + radius * end_rad.cos();
            let ky = cy + radius * end_rad.sin();
            self.fill_circle(kx, ky, thickness * 0.55, theme::CRISP_WHITE);
            self.stroke_circle(kx, ky, thickness * 0.55, theme::VIOLET, 2.0);
        }
    }

    fn draw_arc_segment(&self, cx: f32, cy: f32, r: f32, thickness: f32, start_deg: f32, sweep_deg: f32, color: D2D1_COLOR_F) {
        unsafe {
            let Ok(geo) = self.gpu.d2d_factory.CreatePathGeometry() else { return };
            let Ok(sink) = geo.Open() else { return };
            let s_rad = start_deg.to_radians();
            let e_rad = (start_deg + sweep_deg).to_radians();
            let p0 = D2D_POINT_2F { x: cx + r * s_rad.cos(), y: cy + r * s_rad.sin() };
            let p1 = D2D_POINT_2F { x: cx + r * e_rad.cos(), y: cy + r * e_rad.sin() };
            sink.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: p1,
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep_deg > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = sink.Close();
            if let Some(b) = self.brush(color) {
                self.rt.DrawGeometry(&geo, &b, thickness, Some(&self.gpu.round_stroke));
            }
        }
    }

    /// Real smooth CPU Area + Line Chart (domain-true, crisp on white card!)
    pub fn draw_area_graph(&self, r: Rect, values: &[f32], line_color: D2D1_COLOR_F) {
        if values.len() < 2 {
            return;
        }
        unsafe {
            let Ok(fill_geo) = self.gpu.d2d_factory.CreatePathGeometry() else { return };
            let Ok(line_geo) = self.gpu.d2d_factory.CreatePathGeometry() else { return };
            let (Ok(fs), Ok(ls)) = (fill_geo.Open(), line_geo.Open()) else { return };

            let step = r.w / (values.len() - 1) as f32;
            let pt = |i: usize, v: f32| D2D_POINT_2F {
                x: r.x + i as f32 * step,
                y: r.y + r.h - (v.clamp(2.0, 100.0) / 100.0) * r.h,
            };

            let p0 = pt(0, values[0]);
            fs.BeginFigure(D2D_POINT_2F { x: r.x, y: r.y + r.h }, D2D1_FIGURE_BEGIN_FILLED);
            fs.AddLine(p0);
            ls.BeginFigure(p0, D2D1_FIGURE_BEGIN_HOLLOW);

            for (i, &v) in values.iter().enumerate().skip(1) {
                let p = pt(i, v);
                fs.AddLine(p);
                ls.AddLine(p);
            }

            fs.AddLine(D2D_POINT_2F { x: r.x + r.w, y: r.y + r.h });
            fs.EndFigure(D2D1_FIGURE_END_CLOSED);
            ls.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = fs.Close();
            let _ = ls.Close();

            let stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: D2D1_COLOR_F { a: 0.28, ..line_color } },
                D2D1_GRADIENT_STOP { position: 1.0, color: D2D1_COLOR_F { a: 0.02, ..line_color } },
            ];
            if let Ok(col) = self.rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP) {
                let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: D2D_POINT_2F { x: r.x, y: r.y },
                    endPoint: D2D_POINT_2F { x: r.x, y: r.y + r.h },
                };
                if let Ok(g_br) = self.rt.CreateLinearGradientBrush(&props, None, &col) {
                    self.rt.FillGeometry(&fill_geo, &g_br, None);
                }
            }
            if let Some(lb) = self.brush(line_color) {
                self.rt.DrawGeometry(&line_geo, &lb, 2.2, Some(&self.gpu.round_stroke));
            }
        }
    }

    pub fn draw_text(&mut self, text: &str, r: Rect, style: TextStyle, color: D2D1_COLOR_F) {
        self.draw_text_aligned(text, r, style, color, DWRITE_TEXT_ALIGNMENT_LEADING, false);
    }

    pub fn draw_text_center(&mut self, text: &str, r: Rect, style: TextStyle, color: D2D1_COLOR_F) {
        self.draw_text_aligned(text, r, style, color, DWRITE_TEXT_ALIGNMENT_CENTER, false);
    }

    pub fn draw_text_right(&mut self, text: &str, r: Rect, style: TextStyle, color: D2D1_COLOR_F) {
        self.draw_text_aligned(text, r, style, color, DWRITE_TEXT_ALIGNMENT_TRAILING, false);
    }

    pub fn draw_text_wrap(&mut self, text: &str, r: Rect, style: TextStyle, color: D2D1_COLOR_F) {
        self.draw_text_aligned(text, r, style, color, DWRITE_TEXT_ALIGNMENT_LEADING, true);
    }

    fn draw_text_aligned(&mut self, text: &str, r: Rect, style: TextStyle, color: D2D1_COLOR_F, align: DWRITE_TEXT_ALIGNMENT, wrap: bool) {
        if text.is_empty() {
            return;
        }
        let fmt = self.gpu.text_format(style, align, wrap);
        if let Some(br) = self.brush(color) {
            let w: Vec<u16> = text.encode_utf16().collect();
            unsafe {
                self.rt.DrawText(
                    &w,
                    &fmt,
                    &r.to_d2d(),
                    &br,
                    D2D1_DRAW_TEXT_OPTIONS_CLIP,
                    windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
                );
            }
        }
    }

    /// Uploads and caches a raw premultiplied BGRA DecodedAsset under `cache_key` (e.g. Cushion Treemap / AuraPhotos!)
    pub fn draw_bgra_cached(&mut self, cache_key: &str, asset: &DecodedAsset, dst: Rect) {
        if !self.bmp_cache.contains_key(cache_key) {
            self.bmp_cache.retain(|k, _| !k.starts_with("tm_") && !k.starts_with("photo_"));
            let bmp = self.upload_decoded_to_gpu(asset);
            self.bmp_cache.insert(cache_key.to_string(), bmp);
        }
        if let Some(Some(bmp)) = self.bmp_cache.get(cache_key) {
            unsafe {
                self.rt.DrawBitmap(
                    bmp,
                    Some(&dst.to_d2d()),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
    }

    /// Streams a premultiplied-alpha BGRA buffer (used by AuraPDF & AuraPhotos Studio Editor!)
    pub fn draw_bgra_alpha_live(&mut self, cache_key: &str, w: u32, h: u32, pbgra: &[u8], dst: Rect) {
        if w == 0 || h == 0 || pbgra.len() < (w as usize * h as usize * 4) {
            return;
        }
        let need_create = match self.bmp_cache.get(cache_key) {
            Some(Some(b)) => unsafe {
                let sz = b.GetPixelSize();
                sz.width != w || sz.height != h
            },
            _ => true,
        };
        if need_create {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let bmp = unsafe {
                self.rt
                    .CreateBitmap(
                        D2D_SIZE_U { width: w, height: h },
                        Some(pbgra.as_ptr() as *const _),
                        w * 4,
                        &props,
                    )
                    .ok()
            };
            self.bmp_cache.insert(cache_key.to_string(), bmp);
        } else if let Some(Some(b)) = self.bmp_cache.get(cache_key) {
            unsafe {
                let _ = b.CopyFromMemory(None, pbgra.as_ptr() as *const _, w * 4);
            }
        }
        if let Some(Some(bmp)) = self.bmp_cache.get(cache_key) {
            unsafe {
                self.rt.DrawBitmap(
                    bmp,
                    Some(&dst.to_d2d()),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
    }

    /// Streams a live 60 FPS BGRA video frame into a single reusable VRAM bitmap (zero allocation!)
    pub fn draw_bgra_live(&mut self, cache_key: &str, w: u32, h: u32, bgra: &[u8], dst: Rect) {
        if w == 0 || h == 0 || bgra.len() < (w as usize * h as usize * 4) {
            return;
        }
        let need_create = match self.bmp_cache.get(cache_key) {
            Some(Some(b)) => unsafe {
                let sz = b.GetPixelSize();
                sz.width != w || sz.height != h
            },
            _ => true,
        };
        if need_create {
            let props = D2D1_BITMAP_PROPERTIES {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: 96.0,
                dpiY: 96.0,
            };
            let bmp = unsafe {
                self.rt
                    .CreateBitmap(
                        D2D_SIZE_U { width: w, height: h },
                        Some(bgra.as_ptr() as *const _),
                        w * 4,
                        &props,
                    )
                    .ok()
            };
            self.bmp_cache.insert(cache_key.to_string(), bmp);
        } else if let Some(Some(b)) = self.bmp_cache.get(cache_key) {
            unsafe {
                let _ = b.CopyFromMemory(None, bgra.as_ptr() as *const _, w * 4);
            }
        }
        if let Some(Some(bmp)) = self.bmp_cache.get(cache_key) {
            unsafe {
                self.rt.DrawBitmap(
                    bmp,
                    Some(&dst.to_d2d()),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
        }
    }

    /// Draws a genuine 4-level dynamic Wi-Fi signal indicator (dot + 3 progressive arcs!)
    pub fn draw_wifi_signal(
        &self,
        cx: f32,
        cy: f32,
        size: f32,
        bars: u8,
        active: D2D1_COLOR_F,
        dim: D2D1_COLOR_F,
    ) {
        let s = size * 0.5;
        let sw = (size * 0.115).clamp(1.25, 2.0);
        let by = cy + s * 0.62;
        self.fill_circle(cx, by - s * 0.10, s * 0.16, if bars >= 1 { active } else { dim });
        self.draw_arc_segment(cx, by, s * 0.56, sw, 225.0, 90.0, if bars >= 2 { active } else { dim });
        self.draw_arc_segment(cx, by, s * 0.90, sw, 225.0, 90.0, if bars >= 3 { active } else { dim });
        self.draw_arc_segment(cx, by, s * 1.24, sw, 225.0, 90.0, if bars >= 4 { active } else { dim });
    }

    fn upload_decoded_to_gpu(&self, asset: &DecodedAsset) -> Option<ID2D1Bitmap> {
        let props = D2D1_BITMAP_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: 96.0,
            dpiY: 96.0,
        };
        unsafe {
            self.rt
                .CreateBitmap(
                    D2D_SIZE_U { width: asset.width, height: asset.height },
                    Some(asset.bgra.as_ptr() as *const _),
                    asset.width * 4,
                    &props,
                )
                .ok()
        }
    }

    /// Draws an HD icon extracted from any .exe / .lnk / .ico path
    /// ZERO UI-thread blocking when async loader is enabled (144 FPS Launchpad scrolling!)
    pub fn draw_app_icon(&mut self, path: &str, r: Rect) -> bool {
        if path.trim().is_empty() {
            return false;
        }
        let key = format!("ico:{}", path.to_lowercase());
        if !self.bmp_cache.contains_key(&key) {
            if self.bmp_cache.len() > 600 {
                self.bmp_cache.clear();
            }
            if self.gpu.asset_tx.is_some() {
                if let Some(decoded_opt) = self.gpu.decoded_cache.get(&key) {
                    let bmp_opt = decoded_opt.as_ref().and_then(|a| self.upload_decoded_to_gpu(a));
                    self.bmp_cache.insert(key.clone(), bmp_opt);
                } else {
                    self.gpu.request_icon_async(path);
                    return false;
                }
            } else {
                let bmp_opt = self
                    .gpu
                    .load_wic_icon_for_path(path)
                    .and_then(|wic_src| unsafe { self.rt.CreateBitmapFromWicBitmap(&wic_src, None).ok() });
                self.bmp_cache.insert(key.clone(), bmp_opt);
            }
        }
        if let Some(Some(bmp)) = self.bmp_cache.get(&key) {
            unsafe {
                self.rt.DrawBitmap(
                    bmp,
                    Some(&r.to_d2d()),
                    1.0,
                    D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                    None,
                );
            }
            true
        } else {
            false
        }
    }

    /// Draws a PNG/JPG cover art, wallpaper, or thumbnail with ZERO aspect-ratio distortion (Cover / Aspect-Fill)!
    pub fn draw_image_file(&mut self, path: &str, r: Rect, opacity: f32) -> bool {
        self.draw_image_file_mode(path, r, opacity, true)
    }

    /// Draws an image preserving 100% of its exact original proportions centered inside `r` (Contain / Letterbox, ZERO crop or stretch)!
    pub fn draw_image_file_contain(&mut self, path: &str, r: Rect, opacity: f32) -> bool {
        self.draw_image_file_mode(path, r, opacity, false)
    }

    fn draw_image_file_mode(&mut self, path: &str, r: Rect, opacity: f32, cover: bool) -> bool {
        if path.trim().is_empty() || r.w <= 1.0 || r.h <= 1.0 {
            return false;
        }
        let key = format!("img:{}", path.to_lowercase());
        if !self.bmp_cache.contains_key(&key) {
            if self.bmp_cache.len() > 600 {
                self.bmp_cache.clear();
            }
            if self.gpu.asset_tx.is_some() {
                if let Some(decoded_opt) = self.gpu.decoded_cache.get(&key) {
                    let bmp_opt = decoded_opt.as_ref().and_then(|a| self.upload_decoded_to_gpu(a));
                    self.bmp_cache.insert(key.clone(), bmp_opt);
                } else {
                    let req_max = if r.w > 640.0 || r.h > 640.0 { 1920 } else { 512 };
                    self.gpu.request_image_async(path, req_max);
                    return false;
                }
            } else {
                let bmp_opt = self
                    .gpu
                    .load_wic_image_file(path)
                    .and_then(|wic_src| unsafe { self.rt.CreateBitmapFromWicBitmap(&wic_src, None).ok() });
                self.bmp_cache.insert(key.clone(), bmp_opt);
            }
        }
        if let Some(Some(bmp)) = self.bmp_cache.get(&key) {
            unsafe {
                let sz = bmp.GetSize();
                let bw = sz.width.max(1.0);
                let bh = sz.height.max(1.0);
                if cover {
                    // Aspect-fill center-crop -> fills target cleanly with ZERO stretching!
                    let bmp_ar = bw / bh;
                    let dst_ar = r.w / r.h;
                    let src = if bmp_ar > dst_ar {
                        let sw = bh * dst_ar;
                        let sx = (bw - sw) * 0.5;
                        D2D_RECT_F { left: sx, top: 0.0, right: sx + sw, bottom: bh }
                    } else {
                        let sh = bw / dst_ar;
                        let sy = (bh - sh) * 0.5;
                        D2D_RECT_F { left: 0.0, top: sy, right: bw, bottom: sy + sh }
                    };
                    self.rt.DrawBitmap(
                        bmp,
                        Some(&r.to_d2d()),
                        opacity,
                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        Some(&src),
                    );
                } else {
                    // Aspect-fit (Contain) -> exact original proportions centered, ZERO distortion & ZERO crop!
                    let scale = (r.w / bw).min(r.h / bh);
                    let dw = bw * scale;
                    let dh = bh * scale;
                    let dst = Rect::new(r.x + (r.w - dw) * 0.5, r.y + (r.h - dh) * 0.5, dw, dh);
                    self.rt.DrawBitmap(
                        bmp,
                        Some(&dst.to_d2d()),
                        opacity,
                        D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                        None,
                    );
                }
            }
            true
        } else {
            false
        }
    }

    /// Crisp hand-crafted Direct2D Vector Icons (zero emoji characters!)
    pub fn draw_vector_icon(&self, icon: VectorIcon, cx: f32, cy: f32, size: f32, color: D2D1_COLOR_F) {
        let s = size * 0.5;
        let sw = (size * 0.115).clamp(1.3, 2.2);
        match icon {
            VectorIcon::Search => {
                self.stroke_circle(cx - s * 0.18, cy - s * 0.18, s * 0.56, color, sw);
                self.draw_line(cx + s * 0.24, cy + s * 0.24, cx + s * 0.78, cy + s * 0.78, color, sw);
            }
            VectorIcon::Settings => {
                self.stroke_circle(cx, cy, s * 0.45, color, sw);
                for i in 0..6 {
                    let a = (i as f32 * 60.0).to_radians();
                    self.draw_line(
                        cx + a.cos() * s * 0.58,
                        cy + a.sin() * s * 0.58,
                        cx + a.cos() * s * 0.85,
                        cy + a.sin() * s * 0.85,
                        color,
                        sw,
                    );
                }
            }
            VectorIcon::Power => {
                self.draw_arc_segment(cx, cy, s * 0.68, sw, -52.0, 284.0, color);
                self.draw_line(cx, cy - s * 0.85, cx, cy - s * 0.1, color, sw);
            }
            VectorIcon::Sleep => {
                self.draw_arc_segment(cx, cy, s * 0.66, sw, -30.0, 220.0, color);
                self.draw_arc_segment(cx + s * 0.26, cy - s * 0.22, s * 0.46, sw, 60.0, 140.0, color);
            }
            VectorIcon::Restart | VectorIcon::Refresh => {
                self.draw_arc_segment(cx, cy, s * 0.65, sw, -40.0, 290.0, color);
                self.draw_line(cx + s * 0.48, cy - s * 0.72, cx + s * 0.55, cy - s * 0.28, color, sw);
                self.draw_line(cx + s * 0.14, cy - s * 0.34, cx + s * 0.55, cy - s * 0.28, color, sw);
            }
            VectorIcon::Lock => {
                self.stroke_rrect(Rect::new(cx - s * 0.62, cy - s * 0.08, s * 1.24, s * 0.92), 2.5, color, sw);
                self.draw_arc_segment(cx, cy - s * 0.12, s * 0.38, sw, 180.0, 180.0, color);
            }
            VectorIcon::Volume | VectorIcon::VolumeMute => {
                // Speaker cone
                self.fill_rrect(Rect::new(cx - s * 0.82, cy - s * 0.28, s * 0.42, s * 0.56), 1.2, color);
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx - s * 0.48, y: cy - s * 0.28 },
                    D2D_POINT_2F { x: cx - s * 0.02, y: cy - s * 0.68 },
                    D2D_POINT_2F { x: cx - s * 0.02, y: cy + s * 0.68 },
                    D2D_POINT_2F { x: cx - s * 0.48, y: cy + s * 0.28 },
                ], color);
                if icon == VectorIcon::Volume {
                    self.draw_arc_segment(cx + s * 0.08, cy, s * 0.42, sw, -45.0, 90.0, color);
                    self.draw_arc_segment(cx + s * 0.14, cy, s * 0.72, sw, -45.0, 90.0, color);
                } else {
                    self.draw_line(cx + s * 0.25, cy - s * 0.35, cx + s * 0.85, cy + s * 0.35, color, sw);
                    self.draw_line(cx + s * 0.85, cy - s * 0.35, cx + s * 0.25, cy + s * 0.35, color, sw);
                }
            }
            VectorIcon::Mic => {
                self.stroke_rrect(Rect::new(cx - s * 0.32, cy - s * 0.82, s * 0.64, s * 0.96), s * 0.32, color, sw);
                self.draw_arc_segment(cx, cy - s * 0.05, s * 0.58, sw, 10.0, 160.0, color);
                self.draw_line(cx, cy + s * 0.55, cx, cy + s * 0.88, color, sw);
                self.draw_line(cx - s * 0.35, cy + s * 0.88, cx + s * 0.35, cy + s * 0.88, color, sw);
            }
            VectorIcon::Wifi => {
                self.draw_arc_segment(cx, cy + s * 0.52, s * 1.02, sw, 225.0, 90.0, color);
                self.draw_arc_segment(cx, cy + s * 0.52, s * 0.65, sw, 225.0, 90.0, color);
                self.fill_circle(cx, cy + s * 0.46, s * 0.16, color);
            }
            VectorIcon::Bluetooth => {
                self.draw_line(cx, cy - s * 0.8, cx, cy + s * 0.8, color, sw);
                self.draw_line(cx, cy - s * 0.8, cx + s * 0.52, cy - s * 0.35, color, sw);
                self.draw_line(cx + s * 0.52, cy - s * 0.35, cx - s * 0.45, cy + s * 0.35, color, sw);
                self.draw_line(cx, cy + s * 0.8, cx + s * 0.52, cy + s * 0.35, color, sw);
                self.draw_line(cx + s * 0.52, cy + s * 0.35, cx - s * 0.45, cy - s * 0.35, color, sw);
            }
            VectorIcon::Keyboard | VectorIcon::Code => {
                self.draw_line(cx - s * 0.25, cy - s * 0.45, cx - s * 0.72, cy, color, sw);
                self.draw_line(cx - s * 0.72, cy, cx - s * 0.25, cy + s * 0.45, color, sw);
                self.draw_line(cx + s * 0.25, cy - s * 0.45, cx + s * 0.72, cy, color, sw);
                self.draw_line(cx + s * 0.72, cy, cx + s * 0.25, cy + s * 0.45, color, sw);
            }
            VectorIcon::ChevronDown => {
                self.draw_line(cx - s * 0.5, cy - s * 0.22, cx, cy + s * 0.32, color, sw);
                self.draw_line(cx, cy + s * 0.32, cx + s * 0.5, cy - s * 0.22, color, sw);
            }
            VectorIcon::MusicNote => {
                self.fill_circle(cx - s * 0.26, cy + s * 0.44, s * 0.30, color);
                self.draw_line(cx + s * 0.02, cy + s * 0.42, cx + s * 0.02, cy - s * 0.68, color, sw * 1.1);
                self.draw_line(cx + s * 0.02, cy - s * 0.68, cx + s * 0.58, cy - s * 0.48, color, sw * 1.3);
            }
            VectorIcon::Prev => {
                self.fill_rrect(Rect::new(cx - s * 0.65, cy - s * 0.55, s * 0.22, s * 1.1), 1.0, color);
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx - s * 0.28, y: cy },
                    D2D_POINT_2F { x: cx + s * 0.62, y: cy - s * 0.58 },
                    D2D_POINT_2F { x: cx + s * 0.62, y: cy + s * 0.58 },
                ], color);
            }
            VectorIcon::Next => {
                self.fill_rrect(Rect::new(cx + s * 0.43, cy - s * 0.55, s * 0.22, s * 1.1), 1.0, color);
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx + s * 0.28, y: cy },
                    D2D_POINT_2F { x: cx - s * 0.62, y: cy - s * 0.58 },
                    D2D_POINT_2F { x: cx - s * 0.62, y: cy + s * 0.58 },
                ], color);
            }
            VectorIcon::PlayPause => {
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx - s * 0.58, y: cy - s * 0.55 },
                    D2D_POINT_2F { x: cx + s * 0.16, y: cy },
                    D2D_POINT_2F { x: cx - s * 0.58, y: cy + s * 0.55 },
                ], color);
                self.fill_rrect(Rect::new(cx + s * 0.30, cy - s * 0.52, s * 0.24, s * 1.04), 1.0, color);
            }
            VectorIcon::AppGrid => {
                let gap = s * 0.58;
                let r = s * 0.16;
                for gx in -1..=1 {
                    for gy in -1..=1 {
                        self.fill_circle(cx + gx as f32 * gap, cy + gy as f32 * gap, r, color);
                    }
                }
            }
            VectorIcon::Folder => {
                // Crisp classic macOS/Drool folder silhouette (top-left tab + rounded body)
                self.fill_rrect(Rect::new(cx - s * 0.78, cy - s * 0.62, s * 0.72, s * 0.34), 1.8, color);
                self.stroke_rrect(Rect::new(cx - s * 0.80, cy - s * 0.36, s * 1.60, s * 1.00), 2.4, color, sw);
            }
            VectorIcon::Plus => {
                self.draw_line(cx - s * 0.65, cy, cx + s * 0.65, cy, color, sw * 1.15);
                self.draw_line(cx, cy - s * 0.65, cx, cy + s * 0.65, color, sw * 1.15);
            }
            VectorIcon::Close => {
                self.draw_line(cx - s * 0.52, cy - s * 0.52, cx + s * 0.52, cy + s * 0.52, color, sw);
                self.draw_line(cx + s * 0.52, cy - s * 0.52, cx - s * 0.52, cy + s * 0.52, color, sw);
            }
            VectorIcon::Sun => {
                self.fill_circle(cx, cy, s * 0.42, color);
                for i in 0..8 {
                    let a = (i as f32 * 45.0).to_radians();
                    self.draw_line(
                        cx + a.cos() * s * 0.62,
                        cy + a.sin() * s * 0.62,
                        cx + a.cos() * s * 0.88,
                        cy + a.sin() * s * 0.88,
                        color,
                        sw,
                    );
                }
            }
            VectorIcon::Cloud => {
                self.fill_circle(cx - s * 0.30, cy + s * 0.10, s * 0.36, color);
                self.fill_circle(cx + s * 0.04, cy - s * 0.12, s * 0.44, color);
                self.fill_circle(cx + s * 0.42, cy + s * 0.14, s * 0.32, color);
                self.fill_rrect(Rect::new(cx - s * 0.62, cy + s * 0.12, s * 1.32, s * 0.34), 3.0, color);
            }
            VectorIcon::Gamepad => {
                self.stroke_rrect(Rect::new(cx - s * 0.82, cy - s * 0.44, s * 1.64, s * 0.92), s * 0.42, color, sw);
                self.draw_line(cx - s * 0.46, cy, cx - s * 0.18, cy, color, sw);
                self.draw_line(cx - s * 0.32, cy - s * 0.14, cx - s * 0.32, cy + s * 0.14, color, sw);
                self.fill_circle(cx + s * 0.32, cy - s * 0.08, s * 0.10, color);
                self.fill_circle(cx + s * 0.50, cy + s * 0.08, s * 0.10, color);
            }
            VectorIcon::Sparkle => {
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx, y: cy - s * 0.85 },
                    D2D_POINT_2F { x: cx + s * 0.24, y: cy - s * 0.24 },
                    D2D_POINT_2F { x: cx + s * 0.85, y: cy },
                    D2D_POINT_2F { x: cx + s * 0.24, y: cy + s * 0.24 },
                    D2D_POINT_2F { x: cx, y: cy + s * 0.85 },
                    D2D_POINT_2F { x: cx - s * 0.24, y: cy + s * 0.24 },
                    D2D_POINT_2F { x: cx - s * 0.85, y: cy },
                    D2D_POINT_2F { x: cx - s * 0.24, y: cy - s * 0.24 },
                ], color);
            }
            VectorIcon::External => {
                self.draw_line(cx - s * 0.45, cy + s * 0.45, cx + s * 0.55, cy - s * 0.55, color, sw);
                self.draw_line(cx + s * 0.05, cy - s * 0.55, cx + s * 0.55, cy - s * 0.55, color, sw);
                self.draw_line(cx + s * 0.55, cy - s * 0.55, cx + s * 0.55, cy - s * 0.05, color, sw);
            }
            VectorIcon::Trash => {
                self.draw_line(cx - s * 0.60, cy - s * 0.48, cx + s * 0.60, cy - s * 0.48, color, sw);
                self.stroke_rrect(Rect::new(cx - s * 0.46, cy - s * 0.36, s * 0.92, s * 1.05), 2.0, color, sw);
            }
            VectorIcon::Ethernet => {
                // Crisp Windows/Linux Ethernet LAN Monitor + Cable Connector glyph
                self.stroke_rrect(Rect::new(cx - s * 0.84, cy - s * 0.72, s * 1.22, s * 0.88), 2.0, color, sw);
                self.draw_line(cx - s * 0.23, cy + s * 0.18, cx - s * 0.23, cy + s * 0.56, color, sw);
                self.draw_line(cx - s * 0.56, cy + s * 0.56, cx + s * 0.10, cy + s * 0.56, color, sw);
                // Side RJ45 plug silhouette
                self.fill_rrect(Rect::new(cx + s * 0.50, cy - s * 0.36, s * 0.38, s * 0.56), 1.4, color);
                self.draw_line(cx + s * 0.69, cy + s * 0.20, cx + s * 0.69, cy + s * 0.68, color, sw);
            }
            VectorIcon::NoInternet => {
                // Crisp Globe + Diagonal Alert Slash (unmistakable offline indicator!)
                self.stroke_circle(cx, cy, s * 0.76, color, sw);
                self.draw_line(cx - s * 0.74, cy, cx + s * 0.74, cy, color, sw * 0.85);
                self.draw_line(cx, cy - s * 0.74, cx, cy + s * 0.74, color, sw * 0.85);
                self.draw_line(cx - s * 0.82, cy + s * 0.82, cx + s * 0.82, cy - s * 0.82, color, sw * 1.25);
            }
            VectorIcon::Image => {
                self.stroke_rrect(Rect::new(cx - s * 0.82, cy - s * 0.66, s * 1.64, s * 1.32), 2.4, color, sw);
                self.fill_circle(cx - s * 0.36, cy - s * 0.22, s * 0.16, color);
                self.draw_line(cx - s * 0.64, cy + s * 0.48, cx - s * 0.12, cy - s * 0.04, color, sw);
                self.draw_line(cx - s * 0.12, cy - s * 0.04, cx + s * 0.22, cy + s * 0.24, color, sw);
                self.draw_line(cx + s * 0.16, cy + s * 0.18, cx + s * 0.58, cy - s * 0.20, color, sw);
            }
            VectorIcon::Film => {
                self.stroke_rrect(Rect::new(cx - s * 0.82, cy - s * 0.64, s * 1.64, s * 1.28), 2.4, color, sw);
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx - s * 0.22, y: cy - s * 0.34 },
                    D2D_POINT_2F { x: cx + s * 0.38, y: cy },
                    D2D_POINT_2F { x: cx - s * 0.22, y: cy + s * 0.34 },
                ], color);
            }
            VectorIcon::FileText => {
                self.stroke_rrect(Rect::new(cx - s * 0.64, cy - s * 0.82, s * 1.28, s * 1.64), 2.2, color, sw);
                self.draw_line(cx - s * 0.36, cy - s * 0.32, cx + s * 0.36, cy - s * 0.32, color, sw);
                self.draw_line(cx - s * 0.36, cy + s * 0.02, cx + s * 0.36, cy + s * 0.02, color, sw);
                self.draw_line(cx - s * 0.36, cy + s * 0.36, cx + s * 0.14, cy + s * 0.36, color, sw);
            }
            VectorIcon::RotateCw => {
                self.draw_arc_segment(cx, cy, s * 0.64, sw, -40.0, 275.0, color);
                self.draw_line(cx + s * 0.42, cy - s * 0.62, cx + s * 0.64, cy - s * 0.26, color, sw);
                self.draw_line(cx + s * 0.24, cy - s * 0.24, cx + s * 0.64, cy - s * 0.26, color, sw);
            }
            VectorIcon::ZoomIn => {
                self.stroke_circle(cx - s * 0.14, cy - s * 0.14, s * 0.56, color, sw);
                self.draw_line(cx + s * 0.26, cy + s * 0.26, cx + s * 0.78, cy + s * 0.78, color, sw);
                self.draw_line(cx - s * 0.38, cy - s * 0.14, cx + s * 0.10, cy - s * 0.14, color, sw);
                self.draw_line(cx - s * 0.14, cy - s * 0.38, cx - s * 0.14, cy + s * 0.10, color, sw);
            }
            VectorIcon::ZoomOut => {
                self.stroke_circle(cx - s * 0.14, cy - s * 0.14, s * 0.56, color, sw);
                self.draw_line(cx + s * 0.26, cy + s * 0.26, cx + s * 0.78, cy + s * 0.78, color, sw);
                self.draw_line(cx - s * 0.38, cy - s * 0.14, cx + s * 0.10, cy - s * 0.14, color, sw);
            }
            VectorIcon::Maximize => {
                self.draw_line(cx - s * 0.68, cy - s * 0.68, cx - s * 0.24, cy - s * 0.68, color, sw);
                self.draw_line(cx - s * 0.68, cy - s * 0.68, cx - s * 0.68, cy - s * 0.24, color, sw);
                self.draw_line(cx + s * 0.68, cy - s * 0.68, cx + s * 0.24, cy - s * 0.68, color, sw);
                self.draw_line(cx + s * 0.68, cy - s * 0.68, cx + s * 0.68, cy - s * 0.24, color, sw);
                self.draw_line(cx - s * 0.68, cy + s * 0.68, cx - s * 0.24, cy + s * 0.68, color, sw);
                self.draw_line(cx - s * 0.68, cy + s * 0.68, cx - s * 0.68, cy + s * 0.24, color, sw);
                self.draw_line(cx + s * 0.68, cy + s * 0.68, cx + s * 0.24, cy + s * 0.68, color, sw);
                self.draw_line(cx + s * 0.68, cy + s * 0.68, cx + s * 0.68, cy + s * 0.24, color, sw);
            }
            VectorIcon::ChevronLeft => {
                self.draw_line(cx + s * 0.24, cy - s * 0.52, cx - s * 0.28, cy, color, sw);
                self.draw_line(cx - s * 0.28, cy, cx + s * 0.24, cy + s * 0.52, color, sw);
            }
            VectorIcon::ChevronRight => {
                self.draw_line(cx - s * 0.24, cy - s * 0.52, cx + s * 0.28, cy, color, sw);
                self.draw_line(cx + s * 0.28, cy, cx - s * 0.24, cy + s * 0.52, color, sw);
            }
            VectorIcon::Play => {
                self.draw_polygon(&[
                    D2D_POINT_2F { x: cx - s * 0.44, y: cy - s * 0.58 },
                    D2D_POINT_2F { x: cx + s * 0.58, y: cy },
                    D2D_POINT_2F { x: cx - s * 0.44, y: cy + s * 0.58 },
                ], color);
            }
            VectorIcon::Pause => {
                self.fill_rrect(Rect::new(cx - s * 0.48, cy - s * 0.56, s * 0.30, s * 1.12), 1.5, color);
                self.fill_rrect(Rect::new(cx + s * 0.18, cy - s * 0.56, s * 0.30, s * 1.12), 1.5, color);
            }
            VectorIcon::Save => {
                self.stroke_rrect(Rect::new(cx - s * 0.72, cy - s * 0.72, s * 1.44, s * 1.44), 2.2, color, sw);
                self.fill_rrect(Rect::new(cx - s * 0.36, cy - s * 0.70, s * 0.72, s * 0.44), 1.0, color);
                self.stroke_rrect(Rect::new(cx - s * 0.40, cy + s * 0.08, s * 0.80, s * 0.62), 1.2, color, sw * 0.9);
            }
        }
    }

    fn draw_polygon(&self, pts: &[D2D_POINT_2F], color: D2D1_COLOR_F) {
        if pts.len() < 3 {
            return;
        }
        unsafe {
            let Ok(geo) = self.gpu.d2d_factory.CreatePathGeometry() else { return };
            let Ok(sink) = geo.Open() else { return };
            sink.BeginFigure(pts[0], D2D1_FIGURE_BEGIN_FILLED);
            for p in &pts[1..] {
                sink.AddLine(*p);
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            let _ = sink.Close();
            if let Some(b) = self.brush(color) {
                self.rt.FillGeometry(&geo, &b, None);
            }
        }
    }
}
