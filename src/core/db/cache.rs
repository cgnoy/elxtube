use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

use crate::core::now_secs;

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
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(row)
}

pub fn video_cache_cleanup(conn: &Connection, ttl_secs: i64) -> Result<usize> {
    let deleted = conn.execute(
        "DELETE FROM video_cache WHERE fetched_at < ?1",
        params![now_secs() - ttl_secs],
    )?;
    Ok(deleted)
}

pub fn thumb_blob_put(conn: &Connection, video_id: &str, jpeg: &[u8]) -> Result<()> {
    conn.execute(
        "INSERT INTO thumb_blob(video_id, jpeg, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(video_id) DO UPDATE SET jpeg=excluded.jpeg, created_at=excluded.created_at",
        params![video_id, jpeg, now_secs()],
    )?;
    Ok(())
}

pub fn thumb_blob_put_many<S, B>(conn: &Connection, thumbs: &[(S, B)]) -> Result<usize>
where
    S: AsRef<str>,
    B: AsRef<[u8]>,
{
    if thumbs.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO thumb_blob(video_id, jpeg, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(video_id) DO UPDATE SET jpeg=excluded.jpeg, created_at=excluded.created_at",
        )?;
        let created_at = now_secs();
        for (video_id, jpeg) in thumbs {
            stmt.execute(params![video_id.as_ref(), jpeg.as_ref(), created_at])?;
        }
    }
    tx.commit()?;
    Ok(thumbs.len())
}

pub fn thumb_blob_get(conn: &Connection, video_id: &str) -> Result<Option<Vec<u8>>> {
    let value = conn
        .query_row(
            "SELECT jpeg FROM thumb_blob WHERE video_id=?1",
            params![video_id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    Ok(value)
}

pub fn thumb_blob_get_many(
    conn: &Connection,
    video_ids: &[String],
) -> Result<HashMap<String, Vec<u8>>> {
    let mut result = HashMap::new();
    for chunk in video_ids.chunks(500) {
        if chunk.is_empty() {
            continue;
        }
        let placeholders = std::iter::repeat("?")
            .take(chunk.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql =
            format!("SELECT video_id, jpeg FROM thumb_blob WHERE video_id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        let params = chunk.iter().map(|id| id.as_str()).collect::<Vec<_>>();
        let rows = stmt.query_map(rusqlite::params_from_iter(params), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (video_id, bytes) = row?;
            result.insert(video_id, bytes);
        }
    }
    Ok(result)
}

pub fn thumb_blob_exists(conn: &Connection, video_id: &str) -> Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM thumb_blob WHERE video_id=?1)",
        params![video_id],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

pub fn thumb_blob_keep_only(conn: &Connection, keep_ids: &[String]) -> Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS keep_thumb_ids(video_id TEXT PRIMARY KEY);
         DELETE FROM keep_thumb_ids;",
    )?;
    {
        let mut stmt =
            conn.prepare("INSERT OR IGNORE INTO keep_thumb_ids(video_id) VALUES (?1)")?;
        for id in keep_ids {
            stmt.execute(params![id])?;
        }
    }
    conn.execute(
        "DELETE FROM thumb_blob
         WHERE NOT EXISTS (SELECT 1 FROM keep_thumb_ids k WHERE k.video_id=thumb_blob.video_id)
           AND NOT EXISTS (SELECT 1 FROM history_view h WHERE h.video_id=thumb_blob.video_id)",
        [],
    )?;
    Ok(())
}
