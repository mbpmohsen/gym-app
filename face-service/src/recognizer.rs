//! SFace recognizer (face_recognition_sface_2021dec.onnx): aligned 112x112 face -> 128-d embedding.
//!
//! Channel order: OpenCV's FaceRecognizerSF builds the blob with swapRB=true from a
//! BGR image, so the network actually receives RGB. Confirmed by the golden test.

use std::path::Path;

use anyhow::{ensure, Result};
use image::RgbImage;
use ndarray::Array4;
use ort::{session::Session, value::Tensor};

pub const EMBEDDING_DIM: usize = 128;
/// OpenCV's suggested cosine threshold for SFace. Starting point only: tune on real cameras.
pub const DEFAULT_COSINE_THRESHOLD: f32 = 0.363;

pub type Embedding = [f32; EMBEDDING_DIM];

pub struct Recognizer {
    session: Session,
    input_name: String,
}

impl Recognizer {
    pub fn new(model: &Path, threads: usize) -> Result<Self> {
        let session = crate::session(model, threads)?;
        let input_name = session.inputs[0].name.clone();
        Ok(Self { session, input_name })
    }

    /// L2-normalized embedding of an aligned 112x112 face.
    pub fn embed(&mut self, aligned: &RgbImage) -> Result<Embedding> {
        ensure!(aligned.dimensions() == (112, 112), "expected an aligned 112x112 face");
        let mut x = Array4::<f32>::zeros((1, 3, 112, 112));
        for (px, py, p) in aligned.enumerate_pixels() {
            let (xx, yy) = (px as usize, py as usize);
            x[[0, 0, yy, xx]] = p[0] as f32; // R
            x[[0, 1, yy, xx]] = p[1] as f32; // G
            x[[0, 2, yy, xx]] = p[2] as f32; // B
        }
        let tensor = Tensor::from_array(x)?;
        let outputs = self.session.run(ort::inputs![self.input_name.as_str() => tensor])?;
        let raw = outputs[0].try_extract_array::<f32>()?;
        ensure!(raw.len() == EMBEDDING_DIM, "unexpected embedding size {}", raw.len());

        let norm = raw.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        let mut e = [0.0; EMBEDDING_DIM];
        for (o, v) in e.iter_mut().zip(raw.iter()) {
            *o = v / norm;
        }
        Ok(e)
    }
}

/// Cosine similarity of two L2-normalized embeddings (= dot product). Range -1..1.
pub fn cosine(a: &Embedding, b: &Embedding) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
