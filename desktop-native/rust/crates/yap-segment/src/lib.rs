//! Person segmentation for camera backgrounds (`yap_voice::background`).
//!
//! The model is MediaPipe's `selfie_segmenter.tflite` (the one the web
//! loads from `/mediapipe`), converted to ONNX by
//! `tools/convert-selfie-segmenter.py`: input `input` [1, 3, 256, 256] RGB
//! in 0..1, output `mask` [1, 1, 256, 256] person confidence.
//!
//! It runs on ONNX Runtime 1.24 (the DirectML build), loaded at run time
//! from the app's `segmentation` folder (`onnxruntime.dll`,
//! `selfie_segmenter.onnx`; `YAP_SEGMENTATION_DIR` overrides it): DirectML
//! on the GPU when it registers, ONNX Runtime's CPU provider otherwise
//! (`YAP_SEGMENT_CPU=1` forces it). DirectML is the system's (Windows 10
//! 1903 and later ship it in System32); a `DirectML.dll` in the folder wins
//! if one is there. Windows also ships an `onnxruntime.dll` of its own
//! (Windows ML's), so ours is loaded by full path before anything asks for
//! it by name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ort::ep::{DirectML, CPU};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use yap_voice::background::{Segmenter, MASK_SIZE};

/// The model's file name in the segmentation folder.
pub const MODEL_FILE: &str = "selfie_segmenter.onnx";
/// The runtime's file names.
pub const RUNTIME_FILE: &str = "onnxruntime.dll";
pub const DIRECTML_FILE: &str = "DirectML.dll";

/// Where the runtime and model live: `YAP_SEGMENTATION_DIR`, else the
/// `segmentation` folder beside the exe.
pub fn folder() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("YAP_SEGMENTATION_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("segmentation"))
}

/// Whether the folder holds everything a segmenter needs (cheap: no DLL is
/// loaded). The settings row says the effect is unavailable otherwise.
pub fn available() -> bool {
    folder().is_some_and(|dir| {
        [MODEL_FILE, RUNTIME_FILE]
            .iter()
            .all(|file| dir.join(file).is_file())
    })
}

/// Loads `path` by full path, so a later load by name finds this one.
#[cfg(windows)]
fn preload(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::LibraryLoader::{
        LoadLibraryExW, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: a NUL-terminated wide path; the module stays loaded for the
    // process (ONNX Runtime uses it until exit).
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
        )
    };
    if module.is_null() {
        Err(format!(
            "{} would not load ({})",
            path.display(),
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn preload(_: &Path) -> Result<(), String> {
    Ok(())
}

/// ONNX Runtime, loaded once per process.
fn runtime(dir: &Path) -> Result<(), String> {
    static LOADED: OnceLock<Result<(), String>> = OnceLock::new();
    LOADED
        .get_or_init(|| {
            let directml = dir.join(DIRECTML_FILE);
            if directml.is_file() {
                preload(&directml)?;
            }
            let runtime = dir.join(RUNTIME_FILE);
            preload(&runtime)?;
            ort::init_from(&runtime)
                .map_err(|e| format!("ONNX Runtime: {e}"))?
                .with_name("yap-segment")
                .commit();
            Ok(())
        })
        .clone()
}

/// The selfie segmenter on one ONNX Runtime session.
pub struct OrtSegmenter {
    session: Session,
    backend: &'static str,
}

fn session(model: &Path, gpu: bool) -> Result<Session, String> {
    let fail = |e: ort::Error| e.to_string();
    let builder = Session::builder()
        .map_err(fail)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| e.to_string())?
        // One small model: a single thread keeps it off the other cores.
        .with_intra_threads(1)
        .map_err(|e| e.to_string())?;
    let mut builder = if gpu {
        // DirectML takes neither memory patterns nor parallel execution.
        builder
            .with_memory_pattern(false)
            .map_err(|e| e.to_string())?
            .with_parallel_execution(false)
            .map_err(|e| e.to_string())?
            .with_execution_providers([DirectML::default().build().error_on_failure()])
            .map_err(|e| e.to_string())?
    } else {
        builder
            .with_execution_providers([CPU::default().build()])
            .map_err(|e| e.to_string())?
    };
    builder.commit_from_file(model).map_err(fail)
}

impl OrtSegmenter {
    /// Loads the runtime and the model from [`folder`]: DirectML when it
    /// registers, the CPU otherwise.
    pub fn load() -> Result<Self, String> {
        let dir = folder().ok_or("no segmentation folder")?;
        let model = dir.join(MODEL_FILE);
        if !model.is_file() {
            return Err(format!("no model at {}", model.display()));
        }
        runtime(&dir)?;
        // `YAP_SEGMENT_CPU=1`: the CPU provider, as on a machine whose
        // DirectML is missing or too old (tests and comparisons).
        let gpu = !std::env::var("YAP_SEGMENT_CPU").is_ok_and(|v| v == "1");
        match session(&model, gpu) {
            Ok(session) => Ok(Self {
                session,
                backend: if gpu { "DirectML" } else { "CPU" },
            }),
            Err(why) => {
                eprintln!("[camera] DirectML unavailable ({why}); segmenting on the CPU");
                Ok(Self {
                    session: session(&model, false)?,
                    backend: "CPU",
                })
            }
        }
    }
}

impl Segmenter for OrtSegmenter {
    fn segment(&mut self, rgb: &[f32]) -> Result<Vec<f32>, String> {
        let shape = [1usize, 3, MASK_SIZE, MASK_SIZE];
        if rgb.len() != shape.iter().product::<usize>() {
            return Err(format!("{} input values", rgb.len()));
        }
        let input = TensorRef::from_array_view((shape, rgb)).map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs!["input" => input])
            .map_err(|e| e.to_string())?;
        let (_, mask) = outputs["mask"]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(mask.to_vec())
    }

    fn backend(&self) -> String {
        self.backend.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the runtime and model in `YAP_SEGMENTATION_DIR` (skipped
    /// otherwise): a flat grey frame is no person, and a run is fast.
    #[test]
    fn a_flat_frame_should_have_no_person() {
        if !available() {
            eprintln!("segmentation folder missing; skipped");
            return;
        }
        let mut segmenter = OrtSegmenter::load().expect("segmenter");
        let input = vec![0.5f32; 3 * MASK_SIZE * MASK_SIZE];
        let started = std::time::Instant::now();
        let mask = segmenter.segment(&input).expect("mask");
        for _ in 0..20 {
            segmenter.segment(&input).expect("mask");
        }
        let each = started.elapsed() / 21;
        eprintln!("{}: {each:?} a segmentation", segmenter.backend());
        assert_eq!(mask.len(), MASK_SIZE * MASK_SIZE);
        assert!(mask.iter().all(|&c| c < 0.5));
    }
}
