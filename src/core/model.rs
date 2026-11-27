#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub video_count: Option<u64>,
    pub thumbnail_url: Option<String>,
}

pub fn normalize_playlist_id(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(rest) = value.split_once("list=").map(|(_, rest)| rest) {
        let id = rest
            .split(['&', '#'])
            .next()
            .map(str::trim)
            .filter(|id| !id.is_empty())?;
        return Some(id.to_string());
    }
    if ["PL", "RD", "UU", "LL", "FL", "OL", "UL", "PU", "TL"]
        .iter()
        .any(|prefix| value.starts_with(prefix))
    {
        return Some(value.to_string());
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Video {
    pub id: String,
    pub title: String,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    pub duration: Option<i64>,
    pub thumbnail_url: Option<String>,
    pub published_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub video: Video,
    pub last_position: f64,
    pub last_watched: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackAction {
    NotInterested,
    HideChannel,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FeedbackTarget {
    Video(String),
    Channel(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WatchStats {
    pub watch_count: i64,
    pub completed_count: i64,
    pub total_watched: f64,
}
