use std::sync::Arc;

use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::database::Database;
use crate::core::model::Channel;

use crate::application::ports::{Clock, VideoProvider};

#[derive(Clone)]
pub struct SubscriptionService {
    database: Database,
    provider: Arc<dyn VideoProvider>,
    clock: Arc<dyn Clock>,
}

impl SubscriptionService {
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

    pub fn list(&self) -> Result<Vec<Channel>> {
        self.database.with_read_conn(crate::core::db::channel_list)
    }

    pub fn is_subscribed(&self, channel_id: &str) -> Result<bool> {
        Ok(self.list()?.iter().any(|channel| channel.id == channel_id))
    }

    pub fn subscribe(&self, channel: &Channel) -> Result<()> {
        self.database
            .with_conn(|conn| crate::core::db::channel_upsert(conn, channel))
    }

    pub fn unsubscribe(&self, channel_id: &str) -> Result<()> {
        self.database
            .with_conn(|conn| crate::core::db::channel_delete(conn, channel_id))
    }

    pub fn refresh_channel(
        &self,
        channel: &Channel,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<usize> {
        let mut videos = self.provider.fetch_channel_videos(
            &channel.id,
            Some(&channel.name),
            limit,
            cancellation,
        )?;
        for video in &mut videos {
            video.channel_id = Some(channel.id.clone());
            video.channel_name = Some(channel.name.clone());
        }
        let fetched_at = self.clock.now_secs();
        self.database.with_conn(|conn| {
            crate::core::db::feed_upsert_many_with_source(conn, &videos, fetched_at, "subscription")
        })?;
        Ok(videos.len())
    }

    pub fn refresh(
        &self,
        limit_per_channel: u32,
        cancellation: &CancellationToken,
    ) -> Result<(usize, usize)> {
        const MAX_PARALLEL_CHANNELS: usize = 3;
        let channels = self.list()?;
        let fetched_at = self.clock.now_secs();
        let mut count = 0;
        let mut failed = 0;
        for chunk in channels.chunks(MAX_PARALLEL_CHANNELS) {
            let provider = self.provider.clone();
            let results = std::thread::scope(|scope| {
                let handles = chunk
                    .iter()
                    .map(|channel| {
                        let provider = provider.clone();
                        let cancellation = cancellation.clone();
                        scope.spawn(move || {
                            provider.fetch_channel_videos(
                                &channel.id,
                                Some(&channel.name),
                                limit_per_channel,
                                &cancellation,
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                handles
                    .into_iter()
                    .map(|handle| {
                        handle
                            .join()
                            .unwrap_or_else(|_| Err(anyhow::anyhow!("worker channel gagal")))
                    })
                    .collect::<Vec<_>>()
            });
            for (channel, result) in chunk.iter().zip(results) {
                match result {
                    Ok(mut videos) => {
                        for video in &mut videos {
                            video.channel_id = Some(channel.id.clone());
                            video.channel_name = Some(channel.name.clone());
                        }
                        let inserted = self.database.with_conn(|conn| {
                            crate::core::db::feed_upsert_many_with_source(
                                conn,
                                &videos,
                                fetched_at,
                                "subscription",
                            )
                        });
                        if inserted.is_ok() {
                            count += videos.len();
                        } else {
                            failed += 1;
                        }
                    }
                    Err(_) => failed += 1,
                }
            }
        }
        self.database.with_conn(crate::core::db::prune)?;
        Ok((count, failed))
    }
}
