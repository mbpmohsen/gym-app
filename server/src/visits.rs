//! Entries and exits (SPEC §4.3–§4.7): database side of `rules.rs`, plus the
//! reception page API.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use chrono::{Days, Local, NaiveDateTime, NaiveTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::Status;
use crate::error::{ApiError, ApiResult};
use crate::members::{self, CurrentSub};
use crate::rules::{self, CameraAction, EntryInput, EntrySub, Flag, Shift, Sound};
use crate::settings::Settings;
use crate::AppState;

pub const FMT: &str = "%Y-%m-%d %H:%M:%S";

pub fn now() -> NaiveDateTime {
    Local::now().naive_local().with_nanosecond(0).unwrap()
}

pub fn fmt(t: NaiveDateTime) -> String {
    t.format(FMT).to_string()
}

fn parse(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, FMT).ok()
}

/// Pushed to every open browser tab (see live.rs). The tab refreshes its lists and plays `sound`.
#[derive(Debug, Clone, Serialize)]
pub struct LiveEvent {
    /// entry | exit | face | refresh
    pub kind: &'static str,
    pub member_id: Option<i64>,
    pub name: Option<String>,
    pub status: Option<Status>,
    pub flags: Vec<Flag>,
    pub sound: Option<Sound>,
}

impl LiveEvent {
    pub fn refresh() -> Self {
        Self { kind: "refresh", member_id: None, name: None, status: None, flags: vec![], sound: None }
    }
}

struct Member {
    name: String,
    gender: String,
}

fn active_member(c: &Connection, id: i64) -> ApiResult<Member> {
    let (name, gender, archived): (String, String, bool) = c
        .query_row("SELECT full_name, gender, archived FROM members WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?
        .ok_or_else(|| ApiError::not_found("عضو پیدا نشد"))?;
    if archived {
        return Err(ApiError::bad_request("این عضو بایگانی شده است"));
    }
    Ok(Member { name, gender })
}

fn open_visit(c: &Connection, member_id: i64) -> ApiResult<Option<(i64, NaiveDateTime)>> {
    let v: Option<(i64, String)> = c
        .query_row("SELECT id, entered_at FROM visits WHERE member_id = ?1 AND exited_at IS NULL", [member_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    Ok(v.and_then(|(id, t)| Some((id, parse(&t)?))))
}

fn load_shifts(c: &Connection) -> ApiResult<Vec<Shift>> {
    let mut stmt = c.prepare("SELECT weekday, start_time, end_time, gender FROM shifts")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (weekday, a, b, gender) = row?;
        let t = |s: &str| NaiveTime::parse_from_str(s, "%H:%M").or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S")).ok();
        if let (Some(start), Some(end)) = (t(&a), t(&b)) {
            out.push(Shift { weekday, start, end, gender });
        }
    }
    Ok(out)
}

/// Records an entry (§4.3–§4.5): deducts the session, sets flags, picks the sound.
/// `face_event`: the unknown/uncertain card the receptionist resolved with this entry.
pub fn enter(c: &mut Connection, member_id: i64, at: NaiveDateTime, source: &str, snapshot: Option<&str>, face_event: Option<i64>) -> ApiResult<LiveEvent> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let m = active_member(&tx, member_id)?;
    if open_visit(&tx, member_id)?.is_some() {
        return Err(ApiError::conflict(format!("{} الان داخل باشگاه است", m.name)));
    }
    let settings = Settings::load(&tx)?;
    let today = at.date();
    let subs: Vec<EntrySub> = members::load_subs(&tx, Some(member_id), today)?
        .remove(&member_id)
        .unwrap_or_default()
        .iter()
        .map(|s| EntrySub { id: s.id, alternate: s.frequency == "alternate", rule: s.rule() })
        .collect();

    let mut stmt = tx.prepare("SELECT entered_at, subscription_id FROM visits WHERE member_id = ?1 AND date(entered_at) = ?2")?;
    let todays: Vec<(String, Option<i64>)> = stmt.query_map(params![member_id, today.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    drop(stmt);
    let earlier_today: Vec<NaiveDateTime> = todays.iter().filter_map(|(t, _)| parse(t)).collect();
    let deducted_today = todays.iter().any(|(_, s)| s.is_some());
    let yesterday = (today - Days::new(1)).to_string();
    let visited_yesterday: bool =
        tx.query_row("SELECT EXISTS(SELECT 1 FROM visits WHERE member_id = ?1 AND date(entered_at) = ?2)", params![member_id, yesterday], |r| r.get(0))?;
    let shifts = load_shifts(&tx)?;

    let plan = rules::plan_entry(&EntryInput {
        now: at,
        gender: &m.gender,
        subs: &subs,
        earlier_today: &earlier_today,
        deducted_today,
        visited_yesterday,
        shifts: &shifts,
        settings: &settings,
    });

    tx.execute(
        "INSERT INTO visits (member_id, entered_at, entry_source, subscription_id, status, flags, snapshot) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![member_id, fmt(at), source, plan.deduct, status_str(plan.status), serde_json::to_string(&plan.flags).unwrap(), snapshot],
    )?;
    let visit_id = tx.last_insert_rowid();
    if let Some(sub) = plan.deduct {
        tx.execute("UPDATE subscriptions SET sessions_used = sessions_used + 1 WHERE id = ?1", [sub])?;
    }
    if let Some(fe) = face_event {
        tx.execute("UPDATE face_events SET resolved_visit_id = ?1 WHERE id = ?2", params![visit_id, fe])?;
    }
    tx.commit()?;
    tracing::info!("entry: member {member_id} ({source}) status {:?} flags {:?} sub {:?}", plan.status, plan.flags, plan.deduct);
    Ok(LiveEvent { kind: "entry", member_id: Some(member_id), name: Some(m.name), status: Some(plan.status), flags: plan.flags, sound: Some(plan.sound) })
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::None => "none",
        Status::Expired => "expired",
        Status::Debt => "debt",
        Status::Ok => "ok",
    }
}

fn close(c: &Connection, visit_id: i64, at: NaiveDateTime, source: &str) -> ApiResult<()> {
    let n = c.execute("UPDATE visits SET exited_at = ?1, exit_source = ?2 WHERE id = ?3 AND exited_at IS NULL", params![fmt(at), source, visit_id])?;
    if n == 0 {
        return Err(ApiError::conflict("این ورود قبلاً بسته شده است"));
    }
    Ok(())
}

/// A recognition from the camera (§4.3). None = nothing to do (shoes on/off, unknown id).
pub fn camera(c: &mut Connection, member_id: i64, at: NaiveDateTime, snapshot: Option<&str>) -> ApiResult<Option<LiveEvent>> {
    let m = match active_member(c, member_id) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("recognized member {member_id} ignored: {}", e.message());
            return Ok(None);
        }
    };
    let open = open_visit(c, member_id)?;
    let last_exit: Option<String> =
        c.query_row("SELECT MAX(exited_at) FROM visits WHERE member_id = ?1 AND exit_source IN ('camera', 'manual')", [member_id], |r| r.get(0))?;
    let settings = Settings::load(c)?;
    match rules::camera_action(open, last_exit.as_deref().and_then(parse), at, &settings) {
        CameraAction::Ignore => Ok(None),
        CameraAction::Enter => enter(c, member_id, at, "camera", snapshot, None).map(Some),
        CameraAction::Exit(id) => {
            close(c, id, at, "camera")?;
            tracing::info!("exit: member {member_id} (camera)");
            Ok(Some(LiveEvent { kind: "exit", member_id: Some(member_id), name: Some(m.name), status: None, flags: vec![], sound: Some(Sound::Goodbye) }))
        }
    }
}

/// Closes visits open longer than `auto_exit_hours` (§4.3). Returns how many.
pub fn auto_exit(c: &Connection, at: NaiveDateTime) -> ApiResult<usize> {
    let settings = Settings::load(c)?;
    let mut stmt = c.prepare("SELECT id, entered_at FROM visits WHERE exited_at IS NULL")?;
    let open: Vec<(i64, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    let mut n = 0;
    for (id, entered) in open {
        let Some(entered) = parse(&entered) else { continue };
        let out = rules::auto_exit_at(entered, &settings);
        if out <= at {
            close(c, id, out, "auto")?;
            n += 1;
        }
    }
    if n > 0 {
        tracing::info!("auto-exit: closed {n} visit(s)");
    }
    Ok(n)
}

/// Undo a wrong entry from today (e.g. the wrong candidate was picked): the
/// session goes back to the subscription.
pub fn cancel(c: &mut Connection, visit_id: i64) -> ApiResult<()> {
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (entered, sub): (String, Option<i64>) = tx
        .query_row("SELECT entered_at, subscription_id FROM visits WHERE id = ?1", [visit_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| ApiError::not_found("ورود پیدا نشد"))?;
    if parse(&entered).map(|t| t.date()) != Some(now().date()) {
        return Err(ApiError::bad_request("فقط ورودهای امروز قابل حذف هستند"));
    }
    if let Some(sub) = sub {
        tx.execute("UPDATE subscriptions SET sessions_used = MAX(sessions_used - 1, 0) WHERE id = ?1", [sub])?;
    }
    tx.execute("UPDATE face_events SET resolved_visit_id = NULL WHERE resolved_visit_id = ?1", [visit_id])?;
    tx.execute("DELETE FROM visits WHERE id = ?1", [visit_id])?;
    tx.commit()?;
    tracing::info!("visit {visit_id} cancelled");
    Ok(())
}

// ---------- reading ----------

#[derive(Serialize)]
pub struct VisitRow {
    id: i64,
    member_id: i64,
    full_name: String,
    gender: String,
    entered_at: String,
    exited_at: Option<String>,
    entry_source: String,
    exit_source: Option<String>,
    status: String,
    flags: Value,
    snapshot: Option<String>,
    current: Option<CurrentSub>,
}

#[derive(Serialize)]
pub struct Candidate {
    member_id: i64,
    full_name: String,
    score: f64,
}

#[derive(Serialize)]
pub struct FaceEventRow {
    id: i64,
    r#type: String,
    candidates: Vec<Candidate>,
    snapshot: Option<String>,
    at: String,
}

#[derive(Serialize)]
pub struct Reception {
    inside: i64,
    entries_today: i64,
    visits: Vec<VisitRow>,
    face_events: Vec<FaceEventRow>,
}

fn visit_rows(c: &Connection, filter: &str, arg: &dyn rusqlite::ToSql) -> ApiResult<Vec<VisitRow>> {
    let today = now().date();
    let subs = members::load_subs(c, None, today)?;
    let sql = format!(
        "SELECT v.id, v.member_id, m.full_name, m.gender, v.entered_at, v.exited_at, v.entry_source, v.exit_source, v.status, v.flags, v.snapshot
         FROM visits v JOIN members m ON m.id = v.member_id WHERE {filter}
         ORDER BY MAX(v.entered_at, COALESCE(v.exited_at, '')) DESC, v.id DESC LIMIT 300"
    );
    let mut stmt = c.prepare(&sql)?;
    let rows = stmt.query_map([arg], |r| {
        let member_id: i64 = r.get(1)?;
        Ok(VisitRow {
            id: r.get(0)?,
            member_id,
            full_name: r.get(2)?,
            gender: r.get(3)?,
            entered_at: r.get(4)?,
            exited_at: r.get(5)?,
            entry_source: r.get(6)?,
            exit_source: r.get(7)?,
            status: r.get(8)?,
            flags: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or(Value::Array(vec![])),
            snapshot: r.get(10)?,
            current: subs.get(&member_id).and_then(|s| members::summarize(s, today).2),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub async fn reception(State(s): State<AppState>) -> ApiResult<Json<Reception>> {
    let c = s.db.conn();
    let today = now().date().to_string();
    let visits = visit_rows(&c, "date(v.entered_at) = ?1 OR v.exited_at IS NULL", &today)?;
    let inside = c.query_row("SELECT COUNT(*) FROM visits WHERE exited_at IS NULL", [], |r| r.get(0))?;
    let entries_today = c.query_row("SELECT COUNT(DISTINCT member_id) FROM visits WHERE date(entered_at) = ?1", [&today], |r| r.get(0))?;

    let mut stmt = c.prepare(
        "SELECT id, type, candidates, snapshot, at FROM face_events
         WHERE date(at) = ?1 AND dismissed = 0 AND resolved_visit_id IS NULL ORDER BY id DESC LIMIT 30",
    )?;
    let raw: Vec<(i64, String, String, Option<String>, String)> =
        stmt.query_map([&today], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    let mut face_events = Vec::new();
    for (id, r#type, cands, snapshot, at) in raw {
        let mut candidates = Vec::new();
        for cand in serde_json::from_str::<Vec<Value>>(&cands).unwrap_or_default() {
            let Some(mid) = cand["member_id"].as_str().and_then(|m| m.parse::<i64>().ok()) else { continue };
            let name: Option<String> = c.query_row("SELECT full_name FROM members WHERE id = ?1 AND archived = 0", [mid], |r| r.get(0)).optional()?;
            if let Some(full_name) = name {
                candidates.push(Candidate { member_id: mid, full_name, score: cand["score"].as_f64().unwrap_or(0.0) });
            }
        }
        face_events.push(FaceEventRow { id, r#type, candidates, snapshot, at });
    }
    Ok(Json(Reception { inside, entries_today, visits, face_events }))
}

pub async fn member_visits(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Vec<VisitRow>>> {
    Ok(Json(visit_rows(&s.db.conn(), "v.member_id = ?1", &id)?))
}

#[derive(Deserialize, Default)]
pub struct EnterBody {
    face_event_id: Option<i64>,
}

/// Manual entry (§4.7), or picking a member for an unknown/uncertain card (§4.6).
pub async fn manual_enter(State(s): State<AppState>, Path(id): Path<i64>, body: axum::body::Bytes) -> ApiResult<Json<LiveEvent>> {
    let b: EnterBody = if body.iter().all(u8::is_ascii_whitespace) {
        EnterBody::default()
    } else {
        serde_json::from_slice(&body).map_err(|_| ApiError::bad_request("درخواست نامعتبر است"))?
    };
    let ev = {
        let mut c = s.db.conn();
        let (source, snapshot) = match b.face_event_id {
            Some(fe) => {
                let snap: Option<String> = c
                    .query_row("SELECT snapshot FROM face_events WHERE id = ?1 AND resolved_visit_id IS NULL", [fe], |r| r.get(0))
                    .optional()?
                    .ok_or_else(|| ApiError::conflict("این مورد قبلاً رسیدگی شده است"))?;
                ("picked", snap)
            }
            None => ("manual", None),
        };
        enter(&mut c, id, now(), source, snapshot.as_deref(), b.face_event_id)?
    };
    s.live.send(&ev);
    Ok(Json(ev))
}

pub async fn manual_exit(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    {
        let c = s.db.conn();
        let name = active_member(&c, id).map(|m| m.name).unwrap_or_default();
        let (visit, _) = open_visit(&c, id)?.ok_or_else(|| ApiError::conflict(format!("{name} داخل باشگاه نیست")))?;
        close(&c, visit, now(), "manual")?;
    }
    s.live.send(&LiveEvent::refresh());
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel_visit(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    cancel(&mut s.db.conn(), id)?;
    s.live.send(&LiveEvent::refresh());
    Ok(StatusCode::NO_CONTENT)
}

pub async fn dismiss_face_event(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    s.db.conn().execute("UPDATE face_events SET dismissed = 1 WHERE id = ?1", [id])?;
    s.live.send(&LiveEvent::refresh());
    Ok(StatusCode::NO_CONTENT)
}

/// Snapshot photos copied from face-service (which keeps only its last 2000).
pub async fn snapshot(State(s): State<AppState>, Path(file): Path<String>) -> ApiResult<impl IntoResponse> {
    if !file.ends_with(".jpg") || !file[..file.len() - 4].chars().all(|c| c.is_ascii_digit()) {
        return Err(ApiError::bad_request("نام فایل نامعتبر است"));
    }
    let bytes = tokio::fs::read(s.snapshots_dir.join(&file)).await.map_err(|_| ApiError::not_found("عکس پیدا نشد"))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=31536000, immutable")], bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    fn setup() -> Db {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO members (id, full_name, phone, birth_date, gender) VALUES (1, 'علی', '09120000000', '1990-01-01', 'male')", []).unwrap();
        c.execute(
            "INSERT INTO subscriptions (id, member_id, plan_name, kind, sessions, frequency, shower, locker, price, start_date)
             VALUES (10, 1, '12 جلسه', 'sessions', 12, 'six_days', 0, 0, 0, '2026-01-01')",
            [],
        )
        .unwrap();
        drop(c);
        db
    }

    fn used(db: &Db) -> i64 {
        db.conn().query_row("SELECT sessions_used FROM subscriptions WHERE id = 10", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn full_day_through_the_camera() {
        let db = setup();
        let mut c = db.conn();
        // in
        let e = camera(&mut c, 1, at("2026-10-10 09:00"), Some("5.jpg")).unwrap().unwrap();
        assert_eq!((e.kind, e.sound, e.status), ("entry", Some(Sound::Welcome), Some(Status::Ok)));
        // shoes off: nothing
        assert!(camera(&mut c, 1, at("2026-10-10 09:03"), None).unwrap().is_none());
        // out
        let e = camera(&mut c, 1, at("2026-10-10 10:30"), None).unwrap().unwrap();
        assert_eq!((e.kind, e.sound), ("exit", Some(Sound::Goodbye)));
        // shoes on: nothing
        assert!(camera(&mut c, 1, at("2026-10-10 10:33"), None).unwrap().is_none());
        // evening: second visit, flagged, nothing deducted
        let e = camera(&mut c, 1, at("2026-10-10 18:00"), None).unwrap().unwrap();
        assert_eq!(e.flags, vec![Flag::SecondVisitToday]);
        drop(c);
        assert_eq!(used(&db), 1);
    }

    #[test]
    fn manual_entry_twice_conflicts_and_cancel_restores_session() {
        let db = setup();
        let mut c = db.conn();
        let now = now();
        enter(&mut c, 1, now, "manual", None, None).unwrap();
        assert_eq!(enter(&mut c, 1, now, "manual", None, None).unwrap_err().status(), StatusCode::CONFLICT);
        let id: i64 = c.query_row("SELECT id FROM visits", [], |r| r.get(0)).unwrap();
        cancel(&mut c, id).unwrap();
        drop(c);
        assert_eq!(used(&db), 0);
    }

    #[test]
    fn auto_exit_after_hours() {
        let db = setup();
        let mut c = db.conn();
        camera(&mut c, 1, at("2026-10-10 09:00"), None).unwrap();
        assert_eq!(auto_exit(&c, at("2026-10-10 12:59")).unwrap(), 0);
        assert_eq!(auto_exit(&c, at("2026-10-10 13:00")).unwrap(), 1);
        let (out, src): (String, String) = c.query_row("SELECT exited_at, exit_source FROM visits", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!((out.as_str(), src.as_str()), ("2026-10-10 13:00:00", "auto"));
        // auto-exit doesn't block the next real entry
        assert_eq!(camera(&mut c, 1, at("2026-10-10 13:02"), None).unwrap().unwrap().kind, "entry");
    }

    #[test]
    fn archived_or_unknown_member_is_ignored() {
        let db = setup();
        let mut c = db.conn();
        assert!(camera(&mut c, 99, at("2026-10-10 09:00"), None).unwrap().is_none());
        c.execute("UPDATE members SET archived = 1", []).unwrap();
        assert!(camera(&mut c, 1, at("2026-10-10 09:00"), None).unwrap().is_none());
    }
}
