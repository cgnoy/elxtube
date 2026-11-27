use std::sync::Arc;

use anyhow::Result;

use crate::application::ports::VideoProvider;
use crate::core::cancellation::CancellationToken;
use crate::core::model::{Playlist, Video};

#[derive(Clone)]
pub struct PlaylistService {
    provider: Arc<dyn VideoProvider>,
}

impl PlaylistService {
    pub fn new(provider: Arc<dyn VideoProvider>) -> Self {
        Self { provider }
    }

    pub fn normalize_id(&self, value: &str) -> Option<String> {
        crate::core::model::normalize_playlist_id(value)
    }

    pub fn channel_playlists(
        &self,
        channel_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Playlist>> {
        self.provider
            .fetch_channel_playlists(channel_id, limit, cancellation)
    }

    pub fn load(
        &self,
        playlist_id: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<(Playlist, Vec<Video>)> {
        self.provider
            .fetch_playlist(playlist_id, limit, cancellation)
    }
}
