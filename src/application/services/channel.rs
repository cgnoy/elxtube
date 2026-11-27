use std::sync::Arc;

use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::database::Database;

use crate::application::ports::{ChannelInfo, Clock, VideoProvider};

#[derive(Clone)]
pub struct ChannelService {
    database: Database,
    provider: Arc<dyn VideoProvider>,
    clock: Arc<dyn Clock>,
}

impl ChannelService {
    pub fn new(
        database: Database,
        provider: Arc<dyn VideoProvider>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            database,
            provider,
            clock,
        }
    }

    pub fn load(
        &self,
        channel_id: &str,
        channel_name: Option<&str>,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<(Option<ChannelInfo>, Vec<crate::core::model::Video>)> {
        let provider = self.provider.clone();
        let cancellation = cancellation.clone();
        let channel_id_owned = channel_id.to_string();
        let channel_name_owned = channel_name.map(str::to_string);
        let (info, videos) = std::thread::scope(|scope| {
            let meta_provider = provider.clone();
            let meta_cancellation = cancellation.clone();
            let meta_channel_id = channel_id_owned.clone();
            let meta_handle = scope.spawn(move || {
                meta_provider.fetch_channel_meta(&meta_channel_id, &meta_cancellation)
            });
            let videos_provider = provider.clone();
            let videos_cancellation = cancellation.clone();
            let videos_channel_id = channel_id_owned.clone();
            let videos_channel_name = channel_name_owned.clone();
            let videos_handle = scope.spawn(move || {
                videos_provider.fetch_channel_videos(
                    &videos_channel_id,
                    videos_channel_name.as_deref(),
                    limit,
                    &videos_cancellation,
                )
            });
            let info = meta_handle
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("worker metadata gagal")))
                .ok();
            let videos = videos_handle
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("worker video gagal")))?;
            Ok::<_, anyhow::Error>((info, videos))
        })?;
        let fetched_at = self.clock.now_secs();
        self.database.with_conn(|conn| {
            crate::core::db::feed_upsert_many_with_source(conn, &videos, fetched_at, "channel")
        })?;
        Ok((info, videos))
    }
}
