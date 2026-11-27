use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use crate::core::model::Video;

use super::map_video;

fn load_video(conn: &Connection, video_id: &str) -> Result<Option<Video>> {
    let feed_video = conn
        .query_row(
            "SELECT video_id, title, channel_id, channel_name, duration,
                    thumbnail_url, published_at
             FROM feed_cache_view WHERE video_id = ?1",
            params![video_id],
            map_video,
        )
        .optional()?;
    if feed_video.is_some() {
        return Ok(feed_video);
    }

    conn.query_row(
        "SELECT video_id, title, channel_id, channel_name, duration,
                thumbnail_url, NULL AS published_at
         FROM history_view WHERE video_id = ?1",
        params![video_id],
        map_video,
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn continue_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT h.video_id, h.title, h.channel_id, h.channel_name, h.duration,
                h.thumbnail_url, NULL AS published_at
         FROM history_view h
         WHERE h.last_position > 15.0
           AND (h.duration IS NULL OR h.duration <= 0 OR h.last_position < h.duration * 0.9)
         ORDER BY h.last_watched DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(super) fn channel_videos(
    conn: &Connection,
    channel_id: &str,
    limit: usize,
) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration,
                thumbnail_url, published_at
         FROM feed_cache_view
         WHERE channel_id = ?1
           AND NOT EXISTS (
               SELECT 1 FROM feedback f
               WHERE f.target_type='channel' AND f.target_id=feed_cache_view.channel_id
           )
         ORDER BY COALESCE(published_at, 0) DESC, fetched_at DESC, video_id
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![channel_id, limit as i64], map_video)?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub(super) fn up_next(
    conn: &Connection,
    current_id: &str,
    current_channel_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Video>> {
    let mut current = load_video(conn, current_id)?.unwrap_or_else(|| Video {
        id: current_id.to_string(),
        ..Video::default()
    });
    if current.channel_id.is_none() {
        current.channel_id = current_channel_id.map(str::to_string);
    }
    super::ranked_up_next_at(conn, &current, limit, crate::core::now_secs())
}

pub(super) fn clear_feed(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM feed_entries", [])?;
    tx.execute(
        "DELETE FROM videos
         WHERE NOT EXISTS (SELECT 1 FROM watch_history h WHERE h.video_id=videos.video_id)
           AND NOT EXISTS (
             SELECT 1 FROM watch_session_videos w WHERE w.video_id=videos.video_id
           )",
        [],
    )?;
    tx.execute("DELETE FROM thumb_blob", [])?;
    tx.commit()?;
    Ok(())
}
