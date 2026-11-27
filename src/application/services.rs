mod channel;
mod feed;
mod history;
mod playback;
mod playlist;
mod search;
mod settings;
mod subscription;
mod thumbnails;

pub use channel::ChannelService;
pub use feed::FeedService;
pub use history::WatchHistoryService;
pub use playback::PlaybackService;
pub use playlist::PlaylistService;
pub use search::SearchService;
pub use settings::SettingsService;
pub use subscription::SubscriptionService;
pub use thumbnails::{ThumbnailAsset, ThumbnailService};

pub const DISCOVER_KEYWORDS: &[&str] = &crate::core::feed::DISCOVER_KEYWORDS;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cancellation::CancellationToken;
    use crate::core::database::Database;
    use crate::core::model::{Channel, Playlist, Video};
    use crate::core::ytdlp::StreamQuality;
    use anyhow::Result;
    use std::sync::Arc;

    use super::super::ports::{ChannelInfo, Clock, VideoProvider};

    struct FakeProvider;

    impl VideoProvider for FakeProvider {
        fn resolve_stream(
            &self,
            _video_id: &str,
            _quality: StreamQuality,
            _cancellation: &CancellationToken,
        ) -> Result<(String, Option<String>)> {
            Ok(("https://example.test/video".into(), None))
        }

        fn fetch_metadata(
            &self,
            video_id: &str,
            _cancellation: &CancellationToken,
        ) -> Result<Video> {
            Ok(Video {
                id: video_id.into(),
                title: "Metadata".into(),
                ..Video::default()
            })
        }

        fn search(
            &self,
            query: &str,
            limit: u32,
            _cancellation: &CancellationToken,
        ) -> Result<Vec<Video>> {
            Ok((0..limit)
                .map(|index| Video {
                    id: format!("{query}-{index}"),
                    title: query.into(),
                    ..Video::default()
                })
                .collect())
        }

        fn fetch_channel_meta(
            &self,
            channel_id: &str,
            _cancellation: &CancellationToken,
        ) -> Result<ChannelInfo> {
            Ok(ChannelInfo {
                title: channel_id.into(),
                ..ChannelInfo::default()
            })
        }

        fn fetch_channel_videos(
            &self,
            channel_id: &str,
            _channel_name: Option<&str>,
            limit: u32,
            _cancellation: &CancellationToken,
        ) -> Result<Vec<Video>> {
            Ok((0..limit)
                .map(|index| Video {
                    id: format!("{channel_id}-{index}"),
                    title: "Channel video".into(),
                    channel_id: Some(channel_id.into()),
                    ..Video::default()
                })
                .collect())
        }

        fn fetch_channel_playlists(
            &self,
            channel_id: &str,
            limit: u32,
            _cancellation: &CancellationToken,
        ) -> Result<Vec<Playlist>> {
            Ok((0..limit)
                .map(|index| Playlist {
                    id: format!("{channel_id}-playlist-{index}"),
                    title: "Playlist".into(),
                    channel_id: Some(channel_id.into()),
                    ..Playlist::default()
                })
                .collect())
        }

        fn fetch_playlist(
            &self,
            playlist_id: &str,
            limit: u32,
            _cancellation: &CancellationToken,
        ) -> Result<(Playlist, Vec<Video>)> {
            Ok((
                Playlist {
                    id: playlist_id.into(),
                    title: "Playlist".into(),
                    ..Playlist::default()
                },
                (0..limit)
                    .map(|index| Video {
                        id: format!("{playlist_id}-{index}"),
                        title: "Playlist video".into(),
                        ..Video::default()
                    })
                    .collect(),
            ))
        }

        fn fetch_keywords(
            &self,
            _keywords: &[&str],
            _limit_per_keyword: u32,
            _cancellation: &CancellationToken,
        ) -> Result<Vec<Video>> {
            Ok(vec![Video {
                id: "discover".into(),
                title: "Discover".into(),
                ..Video::default()
            }])
        }
    }

    struct FakeClock;

    impl Clock for FakeClock {
        fn now_secs(&self) -> i64 {
            100
        }
    }

    #[test]
    fn search_persists_provider_results() {
        let database = Database::open_in_memory().unwrap();
        let feed = FeedService::new(database.clone());
        let service = SearchService::new(database, Arc::new(FakeProvider), Arc::new(FakeClock));
        let videos = service
            .search("rust", 2, &CancellationToken::default())
            .unwrap();
        assert_eq!(videos.len(), 2);
        assert_eq!(feed.count_source("search").unwrap(), 2);
    }

    #[test]
    fn subscription_service_manages_channels() {
        let database = Database::open_in_memory().unwrap();
        let service =
            SubscriptionService::new(database, Arc::new(FakeProvider), Arc::new(FakeClock));
        let channel = Channel {
            id: "channel-1".into(),
            name: "Channel".into(),
            avatar_url: None,
        };
        service.subscribe(&channel).unwrap();
        assert!(service.is_subscribed("channel-1").unwrap());
        service.unsubscribe("channel-1").unwrap();
        assert!(!service.is_subscribed("channel-1").unwrap());
    }
}
