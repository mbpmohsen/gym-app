//! Gregorian -> Jalali (Persian calendar), for Excel cells and monthly grouping.
//! Same algorithm as jalaali-js; valid for the years this app will ever see.

use chrono::{Datelike, NaiveDate};

pub const MONTHS: [&str; 12] = ["فروردین", "اردیبهشت", "خرداد", "تیر", "مرداد", "شهریور", "مهر", "آبان", "آذر", "دی", "بهمن", "اسفند"];

pub fn from_gregorian(d: NaiveDate) -> (i32, u32, u32) {
    const G_D_M: [i32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let (gy, gm, gd) = (d.year(), d.month() as i32, d.day() as i32);
    let gy2 = if gm > 2 { gy + 1 } else { gy };
    let mut days = 355_666 + 365 * gy + (gy2 + 3) / 4 - (gy2 + 99) / 100 + (gy2 + 399) / 400 + gd + G_D_M[(gm - 1) as usize];
    let mut jy = -1595 + 33 * (days / 12_053);
    days %= 12_053;
    jy += 4 * (days / 1461);
    days %= 1461;
    if days > 365 {
        jy += (days - 1) / 365;
        days = (days - 1) % 365;
    }
    let (jm, jd) = if days < 186 { (1 + days / 31, 1 + days % 31) } else { (7 + (days - 186) / 30, 1 + (days - 186) % 30) };
    (jy, jm as u32, jd as u32)
}

/// "1405/07/17"
pub fn date(d: NaiveDate) -> String {
    let (y, m, d) = from_gregorian(d);
    format!("{y}/{m:02}/{d:02}")
}

/// "مهر 1405"
pub fn month_name(d: NaiveDate) -> String {
    let (y, m, _) = from_gregorian(d);
    format!("{} {y}", MONTHS[m as usize - 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        let g = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        assert_eq!(date(g("2026-10-09")), "1405/07/17");
        assert_eq!(date(g("2026-03-21")), "1405/01/01"); // Nowruz
        assert_eq!(date(g("2026-03-20")), "1404/12/29");
        assert_eq!(date(g("2025-03-20")), "1403/12/30"); // leap year end
        assert_eq!(month_name(g("2026-10-09")), "مهر 1405");
    }
}
