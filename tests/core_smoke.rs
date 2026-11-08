use ytkw::core::db;
use ytkw::core::feed;
use ytkw::core::model::{Channel, Video};

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
    let now = ytkw::core::now_secs();

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
fn history_resume() -> anyhow::Result<()> {
    let conn = db::open_in_memory()?;
    let v = mk_video("vX", "UCa", ytkw::core::now_secs());
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
    Ok(())
}
