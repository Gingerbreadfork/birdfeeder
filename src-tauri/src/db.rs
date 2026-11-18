use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::Value;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS repos (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  description TEXT,
  is_private INTEGER NOT NULL DEFAULT 0,
  is_fork INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0,
  can_push INTEGER NOT NULL DEFAULT 0,
  language TEXT,
  stars INTEGER NOT NULL DEFAULT 0,
  forks INTEGER NOT NULL DEFAULT 0,
  watchers INTEGER NOT NULL DEFAULT 0,
  open_total INTEGER NOT NULL DEFAULT 0,
  open_issues INTEGER NOT NULL DEFAULT 0,
  open_prs INTEGER NOT NULL DEFAULT 0,
  release_count INTEGER NOT NULL DEFAULT 0,
  downloads INTEGER NOT NULL DEFAULT 0,
  pushed_at TEXT,
  updated_at TEXT,
  created_at TEXT,
  seeded_at TEXT,
  star_cursor TEXT,
  releases_full_at TEXT,
  traffic_at TEXT,
  gone INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS releases (
  repo_id TEXT NOT NULL,
  tag TEXT NOT NULL,
  name TEXT,
  url TEXT,
  published_at TEXT,
  is_prerelease INTEGER NOT NULL DEFAULT 0,
  downloads INTEGER NOT NULL DEFAULT 0,
  assets TEXT NOT NULL DEFAULT '[]',
  PRIMARY KEY (repo_id, tag)
);

CREATE TABLE IF NOT EXISTS events (
  id INTEGER PRIMARY KEY,
  key TEXT NOT NULL UNIQUE,
  kind TEXT NOT NULL,
  repo_id TEXT,
  repo TEXT,
  actor TEXT,
  actor_avatar TEXT,
  subject TEXT,
  number INTEGER,
  count INTEGER NOT NULL DEFAULT 1,
  url TEXT,
  meta TEXT,
  occurred_at TEXT NOT NULL,
  read INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS events_time ON events (occurred_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS events_kind ON events (kind, occurred_at DESC);
CREATE INDEX IF NOT EXISTS events_repo ON events (repo_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS events_unread ON events (read) WHERE read = 0;

CREATE TABLE IF NOT EXISTS repo_daily (
  repo_id TEXT NOT NULL,
  date TEXT NOT NULL,
  stars INTEGER NOT NULL,
  forks INTEGER NOT NULL,
  open_issues INTEGER NOT NULL,
  open_prs INTEGER NOT NULL,
  downloads INTEGER NOT NULL,
  PRIMARY KEY (repo_id, date)
);

CREATE TABLE IF NOT EXISTS account_daily (
  date TEXT PRIMARY KEY,
  followers INTEGER NOT NULL,
  stars INTEGER NOT NULL,
  forks INTEGER NOT NULL,
  downloads INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS followers (
  login TEXT PRIMARY KEY,
  avatar_url TEXT
);

CREATE TABLE IF NOT EXISTS traffic_daily (
  repo_id TEXT NOT NULL,
  date TEXT NOT NULL,
  views INTEGER NOT NULL DEFAULT 0,
  view_uniques INTEGER NOT NULL DEFAULT 0,
  clones INTEGER NOT NULL DEFAULT 0,
  clone_uniques INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (repo_id, date)
);

CREATE TABLE IF NOT EXISTS people (
  login TEXT PRIMARY KEY,
  name TEXT,
  company TEXT,
  followers INTEGER,
  fetched_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS contributions (
  date TEXT PRIMARY KEY,
  count INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sightings (
  id TEXT PRIMARY KEY,
  noticed_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS referrers (
  repo_id TEXT NOT NULL,
  referrer TEXT NOT NULL,
  count INTEGER NOT NULL,
  uniques INTEGER NOT NULL,
  PRIMARY KEY (repo_id, referrer)
);
";

/// Changes to the schema since the first version, in order. Only ever add to the end.
const MIGRATIONS: &[&str] = &["ALTER TABLE repos ADD COLUMN traffic_tried_at TEXT"];

/// Creates the tables and applies the changes this database has not had yet.
fn prepare(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (done, change) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        conn.execute_batch(change)?;
        conn.pragma_update(None, "user_version", done as i64 + 1)?;
    }
    Ok(())
}

pub const FILE_NAME: &str = "birdfeeder.db";

pub fn open(path: &std::path::Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    prepare(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    prepare(&conn)?;
    Ok(conn)
}

/// Forgets everything learned about the account, and keeps the settings.
pub fn forget_account(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction()?;
    for table in [
        "repos",
        "releases",
        "events",
        "repo_daily",
        "account_daily",
        "followers",
        "traffic_daily",
        "referrers",
        "people",
        "contributions",
        "sightings",
    ] {
        tx.execute(&format!("DELETE FROM {table}"), [])?;
    }
    tx.execute("DELETE FROM meta WHERE key != 'settings'", [])?;
    tx.commit()?;
    Ok(())
}

/// How long a note about the window is believed after it was written.
const RESTART_NOTE_SECONDS: i64 = 120;

/// Writes down whether the window is showing, just before a restart for an update.
pub fn note_window(conn: &Connection, visible: bool) -> Result<()> {
    let state = if visible { "shown" } else { "hidden" };
    let now = chrono::Utc::now().timestamp();
    meta_set(conn, "window_before_restart", &format!("{state} {now}"))
}

/// Whether the window was showing before a restart that has just happened.
/// The note is used once, and an old one is ignored.
pub fn take_window_note(conn: &Connection) -> Result<Option<bool>> {
    let Some(note) = meta_get(conn, "window_before_restart")? else {
        return Ok(None);
    };
    meta_delete(conn, "window_before_restart")?;
    let (state, at) = note.split_once(' ').unwrap_or((&note, "0"));
    let age = chrono::Utc::now().timestamp() - at.parse::<i64>().unwrap_or(0);
    Ok((0..=RESTART_NOTE_SECONDS)
        .contains(&age)
        .then_some(state == "shown"))
}

/// The newest events the window has celebrated and summarised, by id.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Marks {
    pub celebrated: i64,
    pub seen: i64,
}

pub fn marks(conn: &Connection) -> Result<Marks> {
    let read = |key| -> Result<i64> {
        Ok(meta_get(conn, key)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    };
    Ok(Marks {
        celebrated: read("celebrated_up_to")?,
        seen: read("seen_up_to")?,
    })
}

pub fn set_celebrated(conn: &Connection, id: i64) -> Result<()> {
    meta_set(conn, "celebrated_up_to", &id.to_string())
}

/// Counts every event so far as looked at, and returns the newest one's id.
pub fn mark_seen(conn: &Connection) -> Result<i64> {
    let newest: i64 =
        conn.query_row("SELECT COALESCE(MAX(id), 0) FROM events", [], |r| r.get(0))?;
    meta_set(conn, "seen_up_to", &newest.to_string())?;
    Ok(newest)
}

pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn meta_delete(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM meta WHERE key = ?1", [key])?;
    Ok(())
}

/// The fields of a stored repo that the sync compares against a fresh listing.
#[derive(Debug, Clone)]
pub struct StoredRepo {
    pub stars: i64,
    pub forks: i64,
    pub open_total: i64,
    pub release_count: i64,
    pub downloads: i64,
    pub pushed_at: Option<String>,
    pub updated_at: Option<String>,
    pub seeded_at: Option<String>,
    pub releases_full_at: Option<String>,
    pub traffic_at: Option<String>,
    /// When traffic was last asked for, whether or not GitHub answered.
    pub traffic_tried_at: Option<String>,
    pub gone: bool,
}

pub fn stored_repos(conn: &Connection) -> Result<std::collections::HashMap<String, StoredRepo>> {
    let mut stmt = conn.prepare(
        "SELECT id, stars, forks, open_total, release_count, downloads,
                pushed_at, updated_at, seeded_at, releases_full_at, traffic_at, gone,
                traffic_tried_at
         FROM repos",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            StoredRepo {
                stars: r.get(1)?,
                forks: r.get(2)?,
                open_total: r.get(3)?,
                release_count: r.get(4)?,
                downloads: r.get(5)?,
                pushed_at: r.get(6)?,
                updated_at: r.get(7)?,
                seeded_at: r.get(8)?,
                releases_full_at: r.get(9)?,
                traffic_at: r.get(10)?,
                gone: r.get(11)?,
                traffic_tried_at: r.get(12)?,
            },
        ))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// An event about to be recorded. `key` makes recording idempotent.
#[derive(Debug, Clone, Default)]
pub struct NewEvent {
    pub key: String,
    pub kind: &'static str,
    pub repo_id: Option<String>,
    pub repo: Option<String>,
    pub actor: Option<String>,
    pub actor_avatar: Option<String>,
    pub subject: Option<String>,
    pub number: Option<i64>,
    pub count: i64,
    pub url: Option<String>,
    pub meta: Option<Value>,
    pub occurred_at: String,
    pub read: bool,
}

/// Records an event once. Returns false when the key was already known.
pub fn insert_event(conn: &Connection, e: &NewEvent) -> Result<bool> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO events
           (key, kind, repo_id, repo, actor, actor_avatar, subject, number, count, url, meta, occurred_at, read)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            e.key,
            e.kind,
            e.repo_id,
            e.repo,
            e.actor,
            e.actor_avatar,
            e.subject,
            e.number,
            e.count.max(1),
            e.url,
            e.meta.as_ref().map(Value::to_string),
            e.occurred_at,
            e.read,
        ],
    )?;
    Ok(changed > 0)
}

/// Adds to a running tally event, creating it on first use.
/// Numeric fields of `meta` are summed into the stored meta.
pub fn bump_event(conn: &Connection, e: &NewEvent) -> Result<()> {
    let existing: Option<(i64, Option<String>)> = conn
        .query_row(
            "SELECT count, meta FROM events WHERE key = ?1",
            [&e.key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((count, stored_meta)) = existing else {
        insert_event(conn, e)?;
        return Ok(());
    };
    let mut merged = stored_meta
        .and_then(|m| serde_json::from_str::<Value>(&m).ok())
        .unwrap_or(Value::Null);
    if let Some(add) = &e.meta {
        merge_tallies(&mut merged, add);
    }
    conn.execute(
        "UPDATE events SET count = ?1, meta = ?2, occurred_at = ?3, read = ?4 WHERE key = ?5",
        params![
            count + e.count,
            (!merged.is_null()).then(|| merged.to_string()),
            e.occurred_at,
            e.read,
            e.key
        ],
    )?;
    Ok(())
}

fn merge_tallies(into: &mut Value, add: &Value) {
    match (into, add) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge_tallies(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (Value::Number(a), Value::Number(b)) => {
            *a = (a.as_i64().unwrap_or(0) + b.as_i64().unwrap_or(0)).into();
        }
        (slot, v) => *slot = v.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub id: i64,
    pub kind: String,
    pub repo_id: Option<String>,
    pub repo: Option<String>,
    pub actor: Option<String>,
    pub actor_avatar: Option<String>,
    /// How many people follow the actor, once their profile has been read.
    pub actor_followers: Option<i64>,
    pub actor_company: Option<String>,
    pub subject: Option<String>,
    pub number: Option<i64>,
    pub count: i64,
    pub url: Option<String>,
    pub meta: Option<Value>,
    pub occurred_at: String,
    pub read: bool,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct EventQuery {
    pub kinds: Vec<String>,
    pub repo_id: Option<String>,
    pub unread_only: bool,
    /// Local calendar day, YYYY-MM-DD.
    pub day: Option<String>,
    /// Only events older than this one, as its time and id.
    pub before: Option<(String, i64)>,
    pub limit: Option<u32>,
}

/// The conditions a query puts on events aliased as `e`, with their arguments.
fn filters(q: &EventQuery) -> (String, Vec<rusqlite::types::Value>) {
    let mut sql = String::new();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if !q.kinds.is_empty() {
        let marks = vec!["?"; q.kinds.len()].join(", ");
        sql.push_str(&format!(" AND e.kind IN ({marks})"));
        args.extend(q.kinds.iter().cloned().map(Into::into));
    }
    if let Some(repo_id) = &q.repo_id {
        sql.push_str(" AND e.repo_id = ?");
        args.push(repo_id.clone().into());
    }
    if q.unread_only {
        sql.push_str(" AND e.read = 0");
    }
    if let Some(day) = &q.day {
        sql.push_str(" AND date(e.occurred_at, 'localtime') = ?");
        args.push(day.clone().into());
    }
    if let Some((at, id)) = &q.before {
        sql.push_str(" AND (e.occurred_at < ? OR (e.occurred_at = ? AND e.id < ?))");
        args.extend([at.clone().into(), at.clone().into(), (*id).into()]);
    }
    (sql, args)
}

pub fn events(conn: &Connection, q: &EventQuery) -> Result<Vec<Event>> {
    let (conditions, mut args) = filters(q);
    let sql = format!(
        "SELECT e.id, e.kind, e.repo_id, COALESCE(r.name, e.repo), e.actor, e.actor_avatar,
                e.subject, e.number, e.count, e.url, e.meta, e.occurred_at, e.read,
                p.followers, p.company
         FROM events e
         LEFT JOIN repos r ON r.id = e.repo_id
         LEFT JOIN people p ON p.login = e.actor
         WHERE 1 = 1{conditions}
         ORDER BY e.occurred_at DESC, e.id DESC LIMIT ?"
    );
    args.push(i64::from(q.limit.unwrap_or(100).clamp(1, 500)).into());

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok(Event {
            id: r.get(0)?,
            kind: r.get(1)?,
            repo_id: r.get(2)?,
            repo: r.get(3)?,
            actor: r.get(4)?,
            actor_avatar: r.get(5)?,
            subject: r.get(6)?,
            number: r.get(7)?,
            count: r.get(8)?,
            url: r.get(9)?,
            meta: r
                .get::<_, Option<String>>(10)?
                .and_then(|m| serde_json::from_str(&m).ok()),
            occurred_at: r.get(11)?,
            read: r.get(12)?,
            actor_followers: r.get(13)?,
            actor_company: r.get(14)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn unread_count(conn: &Connection) -> Result<i64> {
    Ok(
        conn.query_row("SELECT COUNT(*) FROM events WHERE read = 0", [], |r| {
            r.get(0)
        })?,
    )
}

pub fn mark_read(conn: &Connection, ids: &[i64]) -> Result<()> {
    let mut stmt = conn.prepare("UPDATE events SET read = 1 WHERE id = ?1")?;
    for id in ids {
        stmt.execute([id])?;
    }
    Ok(())
}

/// Marks as read every unread event the query's filters match.
pub fn mark_matching_read(conn: &Connection, q: &EventQuery) -> Result<()> {
    let (conditions, args) = filters(q);
    conn.execute(
        &format!("UPDATE events AS e SET read = 1 WHERE e.read = 0{conditions}"),
        rusqlite::params_from_iter(args),
    )?;
    Ok(())
}
