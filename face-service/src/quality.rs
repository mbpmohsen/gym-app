//! Quality gate: decide from detector output alone whether a face is worth embedding.
//! Bad frames (tiny, profile, cut off) are the main source of wrong matches.

use crate::detector::Face;

#[derive(Debug, Clone)]
pub struct QualityConfig {
    /// Minimum distance between the eyes, in pixels.
    pub min_eye_distance: f32,
    /// Nose position between the eyes, 0..1 (0.5 = frontal). Outside => head turned.
    pub max_yaw_offset: f32,
    /// Maximum head tilt (eye line angle), degrees.
    pub max_roll_deg: f32,
    pub min_score: f32,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self { min_eye_distance: 28.0, max_yaw_offset: 0.3, max_roll_deg: 25.0, min_score: 0.9 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    LowScore,
    TooSmall,
    Turned,
    Tilted,
    CutOff,
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Reject::LowScore => "low detection score",
            Reject::TooSmall => "too far / too small",
            Reject::Turned => "head turned",
            Reject::Tilted => "head tilted",
            Reject::CutOff => "face cut off at frame edge",
        })
    }
}

pub fn check(face: &Face, img_w: u32, img_h: u32, cfg: &QualityConfig) -> Result<(), Reject> {
    if face.score < cfg.min_score {
        return Err(Reject::LowScore);
    }
    let [eye_a, eye_b, nose, _, _] = face.landmarks;
    let (dx, dy) = (eye_b[0] - eye_a[0], eye_b[1] - eye_a[1]);
    let eye_dist = (dx * dx + dy * dy).sqrt();
    if eye_dist < cfg.min_eye_distance {
        return Err(Reject::TooSmall);
    }
    if dy.atan2(dx).to_degrees().abs() > cfg.max_roll_deg {
        return Err(Reject::Tilted);
    }
    // project the nose onto the eye line: 0 = over one eye, 1 = over the other
    let t = ((nose[0] - eye_a[0]) * dx + (nose[1] - eye_a[1]) * dy) / (eye_dist * eye_dist);
    if (t - 0.5).abs() > cfg.max_yaw_offset {
        return Err(Reject::Turned);
    }
    let [x, y, w, h] = face.bbox;
    let tol = 0.05 * w;
    if x < -tol || y < -tol || x + w > img_w as f32 + tol || y + h > img_h as f32 + tol {
        return Err(Reject::CutOff);
    }
    Ok(())
}
