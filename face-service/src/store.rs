//! SQLite persistence: members' embeddings and the recent event log
//! (the log lets SSE clients resume with Last-Event-ID after a restart).

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::gallery::Gallery;
use crate::recognizer::{Embedding, EMBEDDING_DIM};

/// Events kept for replay.
const EVENT_LOG_SIZE: i64 = 2000;

pub struct Store {
    conn: Mutex<Connection>,
}

#[derive(Debug, Serialize)]
pub struct MemberInfo {
    pub member_id: String,
    pub samples: i64,
    pub enrolled_at: i64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS members (
                 member_id   TEXT PRIMARY KEY,
                 enrolled_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS samples (
                 id        INTEGER PRIMARY KEY,
                 member_id TEXT NOT NULL REFERENCES members(member_id) ON DELETE CASCADE,
                 embedding BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS events (
                 id   INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts   INTEGER NOT NULL,
                 json TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn load_gallery(&self) -> Result<Gallery> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT member_id, embedding FROM samples")?;
        let mut g = Gallery::default();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))?;
        for row in rows {
            let (id, blob) = row?;
            if let Some(e) = decode(&blob) {
                g.add(&id, e);
            }
        }
        Ok(g)
    }

    /// Replaces all samples of a member (re-enrollment overwrites).
    pub fn put_member(&self, member_id: &str, samples: &[Embedding]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM members WHERE member_id = ?1", [member_id])?;
        tx.execute("INSERT INTO members (member_id, enrolled_at) VALUES (?1, ?2)", params![member_id, now_ms()])?;
        for e in samples {
            tx.execute("INSERT INTO samples (member_id, embedding) VALUES (?1, ?2)", params![member_id, encode(e)])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_member(&self, member_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.execute("DELETE FROM members WHERE member_id = ?1", [member_id])? > 0)
    }

    pub fn list_members(&self) -> Result<Vec<MemberInfo>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT m.member_id, COUNT(s.id), m.enrolled_at FROM members m
             LEFT JOIN samples s ON s.member_id = m.member_id
             GROUP BY m.member_id ORDER BY m.member_id",
        )?;
        let rows = stmt.query_map([], |r| Ok(MemberInfo { member_id: r.get(0)?, samples: r.get(1)?, enrolled_at: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Stores an event (JSON without id) and returns its id. Trims the log.
    pub fn insert_event(&self, ts: i64, json: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO events (ts, json) VALUES (?1, ?2)", params![ts, json])?;
        let id = conn.last_insert_rowid();
        if id % 100 == 0 {
            conn.execute("DELETE FROM events WHERE id <= ?1", [id - EVENT_LOG_SIZE])?;
        }
        Ok(id)
    }

    pub fn events_after(&self, after: i64) -> Result<Vec<(i64, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, json FROM events WHERE id > ?1 ORDER BY id LIMIT 500")?;
        let rows = stmt.query_map([after], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Ids below this have been trimmed; their snapshot files can go too.
    pub fn oldest_event_id(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT COALESCE(MIN(id), 0) FROM events", [], |r| r.get(0))?)
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
}

fn encode(e: &Embedding) -> Vec<u8> {
    e.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn decode(b: &[u8]) -> Option<Embedding> {
    if b.len() != EMBEDDING_DIM * 4 {
        return None;
    }
    let mut e = [0.0; EMBEDDING_DIM];
    for (i, c) in b.chunks_exact(4).enumerate() {
        e[i] = f32::from_le_bytes(c.try_into().unwrap());
    }
    Some(e)
}
