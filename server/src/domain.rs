//! Business rules as pure functions (SPEC §4.1, §4.2, §4.8). No I/O: `today` is
//! always passed in, so every rule is unit-tested below.

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Sessions,
    Duration,
    Combined,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sessions" => Some(Self::Sessions),
            "duration" => Some(Self::Duration),
            "combined" => Some(Self::Combined),
            _ => None,
        }
    }
    pub fn has_sessions(self) -> bool {
        matches!(self, Self::Sessions | Self::Combined)
    }
    pub fn has_duration(self) -> bool {
        matches!(self, Self::Duration | Self::Combined)
    }
}

/// What the rules need to know about one subscription.
#[derive(Debug, Clone)]
pub struct Sub {
    pub kind: Kind,
    pub sessions: Option<i64>,
    pub sessions_used: i64,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub price: i64,
    pub paid: i64,
}

impl Sub {
    /// §4.1: started, not past its end date, sessions left (as applicable).
    pub fn is_valid(&self, today: NaiveDate) -> bool {
        if today < self.start_date {
            return false; // bought in advance, not started yet
        }
        if self.kind.has_duration() && self.end_date.is_none_or(|e| today > e) {
            return false;
        }
        if self.kind.has_sessions() && self.sessions_remaining().is_none_or(|r| r <= 0) {
            return false;
        }
        true
    }

    pub fn debt(&self) -> i64 {
        (self.price - self.paid).max(0)
    }

    pub fn sessions_remaining(&self) -> Option<i64> {
        self.sessions.map(|s| s - self.sessions_used)
    }

    /// Days left including today; None for session-only plans.
    pub fn days_remaining(&self, today: NaiveDate) -> Option<i64> {
        self.end_date.map(|e| (e - today.max(self.start_date)).num_days() + 1).map(|d| d.max(0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// never had a subscription
    None,
    /// no valid subscription
    Expired,
    /// valid subscription, but something is unpaid
    Debt,
    Ok,
}

/// §4.2. Debt counts across all subscriptions, also old ones.
pub fn status(subs: &[Sub], today: NaiveDate) -> Status {
    if subs.is_empty() {
        return Status::None;
    }
    if !subs.iter().any(|s| s.is_valid(today)) {
        return Status::Expired;
    }
    if subs.iter().any(|s| s.debt() > 0) {
        return Status::Debt;
    }
    Status::Ok
}

/// end = start + duration - 1 (a 30-day plan starting on the 1st ends on the 30th).
pub fn end_date(start: NaiveDate, duration_days: Option<i64>) -> Option<NaiveDate> {
    duration_days.map(|d| start + Days::new((d.max(1) - 1) as u64))
}

/// §4.8: a new time-bound subscription starts the day after the member's current
/// time-bound subscription ends (renewal before expiry), otherwise today.
/// Session-only plans always start today.
pub fn default_start(subs: &[Sub], new_kind: Kind, today: NaiveDate) -> NaiveDate {
    if !new_kind.has_duration() {
        return today;
    }
    subs.iter()
        .filter(|s| s.kind.has_duration() && s.end_date.is_some_and(|e| e >= today))
        // only a subscription the member is actually using (or already bought) pushes the start
        .filter(|s| !s.kind.has_sessions() || s.sessions_remaining().is_some_and(|r| r > 0))
        .filter_map(|s| s.end_date)
        .max()
        .map(|e| e + Days::new(1))
        .unwrap_or(today)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn sub(kind: Kind, sessions: Option<i64>, used: i64, start: &str, days: Option<i64>, price: i64, paid: i64) -> Sub {
        Sub { kind, sessions, sessions_used: used, start_date: d(start), end_date: end_date(d(start), days), price, paid }
    }

    #[test]
    fn end_date_is_inclusive() {
        assert_eq!(end_date(d("2026-10-01"), Some(30)), Some(d("2026-10-30")));
        assert_eq!(end_date(d("2026-10-01"), Some(1)), Some(d("2026-10-01")));
        assert_eq!(end_date(d("2026-10-01"), None), None);
    }

    #[test]
    fn sessions_only_never_expires_by_date() {
        let s = sub(Kind::Sessions, Some(12), 11, "2020-01-01", None, 0, 0);
        assert!(s.is_valid(d("2026-10-09")));
        let s = Sub { sessions_used: 12, ..s };
        assert!(!s.is_valid(d("2026-10-09")));
    }

    #[test]
    fn duration_validity_boundaries() {
        let s = sub(Kind::Duration, None, 0, "2026-10-01", Some(30), 0, 0);
        assert!(!s.is_valid(d("2026-09-30")), "not started yet");
        assert!(s.is_valid(d("2026-10-01")));
        assert!(s.is_valid(d("2026-10-30")), "last day is valid");
        assert!(!s.is_valid(d("2026-10-31")));
    }

    #[test]
    fn combined_needs_both() {
        let s = sub(Kind::Combined, Some(12), 0, "2026-10-01", Some(45), 0, 0);
        assert!(s.is_valid(d("2026-10-20")));
        assert!(!Sub { sessions_used: 12, ..s.clone() }.is_valid(d("2026-10-20")), "sessions used up");
        assert!(!s.is_valid(d("2026-11-15")), "date passed with sessions left");
    }

    #[test]
    fn remaining() {
        let s = sub(Kind::Combined, Some(12), 5, "2026-10-01", Some(30), 0, 0);
        assert_eq!(s.sessions_remaining(), Some(7));
        assert_eq!(s.days_remaining(d("2026-10-30")), Some(1));
        assert_eq!(s.days_remaining(d("2026-10-31")), Some(0));
        assert_eq!(s.days_remaining(d("2026-09-01")), Some(30), "advance purchase counts from start");
    }

    #[test]
    fn status_rules() {
        let today = d("2026-10-09");
        assert_eq!(status(&[], today), Status::None);
        let old = sub(Kind::Duration, None, 0, "2026-08-01", Some(30), 100, 100);
        assert_eq!(status(&[old.clone()], today), Status::Expired);
        let cur = sub(Kind::Duration, None, 0, "2026-10-01", Some(30), 100, 100);
        assert_eq!(status(&[old.clone(), cur.clone()], today), Status::Ok);
        let cur_debt = Sub { paid: 60, ..cur.clone() };
        assert_eq!(status(&[old.clone(), cur_debt], today), Status::Debt);
        // debt on an old subscription still counts
        let old_debt = Sub { paid: 0, ..old };
        assert_eq!(status(&[old_debt.clone(), cur], today), Status::Debt);
        // expired beats debt: "پایان شهریه" either way, but expired is the clearer state
        assert_eq!(status(&[old_debt], today), Status::Expired);
    }

    #[test]
    fn overpaid_is_not_negative_debt() {
        let s = sub(Kind::Sessions, Some(12), 0, "2026-10-01", None, 100, 150);
        assert_eq!(s.debt(), 0);
    }

    #[test]
    fn default_start_rules() {
        let today = d("2026-10-09");
        let cur = sub(Kind::Duration, None, 0, "2026-10-01", Some(30), 0, 0); // ends 10-30
        assert_eq!(default_start(&[], Kind::Duration, today), today);
        assert_eq!(default_start(&[cur.clone()], Kind::Duration, today), d("2026-10-31"), "renewal continues");
        assert_eq!(default_start(&[cur.clone()], Kind::Sessions, today), today, "session plans start today");
        let expired = sub(Kind::Duration, None, 0, "2026-08-01", Some(30), 0, 0);
        assert_eq!(default_start(&[expired], Kind::Combined, today), today);
        // a combined plan whose sessions are used up doesn't push the start
        let used_up = sub(Kind::Combined, Some(12), 12, "2026-10-01", Some(45), 0, 0);
        assert_eq!(default_start(&[used_up], Kind::Duration, today), today);
        // advance purchase: chains after the latest end
        let next = sub(Kind::Duration, None, 0, "2026-10-31", Some(30), 0, 0); // ends 11-29
        assert_eq!(default_start(&[cur, next], Kind::Duration, today), d("2026-11-30"));
    }
}
