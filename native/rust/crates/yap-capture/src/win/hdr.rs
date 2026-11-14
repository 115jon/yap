//! HDR displays: whether a capture's display is in HDR (advanced color)
//! mode, a watch that follows it (HDR switched on or off, a window moved to
//! another monitor), and the pixel shader that turns a `R16G16B16A16Float`
//! (scRGB) frame into the BGRA8 SDR frame the share pushes, on the
//! capture's own device (`hdr.hlsl`, the curve in `crate::tonemap`).

use std::mem::size_of;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{s, Interface, PCSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO,
    DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_SDR_WHITE_LEVEL,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND, S_OK};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::{ID3DBlob, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11Query,
    ID3D11RenderTargetView, ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BUFFER_DESC, D3D11_QUERY_DATA_TIMESTAMP_DISJOINT, D3D11_QUERY_DESC,
    D3D11_QUERY_TIMESTAMP, D3D11_QUERY_TIMESTAMP_DISJOINT, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITORINFOEXW,
    MONITOR_DEFAULTTONEAREST,
};

use crate::stop::Stop;
use crate::tonemap::{self, ToneMap, SCRGB_WHITE_NITS};

/// `DXGI_FORMAT_R16G16B16A16_FLOAT`, as `texture_format` reports it.
pub(crate) const FP16_FORMAT: u32 = DXGI_FORMAT_R16G16B16A16_FLOAT.0 as u32;

/// How often the watch looks at the display again.
const WATCH_EVERY: Duration = Duration::from_secs(1);

/// What a display reports about its color.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DisplayColor {
    /// In HDR (advanced color, HDR10) mode.
    pub(crate) hdr: bool,
    /// The "SDR content brightness" white, in nits (80 when unknown).
    pub(crate) sdr_white_nits: f32,
    /// The display's peak (`DXGI_OUTPUT_DESC1::MaxLuminance`), in nits; 0
    /// when unknown.
    pub(crate) max_nits: f32,
}

impl DisplayColor {
    const UNKNOWN: Self = Self {
        hdr: false,
        sdr_white_nits: SCRGB_WHITE_NITS,
        max_nits: 0.0,
    };
}

/// The color of the display behind `monitor` (an `HMONITOR` value).
///
/// HDR is DXGI's output color space (`G2084_NONE_P2020` is HDR10 output;
/// the composition is then scRGB), checked against the display config's
/// advanced color state where DXGI does not know the monitor. SDR white is
/// `DISPLAYCONFIG_SDR_WHITE_LEVEL` (thousandths of 80 nits), the peak
/// `MaxLuminance`.
pub(crate) fn display_color(monitor: isize) -> DisplayColor {
    let mut color = DisplayColor::UNKNOWN;
    let dxgi = dxgi_output(monitor);
    let config = gdi_device(monitor).and_then(|device| config_color(&device));
    // Both must agree where both answer: only the display config tells an
    // SDR display's auto color management (also "advanced color") from HDR.
    color.hdr = match (dxgi, config) {
        (Some((space, _)), Some((advanced, _))) => space && advanced,
        (Some((space, _)), None) => space,
        (None, Some((advanced, _))) => advanced,
        (None, None) => false,
    };
    if let Some((_, max_nits)) = dxgi {
        color.max_nits = max_nits;
    }
    if let Some((_, Some(white))) = config {
        color.sdr_white_nits = white;
    }
    color
}

/// From DXGI: (HDR, peak nits) of the output showing `monitor`.
fn dxgi_output(monitor: isize) -> Option<(bool, f32)> {
    // A new factory each time: an old one keeps the outputs (and their
    // color spaces) it first saw.
    // SAFETY: a plain factory creation.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
    for adapter in 0.. {
        // SAFETY: enumeration on a live factory; the end is an error.
        let Ok(adapter) = (unsafe { factory.EnumAdapters1(adapter) }) else {
            break;
        };
        for output in 0.. {
            // SAFETY: as above.
            let Ok(output) = (unsafe { adapter.EnumOutputs(output) }) else {
                break;
            };
            let Ok(output) = output.cast::<IDXGIOutput6>() else {
                continue;
            };
            // SAFETY: a plain query on a live output.
            let Ok(desc) = (unsafe { output.GetDesc1() }) else {
                continue;
            };
            if desc.Monitor.0 as isize == monitor {
                let hdr = desc.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020
                    || desc.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709;
                return Some((hdr, desc.MaxLuminance));
            }
        }
    }
    None
}

/// The GDI device name (`\\.\DISPLAY1`) of `monitor`.
fn gdi_device(monitor: isize) -> Option<Vec<u16>> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `MONITORINFOEXW` starts with a `MONITORINFO`; `cbSize` tells
    // the call to fill the extended struct. A stale handle fails.
    let got = unsafe {
        GetMonitorInfoW(
            HMONITOR(monitor as *mut _),
            std::ptr::from_mut(&mut info).cast::<MONITORINFO>(),
        )
    };
    got.as_bool().then(|| trim_wide(&info.szDevice).to_vec())
}

fn trim_wide(wide: &[u16]) -> &[u16] {
    &wide[..wide.iter().position(|&c| c == 0).unwrap_or(wide.len())]
}

/// From the display config: (advanced color on, SDR white nits) of the
/// target behind the GDI source `device`.
fn config_color(device: &[u16]) -> Option<(bool, Option<f32>)> {
    let (mut path_count, mut mode_count) = (0u32, 0u32);
    // SAFETY: a plain size query into live out values.
    let sized = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if sized != ERROR_SUCCESS {
        return None;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    // SAFETY: the buffers hold the element counts passed with them.
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return None;
    }
    for path in &paths[..path_count as usize] {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: the header is the first field of a struct whose size it
        // declares; 0 is success.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0
            || trim_wide(&source.viewGdiDeviceName) != device
        {
            continue;
        }
        let target = |kind, size| DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: kind,
            size,
            adapterId: path.targetInfo.adapterId,
            id: path.targetInfo.id,
        };
        let mut advanced = DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO {
            header: target(
                DISPLAYCONFIG_DEVICE_INFO_GET_ADVANCED_COLOR_INFO,
                size_of::<DISPLAYCONFIG_GET_ADVANCED_COLOR_INFO>() as u32,
            ),
            ..Default::default()
        };
        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: target(
                DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
            ),
            ..Default::default()
        };
        // SAFETY: as above.
        let (advanced_ok, white_ok) = unsafe {
            (
                DisplayConfigGetDeviceInfo(&mut advanced.header) == 0,
                DisplayConfigGetDeviceInfo(&mut white.header) == 0,
            )
        };
        // SAFETY: the union's only member is the bit field's `u32`.
        let bits = unsafe { advanced.Anonymous.value };
        // advancedColorEnabled (bit 1) without wideColorEnforced (bit 2,
        // the SDR display's auto color management).
        let hdr = advanced_ok && bits & 0b10 != 0 && bits & 0b100 == 0;
        let white = (white_ok && white.SDRWhiteLevel > 0)
            .then(|| white.SDRWhiteLevel as f32 * SCRGB_WHITE_NITS / 1000.0);
        return Some((hdr, white));
    }
    None
}

/// What a capture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorTarget {
    /// An `HMONITOR` value.
    Monitor(isize),
    /// An `HWND` value: the monitor it is mostly on.
    Window(isize),
}

impl ColorTarget {
    fn monitor(self) -> isize {
        match self {
            Self::Monitor(monitor) => monitor,
            // SAFETY: a plain query; a dead window gives the nearest
            // (primary) monitor.
            Self::Window(window) => unsafe {
                MonitorFromWindow(HWND(window as *mut _), MONITOR_DEFAULTTONEAREST).0 as isize
            },
        }
    }
}

/// What a capture of `target` should do: `None` is today's BGRA8 (an SDR
/// display), `Some` is FP16 with that tone map (an HDR display, or the
/// `YAP_TEST_HDR_CAPTURE` switch). Also the display, for the log.
pub(crate) fn wanted(target: ColorTarget) -> (Option<ToneMap>, isize, DisplayColor) {
    let monitor = target.monitor();
    let color = display_color(monitor);
    let map = tonemap::test_override().or_else(|| {
        color
            .hdr
            .then(|| ToneMap::from_nits(color.sdr_white_nits, color.max_nits))
    });
    (map, monitor, color)
}

fn describe(map: Option<ToneMap>, monitor: isize, color: DisplayColor) -> String {
    let display = format!(
        "monitor {monitor:#x} {} (SDR white {:.0} nits, peak {:.0} nits)",
        if color.hdr { "HDR" } else { "SDR" },
        color.sdr_white_nits,
        color.max_nits
    );
    match map {
        None => format!("{display}: BGRA8 capture"),
        Some(map) => format!(
            "{display}: FP16 capture, tone-mapped (scale {:.3}, peak {:.2}, knee {:.2}){}",
            map.scale,
            map.peak,
            map.knee,
            if tonemap::test_override().is_some() {
                " by YAP_TEST_HDR_CAPTURE"
            } else {
                ""
            }
        ),
    }
}

/// The tone map a capture should use now, kept current by a thread that
/// looks at the display every second. Dropping it stops the thread.
pub(crate) struct ColorWatch {
    wanted: Arc<Mutex<Option<ToneMap>>>,
    stop: Arc<Stop>,
    thread: Option<JoinHandle<()>>,
}

impl ColorWatch {
    /// Reads the display now (so the capture can start in the right
    /// format), then follows it.
    pub(crate) fn start(target: ColorTarget, label: String) -> Self {
        let (map, monitor, color) = wanted(target);
        eprintln!("[capture] {label} {}", describe(map, monitor, color));
        let wanted_now = Arc::new(Mutex::new(map));
        let stop = Arc::new(Stop::default());
        let thread = {
            let (wanted_now, stop) = (Arc::clone(&wanted_now), Arc::clone(&stop));
            std::thread::Builder::new()
                .name("yap-share-color".to_owned())
                .spawn(move || {
                    let mut last = (map, monitor, color);
                    while !stop.wait(WATCH_EVERY) {
                        let now = wanted(target);
                        if now != last {
                            eprintln!("[capture] {label} {}", describe(now.0, now.1, now.2));
                            *wanted_now.lock().unwrap_or_else(PoisonError::into_inner) = now.0;
                            last = now;
                        }
                    }
                })
                .map_err(|e| eprintln!("[capture] the display watch did not start: {e}"))
                .ok()
        };
        Self {
            wanted: wanted_now,
            stop,
            thread,
        }
    }

    /// What the frame callback reads.
    pub(crate) fn cell(&self) -> Arc<Mutex<Option<ToneMap>>> {
        Arc::clone(&self.wanted)
    }

    pub(crate) fn now(&self) -> Option<ToneMap> {
        *self.wanted.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for ColorWatch {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The vertex and pixel shaders' bytecode.
type Bytecode = (Vec<u8>, Vec<u8>);

/// The shaders' bytecode, compiled once per process (d3dcompiler_47, in
/// every Windows 10 and 11).
fn bytecode() -> Result<&'static Bytecode, String> {
    static CODE: OnceLock<Result<Bytecode, String>> = OnceLock::new();
    CODE.get_or_init(|| {
        Ok((
            compile(s!("vs_main"), s!("vs_4_0"))?,
            compile(s!("ps_main"), s!("ps_4_0"))?,
        ))
    })
    .as_ref()
    .map_err(Clone::clone)
}

fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>, String> {
    const SOURCE: &str = include_str!("hdr.hlsl");
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the source pointer and length are of a live `str`; the
    // names are NUL-terminated literals; the out pointers are live.
    let compiled = unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("hdr.hlsl"),
            None,
            None,
            entry,
            target,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    let blob_bytes = |blob: &ID3DBlob| {
        // SAFETY: a blob's pointer is valid for its size while it lives.
        unsafe {
            std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
        }
        .to_vec()
    };
    match (compiled, code) {
        (Ok(()), Some(code)) => Ok(blob_bytes(&code)),
        (result, _) => Err(format!(
            "the HDR shader did not compile ({result:?}): {}",
            errors
                .as_ref()
                .map(|e| String::from_utf8_lossy(&blob_bytes(e)).into_owned())
                .unwrap_or_default()
        )),
    }
}

/// The shader's constants (`cbuffer ToneMap`).
#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
struct Constants {
    scale: f32,
    peak: f32,
    knee: f32,
    unused: f32,
}

/// The BGRA8 render target a conversion draws into.
struct Target {
    texture: ID3D11Texture2D,
    view: ID3D11RenderTargetView,
    size: (u32, u32),
}

/// FP16 scRGB to BGRA8 SDR on one device. The output stays on the GPU; the
/// caller copies it to its staging texture as it copies a BGRA8 frame.
pub(crate) struct HdrConverter {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    constants: ID3D11Buffer,
    loaded: Option<Constants>,
    target: Option<Target>,
    /// Views of the frame pool's textures (two, unless it was recreated),
    /// each kept with its texture so the address cannot be reused.
    views: Vec<(ID3D11Texture2D, ID3D11ShaderResourceView)>,
    /// A copy of a source that cannot be bound as a shader resource.
    copy: Option<(ID3D11Texture2D, ID3D11ShaderResourceView, (u32, u32))>,
}

impl HdrConverter {
    pub(crate) fn new(device: &ID3D11Device) -> Result<Self, String> {
        let (vs_code, ps_code) = bytecode()?;
        let mut vs = None;
        let mut ps = None;
        // SAFETY: bytecode from `D3DCompile` for these stages; live out
        // pointers.
        unsafe {
            device
                .CreateVertexShader(vs_code, None, Some(&mut vs))
                .map_err(|e| format!("CreateVertexShader: {e}"))?;
            device
                .CreatePixelShader(ps_code, None, Some(&mut ps))
                .map_err(|e| format!("CreatePixelShader: {e}"))?;
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of::<Constants>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        let mut constants = None;
        // SAFETY: a plain 16-byte constant buffer; live out pointer.
        unsafe { device.CreateBuffer(&desc, None, Some(&mut constants)) }
            .map_err(|e| format!("CreateBuffer: {e}"))?;
        Ok(Self {
            vs: vs.ok_or("no vertex shader")?,
            ps: ps.ok_or("no pixel shader")?,
            constants: constants.ok_or("no constant buffer")?,
            loaded: None,
            target: None,
            views: Vec::new(),
            copy: None,
        })
    }

    /// Draws the top-left `width` x `height` of `source` (FP16 scRGB, at
    /// least that large) tone-mapped into a BGRA8 texture of exactly that
    /// size, and returns it. GPU work only: nothing waits.
    pub(crate) fn convert(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        (width, height): (u32, u32),
        map: ToneMap,
    ) -> Result<&ID3D11Texture2D, String> {
        if self.target.as_ref().map(|t| t.size) != Some((width, height)) {
            self.target = None;
            self.target = Some(render_target(device, width, height)?);
        }
        let view = self.source_view(device, context, source)?;
        let constants = Constants {
            scale: map.scale,
            peak: map.peak,
            knee: map.knee,
            unused: 0.0,
        };
        // SAFETY: every object is live and of this device; the constant
        // buffer is exactly `Constants`.
        unsafe {
            if self.loaded != Some(constants) {
                context.UpdateSubresource(
                    &self.constants,
                    0,
                    None,
                    std::ptr::from_ref(&constants).cast(),
                    0,
                    0,
                );
                self.loaded = Some(constants);
            }
            let target = self.target.as_ref().ok_or("no render target")?;
            context.IASetInputLayout(None);
            context.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(&self.vs, None);
            context.PSSetShader(&self.ps, None);
            context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            context.PSSetShaderResources(0, Some(&[Some(view)]));
            context.OMSetRenderTargets(Some(&[Some(target.view.clone())]), None);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: width as f32,
                Height: height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            context.Draw(3, 0);
            // Let go of the pool's texture and our target, so neither is
            // bound when WGC or the copy uses it next.
            context.PSSetShaderResources(0, Some(&[None]));
            context.OMSetRenderTargets(None, None);
        }
        Ok(&self.target.as_ref().ok_or("no render target")?.texture)
    }

    /// A shader view of `source`, kept per texture; a source without
    /// shader binding is copied to a texture of ours first.
    fn source_view(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
    ) -> Result<ID3D11ShaderResourceView, String> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a plain query into a live out struct.
        unsafe { source.GetDesc(&mut desc) };
        if desc.BindFlags & D3D11_BIND_SHADER_RESOURCE.0 as u32 == 0 {
            let size = (desc.Width, desc.Height);
            if self.copy.as_ref().map(|c| c.2) != Some(size) {
                self.copy = None;
                let texture = texture(device, size, desc.Format, D3D11_BIND_SHADER_RESOURCE.0)?;
                let view = shader_view(device, &texture)?;
                self.copy = Some((texture, view, size));
            }
            let (texture, view, _) = self.copy.as_ref().ok_or("no copy")?;
            // SAFETY: same size and format, both live on this device.
            unsafe { context.CopyResource(texture, source) };
            return Ok(view.clone());
        }
        if let Some((_, view)) = self.views.iter().find(|(t, _)| t == source) {
            return Ok(view.clone());
        }
        let view = shader_view(device, source)?;
        if self.views.len() >= 4 {
            self.views.remove(0);
        }
        self.views.push((source.clone(), view.clone()));
        Ok(view)
    }
}

fn texture(
    device: &ID3D11Device,
    (width, height): (u32, u32),
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    bind: i32,
) -> Result<ID3D11Texture2D, String> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: a plain 2D texture; live out pointer.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(|e| format!("CreateTexture2D: {e}"))?;
    texture.ok_or_else(|| "CreateTexture2D returned no texture".into())
}

fn shader_view(
    device: &ID3D11Device,
    texture: &ID3D11Texture2D,
) -> Result<ID3D11ShaderResourceView, String> {
    let mut view = None;
    // SAFETY: a whole-texture view of a live texture with shader binding.
    unsafe { device.CreateShaderResourceView(texture, None, Some(&mut view)) }
        .map_err(|e| format!("CreateShaderResourceView: {e}"))?;
    view.ok_or_else(|| "no shader view".into())
}

fn render_target(device: &ID3D11Device, width: u32, height: u32) -> Result<Target, String> {
    let texture = texture(
        device,
        (width, height),
        DXGI_FORMAT_B8G8R8A8_UNORM,
        D3D11_BIND_RENDER_TARGET.0,
    )?;
    let mut view = None;
    // SAFETY: a whole-texture view of a live render-target texture.
    unsafe { device.CreateRenderTargetView(&texture, None, Some(&mut view)) }
        .map_err(|e| format!("CreateRenderTargetView: {e}"))?;
    Ok(Target {
        texture,
        view: view.ok_or("no render target view")?,
        size: (width, height),
    })
}

/// `YAP_CAPTURE_TIMING=1` (measurements): per-frame GPU time of the
/// conversion and of the copy to staging (timestamp queries) and CPU time
/// of the read and of the push, logged as averages every 300 frames.
pub(crate) fn timing_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("YAP_CAPTURE_TIMING").is_ok_and(|v| v == "1"))
}

const TIMING_EVERY: u32 = 300;

#[derive(Default)]
struct Sums {
    frames: u32,
    gpu_samples: u32,
    convert_ms: f64,
    copy_ms: f64,
    read_ms: f64,
    push_ms: f64,
}

/// The queries and sums of `YAP_CAPTURE_TIMING`.
pub(crate) struct FrameTiming {
    disjoint: ID3D11Query,
    marks: [ID3D11Query; 3],
    sums: Sums,
    started: Option<Instant>,
}

impl FrameTiming {
    pub(crate) fn new(device: &ID3D11Device) -> Result<Self, String> {
        let query = |kind| {
            let mut query = None;
            // SAFETY: a plain query object; live out pointer.
            unsafe {
                device.CreateQuery(
                    &D3D11_QUERY_DESC {
                        Query: kind,
                        MiscFlags: 0,
                    },
                    Some(&mut query),
                )
            }
            .map_err(|e| format!("CreateQuery: {e}"))?;
            query.ok_or_else(|| "no query".to_owned())
        };
        Ok(Self {
            disjoint: query(D3D11_QUERY_TIMESTAMP_DISJOINT)?,
            marks: [
                query(D3D11_QUERY_TIMESTAMP)?,
                query(D3D11_QUERY_TIMESTAMP)?,
                query(D3D11_QUERY_TIMESTAMP)?,
            ],
            sums: Sums::default(),
            started: None,
        })
    }

    /// Before the conversion (or the copy, for a BGRA8 frame).
    pub(crate) fn begin(&mut self, context: &ID3D11DeviceContext) {
        self.started = Some(Instant::now());
        // SAFETY: live queries of this context's device.
        unsafe {
            context.Begin(&self.disjoint);
            context.End(&self.marks[0]);
        }
    }

    /// Between the conversion and the copy.
    pub(crate) fn converted(&self, context: &ID3D11DeviceContext) {
        // SAFETY: as above.
        unsafe { context.End(&self.marks[1]) };
    }

    /// After the copy, before the map.
    pub(crate) fn copied(&self, context: &ID3D11DeviceContext) {
        // SAFETY: as above.
        unsafe {
            context.End(&self.marks[2]);
            context.End(&self.disjoint);
        }
    }

    /// After the map and the push; logs every `TIMING_EVERY` frames.
    pub(crate) fn finish(
        &mut self,
        context: &ID3D11DeviceContext,
        read_done: Instant,
        label: &str,
        converted: bool,
    ) {
        let Some(started) = self.started.take() else {
            return;
        };
        let now = Instant::now();
        self.sums.frames += 1;
        self.sums.read_ms += (read_done - started).as_secs_f64() * 1e3;
        self.sums.push_ms += (now - read_done).as_secs_f64() * 1e3;
        if let Some((convert, copy)) = self.gpu_ms(context) {
            self.sums.gpu_samples += 1;
            self.sums.convert_ms += convert;
            self.sums.copy_ms += copy;
        }
        if self.sums.frames >= TIMING_EVERY {
            let s = std::mem::take(&mut self.sums);
            let n = f64::from(s.frames);
            let g = f64::from(s.gpu_samples.max(1));
            eprintln!(
                "[capture] timing {label} {}: {} frames, gpu convert {:.3} ms + copy {:.3} ms ({} samples), cpu read {:.3} ms, push {:.3} ms",
                if converted { "fp16" } else { "bgra8" },
                s.frames,
                s.convert_ms / g,
                s.copy_ms / g,
                s.gpu_samples,
                s.read_ms / n,
                s.push_ms / n,
            );
        }
    }

    /// (convert, copy) in ms, if the GPU has the answers (the map waited
    /// for the copy, so they normally are).
    fn gpu_ms(&self, context: &ID3D11DeviceContext) -> Option<(f64, f64)> {
        let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
        let mut ticks = [0u64; 3];
        for _ in 0..200 {
            if get_data(context, &self.disjoint, &mut disjoint) {
                break;
            }
            std::hint::spin_loop();
        }
        if disjoint.Frequency == 0 || disjoint.Disjoint.as_bool() {
            return None;
        }
        for (mark, tick) in self.marks.iter().zip(&mut ticks) {
            if !get_data(context, mark, tick) {
                return None;
            }
        }
        let ms = |a: u64, b: u64| b.saturating_sub(a) as f64 * 1e3 / disjoint.Frequency as f64;
        Some((ms(ticks[0], ticks[1]), ms(ticks[1], ticks[2])))
    }
}

/// `GetData` that tells "not ready" (`S_FALSE`) from ready: the wrapper
/// folds both into `Ok`.
fn get_data<T>(context: &ID3D11DeviceContext, query: &ID3D11Query, out: &mut T) -> bool {
    // SAFETY: the vtable call `GetData` wraps, with a live query and an
    // out buffer of the size passed.
    let hr = unsafe {
        (Interface::vtable(context).GetData)(
            Interface::as_raw(context),
            Interface::as_raw(query),
            std::ptr::from_mut(out).cast(),
            size_of::<T>() as u32,
            0,
        )
    };
    hr == S_OK
}

#[cfg(test)]
mod tests;
