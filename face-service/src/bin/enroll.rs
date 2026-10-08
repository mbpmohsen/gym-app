//! enroll (dev CLI): enroll one member into a gallery FOLDER (used by `live`).
//! The service enrolls through its API instead; both share `EnrollSession`.
//!
//! usage: enroll <onnxruntime lib> <models dir> <gallery dir> <member_id> <source> [--samples 8] [--fps 10]

use std::path::PathBuf;

use anyhow::{bail, Result};
use face_service::{
    camera,
    detector::Detector,
    enrollment::{valid_member_id, EnrollSession, EnrollState},
    gallery::write_emb,
    recognizer::Recognizer,
};

fn main() -> Result<()> {
    face_service::logging::init_console();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let mut pos = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a.starts_with("--") {
            it.next();
        } else {
            pos.push(a.as_str());
        }
    }
    if pos.len() < 5 {
        bail!("usage: enroll <onnxruntime lib> <models dir> <gallery dir> <member_id> <source> [--samples 8] [--fps 10]");
    }
    let target: usize = flag("--samples").map(|v| v.parse()).transpose()?.unwrap_or(8);
    let fps: f32 = flag("--fps").map(|v| v.parse()).transpose()?.unwrap_or(10.0);
    let member = pos[3];
    if !valid_member_id(member) {
        bail!("member_id: 1-64 chars of a-z A-Z 0-9 - _");
    }

    face_service::init_runtime(Some(&PathBuf::from(pos[0])))?;
    let models = PathBuf::from(pos[1]);
    let mut det = Detector::new(&models.join("face_detection_yunet_2023mar.onnx"), 2)?;
    let mut rec = Recognizer::new(&models.join("face_recognition_sface_2021dec.onnx"), 2)?;
    let out = PathBuf::from(pos[2]).join(member);
    if out.exists() && std::fs::read_dir(&out)?.next().is_some() {
        bail!("{} already has samples; delete the folder to re-enroll", out.display());
    }

    let source = camera::open_source(pos[4], fps)?;
    println!("source: {}\nlook at the camera; slowly move your head a little left/right/up/down\n", source.description);

    let mut s = EnrollSession::new(member, target);
    let (mut last_seq, mut last_hint) = (0, String::new());
    while s.state == EnrollState::Collecting {
        let Some(frame) = source.latest.wait_newer(last_seq) else { break };
        last_seq = frame.seq;
        if s.feed(&mut det, &mut rec, &frame.image, frame.at)? {
            println!("  sample {}/{} ✔", s.kept.len(), s.target);
        } else if s.hint != last_hint {
            println!("  … {}", s.hint);
        }
        last_hint = s.hint.clone();
    }
    if s.state != EnrollState::Ready {
        bail!("enrollment failed: {} ({} samples)", s.hint, s.kept.len());
    }
    std::fs::create_dir_all(&out)?;
    for (n, (e, crop)) in s.kept.iter().zip(&s.crops).enumerate() {
        write_emb(&out.join(format!("{}.emb", n + 1)), e)?;
        crop.save(out.join(format!("{}.jpg", n + 1)))?;
    }
    println!(
        "\nenrolled {member}: {} samples in {} (diversity {:.3})",
        s.kept.len(),
        out.display(),
        s.diversity().unwrap_or(1.0)
    );
    Ok(())
}
