use std::sync::Arc;

use anyhow::Result;

use crate::core::database::Database;
use crate::core::model::Video;

use crate::application::ports::Clock;

#[derive(Clone)]
pub struct WatchHistoryService {
    database: Database,
    clock: Arc<dyn Clock>,
}

impl WatchHistoryService {
    pub fn new(database: Database, clock: Arc<dyn Clock>) -> Self {
        Self { database, clock }
    }

    pub fn resume_position(&self, video_id: &str) -> Result<Option<f64>> {
        self.database
            .with_read_conn(|conn| crate::core::db::history_resume_position(conn, video_id))
    }

    pub fn record_video(&self, session_id: &str, video: &Video) -> Result<()> {
        let now = self.clock.now_secs();
        self.database
            .with_conn(|conn| crate::core::db::watch_session_video(conn, session_id, video, now))
    }

    pub fn record_progress(
        &self,
        session_id: &str,
        video: &Video,
        position: f64,
        duration: Option<i64>,
        watched_delta: f64,
    ) -> Result<()> {
        let now = self.clock.now_secs();
        self.database.with_conn(|conn| {
            crate::core::db::watch_progress(
                conn,
                session_id,
                video,
                position,
                duration,
                watched_delta,
                now,
            )
        })
    }
}
