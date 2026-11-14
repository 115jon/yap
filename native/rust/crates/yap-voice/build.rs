//! With the `libwebrtc` feature: compiles `cpp/track_audio.cpp`,
//! `cpp/camera_i420.cpp` and `cpp/screen_i420.cpp` against the same
//! prebuilt WebRTC headers and defines `webrtc-sys` uses.

fn main() {
    println!("cargo:rerun-if-changed=cpp/track_audio.cpp");
    println!("cargo:rerun-if-changed=cpp/camera_i420.cpp");
    println!("cargo:rerun-if-changed=cpp/screen_i420.cpp");
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");
    #[cfg(feature = "libwebrtc")]
    webrtc_shim();
}

#[cfg(feature = "libwebrtc")]
fn webrtc_shim() {
    // webrtc-sys's build script downloads the prebuilt, and on a fresh
    // target folder cargo may run this one first: fetch it here then (the
    // download takes a file lock, so the two do not collide).
    if !webrtc_sys_build::webrtc_dir().exists() {
        webrtc_sys_build::download_webrtc().expect("the WebRTC prebuilt");
    }
    let include = webrtc_sys_build::webrtc_dir().join("include");
    assert!(
        include.join("api/media_stream_interface.h").exists(),
        "no WebRTC headers under {} (set LK_CUSTOM_WEBRTC, or build webrtc-sys first)",
        include.display()
    );
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .file("cpp/track_audio.cpp")
        .file("cpp/camera_i420.cpp")
        .file("cpp/screen_i420.cpp")
        .include(&include)
        .include(include.join("third_party/abseil-cpp"))
        .include(include.join("third_party/libyuv/include"))
        .define("WEBRTC_APM_DEBUG_DUMP", "0");
    for (name, value) in webrtc_sys_build::webrtc_defines() {
        build.define(&name, value.as_deref());
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag("/std:c++20").flag("/EHsc");
    } else {
        // WebRTC's own headers trip this one on every include.
        build.flag("-std=c++20").flag("-Wno-unused-parameter");
    }
    build.compile("yap_voice_track_audio");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        android_export();
    }
}

/// Android: what the app's packaging needs from the prebuilt WebRTC, written
/// to the folder `YAP_WEBRTC_EXPORT_DIR` names (the app's CMake sets it):
///
/// - `libwebrtc.jar`, the library's Java side (`livekit.org.webrtc.*`), for
///   the APK;
/// - `webrtc_jni.rsp`, a linker response file that keeps every
///   `Java_livekit_org_webrtc_*` entry point. Java finds them by name in
///   the app's shared library, and nothing in the native code refers to
///   them, so the link would drop them. (webrtc-sys asks for the same with
///   `cargo:rustc-link-arg`, which only reaches a crate cargo links
///   itself, not a staticlib linked by CMake.)
#[cfg(feature = "libwebrtc")]
fn android_export() {
    println!("cargo:rerun-if-env-changed=YAP_WEBRTC_EXPORT_DIR");
    println!("cargo:rerun-if-env-changed=ANDROID_NDK_HOME");
    let Some(out) = std::env::var_os("YAP_WEBRTC_EXPORT_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    let webrtc = webrtc_sys_build::webrtc_dir();
    std::fs::create_dir_all(&out).expect("YAP_WEBRTC_EXPORT_DIR");
    std::fs::copy(webrtc.join("libwebrtc.jar"), out.join("libwebrtc.jar"))
        .expect("libwebrtc.jar in the WebRTC prebuilt");
    let toolchain = webrtc_sys_build::android_ndk_toolchain().expect("the Android NDK");
    let listing = std::process::Command::new(toolchain.join("bin/llvm-nm"))
        .arg("--defined-only")
        .arg("--extern-only")
        .arg("--format=just-symbols")
        .arg(webrtc.join("lib/libwebrtc.a"))
        .output()
        .expect("llvm-nm from the NDK");
    assert!(listing.status.success(), "llvm-nm failed on libwebrtc.a");
    let mut symbols: Vec<&str> = std::str::from_utf8(&listing.stdout)
        .expect("symbol names")
        .lines()
        .map(str::trim)
        .filter(|name| name.starts_with("Java_livekit_org_webrtc_"))
        .collect();
    symbols.sort_unstable();
    symbols.dedup();
    assert!(!symbols.is_empty(), "no JNI entry points in libwebrtc.a");
    let args: String = symbols
        .iter()
        .map(|name| format!("-Wl,--undefined={name}\n"))
        .collect();
    std::fs::write(out.join("webrtc_jni.rsp"), args).expect("webrtc_jni.rsp");
}
