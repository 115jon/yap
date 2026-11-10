// CI only (docs/INSTALLER.md, "CI"): the RUSTC_WRAPPER in front of sccache.
//
// yap_native's rustc command line (cxx-qt's link arguments) is longer than
// Windows allows, so cargo passes it as an @argfile. sccache expands that
// file and then cannot start rustc ("The filename or extension is too
// long", os error 206). A staticlib is never cached anyway, so such a call,
// and any other yap_native call, goes straight to the compiler; everything
// else goes to sccache.
//
// The file is named sccache.exe so that `cc` in build scripts, which takes
// a RUSTC_WRAPPER named sccache as its compiler wrapper too, still sends
// C and C++ through it.
//
//   rustc -O desktop-native/tools/ci/sccache-wrapper.rs -o <dir>/sccache.exe
//   RUSTC_WRAPPER=<dir>/sccache.exe  YAP_SCCACHE=<the real sccache>
use std::env;
use std::ffi::OsString;
use std::process::{exit, Command};

fn main() {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let Some((compiler, rest)) = args.split_first() else {
        eprintln!("sccache-wrapper: no compiler given");
        exit(2);
    };
    let argfile = rest.iter().any(|a| a.to_string_lossy().starts_with('@'));
    let app = rest
        .windows(2)
        .any(|w| w[0] == "--crate-name" && w[1] == "yap_native");
    let status = if argfile || app {
        Command::new(compiler).args(rest).status()
    } else {
        let Some(sccache) = env::var_os("YAP_SCCACHE") else {
            eprintln!("sccache-wrapper: YAP_SCCACHE is not set");
            exit(2);
        };
        Command::new(sccache).args(&args).status()
    };
    match status {
        Ok(s) => exit(s.code().unwrap_or(1)),
        Err(e) => {
            eprintln!("sccache-wrapper: {e}");
            exit(1);
        }
    }
}
