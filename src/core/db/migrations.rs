use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};

use crate::core::now_secs;

pub(super) fn apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );",
    )?;

    let current = conn
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get::<_, Option<i64>>(0)
        })
        .optional()?
        .flatten()
        .unwrap_or(0);

    if current < 1 {
        migrate_to_v1(conn)?;
    }
    if current < 2 {
        migrate_to_v2(conn)?;
    }
    if current < 3 {
        migrate_to_v3(conn)?;
    }

    Ok(())
}

fn migrate_to_v1(conn: &Connection) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("start schema migration 1")?;

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS channels (
           id TEXT PRIMARY KEY,
           name TEXT NOT NULL,
           avatar_url TEXT,
           subscribed_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS videos (
           video_id TEXT PRIMARY KEY,
           channel_id TEXT,
           channel_name TEXT,
           title TEXT NOT NULL,
           duration INTEGER,
           thumbnail_url TEXT,
           published_at INTEGER,
           updated_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_videos_channel ON videos(channel_id);
         CREATE INDEX IF NOT EXISTS idx_videos_published ON videos(published_at DESC);
         CREATE TABLE IF NOT EXISTS feed_entries (
           video_id TEXT NOT NULL,
           source TEXT NOT NULL,
           fetched_at INTEGER NOT NULL,
           PRIMARY KEY(video_id, source),
           FOREIGN KEY(video_id) REFERENCES videos(video_id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_feed_entries_channel ON feed_entries(source, fetched_at DESC);
         CREATE INDEX IF NOT EXISTS idx_feed_entries_video ON feed_entries(video_id);
         CREATE TABLE IF NOT EXISTS watch_history (
           video_id TEXT PRIMARY KEY,
           last_position REAL NOT NULL DEFAULT 0,
           last_watched INTEGER NOT NULL,
           FOREIGN KEY(video_id) REFERENCES videos(video_id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_watch_history_watched ON watch_history(last_watched DESC);
         CREATE TABLE IF NOT EXISTS watch_stats (
           video_id TEXT PRIMARY KEY,
           watch_count INTEGER NOT NULL DEFAULT 0,
           completed_count INTEGER NOT NULL DEFAULT 0,
           total_watched REAL NOT NULL DEFAULT 0,
           last_completed_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS watch_sessions (
           session_id TEXT PRIMARY KEY,
           started_at INTEGER NOT NULL,
           last_activity INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS watch_session_videos (
           session_id TEXT NOT NULL,
           video_id TEXT NOT NULL,
           watched_at INTEGER NOT NULL,
           watched_seconds REAL NOT NULL DEFAULT 0,
           completed INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY(session_id, video_id),
           FOREIGN KEY(session_id) REFERENCES watch_sessions(session_id) ON DELETE CASCADE,
           FOREIGN KEY(video_id) REFERENCES videos(video_id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_session_videos_video ON watch_session_videos(video_id);
         CREATE INDEX IF NOT EXISTS idx_session_videos_session ON watch_session_videos(session_id);
         CREATE TABLE IF NOT EXISTS feedback (
           target_type TEXT NOT NULL CHECK(target_type IN ('video', 'channel')),
           target_id TEXT NOT NULL,
           created_at INTEGER NOT NULL,
           PRIMARY KEY(target_type, target_id)
         );
         CREATE TABLE IF NOT EXISTS video_cache (
           video_id TEXT PRIMARY KEY,
           json TEXT NOT NULL,
           fetched_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS thumb_blob (
           video_id TEXT PRIMARY KEY,
           jpeg BLOB NOT NULL,
           created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_thumb_created ON thumb_blob(created_at DESC);
         CREATE TABLE IF NOT EXISTS settings (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );",
    )?;

    migrate_legacy_feed(&tx)?;
    migrate_legacy_history(&tx)?;
    migrate_legacy_session_videos(&tx)?;

    tx.execute(
        "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        rusqlite::params![1, now_secs()],
    )?;
    tx.commit().context("commit schema migration 1")?;
    Ok(())
}

fn migrate_to_v2(conn: &Connection) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("start schema migration 2")?;

    if view_exists(&tx, "feed_cache")? {
        tx.execute("DROP VIEW feed_cache", [])?;
    }
    if !table_exists(&tx, "feed_cache")? {
        tx.execute_batch(
            "CREATE TABLE feed_cache (
               video_id TEXT PRIMARY KEY,
               channel_id TEXT,
               channel_name TEXT,
               title TEXT NOT NULL,
               duration INTEGER,
               thumbnail_url TEXT,
               published_at INTEGER,
               source TEXT NOT NULL DEFAULT 'cache',
               fetched_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_feed_cache_channel ON feed_cache(channel_id);
             CREATE INDEX IF NOT EXISTS idx_feed_cache_fetched ON feed_cache(fetched_at);
             CREATE INDEX IF NOT EXISTS idx_feed_cache_published ON feed_cache(published_at DESC);",
        )?;
    } else if !table_has_column(&tx, "feed_cache", "source")? {
        tx.execute(
            "ALTER TABLE feed_cache ADD COLUMN source TEXT NOT NULL DEFAULT 'cache'",
            [],
        )?;
    }
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_feed_cache_channel ON feed_cache(channel_id);
         CREATE INDEX IF NOT EXISTS idx_feed_cache_fetched ON feed_cache(fetched_at);
         CREATE INDEX IF NOT EXISTS idx_feed_cache_published ON feed_cache(published_at DESC);",
    )?;

    if view_exists(&tx, "history")? {
        tx.execute("DROP VIEW history", [])?;
    }
    if !table_exists(&tx, "history")? {
        tx.execute_batch(
            "CREATE TABLE history (
               video_id TEXT PRIMARY KEY,
               title TEXT NOT NULL,
               channel_id TEXT,
               channel_name TEXT,
               duration INTEGER,
               last_position REAL DEFAULT 0,
               last_watched INTEGER NOT NULL,
               thumbnail_url TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_history_watched ON history(last_watched DESC);",
        )?;
    }
    tx.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_history_watched ON history(last_watched DESC);",
    )?;

    tx.execute_batch(
        "INSERT INTO feed_cache(
           video_id, channel_id, channel_name, title, duration,
           thumbnail_url, published_at, source, fetched_at
         )
         SELECT
           videos.video_id,
           videos.channel_id,
           videos.channel_name,
           videos.title,
           videos.duration,
           videos.thumbnail_url,
           videos.published_at,
           (
             SELECT source FROM feed_entries candidate
             WHERE candidate.video_id = videos.video_id
             ORDER BY CASE candidate.source
               WHEN 'discover' THEN 5
               WHEN 'subscription' THEN 4
               WHEN 'search' THEN 3
               WHEN 'channel' THEN 2
               ELSE 1
             END DESC, candidate.fetched_at DESC, candidate.source
             LIMIT 1
           ),
           COALESCE((
             SELECT MAX(fetched_at) FROM feed_entries candidate
             WHERE candidate.video_id = videos.video_id
           ), 0)
         FROM videos
         WHERE EXISTS (
           SELECT 1 FROM feed_entries candidate
           WHERE candidate.video_id = videos.video_id
         )
         ON CONFLICT(video_id) DO UPDATE SET
           channel_id=excluded.channel_id,
           channel_name=excluded.channel_name,
           title=excluded.title,
           duration=excluded.duration,
           thumbnail_url=excluded.thumbnail_url,
           published_at=excluded.published_at,
           source=excluded.source,
           fetched_at=excluded.fetched_at;
         INSERT INTO history(
           video_id, title, channel_id, channel_name, duration,
           last_position, last_watched, thumbnail_url
         )
         SELECT
           watch_history.video_id,
           videos.title,
           videos.channel_id,
           videos.channel_name,
           videos.duration,
           watch_history.last_position,
           watch_history.last_watched,
           videos.thumbnail_url
         FROM watch_history
         JOIN videos ON videos.video_id = watch_history.video_id
         WHERE true
         ON CONFLICT(video_id) DO UPDATE SET
           title=excluded.title,
           channel_id=excluded.channel_id,
           channel_name=excluded.channel_name,
           duration=excluded.duration,
           last_position=excluded.last_position,
           last_watched=excluded.last_watched,
           thumbnail_url=excluded.thumbnail_url;",
    )?;

    tx.execute(
        "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        rusqlite::params![2, now_secs()],
    )?;
    tx.commit().context("commit schema migration 2")?;
    Ok(())
}

fn migrate_to_v3(conn: &Connection) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("start schema migration 3")?;
    tx.execute_batch(
        "DROP VIEW IF EXISTS feed_cache_view;
         CREATE VIEW feed_cache_view AS
         WITH ranked_sources AS (
           SELECT
             feed_entries.video_id,
             feed_entries.source,
             feed_entries.fetched_at,
             ROW_NUMBER() OVER (
               PARTITION BY feed_entries.video_id
               ORDER BY CASE feed_entries.source
                 WHEN 'discover' THEN 5
                 WHEN 'subscription' THEN 4
                 WHEN 'search' THEN 3
                 WHEN 'channel' THEN 2
                 ELSE 1
               END DESC,
               feed_entries.fetched_at DESC,
               feed_entries.source
             ) AS source_rank,
             MAX(feed_entries.fetched_at) OVER (
               PARTITION BY feed_entries.video_id
             ) AS latest_fetched_at
           FROM feed_entries
         )
         SELECT
           videos.video_id,
           videos.channel_id,
           videos.channel_name,
           videos.title,
           videos.duration,
           videos.thumbnail_url,
           videos.published_at,
           ranked_sources.source,
           ranked_sources.latest_fetched_at AS fetched_at
         FROM videos
         JOIN ranked_sources ON ranked_sources.video_id = videos.video_id
         WHERE ranked_sources.source_rank = 1;
         DROP VIEW IF EXISTS history_view;
         CREATE VIEW history_view AS
         SELECT
           watch_history.video_id,
           videos.title,
           videos.channel_id,
           videos.channel_name,
           videos.duration,
           watch_history.last_position,
           watch_history.last_watched,
           videos.thumbnail_url
         FROM watch_history
         JOIN videos ON videos.video_id = watch_history.video_id;
         CREATE INDEX IF NOT EXISTS idx_feed_entries_fetched ON feed_entries(fetched_at);
         CREATE INDEX IF NOT EXISTS idx_thumb_created_video ON thumb_blob(created_at DESC, video_id);
         CREATE INDEX IF NOT EXISTS idx_videos_published_coalesced
           ON videos(COALESCE(published_at, 0) DESC);",
    )?;
    tx.execute(
        "INSERT INTO schema_migrations(version, applied_at) VALUES (?1, ?2)",
        rusqlite::params![3, now_secs()],
    )?;
    tx.commit().context("commit schema migration 3")?;
    Ok(())
}

fn migrate_legacy_feed(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "feed_cache")? {
        return Ok(());
    }
    if !table_has_column(conn, "feed_cache", "source")? {
        conn.execute(
            "ALTER TABLE feed_cache ADD COLUMN source TEXT NOT NULL DEFAULT 'cache'",
            [],
        )?;
        conn.execute(
            "UPDATE feed_cache SET source='discover'
             WHERE channel_id='DISCOVER' AND source='cache'",
            [],
        )?;
        conn.execute(
            "UPDATE feed_cache SET source='subscription'
             WHERE source='cache'
               AND channel_id IN (SELECT id FROM channels)",
            [],
        )?;
    }

    let now = now_secs();
    conn.execute_batch(&format!(
        "INSERT INTO videos(
           video_id, channel_id, channel_name, title, duration,
           thumbnail_url, published_at, updated_at
         )
         SELECT video_id, channel_id, channel_name, title, duration,
                thumbnail_url, published_at, {now}
         FROM feed_cache
         WHERE true
         ON CONFLICT(video_id) DO UPDATE SET
           channel_id=COALESCE(excluded.channel_id, videos.channel_id),
           channel_name=COALESCE(excluded.channel_name, videos.channel_name),
           title=excluded.title,
           duration=COALESCE(excluded.duration, videos.duration),
           thumbnail_url=COALESCE(excluded.thumbnail_url, videos.thumbnail_url),
           published_at=COALESCE(excluded.published_at, videos.published_at),
           updated_at=excluded.updated_at;
         INSERT INTO feed_entries(video_id, source, fetched_at)
         SELECT video_id, source, fetched_at FROM feed_cache
         WHERE true
         ON CONFLICT(video_id, source) DO UPDATE SET
           fetched_at=MAX(feed_entries.fetched_at, excluded.fetched_at);"
    ))?;
    Ok(())
}

fn migrate_legacy_history(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "history")? {
        return Ok(());
    }

    let now = now_secs();
    conn.execute_batch(&format!(
        "INSERT INTO videos(
           video_id, channel_id, channel_name, title, duration,
           thumbnail_url, published_at, updated_at
         )
         SELECT video_id, channel_id, channel_name, title, duration,
                thumbnail_url, NULL, {now}
         FROM history
         WHERE true
         ON CONFLICT(video_id) DO UPDATE SET
           channel_id=COALESCE(excluded.channel_id, videos.channel_id),
           channel_name=COALESCE(excluded.channel_name, videos.channel_name),
           title=excluded.title,
           duration=COALESCE(excluded.duration, videos.duration),
           thumbnail_url=COALESCE(excluded.thumbnail_url, videos.thumbnail_url),
           updated_at=excluded.updated_at;
         INSERT INTO watch_history(video_id, last_position, last_watched)
         SELECT video_id, last_position, last_watched FROM history
         WHERE true
         ON CONFLICT(video_id) DO UPDATE SET
           last_position=excluded.last_position,
           last_watched=MAX(watch_history.last_watched, excluded.last_watched);"
    ))?;
    Ok(())
}

fn migrate_legacy_session_videos(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "watch_session_videos")?
        || !table_has_column(conn, "watch_session_videos", "channel_id")?
    {
        return Ok(());
    }

    let now = now_secs();
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS watch_sessions (
           session_id TEXT PRIMARY KEY,
           started_at INTEGER NOT NULL,
           last_activity INTEGER NOT NULL
         );
         INSERT INTO videos(
           video_id, channel_id, channel_name, title, duration,
           thumbnail_url, published_at, updated_at
         )
         SELECT video_id, channel_id, NULL, title, NULL, NULL, NULL, {now}
         FROM watch_session_videos
         WHERE true
         ON CONFLICT(video_id) DO UPDATE SET
           channel_id=COALESCE(excluded.channel_id, videos.channel_id),
           title=excluded.title,
           updated_at=excluded.updated_at;
         ALTER TABLE watch_session_videos RENAME TO watch_session_videos_legacy;
         CREATE TABLE watch_session_videos (
           session_id TEXT NOT NULL,
           video_id TEXT NOT NULL,
           watched_at INTEGER NOT NULL,
           watched_seconds REAL NOT NULL DEFAULT 0,
           completed INTEGER NOT NULL DEFAULT 0,
           PRIMARY KEY(session_id, video_id),
           FOREIGN KEY(session_id) REFERENCES watch_sessions(session_id) ON DELETE CASCADE,
           FOREIGN KEY(video_id) REFERENCES videos(video_id) ON DELETE CASCADE
         );
         INSERT INTO watch_session_videos(
            session_id, video_id, watched_at, watched_seconds, completed
         )
         SELECT session_id, video_id, watched_at, watched_seconds, completed
         FROM watch_session_videos_legacy;
         DROP TABLE watch_session_videos_legacy;"
    ))?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false);
    Ok(exists)
}

fn view_exists(conn: &Connection, view: &str) -> Result<bool> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='view' AND name=?1)",
        [view],
        |row| row.get::<_, bool>(0),
    )?;
    Ok(exists)
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}
