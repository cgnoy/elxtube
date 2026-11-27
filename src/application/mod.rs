pub mod adapters;
pub mod ports;
pub mod services;
pub mod tasks;

use std::sync::Arc;

use crate::core::database::Database;

use adapters::{default_clock, default_provider, default_thumbnail_client};
use ports::{Clock, VideoProvider};
use services::{
    ChannelService, FeedService, PlaybackService, PlaylistService, SearchService, SettingsService,
    SubscriptionService, ThumbnailService, WatchHistoryService,
};

#[derive(Clone)]
pub struct AppContext {
    pub settings: SettingsService,
    pub feed: FeedService,
    pub thumbnail: ThumbnailService,
    pub search: SearchService,
    pub channel: ChannelService,
    pub subscription: SubscriptionService,
    pub playback: PlaybackService,
    pub playlist: PlaylistService,
    pub watch_history: WatchHistoryService,
}

impl AppContext {
    pub fn new(database: Database) -> Self {
        Self::with_provider(database, default_provider(), default_clock())
    }

    pub fn with_provider(
        database: Database,
        provider: Arc<dyn VideoProvider>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let feed = FeedService::new(database.clone());
        Self {
            settings: SettingsService::new(database.clone()),
            thumbnail: ThumbnailService::new(default_thumbnail_client(), feed.clone()),
            feed,
            search: SearchService::new(database.clone(), provider.clone(), clock.clone()),
            channel: ChannelService::new(database.clone(), provider.clone(), clock.clone()),
            subscription: SubscriptionService::new(
                database.clone(),
                provider.clone(),
                clock.clone(),
            ),
            playback: PlaybackService::new(database.clone(), provider.clone()),
            playlist: PlaylistService::new(provider.clone()),
            watch_history: WatchHistoryService::new(database.clone(), clock),
        }
    }
}
