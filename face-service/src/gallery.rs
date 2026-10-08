//! In-memory gallery of enrolled members. Persisted (for now) as a folder:
//!
//!   gallery/<member_id>/*.emb   128 little-endian f32, written by `enroll`
//!   gallery/<member_id>/*.jpg   if a member folder has NO .emb files, its images are
//!                               treated as raw photos (detect -> align -> embed on load)
//!
//! Matching is brute force over every sample; for thousands of members this is
//! well under a millisecond. SQLite replaces the folder in milestone 4.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::align;
use crate::detector::Detector;
use crate::recognizer::{cosine, Embedding, Recognizer, EMBEDDING_DIM};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Candidate {
    pub member_id: String,
    pub score: f32,
}

#[derive(Default)]
pub struct Gallery {
    samples: Vec<(String, Embedding)>,
}

impl Gallery {
    pub fn add(&mut self, member_id: &str, e: Embedding) {
        self.samples.push((member_id.to_string(), e));
    }

    pub fn member_count(&self) -> usize {
        let mut ids: Vec<&String> = self.samples.iter().map(|(id, _)| id).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Best score per member (max over that member's samples), highest first.
    pub fn rank(&self, e: &Embedding, top: usize) -> Vec<Candidate> {
        let mut best: HashMap<&str, f32> = HashMap::new();
        for (id, s) in &self.samples {
            let c = cosine(e, s);
            let entry = best.entry(id.as_str()).or_insert(f32::MIN);
            if c > *entry {
                *entry = c;
            }
        }
        let mut v: Vec<Candidate> =
            best.into_iter().map(|(id, score)| Candidate { member_id: id.to_string(), score }).collect();
        v.sort_by(|a, b| b.score.total_cmp(&a.score));
        v.truncate(top);
        v
    }

    pub fn load_dir(dir: &Path, det: &mut Detector, rec: &mut Recognizer) -> Result<Self> {
        let mut g = Gallery::default();
        if !dir.exists() {
            return Ok(g);
        }
        for member in std::fs::read_dir(dir)? {
            let member = member?.path();
            if !member.is_dir() {
                continue;
            }
            let id = member.file_name().unwrap().to_string_lossy().to_string();
            let files: Vec<_> = std::fs::read_dir(&member)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
            let embs: Vec<_> = files.iter().filter(|p| ext(p) == "emb").collect();
            if !embs.is_empty() {
                for p in embs {
                    g.add(&id, read_emb(p)?);
                }
            } else {
                for p in files.iter().filter(|p| matches!(ext(p).as_str(), "jpg" | "jpeg" | "png")) {
                    let img = image::open(p).with_context(|| format!("open {}", p.display()))?.to_rgb8();
                    let faces = det.detect(&img)?;
                    match faces.iter().max_by(|a, b| a.area().total_cmp(&b.area())) {
                        Some(f) => g.add(&id, rec.embed(&align::align_face(&img, &f.landmarks))?),
                        None => eprintln!("gallery: no face in {}, skipped", p.display()),
                    }
                }
            }
        }
        Ok(g)
    }
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

pub fn write_emb(path: &Path, e: &Embedding) -> Result<()> {
    let bytes: Vec<u8> = e.iter().flat_map(|v| v.to_le_bytes()).collect();
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn read_emb(path: &Path) -> Result<Embedding> {
    let bytes = std::fs::read(path)?;
    if bytes.len() != EMBEDDING_DIM * 4 {
        bail!("{}: expected {} bytes, got {}", path.display(), EMBEDDING_DIM * 4, bytes.len());
    }
    let mut e = [0.0; EMBEDDING_DIM];
    for (i, c) in bytes.chunks_exact(4).enumerate() {
        e[i] = f32::from_le_bytes(c.try_into().unwrap());
    }
    Ok(e)
}
