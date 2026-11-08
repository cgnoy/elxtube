#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub avatar_url: Option<String>,
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
