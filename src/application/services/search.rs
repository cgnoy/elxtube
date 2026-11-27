use std::sync::Arc;

use anyhow::Result;

use crate::core::cancellation::CancellationToken;
use crate::core::database::Database;
use crate::core::model::Video;

use crate::application::ports::{Clock, VideoProvider};

#[derive(Clone)]
pub struct SearchService {
    database: Database,
    provider: Arc<dyn VideoProvider>,
    clock: Arc<dyn Clock>,
}

impl SearchService {
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

    pub fn search(
        &self,
        query: &str,
        limit: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>> {
        let videos = self.provider.search(query, limit, cancellation)?;
        let fetched_at = self.clock.now_secs();
        self.database.with_conn(|conn| {
            crate::core::db::feed_upsert_many_with_source(conn, &videos, fetched_at, "search")
        })?;
        Ok(videos)
    }

    pub fn discover(
        &self,
        keywords: &[&str],
        limit_per_keyword: u32,
        cancellation: &CancellationToken,
    ) -> Result<Vec<Video>> {
        let videos = self
            .provider
            .fetch_keywords(keywords, limit_per_keyword, cancellation)?;
        let fetched_at = self.clock.now_secs();
        self.database.with_conn(|conn| {
            crate::core::db::feed_upsert_many_with_source(conn, &videos, fetched_at, "discover")
        })?;
        Ok(videos)
    }
}
