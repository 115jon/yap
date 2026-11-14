//! GPU checks of the HDR shader against the CPU reference, the gradient
//! strip, the shader's cost, and a probe of this PC's displays.

use windows::Win32::Graphics::Direct3D11::D3D11_SUBRESOURCE_DATA;

use super::*;
use crate::tonemap::{srgb_oetf, unorm8};
use crate::win::gpu::{Gpu, Staging};

/// `f32` to IEEE half, round to nearest even (normal and zero values, all
/// the test textures hold).
fn f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exp = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x7f_ffff;
    if value == 0.0 || exp <= 0 {
        return sign;
    }
    let mut half = ((exp as u32) << 10) | (mantissa >> 13);
    let rest = mantissa & 0x1fff;
    if rest > 0x1000 || (rest == 0x1000 && half & 1 == 1) {
        half += 1;
    }
    sign | half as u16
}

fn f16_value(half: u16) -> f32 {
    let exp = i32::from((half >> 10) & 0x1f);
    let mantissa = f32::from(half & 0x3ff);
    if exp == 0 {
        return mantissa * 2f32.powi(-24);
    }
    (1.0 + mantissa / 1024.0) * 2f32.powi(exp - 15)
}

#[test]
fn halves_should_round_trip() {
    for v in [0.0f32, 0.5, 1.0, 2.5, 10.0, 12.5, 0.001] {
        let back = f16_value(f16_bits(v));
        assert!((back - v).abs() <= v * 1e-3, "{v} -> {back}");
    }
    assert_eq!(f16_bits(1.0), 0x3c00);
}

const WIDTH: u32 = 1024;
const BAND: u32 = 24;
/// The hues of the bands, as channel ratios: grey, red, green, blue, an
/// orange, cyan, magenta and a skin tone.
const HUES: [[f32; 3]; 8] = [
    [1.0, 1.0, 1.0],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 0.5, 0.1],
    [0.0, 1.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 0.7, 0.55],
];
/// The ramps' top, in scRGB (10.0 is 800 nits; 1.0 is 80).
const TOP: f32 = 10.0;

fn strip_height() -> u32 {
    BAND * HUES.len() as u32
}

/// The FP16 test picture: each band ramps its hue from 0.01 to `TOP`
/// across the width, on a log scale. The halves (RGBA), and the values they hold.
fn gradient() -> (Vec<u16>, Vec<[f32; 3]>) {
    let height = strip_height();
    let mut halves = Vec::with_capacity((WIDTH * height * 4) as usize);
    let mut values = Vec::with_capacity((WIDTH * height) as usize);
    for y in 0..height {
        let hue = HUES[(y / BAND) as usize];
        for x in 0..WIDTH {
            // Log scale, 0.01 to `TOP` (0.8 to 800 nits): the curve's
            // knee and shoulder get room on the strip.
            let v = TOP * 10f32.powf(-3.0 * (1.0 - x as f32 / (WIDTH - 1) as f32));
            let rgb = hue.map(|c| f16_bits(c * v));
            halves.extend_from_slice(&[rgb[0], rgb[1], rgb[2], f16_bits(1.0)]);
            values.push(rgb.map(f16_value));
        }
    }
    (halves, values)
}

fn texture_with(
    gpu: &Gpu,
    (width, height): (u32, u32),
    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    bind: i32,
    halves: Option<&[u16]>,
) -> ID3D11Texture2D {
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
    let data = halves.map(|halves| D3D11_SUBRESOURCE_DATA {
        pSysMem: halves.as_ptr().cast(),
        SysMemPitch: width * 8,
        SysMemSlicePitch: 0,
    });
    let mut texture = None;
    // SAFETY: `halves`, when given, holds `height` rows of `width * 8`
    // bytes (an FP16 texture); live out pointer.
    unsafe {
        gpu.device
            .CreateTexture2D(
                &desc,
                data.as_ref().map(std::ptr::from_ref),
                Some(&mut texture),
            )
            .unwrap();
    }
    texture.unwrap()
}

/// Converts the strip on the GPU; the BGRA rows, tightly packed.
fn shade(gpu: &Gpu, source: &ID3D11Texture2D, map: ToneMap) -> Vec<u8> {
    let height = strip_height();
    let mut converter = HdrConverter::new(&gpu.device).unwrap();
    let out = converter
        .convert(&gpu.device, &gpu.context, source, (WIDTH, height), map)
        .unwrap()
        .clone();
    let staging = Staging::new(&gpu.device, WIDTH, height).unwrap();
    staging
        .read(&gpu.context, &out, |rows, pitch| {
            let mut packed = Vec::with_capacity((WIDTH * height * 4) as usize);
            for y in 0..height as usize {
                let start = y * pitch as usize;
                packed.extend_from_slice(&rows[start..start + WIDTH as usize * 4]);
            }
            packed
        })
        .unwrap()
}

fn gpu_or_skip() -> Option<Gpu> {
    match Gpu::new() {
        Ok(gpu) => Some(gpu),
        Err(error) => {
            eprintln!("skipped, no hardware D3D11 device: {error}");
            None
        }
    }
}

/// The maps the strip goes through: none (an SDR display, as today), a
/// 200-nit white on a 1000-nit display, and 80-nit white on the same.
fn maps() -> [(&'static str, ToneMap); 3] {
    [
        ("80,80", ToneMap::from_nits(80.0, 80.0)),
        ("200,1000", ToneMap::from_nits(200.0, 1000.0)),
        ("80,1000", ToneMap::from_nits(80.0, 1000.0)),
    ]
}

#[test]
fn the_shader_should_match_the_cpu_reference() {
    let Some(gpu) = gpu_or_skip() else {
        return;
    };
    let (halves, values) = gradient();
    let size = (WIDTH, strip_height());
    // A pool texture with shader binding, and one without (copied first).
    for bind in [D3D11_BIND_SHADER_RESOURCE.0, 0] {
        let source = texture_with(
            &gpu,
            size,
            DXGI_FORMAT_R16G16B16A16_FLOAT,
            bind,
            Some(&halves),
        );
        for (name, map) in maps() {
            let shaded = shade(&gpu, &source, map);
            let mut worst = 0;
            let mut off = 0;
            for (i, value) in values.iter().enumerate() {
                let want = map.map_bgra8(*value);
                let got = &shaded[i * 4..i * 4 + 4];
                for c in 0..4 {
                    let diff = want[c].abs_diff(got[c]);
                    worst = worst.max(diff);
                    off += usize::from(diff > 0);
                }
            }
            eprintln!("{name} bind {bind}: worst {worst} codes, {off} channels off by any");
            assert!(worst <= 1, "{name} bind {bind}: off by {worst} codes");
        }
    }
}

/// The strip to look at: per tone map, the eight bands through the shader;
/// under them the same ramps scaled and clipped per channel (no curve), as
/// a naive conversion looks. `YAP_HDR_GRADIENT_PNG=<path>` writes it.
#[test]
fn the_gradient_strip_should_be_written_when_asked() {
    let Ok(path) = std::env::var("YAP_HDR_GRADIENT_PNG") else {
        return;
    };
    let Some(gpu) = gpu_or_skip() else {
        return;
    };
    let (halves, values) = gradient();
    let height = strip_height();
    let source = texture_with(
        &gpu,
        (WIDTH, height),
        DXGI_FORMAT_R16G16B16A16_FLOAT,
        D3D11_BIND_SHADER_RESOURCE.0,
        Some(&halves),
    );
    let mut rgba = Vec::new();
    let mut rows = 0;
    let gap = |rgba: &mut Vec<u8>, rows: &mut u32, n: u32, shade: u8| {
        for _ in 0..n * WIDTH {
            rgba.extend_from_slice(&[shade, shade, shade, 255]);
        }
        *rows += n;
    };
    for (_, map) in maps() {
        for px in shade(&gpu, &source, map).chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
        rows += height;
        gap(&mut rgba, &mut rows, 4, 128);
        for value in &values {
            let clipped = value.map(|c| unorm8(srgb_oetf((c * map.scale).clamp(0.0, 1.0))));
            rgba.extend_from_slice(&[clipped[0], clipped[1], clipped[2], 255]);
        }
        rows += height;
        gap(&mut rgba, &mut rows, 16, 255);
    }
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), WIDTH, rows);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
    eprintln!("wrote {path} ({WIDTH}x{rows})");
}

/// GPU time of the conversion and of the copy to staging, and CPU time of
/// the map, at 1080p and 1440p, one frame every 16 ms.
#[test]
#[ignore = "a measurement: run with --ignored --nocapture"]
fn shader_cost() {
    let Some(gpu) = gpu_or_skip() else {
        return;
    };
    for size in [(1920u32, 1080u32), (2560, 1440)] {
        let halves = vec![f16_bits(0.5); (size.0 * size.1 * 4) as usize];
        let fp16 = texture_with(
            &gpu,
            size,
            DXGI_FORMAT_R16G16B16A16_FLOAT,
            D3D11_BIND_SHADER_RESOURCE.0,
            Some(&halves),
        );
        let bgra = texture_with(
            &gpu,
            size,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_BIND_SHADER_RESOURCE.0,
            None,
        );
        let map = ToneMap::from_nits(200.0, 1000.0);
        let mut converter = HdrConverter::new(&gpu.device).unwrap();
        let staging = Staging::new(&gpu.device, size.0, size.1).unwrap();
        let mut timing = FrameTiming::new(&gpu.device).unwrap();
        for converting in [false, true] {
            let (mut convert, mut copy, mut map_ms, mut cpu_ms, mut n) = (0.0, 0.0, 0.0, 0.0, 0);
            for frame in 0..330 {
                let started = Instant::now();
                timing.begin(&gpu.context);
                let source = if converting {
                    converter
                        .convert(&gpu.device, &gpu.context, &fp16, size, map)
                        .unwrap()
                        .clone()
                } else {
                    bgra.clone()
                };
                timing.converted(&gpu.context);
                staging.copy(&gpu.context, &source);
                timing.copied(&gpu.context);
                let queued = Instant::now();
                staging.map(&gpu.context, |_, _| ()).unwrap();
                let mapped = Instant::now();
                if let Some((a, b)) = timing.gpu_ms(&gpu.context) {
                    if frame >= 30 {
                        convert += a;
                        copy += b;
                        cpu_ms += (queued - started).as_secs_f64() * 1e3;
                        map_ms += (mapped - queued).as_secs_f64() * 1e3;
                        n += 1;
                    }
                }
                timing.started = None;
                std::thread::sleep(Duration::from_millis(16));
            }
            let n = f64::from(n.max(1));
            eprintln!(
                "{}x{} {}: gpu convert {:.3} ms, gpu copy {:.3} ms, cpu queue {:.3} ms, cpu map (waits for the gpu) {:.3} ms",
                size.0,
                size.1,
                if converting {
                    "fp16 tone map"
                } else {
                    "bgra8 as today"
                },
                convert / n,
                copy / n,
                cpu_ms / n,
                map_ms / n
            );
        }
    }
}

/// What every display reports.
#[test]
#[ignore = "a probe of this PC's displays: run with --ignored --nocapture"]
fn display_probe() {
    for (monitor, device) in crate::win::enumerate_monitors() {
        eprintln!(
            "{device}: {:?}; dxgi (hdr, peak) {:?}; display config (advanced color, white) {:?}",
            display_color(monitor),
            dxgi_output(monitor),
            gdi_device(monitor).and_then(|d| config_color(&d))
        );
        // What the display watch spends each second on this monitor.
        let started = Instant::now();
        for _ in 0..20 {
            let _ = wanted(ColorTarget::Monitor(monitor));
        }
        eprintln!(
            "  one look: {:.2} ms",
            started.elapsed().as_secs_f64() * 1e3 / 20.0
        );
    }
}

/// A captured frame: width, height, tightly packed BGRA.
type Frame = (u32, u32, Vec<u8>);

/// The latest frame of `pool`, as tightly packed BGRA: FP16 frames through
/// the shader with `map`.
fn latest_frame(
    gpu: &Gpu,
    pool: &windows::Graphics::Capture::Direct3D11CaptureFramePool,
    converter: &mut HdrConverter,
    map: ToneMap,
) -> Option<Frame> {
    let mut frame = None;
    while let Ok(next) = pool.TryGetNextFrame() {
        frame = Some(next);
    }
    let frame = frame?;
    let texture = crate::win::gpu::frame_texture(&frame).ok()?;
    let (width, height) = crate::win::gpu::texture_size(&texture);
    let content = frame.ContentSize().ok()?;
    let size = (
        width.min(content.Width as u32),
        height.min(content.Height as u32),
    );
    let source = if crate::win::gpu::texture_format(&texture) == FP16_FORMAT {
        converter
            .convert(&gpu.device, &gpu.context, &texture, size, map)
            .ok()?
            .clone()
    } else {
        texture
    };
    let staging = Staging::new(&gpu.device, size.0, size.1).ok()?;
    staging
        .read(&gpu.context, &source, |rows, pitch| {
            let mut packed = Vec::with_capacity((size.0 * size.1 * 4) as usize);
            for y in 0..size.1 as usize {
                let start = y * pitch as usize;
                packed.extend_from_slice(&rows[start..start + size.0 as usize * 4]);
            }
            (size.0, size.1, packed)
        })
        .ok()
}

/// On an SDR display, the FP16 capture through the identity tone map
/// should give what the BGRA8 capture gives. Captures the window titled
/// "Yap Native Share Test" (tools/share-test-window.ps1; nothing else)
/// both ways at once and compares the closest pair of frames; it moves, so
/// a few columns differ between any two frames. `YAP_HDR_COMPARE_PNG`
/// writes the pair and their difference (x16).
#[test]
#[ignore = "needs the share test window: run with --ignored --nocapture"]
fn wgc_fp16_identity_should_match_bgra8_on_sdr() {
    use windows::Graphics::Capture::Direct3D11CaptureFramePool;
    use windows::Graphics::DirectX::DirectXPixelFormat;

    let Some(source) = crate::sources()
        .into_iter()
        .find(|s| s.name == crate::TEST_WINDOW_TITLE)
    else {
        eprintln!("skipped: no window titled {:?}", crate::TEST_WINDOW_TITLE);
        return;
    };
    let id = crate::ids::SourceId::parse(&source.id).unwrap();
    let Some(gpu) = gpu_or_skip() else {
        return;
    };
    let map = ToneMap::from_nits(80.0, 80.0);
    let mut converter = HdrConverter::new(&gpu.device).unwrap();
    let start = |format| {
        let item = crate::win::item::capture_item(id).unwrap();
        let size = item.Size().unwrap();
        let pool =
            Direct3D11CaptureFramePool::CreateFreeThreaded(&gpu.winrt, format, 2, size).unwrap();
        let session = pool.CreateCaptureSession(&item).unwrap();
        let _ = session.SetIsCursorCaptureEnabled(false);
        let _ = session.SetIsBorderRequired(false);
        session.StartCapture().unwrap();
        (pool, session)
    };
    let (bgra_pool, bgra_session) = start(DirectXPixelFormat::B8G8R8A8UIntNormalized);
    let (fp16_pool, fp16_session) = start(DirectXPixelFormat::R16G16B16A16Float);
    std::thread::sleep(Duration::from_millis(300));
    let mut best: Option<(f64, Frame, Frame)> = None;
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(40));
        let (Some(a), Some(b)) = (
            latest_frame(&gpu, &bgra_pool, &mut converter, map),
            latest_frame(&gpu, &fp16_pool, &mut converter, map),
        ) else {
            continue;
        };
        if (a.0, a.1) != (b.0, b.1) {
            continue;
        }
        // Colour channels only: WGC's BGRA8 alpha is the window's.
        let close =
            a.2.chunks_exact(4)
                .zip(b.2.chunks_exact(4))
                .filter(|(p, q)| (0..3).all(|c| p[c].abs_diff(q[c]) <= 1))
                .count() as f64
                / (a.0 * a.1) as f64;
        if best.as_ref().is_none_or(|(r, _, _)| close > *r) {
            best = Some((close, a, b));
        }
    }
    let _ = (bgra_session.Close(), fp16_session.Close());
    let _ = (bgra_pool.Close(), fp16_pool.Close());
    let (close, a, b) = best.expect("no frame pair");
    let (mut exact, mut worst) = (0usize, 0u8);
    let mut diffs = [0usize; 4];
    for (p, q) in a.2.chunks_exact(4).zip(b.2.chunks_exact(4)) {
        let d = (0..3).map(|c| p[c].abs_diff(q[c])).max().unwrap_or(0);
        exact += usize::from(d == 0);
        worst = worst.max(d);
        diffs[match d {
            0 => 0,
            1 => 1,
            2..=8 => 2,
            _ => 3,
        }] += 1;
    }
    let n = (a.0 * a.1) as f64;
    eprintln!(
        "{}x{}: identical {:.3} %, within 1 code {:.3} %, 2-8 codes {:.3} %, more {:.3} % (moving content), worst {worst}",
        a.0,
        a.1,
        100.0 * exact as f64 / n,
        100.0 * close,
        100.0 * diffs[2] as f64 / n,
        100.0 * diffs[3] as f64 / n
    );
    if let Ok(path) = std::env::var("YAP_HDR_COMPARE_PNG") {
        let (w, h) = (a.0, a.1);
        let mut rgba = Vec::with_capacity((w * 3 * h * 4) as usize);
        for y in 0..h as usize {
            let span = y * w as usize * 4..(y + 1) * w as usize * 4;
            let row = |v: &[u8]| -> Vec<u8> { v[span.clone()].to_vec() };
            for px in row(&a.2).chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
            for px in row(&b.2).chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
            }
            for (p, q) in row(&a.2).chunks_exact(4).zip(row(&b.2).chunks_exact(4)) {
                let d = |c: usize| p[c].abs_diff(q[c]).saturating_mul(16);
                rgba.extend_from_slice(&[d(2), d(1), d(0), 255]);
            }
        }
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w * 3, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
        eprintln!("wrote {path}");
    }
    assert!(close > 0.97, "only {:.2} % within one code", 100.0 * close);
}
