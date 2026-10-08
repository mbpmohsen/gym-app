//! Live recognition engine: frame -> detect -> quality -> embed -> match -> track -> vote -> events.
//!
//! A decision is never made from a single frame. Faces are tracked across frames
//! (IoU), each good frame casts a vote, and a track is announced once the votes agree.
//! Every track produces at most one announcement (it can only be upgraded to
//! "recognized"), and a recognized member is muted for `cooldown`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use anyhow::Result;
use image::RgbImage;

use crate::align;
use crate::detector::{Detector, Face};
use crate::gallery::{Candidate, Gallery};
use crate::quality::{self, QualityConfig, Reject};
use crate::recognizer::{Recognizer, DEFAULT_COSINE_THRESHOLD};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub threshold: f32,
    /// votes kept per track
    pub window: usize,
    /// votes for the same member (above threshold) needed to announce
    pub votes_needed: usize,
    /// a recognized member isn't announced again for this long
    pub cooldown: Duration,
    /// a track not seen for this long is dropped
    pub track_timeout: Duration,
    /// IoU to continue a track from the previous frame
    pub track_iou: f32,
    pub quality: QualityConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_COSINE_THRESHOLD,
            window: 5,
            votes_needed: 3,
            cooldown: Duration::from_secs(600),
            track_timeout: Duration::from_millis(1500),
            track_iou: 0.3,
            quality: QualityConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Confirmed member. `score` is the mean of the agreeing votes.
    Recognized { track_id: u64, member_id: String, score: f32 },
    /// Several good frames, none close to anyone in the gallery.
    Unknown { track_id: u64 },
    /// Votes disagree or sit around the threshold: let the receptionist pick.
    Uncertain { track_id: u64, candidates: Vec<Candidate> },
    /// Recognized, but within cooldown: no announcement (informational).
    Suppressed { track_id: u64, member_id: String, remaining: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Pending,
    Uncertain,
    Unknown,
    Recognized,
}

struct Track {
    id: u64,
    bbox: [f32; 4],
    last_seen: Instant,
    /// best candidate of each good frame (None = gallery empty)
    votes: VecDeque<Option<Candidate>>,
    state: State,
}

/// Per-frame diagnostics, for logs and the future preview overlay.
#[derive(Debug, Clone)]
pub struct FaceInfo {
    pub track_id: u64,
    pub face: Face,
    pub rejected: Option<Reject>,
    pub best: Option<Candidate>,
}

pub struct Engine {
    pub det: Detector,
    pub rec: Recognizer,
    pub gallery: Gallery,
    pub cfg: EngineConfig,
    tracks: Vec<Track>,
    next_track: u64,
    last_announced: HashMap<String, Instant>,
}

impl Engine {
    pub fn new(det: Detector, rec: Recognizer, gallery: Gallery, cfg: EngineConfig) -> Self {
        Self { det, rec, gallery, cfg, tracks: Vec::new(), next_track: 1, last_announced: HashMap::new() }
    }

    pub fn process(&mut self, img: &RgbImage, now: Instant) -> Result<(Vec<Event>, Vec<FaceInfo>)> {
        let faces = self.det.detect(img)?;
        let mut infos = Vec::with_capacity(faces.len());
        let mut events = Vec::new();

        // 1) associate detections with existing tracks, greedy by IoU
        let mut taken = vec![false; self.tracks.len()];
        let mut assignment = Vec::with_capacity(faces.len());
        for f in &faces {
            let best = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| !taken[*i])
                .map(|(i, t)| (i, iou(&t.bbox, &f.bbox)))
                .filter(|(_, v)| *v >= self.cfg.track_iou)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((i, _)) => {
                    taken[i] = true;
                    assignment.push(i);
                }
                None => {
                    self.tracks.push(Track {
                        id: self.next_track,
                        bbox: f.bbox,
                        last_seen: now,
                        votes: VecDeque::new(),
                        state: State::Pending,
                    });
                    self.next_track += 1;
                    taken.push(true);
                    assignment.push(self.tracks.len() - 1);
                }
            }
        }

        // 2) quality gate, embed, vote, decide
        for (f, ti) in faces.into_iter().zip(assignment) {
            let rejected = quality::check(&f, img.width(), img.height(), &self.cfg.quality).err();
            let mut best = None;
            if rejected.is_none() {
                let e = self.rec.embed(&align::align_face(img, &f.landmarks))?;
                best = self.gallery.rank(&e, 1).into_iter().next();
            }

            let cfg = self.cfg.clone();
            let t = &mut self.tracks[ti];
            t.bbox = f.bbox;
            t.last_seen = now;
            if rejected.is_none() {
                t.votes.push_back(best.clone());
                if t.votes.len() > cfg.window {
                    t.votes.pop_front();
                }
                if let Some(ev) = decide(t, &cfg) {
                    events.push(ev);
                }
            }
            infos.push(FaceInfo { track_id: t.id, face: f, rejected, best });
        }

        // 3) drop stale tracks
        let timeout = self.cfg.track_timeout;
        self.tracks.retain(|t| now.duration_since(t.last_seen) <= timeout);

        // 4) cooldown
        let events = events
            .into_iter()
            .map(|ev| match ev {
                Event::Recognized { track_id, member_id, score } => {
                    match self.last_announced.get(&member_id) {
                        Some(&at) if now.duration_since(at) < self.cfg.cooldown => Event::Suppressed {
                            track_id,
                            remaining: self.cfg.cooldown - now.duration_since(at),
                            member_id,
                        },
                        _ => {
                            self.last_announced.insert(member_id.clone(), now);
                            Event::Recognized { track_id, member_id, score }
                        }
                    }
                }
                other => other,
            })
            .collect();
        Ok((events, infos))
    }
}

fn decide(t: &mut Track, cfg: &EngineConfig) -> Option<Event> {
    if t.state == State::Recognized {
        return None;
    }

    // votes per member, counting only those above threshold
    let mut tally: HashMap<&str, (usize, f32)> = HashMap::new();
    for c in t.votes.iter().flatten().filter(|c| c.score >= cfg.threshold) {
        let e = tally.entry(c.member_id.as_str()).or_default();
        e.0 += 1;
        e.1 += c.score;
    }
    if let Some((id, (n, sum))) = tally.iter().max_by_key(|(_, (n, _))| *n) {
        if *n >= cfg.votes_needed {
            t.state = State::Recognized;
            return Some(Event::Recognized { track_id: t.id, member_id: id.to_string(), score: sum / *n as f32 });
        }
    }

    // only judge "unknown"/"uncertain" on a full window
    if t.votes.len() < cfg.window || t.state != State::Pending {
        return None;
    }
    if tally.is_empty() {
        t.state = State::Unknown;
        return Some(Event::Unknown { track_id: t.id });
    }
    // some votes above threshold but not enough agreement
    let mut best: HashMap<&str, f32> = HashMap::new();
    for c in t.votes.iter().flatten() {
        let e = best.entry(c.member_id.as_str()).or_insert(f32::MIN);
        *e = e.max(c.score);
    }
    let mut candidates: Vec<Candidate> =
        best.into_iter().map(|(id, score)| Candidate { member_id: id.to_string(), score }).collect();
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    candidates.truncate(3);
    t.state = State::Uncertain;
    Some(Event::Uncertain { track_id: t.id, candidates })
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = (a[0] + a[2]).min(b[0] + b[2]);
    let y2 = (a[1] + a[3]).min(b[1] + b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    inter / (a[2] * a[3] + b[2] * b[3] - inter)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(votes: &[Option<(&str, f32)>]) -> Track {
        Track {
            id: 1,
            bbox: [0.0; 4],
            last_seen: Instant::now(),
            votes: votes
                .iter()
                .map(|v| v.map(|(id, s)| Candidate { member_id: id.into(), score: s }))
                .collect(),
            state: State::Pending,
        }
    }

    #[test]
    fn three_agreeing_votes_recognize() {
        let mut t = track(&[Some(("a", 0.5)), Some(("a", 0.6)), Some(("a", 0.7))]);
        let ev = decide(&mut t, &EngineConfig::default());
        assert!(matches!(ev, Some(Event::Recognized { ref member_id, score, .. }) if member_id == "a" && (score - 0.6).abs() < 1e-6));
        // never announced twice
        assert_eq!(decide(&mut t, &EngineConfig::default()), None);
    }

    #[test]
    fn votes_below_threshold_dont_count() {
        let mut t = track(&[Some(("a", 0.5)), Some(("a", 0.5)), Some(("a", 0.30))]);
        assert_eq!(decide(&mut t, &EngineConfig::default()), None); // window not full yet
    }

    #[test]
    fn full_window_all_low_is_unknown() {
        let mut t = track(&[Some(("a", 0.1)), Some(("b", 0.2)), Some(("a", 0.15)), None, Some(("a", 0.3))]);
        assert_eq!(decide(&mut t, &EngineConfig::default()), Some(Event::Unknown { track_id: 1 }));
    }

    #[test]
    fn disagreement_is_uncertain_then_upgradable() {
        let cfg = EngineConfig::default();
        let mut t = track(&[Some(("a", 0.40)), Some(("b", 0.45)), Some(("a", 0.30)), Some(("b", 0.38)), Some(("c", 0.2))]);
        match decide(&mut t, &cfg) {
            Some(Event::Uncertain { candidates, .. }) => {
                assert_eq!(candidates[0].member_id, "b");
                assert_eq!(candidates.len(), 3);
            }
            other => panic!("expected uncertain, got {other:?}"),
        }
        // later frames agree on "a" -> upgrade to recognized
        for _ in 0..3 {
            t.votes.pop_front();
            t.votes.push_back(Some(Candidate { member_id: "a".into(), score: 0.6 }));
        }
        assert!(matches!(decide(&mut t, &cfg), Some(Event::Recognized { ref member_id, .. }) if member_id == "a"));
    }
}
