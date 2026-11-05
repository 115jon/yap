#pragma once

// The log file, as the desktop app keeps one (tauri-plugin-log: keep 5,
// local time): %LOCALAPPDATA%\YapNative\logs\yap-native.log. Everything
// written to stderr (Rust's eprintln!, Qt's messages, fprintf) goes through
// a pipe into it, each line stamped with the local time, and is mirrored to
// the original stderr when that is a file or a pipe (not a console, which
// could stall every writer). The file rotates past 5 MB, at
// start and while running, keeping yap-native.1.log to yap-native.4.log.
//
// Nothing secret is logged: the Rust side never prints tokens or tickets.

namespace logfile {

// Call first thing in main, before anything writes to stderr.
void start();

} // namespace logfile
