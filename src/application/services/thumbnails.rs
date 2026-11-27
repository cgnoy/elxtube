use std::sync::Arc;

use anyhow::Result;

use super::FeedService;
use crate::application::ports::ThumbnailClient;

pub struct ThumbnailAsset {
    pub jpeg: Arc<[u8]>,
    pub rgba: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone)]
pub struct ThumbnailService {
    client: Arc<dyn ThumbnailClient>,
    feed: FeedService,
}

impl ThumbnailService {
    pub fn new(client: Arc<dyn ThumbnailClient>, feed: FeedService) -> Self {
        Self { client, feed }
    }

    pub fn fetch(&self, url: &str, dimensions: Option<(u32, u32)>) -> Result<ThumbnailAsset> {
        let raw = self.client.fetch_bytes(url)?;
        let pixels = crate::core::thumbs::decode_thumbnail(&raw, dimensions)?;
        Ok(ThumbnailAsset {
            jpeg: Arc::from(pixels.jpeg.into_boxed_slice()),
            rgba: Arc::from(pixels.rgba.into_boxed_slice()),
            width: pixels.width,
            height: pixels.height,
        })
    }

    pub fn put_thumbnails<S, B>(&self, thumbnails: &[(S, B)]) -> Result<usize>
    where
        S: AsRef<str>,
        B: AsRef<[u8]>,
    {
        self.feed.put_thumbnails(thumbnails)
    }
}
