//! Reports (SPEC §6). Every report builds one `Table`; the same table is sent to
//! the page as JSON or written as an Excel file, so the export always matches
//! what is on screen.
//!
//!   GET /api/reports/{name}?filters        -> JSON
//!   GET /api/reports/{name}.xlsx?filters   -> Excel (right-to-left, Jalali dates)
//!   ... &sms=1                              -> Excel with name + mobile only

use std::collections::{BTreeMap, HashMap};

use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Days, NaiveDate, NaiveDateTime};
use rusqlite::{params, Connection};
use rust_xlsxwriter::{Format, FormatAlign, Workbook};
use serde::Serialize;
use serde_json::{json, Value};

use crate::domain::{Kind, Status};
use crate::error::{ApiError, ApiResult};
use crate::members::{self, SubView};
use crate::rules::weekday;
use crate::validate::{self, fa_digits};
use crate::visits::{self, FMT};
use crate::{jalali, AppState};

/// How a column is shown (page) and written (Excel).
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ColKind {
    Text,
    /// value: [member_id, name]
    Member,
    Phone,
    Int,
    Money,
    /// ISO date
    Date,
    /// ISO datetime
    Datetime,
    /// ISO datetime, shown as HH:MM
    Time,
    Minutes,
    Status,
    Flags,
    /// busy-hours cell (average entries)
    Heat,
}

#[derive(Debug, Serialize)]
pub struct Col {
    key: &'static str,
    label: String,
    kind: ColKind,
}

fn col(key: &'static str, label: impl Into<String>, kind: ColKind) -> Col {
    Col { key, label: label.into(), kind }
}

#[derive(Debug, Serialize)]
pub struct Total {
    label: &'static str,
    value: Value,
    kind: ColKind,
}

#[derive(Debug, Serialize)]
pub struct Table {
    title: String,
    columns: Vec<Col>,
    rows: Vec<Vec<Value>>,
    totals: Vec<Total>,
}

type Q = HashMap<String, String>;

fn opt<'a>(q: &'a Q, k: &str) -> Option<&'a str> {
    q.get(k).map(|s| s.trim()).filter(|s| !s.is_empty() && *s != "all")
}

fn num(q: &Q, k: &str, default: i64) -> ApiResult<i64> {
    match opt(q, k) {
        None => Ok(default),
        Some(v) => validate::ascii_digits(v).parse().map_err(|_| ApiError::bad_request("عدد نامعتبر در فیلتر")),
    }
}

fn date_param(q: &Q, k: &str, default: NaiveDate) -> ApiResult<NaiveDate> {
    match opt(q, k) {
        None => Ok(default),
        Some(v) => validate::date(v).ok_or_else(|| ApiError::bad_request("تاریخ نامعتبر در فیلتر")),
    }
}

/// from/to, both inclusive; from <= to.
fn range(q: &Q, default_from: NaiveDate, default_to: NaiveDate) -> ApiResult<(NaiveDate, NaiveDate)> {
    let (from, to) = (date_param(q, "from", default_from)?, date_param(q, "to", default_to)?);
    if from > to {
        return Err(ApiError::bad_request("تاریخ شروع بعد از تاریخ پایان است"));
    }
    Ok((from, to))
}

fn gender(q: &Q) -> Option<&str> {
    opt(q, "gender").filter(|g| matches!(*g, "male" | "female"))
}

fn member(id: i64, name: &str) -> Value {
    json!([id, name])
}

struct MemberInfo {
    name: String,
    phone: String,
    gender: String,
}

fn active_members(c: &Connection) -> ApiResult<BTreeMap<i64, MemberInfo>> {
    let mut stmt = c.prepare("SELECT id, full_name, phone, gender FROM members WHERE archived = 0")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, MemberInfo { name: r.get(1)?, phone: r.get(2)?, gender: r.get(3)? })))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn rule_status(subs: &[SubView], today: NaiveDate) -> Status {
    members::summarize(subs, today).0
}

// ---------- the reports ----------

fn revenue(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    // default: this Jalali month
    let month_start = today - Days::new(jalali::from_gregorian(today).2 as u64 - 1);
    let (from, to) = range(q, month_start, today)?;
    let plan: Option<i64> = opt(q, "plan").and_then(|p| p.parse().ok());
    let kind = opt(q, "kind").filter(|k| Kind::parse(k).is_some());
    let mut stmt = c.prepare(
        "SELECT p.paid_at, m.id, m.full_name, m.phone, s.plan_name, s.price, p.amount, p.note
         FROM payments p JOIN subscriptions s ON s.id = p.subscription_id JOIN members m ON m.id = s.member_id
         WHERE date(p.paid_at) BETWEEN ?1 AND ?2 AND (?3 IS NULL OR s.plan_id = ?3) AND (?4 IS NULL OR s.kind = ?4) AND (?5 IS NULL OR m.gender = ?5)
         ORDER BY p.paid_at, p.id",
    )?;
    let raw: Vec<(String, i64, String, String, String, i64, i64, String)> = stmt
        .query_map(params![from.to_string(), to.to_string(), plan, kind, gender(q)], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })?
        .collect::<Result<_, _>>()?;
    let total: i64 = raw.iter().map(|r| r.6).sum();
    let totals = vec![
        Total { label: "تعداد پرداخت", value: json!(raw.len()), kind: ColKind::Int },
        Total { label: "جمع درآمد", value: json!(total), kind: ColKind::Money },
    ];

    let group = opt(q, "group").unwrap_or("none");
    if group == "none" {
        return Ok(Table {
            title: "درآمد".into(),
            columns: vec![
                col("paid_at", "تاریخ پرداخت", ColKind::Datetime),
                col("member", "عضو", ColKind::Member),
                col("phone", "موبایل", ColKind::Phone),
                col("plan", "تعرفه", ColKind::Text),
                col("price", "مبلغ اشتراک", ColKind::Money),
                col("amount", "پرداختی", ColKind::Money),
                col("note", "یادداشت", ColKind::Text),
            ],
            rows: raw.into_iter().map(|r| vec![json!(r.0), member(r.1, &r.2), json!(r.3), json!(r.4), json!(r.5), json!(r.6), json!(r.7)]).collect(),
            totals,
        });
    }
    // sortable key -> (label, count, sum)
    let mut groups: BTreeMap<String, (String, i64, i64)> = BTreeMap::new();
    for r in &raw {
        let d = NaiveDateTime::parse_from_str(&r.0, FMT).map(|t| t.date()).unwrap_or(today);
        let (key, label) = match group {
            "week" => {
                let sat = d - Days::new(weekday(d) as u64);
                (sat.to_string(), format!("هفته‌ی {}", fa_digits(&jalali::date(sat))))
            }
            "month" => {
                let (y, m, _) = jalali::from_gregorian(d);
                (format!("{y}-{m:02}"), fa_digits(&jalali::month_name(d)))
            }
            _ => (d.to_string(), fa_digits(&jalali::date(d))),
        };
        let g = groups.entry(key).or_insert((label, 0, 0));
        g.1 += 1;
        g.2 += r.6;
    }
    Ok(Table {
        title: "درآمد".into(),
        columns: vec![col("period", "دوره", ColKind::Text), col("count", "تعداد پرداخت", ColKind::Int), col("amount", "درآمد", ColKind::Money)],
        rows: groups.into_values().map(|(l, n, s)| vec![json!(l), json!(n), json!(s)]).collect(),
        totals,
    })
}

fn expiring(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let max_sessions = num(q, "sessions", 3)?;
    let max_days = num(q, "days", 7)?;
    let plan: Option<i64> = opt(q, "plan").and_then(|p| p.parse().ok());
    let members = active_members(c)?;
    let subs = members::load_subs(c, None, today)?;
    let mut rows = Vec::new();
    for (id, m) in &members {
        if gender(q).is_some_and(|g| g != m.gender) {
            continue;
        }
        let Some(list) = subs.get(id) else { continue };
        if list.iter().any(|s| s.upcoming) {
            continue; // already renewed
        }
        for s in list.iter().filter(|s| s.valid && plan.is_none_or(|p| s.plan_id == Some(p))) {
            let by_sessions = s.sessions_remaining.is_some_and(|r| r <= max_sessions);
            let by_days = s.days_remaining.is_some_and(|d| d <= max_days);
            if by_sessions || by_days {
                rows.push((s.days_remaining.unwrap_or(i64::MAX), s.sessions_remaining.unwrap_or(i64::MAX), vec![
                    member(*id, &m.name),
                    json!(m.phone),
                    json!(s.plan_name),
                    json!(s.sessions_remaining),
                    json!(s.days_remaining),
                    json!(s.end_date),
                ]));
            }
        }
    }
    rows.sort_by_key(|r| (r.0.min(r.1), r.0));
    Ok(Table {
        title: "در حال اتمام".into(),
        columns: vec![
            col("member", "عضو", ColKind::Member),
            col("phone", "موبایل", ColKind::Phone),
            col("plan", "تعرفه", ColKind::Text),
            col("sessions", "جلسات باقی‌مانده", ColKind::Int),
            col("days", "روزهای باقی‌مانده", ColKind::Int),
            col("end", "تاریخ پایان", ColKind::Date),
        ],
        totals: vec![Total { label: "تعداد", value: json!(rows.len()), kind: ColKind::Int }],
        rows: rows.into_iter().map(|r| r.2).collect(),
    })
}

/// When a subscription stopped being usable (None while it still is).
fn ended_on(c: &Connection, s: &SubView, today: NaiveDate) -> ApiResult<Option<NaiveDate>> {
    if s.valid || s.upcoming {
        return Ok(None);
    }
    let by_date = s.end_date.filter(|e| *e < today);
    let by_sessions = if s.sessions_remaining.is_some_and(|r| r <= 0) {
        let last: Option<String> = c.query_row("SELECT MAX(entered_at) FROM visits WHERE subscription_id = ?1", [s.id], |r| r.get(0))?;
        Some(last.and_then(|t| NaiveDateTime::parse_from_str(&t, FMT).ok()).map(|t| t.date()).unwrap_or(s.start_date))
    } else {
        None
    };
    Ok(match (by_date, by_sessions) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    })
}

fn last_visits(c: &Connection) -> ApiResult<HashMap<i64, NaiveDate>> {
    let mut stmt = c.prepare("SELECT member_id, MAX(entered_at) FROM visits GROUP BY member_id")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, t) = row?;
        if let Ok(t) = NaiveDateTime::parse_from_str(&t, FMT) {
            out.insert(id, t.date());
        }
    }
    Ok(out)
}

fn expired(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let from = opt(q, "from").map(|_| date_param(q, "from", today)).transpose()?;
    let to = opt(q, "to").map(|_| date_param(q, "to", today)).transpose()?;
    let min_days = num(q, "min_days", 0)?;
    let members = active_members(c)?;
    let subs = members::load_subs(c, None, today)?;
    let last = last_visits(c)?;
    let mut rows = Vec::new();
    for (id, m) in &members {
        if gender(q).is_some_and(|g| g != m.gender) {
            continue;
        }
        let Some(list) = subs.get(id) else { continue };
        if rule_status(list, today) != Status::Expired || list.iter().any(|s| s.upcoming) {
            continue;
        }
        let mut latest: Option<(NaiveDate, &SubView)> = None;
        for s in list {
            if let Some(e) = ended_on(c, s, today)? {
                if latest.is_none_or(|(l, _)| e > l) {
                    latest = Some((e, s));
                }
            }
        }
        let Some((ended, sub)) = latest else { continue };
        let since = (today - ended).num_days();
        if since < min_days || from.is_some_and(|f| ended < f) || to.is_some_and(|t| ended > t) {
            continue;
        }
        rows.push((ended, vec![member(*id, &m.name), json!(m.phone), json!(sub.plan_name), json!(ended), json!(since), json!(last.get(id))]));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(Table {
        title: "تمام‌شده‌ها".into(),
        columns: vec![
            col("member", "عضو", ColKind::Member),
            col("phone", "موبایل", ColKind::Phone),
            col("plan", "آخرین تعرفه", ColKind::Text),
            col("ended", "تاریخ اتمام", ColKind::Date),
            col("since", "روز از اتمام", ColKind::Int),
            col("last_visit", "آخرین ورود", ColKind::Date),
        ],
        totals: vec![Total { label: "تعداد", value: json!(rows.len()), kind: ColKind::Int }],
        rows: rows.into_iter().map(|r| r.1).collect(),
    })
}

fn debtors(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let min = num(q, "min_debt", 1)?.max(1);
    let members = active_members(c)?;
    let subs = members::load_subs(c, None, today)?;
    let mut rows = Vec::new();
    for (id, m) in &members {
        if gender(q).is_some_and(|g| g != m.gender) {
            continue;
        }
        for s in subs.get(id).into_iter().flatten().filter(|s| s.debt >= min) {
            rows.push((s.debt, vec![member(*id, &m.name), json!(m.phone), json!(s.plan_name), json!(s.start_date), json!(s.price), json!(s.paid), json!(s.debt)]));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let total: i64 = rows.iter().map(|r| r.0).sum();
    Ok(Table {
        title: "بدهکاران".into(),
        columns: vec![
            col("member", "عضو", ColKind::Member),
            col("phone", "موبایل", ColKind::Phone),
            col("plan", "اشتراک", ColKind::Text),
            col("start", "شروع", ColKind::Date),
            col("price", "مبلغ", ColKind::Money),
            col("paid", "پرداختی", ColKind::Money),
            col("debt", "بدهی", ColKind::Money),
        ],
        totals: vec![
            Total { label: "تعداد", value: json!(rows.len()), kind: ColKind::Int },
            Total { label: "جمع بدهی", value: json!(total), kind: ColKind::Money },
        ],
        rows: rows.into_iter().map(|r| r.1).collect(),
    })
}

const SOURCE_LABEL: [(&str, &str); 3] = [("camera", "دوربین"), ("manual", "دستی"), ("picked", "انتخاب از دوربین")];

fn visits_report(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let (from, to) = range(q, today, today)?;
    let status = opt(q, "status").filter(|s| matches!(*s, "ok" | "expired" | "debt" | "none"));
    let flag = opt(q, "flag").filter(|f| matches!(*f, "second_visit_today" | "alternate_day" | "wrong_shift" | "outside_shift")).map(|f| format!("%\"{f}\"%"));
    let source = opt(q, "source").filter(|s| matches!(*s, "camera" | "manual" | "picked"));
    let mut stmt = c.prepare(
        "SELECT m.id, m.full_name, m.phone, v.entered_at, v.exited_at, v.status, v.flags, v.entry_source
         FROM visits v JOIN members m ON m.id = v.member_id
         WHERE date(v.entered_at) BETWEEN ?1 AND ?2 AND (?3 IS NULL OR m.gender = ?3) AND (?4 IS NULL OR v.status = ?4)
           AND (?5 IS NULL OR v.flags LIKE ?5) AND (?6 IS NULL OR v.entry_source = ?6)
         ORDER BY v.entered_at DESC LIMIT 10000",
    )?;
    let raw: Vec<(i64, String, String, String, Option<String>, String, String, String)> = stmt
        .query_map(params![from.to_string(), to.to_string(), gender(q), status, flag, source], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })?
        .collect::<Result<_, _>>()?;
    let people: std::collections::HashSet<i64> = raw.iter().map(|r| r.0).collect();
    let totals = vec![
        Total { label: "تعداد ورود", value: json!(raw.len()), kind: ColKind::Int },
        Total { label: "تعداد افراد", value: json!(people.len()), kind: ColKind::Int },
    ];
    let rows = raw
        .into_iter()
        .map(|r| {
            let minutes = r.4.as_deref().and_then(|out| {
                Some((NaiveDateTime::parse_from_str(out, FMT).ok()? - NaiveDateTime::parse_from_str(&r.3, FMT).ok()?).num_minutes())
            });
            let src = SOURCE_LABEL.iter().find(|(k, _)| *k == r.7).map(|(_, l)| *l).unwrap_or("");
            vec![member(r.0, &r.1), json!(r.2), json!(r.3), json!(r.4), json!(minutes), json!(r.5), serde_json::from_str(&r.6).unwrap_or(json!([])), json!(src)]
        })
        .collect();
    Ok(Table {
        title: "ورودها".into(),
        columns: vec![
            col("member", "عضو", ColKind::Member),
            col("phone", "موبایل", ColKind::Phone),
            col("entered", "ورود", ColKind::Datetime),
            col("exited", "خروج", ColKind::Time),
            col("minutes", "مدت حضور", ColKind::Minutes),
            col("status", "شهریه", ColKind::Status),
            col("flags", "برچسب‌ها", ColKind::Flags),
            col("source", "منبع", ColKind::Text),
        ],
        rows,
        totals,
    })
}

const DAYS: [&str; 7] = ["شنبه", "یکشنبه", "دوشنبه", "سه‌شنبه", "چهارشنبه", "پنجشنبه", "جمعه"];
const HOUR_KEYS: [&str; 24] = [
    "h0", "h1", "h2", "h3", "h4", "h5", "h6", "h7", "h8", "h9", "h10", "h11", "h12", "h13", "h14", "h15", "h16", "h17", "h18", "h19", "h20", "h21", "h22", "h23",
];

/// Average entries per weekday × hour over the range.
fn busy(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let (from, to) = range(q, today - Days::new(29), today)?;
    let mut stmt = c.prepare(
        "SELECT v.entered_at FROM visits v JOIN members m ON m.id = v.member_id
         WHERE date(v.entered_at) BETWEEN ?1 AND ?2 AND (?3 IS NULL OR m.gender = ?3)",
    )?;
    let mut counts = [[0u32; 24]; 7];
    for t in stmt.query_map(params![from.to_string(), to.to_string(), gender(q)], |r| r.get::<_, String>(0))? {
        if let Ok(t) = NaiveDateTime::parse_from_str(&t?, FMT) {
            counts[weekday(t.date()) as usize][chrono::Timelike::hour(&t) as usize] += 1;
        }
    }
    // how many of each weekday the range holds
    let mut occurrences = [0u32; 7];
    let mut d = from;
    while d <= to {
        occurrences[weekday(d) as usize] += 1;
        d = d + Days::new(1);
    }
    let used: Vec<usize> = (0..24).filter(|h| counts.iter().any(|row| row[*h] > 0)).collect();
    let (first, last) = (used.first().copied().unwrap_or(6).min(6), used.last().copied().unwrap_or(22).max(22));
    let mut columns = vec![col("day", "روز", ColKind::Text)];
    columns.extend((first..=last).map(|h| col(HOUR_KEYS[h], fa_digits(&h.to_string()), ColKind::Heat)));
    let rows = (0..7)
        .map(|day| {
            let mut row = vec![json!(DAYS[day])];
            row.extend((first..=last).map(|h| {
                let avg = if occurrences[day] == 0 { 0.0 } else { counts[day][h] as f64 / occurrences[day] as f64 };
                json!((avg * 10.0).round() / 10.0)
            }));
            row
        })
        .collect();
    let total: u32 = counts.iter().flatten().sum();
    Ok(Table { title: "ساعات شلوغی".into(), columns, rows, totals: vec![Total { label: "کل ورودها", value: json!(total), kind: ColKind::Int }] })
}

fn absent(c: &Connection, q: &Q) -> ApiResult<Table> {
    let today = visits::now().date();
    let min_days = num(q, "days", 7)?;
    let members = active_members(c)?;
    let subs = members::load_subs(c, None, today)?;
    let last = last_visits(c)?;
    let mut rows = Vec::new();
    for (id, m) in &members {
        if gender(q).is_some_and(|g| g != m.gender) {
            continue;
        }
        let Some(valid) = subs.get(id).map(|l| l.iter().filter(|s| s.valid).collect::<Vec<_>>()).filter(|v| !v.is_empty()) else { continue };
        let oldest_start = valid.iter().map(|s| s.start_date).min().unwrap();
        // never came since buying: count from the start
        let since_date = last.get(id).map_or(oldest_start, |l| (*l).max(oldest_start));
        let days = (today - since_date).num_days();
        if days >= min_days {
            let current = members::summarize(subs.get(id).unwrap(), today).2.map(|c| c.plan_name).unwrap_or_default();
            rows.push((days, vec![member(*id, &m.name), json!(m.phone), json!(current), json!(last.get(id)), json!(days)]));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(Table {
        title: "غایبین".into(),
        columns: vec![
            col("member", "عضو", ColKind::Member),
            col("phone", "موبایل", ColKind::Phone),
            col("plan", "اشتراک فعلی", ColKind::Text),
            col("last_visit", "آخرین ورود", ColKind::Date),
            col("days", "روز غیبت", ColKind::Int),
        ],
        totals: vec![Total { label: "تعداد", value: json!(rows.len()), kind: ColKind::Int }],
        rows: rows.into_iter().map(|r| r.1).collect(),
    })
}

fn build(c: &Connection, name: &str, q: &Q) -> ApiResult<Table> {
    match name {
        "revenue" => revenue(c, q),
        "expiring" => expiring(c, q),
        "expired" => expired(c, q),
        "debtors" => debtors(c, q),
        "visits" => visits_report(c, q),
        "busy" => busy(c, q),
        "absent" => absent(c, q),
        _ => Err(ApiError::not_found("گزارش پیدا نشد")),
    }
}

/// Keeps only name + mobile (one row per person), for bulk SMS tools.
fn for_sms(t: Table) -> ApiResult<Table> {
    let m = t.columns.iter().position(|c| c.kind == ColKind::Member);
    let p = t.columns.iter().position(|c| c.kind == ColKind::Phone);
    let (Some(m), Some(p)) = (m, p) else { return Err(ApiError::bad_request("این گزارش شماره موبایل ندارد")) };
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Vec<Value>> = t.rows.into_iter().filter(|r| seen.insert(r[p].clone())).map(|r| vec![r[m].clone(), r[p].clone()]).collect();
    Ok(Table {
        title: format!("{} - پیامک", t.title),
        columns: vec![col("member", "نام", ColKind::Member), col("phone", "موبایل", ColKind::Phone)],
        totals: vec![Total { label: "تعداد", value: json!(rows.len()), kind: ColKind::Int }],
        rows,
    })
}

pub async fn get(State(s): State<AppState>, Path(name): Path<String>, Query(q): Query<Q>) -> ApiResult<Response> {
    let (name, xlsx) = match name.strip_suffix(".xlsx") {
        Some(n) => (n.to_string(), true),
        None => (name, false),
    };
    let mut table = build(&s.db.conn(), &name, &q)?;
    if q.get("sms").is_some_and(|v| v == "1") {
        table = for_sms(table)?;
    }
    if !xlsx {
        return Ok(Json(table).into_response());
    }
    let bytes = excel(&table).map_err(ApiError::internal)?;
    let file = format!("{} {}.xlsx", table.title, fa_digits(&jalali::date(visits::now().date()).replace('/', "-")));
    Ok((
        [
            (header::CONTENT_TYPE, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"report.xlsx\"; filename*=UTF-8''{}", percent(&file))),
        ],
        bytes,
    )
        .into_response())
}

fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ---------- Excel ----------

fn status_label(s: &str) -> &'static str {
    match s {
        "ok" => "شهریه دارد",
        "debt" => "بدهکار",
        "expired" => "پایان شهریه",
        _ => "بدون اشتراک",
    }
}

fn flag_label(f: &str) -> &'static str {
    match f {
        "second_visit_today" => "بار دوم امروز",
        "alternate_day" => "یک روز در میان",
        "wrong_shift" => "سانس نامعتبر",
        "outside_shift" => "خارج از سانس",
        _ => "",
    }
}

/// A cell as Excel text, for kinds written as strings.
fn cell_text(kind: ColKind, v: &Value) -> Option<String> {
    let s = v.as_str();
    Some(match kind {
        ColKind::Member => v.get(1)?.as_str()?.to_string(),
        ColKind::Date => jalali::date(validate::date(s?)?),
        ColKind::Datetime => {
            let t = NaiveDateTime::parse_from_str(s?, FMT).ok()?;
            format!("{} {}", jalali::date(t.date()), t.format("%H:%M"))
        }
        ColKind::Time => NaiveDateTime::parse_from_str(s?, FMT).ok()?.format("%H:%M").to_string(),
        ColKind::Status => status_label(s?).to_string(),
        ColKind::Flags => v.as_array()?.iter().filter_map(|f| f.as_str()).map(flag_label).collect::<Vec<_>>().join("، "),
        _ => s?.to_string(),
    })
}

fn excel(t: &Table) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.set_name("گزارش")?;
    ws.set_right_to_left(true);
    let font = |f: Format| f.set_font_name("Tahoma").set_font_size(10);
    let head = font(Format::new()).set_bold().set_background_color("E8E8E8").set_align(FormatAlign::Center);
    let text = font(Format::new());
    let money = font(Format::new()).set_num_format("#,##0");
    let int = font(Format::new()).set_num_format("0");
    let heat = font(Format::new()).set_num_format("0.0");
    let bold = font(Format::new()).set_bold();
    let bold_money = font(Format::new()).set_bold().set_num_format("#,##0");

    for (i, c) in t.columns.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, &c.label, &head)?;
    }
    for (r, row) in t.rows.iter().enumerate() {
        let r = r as u32 + 1;
        for (i, (c, v)) in t.columns.iter().zip(row).enumerate() {
            let i = i as u16;
            match c.kind {
                ColKind::Money | ColKind::Int | ColKind::Minutes | ColKind::Heat if v.is_number() => {
                    let f = match c.kind {
                        ColKind::Money => &money,
                        ColKind::Heat => &heat,
                        _ => &int,
                    };
                    ws.write_number_with_format(r, i, v.as_f64().unwrap(), f)?;
                }
                _ => {
                    if let Some(s) = cell_text(c.kind, v) {
                        ws.write_string_with_format(r, i, &s, &text)?;
                    }
                }
            }
        }
    }
    let mut r = t.rows.len() as u32 + 2;
    for total in &t.totals {
        ws.write_string_with_format(r, 0, total.label, &bold)?;
        ws.write_number_with_format(r, 1, total.value.as_f64().unwrap_or(0.0), if total.kind == ColKind::Money { &bold_money } else { &bold })?;
        r += 1;
    }
    ws.set_freeze_panes(1, 0)?;
    ws.autofit();
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn q(pairs: &[(&str, &str)]) -> Q {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn seed() -> Db {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        c.execute_batch(
            "INSERT INTO members (id, full_name, phone, birth_date, gender) VALUES
               (1, 'علی', '09120000001', '1990-01-01', 'male'),
               (2, 'مریم', '09120000002', '1990-01-01', 'female');
             INSERT INTO subscriptions (id, member_id, plan_name, kind, sessions, sessions_used, frequency, shower, locker, price, start_date)
               VALUES (1, 1, '12 جلسه', 'sessions', 12, 10, 'six_days', 0, 0, 1000000, '2026-01-01'),
                      (2, 2, '12 جلسه', 'sessions', 12, 12, 'six_days', 0, 0, 1000000, '2026-01-01');
             INSERT INTO payments (subscription_id, amount, paid_at) VALUES (1, 600000, '2026-02-01 10:00:00'), (2, 1000000, '2026-02-03 10:00:00');
             INSERT INTO visits (member_id, entered_at, exited_at, entry_source, exit_source, subscription_id, status)
               VALUES (2, '2026-02-10 18:00:00', '2026-02-10 19:30:00', 'camera', 'camera', 2, 'ok');",
        )
        .unwrap();
        drop(c);
        db
    }

    #[test]
    fn revenue_rows_and_groups() {
        let db = seed();
        let c = db.conn();
        let t = revenue(&c, &q(&[("from", "2026-01-01"), ("to", "2026-12-31")])).unwrap();
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.totals[1].value, json!(1_600_000));
        let t = revenue(&c, &q(&[("from", "2026-01-01"), ("to", "2026-12-31"), ("group", "month")])).unwrap();
        assert_eq!(t.rows.len(), 1); // both in Bahman 1404
        let t = revenue(&c, &q(&[("from", "2026-01-01"), ("to", "2026-12-31"), ("gender", "female")])).unwrap();
        assert_eq!(t.totals[1].value, json!(1_000_000));
    }

    #[test]
    fn expiring_expired_debtors() {
        let db = seed();
        let c = db.conn();
        // علی: 2 sessions left
        let t = expiring(&c, &q(&[])).unwrap();
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0][3], json!(2));
        // مریم used everything; ended on her last visit
        let t = expired(&c, &q(&[])).unwrap();
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.rows[0][3], json!("2026-02-10"));
        let t = debtors(&c, &q(&[])).unwrap();
        assert_eq!(t.rows.len(), 1);
        assert_eq!(t.totals[1].value, json!(400_000));
    }

    #[test]
    fn visits_and_busy_and_sms() {
        let db = seed();
        let c = db.conn();
        let t = visits_report(&c, &q(&[("from", "2026-02-01"), ("to", "2026-02-28")])).unwrap();
        assert_eq!(t.rows[0][4], json!(90));
        let t = busy(&c, &q(&[("from", "2026-02-10"), ("to", "2026-02-10")])).unwrap();
        let tue = weekday(NaiveDate::from_ymd_opt(2026, 2, 10).unwrap()) as usize;
        let h18 = t.columns.iter().position(|c| c.key == "h18").unwrap();
        assert_eq!(t.rows[tue][h18], json!(1.0));
        let sms = for_sms(debtors(&c, &q(&[])).unwrap()).unwrap();
        assert_eq!(sms.columns.len(), 2);
        assert!(excel(&sms).unwrap().len() > 1000);
        assert!(excel(&busy(&c, &q(&[])).unwrap()).is_ok());
    }
}

// ---------- dashboard (SPEC §6) ----------

#[derive(Serialize)]
pub struct Dashboard {
    inside: i64,
    entries_today: i64,
    revenue_today: i64,
    revenue_month: i64,
    active_members: i64,
    expiring_week: usize,
    total_debt: i64,
    no_face: i64,
    /// last 30 days, oldest first: [iso date, revenue, entries]
    days: Vec<(NaiveDate, i64, i64)>,
    busy: Table,
}

pub async fn dashboard(State(s): State<AppState>) -> ApiResult<Json<Dashboard>> {
    let c = s.db.conn();
    let today = visits::now().date();
    let t = today.to_string();
    let month_start = today - Days::new(jalali::from_gregorian(today).2 as u64 - 1);
    let from30 = today - Days::new(29);
    let one = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> ApiResult<i64> { Ok(c.query_row(sql, p, |r| r.get(0))?) };

    let inside = one("SELECT COUNT(*) FROM visits WHERE exited_at IS NULL", &[])?;
    let entries_today = one("SELECT COUNT(DISTINCT member_id) FROM visits WHERE date(entered_at) = ?1", &[&t])?;
    let revenue_today = one("SELECT COALESCE(SUM(amount), 0) FROM payments WHERE date(paid_at) = ?1", &[&t])?;
    let revenue_month = one("SELECT COALESCE(SUM(amount), 0) FROM payments WHERE date(paid_at) BETWEEN ?1 AND ?2", &[&month_start.to_string(), &t])?;
    let no_face = one("SELECT COUNT(*) FROM members WHERE archived = 0 AND face_enrolled = 0", &[])?;

    let members = active_members(&c)?;
    let subs = members::load_subs(&c, None, today)?;
    let active_members = members.keys().filter(|id| subs.get(id).is_some_and(|l| l.iter().any(|s| s.valid))).count() as i64;
    let total_debt: i64 = members.keys().filter_map(|id| subs.get(id)).flatten().map(|s| s.debt).sum();
    let expiring_week = expiring(&c, &Q::from([("sessions".into(), "3".into()), ("days".into(), "7".into())]))?.rows.len();

    let mut by_day: BTreeMap<NaiveDate, (i64, i64)> = BTreeMap::new();
    let mut d = from30;
    while d <= today {
        by_day.insert(d, (0, 0));
        d = d + Days::new(1);
    }
    let (f, to) = (from30.to_string(), t.clone());
    let mut stmt = c.prepare("SELECT date(paid_at), SUM(amount) FROM payments WHERE date(paid_at) BETWEEN ?1 AND ?2 GROUP BY 1")?;
    for row in stmt.query_map([&f, &to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (day, sum) = row?;
        if let Some(e) = validate::date(&day).and_then(|d| by_day.get_mut(&d)) {
            e.0 = sum;
        }
    }
    let mut stmt = c.prepare("SELECT date(entered_at), COUNT(DISTINCT member_id) FROM visits WHERE date(entered_at) BETWEEN ?1 AND ?2 GROUP BY 1")?;
    for row in stmt.query_map([&f, &to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
        let (day, n) = row?;
        if let Some(e) = validate::date(&day).and_then(|d| by_day.get_mut(&d)) {
            e.1 = n;
        }
    }
    let busy = busy(&c, &Q::new())?;
    Ok(Json(Dashboard {
        inside,
        entries_today,
        revenue_today,
        revenue_month,
        active_members,
        expiring_week,
        total_debt,
        no_face,
        days: by_day.into_iter().map(|(d, (r, e))| (d, r, e)).collect(),
        busy,
    }))
}
