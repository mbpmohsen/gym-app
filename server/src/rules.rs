//! Entry/exit rules as pure functions (SPEC §4.3–§4.5). No I/O: time, history and
//! subscriptions are passed in, so every rule is unit-tested below.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::Serialize;

use crate::domain::{self, Kind, Status, Sub};
use crate::settings::Settings;

/// What a recognition from the camera means for a member right now (§4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraAction {
    Enter,
    /// close this open visit
    Exit(i64),
    /// still taking shoes off / putting them on
    Ignore,
}

/// `open`: the member's open visit (id, entered_at).
/// `last_exit`: when their latest visit was closed by the camera or by hand
/// (auto-exits don't count: nobody was standing at the rack then).
pub fn camera_action(open: Option<(i64, NaiveDateTime)>, last_exit: Option<NaiveDateTime>, now: NaiveDateTime, s: &Settings) -> CameraAction {
    let min = Duration::minutes(s.exit_min_minutes as i64);
    match open {
        Some((_, entered)) if now - entered < min => CameraAction::Ignore,
        Some((id, _)) => CameraAction::Exit(id),
        // just left, still at the rack: seen again on the way out is not a new visit
        None if last_exit.is_some_and(|e| now - e < min) => CameraAction::Ignore,
        None => CameraAction::Enter,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flag {
    SecondVisitToday,
    AlternateDay,
    WrongShift,
    OutsideShift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Sound {
    #[serde(rename = "welcome")]
    Welcome,
    #[serde(rename = "goodbye")]
    Goodbye,
    #[serde(rename = "end-of-tuition")]
    EndOfTuition,
    #[serde(rename = "wrong-shift")]
    WrongShift,
}

/// A subscription as the entry rules see it.
#[derive(Debug, Clone)]
pub struct EntrySub {
    pub id: i64,
    pub alternate: bool,
    pub rule: Sub,
}

#[derive(Debug, Clone)]
pub struct Shift {
    /// 0 = Saturday … 6 = Friday
    pub weekday: u32,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub gender: String,
}

/// Iranian week: Saturday = 0.
pub fn weekday(d: NaiveDate) -> u32 {
    (d.weekday().num_days_from_monday() + 2) % 7
}

pub struct EntryInput<'a> {
    pub now: NaiveDateTime,
    pub gender: &'a str,
    pub subs: &'a [EntrySub],
    /// earlier entries today (any source)
    pub earlier_today: &'a [NaiveDateTime],
    /// did any of today's earlier visits already take a session?
    pub deducted_today: bool,
    pub visited_yesterday: bool,
    pub shifts: &'a [Shift],
    pub settings: &'a Settings,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntryPlan {
    pub status: Status,
    /// subscription to take one session from
    pub deduct: Option<i64>,
    pub flags: Vec<Flag>,
    pub sound: Sound,
}

/// §4.5 shift check. No shifts defined at all = the gym doesn't use shifts: no flag.
pub fn shift_flag(shifts: &[Shift], now: NaiveDateTime, gender: &str) -> Option<Flag> {
    if shifts.is_empty() {
        return None;
    }
    let (day, time) = (weekday(now.date()), now.time());
    let current: Vec<&Shift> = shifts.iter().filter(|s| s.weekday == day && s.start <= time && time < s.end).collect();
    if current.is_empty() {
        Some(Flag::OutsideShift)
    } else if current.iter().any(|s| s.gender == gender) {
        None
    } else {
        Some(Flag::WrongShift)
    }
}

/// §4.4 and §4.5: status, which subscription pays, flags and the one sound to play.
pub fn plan_entry(i: &EntryInput) -> EntryPlan {
    let today = i.now.date();
    let rules: Vec<Sub> = i.subs.iter().map(|s| s.rule.clone()).collect();
    let status = domain::status(&rules, today);

    let oldest_valid = |pred: &dyn Fn(&EntrySub) -> bool| {
        i.subs.iter().filter(|s| s.rule.is_valid(today) && pred(s)).min_by_key(|s| (s.rule.start_date, s.id))
    };
    // at most one session per calendar day, from the oldest valid session-bearing subscription
    let deduct_from = if i.deducted_today { None } else { oldest_valid(&|s| s.rule.kind.has_sessions()) };
    // the subscription the member is training on today, for the frequency check
    let governing = deduct_from.or_else(|| oldest_valid(&|s| s.rule.kind.has_sessions() || s.rule.kind == Kind::Duration));

    let mut flags = Vec::new();
    let gap = Duration::hours(i.settings.second_visit_hours as i64);
    if i.earlier_today.iter().any(|t| i.now - *t >= gap) {
        flags.push(Flag::SecondVisitToday);
    }
    if governing.is_some_and(|s| s.alternate) && i.visited_yesterday {
        flags.push(Flag::AlternateDay);
    }
    if let Some(f) = shift_flag(i.shifts, i.now, i.gender) {
        flags.push(f);
    }

    // one sound, by priority: wrong shift > end of tuition > welcome
    let sound = if flags.contains(&Flag::WrongShift) && i.settings.wrong_shift_alarm {
        Sound::WrongShift
    } else if status != Status::Ok {
        Sound::EndOfTuition
    } else {
        Sound::Welcome
    };
    EntryPlan { status, deduct: deduct_from.map(|s| s.id), flags, sound }
}

/// Visits open longer than this are closed automatically (§4.3), at entered + hours.
pub fn auto_exit_at(entered: NaiveDateTime, s: &Settings) -> NaiveDateTime {
    entered + Duration::hours(s.auto_exit_hours as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }
    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }
    fn sessions(id: i64, n: i64, used: i64, start: &str) -> EntrySub {
        EntrySub {
            id,
            alternate: false,
            rule: Sub { kind: Kind::Sessions, sessions: Some(n), sessions_used: used, start_date: d(start), end_date: None, price: 0, paid: 0 },
        }
    }
    fn duration(id: i64, start: &str, end: &str) -> EntrySub {
        EntrySub {
            id,
            alternate: false,
            rule: Sub { kind: Kind::Duration, sessions: None, sessions_used: 0, start_date: d(start), end_date: Some(d(end)), price: 0, paid: 0 },
        }
    }
    fn input<'a>(now: &str, subs: &'a [EntrySub], s: &'a Settings) -> EntryInput<'a> {
        EntryInput { now: dt(now), gender: "male", subs, earlier_today: &[], deducted_today: false, visited_yesterday: false, shifts: &[], settings: s }
    }

    #[test]
    fn camera_enter_ignore_exit() {
        let s = Settings::default(); // 5 minutes
        let now = dt("2026-10-10 10:00");
        assert_eq!(camera_action(None, None, now, &s), CameraAction::Enter);
        // shoes off
        assert_eq!(camera_action(Some((7, dt("2026-10-10 09:56"))), None, now, &s), CameraAction::Ignore);
        assert_eq!(camera_action(Some((7, dt("2026-10-10 09:55"))), None, now, &s), CameraAction::Exit(7));
        // left 2 minutes ago, still putting shoes on
        assert_eq!(camera_action(None, Some(dt("2026-10-10 09:58")), now, &s), CameraAction::Ignore);
        // came back later: new visit
        assert_eq!(camera_action(None, Some(dt("2026-10-10 09:50")), now, &s), CameraAction::Enter);
    }

    #[test]
    fn welcome_and_deduct_oldest_session_sub() {
        let s = Settings::default();
        let subs = [sessions(2, 12, 0, "2026-10-05"), sessions(1, 12, 11, "2026-09-01"), duration(3, "2026-09-01", "2026-12-01")];
        let p = plan_entry(&input("2026-10-10 10:00", &subs, &s));
        assert_eq!(p, EntryPlan { status: Status::Ok, deduct: Some(1), flags: vec![], sound: Sound::Welcome });
    }

    #[test]
    fn duration_only_deducts_nothing() {
        let s = Settings::default();
        let subs = [duration(3, "2026-10-01", "2026-10-30")];
        let p = plan_entry(&input("2026-10-10 10:00", &subs, &s));
        assert_eq!((p.status, p.deduct, p.sound), (Status::Ok, None, Sound::Welcome));
    }

    #[test]
    fn one_session_per_day() {
        let s = Settings::default();
        let subs = [sessions(1, 12, 3, "2026-09-01")];
        let mut i = input("2026-10-10 18:00", &subs, &s);
        i.deducted_today = true;
        assert_eq!(plan_entry(&i).deduct, None);
    }

    #[test]
    fn expired_and_none_and_debt_play_end_of_tuition() {
        let s = Settings::default();
        let used_up = [sessions(1, 12, 12, "2026-09-01")];
        let p = plan_entry(&input("2026-10-10 10:00", &used_up, &s));
        assert_eq!((p.status, p.deduct, p.sound), (Status::Expired, None, Sound::EndOfTuition));

        let p = plan_entry(&input("2026-10-10 10:00", &[], &s));
        assert_eq!((p.status, p.sound), (Status::None, Sound::EndOfTuition));

        let mut owing = sessions(1, 12, 0, "2026-09-01");
        owing.rule.price = 1_000_000;
        owing.rule.paid = 400_000;
        let p = plan_entry(&input("2026-10-10 10:00", std::slice::from_ref(&owing), &s));
        // debt still enters and still takes the session
        assert_eq!((p.status, p.deduct, p.sound), (Status::Debt, Some(1), Sound::EndOfTuition));
    }

    #[test]
    fn prepaid_future_sub_is_not_used() {
        let s = Settings::default();
        let subs = [sessions(1, 12, 0, "2026-10-20")];
        let p = plan_entry(&input("2026-10-10 10:00", &subs, &s));
        assert_eq!((p.status, p.deduct), (Status::Expired, None));
    }

    #[test]
    fn second_visit_today_after_gap_only() {
        let s = Settings::default(); // 3 hours
        let subs = [sessions(1, 12, 1, "2026-09-01")];
        let earlier = [dt("2026-10-10 09:00")];
        let mut i = input("2026-10-10 11:30", &subs, &s);
        i.earlier_today = &earlier;
        assert!(plan_entry(&i).flags.is_empty());
        i.now = dt("2026-10-10 12:00");
        assert_eq!(plan_entry(&i).flags, vec![Flag::SecondVisitToday]);
    }

    #[test]
    fn alternate_day_warning() {
        let s = Settings::default();
        let mut sub = sessions(1, 12, 1, "2026-09-01");
        sub.alternate = true;
        let subs = [sub];
        let mut i = input("2026-10-10 10:00", &subs, &s);
        assert!(plan_entry(&i).flags.is_empty());
        i.visited_yesterday = true;
        assert_eq!(plan_entry(&i).flags, vec![Flag::AlternateDay]);
        // alternate duration plan (nothing deducted) warns too
        let mut dur = duration(2, "2026-10-01", "2026-10-30");
        dur.alternate = true;
        let subs = [dur];
        i.subs = &subs;
        assert_eq!(plan_entry(&i).flags, vec![Flag::AlternateDay]);
    }

    #[test]
    fn shifts() {
        // 2026-10-10 is a Saturday
        assert_eq!(weekday(d("2026-10-10")), 0);
        assert_eq!(weekday(d("2026-10-16")), 6);
        let sh = |g: &str, a: &str, b: &str| Shift { weekday: 0, start: t(a), end: t(b), gender: g.into() };
        let shifts = [sh("female", "08:00", "12:00"), sh("male", "16:00", "22:00")];
        assert_eq!(shift_flag(&[], dt("2026-10-10 10:00"), "male"), None);
        assert_eq!(shift_flag(&shifts, dt("2026-10-10 10:00"), "female"), None);
        assert_eq!(shift_flag(&shifts, dt("2026-10-10 10:00"), "male"), Some(Flag::WrongShift));
        assert_eq!(shift_flag(&shifts, dt("2026-10-10 13:00"), "male"), Some(Flag::OutsideShift));
        // other weekday: nothing defined then
        assert_eq!(shift_flag(&shifts, dt("2026-10-11 10:00"), "female"), Some(Flag::OutsideShift));
    }

    #[test]
    fn sound_priority() {
        let s = Settings::default();
        let quiet = Settings { wrong_shift_alarm: false, ..Settings::default() };
        let shifts = [Shift { weekday: 0, start: t("08:00"), end: t("12:00"), gender: "female".into() }];
        let mut i = input("2026-10-10 10:00", &[], &s); // no subscription + wrong shift
        i.shifts = &shifts;
        let p = plan_entry(&i);
        assert_eq!((p.flags.clone(), p.sound), (vec![Flag::WrongShift], Sound::WrongShift));
        i.settings = &quiet;
        assert_eq!(plan_entry(&i).sound, Sound::EndOfTuition);
    }
}
