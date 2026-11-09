//! A tiny Direct3D 11 game stand-in for testing the game capture hook: a
//! window titled "Yap Native Share Test" that clears to magenta and presents
//! through a DXGI swap chain at the display rate. It shows without taking
//! the focus and quits after `--seconds` (default 60) or when closed.
//! `--fp16` makes the swap chain R16G16B16A16_FLOAT (scRGB, as an HDR game
//! renders) and clears to magenta at twice SDR white (2.0, 0, 2.0).
//!
//! The only process the hook tests ever inject into.

#[cfg(windows)]
fn main() -> windows::core::Result<()> {
    use std::time::{Duration, Instant};

    use windows::core::{w, Interface};
    use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView,
        ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
    };
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_MODE_DESC,
        DXGI_SAMPLE_DESC,
    };
    use windows::Win32::Graphics::Dxgi::{
        IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_FLIP_DISCARD,
        DXGI_USAGE_RENDER_TARGET_OUTPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW, PostQuitMessage,
        RegisterClassW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW, MSG, PM_REMOVE,
        SW_SHOWNOACTIVATE, WM_DESTROY, WM_QUIT, WNDCLASSW, WS_EX_NOACTIVATE, WS_OVERLAPPEDWINDOW,
    };

    extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if msg == WM_DESTROY {
            // SAFETY: posts to this thread's queue.
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        // SAFETY: the default handling for our own window.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let seconds: u64 = std::env::args()
        .skip_while(|a| a != "--seconds")
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let fp16 = std::env::args().any(|a| a == "--fp16");
    let (format, clear) = if fp16 {
        (DXGI_FORMAT_R16G16B16A16_FLOAT, [2.0, 0.0, 2.0, 1.0])
    } else {
        (DXGI_FORMAT_B8G8R8A8_UNORM, [1.0, 0.0, 1.0, 1.0])
    };

    // SAFETY: plain Win32 window creation on this thread, with a class whose
    // procedure is above.
    let hwnd = unsafe {
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            lpszClassName: w!("YapHookTarget"),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("YapHookTarget"),
            w!("Yap Native Share Test"),
            WS_OVERLAPPEDWINDOW,
            40,
            40,
            480,
            300,
            None,
            None,
            None,
            None,
        )?
    };

    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 0,
            Height: 0,
            Format: format,
            ..Default::default()
        },
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        OutputWindow: hwnd,
        Windowed: true.into(),
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
        Flags: 0,
    };
    let mut swap: Option<IDXGISwapChain> = None;
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    // SAFETY: the out pointers are live `Option`s; `desc` names our window.
    unsafe {
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut swap),
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    let (Some(swap), Some(device), Some(context)) = (swap, device, context) else {
        return Err(windows::core::Error::from_thread());
    };
    // SAFETY: buffer 0 of a live swap chain; the view is made on its device.
    let view: ID3D11RenderTargetView = unsafe {
        let back: ID3D11Texture2D = swap.GetBuffer(0)?;
        let mut view = None;
        device.CreateRenderTargetView(&back.cast::<ID3D11Texture2D>()?, None, Some(&mut view))?;
        view.ok_or_else(windows::core::Error::from_thread)?
    };
    // SAFETY: shows our own window without activating it.
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    println!("hwnd {}", hwnd.0 as isize);

    let until = Instant::now() + Duration::from_secs(seconds);
    let mut msg = MSG::default();
    while Instant::now() < until {
        // SAFETY: the message pump of this thread.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Magenta, opaque: easy to tell from anything WGC would see.
            context.ClearRenderTargetView(&view, &clear);
            let _ = swap.Present(1, DXGI_PRESENT(0));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
