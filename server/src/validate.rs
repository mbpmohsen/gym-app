//! Input normalization shared by the APIs. Receptionists type Persian digits.

use chrono::NaiveDate;

/// Persian (۰-۹) and Arabic (٠-٩) digits -> ASCII.
pub fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            '٠'..='٩' => char::from(b'0' + (c as u32 - '٠' as u32) as u8),
            _ => c,
        })
        .collect()
}

/// Iranian mobile -> "09xxxxxxxxx". Accepts +98 / 0098 / 98 / 9xxxxxxxxx, spaces and dashes.
pub fn phone(s: &str) -> Option<String> {
    let d: String = ascii_digits(s).chars().filter(|c| c.is_ascii_digit()).collect();
    let rest = d
        .strip_prefix("0098")
        .or_else(|| d.strip_prefix("98").filter(|r| r.len() == 10))
        .or_else(|| d.strip_prefix('0'))
        .unwrap_or(&d);
    (rest.len() == 10 && rest.starts_with('9')).then(|| format!("0{rest}"))
}

pub fn date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(ascii_digits(s).trim(), "%Y-%m-%d").ok()
}

/// Trimmed, collapsed whitespace; None if empty or too long.
pub fn name(s: &str, max: usize) -> Option<String> {
    let n = s.split_whitespace().collect::<Vec<_>>().join(" ");
    (!n.is_empty() && n.chars().count() <= max).then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones() {
        for ok in ["09121234567", "۰۹۱۲۱۲۳۴۵۶۷", "+989121234567", "00989121234567", "9121234567", "0912 123 4567", "0912-123-4567"] {
            assert_eq!(phone(ok).as_deref(), Some("09121234567"), "{ok}");
        }
        for bad in ["0812345678", "091212345", "0912123456789", "", "abc"] {
            assert_eq!(phone(bad), None, "{bad}");
        }
    }

    #[test]
    fn names() {
        assert_eq!(name("  علی   رضایی ", 50).as_deref(), Some("علی رضایی"));
        assert_eq!(name("   ", 50), None);
    }

    #[test]
    fn dates() {
        assert!(date("2026-10-09").is_some());
        assert!(date("۲۰۲۶-۱۰-۰۹").is_some());
        assert!(date("2026-13-01").is_none());
    }
}

/// ASCII digits -> Persian digits (for messages shown in the UI).
pub fn fa_digits(s: &str) -> String {
    s.chars().map(|c| c.to_digit(10).map(|d| char::from_u32(0x06F0 + d).unwrap()).unwrap_or(c)).collect()
}
