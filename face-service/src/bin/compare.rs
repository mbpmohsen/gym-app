//! compare: are these two photos the same person?
//!
//! usage: compare <onnxruntime lib> <models dir> <image A> <image B> [threshold]
//! Uses the largest face in each image. Saves the aligned crops (aligned_a.png,
//! aligned_b.png) so you can eyeball the alignment.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use face_service::{
    align,
    detector::Detector,
    recognizer::{cosine, Recognizer, DEFAULT_COSINE_THRESHOLD},
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        bail!("usage: compare <onnxruntime lib> <models dir> <image A> <image B> [threshold]");
    }
    face_service::init_runtime(Some(&PathBuf::from(&args[1])))?;
    let models = PathBuf::from(&args[2]);
    let threshold: f32 = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(DEFAULT_COSINE_THRESHOLD);

    let mut det = Detector::new(&models.join("face_detection_yunet_2023mar.onnx"), 2)?;
    let mut rec = Recognizer::new(&models.join("face_recognition_sface_2021dec.onnx"), 2)?;

    let mut embs = Vec::new();
    for (path, tag) in [(&args[3], "a"), (&args[4], "b")] {
        let img = image::open(path).with_context(|| format!("open {path}"))?.to_rgb8();
        let t = Instant::now();
        let faces = det.detect(&img)?;
        let t_det = t.elapsed();
        let face = faces
            .iter()
            .max_by(|x, y| x.area().total_cmp(&y.area()))
            .with_context(|| format!("no face found in {path}"))?;
        let t = Instant::now();
        let aligned = align::align_face(&img, &face.landmarks);
        let e = rec.embed(&aligned)?;
        let t_rec = t.elapsed();
        aligned.save(format!("aligned_{tag}.png"))?;
        println!(
            "{path}: {} face(s), using score {:.3} at [{:.0},{:.0} {:.0}x{:.0}] | detect {:.1}ms, align+embed {:.1}ms",
            faces.len(),
            face.score,
            face.bbox[0], face.bbox[1], face.bbox[2], face.bbox[3],
            t_det.as_secs_f64() * 1e3,
            t_rec.as_secs_f64() * 1e3,
        );
        embs.push(e);
    }

    let score = cosine(&embs[0], &embs[1]);
    let verdict = if score >= threshold { "SAME person" } else { "DIFFERENT people" };
    println!("cosine = {score:.4} (threshold {threshold}) => {verdict}");
    Ok(())
}
