use anyhow::Result;
use std::collections::HashMap;

use crate::core::database::Database;
use crate::core::model::{FeedbackTarget, HistoryEntry, Video};

#[derive(Clone)]
pub struct FeedService {
    database: Database,
}

impl FeedService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn all(&self, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::home_feed(conn, limit))
    }

    pub fn subscriptions(&self, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::subs_feed(conn, limit))
    }

    pub fn continue_watching(&self, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::continue_feed(conn, limit))
    }

    pub fn up_next(&self, current_video_id: &str, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::up_next(conn, current_video_id, None, limit))
    }

    pub fn channel_videos(&self, channel_id: &str, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::channel_videos(conn, channel_id, limit))
    }

    pub fn similar_up_next(&self, video: &Video, limit: usize) -> Result<Vec<Video>> {
        self.database
            .with_read_conn(|conn| crate::core::feed::similar_up_next(conn, video, limit))
    }

    pub fn history(&self, limit: usize) -> Result<Vec<HistoryEntry>> {
        self.database
            .with_read_conn(|conn| crate::core::db::history_list(conn, limit))
    }

    pub fn thumbnail(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.database
            .with_read_conn(|conn| crate::core::db::thumb_blob_get(conn, key))
    }

    pub fn thumbnails(&self, keys: &[String]) -> Result<HashMap<String, Vec<u8>>> {
        self.database
            .with_read_conn(|conn| crate::core::db::thumb_blob_get_many(conn, keys))
    }

    pub fn has_thumbnail(&self, key: &str) -> Result<bool> {
        self.database
            .with_read_conn(|conn| crate::core::db::thumb_blob_exists(conn, key))
    }

    pub fn put_thumbnails<S, B>(&self, thumbnails: &[(S, B)]) -> Result<usize>
    where
        S: AsRef<str>,
        B: AsRef<[u8]>,
    {
        self.database
            .with_conn(|conn| crate::core::db::thumb_blob_put_many(conn, thumbnails))
    }

    pub fn add_feedback(&self, target: &FeedbackTarget) -> Result<()> {
        self.database
            .with_conn(|conn| crate::core::db::feedback_add(conn, target))
    }

    pub fn prune(&self) -> Result<()> {
        self.database.with_conn(crate::core::db::prune).map(|_| ())
    }

    pub fn count_source(&self, source: &str) -> Result<i64> {
        self.database.with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM feed_entries WHERE source=?1",
                [source],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
    }
}
