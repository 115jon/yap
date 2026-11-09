//! Direct3D 11: a device per capture, and the CPU staging texture frames are
//! read back through.

use windows::core::Interface;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;

/// A D3D11 device, its immediate context and the WinRT wrapper of the
/// device (what a frame pool takes).
pub(crate) struct Gpu {
    pub(crate) device: ID3D11Device,
    /// Use only under the owner's lock: the immediate context is not
    /// re-entrant.
    pub(crate) context: ID3D11DeviceContext,
    pub(crate) winrt: IDirect3DDevice,
}

impl Gpu {
    /// A hardware device with BGRA support (WGC delivers B8G8R8A8).
    pub(crate) fn new() -> Result<Self, String> {
        let mut device = None;
        let mut context = None;
        // SAFETY: the out pointers are live `Option`s of the types the call
        // fills; no adapter, software module or feature levels are passed.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(|e| format!("D3D11CreateDevice failed: {e}"))?;
        let (Some(device), Some(context)) = (device, context) else {
            return Err("D3D11CreateDevice returned no device".into());
        };
        // WGC uses the device from its own threads too.
        if let Ok(multithread) = device.cast::<ID3D11Multithread>() {
            // SAFETY: a plain flag set on a live interface.
            let _ = unsafe { multithread.SetMultithreadProtected(true) };
        }
        let dxgi: IDXGIDevice = device
            .cast()
            .map_err(|e| format!("device is not a DXGI device: {e}"))?;
        // SAFETY: `dxgi` is a live DXGI device.
        let winrt = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .and_then(|inspectable| inspectable.cast::<IDirect3DDevice>())
            .map_err(|e| format!("could not wrap the device for WinRT: {e}"))?;
        Ok(Self {
            device,
            context,
            winrt,
        })
    }
}

/// A CPU-readable texture the size of the region read from a frame.
pub(crate) struct Staging {
    texture: ID3D11Texture2D,
    width: u32,
    height: u32,
}

impl Staging {
    pub(crate) fn new(device: &ID3D11Device, width: u32, height: u32) -> Result<Self, String> {
        Self::with_format(device, width, height, DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32)
    }

    /// A staging texture in `format` (a `DXGI_FORMAT`), for a source of
    /// that format (a copy needs the same one).
    pub(crate) fn with_format(
        device: &ID3D11Device,
        width: u32,
        height: u32,
        format: u32,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("an empty staging texture".into());
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT(format as i32),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: `desc` describes a plain staging texture; the out pointer
        // is a live `Option`.
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .map_err(|e| format!("could not create the staging texture: {e}"))?;
        let texture = texture.ok_or("CreateTexture2D returned no texture")?;
        Ok(Self {
            texture,
            width,
            height,
        })
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Copies the top-left `width` x `height` of `source` here, maps it and
    /// calls `read` with the rows (`(height - 1) * pitch + width * 4` bytes)
    /// and the row pitch. `source` must be at least that large.
    pub(crate) fn read<R>(
        &self,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        read: impl FnOnce(&[u8], u32) -> R,
    ) -> Result<R, String> {
        self.copy(context, source);
        self.map(context, read)
    }

    /// The copy half of [`Staging::read`]: queues the copy of the top-left
    /// of `source` (at least this large) into this texture.
    pub(crate) fn copy(&self, context: &ID3D11DeviceContext, source: &ID3D11Texture2D) {
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: self.width,
            bottom: self.height,
            back: 1,
        };
        // SAFETY: both textures are live and the region is inside the
        // staging texture (same size) and, by the caller's contract, inside
        // `source`.
        unsafe {
            context.CopySubresourceRegion(&self.texture, 0, 0, 0, 0, source, 0, Some(&region));
        }
    }

    /// The map half of [`Staging::read`]: waits for the copy, maps this
    /// texture and calls `read` with its rows and pitch.
    pub(crate) fn map<R>(
        &self,
        context: &ID3D11DeviceContext,
        read: impl FnOnce(&[u8], u32) -> R,
    ) -> Result<R, String> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the texture is a staging texture with CPU read access.
        unsafe { context.Map(&self.texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
            .map_err(|e| format!("could not map the staging texture: {e}"))?;
        let _unmap = Unmap {
            context,
            texture: &self.texture,
        };
        if mapped.pData.is_null() {
            return Err("the staging texture mapped to null".into());
        }
        let pitch = mapped.RowPitch;
        let len = (self.height as usize - 1) * pitch as usize + self.width as usize * 4;
        // SAFETY: a mapped texture of `height` rows of `pitch` bytes holds
        // at least `(height - 1) * pitch + width * 4` readable bytes, and
        // stays mapped until `_unmap` drops after `read` returns.
        let rows = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), len) };
        Ok(read(rows, pitch))
    }
}

/// Unmaps on every exit path, including a panic in the reader.
struct Unmap<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}

impl Drop for Unmap<'_> {
    fn drop(&mut self) {
        // SAFETY: the texture was mapped by the matching `Map`.
        unsafe { self.context.Unmap(self.texture, 0) };
    }
}

/// The D3D11 texture behind a captured frame's surface.
pub(crate) fn frame_texture(
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
) -> windows::core::Result<ID3D11Texture2D> {
    use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;

    let surface = frame.Surface()?;
    let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
    // SAFETY: the surface of a D3D11 frame pool is a D3D11 texture.
    unsafe { access.GetInterface::<ID3D11Texture2D>() }
}

/// The size of a texture.
pub(crate) fn texture_size(texture: &ID3D11Texture2D) -> (u32, u32) {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: a plain query into a live out struct.
    unsafe { texture.GetDesc(&mut desc) };
    (desc.Width, desc.Height)
}

/// A texture's `DXGI_FORMAT`.
pub(crate) fn texture_format(texture: &ID3D11Texture2D) -> u32 {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: a plain query into a live out struct.
    unsafe { texture.GetDesc(&mut desc) };
    desc.Format.0 as u32
}
