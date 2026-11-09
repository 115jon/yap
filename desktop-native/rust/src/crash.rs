//! Minidumps on a crash. `crash-handler` catches an unhandled exception, an
//! abort, a pure virtual call, an invalid CRT parameter or heap corruption
//! anywhere in the process (Qt, libwebrtc and Rust alike), and
//! `minidump-writer` writes the dump with `MiniDumpWriteDump`. Dumps go to
//! the profile's own `crashes` folder (`%LOCALAPPDATA%\Yap.Data\crashes`, or
//! `...\profiles\<name>\crashes`); nothing is uploaded. The next launch
//! offers to open the folder (cpp/crashnotice.*).
//!
//! `YAP_TEST_CRASH=1` crashes the app on purpose a few seconds after the
//! window shows (main.cpp calls [`yap_test_crash`]), to check the path.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Dumps kept: the oldest go when a new run starts with more than this.
const KEEP: usize = 10;

/// Threads, their stacks and unloaded modules: enough to symbolize a crash
/// at a few MB. No heap pointed at by the stacks (that is where message
/// text and tokens would be); stacks can still hold some app memory.
fn dump_type() -> minidump_writer::MinidumpType {
    use minidump_writer::MinidumpType as T;
    T::Normal | T::WithThreadInfo | T::WithUnloadedModules | T::IgnoreInaccessibleMemory
}

/// The dumps in `dir`, newest first.
fn dumps(dir: &Path) -> Vec<(SystemTime, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "dmp"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    found.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    found
}

/// Deletes the empty dumps: every run creates its file up front, and a run
/// that did not crash leaves it empty. Left in place they would crowd real
/// dumps out of the kept ones. Called before this run makes its own file.
fn remove_empty(dir: &Path) {
    for (_, path) in dumps(dir) {
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == 0) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Deletes all but the newest `keep` dumps; the number deleted.
fn prune(dir: &Path, keep: usize) -> usize {
    dumps(dir)
        .into_iter()
        .skip(keep)
        .filter(|(_, path)| std::fs::remove_file(path).is_ok())
        .count()
}

/// `yap-<unix seconds>-<pid>.dmp`: unique per run, sorted by time.
fn dump_name(now: SystemTime, pid: u32) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("yap-{secs}-{pid}.dmp")
}

/// Installs the handler; dumps go to `dir`. `false` when it could not be
/// attached (the app runs on without it).
fn install(dir: PathBuf) -> bool {
    if let Err(error) = std::fs::create_dir_all(&dir) {
        eprintln!("[crash] no dump folder: {error}");
        return false;
    }
    remove_empty(&dir);
    // This run adds one dump, so keep one fewer.
    let pruned = prune(&dir, KEEP - 1);
    if pruned > 0 {
        eprintln!("[crash] {pruned} old dump(s) deleted");
    }
    // Everything the handler needs is made now, the file included: it runs
    // in a broken process, where opening one allocates. A run that does not
    // crash leaves it empty; the next start deletes it.
    let path = dir.join(dump_name(SystemTime::now(), std::process::id()));
    let mut file = match std::fs::File::create(&path) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("[crash] no dump file: {error}");
            return false;
        }
    };
    // SAFETY: the closure only writes the dump into the open file (the
    // crate's documented use); it touches no lock this process could hold
    // at the crash.
    let event = unsafe {
        crash_handler::make_single_crash_event(move |context: &crash_handler::CrashContext| {
            let copy = crash_handler::CrashContext {
                exception_pointers: context.exception_pointers,
                exception_code: context.exception_code,
                process_id: context.process_id,
                thread_id: context.thread_id,
            };
            let written = minidump_writer::minidump_writer::MinidumpWriter::dump_crash_context(
                copy,
                Some(dump_type()),
                &mut file,
            )
            .is_ok();
            crash_handler::write_stderr(if written {
                "[crash] minidump written\n"
            } else {
                "[crash] minidump failed\n"
            });
            crash_handler::CrashEventResult::Handled(written)
        })
    };
    match crash_handler::CrashHandler::attach(event) {
        Ok(handler) => {
            // Attached for the life of the process.
            std::mem::forget(handler);
            eprintln!("[crash] dumps go to {}", dir.display());
            true
        }
        Err(error) => {
            eprintln!("[crash] handler not attached: {error}");
            false
        }
    }
}

/// Installs the crash handler with dumps in `dir` (UTF-16, `len` units).
///
/// # Safety
///
/// `dir` points at `len` readable UTF-16 units.
#[no_mangle]
pub unsafe extern "C" fn yap_crash_install(dir: *const u16, len: usize) -> bool {
    if dir.is_null() || len == 0 {
        return false;
    }
    // SAFETY: the caller's contract.
    let units = unsafe { std::slice::from_raw_parts(dir, len) };
    install(PathBuf::from(OsString::from_wide(units)))
}

/// `YAP_TEST_CRASH=1` only: an access violation, as a real crash would be.
#[no_mangle]
pub extern "C" fn yap_test_crash() {
    eprintln!("[crash] YAP_TEST_CRASH: crashing on purpose");
    // SAFETY: none; this write is the crash under test.
    unsafe { std::ptr::null_mut::<u32>().write_volatile(0xDEAD) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dump_name_should_carry_the_time_and_pid() {
        let at = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(dump_name(at, 42), "yap-1700000000-42.dmp");
    }

    #[test]
    fn remove_empty_should_delete_only_the_empty_dumps() {
        let dir = std::env::temp_dir().join(format!("yap-crash-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("empty.dmp"), b"").expect("write");
        std::fs::write(dir.join("real.dmp"), b"MDMP").expect("write");
        std::fs::write(dir.join("empty.txt"), b"").expect("write");
        remove_empty(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        left.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(left, ["empty.txt", "real.dmp"]);
    }

    #[test]
    fn prune_should_keep_the_newest_dumps() {
        let dir = std::env::temp_dir().join(format!("yap-crash-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        for i in 0..4 {
            let path = dir.join(format!("d{i}.dmp"));
            std::fs::write(&path, b"x").expect("write");
            let at = UNIX_EPOCH + std::time::Duration::from_secs(1_000 + i);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .and_then(|f| f.set_modified(at))
                .expect("mtime");
        }
        std::fs::write(dir.join("notes.txt"), b"x").expect("write");
        prune(&dir, 2);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .expect("read")
            .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().into_owned()))
            .collect();
        left.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(left, ["d2.dmp", "d3.dmp", "notes.txt"]);
    }
}
