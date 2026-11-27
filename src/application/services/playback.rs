use std::sync::Arc;

use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::database::Database;
use crate::core::model::Video;
use crate::core::ytdlp::StreamQuality;

use crate::application::ports::VideoProvider;

#[derive(Clone)]
pub struct PlaybackService {
    database: Database,
    provider: Arc<dyn VideoProvider>,
}

impl PlaybackService {
    pub fn new(database: Database, provider: Arc<dyn VideoProvider>) -> Self {
        Self { database, provider }
    }

    pub fn resolve_stream(
        &self,
        video_id: &str,
        quality: StreamQuality,
        cancellation: &CancellationToken,
    ) -> Result<(String, Option<String>)> {
        self.provider
            .resolve_stream(video_id, quality, cancellation)
    }

    pub fn metadata(&self, video_id: &str, cancellation: &CancellationToken) -> Result<Video> {
        let video = self.provider.fetch_metadata(video_id, cancellation)?;
        self.database.with_conn(|conn| {
            crate::core::db::feed_upsert_with_source(conn, &video, crate::core::now_secs(), "cache")
        })?;
        Ok(video)
    }
}
