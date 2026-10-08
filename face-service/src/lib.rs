//! face-service core: detect (YuNet) -> align (5-point similarity) -> embed (SFace) -> compare.

pub mod align;
pub mod api;
pub mod camera;
pub mod config;
pub mod detector;
pub mod engine;
pub mod enrollment;
pub mod gallery;
pub mod logging;
pub mod quality;
pub mod recognizer;
pub mod server;
#[cfg(windows)]
pub mod service;
pub mod store;
pub mod worker;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Loads the bundled ONNX Runtime library.
///
/// Always uses an ABSOLUTE path: Windows 11 ships its own (older) onnxruntime.dll in
/// System32, and a bare or relative name can resolve to that one instead of ours.
/// With `None`, looks for the library next to the running executable.
pub fn init_runtime(lib: Option<&Path>) -> Result<PathBuf> {
    let path = match lib {
        Some(p) => std::path::absolute(p)?,
        None => {
            let exe = std::env::current_exe()?;
            let name = if cfg!(windows) { "onnxruntime.dll" } else { "libonnxruntime.so" };
            exe.parent().context("exe has no parent dir")?.join(name)
        }
    };
    if !path.is_file() {
        anyhow::bail!("onnxruntime library not found at {}", path.display());
    }
    ort::init_from(path.to_string_lossy().to_string())
        .commit()
        .with_context(|| format!("failed to load onnxruntime from {}", path.display()))?;
    Ok(path)
}

pub(crate) fn session(path: &Path, threads: usize) -> Result<ort::session::Session> {
    ort::session::Session::builder()?
        .with_intra_threads(threads)?
        .commit_from_file(path)
        .with_context(|| format!("failed to load model {}", path.display()))
}
