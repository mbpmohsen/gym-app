//! cam_probe: list cameras, open one with a fallback chain of format requests,
//! grab N frames, report FPS, save a snapshot.
//!
//! usage: cam_probe [camera_index] [frames]
//!   cam_probe            -> first non-virtual camera, 100 frames
//!   cam_probe 1 300

use std::time::Instant;

use anyhow::{Context, Result};
use nokhwa::{
    native_api_backend,
    pixel_format::RgbFormat,
    query,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let n_frames: u32 = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(100);

    // 1) enumerate
    let backend = native_api_backend().context("no native camera backend on this OS")?;
    let devices = query(backend).context("failed to query cameras")?;
    println!("backend: {backend:?}");
    println!("found {} camera(s):", devices.len());
    for d in &devices {
        println!("  [{}] {} | {}", d.index(), d.human_name(), d.description());
    }

    // 2) pick camera: explicit index, or first one that doesn't look virtual
    let index: u32 = match args.get(1) {
        Some(s) => s.parse()?,
        None => face_service::camera::pick_camera()?,
    };
    println!("\nusing camera {index}");

    // 3) open with a fallback chain; real webcams disagree wildly on what they accept
    let mut cam = open_with_fallback(index)?;
    println!("opened: {} | format: {}", cam.info().human_name(), cam.camera_format());

    cam.open_stream().context("failed to start stream")?;

    // 4) warm-up: many webcams return dark/garbage frames at first (auto-exposure)
    for _ in 0..10 {
        let _ = cam.frame();
    }

    // 5) measure: capture + decode, which is what the real service will do
    let start = Instant::now();
    let mut last = None;
    let mut errors = 0;
    for _ in 0..n_frames {
        match cam.frame().and_then(|f| f.decode_image::<RgbFormat>()) {
            Ok(img) => last = Some(img),
            Err(e) => {
                errors += 1;
                if errors <= 5 {
                    eprintln!("frame error: {e}");
                }
            }
        }
    }
    let secs = start.elapsed().as_secs_f64();
    println!(
        "{n_frames} frames in {secs:.2}s => {:.1} fps (errors: {errors})",
        n_frames as f64 / secs
    );

    cam.stop_stream().ok();

    // 6) snapshot to eyeball color order / exposure
    if let Some(img) = last {
        let path = format!("snapshot_cam{index}.jpg");
        img.save(&path)?;
        println!(
            "saved {path} ({}x{}) — check colors look natural (not blue-ish skin)",
            img.width(),
            img.height()
        );
    }
    Ok(())
}

fn open_with_fallback(index: u32) -> Result<Camera> {
    // list what the camera supports first, useful when a new webcam misbehaves
    match Camera::new(
        CameraIndex::Index(index),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::None),
    ) {
        Ok(mut probe) => match probe.compatible_camera_formats() {
            Ok(mut formats) => {
                formats.sort_by_key(|f| (f.resolution().width(), f.frame_rate()));
                formats.dedup();
                println!("supported formats ({}):", formats.len());
                for f in &formats {
                    println!("  {f}");
                }
            }
            Err(e) => println!("could not list formats: {e}"),
        },
        Err(e) => println!("open with no constraint failed: {e}"),
    }
    face_service::camera::open_camera(index, true)
}
