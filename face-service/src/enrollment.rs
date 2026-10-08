//! Enrollment session: collects varied, good-quality samples of one person.
//! Shared by the `enroll` CLI and the service's /v1/enroll API.

use std::time::{Duration, Instant};

use anyhow::Result;
use image::RgbImage;
use serde::Serialize;

use crate::align;
use crate::detector::Detector;
use crate::quality::{self, QualityConfig};
use crate::recognizer::{cosine, Embedding, Recognizer};

/// A new sample must differ from every kept one at least this much (cosine below).
const MAX_SIMILARITY_TO_KEPT: f32 = 0.93;
/// ...but must still be the same person: similarity to the mean of kept samples.
/// Same person measured 0.68-0.90 on a real webcam, different people ~0.1.
const MIN_SIMILARITY_TO_CENTROID: f32 = 0.5;
const MIN_GAP: Duration = Duration::from_millis(300);
pub const TIMEOUT: Duration = Duration::from_secs(60);
pub const MIN_SAMPLES: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrollState {
    Collecting,
    /// target reached (or timeout with enough samples): waiting for commit
    Ready,
    Failed,
}

pub struct EnrollSession {
    pub member_id: String,
    pub target: usize,
    pub kept: Vec<Embedding>,
    pub crops: Vec<RgbImage>,
    pub state: EnrollState,
    /// human-readable guidance for the person in front of the camera
    pub hint: String,
    started: Instant,
    last_kept: Option<Instant>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrollStatus {
    pub member_id: String,
    pub state: EnrollState,
    pub collected: usize,
    pub target: usize,
    pub hint: String,
    /// lowest similarity between two samples (lower = more varied); null until 2 samples
    pub diversity: Option<f32>,
}

impl EnrollSession {
    pub fn new(member_id: &str, target: usize) -> Self {
        Self {
            member_id: member_id.into(),
            target: target.max(MIN_SAMPLES),
            kept: Vec::new(),
            crops: Vec::new(),
            state: EnrollState::Collecting,
            hint: "look at the camera".into(),
            started: Instant::now(),
            last_kept: None,
        }
    }

    /// Feeds one frame. Returns true if a sample was accepted.
    pub fn feed(&mut self, det: &mut Detector, rec: &mut Recognizer, img: &RgbImage, at: Instant) -> Result<bool> {
        if self.state != EnrollState::Collecting {
            return Ok(false);
        }
        if self.started.elapsed() > TIMEOUT {
            self.finish_on_timeout();
            return Ok(false);
        }
        let faces = det.detect(img)?;
        let face = match faces.len() {
            0 => return self.hint("no face"),
            1 => &faces[0],
            _ => return self.hint("more than one face in view"),
        };
        if let Err(r) = quality::check(face, img.width(), img.height(), &QualityConfig::default()) {
            return self.hint(&r.to_string());
        }
        if self.last_kept.is_some_and(|t| at.duration_since(t) < MIN_GAP) {
            return Ok(false);
        }
        let aligned = align::align_face(img, &face.landmarks);
        let e = rec.embed(&aligned)?;
        if self.kept.iter().any(|k| cosine(k, &e) >= MAX_SIMILARITY_TO_KEPT) {
            return self.hint("slowly turn your head a little");
        }
        if !self.kept.is_empty() && cosine(&self.centroid(), &e) < MIN_SIMILARITY_TO_CENTROID {
            return self.hint("a different person is in view; only the member should face the camera");
        }
        self.kept.push(e);
        self.crops.push(aligned);
        self.last_kept = Some(at);
        self.hint = "good, keep moving slowly".into();
        if self.kept.len() >= self.target {
            self.state = EnrollState::Ready;
            self.hint = "done".into();
        }
        Ok(true)
    }

    fn hint(&mut self, h: &str) -> Result<bool> {
        self.hint = h.into();
        Ok(false)
    }

    fn finish_on_timeout(&mut self) {
        if self.kept.len() >= MIN_SAMPLES {
            self.state = EnrollState::Ready;
            self.hint = format!("timeout, {} samples collected", self.kept.len());
        } else {
            self.state = EnrollState::Failed;
            self.hint = format!("timeout with only {} usable samples; improve light and retry", self.kept.len());
        }
    }

    /// Normalized mean of the kept samples.
    fn centroid(&self) -> Embedding {
        let mut c = [0.0; crate::recognizer::EMBEDDING_DIM];
        for k in &self.kept {
            for (a, b) in c.iter_mut().zip(k) {
                *a += b;
            }
        }
        let n = c.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-12);
        c.iter_mut().for_each(|v| *v /= n);
        c
    }

    /// Lowest pairwise similarity between samples.
    pub fn diversity(&self) -> Option<f32> {
        let mut min: Option<f32> = None;
        for i in 0..self.kept.len() {
            for j in i + 1..self.kept.len() {
                let c = cosine(&self.kept[i], &self.kept[j]);
                min = Some(min.map_or(c, |m: f32| m.min(c)));
            }
        }
        min
    }

    pub fn status(&self) -> EnrollStatus {
        EnrollStatus {
            member_id: self.member_id.clone(),
            state: self.state,
            collected: self.kept.len(),
            target: self.target,
            hint: self.hint.clone(),
            diversity: self.diversity(),
        }
    }
}

/// Member ids end up in file names and URLs: keep them boring.
pub fn valid_member_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
