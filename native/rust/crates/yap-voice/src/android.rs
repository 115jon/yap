//! Android: libwebrtc's Java side. Its audio device module records and
//! plays through `livekit.org.webrtc.audio.*` (`AudioRecord` and
//! `AudioTrack`), and its video codec factories are Java objects, so the
//! library needs the process's `JavaVM`, the application `Context` and a
//! class loader that finds its classes (`libwebrtc.jar`, packaged by the
//! app) before the first call.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

static READY: AtomicBool = AtomicBool::new(false);

/// Hands libwebrtc the JVM and the application context. Once per process;
/// later calls return the first result's `true` or try again.
///
/// Call it from a thread Java called into (a `native` method, `JNI_OnLoad`):
/// libwebrtc looks its classes up with `FindClass` here, and a thread that
/// was only attached to the VM sees the system class loader, not the
/// app's.
///
/// # Safety
/// `jvm` is the process's `JavaVM*` and `context` a live `jobject` (an
/// `android.content.Context`) for the length of the call.
pub unsafe fn init(jvm: *mut c_void, context: *mut c_void) -> bool {
    if READY.load(Ordering::Acquire) {
        return true;
    }
    if jvm.is_null() || context.is_null() {
        return false;
    }
    // SAFETY: the caller's contract; libwebrtc keeps its own global
    // reference to the context.
    let ok =
        unsafe { webrtc_sys::android::ffi::init_android_context(jvm.cast(), context as usize) };
    READY.store(ok, Ordering::Release);
    ok
}

/// [`init`] succeeded: a call can open its audio.
pub fn ready() -> bool {
    READY.load(Ordering::Acquire)
}
