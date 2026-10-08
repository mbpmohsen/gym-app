//! YuNet face detector (face_detection_yunet_2023mar.onnx).
//!
//! The model has a FIXED 640x640 input. Frames are letterboxed: scaled down to fit
//! (never up), placed at the top-left of a black 640x640 canvas. Because the
//! padding is only on the right/bottom, mapping back is just a divide by `scale`.
//!
//! Output decoding follows OpenCV's FaceDetectorYN (modules/objdetect/src/face_detect.cpp).

use std::path::Path;

use anyhow::Result;
use image::{imageops::FilterType, RgbImage};
use ndarray::Array4;
use ort::{session::Session, value::Tensor};

pub const INPUT_SIZE: u32 = 640;
const STRIDES: [usize; 3] = [8, 16, 32];

#[derive(Debug, Clone)]
pub struct Face {
    /// x, y, width, height in original image pixels
    pub bbox: [f32; 4],
    /// right eye, left eye, nose tip, right mouth corner, left mouth corner
    /// (from the viewer's perspective: first point is on the image's left side)
    pub landmarks: [[f32; 2]; 5],
    pub score: f32,
}

impl Face {
    pub fn area(&self) -> f32 {
        self.bbox[2] * self.bbox[3]
    }
}

pub struct Detector {
    session: Session,
    input_name: String,
    pub score_threshold: f32,
    pub nms_threshold: f32,
}

impl Detector {
    pub fn new(model: &Path, threads: usize) -> Result<Self> {
        let session = crate::session(model, threads)?;
        let input_name = session.inputs[0].name.clone();
        Ok(Self { session, input_name, score_threshold: 0.9, nms_threshold: 0.3 })
    }

    /// All faces above the score threshold, best first.
    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Face>> {
        let (input, scale) = letterbox(img);
        let tensor = Tensor::from_array(input)?;
        let outputs = self.session.run(ort::inputs![self.input_name.as_str() => tensor])?;

        let size = INPUT_SIZE as usize;
        let mut faces = Vec::new();
        for stride in STRIDES {
            let cols = size / stride;
            let rows = size / stride;
            let cls = outputs[format!("cls_{stride}").as_str()].try_extract_array::<f32>()?;
            let obj = outputs[format!("obj_{stride}").as_str()].try_extract_array::<f32>()?;
            let bbox = outputs[format!("bbox_{stride}").as_str()].try_extract_array::<f32>()?;
            let kps = outputs[format!("kps_{stride}").as_str()].try_extract_array::<f32>()?;
            let (cls, obj) = (cls.as_slice().unwrap(), obj.as_slice().unwrap());
            let (bbox, kps) = (bbox.as_slice().unwrap(), kps.as_slice().unwrap());
            let s = stride as f32;

            for r in 0..rows {
                for c in 0..cols {
                    let i = r * cols + c;
                    let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                    if score < self.score_threshold {
                        continue;
                    }
                    let b = &bbox[i * 4..i * 4 + 4];
                    let cx = (c as f32 + b[0]) * s;
                    let cy = (r as f32 + b[1]) * s;
                    let w = b[2].exp() * s;
                    let h = b[3].exp() * s;
                    let mut landmarks = [[0.0; 2]; 5];
                    for (n, lm) in landmarks.iter_mut().enumerate() {
                        lm[0] = (kps[i * 10 + 2 * n] + c as f32) * s / scale;
                        lm[1] = (kps[i * 10 + 2 * n + 1] + r as f32) * s / scale;
                    }
                    faces.push(Face {
                        bbox: [(cx - w / 2.0) / scale, (cy - h / 2.0) / scale, w / scale, h / scale],
                        landmarks,
                        score,
                    });
                }
            }
        }
        Ok(nms(faces, self.nms_threshold))
    }
}

/// RGB image -> 1x3x640x640 BGR float tensor (0..255), plus the scale applied.
fn letterbox(img: &RgbImage) -> (Array4<f32>, f32) {
    let size = INPUT_SIZE as f32;
    let scale = (size / img.width() as f32).min(size / img.height() as f32).min(1.0);
    let resized;
    let src = if scale < 1.0 {
        let w = (img.width() as f32 * scale).round() as u32;
        let h = (img.height() as f32 * scale).round() as u32;
        resized = image::imageops::resize(img, w, h, FilterType::Triangle);
        &resized
    } else {
        img
    };

    let n = INPUT_SIZE as usize;
    let mut x = Array4::<f32>::zeros((1, 3, n, n));
    for (px, py, p) in src.enumerate_pixels() {
        let (xx, yy) = (px as usize, py as usize);
        x[[0, 0, yy, xx]] = p[2] as f32; // B
        x[[0, 1, yy, xx]] = p[1] as f32; // G
        x[[0, 2, yy, xx]] = p[0] as f32; // R
    }
    (x, scale)
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    inter / (a[2] * a[3] + b[2] * b[3] - inter)
}

fn nms(mut faces: Vec<Face>, threshold: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Face> = Vec::new();
    for f in faces {
        if keep.iter().all(|k| iou(&k.bbox, &f.bbox) <= threshold) {
            keep.push(f);
        }
    }
    keep
}
