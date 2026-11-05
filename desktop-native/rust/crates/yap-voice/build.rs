//! With the `libwebrtc` feature: compiles `cpp/track_audio.cpp` against the
//! same prebuilt WebRTC headers and defines `webrtc-sys` uses.

fn main() {
    println!("cargo:rerun-if-changed=cpp/track_audio.cpp");
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");
    #[cfg(feature = "libwebrtc")]
    webrtc_shim();
}

#[cfg(feature = "libwebrtc")]
fn webrtc_shim() {
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
        .include(&include)
        .include(include.join("third_party/abseil-cpp"))
        .define("WEBRTC_APM_DEBUG_DUMP", "0");
    for (name, value) in webrtc_sys_build::webrtc_defines() {
        build.define(&name, value.as_deref());
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        build.flag("/std:c++20").flag("/EHsc");
    } else {
        build.flag("-std=c++20");
    }
    build.compile("yap_voice_track_audio");
}
