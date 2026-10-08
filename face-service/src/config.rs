//! Service configuration (face-service.toml). Relative paths are resolved against
//! the config file's folder. Created with defaults (and a random token) if missing.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Always loopback: the service must not be reachable from the network.
    pub bind: String,
    /// Shared secret. Clients send `Authorization: Bearer <token>` or `?token=`.
    /// Empty = no auth (development only).
    pub token: String,
    /// "auto" (first non-virtual camera) or a camera index like "1".
    pub camera: String,
    pub models_dir: String,
    pub data_dir: String,
    /// onnxruntime library; empty = next to the executable.
    pub onnxruntime: String,
    pub recognition: Recognition,
}

/// The part that can be changed at runtime via PUT /v1/config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Recognition {
    pub threshold: f32,
    pub cooldown_secs: f32,
    /// Upper bound on frames run through the models per second (CPU budget).
    pub max_fps: f32,
}

impl Default for Recognition {
    fn default() -> Self {
        Self { threshold: crate::recognizer::DEFAULT_COSINE_THRESHOLD, cooldown_secs: 600.0, max_fps: 8.0 }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:7480".into(),
            token: String::new(),
            camera: "auto".into(),
            models_dir: "models".into(),
            data_dir: "data".into(),
            onnxruntime: String::new(),
            recognition: Recognition::default(),
        }
    }
}

impl Recognition {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!((0.0..=1.0).contains(&self.threshold), "threshold must be within 0..1");
        anyhow::ensure!(self.cooldown_secs >= 0.0, "cooldown_secs must be >= 0");
        anyhow::ensure!((1.0..=30.0).contains(&self.max_fps), "max_fps must be within 1..30");
        Ok(())
    }
}

pub struct Loaded {
    pub config: Config,
    pub path: PathBuf,
}

impl Loaded {
    pub fn load_or_create(path: &Path, default_onnxruntime: &str) -> Result<Self> {
        let path = std::path::absolute(path)?;
        let config = if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))?
        } else {
            let c = Config { token: random_token(), onnxruntime: default_onnxruntime.into(), ..Config::default() };
            std::fs::write(&path, toml::to_string_pretty(&c)?)?;
            println!("created default config {}", path.display());
            c
        };
        config.recognition.validate()?;
        Ok(Self { config, path })
    }

    pub fn save(&self) -> Result<()> {
        std::fs::write(&self.path, toml::to_string_pretty(&self.config)?)?;
        Ok(())
    }

    /// Resolves a path from the config relative to the config file's folder.
    pub fn resolve(&self, p: &str) -> PathBuf {
        let p = Path::new(p);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.path.parent().unwrap().join(p)
        }
    }
}

/// 128-bit random hex token from the OS RNG (via std's randomly seeded hasher).
fn random_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    (0..2)
        .map(|_| {
            let mut h = RandomState::new().build_hasher();
            h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
            format!("{:016x}", h.finish())
        })
        .collect()
}
