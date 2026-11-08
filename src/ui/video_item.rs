use gtk::gdk;
use crate::core::model::Video;

#[derive(Clone)]
pub struct VideoItem {
    pub video: Video,
    pub texture: gdk::Texture,
}
