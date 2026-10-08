//! ort_probe: load the bundled onnxruntime, load YuNet + SFace, print their I/O,
//! run them on an image and time it.
//!
//! usage: ort_probe <onnxruntime lib> <models dir> [image]
//!   ort_probe ./onnxruntime.dll ./models snapshot_cam0.jpg
//!
//! What "pass" means for milestone 1:
//!   - the DLL loads (version matches the ort crate)
//!   - both models load and run on CPU
//!   - SFace gives a 128-d vector; latencies are a few ms / tens of ms
//! Decoding YuNet output (boxes, landmarks, NMS) is milestone 2.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use image::{imageops::FilterType, RgbImage};
use ndarray::Array4;
use ort::{session::Session, value::Tensor};

const YUNET: &str = "face_detection_yunet_2023mar.onnx";
const SFACE: &str = "face_recognition_sface_2021dec.onnx";
const RUNS: usize = 20;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        bail!("usage: ort_probe <onnxruntime lib> <models dir> [image]");
    }
    let lib = PathBuf::from(&args[1]);
    let models = PathBuf::from(&args[2]);

    // 1) bundled runtime, never a system / downloaded one
    ort::init_from(lib.to_string_lossy().to_string())
        .commit()
        .with_context(|| format!("failed to load onnxruntime from {}", lib.display()))?;
    println!("onnxruntime loaded from {}", lib.display());

    let img = match args.get(3) {
        Some(p) => image::open(p).with_context(|| format!("open {p}"))?.to_rgb8(),
        None => RgbImage::from_pixel(640, 480, image::Rgb([128, 128, 128])),
    };
    println!("input image: {}x{}", img.width(), img.height());

    // 2) YuNet: detector. Input is BGR, NCHW, float 0..255. 640x640 here.
    let mut yunet = load(&models.join(YUNET))?;
    let x = to_bgr_nchw(&img, 640, 640);
    bench("yunet", &mut yunet, x)?;

    // 3) SFace: recognizer. Input is an ALIGNED 112x112 face, BGR NCHW 0..255.
    //    Here we just squash the whole image: the vector is meaningless,
    //    we only check shape, norm and speed.
    let mut sface = load(&models.join(SFACE))?;
    let x = to_bgr_nchw(&img, 112, 112);
    let emb = bench("sface", &mut sface, x)?;
    let norm = emb.iter().map(|v| v * v).sum::<f32>().sqrt();
    println!("sface embedding: dim={} L2 norm={norm:.3} first={:?}", emb.len(), &emb[..4.min(emb.len())]);
    if emb.len() != 128 {
        println!("WARNING: expected 128-d embedding");
    }
    Ok(())
}

fn load(path: &Path) -> Result<Session> {
    let s = Session::builder()?
        .with_intra_threads(2)? // leave CPU for the rest of the reception PC
        .commit_from_file(path)
        .with_context(|| format!("failed to load model {}", path.display()))?;
    println!("\nmodel {}", path.display());
    for i in &s.inputs {
        println!("  in  {} : {:?}", i.name, i.input_type);
    }
    for o in &s.outputs {
        println!("  out {} : {:?}", o.name, o.output_type);
    }
    Ok(s)
}

/// Runs the model RUNS times on the same input, prints all output shapes and
/// timing, and returns the first output flattened.
fn bench(name: &str, session: &mut Session, input: Array4<f32>) -> Result<Vec<f32>> {
    let input_name = session.inputs[0].name.clone();
    let mut first_out = Vec::new();
    let mut times = Vec::with_capacity(RUNS);

    for run in 0..RUNS {
        let tensor = Tensor::from_array(input.clone())?;
        let t = Instant::now();
        let outputs = session.run(ort::inputs![input_name.as_str() => tensor])?;
        times.push(t.elapsed().as_secs_f64() * 1000.0);

        if run == 0 {
            for (oname, value) in outputs.iter() {
                let arr = value.try_extract_array::<f32>()?;
                println!("  {name} out {oname}: shape {:?}", arr.shape());
                if first_out.is_empty() {
                    first_out = arr.iter().copied().collect();
                }
            }
        }
    }

    // first run includes graph optimisation / allocation, report it apart
    let warm = &times[1..];
    let avg = warm.iter().sum::<f64>() / warm.len() as f64;
    let max = warm.iter().cloned().fold(0.0, f64::max);
    println!("  {name}: first run {:.1} ms, then avg {avg:.1} ms, max {max:.1} ms", times[0]);
    Ok(first_out)
}

/// RGB image -> resized BGR NCHW float tensor with 0..255 values (what both models expect).
fn to_bgr_nchw(img: &RgbImage, w: u32, h: u32) -> Array4<f32> {
    let r = image::imageops::resize(img, w, h, FilterType::Triangle);
    let mut x = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
    for (px, py, p) in r.enumerate_pixels() {
        let (xx, yy) = (px as usize, py as usize);
        x[[0, 0, yy, xx]] = p[2] as f32; // B
        x[[0, 1, yy, xx]] = p[1] as f32; // G
        x[[0, 2, yy, xx]] = p[0] as f32; // R
    }
    x
}
