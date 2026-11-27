use anyhow::Result;
use rusqlite::{params, Connection};

use crate::core::model::Video;
use crate::core::now_secs;

pub(super) const LARGE_BATCH_SIZE: usize = 50;

fn normalize_feed_source(source: &str) -> &'static str {
    match source {
        "discover" => "discover",
        "subscription" => "subscription",
        "search" => "search",
        "channel" => "channel",
        _ => "cache",
    }
}

const VIDEO_UPSERT_SQL: &str = "INSERT INTO videos(
         video_id, channel_id, channel_name, title, duration,
         thumbnail_url, published_at, updated_at
       )
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
       ON CONFLICT(video_id) DO UPDATE SET
         channel_id=CASE
           WHEN videos.channel_id IS NULL OR videos.channel_id='DISCOVER'
             THEN excluded.channel_id
           ELSE videos.channel_id
         END,
         channel_name=CASE
           WHEN videos.channel_id IS NULL OR videos.channel_id='DISCOVER'
             THEN excluded.channel_name
           ELSE videos.channel_name
         END,
         title=excluded.title,
         duration=COALESCE(excluded.duration, videos.duration),
         thumbnail_url=COALESCE(excluded.thumbnail_url, videos.thumbnail_url),
         published_at=COALESCE(excluded.published_at, videos.published_at),
         updated_at=excluded.updated_at";

const FEED_ENTRY_UPSERT_SQL: &str = "INSERT INTO feed_entries(video_id, source, fetched_at)
       VALUES (?1, ?2, ?3)
       ON CONFLICT(video_id, source) DO UPDATE SET
         fetched_at=MAX(feed_entries.fetched_at, excluded.fetched_at)";

const LEGACY_FEED_UPSERT_SQL: &str = "INSERT INTO feed_cache(
         video_id, channel_id, channel_name, title, duration,
         thumbnail_url, published_at, source, fetched_at
       )
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
       ON CONFLICT(video_id) DO UPDATE SET
         channel_id=excluded.channel_id,
         channel_name=excluded.channel_name,
         title=excluded.title,
         duration=excluded.duration,
         thumbnail_url=excluded.thumbnail_url,
         published_at=excluded.published_at,
         source=excluded.source,
         fetched_at=excluded.fetched_at";

fn upsert_legacy_feed(
    conn: &Connection,
    video: &Video,
    source: &str,
    fetched_at: i64,
) -> Result<()> {
    conn.execute(
        LEGACY_FEED_UPSERT_SQL,
        params![
            video.id,
            video.channel_id,
            video.channel_name,
            video.title,
            video.duration,
            video.thumbnail_url,
            video.published_at,
            source,
            fetched_at
        ],
    )?;
    Ok(())
}

pub(super) fn upsert_video(conn: &Connection, video: &Video, now: i64) -> Result<()> {
    conn.execute(
        VIDEO_UPSERT_SQL,
        params![
            video.id,
            video.channel_id,
            video.channel_name,
            video.title,
            video.duration,
            video.thumbnail_url,
            video.published_at,
            now
        ],
    )?;
    Ok(())
}

pub fn feed_upsert(conn: &Connection, video: &Video, fetched_at: i64) -> Result<()> {
    feed_upsert_with_source(conn, video, fetched_at, "cache")
}

pub fn feed_upsert_with_source(
    conn: &Connection,
    video: &Video,
    fetched_at: i64,
    source: &str,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    let normalized_source = normalize_feed_source(source);
    upsert_video(&tx, video, fetched_at)?;
    tx.execute(
        FEED_ENTRY_UPSERT_SQL,
        params![video.id, normalized_source, fetched_at],
    )?;
    upsert_legacy_feed(&tx, video, normalized_source, fetched_at)?;
    tx.commit()?;
    Ok(())
}

pub fn feed_upsert_many(conn: &Connection, videos: &[Video], fetched_at: i64) -> Result<usize> {
    feed_upsert_many_with_source(conn, videos, fetched_at, "cache")
}

pub fn feed_upsert_many_with_source(
    conn: &Connection,
    videos: &[Video],
    fetched_at: i64,
    source: &str,
) -> Result<usize> {
    if videos.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    {
        let mut video_stmt = tx.prepare(VIDEO_UPSERT_SQL)?;
        let mut entry_stmt = tx.prepare(FEED_ENTRY_UPSERT_SQL)?;
        let normalized_source = normalize_feed_source(source);
        for video in videos {
            video_stmt.execute(params![
                video.id,
                video.channel_id,
                video.channel_name,
                video.title,
                video.duration,
                video.thumbnail_url,
                video.published_at,
                fetched_at
            ])?;
            entry_stmt.execute(params![video.id, normalized_source, fetched_at])?;
            upsert_legacy_feed(&tx, video, normalized_source, fetched_at)?;
        }
    }
    tx.commit()?;

    if videos.len() >= LARGE_BATCH_SIZE {
        super::maintenance::prune(conn)?;
    }
    Ok(videos.len())
}

pub fn feed_cleanup_older_than(conn: &Connection, age_secs: i64) -> Result<usize> {
    let cutoff = now_secs() - age_secs;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM feed_entries
         WHERE fetched_at < ?1
           AND NOT EXISTS (
             SELECT 1 FROM feed_entries fresh
             WHERE fresh.video_id=feed_entries.video_id AND fresh.fetched_at >= ?1
           )
           AND NOT EXISTS (SELECT 1 FROM watch_history h WHERE h.video_id=feed_entries.video_id)",
        params![cutoff],
    )?;
    let deleted = tx.execute(
        "DELETE FROM videos
         WHERE NOT EXISTS (SELECT 1 FROM feed_entries f WHERE f.video_id=videos.video_id)
           AND NOT EXISTS (SELECT 1 FROM watch_history h WHERE h.video_id=videos.video_id)
           AND NOT EXISTS (
             SELECT 1 FROM watch_session_videos w WHERE w.video_id=videos.video_id
           )",
        [],
    )?;
    tx.execute(
        "DELETE FROM feed_cache
         WHERE NOT EXISTS (
           SELECT 1 FROM feed_entries f WHERE f.video_id=feed_cache.video_id
         )",
        [],
    )?;
    tx.commit()?;
    Ok(deleted)
}
