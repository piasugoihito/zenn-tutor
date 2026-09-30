//! ローカル SQLite 永続化（既読記事リスト・対話ログ・理解度メモ・設定）

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub type Db = std::sync::Arc<std::sync::Mutex<Connection>>;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        -- 1日1本の厳選記事（= 既読記事リスト）
        CREATE TABLE IF NOT EXISTS picks (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            date         TEXT NOT NULL UNIQUE,
            url          TEXT NOT NULL UNIQUE,
            title        TEXT NOT NULL,
            author       TEXT NOT NULL DEFAULT '',
            topics       TEXT NOT NULL DEFAULT '[]',
            summary      TEXT NOT NULL DEFAULT '',
            bridge       TEXT NOT NULL DEFAULT '[]',
            related      TEXT NOT NULL DEFAULT '',
            body_text    TEXT NOT NULL DEFAULT '',
            rule_score   REAL NOT NULL DEFAULT 0,
            status       TEXT NOT NULL DEFAULT 'ready', -- ready | in_progress | completed
            created_at   TEXT NOT NULL,
            completed_at TEXT
        );

        -- 対話ログ
        CREATE TABLE IF NOT EXISTS messages (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            pick_id    INTEGER NOT NULL REFERENCES picks(id) ON DELETE CASCADE,
            role       TEXT NOT NULL, -- user | model
            kind       TEXT NOT NULL DEFAULT 'chat', -- chat | summary | verdict
            content    TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        -- 理解度メモ（パス時に獲得した概念。翌日以降の選定に反映）
        CREATE TABLE IF NOT EXISTS memos (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            pick_id      INTEGER NOT NULL REFERENCES picks(id) ON DELETE CASCADE,
            concept      TEXT NOT NULL,
            user_summary TEXT NOT NULL,
            feedback     TEXT NOT NULL,
            passed       INTEGER NOT NULL,
            created_at   TEXT NOT NULL
        );
        ",
    )?;
    Ok(conn)
}

pub fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

// ---------- settings ----------

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()
        .ok()
        .flatten()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn delete_setting(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    Ok(())
}

// ---------- picks ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pick {
    pub id: i64,
    pub date: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub topics: Vec<String>,
    pub summary: String,
    pub bridge: Vec<String>,
    pub related: String,
    #[serde(skip_serializing)]
    pub body_text: String,
    pub rule_score: f64,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub struct NewPick {
    pub date: String,
    pub url: String,
    pub title: String,
    pub author: String,
    pub topics: Vec<String>,
    pub summary: String,
    pub bridge: Vec<String>,
    pub related: String,
    pub body_text: String,
    pub rule_score: f64,
}

const PICK_COLUMNS: &str = "id, date, url, title, author, topics, summary, bridge, related, body_text, rule_score, status, created_at, completed_at";

fn row_to_pick(r: &rusqlite::Row) -> rusqlite::Result<Pick> {
    let topics: String = r.get(5)?;
    let bridge: String = r.get(7)?;
    Ok(Pick {
        id: r.get(0)?,
        date: r.get(1)?,
        url: r.get(2)?,
        title: r.get(3)?,
        author: r.get(4)?,
        topics: serde_json::from_str(&topics).unwrap_or_default(),
        summary: r.get(6)?,
        bridge: serde_json::from_str(&bridge).unwrap_or_default(),
        related: r.get(8)?,
        body_text: r.get(9)?,
        rule_score: r.get(10)?,
        status: r.get(11)?,
        created_at: r.get(12)?,
        completed_at: r.get(13)?,
    })
}

pub fn insert_pick(conn: &Connection, p: &NewPick) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO picks (date, url, title, author, topics, summary, bridge, related, body_text, rule_score, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            p.date,
            p.url,
            p.title,
            p.author,
            serde_json::to_string(&p.topics).unwrap(),
            p.summary,
            serde_json::to_string(&p.bridge).unwrap(),
            p.related,
            p.body_text,
            p.rule_score,
            now()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_pick(conn: &Connection, id: i64) -> rusqlite::Result<Option<Pick>> {
    conn.query_row(
        &format!("SELECT {PICK_COLUMNS} FROM picks WHERE id = ?1"),
        [id],
        row_to_pick,
    )
    .optional()
}

pub fn get_pick_by_date(conn: &Connection, date: &str) -> rusqlite::Result<Option<Pick>> {
    conn.query_row(
        &format!("SELECT {PICK_COLUMNS} FROM picks WHERE date = ?1"),
        [date],
        row_to_pick,
    )
    .optional()
}

pub fn list_picks(conn: &Connection) -> rusqlite::Result<Vec<Pick>> {
    let mut stmt = conn.prepare(&format!("SELECT {PICK_COLUMNS} FROM picks ORDER BY date DESC"))?;
    let rows = stmt.query_map([], row_to_pick)?;
    rows.collect()
}

pub fn picked_urls(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT url FROM picks")?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    rows.collect()
}

pub fn set_pick_status(conn: &Connection, id: i64, status: &str) -> rusqlite::Result<()> {
    let completed_at = (status == "completed").then(now);
    conn.execute(
        "UPDATE picks SET status = ?1, completed_at = ?2 WHERE id = ?3",
        params![status, completed_at, id],
    )?;
    Ok(())
}

// ---------- messages ----------

#[derive(Debug, Clone, Serialize)]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub kind: String,
    pub content: String,
    pub created_at: String,
}

pub fn add_message(conn: &Connection, pick_id: i64, role: &str, kind: &str, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO messages (pick_id, role, kind, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![pick_id, role, kind, content, now()],
    )?;
    Ok(())
}

pub fn list_messages(conn: &Connection, pick_id: i64) -> rusqlite::Result<Vec<Message>> {
    let mut stmt = conn.prepare(
        "SELECT id, role, kind, content, created_at FROM messages WHERE pick_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([pick_id], |r| {
        Ok(Message {
            id: r.get(0)?,
            role: r.get(1)?,
            kind: r.get(2)?,
            content: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    rows.collect()
}

// ---------- memos ----------

#[derive(Debug, Clone, Serialize)]
pub struct Memo {
    pub id: i64,
    pub pick_id: i64,
    pub title: String,
    pub concept: String,
    pub user_summary: String,
    pub feedback: String,
    pub passed: bool,
    pub created_at: String,
}

pub fn add_memo(
    conn: &Connection,
    pick_id: i64,
    concept: &str,
    user_summary: &str,
    feedback: &str,
    passed: bool,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO memos (pick_id, concept, user_summary, feedback, passed, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![pick_id, concept, user_summary, feedback, passed as i64, now()],
    )?;
    Ok(())
}

pub fn list_memos(conn: &Connection) -> rusqlite::Result<Vec<Memo>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.pick_id, p.title, m.concept, m.user_summary, m.feedback, m.passed, m.created_at
         FROM memos m JOIN picks p ON p.id = m.pick_id ORDER BY m.id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Memo {
            id: r.get(0)?,
            pick_id: r.get(1)?,
            title: r.get(2)?,
            concept: r.get(3)?,
            user_summary: r.get(4)?,
            feedback: r.get(5)?,
            passed: r.get::<_, i64>(6)? != 0,
            created_at: r.get(7)?,
        })
    })?;
    rows.collect()
}

/// パス済みの概念（プロファイル成長用）。新しい順に最大 `limit` 件。
pub fn understood_concepts(conn: &Connection, limit: usize) -> Vec<String> {
    let Ok(mut stmt) =
        conn.prepare("SELECT concept FROM memos WHERE passed = 1 ORDER BY id DESC LIMIT ?1")
    else {
        return vec![];
    };
    stmt.query_map([limit as i64], |r| r.get(0))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}
