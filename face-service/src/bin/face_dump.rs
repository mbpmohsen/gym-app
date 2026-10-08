//! face_dump: run the full pipeline on one image and print JSON (for the golden test).
//!
//! usage: face_dump <onnxruntime lib> <models dir> <image> [aligned_out.png]

use std::path::PathBuf;

use anyhow::{bail, Result};
use face_service::{align, detector::Detector, recognizer::Recognizer};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        bail!("usage: face_dump <onnxruntime lib> <models dir> <image> [aligned_out.png]");
    }
    face_service::init_runtime(Some(&PathBuf::from(&args[1])))?;
    let models = PathBuf::from(&args[2]);
    let img = image::open(&args[3])?.to_rgb8();

    let mut det = Detector::new(&models.join("face_detection_yunet_2023mar.onnx"), 2)?;
    let mut rec = Recognizer::new(&models.join("face_recognition_sface_2021dec.onnx"), 2)?;

    let faces = det.detect(&img)?;
    let mut json = String::from("{\"faces\":[");
    for (i, f) in faces.iter().enumerate() {
        if i > 0 {
            json.push(',');
        }
        let lm: Vec<String> = f.landmarks.iter().map(|p| format!("[{},{}]", p[0], p[1])).collect();
        json.push_str(&format!(
            "{{\"bbox\":[{},{},{},{}],\"landmarks\":[{}],\"score\":{}}}",
            f.bbox[0], f.bbox[1], f.bbox[2], f.bbox[3], lm.join(","), f.score
        ));
    }
    json.push_str("],\"embedding\":");
    // embedding of the highest-scoring face, same choice the Python side makes
    match faces.first() {
        Some(f) => {
            let aligned = align::align_face(&img, &f.landmarks);
            if let Some(out) = args.get(4) {
                aligned.save(out)?;
            }
            let e = rec.embed(&aligned)?;
            let v: Vec<String> = e.iter().map(|x| x.to_string()).collect();
            json.push_str(&format!("[{}]", v.join(",")));
        }
        None => json.push_str("null"),
    }
    json.push('}');
    println!("{json}");
    Ok(())
}
