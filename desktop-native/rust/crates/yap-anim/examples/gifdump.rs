//! Decodes GIFs the way the app streams them and writes every frame out, for
//! comparing against another decoder (performance pass 8 compared Qt's).
//!
//!   cargo run -p yap-anim --example gifdump -- <out dir> <file.gif>...
//!
//! For each file it writes `<out>/<stem>/info.txt` (canvas, loops, delays)
//! and `<out>/<stem>/<n>.rgba`: frame n at the canvas size, premultiplied
//! RGBA, 4 bytes a pixel, no header. A file the stream refuses gets an
//! `error.txt` instead.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use yap_anim::gifstream::{scan, GifStream};

fn dump(out: &Path, file: &Path) -> Result<String, String> {
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let info = scan(&bytes).map_err(|e| e.to_string())?;
    let scanned = started.elapsed();
    let mut stream = GifStream::new(Arc::from(bytes), &info);
    let size = info.width as usize * info.height as usize * 4;
    // Timed without the file writes: every frame decoded and composited
    // into its own buffer (best of three passes), as the app's worker does.
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut decoded = std::time::Duration::MAX;
    for _ in 0..3 {
        frames.clear();
        let started = Instant::now();
        for index in 0..info.frames() {
            if !stream.seek(index).map_err(|e| e.to_string())? {
                break;
            }
            let mut frame = vec![0u8; size];
            stream.write(&mut frame, info.width, info.height, info.width as usize * 4);
            frames.push(frame);
        }
        decoded = decoded.min(started.elapsed());
    }
    let written = frames.len();
    for (index, frame) in frames.iter().enumerate() {
        fs::write(out.join(format!("{index}.rgba")), frame).map_err(|e| e.to_string())?;
    }
    let text = format!(
        "width {}\nheight {}\nframes {}\ndecoded {}\nloops {}\ndelays {}\nscan_us {}\ndecode_us {}\n",
        info.width,
        info.height,
        info.frames(),
        written,
        info.loops,
        info.delays.iter().map(u32::to_string).collect::<Vec<_>>().join(","),
        scanned.as_micros(),
        decoded.as_micros(),
    );
    Ok(text)
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(out) = args.next().map(PathBuf::from) else {
        eprintln!("usage: gifdump <out dir> <file.gif>...");
        std::process::exit(2);
    };
    for file in args.map(PathBuf::from) {
        let stem = file.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let dir = out.join(&stem);
        if let Err(error) = fs::create_dir_all(&dir) {
            eprintln!("{}: {error}", dir.display());
            continue;
        }
        match dump(&dir, &file) {
            Ok(info) => {
                let _ = fs::write(dir.join("info.txt"), info);
                println!("{stem}: ok");
            }
            Err(error) => {
                let _ = fs::write(dir.join("error.txt"), &error);
                println!("{stem}: {error}");
            }
        }
    }
}
