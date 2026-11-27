use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::model::{Playlist, Video};
pub use crate::core::ytdlp::StreamQuality;

#[derive(Debug, Clone, Default)]
pub struct ChannelInfo {
    pub banner_url: Option<String>,
    pub avatar_url: Option<String>,
    pub title: String,
    pub follower_count: i64,
    pub description: String,
}

pub trait VideoProvider: Send + Sync {
    fn resolve_stream(
        &self,
        video_id: &str,
        quality: StreamQuality,
        cancellation: &CancellationToken,
    ) -> Result<(String, Option<String>)>;

    fn fetch_metadata(&self, video_id: &str, cancellation: &CancellationToken) -> Result<Video>;

    fn search(
        &self,
        query: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>>;

    fn fetch_channel_meta(
        &self,
        channel_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<ChannelInfo>;

    fn fetch_channel_videos(
        &self,
        channel_id: &str,
        channel_name: Option<&str>,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>>;

    fn fetch_channel_playlists(
        &self,
        channel_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Playlist>>;

    fn fetch_playlist(
        &self,
        playlist_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<(Playlist, Vec<Video>)>;

    fn fetch_keywords(
        &self,
        keywords: &[&str],
        limit_per_keyword: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>>;
}

pub trait ThumbnailClient: Send + Sync {
    fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>>;
}

pub trait Clock: Send + Sync {
    fn now_secs(&self) -> i64;
}
