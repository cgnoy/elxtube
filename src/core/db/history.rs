use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

use crate::core::model::{FeedbackTarget, HistoryEntry, Video, WatchStats};
use crate::core::now_secs;

use super::feed_store::upsert_video;

fn upsert_legacy_history(
    conn: &Connection,
    video: &Video,
    position: f64,
    last_watched: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO history(
           video_id, title, channel_id, channel_name, duration,
           last_position, last_watched, thumbnail_url
         )
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
            video.id,
            video.title,
            video.channel_id,
            video.channel_name,
            video.duration,
            position,
            last_watched,
            video.thumbnail_url
        ],
    )?;
    Ok(())
}

pub fn history_upsert(conn: &Connection, video: &Video, position: f64) -> Result<()> {
    let now = now_secs();
    let tx = conn.unchecked_transaction()?;
    upsert_video(&tx, video, now)?;
    tx.execute(
        "INSERT INTO watch_history(video_id, last_position, last_watched)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(video_id) DO UPDATE SET
           last_position=excluded.last_position,
           last_watched=excluded.last_watched",
        params![video.id, position, now],
    )?;
    upsert_legacy_history(&tx, video, position, now)?;
    tx.commit()?;
    Ok(())
}

pub fn history_position(conn: &Connection, video_id: &str) -> Result<Option<f64>> {
    let position = conn
        .query_row(
            "SELECT last_position FROM history_view WHERE video_id=?1",
            params![video_id],
            |row| row.get::<_, f64>(0),
        )
        .optional()?;
    Ok(position)
}

pub fn history_list(conn: &Connection, limit: usize) -> Result<Vec<HistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url,
                last_position, last_watched
         FROM history_view ORDER BY last_watched DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(HistoryEntry {
            video: Video {
                id: row.get(0)?,
                title: row.get(1)?,
                channel_id: row.get(2)?,
                channel_name: row.get(3)?,
                duration: row.get(4)?,
                thumbnail_url: row.get(5)?,
                published_at: None,
            },
            last_position: row.get(6)?,
            last_watched: row.get(7)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn history_clear(conn: &Connection) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM watch_history", [])?;
    tx.execute("DELETE FROM history", [])?;
    tx.commit()?;
    Ok(())
}

pub fn history_resume_position(conn: &Connection, video_id: &str) -> Result<Option<f64>> {
    let row = conn
        .query_row(
            "SELECT last_position, duration FROM history_view WHERE video_id=?1",
            params![video_id],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(position, duration)| {
        let resumable = position > 15.0
            && duration
                .map(|duration| duration > 0 && position < duration as f64 * 0.9)
                .unwrap_or(true);
        resumable.then_some(position)
    }))
}

pub fn feedback_add(conn: &Connection, target: &FeedbackTarget) -> Result<()> {
    let (target_type, target_id) = match target {
        FeedbackTarget::Video(id) => ("video", id),
        FeedbackTarget::Channel(id) => ("channel", id),
    };
    conn.execute(
        "INSERT INTO feedback(target_type, target_id, created_at) VALUES(?1, ?2, ?3)
         ON CONFLICT(target_type, target_id) DO UPDATE SET created_at=excluded.created_at",
        params![target_type, target_id, now_secs()],
    )?;
    Ok(())
}

pub fn feedback_targets(conn: &Connection) -> Result<HashSet<FeedbackTarget>> {
    let mut stmt = conn.prepare("SELECT target_type, target_id FROM feedback")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut targets = HashSet::new();
    for row in rows {
        let (target_type, target_id) = row?;
        match target_type.as_str() {
            "video" => {
                targets.insert(FeedbackTarget::Video(target_id));
            }
            "channel" => {
                targets.insert(FeedbackTarget::Channel(target_id));
            }
            _ => {}
        }
    }
    Ok(targets)
}

pub fn watch_session_ensure(conn: &Connection, session_id: &str, now: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO watch_sessions(session_id, started_at, last_activity) VALUES(?1, ?2, ?2)
         ON CONFLICT(session_id) DO UPDATE SET last_activity=excluded.last_activity",
        params![session_id, now],
    )?;
    Ok(())
}

pub fn watch_session_video(
    conn: &Connection,
    session_id: &str,
    video: &Video,
    now: i64,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    watch_session_ensure(&tx, session_id, now)?;
    upsert_video(&tx, video, now)?;
    let inserted = tx.execute(
        "INSERT INTO watch_session_videos(session_id, video_id, watched_at)
         VALUES (?1, ?2, ?3) ON CONFLICT(session_id, video_id) DO NOTHING",
        params![session_id, video.id, now],
    )?;
    if inserted > 0 {
        tx.execute(
            "INSERT INTO watch_stats(video_id, watch_count) VALUES (?1, 1)
             ON CONFLICT(video_id) DO UPDATE SET watch_count=watch_count + 1",
            params![video.id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn watch_progress(
    conn: &Connection,
    session_id: &str,
    video: &Video,
    position: f64,
    duration: Option<i64>,
    watched_delta: f64,
    now: i64,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    watch_session_ensure(&tx, session_id, now)?;
    upsert_video(&tx, video, now)?;
    tx.execute(
        "INSERT INTO watch_history(video_id, last_position, last_watched)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(video_id) DO UPDATE SET
           last_position=excluded.last_position,
           last_watched=excluded.last_watched",
        params![video.id, position, now],
    )?;
    upsert_legacy_history(&tx, video, position, now)?;
    tx.execute(
        "INSERT INTO watch_stats(video_id, watch_count, total_watched)
         VALUES (?1, 0, ?2)
         ON CONFLICT(video_id) DO UPDATE SET
           total_watched=watch_stats.total_watched + ?2",
        params![video.id, watched_delta.max(0.0)],
    )?;
    let completed = duration
        .filter(|duration| *duration > 0)
        .map(|duration| position >= duration as f64 * 0.9)
        .unwrap_or(false);
    let was_completed: bool = tx
        .query_row(
            "SELECT completed FROM watch_session_videos WHERE session_id=?1 AND video_id=?2",
            params![session_id, video.id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false);
    tx.execute(
        "INSERT INTO watch_session_videos(
           session_id, video_id, watched_at, watched_seconds, completed
         )
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(session_id, video_id) DO UPDATE SET
           watched_at=excluded.watched_at,
           watched_seconds=watch_session_videos.watched_seconds + excluded.watched_seconds,
           completed=MAX(watch_session_videos.completed, excluded.completed)",
        params![
            session_id,
            video.id,
            now,
            watched_delta.max(0.0),
            i32::from(completed)
        ],
    )?;
    if completed && !was_completed {
        tx.execute(
            "UPDATE watch_stats SET completed_count=completed_count + 1, last_completed_at=?2
             WHERE video_id=?1",
            params![video.id, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn watch_stats_list(conn: &Connection) -> Result<HashMap<String, WatchStats>> {
    let mut stmt = conn
        .prepare("SELECT video_id, watch_count, completed_count, total_watched FROM watch_stats")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            WatchStats {
                watch_count: row.get(1)?,
                completed_count: row.get(2)?,
                total_watched: row.get(3)?,
            },
        ))
    })?;
    let mut stats = HashMap::new();
    for row in rows {
        let (video_id, value) = row?;
        stats.insert(video_id, value);
    }
    Ok(stats)
}

pub fn co_watch_counts(conn: &Connection, seed_limit: usize) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT candidate.video_id, COUNT(DISTINCT candidate.session_id)
         FROM watch_session_videos candidate
         JOIN watch_session_videos seed ON seed.session_id=candidate.session_id
         WHERE seed.video_id IN (SELECT video_id FROM history_view ORDER BY last_watched DESC LIMIT ?1)
           AND candidate.video_id != seed.video_id
         GROUP BY candidate.video_id",
    )?;
    let rows = stmt.query_map(params![seed_limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut counts = HashMap::new();
    for row in rows {
        let (video_id, count) = row?;
        counts.insert(video_id, count);
    }
    Ok(counts)
}
