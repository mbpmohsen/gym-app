//! SQLite: connection + schema migrations (tracked with PRAGMA user_version).
//! Schema follows docs/SPEC.md §3. Money is integer toman; dates are ISO
//! (YYYY-MM-DD, local), datetimes ISO local "YYYY-MM-DD HH:MM:SS".
//!
//! Never edit a migration that has shipped: append a new one.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // 1: initial schema
    r#"
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );

    CREATE TABLE members (
        id            INTEGER PRIMARY KEY,
        full_name     TEXT NOT NULL,
        phone         TEXT NOT NULL UNIQUE,
        birth_date    TEXT NOT NULL,
        gender        TEXT NOT NULL CHECK (gender IN ('male', 'female')),
        face_enrolled INTEGER NOT NULL DEFAULT 0,
        notes         TEXT NOT NULL DEFAULT '',
        archived      INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
    );

    CREATE TABLE plans (
        id            INTEGER PRIMARY KEY,
        name          TEXT NOT NULL,
        kind          TEXT NOT NULL CHECK (kind IN ('sessions', 'duration', 'combined')),
        sessions      INTEGER CHECK (sessions IS NULL OR sessions > 0),
        duration_days INTEGER CHECK (duration_days IS NULL OR duration_days > 0),
        frequency     TEXT NOT NULL CHECK (frequency IN ('six_days', 'alternate')),
        shower        INTEGER NOT NULL DEFAULT 0,
        locker        INTEGER NOT NULL DEFAULT 0,
        price         INTEGER NOT NULL CHECK (price >= 0),
        active        INTEGER NOT NULL DEFAULT 1,
        CHECK ((kind = 'sessions' AND sessions IS NOT NULL AND duration_days IS NULL)
            OR (kind = 'duration' AND sessions IS NULL AND duration_days IS NOT NULL)
            OR (kind = 'combined' AND sessions IS NOT NULL AND duration_days IS NOT NULL))
    );

    CREATE TABLE subscriptions (
        id            INTEGER PRIMARY KEY,
        member_id     INTEGER NOT NULL REFERENCES members(id),
        plan_id       INTEGER REFERENCES plans(id),
        -- copied from the plan at sale time
        plan_name     TEXT NOT NULL,
        kind          TEXT NOT NULL CHECK (kind IN ('sessions', 'duration', 'combined')),
        sessions      INTEGER,
        duration_days INTEGER,
        frequency     TEXT NOT NULL CHECK (frequency IN ('six_days', 'alternate')),
        shower        INTEGER NOT NULL,
        locker        INTEGER NOT NULL,
        price         INTEGER NOT NULL CHECK (price >= 0),
        start_date    TEXT NOT NULL,
        end_date      TEXT,
        sessions_used INTEGER NOT NULL DEFAULT 0,
        created_at    TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
    );
    CREATE INDEX subscriptions_member ON subscriptions(member_id);

    CREATE TABLE payments (
        id              INTEGER PRIMARY KEY,
        subscription_id INTEGER NOT NULL REFERENCES subscriptions(id),
        amount          INTEGER NOT NULL CHECK (amount > 0),
        paid_at         TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
        note            TEXT NOT NULL DEFAULT ''
    );
    CREATE INDEX payments_subscription ON payments(subscription_id);
    CREATE INDEX payments_paid_at ON payments(paid_at);

    CREATE TABLE visits (
        id              INTEGER PRIMARY KEY,
        member_id       INTEGER NOT NULL REFERENCES members(id),
        entered_at      TEXT NOT NULL,
        exited_at       TEXT,
        entry_source    TEXT NOT NULL CHECK (entry_source IN ('camera', 'manual', 'picked')),
        exit_source     TEXT CHECK (exit_source IN ('camera', 'manual', 'auto')),
        subscription_id INTEGER REFERENCES subscriptions(id),
        status          TEXT NOT NULL CHECK (status IN ('ok', 'expired', 'debt', 'none')),
        flags           TEXT NOT NULL DEFAULT '[]',
        snapshot        TEXT
    );
    CREATE INDEX visits_member ON visits(member_id, entered_at);
    CREATE INDEX visits_entered ON visits(entered_at);
    -- at most one open visit per member
    CREATE UNIQUE INDEX visits_one_open ON visits(member_id) WHERE exited_at IS NULL;

    CREATE TABLE face_events (
        id                INTEGER PRIMARY KEY, -- same id as in face-service (Last-Event-ID)
        type              TEXT NOT NULL CHECK (type IN ('unknown', 'uncertain')),
        candidates        TEXT NOT NULL DEFAULT '[]',
        snapshot          TEXT,
        at                TEXT NOT NULL,
        resolved_visit_id INTEGER REFERENCES visits(id),
        dismissed         INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE shifts (
        id         INTEGER PRIMARY KEY,
        weekday    INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6), -- 0 = Saturday
        start_time TEXT NOT NULL,
        end_time   TEXT NOT NULL,
        gender     TEXT NOT NULL CHECK (gender IN ('male', 'female')),
        CHECK (start_time < end_time)
    );
    "#,
];

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path).with_context(|| format!("open {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        migrate(&mut conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        migrate(&mut conn)?;
        Ok(Self(Mutex::new(conn)))
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap()
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let current: usize = conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
    anyhow::ensure!(
        current <= MIGRATIONS.len(),
        "database is from a newer version of gym-server (schema {current}, this build knows {})",
        MIGRATIONS.len()
    );
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
        tracing::info!("database migrated to schema {}", i + 1);
    }
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    Ok(conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_is_idempotent() {
        let db = Db::in_memory().unwrap();
        let mut c = db.conn();
        migrate(&mut c).unwrap(); // second run is a no-op
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
    }

    #[test]
    fn plan_kind_constraints() {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        let ins = |kind: &str, s: Option<i64>, d: Option<i64>| {
            c.execute(
                "INSERT INTO plans (name, kind, sessions, duration_days, frequency, price) VALUES ('x', ?1, ?2, ?3, 'six_days', 100)",
                rusqlite::params![kind, s, d],
            )
        };
        assert!(ins("sessions", Some(12), None).is_ok());
        assert!(ins("duration", None, Some(30)).is_ok());
        assert!(ins("combined", Some(12), Some(45)).is_ok());
        assert!(ins("sessions", None, None).is_err());
        assert!(ins("duration", Some(12), Some(30)).is_err());
        assert!(ins("combined", Some(12), None).is_err());
    }

    #[test]
    fn one_open_visit_per_member() {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        c.execute("INSERT INTO members (full_name, phone, birth_date, gender) VALUES ('a', '09120000000', '1990-01-01', 'male')", []).unwrap();
        let open = || {
            c.execute(
                "INSERT INTO visits (member_id, entered_at, entry_source, status) VALUES (1, '2026-10-09 10:00:00', 'camera', 'ok')",
                [],
            )
        };
        assert!(open().is_ok());
        assert!(open().is_err(), "second open visit must be rejected");
    }

    #[test]
    fn settings_roundtrip() {
        let db = Db::in_memory().unwrap();
        let c = db.conn();
        assert_eq!(get_setting(&c, "voice").unwrap(), None);
        set_setting(&c, "voice", "male").unwrap();
        set_setting(&c, "voice", "female").unwrap();
        assert_eq!(get_setting(&c, "voice").unwrap().as_deref(), Some("female"));
    }
}
