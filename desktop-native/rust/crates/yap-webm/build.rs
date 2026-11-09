//! With the `libvpx` feature, links LiveKit's `webrtc.lib` into this
//! crate's own test binaries (the app links it through yap-voice). Link
//! arguments from a build script never reach the app's static library.

fn main() {
    println!("cargo:rerun-if-env-changed=LK_CUSTOM_WEBRTC");
    println!("cargo:rerun-if-env-changed=YAP_LIBWEBRTC_DIR");
    if std::env::var_os("CARGO_FEATURE_LIBVPX").is_none() {
        return;
    }
    let dir = ["LK_CUSTOM_WEBRTC", "YAP_LIBWEBRTC_DIR"]
        .iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .find(|dir| dir.join("lib/webrtc.lib").exists());
    match dir {
        Some(dir) => println!(
            "cargo:rustc-link-arg={}",
            dir.join("lib/webrtc.lib").display()
        ),
        None => println!(
            "cargo:warning=yap-webm: no webrtc.lib (set LK_CUSTOM_WEBRTC); tests that decode will not link"
        ),
    }
}
