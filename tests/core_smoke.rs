use elxtube::core::database::Database;
use elxtube::core::db;
use elxtube::core::feed;
use elxtube::core::model::{Channel, FeedbackTarget, Video};
use elxtube::core::thumbs;
use image::codecs::jpeg::JpegEncoder;
use rusqlite::Connection;
use std::io::Cursor;

fn mk_video(id: &str, ch: &str, ts: i64) -> Video {
    Video {
        id: id.to_string(),
        title: format!("Video {}", id),
        channel_id: Some(ch.to_string()),
        channel_name: Some(format!("Ch {}", ch)),
        duration: Some(300),
        thumbnail_url: Some(format!("https://example.com/{}.jpg", id)),
        published_at: Some(ts),
    }
}

#[test]
fn schema_and_channels() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    db::channel_upsert(
        &conn,
        &Channel {
            id: "UCa".into(),
            name: "Channel A".into(),
            avatar_url: None,
        },
    )?;
    db::channel_upsert(
        &conn,
        &Channel {
            id: "UCb".into(),
            name: "Channel B".into(),
            avatar_url: None,
        },
    )?;
    let list = db::channel_list(&conn)?;
    assert_eq!(list.len(), 2);
    db::channel_delete(&conn, "UCa")?;
    assert_eq!(db::channel_list(&conn)?.len(), 1);
    Ok(())
}

#[test]
fn feed_home_and_upnext() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    db::channel_upsert(
        &conn,
        &Channel {
            id: "UCa".into(),
            name: "Channel A".into(),
            avatar_url: None,
        },
    )?;
    let now = elxtube::core::now_secs();

    // 20 feed entries, all fresh (< 7 days)
    for i in 0..20 {
        let v = mk_video(&format!("v{}", i), "UCa", now - i as i64 * 3600);
        db::feed_upsert(&conn, &v, now)?;
    }

    let home = feed::home_feed(&conn, 10)?;
    assert!(!home.is_empty(), "home feed kosong");
    assert!(home.len() <= 10);

    // Mark v0 as watched, mid-way → should not appear in "new", but may appear in "continue"
    db::history_upsert(&conn, &mk_video("v0", "UCa", now), 120.0)?;

    let up = feed::up_next(&conn, "v5", Some("UCa"), 12)?;
    assert!(!up.iter().any(|v| v.id == "v5"), "up_next memuat current");
    assert!(up.len() <= 12);
    Ok(())
}

#[test]
fn similar_up_next_ranks_matching_titles_first() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let now = elxtube::core::now_secs();
    let mut current = mk_video("current", "UCa", now);
    current.title = "Tutorial Rust Async".into();

    let mut matching = mk_video("matching", "UCb", now - 10);
    matching.title = "Rust Async Ownership Explained".into();
    let mut same_channel = mk_video("same-channel", "UCa", now - 20);
    same_channel.title = "Unrelated Cooking Recipe".into();
    let mut fallback = mk_video("fallback", "UCc", now - 30);
    fallback.title = "Recent Technology Video".into();

    db::feed_upsert(&conn, &matching, now)?;
    db::feed_upsert(&conn, &same_channel, now)?;
    db::feed_upsert(&conn, &fallback, now)?;
    db::history_upsert(&conn, &matching, 10.0)?;

    let recommendations = feed::similar_up_next(&conn, &current, 3)?;
    assert_eq!(recommendations[0].id, "matching");
    assert_eq!(
        recommendations
            .iter()
            .filter(|video| video.id == "matching")
            .count(),
        1
    );
    assert!(!recommendations.iter().any(|video| video.id == "current"));
    Ok(())
}

#[test]
fn feedback_excludes_video_and_channel_from_feed() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let now = elxtube::core::now_secs();
    let video = mk_video("feedback-video", "UCfeedback", now);
    db::feed_upsert_with_source(&conn, &video, now, "subscription")?;
    assert!(feed::home_feed(&conn, 10)?
        .iter()
        .any(|candidate| candidate.id == "feedback-video"));

    db::feedback_add(&conn, &FeedbackTarget::Video("feedback-video".into()))?;
    assert!(!feed::home_feed(&conn, 10)?
        .iter()
        .any(|candidate| candidate.id == "feedback-video"));

    let second = mk_video("feedback-channel-video", "UCfeedback", now);
    db::feed_upsert_with_source(&conn, &second, now, "subscription")?;
    db::feedback_add(&conn, &FeedbackTarget::Channel("UCfeedback".into()))?;
    assert!(!feed::home_feed(&conn, 10)?
        .iter()
        .any(|candidate| { candidate.channel_id.as_deref() == Some("UCfeedback") }));
    assert!(feed::channel_videos(&conn, "UCfeedback", 10)?.is_empty());
    Ok(())
}

#[test]
fn watch_time_and_completion_strengthen_channel_affinity() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let now = elxtube::core::now_secs();
    let watched = mk_video("quality-watch", "UCwatched", now);
    let lightly_watched = mk_video("quality-light", "UClight", now);
    db::history_upsert(&conn, &watched, 120.0)?;
    db::history_upsert(&conn, &lightly_watched, 20.0)?;
    db::watch_progress(
        &conn,
        "quality-session",
        &watched,
        120.0,
        Some(120),
        3_600.0,
        now,
    )?;
    db::watch_progress(
        &conn,
        "quality-session",
        &lightly_watched,
        20.0,
        Some(300),
        20.0,
        now,
    )?;

    let candidate_watched = mk_video("candidate-watched", "UCwatched", now);
    let candidate_light = mk_video("candidate-light", "UClight", now);
    db::feed_upsert_with_source(&conn, &candidate_watched, now, "subscription")?;
    db::feed_upsert_with_source(&conn, &candidate_light, now, "subscription")?;

    let ranked = feed::home_feed(&conn, 10)?;
    let watched_position = ranked
        .iter()
        .position(|video| video.id == "candidate-watched")
        .expect("watched channel candidate");
    let light_position = ranked
        .iter()
        .position(|video| video.id == "candidate-light")
        .expect("light channel candidate");
    assert!(watched_position < light_position);
    Ok(())
}

#[test]
fn continue_feed_excludes_early_and_completed_videos() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;

    for (id, position) in [
        ("older", 60.0),
        ("newer", 120.0),
        ("early", 15.0),
        ("completed", 270.0),
    ] {
        db::history_upsert(&conn, &mk_video(id, "UCa", 0), position)?;
    }
    conn.execute(
        "UPDATE watch_history SET last_watched = 100 WHERE video_id = 'older'",
        [],
    )?;
    conn.execute(
        "UPDATE watch_history SET last_watched = 200 WHERE video_id = 'newer'",
        [],
    )?;

    let videos = feed::continue_feed(&conn, 10)?;
    let ids: Vec<_> = videos.iter().map(|video| video.id.as_str()).collect();
    assert_eq!(ids, ["newer", "older"]);
    Ok(())
}

#[test]
fn local_feeds_are_scoped_and_limited() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    db::channel_upsert(
        &conn,
        &Channel {
            id: "UCsubscribed".into(),
            name: "Subscribed".into(),
            avatar_url: None,
        },
    )?;
    let now = elxtube::core::now_secs();

    for index in 0..4 {
        db::feed_upsert(
            &conn,
            &mk_video(&format!("subs{index}"), "UCsubscribed", now + index),
            now,
        )?;
    }
    db::feed_upsert(&conn, &mk_video("other", "UCother", now), now)?;
    db::feed_upsert(&conn, &mk_video("discover", "DISCOVER", now), now)?;

    let subscribed = feed::subs_feed(&conn, 2)?;
    assert_eq!(subscribed.len(), 2);
    assert!(subscribed
        .iter()
        .all(|video| video.channel_id.as_deref() == Some("UCsubscribed")));

    let discovered = feed::discover_feed(&conn, 30)?;
    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].id, "discover");

    let channel = feed::channel_videos(&conn, "UCsubscribed", 3)?;
    assert_eq!(channel.len(), 3);
    assert!(channel
        .iter()
        .all(|video| video.channel_id.as_deref() == Some("UCsubscribed")));
    Ok(())
}

#[test]
fn history_list_preserves_progress_and_recency() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    db::history_upsert(&conn, &mk_video("old", "UCa", 0), 30.0)?;
    db::history_upsert(&conn, &mk_video("new", "UCa", 0), 90.0)?;
    conn.execute(
        "UPDATE watch_history SET last_watched = 100 WHERE video_id = 'old'",
        [],
    )?;
    conn.execute(
        "UPDATE watch_history SET last_watched = 200 WHERE video_id = 'new'",
        [],
    )?;

    let history = db::history_list(&conn, 30)?;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].video.id, "new");
    assert_eq!(history[0].last_position, 90.0);
    assert_eq!(history[0].video.duration, Some(300));
    assert_eq!(history[1].video.id, "old");
    assert_eq!(history[1].last_position, 30.0);
    Ok(())
}

#[test]
fn history_resume() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let v = mk_video("vX", "UCa", elxtube::core::now_secs());
    db::history_upsert(&conn, &v, 42.5)?;
    let pos = db::history_position(&conn, "vX")?;
    assert_eq!(pos, Some(42.5));
    db::history_clear(&conn)?;
    assert!(db::history_list(&conn, 10)?.is_empty());
    Ok(())
}

#[test]
fn thumb_blob_and_settings() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    db::thumb_blob_put(&conn, "vA", &[0xFF, 0xD8, 0x00, 0x01])?;
    db::thumb_blob_put(&conn, "vB", &[0xFF, 0xD8, 0x00, 0x02])?;
    let a = db::thumb_blob_get(&conn, "vA")?.expect("vA blob");
    assert_eq!(a, vec![0xFF, 0xD8, 0x00, 0x01]);
    db::thumb_blob_keep_only(&conn, &["vA".to_string()])?;
    assert!(db::thumb_blob_get(&conn, "vB")?.is_none());

    assert!(db::settings_get(&conn, "quality")?.is_none());
    db::settings_set(&conn, "quality", "720p")?;
    assert_eq!(db::settings_get(&conn, "quality")?.as_deref(), Some("720p"));
    db::settings_set(&conn, "theme", "dark")?;
    db::settings_set(&conn, "theme", "light")?;
    assert_eq!(db::settings_get(&conn, "theme")?.as_deref(), Some("light"));
    Ok(())
}

#[test]
fn batch_upserts_and_prune_preserve_history() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let now = elxtube::core::now_secs();
    let videos = vec![
        mk_video("batch1", "UCa", now),
        mk_video("batch2", "UCa", now),
    ];
    assert_eq!(db::feed_upsert_many(&conn, &videos, now)?, 2);
    assert_eq!(feed::channel_videos(&conn, "UCa", 10)?.len(), 2);

    db::thumb_blob_put_many(
        &conn,
        &[
            ("batch1", &[0xFF, 0xD8, 0x00][..]),
            ("batch2", &[0xFF, 0xD8, 0x01][..]),
        ],
    )?;
    db::history_upsert(&conn, &videos[0], 30.0)?;
    conn.execute(
        "UPDATE feed_entries SET fetched_at = 1 WHERE video_id IN ('batch1', 'batch2')",
        [],
    )?;
    conn.execute(
        "UPDATE thumb_blob SET created_at = 1 WHERE video_id IN ('batch1', 'batch2')",
        [],
    )?;
    db::video_cache_put(&conn, "cached", "{}")?;
    conn.execute(
        "UPDATE video_cache SET fetched_at = 1 WHERE video_id = 'cached'",
        [],
    )?;

    let stats = db::prune(&conn)?;
    assert_eq!(stats.feed_deleted, 1);
    assert_eq!(stats.video_cache_deleted, 1);
    assert_eq!(stats.thumbs_deleted, 1);
    assert!(feed::channel_videos(&conn, "UCa", 10)?
        .iter()
        .any(|video| video.id == "batch1"));
    assert!(db::thumb_blob_get(&conn, "batch1")?.is_some());
    assert!(db::thumb_blob_get(&conn, "batch2")?.is_none());
    Ok(())
}

#[test]
fn feed_entries_support_multiple_sources_per_video() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let now = elxtube::core::now_secs();
    let video = mk_video("shared", "UCa", now);

    db::feed_upsert_with_source(&conn, &video, now, "subscription")?;
    db::feed_upsert_with_source(&conn, &video, now + 1, "discover")?;

    let video_count: i64 = conn.query_row("SELECT COUNT(*) FROM videos", [], |row| row.get(0))?;
    let entry_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM feed_entries WHERE video_id='shared'",
        [],
        |row| row.get(0),
    )?;
    let selected_source: String = conn.query_row(
        "SELECT source FROM feed_cache WHERE video_id='shared'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(video_count, 1);
    assert_eq!(entry_count, 2);
    assert_eq!(selected_source, "discover");
    assert_eq!(feed::channel_videos(&conn, "UCa", 10)?.len(), 1);
    Ok(())
}

#[test]
fn legacy_schema_is_migrated_without_losing_feed_or_history() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!(
        "elxtube-migration-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "CREATE TABLE channels (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               avatar_url TEXT,
               subscribed_at INTEGER NOT NULL
             );
             INSERT INTO channels VALUES('UClegacy', 'Legacy Channel', NULL, 1);
             CREATE TABLE feed_cache (
               video_id TEXT PRIMARY KEY,
               channel_id TEXT,
               channel_name TEXT,
               title TEXT NOT NULL,
               duration INTEGER,
               thumbnail_url TEXT,
               published_at INTEGER,
               fetched_at INTEGER NOT NULL
             );
             INSERT INTO feed_cache VALUES(
               'legacy-video', 'UClegacy', 'Legacy Channel', 'Legacy Video',
               120, 'https://example.com/legacy.jpg', 100, 200
             );
             CREATE TABLE history (
               video_id TEXT PRIMARY KEY,
               title TEXT NOT NULL,
               channel_id TEXT,
               channel_name TEXT,
               duration INTEGER,
               last_position REAL DEFAULT 0,
               last_watched INTEGER NOT NULL,
               thumbnail_url TEXT
             );
             INSERT INTO history VALUES(
               'legacy-video', 'Legacy Video', 'UClegacy', 'Legacy Channel',
               120, 42.5, 300, 'https://example.com/legacy.jpg'
             );",
        )?;
    }

    let conn = db::open_at(&path)?;
    let version: i64 = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
        row.get(0)
    })?;
    let legacy_table_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='feed_cache'",
        [],
        |row| row.get(0),
    )?;
    let source: String = conn.query_row(
        "SELECT source FROM feed_entries WHERE video_id='legacy-video'",
        [],
        |row| row.get(0),
    )?;
    let position: f64 = conn.query_row(
        "SELECT last_position FROM watch_history WHERE video_id='legacy-video'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(version, 3);
    assert_eq!(legacy_table_count, 1);
    assert_eq!(source, "subscription");
    assert_eq!(position, 42.5);
    assert_eq!(db::history_position(&conn, "legacy-video")?, Some(42.5));
    assert_eq!(feed::channel_videos(&conn, "UClegacy", 10)?.len(), 1);
    let legacy_position: f64 = conn.query_row(
        "SELECT last_position FROM history WHERE video_id='legacy-video'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(legacy_position, 42.5);

    db::feed_upsert_with_source(
        &conn,
        &mk_video("new-video", "UClegacy", 300),
        300,
        "search",
    )?;
    let legacy_source: String = conn.query_row(
        "SELECT source FROM feed_cache WHERE video_id='new-video'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(legacy_source, "search");

    drop(conn);
    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn migration_replaces_legacy_views_with_rollback_tables() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!(
        "elxtube-view-migration-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let conn = db::open_at(&path)?;
        db::feed_upsert_with_source(
            &conn,
            &mk_video("view-video", "UCa", 100),
            100,
            "subscription",
        )?;
        db::history_upsert(&conn, &mk_video("view-video", "UCa", 100), 25.0)?;
        conn.execute_batch(
            "DROP TABLE feed_cache;
             DROP TABLE history;
             CREATE VIEW feed_cache AS
               SELECT video_id, channel_id, channel_name, title, duration,
                      thumbnail_url, published_at, source, fetched_at
               FROM feed_cache_view;
             CREATE VIEW history AS
               SELECT video_id, title, channel_id, channel_name, duration,
                      last_position, last_watched, thumbnail_url
               FROM history_view;
             DELETE FROM schema_migrations;
             INSERT INTO schema_migrations(version, applied_at) VALUES(1, 1);",
        )?;
    }

    let conn = db::open_at(&path)?;
    let version: i64 = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
        row.get(0)
    })?;
    let feed_table: String = conn.query_row(
        "SELECT type FROM sqlite_master WHERE name='feed_cache'",
        [],
        |row| row.get(0),
    )?;
    let history_table: String = conn.query_row(
        "SELECT type FROM sqlite_master WHERE name='history'",
        [],
        |row| row.get(0),
    )?;
    let position: f64 = conn.query_row(
        "SELECT last_position FROM history WHERE video_id='view-video'",
        [],
        |row| row.get(0),
    )?;

    assert_eq!(version, 3);
    assert_eq!(feed_table, "table");
    assert_eq!(history_table, "table");
    assert_eq!(position, 25.0);

    drop(conn);
    std::fs::remove_file(path)?;
    Ok(())
}

#[test]
fn database_owner_shares_one_connection_across_threads() -> anyhow::Result<()> {
    let database = Database::open_in_memory()?;
    let worker_database = database.clone();
    std::thread::spawn(move || {
        worker_database
            .with_conn(|conn| db::settings_set(conn, "worker", "done"))
            .unwrap();
    })
    .join()
    .map_err(|_| anyhow::anyhow!("database worker panicked"))?;

    let value = database.with_conn(|conn| db::settings_get(conn, "worker"))?;
    assert_eq!(value.as_deref(), Some("done"));
    Ok(())
}

#[test]
fn database_read_pool_supports_concurrent_queries() -> anyhow::Result<()> {
    let path = std::env::temp_dir().join(format!(
        "elxtube-read-pool-{}-{}.db",
        std::process::id(),
        elxtube::core::now_secs()
    ));
    let database = Database::open_at(&path)?;
    database.with_conn(|conn| db::settings_set(conn, "pool", "ready"))?;

    let mut workers = Vec::new();
    for _ in 0..8 {
        let worker = database.clone();
        workers.push(std::thread::spawn(move || {
            worker.with_read_conn(|conn| db::settings_get(conn, "pool"))
        }));
    }
    for worker in workers {
        let value = worker
            .join()
            .map_err(|_| anyhow::anyhow!("database reader panicked"))??;
        assert_eq!(value.as_deref(), Some("ready"));
    }

    drop(database);
    std::fs::remove_file(&path)?;
    Ok(())
}

#[test]
fn watch_progress_updates_history_and_stats_atomically() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let video = mk_video("atomic", "UCa", elxtube::core::now_secs());

    db::watch_progress(&conn, "session", &video, 280.0, Some(300), 30.0, 100)?;

    assert_eq!(db::history_position(&conn, "atomic")?, Some(280.0));
    let stats = db::watch_stats_list(&conn)?;
    assert_eq!(
        stats.get("atomic").map(|stats| stats.total_watched),
        Some(30.0)
    );
    assert_eq!(
        stats.get("atomic").map(|stats| stats.completed_count),
        Some(1)
    );
    Ok(())
}

#[test]
fn thumbnail_resize_uses_requested_dimensions() -> anyhow::Result<()> {
    let source = image::RgbImage::from_pixel(800, 400, image::Rgb([32, 64, 96]));
    let mut encoded = Vec::new();
    JpegEncoder::new_with_quality(Cursor::new(&mut encoded), 90).encode_image(&source)?;

    let resized = thumbs::resize_to_dimensions(&encoded, 96, 96, 80)?;
    let decoded = image::load_from_memory(&resized)?;
    assert_eq!(decoded.width(), 96);
    assert_eq!(decoded.height(), 96);

    let preserved = thumbs::preserve_resolution_to_jpeg_bytes(&encoded)?;
    let decoded = image::load_from_memory(&preserved)?;
    assert_eq!(decoded.width(), 800);
    assert_eq!(decoded.height(), 400);

    let banner = thumbs::resize_to_dimensions(&encoded, 960, 180, 80)?;
    let decoded = image::load_from_memory(&banner)?;
    assert_eq!(decoded.width(), 960);
    assert_eq!(decoded.height(), 180);
    Ok(())
}
