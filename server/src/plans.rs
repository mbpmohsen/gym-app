//! Plans (tariffs), SPEC §3 `plans`. Never deleted (subscriptions refer to them): deactivated.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::domain::Kind;
use crate::error::{ApiError, ApiResult};
use crate::validate;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub sessions: Option<i64>,
    pub duration_days: Option<i64>,
    pub frequency: String,
    pub shower: bool,
    pub locker: bool,
    pub price: i64,
    pub active: bool,
}

const COLS: &str = "id, name, kind, sessions, duration_days, frequency, shower, locker, price, active";

fn from_row(r: &Row) -> rusqlite::Result<Plan> {
    Ok(Plan {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: Kind::parse(&r.get::<_, String>(2)?).unwrap_or(Kind::Sessions),
        sessions: r.get(3)?,
        duration_days: r.get(4)?,
        frequency: r.get(5)?,
        shower: r.get(6)?,
        locker: r.get(7)?,
        price: r.get(8)?,
        active: r.get(9)?,
    })
}

pub fn get_plan(conn: &Connection, id: i64) -> ApiResult<Plan> {
    conn.query_row(&format!("SELECT {COLS} FROM plans WHERE id = ?1"), [id], from_row)
        .optional()?
        .ok_or_else(|| ApiError::not_found("تعرفه پیدا نشد"))
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    all: bool,
}

pub async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<Vec<Plan>>> {
    let c = s.db.conn();
    let filter = if q.all { "" } else { "WHERE active = 1" };
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM plans {filter} ORDER BY active DESC, kind, sessions, duration_days, price"))?;
    let rows = stmt.query_map([], from_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct PlanInput {
    name: String,
    kind: Kind,
    sessions: Option<i64>,
    duration_days: Option<i64>,
    frequency: String,
    shower: bool,
    locker: bool,
    price: i64,
    #[serde(default = "yes")]
    active: bool,
}

fn yes() -> bool {
    true
}

impl PlanInput {
    /// Normalizes (drops fields the kind doesn't use) and validates.
    fn clean(mut self) -> ApiResult<Self> {
        self.name = validate::name(&self.name, 80).ok_or_else(|| ApiError::bad_request("نام تعرفه را وارد کنید"))?;
        if !self.kind.has_sessions() {
            self.sessions = None;
        }
        if !self.kind.has_duration() {
            self.duration_days = None;
        }
        if self.kind.has_sessions() && !self.sessions.is_some_and(|n| (1..=500).contains(&n)) {
            return Err(ApiError::bad_request("تعداد جلسات باید بین ۱ تا ۵۰۰ باشد"));
        }
        if self.kind.has_duration() && !self.duration_days.is_some_and(|n| (1..=730).contains(&n)) {
            return Err(ApiError::bad_request("مدت باید بین ۱ تا ۷۳۰ روز باشد"));
        }
        if !matches!(self.frequency.as_str(), "six_days" | "alternate") {
            return Err(ApiError::bad_request("تواتر نامعتبر است"));
        }
        if !(0..=10_000_000_000).contains(&self.price) {
            return Err(ApiError::bad_request("مبلغ نامعتبر است"));
        }
        Ok(self)
    }

    fn kind_str(&self) -> &'static str {
        match self.kind {
            Kind::Sessions => "sessions",
            Kind::Duration => "duration",
            Kind::Combined => "combined",
        }
    }
}

pub async fn create(State(s): State<AppState>, Json(p): Json<PlanInput>) -> ApiResult<Json<Plan>> {
    let p = p.clean()?;
    let c = s.db.conn();
    c.execute(
        "INSERT INTO plans (name, kind, sessions, duration_days, frequency, shower, locker, price, active) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![p.name, p.kind_str(), p.sessions, p.duration_days, p.frequency, p.shower, p.locker, p.price, p.active],
    )?;
    Ok(Json(get_plan(&c, c.last_insert_rowid())?))
}

/// Editing a plan never changes subscriptions already sold (they hold a copy).
pub async fn update(State(s): State<AppState>, Path(id): Path<i64>, Json(p): Json<PlanInput>) -> ApiResult<Json<Plan>> {
    let p = p.clean()?;
    let c = s.db.conn();
    get_plan(&c, id)?;
    c.execute(
        "UPDATE plans SET name=?1, kind=?2, sessions=?3, duration_days=?4, frequency=?5, shower=?6, locker=?7, price=?8, active=?9 WHERE id=?10",
        params![p.name, p.kind_str(), p.sessions, p.duration_days, p.frequency, p.shower, p.locker, p.price, p.active, id],
    )?;
    Ok(Json(get_plan(&c, id)?))
}
