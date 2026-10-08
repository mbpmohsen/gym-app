//! live: the recognition loop, printing events to the terminal.
//!
//! usage: live <onnxruntime lib> <models dir> <gallery dir> <source> [options]
//!   source: auto | <camera index> | <folder of frames to replay>
//!   --threshold 0.363   --cooldown 600 (seconds)   --fps 10 (replay only)   --verbose

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use face_service::{
    camera,
    detector::Detector,
    engine::{Engine, EngineConfig, Event},
    gallery::Gallery,
    recognizer::Recognizer,
};

fn main() -> Result<()> {
    face_service::logging::init_console();
    let (pos, opt) = parse_args();
    if pos.len() < 4 {
        bail!("usage: live <onnxruntime lib> <models dir> <gallery dir> <source> [--threshold x] [--cooldown s] [--fps n] [--verbose]");
    }
    face_service::init_runtime(Some(&PathBuf::from(&pos[0])))?;
    let models = PathBuf::from(&pos[1]);
    let mut det = Detector::new(&models.join("face_detection_yunet_2023mar.onnx"), 2)?;
    let mut rec = Recognizer::new(&models.join("face_recognition_sface_2021dec.onnx"), 2)?;

    let gallery = Gallery::load_dir(&PathBuf::from(&pos[2]), &mut det, &mut rec)?;
    println!("gallery: {} member(s), {} sample(s)", gallery.member_count(), gallery.sample_count());

    let mut cfg = EngineConfig::default();
    if let Some(v) = opt("threshold") {
        cfg.threshold = v.parse()?;
    }
    if let Some(v) = opt("cooldown") {
        cfg.cooldown = Duration::from_secs_f32(v.parse()?);
    }
    let verbose = opt("verbose").is_some();
    let fps: f32 = opt("fps").map(|v| v.parse()).transpose()?.unwrap_or(10.0);

    let source = camera::open_source(&pos[3], fps)?;
    println!("source: {}", source.description);
    println!("threshold {} | {}/{} votes | cooldown {:?}\n", cfg.threshold, cfg.votes_needed, cfg.window, cfg.cooldown);

    let mut engine = Engine::new(det, rec, gallery, cfg);
    let start = Instant::now();
    let (mut last_seq, mut processed, mut dropped) = (0u64, 0u64, 0u64);
    let mut busy = Duration::ZERO;
    let mut last_stats = Instant::now();

    while let Some(frame) = source.latest.wait_newer(last_seq) {
        dropped += frame.seq - last_seq - 1;
        last_seq = frame.seq;

        let t = Instant::now();
        let (events, infos) = engine.process(&frame.image, frame.at)?;
        busy += t.elapsed();
        processed += 1;

        let ts = format!("[{:7.2}s]", frame.at.duration_since(start).as_secs_f32());
        if verbose {
            for i in &infos {
                let what = match (&i.rejected, &i.best) {
                    (Some(r), _) => format!("rejected: {r}"),
                    (None, Some(c)) => format!("best {} {:.3}", c.member_id, c.score),
                    (None, None) => "no gallery".into(),
                };
                println!("{ts}   frame {} track {}: {what}", frame.seq, i.track_id);
            }
        }
        for ev in events {
            match ev {
                Event::Recognized { track_id, member_id, score } => {
                    println!("{ts} ✅ RECOGNIZED  {member_id}  (score {score:.3}, track {track_id})")
                }
                Event::Unknown { track_id } => println!("{ts} ❔ UNKNOWN person (track {track_id})"),
                Event::Uncertain { track_id, candidates } => {
                    let c: Vec<String> = candidates.iter().map(|c| format!("{} {:.3}", c.member_id, c.score)).collect();
                    println!("{ts} ⚠️  UNCERTAIN (track {track_id}): {}", c.join(", "))
                }
                Event::Suppressed { track_id, member_id, remaining } => println!(
                    "{ts}    (cooldown) {member_id} seen again, track {track_id}, {}s left",
                    remaining.as_secs()
                ),
            }
        }

        if last_stats.elapsed() >= Duration::from_secs(10) {
            println!(
                "{ts}    stats: {processed} frames processed, {dropped} dropped, avg {:.1} ms/frame",
                busy.as_secs_f64() * 1e3 / processed.max(1) as f64
            );
            last_stats = Instant::now();
        }
    }
    println!(
        "\nsource ended. {processed} frames processed, {dropped} dropped, avg {:.1} ms/frame",
        busy.as_secs_f64() * 1e3 / processed.max(1) as f64
    );
    Ok(())
}

/// Positional args + a lookup for `--key value` / `--flag`.
fn parse_args() -> (Vec<String>, impl Fn(&str) -> Option<String>) {
    let mut pos = Vec::new();
    let mut opts = std::collections::HashMap::new();
    let mut it = std::env::args().skip(1).peekable();
    while let Some(a) = it.next() {
        if let Some(k) = a.strip_prefix("--") {
            let v = match it.peek() {
                Some(n) if !n.starts_with("--") => it.next().unwrap(),
                _ => String::new(),
            };
            opts.insert(k.to_string(), v);
        } else {
            pos.push(a);
        }
    }
    (pos, move |k: &str| opts.get(k).cloned())
}
