//! Members, their subscriptions and payments (SPEC §3, §4.1, §4.2, §4.8).

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::domain::{self, Kind, Status, Sub};
use crate::error::{ApiError, ApiResult};
use crate::plans;
use crate::validate;
use crate::AppState;

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

// ---------- reading ----------

#[derive(Debug, Clone, Serialize)]
pub struct SubView {
    pub id: i64,
    pub member_id: i64,
    pub plan_id: Option<i64>,
    pub plan_name: String,
    pub kind: Kind,
    pub sessions: Option<i64>,
    pub sessions_used: i64,
    pub sessions_remaining: Option<i64>,
    pub duration_days: Option<i64>,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub days_remaining: Option<i64>,
    pub frequency: String,
    pub shower: bool,
    pub locker: bool,
    pub price: i64,
    pub paid: i64,
    pub debt: i64,
    pub valid: bool,
    /// starts in the future (bought in advance)
    pub upcoming: bool,
    pub created_at: String,
}

impl SubView {
    pub fn rule(&self) -> Sub {
        Sub {
            kind: self.kind,
            sessions: self.sessions,
            sessions_used: self.sessions_used,
            start_date: self.start_date,
            end_date: self.end_date,
            price: self.price,
            paid: self.paid,
        }
    }
}

/// All subscriptions (with payment totals), grouped by member. `member` limits to one.
pub fn load_subs(c: &Connection, member: Option<i64>, today: NaiveDate) -> ApiResult<HashMap<i64, Vec<SubView>>> {
    let mut stmt = c.prepare(
        "SELECT s.id, s.member_id, s.plan_name, s.kind, s.sessions, s.sessions_used, s.duration_days,
                s.start_date, s.end_date, s.frequency, s.shower, s.locker, s.price, s.created_at,
                COALESCE((SELECT SUM(amount) FROM payments p WHERE p.subscription_id = s.id), 0), s.plan_id
         FROM subscriptions s
         WHERE (?1 IS NULL OR s.member_id = ?1)
         ORDER BY s.start_date DESC, s.id DESC",
    )?;
    let rows = stmt.query_map([member], |r| {
        let start: String = r.get(7)?;
        let end: Option<String> = r.get(8)?;
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get(4)?, r.get(5)?, r.get(6)?, start, end, r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?, r.get(14)?, r.get(15)?))
    })?;
    let mut out: HashMap<i64, Vec<SubView>> = HashMap::new();
    for row in rows {
        let (id, member_id, plan_name, kind, sessions, sessions_used, duration_days, start, end, frequency, shower, locker, price, created_at, paid, plan_id): (
            i64, i64, String, String, Option<i64>, i64, Option<i64>, String, Option<String>, String, bool, bool, i64, String, i64, Option<i64>,
        ) = row?;
        let start_date = validate::date(&start).ok_or_else(|| ApiError::internal(format!("bad start_date in subscription {id}")))?;
        let end_date = end.as_deref().and_then(validate::date);
        let mut v = SubView {
            id,
            member_id,
            plan_id,
            plan_name,
            kind: Kind::parse(&kind).unwrap_or(Kind::Sessions),
            sessions,
            sessions_used,
            sessions_remaining: None,
            duration_days,
            start_date,
            end_date,
            days_remaining: None,
            frequency,
            shower,
            locker,
            price,
            paid,
            debt: 0,
            valid: false,
            upcoming: start_date > today,
            created_at,
        };
        let rule = v.rule();
        v.sessions_remaining = rule.sessions_remaining();
        v.days_remaining = rule.days_remaining(today);
        v.debt = rule.debt();
        v.valid = rule.is_valid(today);
        out.entry(member_id).or_default().push(v);
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct MemberRow {
    pub id: i64,
    pub full_name: String,
    pub phone: String,
    pub gender: String,
    pub birth_date: String,
    pub face_enrolled: bool,
    pub archived: bool,
    pub status: Status,
    pub debt: i64,
    /// the valid subscription ending first: what the receptionist cares about
    pub current: Option<CurrentSub>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CurrentSub {
    pub plan_name: String,
    pub sessions_remaining: Option<i64>,
    pub days_remaining: Option<i64>,
}

pub fn summarize(subs: &[SubView], today: NaiveDate) -> (Status, i64, Option<CurrentSub>) {
    let rules: Vec<Sub> = subs.iter().map(SubView::rule).collect();
    let status = domain::status(&rules, today);
    let debt = subs.iter().map(|s| s.debt).sum();
    let current = subs
        .iter()
        .filter(|s| s.valid)
        .min_by_key(|s| (s.end_date.unwrap_or(NaiveDate::MAX), s.sessions_remaining.unwrap_or(i64::MAX)))
        .map(|s| CurrentSub { plan_name: s.plan_name.clone(), sessions_remaining: s.sessions_remaining, days_remaining: s.days_remaining });
    (status, debt, current)
}

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    status: Option<String>,
    gender: Option<String>,
    #[serde(default)]
    archived: bool,
    face: Option<bool>,
}

pub async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Json<Vec<MemberRow>>> {
    let today = today();
    let c = s.db.conn();
    let subs = load_subs(&c, None, today)?;
    let search = q.q.as_deref().map(validate::ascii_digits).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let mut stmt = c.prepare(
        "SELECT id, full_name, phone, gender, birth_date, face_enrolled, archived, created_at FROM members
         WHERE archived = ?1
           AND (?2 IS NULL OR full_name LIKE '%' || ?2 || '%' OR phone LIKE '%' || ?2 || '%')
           AND (?3 IS NULL OR gender = ?3)
           AND (?4 IS NULL OR face_enrolled = ?4)
         ORDER BY id DESC",
    )?;
    let rows = stmt.query_map(params![q.archived, search, q.gender, q.face], |r| {
        Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, full_name, phone, gender, birth_date, face_enrolled, archived, created_at) = row?;
        let (status, debt, current) = summarize(subs.get(&id).map(Vec::as_slice).unwrap_or(&[]), today);
        let wanted = match q.status.as_deref() {
            None | Some("") => true,
            // "end of tuition" for the receptionist = anything that plays that sound
            Some("alert") => status != Status::Ok,
            Some(st) => serde_json::to_value(status).ok().and_then(|v| v.as_str().map(|x| x == st)).unwrap_or(false),
        };
        if wanted {
            out.push(MemberRow { id, full_name, phone, gender, birth_date, face_enrolled, archived, status, debt, current, created_at });
        }
    }
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct PaymentView {
    pub id: i64,
    pub subscription_id: i64,
    pub plan_name: String,
    pub amount: i64,
    pub paid_at: String,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct MemberDetail {
    #[serde(flatten)]
    pub member: MemberRow,
    pub notes: String,
    pub subscriptions: Vec<SubView>,
    pub payments: Vec<PaymentView>,
}

fn member_row(c: &Connection, id: i64) -> ApiResult<(MemberRow, String)> {
    let today = today();
    let r = c
        .query_row(
            "SELECT id, full_name, phone, gender, birth_date, face_enrolled, archived, created_at, notes FROM members WHERE id = ?1",
            [id],
            |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get::<_, String>(8)?)),
        )
        .optional()?
        .ok_or_else(|| ApiError::not_found("عضو پیدا نشد"))?;
    let subs = load_subs(c, Some(id), today)?;
    let (status, debt, current) = summarize(subs.get(&id).map(Vec::as_slice).unwrap_or(&[]), today);
    Ok((
        MemberRow { id: r.0, full_name: r.1, phone: r.2, gender: r.3, birth_date: r.4, face_enrolled: r.5, archived: r.6, status, debt, current, created_at: r.7 },
        r.8,
    ))
}

pub fn detail(c: &Connection, id: i64) -> ApiResult<MemberDetail> {
    let (member, notes) = member_row(c, id)?;
    let subscriptions = load_subs(c, Some(id), today())?.remove(&id).unwrap_or_default();
    let mut stmt = c.prepare(
        "SELECT p.id, p.subscription_id, s.plan_name, p.amount, p.paid_at, p.note FROM payments p
         JOIN subscriptions s ON s.id = p.subscription_id WHERE s.member_id = ?1 ORDER BY p.paid_at DESC, p.id DESC",
    )?;
    let payments = stmt
        .query_map([id], |r| Ok(PaymentView { id: r.get(0)?, subscription_id: r.get(1)?, plan_name: r.get(2)?, amount: r.get(3)?, paid_at: r.get(4)?, note: r.get(5)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MemberDetail { member, notes, subscriptions, payments })
}

pub async fn get(State(s): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<MemberDetail>> {
    Ok(Json(detail(&s.db.conn(), id)?))
}

// ---------- writing ----------

#[derive(Deserialize)]
pub struct MemberInput {
    full_name: String,
    phone: String,
    birth_date: String,
    gender: String,
    #[serde(default)]
    notes: String,
}

struct CleanMember {
    full_name: String,
    phone: String,
    birth_date: NaiveDate,
    gender: String,
    notes: String,
}

impl MemberInput {
    fn clean(self) -> ApiResult<CleanMember> {
        let full_name = validate::name(&self.full_name, 80).ok_or_else(|| ApiError::bad_request("نام کامل را وارد کنید"))?;
        let phone = validate::phone(&self.phone).ok_or_else(|| ApiError::bad_request("شماره موبایل نامعتبر است (مثلاً ۰۹۱۲۱۲۳۴۵۶۷)"))?;
        let birth_date = validate::date(&self.birth_date).ok_or_else(|| ApiError::bad_request("تاریخ تولد نامعتبر است"))?;
        let t = today();
        if birth_date >= t || birth_date < NaiveDate::from_ymd_opt(1900, 1, 1).unwrap() {
            return Err(ApiError::bad_request("تاریخ تولد نامعتبر است"));
        }
        if !matches!(self.gender.as_str(), "male" | "female") {
            return Err(ApiError::bad_request("جنسیت را انتخاب کنید"));
        }
        let notes = self.notes.trim().chars().take(1000).collect();
        Ok(CleanMember { full_name, phone, birth_date, gender: self.gender, notes })
    }
}

fn unique_phone(e: rusqlite::Error) -> ApiError {
    match &e {
        rusqlite::Error::SqliteFailure(f, Some(msg)) if f.code == rusqlite::ErrorCode::ConstraintViolation && msg.contains("phone") => {
            ApiError::conflict("عضو دیگری با این شماره موبایل ثبت شده است")
        }
        _ => e.into(),
    }
}

pub async fn create(State(s): State<AppState>, Json(m): Json<MemberInput>) -> ApiResult<Json<MemberDetail>> {
    let m = m.clean()?;
    let c = s.db.conn();
    c.execute(
        "INSERT INTO members (full_name, phone, birth_date, gender, notes) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![m.full_name, m.phone, m.birth_date.to_string(), m.gender, m.notes],
    )
    .map_err(unique_phone)?;
    Ok(Json(detail(&c, c.last_insert_rowid())?))
}

pub async fn update(State(s): State<AppState>, Path(id): Path<i64>, Json(m): Json<MemberInput>) -> ApiResult<Json<MemberDetail>> {
    let m = m.clean()?;
    let c = s.db.conn();
    member_row(&c, id)?;
    c.execute(
        "UPDATE members SET full_name=?1, phone=?2, birth_date=?3, gender=?4, notes=?5 WHERE id=?6",
        params![m.full_name, m.phone, m.birth_date.to_string(), m.gender, m.notes, id],
    )
    .map_err(unique_phone)?;
    Ok(Json(detail(&c, id)?))
}

#[derive(Deserialize)]
pub struct ArchiveInput {
    archived: bool,
}

/// SPEC §4.9: an archived member's face is removed from face-service so they stop
/// being recognized. If face-service is down, the periodic reconcile removes it later.
pub async fn archive(State(s): State<AppState>, Path(id): Path<i64>, Json(a): Json<ArchiveInput>) -> ApiResult<Json<MemberDetail>> {
    {
        let c = s.db.conn();
        member_row(&c, id)?;
        c.execute("UPDATE members SET archived = ?1 WHERE id = ?2", params![a.archived, id])?;
    }
    if a.archived {
        match s.face.delete_face(id).await {
            Ok(()) => {
                s.db.conn().execute("UPDATE members SET face_enrolled = 0 WHERE id = ?1", [id])?;
            }
            Err(e) => tracing::warn!("archive: face of member {id} not removed yet ({}); reconcile will retry", e.message()),
        }
    }
    Ok(Json(detail(&s.db.conn(), id)?))
}

// ---------- subscriptions & payments ----------

#[derive(Deserialize)]
pub struct PreviewQuery {
    plan_id: i64,
    start_date: Option<String>,
}

#[derive(Serialize)]
pub struct Preview {
    start_date: NaiveDate,
    end_date: Option<NaiveDate>,
    price: i64,
}

/// Defaults for the "sell subscription" form (§4.8).
pub async fn preview(State(s): State<AppState>, Path(id): Path<i64>, Query(q): Query<PreviewQuery>) -> ApiResult<Json<Preview>> {
    let c = s.db.conn();
    member_row(&c, id)?;
    let plan = plans::get_plan(&c, q.plan_id)?;
    let today = today();
    let start_date = match q.start_date.as_deref().filter(|x| !x.is_empty()) {
        Some(x) => validate::date(x).ok_or_else(|| ApiError::bad_request("تاریخ شروع نامعتبر است"))?,
        None => {
            let subs: Vec<Sub> = load_subs(&c, Some(id), today)?.remove(&id).unwrap_or_default().iter().map(SubView::rule).collect();
            domain::default_start(&subs, plan.kind, today)
        }
    };
    Ok(Json(Preview { start_date, end_date: domain::end_date(start_date, plan.duration_days), price: plan.price }))
}

#[derive(Deserialize)]
pub struct SellInput {
    plan_id: i64,
    price: i64,
    start_date: String,
    #[serde(default)]
    paid: i64,
    #[serde(default)]
    note: String,
}

pub async fn sell(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<SellInput>) -> ApiResult<Json<MemberDetail>> {
    let mut c = s.db.conn();
    let (member, _) = member_row(&c, id)?;
    if member.archived {
        return Err(ApiError::bad_request("این عضو بایگانی شده است"));
    }
    let plan = plans::get_plan(&c, b.plan_id)?;
    if !plan.active {
        return Err(ApiError::bad_request("این تعرفه غیرفعال است"));
    }
    if !(0..=10_000_000_000).contains(&b.price) {
        return Err(ApiError::bad_request("مبلغ نامعتبر است"));
    }
    if b.paid < 0 || b.paid > b.price {
        return Err(ApiError::bad_request("مبلغ پرداختی باید بین صفر و مبلغ اشتراک باشد"));
    }
    let start = validate::date(&b.start_date).ok_or_else(|| ApiError::bad_request("تاریخ شروع نامعتبر است"))?;
    let end = domain::end_date(start, plan.duration_days);
    let kind = match plan.kind {
        Kind::Sessions => "sessions",
        Kind::Duration => "duration",
        Kind::Combined => "combined",
    };

    let tx = c.transaction()?;
    tx.execute(
        "INSERT INTO subscriptions (member_id, plan_id, plan_name, kind, sessions, duration_days, frequency, shower, locker, price, start_date, end_date)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![id, plan.id, plan.name, kind, plan.sessions, plan.duration_days, plan.frequency, plan.shower, plan.locker, b.price, start.to_string(), end.map(|e| e.to_string())],
    )?;
    let sub_id = tx.last_insert_rowid();
    if b.paid > 0 {
        tx.execute(
            "INSERT INTO payments (subscription_id, amount, note) VALUES (?1, ?2, ?3)",
            params![sub_id, b.paid, b.note.trim().chars().take(300).collect::<String>()],
        )?;
    }
    tx.commit()?;
    Ok(Json(detail(&c, id)?))
}

#[derive(Deserialize)]
pub struct PaymentInput {
    amount: i64,
    #[serde(default)]
    note: String,
}

pub async fn pay(State(s): State<AppState>, Path(sub_id): Path<i64>, Json(b): Json<PaymentInput>) -> ApiResult<Json<MemberDetail>> {
    let c = s.db.conn();
    let member_id: i64 = c
        .query_row("SELECT member_id FROM subscriptions WHERE id = ?1", [sub_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| ApiError::not_found("اشتراک پیدا نشد"))?;
    let debt = load_subs(&c, Some(member_id), today())?
        .remove(&member_id)
        .unwrap_or_default()
        .into_iter()
        .find(|v| v.id == sub_id)
        .map(|v| v.debt)
        .unwrap_or(0);
    if b.amount <= 0 {
        return Err(ApiError::bad_request("مبلغ باید بیشتر از صفر باشد"));
    }
    if b.amount > debt {
        return Err(ApiError::bad_request("مبلغ بیشتر از بدهی این اشتراک است"));
    }
    c.execute(
        "INSERT INTO payments (subscription_id, amount, note) VALUES (?1, ?2, ?3)",
        params![sub_id, b.amount, b.note.trim().chars().take(300).collect::<String>()],
    )?;
    Ok(Json(detail(&c, member_id)?))
}
