//! App settings (SPEC §3 `settings`), stored as key/value with typed defaults.

use axum::{extract::State, Json};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db;
use crate::error::{ApiError, ApiResult};
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub gym_name: String,
    /// "male" | "female"
    pub voice: String,
    pub exit_min_minutes: u32,
    pub second_visit_hours: u32,
    pub auto_exit_hours: u32,
    pub wrong_shift_alarm: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gym_name: String::new(),
            voice: "male".into(),
            exit_min_minutes: 5,
            second_visit_hours: 3,
            auto_exit_hours: 4,
            wrong_shift_alarm: true,
        }
    }
}

const KEY: &str = "app_settings";

impl Settings {
    pub fn load(conn: &Connection) -> anyhow::Result<Self> {
        Ok(match db::get_setting(conn, KEY)? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Self::default(),
        })
    }

    fn validate(&self) -> Result<(), String> {
        if self.voice != "male" && self.voice != "female" {
            return Err("صدا باید مرد یا زن باشد".into());
        }
        if !(1..=60).contains(&self.exit_min_minutes) {
            return Err("حداقل فاصله‌ی ورود تا خروج باید بین ۱ تا ۶۰ دقیقه باشد".into());
        }
        if !(1..=12).contains(&self.second_visit_hours) {
            return Err("فاصله‌ی ورود دوم باید بین ۱ تا ۱۲ ساعت باشد".into());
        }
        if !(1..=24).contains(&self.auto_exit_hours) {
            return Err("خروج خودکار باید بین ۱ تا ۲۴ ساعت باشد".into());
        }
        if self.gym_name.chars().count() > 80 {
            return Err("نام باشگاه طولانی است".into());
        }
        Ok(())
    }
}

pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Settings>> {
    Ok(Json(Settings::load(&s.db.conn())?))
}

pub async fn put(State(s): State<AppState>, Json(new): Json<Settings>) -> ApiResult<Json<Settings>> {
    new.validate().map_err(ApiError::bad_request)?;
    db::set_setting(&s.db.conn(), KEY, &serde_json::to_string(&new).map_err(ApiError::internal)?)?;
    Ok(Json(new))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_then_roundtrip() {
        let db = crate::db::Db::in_memory().unwrap();
        let c = db.conn();
        assert_eq!(Settings::load(&c).unwrap(), Settings::default());
        let s = Settings { voice: "female".into(), gym_name: "باشگاه".into(), ..Settings::default() };
        db::set_setting(&c, KEY, &serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(Settings::load(&c).unwrap(), s);
    }

    #[test]
    fn validation() {
        assert!(Settings::default().validate().is_ok());
        assert!(Settings { voice: "robot".into(), ..Settings::default() }.validate().is_err());
        assert!(Settings { exit_min_minutes: 0, ..Settings::default() }.validate().is_err());
    }
}
