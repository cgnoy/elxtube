use crate::core::model::Video;
use gtk::gdk;

#[derive(Clone)]
pub struct VideoItem {
    pub video: Video,
    pub texture: gdk::Texture,
    pub progress: Option<f64>,
}

impl VideoItem {
    pub fn new(video: Video, texture: gdk::Texture) -> Self {
        Self {
            video,
            texture,
            progress: None,
        }
    }

    pub fn with_progress(video: Video, texture: gdk::Texture, progress: f64) -> Self {
        Self {
            video,
            texture,
            progress: Some(progress.clamp(0.0, 1.0)),
        }
    }
}
