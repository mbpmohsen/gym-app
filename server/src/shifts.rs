//! Weekly shifts (SPEC §3 `shifts`). A shift is one weekday, a time range inside
//! that day (no shift crosses midnight) and a gender. Shifts on the same day may
//! not overlap: during an overlap it would be unclear whose shift it is.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveTime;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::rules::weekday;
use crate::visits;
use crate::AppState;

#[derive(Debug, Serialize)]
pub struct ShiftRow {
    id: i64,
    weekday: u32,
    start_time: String,
    end_time: String,
    gender: String,
}

const DAYS: [&str; 7] = ["شنبه", "یکشنبه", "دوشنبه", "سه‌شنبه", "چهارشنبه", "پنجشنبه", "جمعه"];

fn time(s: &str) -> Option<NaiveTime> {
    // "16", "16:30", "۱۶:۳۰"
    let s = crate::validate::ascii_digits(s.trim());
    let s = if s.contains(':') { s } else { format!("{s}:00") };
    NaiveTime::parse_from_str(&s, "%H:%M").ok()
}

fn list_rows(c: &Connection) -> ApiResult<Vec<ShiftRow>> {
    let mut stmt = c.prepare("SELECT id, weekday, start_time, end_time, gender FROM shifts ORDER BY weekday, start_time")?;
    let rows = stmt.query_map([], |r| Ok(ShiftRow { id: r.get(0)?, weekday: r.get(1)?, start_time: r.get(2)?, end_time: r.get(3)?, gender: r.get(4)? }))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub async fn list(State(s): State<AppState>) -> ApiResult<Json<Vec<ShiftRow>>> {
    Ok(Json(list_rows(&s.db.conn())?))
}

#[derive(Deserialize)]
pub struct ShiftInput {
    /// one or more weekdays (0 = Saturday); a preset sends several
    weekdays: Vec<u32>,
    start_time: String,
    end_time: String,
    gender: String,
}

struct Clean {
    weekdays: Vec<u32>,
    start: String,
    end: String,
    gender: String,
}

fn clean(i: ShiftInput) -> ApiResult<Clean> {
    let mut weekdays = i.weekdays;
    weekdays.sort_unstable();
    weekdays.dedup();
    if weekdays.is_empty() || weekdays.iter().any(|d| *d > 6) {
        return Err(ApiError::bad_request("روزهای هفته را انتخاب کنید"));
    }
    let (Some(start), Some(end)) = (time(&i.start_time), time(&i.end_time)) else {
        return Err(ApiError::bad_request("ساعت را به شکل ۱۶:۳۰ وارد کنید"));
    };
    if start >= end {
        return Err(ApiError::bad_request("ساعت پایان باید بعد از ساعت شروع باشد"));
    }
    if !matches!(i.gender.as_str(), "male" | "female") {
        return Err(ApiError::bad_request("سانس آقایان یا خانم‌ها را انتخاب کنید"));
    }
    Ok(Clean { weekdays, start: start.format("%H:%M").to_string(), end: end.format("%H:%M").to_string(), gender: i.gender })
}

/// Rejects an overlap with another shift on the same day (`except` = the shift being edited).
fn check_overlap(c: &Connection, day: u32, start: &str, end: &str, except: Option<i64>) -> ApiResult<()> {
    let clash: Option<(String, String, String)> = c
        .query_row(
            "SELECT start_time, end_time, gender FROM shifts
             WHERE weekday = ?1 AND start_time < ?3 AND ?2 < end_time AND (?4 IS NULL OR id != ?4) LIMIT 1",
            params![day, start, end, except],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match clash {
        Some((a, b, g)) => Err(ApiError::conflict(format!(
            "{}: این بازه با سانس {} از {} تا {} هم‌پوشانی دارد",
            DAYS[day as usize],
            if g == "male" { "آقایان" } else { "خانم‌ها" },
            crate::validate::fa_digits(&a),
            crate::validate::fa_digits(&b)
        ))),
        None => Ok(()),
    }
}

/// Creates one row per weekday, all or nothing.
pub async fn create(State(s): State<AppState>, Json(i): Json<ShiftInput>) -> ApiResult<Json<Vec<ShiftRow>>> {
    let i = clean(i)?;
    let mut c = s.db.conn();
    let tx = c.transaction()?;
    for day in &i.weekdays {
        check_overlap(&tx, *day, &i.start, &i.end, None)?;
        tx.execute("INSERT INTO shifts (weekday, start_time, end_time, gender) VALUES (?1, ?2, ?3, ?4)", params![day, i.start, i.end, i.gender])?;
    }
    tx.commit()?;
    Ok(Json(list_rows(&c)?))
}

/// Edits one shift (weekdays must hold exactly its day).
pub async fn update(State(s): State<AppState>, Path(id): Path<i64>, Json(i): Json<ShiftInput>) -> ApiResult<Json<Vec<ShiftRow>>> {
    let i = clean(i)?;
    if i.weekdays.len() != 1 {
        return Err(ApiError::bad_request("یک روز را انتخاب کنید"));
    }
    let c = s.db.conn();
    check_overlap(&c, i.weekdays[0], &i.start, &i.end, Some(id))?;
    let n = c.execute(
        "UPDATE shifts SET weekday = ?1, start_time = ?2, end_time = ?3, gender = ?4 WHERE id = ?5",
        params![i.weekdays[0], i.start, i.end, i.gender, id],
    )?;
    if n == 0 {
        return Err(ApiError::not_found("سانس پیدا نشد"));
    }
    Ok(Json(list_rows(&c)?))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    s.db.conn().execute("DELETE FROM shifts WHERE id = ?1", [id])?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct Current {
    /// false = no shifts defined: the gym doesn't use shifts
    uses_shifts: bool,
    current: Option<ShiftRow>,
    next: Option<ShiftRow>,
}

/// The shift running now and the next one today, for the reception header.
pub async fn current(State(s): State<AppState>) -> ApiResult<Json<Current>> {
    let c = s.db.conn();
    let all = list_rows(&c)?;
    let now = visits::now();
    let (day, t) = (weekday(now.date()), now.time().format("%H:%M").to_string());
    let uses_shifts = !all.is_empty();
    let mut today: Vec<ShiftRow> = all.into_iter().filter(|r| r.weekday == day).collect();
    let cur = today.iter().position(|r| r.start_time <= t && t < r.end_time).map(|i| today.remove(i));
    let next = today.into_iter().find(|r| r.start_time > t);
    Ok(Json(Current { uses_shifts, current: cur, next }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn overlap_rules() {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO shifts (weekday, start_time, end_time, gender) VALUES (0, '16:00', '20:00', 'male')", []).unwrap();
        assert!(check_overlap(&c, 0, "18:00", "22:00", None).is_err());
        assert!(check_overlap(&c, 0, "08:00", "16:30", None).is_err());
        // touching ends is fine
        assert!(check_overlap(&c, 0, "20:00", "22:00", None).is_ok());
        assert!(check_overlap(&c, 0, "08:00", "16:00", None).is_ok());
        // other day
        assert!(check_overlap(&c, 1, "16:00", "20:00", None).is_ok());
        // editing itself
        assert!(check_overlap(&c, 0, "15:00", "21:00", Some(1)).is_ok());
    }

    #[test]
    fn cleaning() {
        let ok = |a: &str, b: &str| clean(ShiftInput { weekdays: vec![0, 1, 1], start_time: a.into(), end_time: b.into(), gender: "male".into() });
        let c = ok("۸:۰۰", "12:30").unwrap();
        assert_eq!((c.weekdays, c.start.as_str(), c.end.as_str()), (vec![0, 1], "08:00", "12:30"));
        assert!(ok("12:00", "08:00").is_err());
        assert!(ok("22:00", "01:00").is_err()); // no shift crosses midnight
        assert!(ok("25:00", "26:00").is_err());
        assert_eq!(ok("16", "۲۰").unwrap().end, "20:00");
    }
}
