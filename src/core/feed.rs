use anyhow::Result;
use rusqlite::{params, Connection};
use std::collections::HashSet;

use super::model::Video;
use super::now_secs;

const DAY: i64 = 86_400;

fn map_video(r: &rusqlite::Row<'_>) -> rusqlite::Result<Video> {
    Ok(Video {
        id: r.get(0)?,
        title: r.get(1)?,
        channel_id: r.get(2)?,
        channel_name: r.get(3)?,
        duration: r.get(4)?,
        thumbnail_url: r.get(5)?,
        published_at: r.get(6)?,
    })
}

fn query_continue(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, NULL as published_at
         FROM history
         WHERE last_position > 15.0
           AND (duration IS NULL OR last_position < duration * 0.9)
         ORDER BY last_watched DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn query_new_from_subs(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let cutoff = now_secs() - 7 * DAY;
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id IN (SELECT id FROM channels)
           AND video_id NOT IN (SELECT video_id FROM history)
           AND (published_at IS NULL OR published_at >= ?1)
         ORDER BY published_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![cutoff, limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn query_random_subs(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id IN (SELECT id FROM channels)
           AND video_id NOT IN (SELECT video_id FROM history)
         ORDER BY RANDOM()
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn query_random_history(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, NULL as published_at
         FROM history ORDER BY RANDOM() LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn query_same_channel(
    conn: &Connection,
    channel_id: &str,
    exclude: &str,
    limit: usize,
) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id = ?1
           AND video_id != ?2
           AND video_id NOT IN (SELECT video_id FROM history)
         ORDER BY published_at DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![channel_id, exclude, limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn query_new_from_subs_excluding(
    conn: &Connection,
    exclude: &HashSet<String>,
    limit: usize,
) -> Result<Vec<Video>> {
    // Ambil kandidat lebih banyak dari limit, filter di memori, cap.
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id IN (SELECT id FROM channels)
           AND video_id NOT IN (SELECT video_id FROM history)
         ORDER BY published_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![(limit * 3) as i64], map_video)?;
    let mut out = Vec::with_capacity(limit);
    for r in rows {
        let v = r?;
        if !exclude.contains(&v.id) {
            out.push(v);
            if out.len() >= limit {
                break;
            }
        }
    }
    Ok(out)
}

/// Home feed: Lanjutkan + Baru dari subscription + Random subscription.
/// Dedupe by video id, cap at `limit`.
pub fn home_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut out = Vec::with_capacity(limit);
    let mut seen: HashSet<String> = HashSet::new();

    let cont = query_continue(conn, 5.min(limit))?;
    for v in cont {
        if seen.insert(v.id.clone()) && out.len() < limit {
            out.push(v);
        }
    }

    let fresh = query_new_from_subs(conn, 15)?;
    for v in fresh {
        if seen.insert(v.id.clone()) && out.len() < limit {
            out.push(v);
        }
    }

    if out.len() < limit {
        let fill = query_random_subs(conn, limit - out.len())?;
        for v in fill {
            if seen.insert(v.id.clone()) && out.len() < limit {
                out.push(v);
            }
        }
    }

    Ok(out)
}

/// Up Next untuk sidebar player. 3 same-channel + new-subs + random-history.
pub fn up_next(
    conn: &Connection,
    current_video_id: &str,
    current_channel_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Video>> {
    let mut out = Vec::with_capacity(limit);
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(current_video_id.to_string());

    if let Some(ch) = current_channel_id {
        for v in query_same_channel(conn, ch, current_video_id, 3)? {
            if seen.insert(v.id.clone()) && out.len() < limit {
                out.push(v);
            }
        }
    }

    if out.len() < limit {
        for v in query_new_from_subs_excluding(conn, &seen, limit - out.len())? {
            if seen.insert(v.id.clone()) && out.len() < limit {
                out.push(v);
            }
        }
    }

    if out.len() < limit {
        for v in query_random_history(conn, limit - out.len())? {
            if seen.insert(v.id.clone()) && out.len() < limit {
                out.push(v);
            }
        }
    }

    Ok(out)
}

/// Mengambil video dari channel yang di-subscribe
pub fn subs_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id IN (SELECT id FROM channels)
         ORDER BY published_at DESC
         LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn clear_feed(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM feed_cache", [])?;
    conn.execute("DELETE FROM thumb_blob", [])?;
    Ok(())
}

pub const DISCOVER_KEYWORDS: [&str; 50] = [
    // Musik & Audio
    "dangdut koplo", "via vallen", "nella kharisma", "lofi indonesia", "lagu viral tiktok", 
    "kpop terbaru", "sholawat", "murottal", "musik religi", "asmr indonesia",
    
    // Gaming
    "mobile legends", "free fire", "pubg mobile", "valorant", "minecraft", 
    "roblox", "genshin impact", "gameplay", "turnamen esports", "game offline",
    
    // Hiburan & Film
    "upin ipin", "film indonesia", "drakor", "sinetron", "misteri horor", 
    "drama korea", "anime", "kartun anak", "komedi standup", "prank",
    
    // Vlog & Kreator
    "vlog indonesia", "fyp tiktok", "konten kreator", "jerome polin", "nex carlos", 
    "dedy corbuzier", "raditya dika", "challenge", "reaksi", "podcast indonesia",
    
    // Berita, Edukasi & Tech
    "berita hari ini", "politik indonesia", "sepak bola indonesia", "motogp", "badminton", 
    "review hp", "tutorial youtube", "belajar coding", "cara membuat", "diy life hack"
];

// Fungsi untuk mengambil video Discover dari Database (yang sudah di-fetch)
pub fn discover_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id = 'DISCOVER'
         ORDER BY RANDOM()
         LIMIT ?1"
    )?;
    let rows = stmt.query_map(params![limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn channel_videos(conn: &Connection, channel_id: &str, limit: usize) -> Result<Vec<Video>> {
    let mut stmt = conn.prepare(
        "SELECT video_id, title, channel_id, channel_name, duration, thumbnail_url, published_at
         FROM feed_cache
         WHERE channel_id = ?
         ORDER BY published_at DESC
         LIMIT ?"
    )?;
    let rows = stmt.query_map(params![channel_id, limit as i64], map_video)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
