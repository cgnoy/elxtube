use std::sync::Arc;

use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::model::{Playlist, Video};
use crate::core::ytdlp::StreamQuality;

use super::ports::{ChannelInfo, Clock, ThumbnailClient, VideoProvider};

pub struct YtDlpProvider;

impl VideoProvider for YtDlpProvider {
    fn resolve_stream(
        &self,
        video_id: &str,
        quality: StreamQuality,
        cancellation: &CancellationToken,
    ) -> Result<(String, Option<String>)> {
        crate::core::ytdlp::resolve_stream(video_id, quality, cancellation)
    }

    fn fetch_metadata(&self, video_id: &str, cancellation: &CancellationToken) -> Result<Video> {
        crate::core::ytdlp::fetch_metadata(video_id, cancellation)
    }

    fn search(
        &self,
        query: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>> {
        crate::core::ytdlp::search(query, limit, cancellation)
    }

    fn fetch_channel_meta(
        &self,
        channel_id: &str,
        cancellation: &CancellationToken,
    ) -> Result<ChannelInfo> {
        crate::core::ytdlp::fetch_channel_meta(channel_id, cancellation).map(|meta| ChannelInfo {
            banner_url: meta.banner_url,
            avatar_url: meta.avatar_url,
            title: meta.title,
            follower_count: meta.follower_count,
            description: meta.description,
        })
    }

    fn fetch_channel_videos(
        &self,
        channel_id: &str,
        channel_name: Option<&str>,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>> {
        crate::core::ytdlp::fetch_channel_videos(channel_id, channel_name, limit, cancellation)
    }

    fn fetch_channel_playlists(
        &self,
        channel_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Playlist>> {
        crate::core::ytdlp::fetch_channel_playlists(channel_id, limit, cancellation)
    }

    fn fetch_playlist(
        &self,
        playlist_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<(Playlist, Vec<Video>)> {
        crate::core::ytdlp::fetch_playlist(playlist_id, limit, cancellation)
    }

    fn fetch_keywords(
        &self,
        keywords: &[&str],
        limit_per_keyword: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>> {
        crate::core::ytdlp::fetch_keywords(keywords, limit_per_keyword, cancellation)
    }
}

pub struct HttpThumbnailClient;

impl ThumbnailClient for HttpThumbnailClient {
    fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        crate::core::thumbs::fetch_bytes(url)
    }
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_secs(&self) -> i64 {
        crate::core::now_secs()
    }
}

pub fn default_provider() -> Arc<dyn VideoProvider> {
    Arc::new(YtDlpProvider)
}

pub fn default_thumbnail_client() -> Arc<dyn ThumbnailClient> {
    Arc::new(HttpThumbnailClient)
}

pub fn default_clock() -> Arc<dyn Clock> {
    Arc::new(SystemClock)
}
