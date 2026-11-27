use anyhow::Result;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rusqlite::{params, Connection};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use super::db;
use super::model::{FeedbackTarget, Video};
use super::now_secs;

mod queries;
mod ranking;

use ranking::{ranked_discover_at, ranked_feed_at, ranked_subscriptions_at, ranked_up_next_at};

pub const DISCOVER_KEYWORDS: [&str; 50] = [
    "dangdut koplo",
    "via vallen",
    "nella kharisma",
    "lofi indonesia",
    "lagu viral tiktok",
    "kpop terbaru",
    "sholawat",
    "murottal",
    "musik religi",
    "asmr indonesia",
    "mobile legends",
    "free fire",
    "pubg mobile",
    "valorant",
    "minecraft",
    "roblox",
    "genshin impact",
    "gameplay",
    "turnamen esports",
    "game offline",
    "upin ipin",
    "film indonesia",
    "drakor",
    "sinetron",
    "misteri horor",
    "drama korea",
    "anime",
    "kartun anak",
    "komedi standup",
    "prank",
    "vlog indonesia",
    "fyp tiktok",
    "konten kreator",
    "jerome polin",
    "nex carlos",
    "dedy corbuzier",
    "raditya dika",
    "challenge",
    "reaksi",
    "podcast indonesia",
    "berita hari ini",
    "politik indonesia",
    "sepak bola indonesia",
    "motogp",
    "badminton",
    "review hp",
    "tutorial youtube",
    "belajar coding",
    "cara membuat",
    "diy life hack",
];

pub(super) fn map_video(row: &rusqlite::Row<'_>) -> rusqlite::Result<Video> {
    Ok(Video {
        id: row.get(0)?,
        title: row.get(1)?,
        channel_id: row.get(2)?,
        channel_name: row.get(3)?,
        duration: row.get(4)?,
        thumbnail_url: row.get(5)?,
        published_at: row.get(6)?,
    })
}

pub fn home_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    ranked_feed_at(conn, limit, now_secs())
}

pub fn subs_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    ranked_subscriptions_at(conn, limit, now_secs())
}

pub fn discover_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    ranked_discover_at(conn, limit, now_secs())
}

pub fn continue_feed(conn: &Connection, limit: usize) -> Result<Vec<Video>> {
    queries::continue_feed(conn, limit)
}

pub fn channel_videos(conn: &Connection, channel_id: &str, limit: usize) -> Result<Vec<Video>> {
    queries::channel_videos(conn, channel_id, limit)
}

pub fn similar_up_next(conn: &Connection, current: &Video, limit: usize) -> Result<Vec<Video>> {
    ranked_up_next_at(conn, current, limit, now_secs())
}

pub fn up_next(
    conn: &Connection,
    current_id: &str,
    current_channel_id: Option<&str>,
    limit: usize,
) -> Result<Vec<Video>> {
    queries::up_next(conn, current_id, current_channel_id, limit)
}

pub fn clear_feed(conn: &Connection) -> Result<()> {
    queries::clear_feed(conn)
}
