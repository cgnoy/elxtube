use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use super::model::{Channel, HistoryEntry, Video};
use super::paths;
use super::now_secs;

const SCHEMA: &str = include_str!("schema.sql");

pub fn open() -> Result<Connection> {
    open_at(&paths::db_path()?)
}

pub fn open_at(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("open db {}", path.display()))?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

// ---- channels ----

pub fn channel_upsert(conn: &Connection, ch: &Channel) -> Result<()> {
    conn.execute(
        "INSERT INTO channels(id, name, avatar_url, subscribed_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, avatar_url=excluded.avatar_url",
        params![ch.id, ch.name, ch.avatar_url, now_secs()],
    )?;
    Ok(())
}

pub fn channel_delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM channels WHERE id=?1", params![id])?;
    Ok(())
}

pub fn channel_list(conn: &Connection) -> Result<Vec<Channel>> {
    let mut stmt = conn.prepare("SELECT id, name, avatar_url FROM channels ORDER BY name")?;
    let rows = stmt.query_map([], |r| {
        Ok(Channel {
            id: r.get(0)?,
            name: r.get(1)?,
            avatar_url: r.get(2)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

// ---- feed ----

pub fn feed_upsert(conn: &Connection, v: &Video, fetched_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO feed_cache(video_id, channel_id, channel_name, title, duration, thumbnail_url, published_at, fetched_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(video_id) DO UPDATE SET
           channel_id=excluded.channel_id,
           channel_name=excluded.channel_name,
           title=excluded.title,
           duration=excluded.duration,
           thumbnail_url=excluded.thumbnail_url,
           published_at=excluded.published_at,
           fetched_at=excluded.fetched_at",
        params![
            v.id,
            v.channel_id,
            v.channel_name,
            v.title,
            v.duration,
            v.thumbnail_url,
            v.published_at,
            fetched_at
        ],
    )?;
    Ok(())
}

pub fn feed_cleanup_older_than(conn: &Connection, age_secs: i64) -> Result<usize> {
    let cutoff = now_secs() - age_secs;
    let n = conn.execute("DELETE FROM feed_cache WHERE fetched_at < ?1", params![cutoff])?;
    Ok(n)
}

// ---- history ----

pub fn history_upsert(conn: &Connection, v: &Video, position: f64) -> Result<()> {
    conn.execute(
        "INSERT INTO history(video_id, title, channel_id, channel_name, duration, last_position, last_watched, thumbnail_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(video_id) DO UPDATE SET
           title=excluded.title,
           channel_id=excluded.channel_id,
           channel_name=excluded.channel_name,
           duration=excluded.duration,
           last_position=excluded.last_position,
           last_watched=excluded.last_watched,
           thumbnail_url=excluded.thumbnail_url",
        params![
            v.id,
            v.title,
            v.channel_id,
            v.channel_name,
            v.duration,
            position,
            now_secs(),
            v.thumbnail_url
        ],
    )?;
    Ok(())
}

pub fn history_position(conn: &Connection, video_id: &str) -> Result<Option<f64>> {
    let v = conn
        .query_row(
            "SELECT last_position FROM history WHERE video_id=?1",
            params![video_id],
            |r| r.get::<_, f64>(0),
        )
        .optional()?;
    Ok(v)
}

pub fn history_list(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url,
                last_position, last_watched
         FROM history ORDER BY last_watched DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |r| {
        Ok(HistoryEntry {
            video: Video {
                id: r.get(0)?,
                title: r.get(1)?,
                channel_id: r.get(2)?,
                channel_name: r.get(3)?,
                duration: r.get(4)?,
                thumbnail_url: r.get(5)?,
                published_at: None,
            },
            last_position: r.get(6)?,
            last_watched: r.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn history_clear(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM history", [])?;
    Ok(())
}

// ---- video cache (JSON metadata, TTL) ----

pub fn video_cache_put(conn: &Connection, video_id: &str, json: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO video_cache(video_id, json, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(video_id) DO UPDATE SET json=excluded.json, fetched_at=excluded.fetched_at",
        params![video_id, json, now_secs()],
    )?;
    Ok(())
}

pub fn video_cache_get(conn: &Connection, video_id: &str, ttl_secs: i64) -> Result<Option<String>> {
    let row = conn
        .query_row(
            "SELECT json FROM video_cache WHERE video_id=?1 AND fetched_at > ?2",
            params![video_id, now_secs() - ttl_secs],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(row)
}

pub fn video_cache_cleanup(conn: &Connection, ttl_secs: i64) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM video_cache WHERE fetched_at < ?1",
        params![now_secs() - ttl_secs],
    )?;
    Ok(n)
}

// ---- thumbnails ----

pub fn thumb_blob_put(conn: &Connection, video_id: &str, jpeg: &[u8]) -> Result<()> {
    conn.execute(
        "INSERT INTO thumb_blob(video_id, jpeg, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(video_id) DO UPDATE SET jpeg=excluded.jpeg, created_at=excluded.created_at",
        params![video_id, jpeg, now_secs()],
    )?;
    Ok(())
}

pub fn thumb_blob_get(conn: &Connection, video_id: &str) -> Result<Option<Vec<u8>>> {
    let v = conn
        .query_row(
            "SELECT jpeg FROM thumb_blob WHERE video_id=?1",
            params![video_id],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    Ok(v)
}

pub fn thumb_blob_keep_only(conn: &Connection, keep_ids: &[String]) -> Result<()> {
    if keep_ids.is_empty() {
        conn.execute("DELETE FROM thumb_blob", [])?;
        return Ok(());
    }
    // Simple approach: delete anything not in the list.
    let placeholders = std::iter::repeat("?")
        .take(keep_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM thumb_blob WHERE video_id NOT IN ({})", placeholders);
    let params: Vec<&dyn rusqlite::ToSql> =
        keep_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    conn.execute(&sql, params.as_slice())?;
    Ok(())
}

// ---- settings ----

pub fn settings_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT value FROM settings WHERE key=?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .optional()?)
}

pub fn settings_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}
