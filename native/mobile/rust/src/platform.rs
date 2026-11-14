//! What the mobile app (this crate; the archived UniFFI baseline shared
//! this file) needs from the platform under the unchanged
//! core: an HTTP client on rustls (Android has no system OpenSSL for
//! native-tls) and the core's stderr log in logcat.

use std::sync::Once;

// ---- TLS -------------------------------------------------------------------

/// rustls with ring and the bundled web roots: no system OpenSSL on
/// Android, and nothing to initialise over JNI. (A release build would use
/// rustls-platform-verifier, which honours user-installed CAs.)
pub fn http_client() -> reqwest::Client {
    static PROVIDER: Once = Once::new();
    PROVIDER.call_once(|| {
        // The gateway's tokio-tungstenite builds its own config from the
        // process default.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    tls.alpn_protocols = vec![b"http/1.1".to_vec()];
    reqwest::Client::builder()
        .use_preconfigured_tls(tls)
        .user_agent(concat!("YapMobile/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("HTTP client")
}

// ---- Logging ---------------------------------------------------------------

/// The core logs with `eprintln!` (the desktop's log file). Android drops
/// stderr, so pipe it into logcat under the tag `yap-core`.
#[cfg(target_os = "android")]
pub fn stderr_to_logcat() {
    use std::io::{BufRead, BufReader};
    use std::os::fd::FromRawFd;

    #[link(name = "log")]
    extern "C" {
        fn __android_log_write(
            prio: i32,
            tag: *const std::ffi::c_char,
            text: *const std::ffi::c_char,
        ) -> i32;
    }
    const INFO: i32 = 4;

    let mut fds = [0; 2];
    // SAFETY: plain pipe/dup2 on fds this function owns; the read end moves
    // into the thread's File, which closes it.
    unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 || libc::dup2(fds[1], libc::STDERR_FILENO) < 0 {
            return;
        }
        libc::close(fds[1]);
    }
    let read = unsafe { std::fs::File::from_raw_fd(fds[0]) };
    let _ = std::thread::Builder::new()
        .name("stderr-logcat".into())
        .spawn(move || {
            let tag = c"yap-core";
            // Bytes, not lines of UTF-8: one bad line must not end the
            // reader (a closed pipe would make every later eprintln! panic).
            let mut read = BufReader::new(read);
            let mut line = Vec::new();
            while read.read_until(b'\n', &mut line).unwrap_or(0) > 0 {
                let text = String::from_utf8_lossy(&line).trim_end().replace('\0', "");
                line.clear();
                if let Ok(text) = std::ffi::CString::new(text) {
                    // SAFETY: both pointers are valid NUL-terminated strings.
                    unsafe { __android_log_write(INFO, tag.as_ptr(), text.as_ptr()) };
                }
            }
        });
}

#[cfg(not(target_os = "android"))]
pub fn stderr_to_logcat() {}
