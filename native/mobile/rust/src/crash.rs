//! A Rust panic's message, kept for the next launch's crash report
//! (android/src/dev/yap/qtproto/crash/CrashReports.java). Android's exit
//! record says the process died of SIGABRT; this file says why. The hook
//! writes `rust-<pid>.txt` into the crash folder the activity named in
//! `YAP_CRASH_DIR` and hands over to the default hook (the logcat line). A
//! panic the process survives (a task's) leaves a file the next launch
//! deletes unread, since no exit record names its pid.
//!
//! The file is raw: the Java side redacts it before anything is shown or
//! shared, and nothing leaves the phone unless the user sends it.
//!
//! The `crash-test` feature (CMake, `YAP_CRASH_TEST=1`; never in release
//! builds) adds [`yap_crash_test`] for the debug trigger.

use std::io::Write;
use std::path::PathBuf;

/// The longest message kept: a panic's text can hold anything it formats.
const MAX_MESSAGE: usize = 4096;

/// Installs the hook (once, from `yap_mobile_init`). Without the folder the
/// default hook alone runs.
pub fn install() {
    let Some(dir) = std::env::var_os("YAP_CRASH_DIR").map(PathBuf::from) else {
        return;
    };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let record = record(
            thread.name().unwrap_or("unnamed"),
            info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
            message(info.payload()),
        );
        let path = dir.join(format!("rust-{}.txt", std::process::id()));
        // Appended: the first panic is usually the cause, a second one (a
        // poisoned lock) the consequence.
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = file.write_all(record.as_bytes());
        }
        previous(info);
    }));
}

/// The panic's text: a `&str` or a `String` payload, as `panic!` makes.
fn message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "(no message)".to_owned()
    }
}

/// One panic, as the file holds it.
fn record(thread: &str, location: Option<String>, mut message: String) -> String {
    if message.len() > MAX_MESSAGE {
        let mut end = MAX_MESSAGE;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
        message.push_str(" [cut]");
    }
    format!(
        "panic on thread '{thread}' at {}\n{message}\n\n",
        location.as_deref().unwrap_or("an unknown location")
    )
}

/// The debug trigger's crashes (`crash-test` builds only): `1` a panic
/// (fatal here: it cannot unwind out of an `extern "C"` function), `2` a
/// segmentation fault. The panic's text has made-up secrets in it, for the
/// report's redaction to remove.
#[cfg(feature = "crash-test")]
#[no_mangle]
pub extern "C" fn yap_crash_test(kind: i32) {
    eprintln!("[crash] crash test {kind}: crashing on purpose");
    if kind == 2 {
        // SAFETY: none; this write is the crash under test.
        unsafe { std::ptr::null_mut::<u32>().write_volatile(0xDEAD) };
    }
    panic!(
        "crash test: Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzcGlrZSJ9.c2lnbmF0dXJl \
         token=Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MGFiY2RlZg from spike@example.com \
         in /data/user/0/dev.yap.app/files/session.bin"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_should_name_the_thread_and_location() {
        let text = record("tokio-1", Some("src/client.rs:10:5".into()), "boom".into());
        assert_eq!(text, "panic on thread 'tokio-1' at src/client.rs:10:5\nboom\n\n");
    }

    #[test]
    fn record_should_cut_a_long_message_on_a_char_boundary() {
        let text = record("t", None, "é".repeat(MAX_MESSAGE));
        assert!(text.contains(" [cut]"));
        assert!(text.len() < MAX_MESSAGE + 100);
    }

    #[test]
    fn message_should_read_str_and_string_payloads() {
        assert_eq!(message(&"a"), "a");
        assert_eq!(message(&String::from("b")), "b");
        assert_eq!(message(&3_u8), "(no message)");
    }
}
