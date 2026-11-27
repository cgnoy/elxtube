use anyhow::Result;
use rusqlite::{params, Connection};

use crate::core::now_secs;

const DAY: i64 = 86_400;
const VIDEO_CACHE_TTL_SECS: i64 = 10 * 60;
const FEED_RETENTION_SECS: i64 = 30 * DAY;
const THUMB_RETENTION_SECS: i64 = 30 * DAY;
const MAX_NON_HISTORY_THUMBS: usize = 500;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PruneStats {
    pub feed_deleted: usize,
    pub video_cache_deleted: usize,
    pub thumbs_deleted: usize,
}

pub fn prune(conn: &Connection) -> Result<PruneStats> {
    let now = now_secs();
    let feed_cutoff = now - FEED_RETENTION_SECS;
    let thumb_cutoff = now - THUMB_RETENTION_SECS;
    let video_cutoff = now - VIDEO_CACHE_TTL_SECS;

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM feed_entries
         WHERE fetched_at < ?1
           AND NOT EXISTS (
             SELECT 1 FROM feed_entries fresh
             WHERE fresh.video_id=feed_entries.video_id AND fresh.fetched_at >= ?1
           )
           AND NOT EXISTS (SELECT 1 FROM watch_history h WHERE h.video_id=feed_entries.video_id)",
        params![feed_cutoff],
    )?;
    let feed_deleted = tx.execute(
        "DELETE FROM videos
         WHERE NOT EXISTS (SELECT 1 FROM feed_entries f WHERE f.video_id=videos.video_id)
           AND NOT EXISTS (SELECT 1 FROM watch_history h WHERE h.video_id=videos.video_id)
           AND NOT EXISTS (
             SELECT 1 FROM watch_session_videos w WHERE w.video_id=videos.video_id
           )",
        [],
    )?;
    let video_cache_deleted = tx.execute(
        "DELETE FROM video_cache WHERE fetched_at < ?1",
        params![video_cutoff],
    )?;
    let old_thumbs_deleted = tx.execute(
        "DELETE FROM thumb_blob
         WHERE created_at < ?1
           AND NOT EXISTS (SELECT 1 FROM history_view h WHERE h.video_id=thumb_blob.video_id)",
        params![thumb_cutoff],
    )?;
    let excess_thumbs_deleted = tx.execute(
        "DELETE FROM thumb_blob
         WHERE NOT EXISTS (SELECT 1 FROM history_view h WHERE h.video_id=thumb_blob.video_id)
           AND video_id IN (
             SELECT video_id FROM (
               SELECT t.video_id
               FROM thumb_blob t
               WHERE NOT EXISTS (SELECT 1 FROM history_view h WHERE h.video_id=t.video_id)
               ORDER BY t.created_at DESC, t.video_id
               LIMIT -1 OFFSET ?1
             )
           )",
        params![MAX_NON_HISTORY_THUMBS as i64],
    )?;
    tx.commit()?;

    Ok(PruneStats {
        feed_deleted,
        video_cache_deleted,
        thumbs_deleted: old_thumbs_deleted + excess_thumbs_deleted,
    })
}
