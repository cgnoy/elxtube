use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{
    AspectFrame, Box as GtkBox, GridView, Label, ListItem, Orientation, Overlay, Picture,
    ProgressBar, SignalListItemFactory, SingleSelection,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use super::i18n;
use super::video_item::VideoItem;
use crate::core::model::Video;

pub const GRID_COLUMNS: u32 = 3;
const CARD_ASPECT_RATIO: f64 = 16.0 / 9.0;
const CARD_MIN_WIDTH: i32 = 160;

fn card_height(width: i32) -> i32 {
    (width as f64 / CARD_ASPECT_RATIO).round() as i32
}

mod video_card {
    use super::*;

    #[derive(Default)]
    pub struct VideoCard {
        pub(super) surface: RefCell<Option<Overlay>>,
        pub(super) thumbnail: RefCell<Option<AspectFrame>>,
        pub(super) metadata: RefCell<Option<GtkBox>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for VideoCard {
        const NAME: &'static str = "ElxtubeVideoCard";
        type Type = super::VideoCard;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for VideoCard {
        fn constructed(&self) {
            self.parent_constructed();
            let surface = Overlay::new();
            surface.set_hexpand(true);
            surface.set_vexpand(true);
            surface.set_parent(self.obj().upcast_ref::<gtk::Widget>());
            *self.surface.borrow_mut() = Some(surface);
        }

        fn dispose(&self) {
            if let Some(surface) = self.surface.borrow_mut().take() {
                surface.unparent();
            }
        }
    }

    impl WidgetImpl for VideoCard {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == Orientation::Vertical {
                let width = if for_size > 0 {
                    for_size
                } else {
                    self.thumbnail
                        .borrow()
                        .as_ref()
                        .map(|thumbnail| thumbnail.measure(Orientation::Horizontal, -1).1)
                        .unwrap_or(CARD_MIN_WIDTH)
                };
                let height = card_height(width);
                return (height, height, -1, -1);
            }
            self.surface
                .borrow()
                .as_ref()
                .map(|surface| surface.measure(orientation, for_size))
                .unwrap_or((0, 0, -1, -1))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(surface) = self.surface.borrow().as_ref() {
                surface.allocate(width, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    pub struct VideoCard(ObjectSubclass<video_card::VideoCard>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl VideoCard {
    fn new() -> Self {
        glib::Object::new()
    }

    fn set_parts(&self, thumbnail: AspectFrame, metadata: GtkBox) {
        *self.imp().thumbnail.borrow_mut() = Some(thumbnail.clone());
        *self.imp().metadata.borrow_mut() = Some(metadata.clone());
        metadata.set_halign(gtk::Align::Fill);
        metadata.set_valign(gtk::Align::End);
        metadata.set_hexpand(true);
        if let Some(surface) = self.imp().surface.borrow().as_ref() {
            surface.set_child(Some(&thumbnail));
            surface.add_overlay(&metadata);
        }
    }

    fn thumbnail(&self) -> Option<AspectFrame> {
        self.imp().thumbnail.borrow().clone()
    }

    fn metadata(&self) -> Option<GtkBox> {
        self.imp().metadata.borrow().clone()
    }
}

pub fn thumbnail_frame() -> (AspectFrame, Overlay, Picture) {
    let frame = AspectFrame::new(0.0, 0.0, CARD_ASPECT_RATIO as f32, false);
    frame.set_hexpand(true);
    frame.set_vexpand(true);
    frame.set_size_request(CARD_MIN_WIDTH, card_height(CARD_MIN_WIDTH));

    let overlay = Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);

    let picture = Picture::builder()
        .content_fit(gtk::ContentFit::Fill)
        .build();
    picture.set_halign(gtk::Align::Fill);
    picture.set_valign(gtk::Align::Fill);
    picture.set_hexpand(true);
    picture.set_vexpand(true);

    overlay.set_child(Some(&picture));
    frame.set_child(Some(&overlay));
    (frame, overlay, picture)
}

pub fn build(
    _t0: Instant,
    videos: Vec<Video>,
    textures: HashMap<String, gdk::Texture>,
    on_activate: Rc<dyn Fn(String)>,
    on_channel_click: Rc<dyn Fn(String, String)>,
) -> (GridView, gio::ListStore) {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for video in videos {
        let texture = textures
            .get(&video.id)
            .cloned()
            .unwrap_or_else(fallback_texture);
        store.append(&glib::BoxedAnyObject::new(VideoItem::new(video, texture)));
    }

    let selection = SingleSelection::builder().model(&store).build();
    let grid = GridView::builder()
        .model(&selection)
        .min_columns(GRID_COLUMNS)
        .max_columns(GRID_COLUMNS)
        .build();
    grid.set_single_click_activate(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);
    grid.add_css_class("video-grid");

    let factory = SignalListItemFactory::new();
    let on_channel_setup = on_channel_click.clone();
    factory.connect_setup(move |_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let card = VideoCard::new();
        card.add_css_class("video-card");
        card.set_hexpand(true);
        card.set_halign(gtk::Align::Fill);

        let (frame, overlay, picture) = thumbnail_frame();
        frame.add_css_class("thumbnail-frame");
        overlay.add_css_class("thumbnail-wrapper");
        picture.add_css_class("thumbnail");

        let progress = ProgressBar::new();
        progress.add_css_class("thumb-progress");
        progress.set_halign(gtk::Align::Fill);
        progress.set_valign(gtk::Align::End);
        progress.set_visible(false);
        overlay.add_overlay(&progress);

        let duration = Label::new(Some(""));
        duration.add_css_class("duration-badge");
        duration.set_halign(gtk::Align::End);
        duration.set_valign(gtk::Align::Start);
        overlay.add_overlay(&duration);
        frame.set_child(Some(&overlay));

        let text_box = GtkBox::new(Orientation::Horizontal, 12);
        text_box.add_css_class("video-card-overlay-meta");
        let avatar = Label::new(Some("C"));
        avatar.add_css_class("avatar");
        avatar.set_size_request(36, 36);
        avatar.set_valign(gtk::Align::Start);
        avatar.set_cursor_from_name(Some("pointer"));
        text_box.append(&avatar);

        let channel_click = on_channel_setup.clone();
        let item_weak = item.downgrade();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |gesture, _, _, _| {
            if let Some(item) = item_weak.upgrade() {
                if let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() {
                    let data = object.borrow::<VideoItem>();
                    if let (Some(id), Some(name)) =
                        (&data.video.channel_id, &data.video.channel_name)
                    {
                        channel_click(id.clone(), name.clone());
                    }
                }
            }
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        avatar.add_controller(gesture);

        let meta = GtkBox::new(Orientation::Vertical, 4);
        meta.set_hexpand(true);
        let title = Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(2)
            .wrap(true)
            .xalign(0.0)
            .build();
        title.add_css_class("video-title");
        let channel = Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        channel.add_css_class("video-meta");
        meta.append(&title);
        meta.append(&channel);
        text_box.append(&meta);
        card.set_parts(frame, text_box);
        item.set_child(Some(&card));
    });

    factory.connect_bind(move |_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let data = object.borrow::<VideoItem>();
        let card = item.child().and_downcast::<VideoCard>().unwrap();
        let frame = card.thumbnail().unwrap();
        let overlay = frame.child().and_downcast::<Overlay>().unwrap();
        let picture = overlay.child().and_downcast::<Picture>().unwrap();
        picture.set_paintable(Some(&data.texture));

        let duration_label = overlay.last_child().and_downcast::<Label>().unwrap();
        duration_label.set_text(&data.video.duration.map(format_duration).unwrap_or_default());
        let progress_bar = duration_label
            .prev_sibling()
            .and_downcast::<ProgressBar>()
            .unwrap();
        if let Some(progress) = data.progress {
            progress_bar.set_fraction(progress);
            progress_bar.set_visible(true);
        } else {
            progress_bar.set_visible(false);
        }

        let text_box = card.metadata().unwrap();
        let avatar = text_box.first_child().and_downcast::<Label>().unwrap();
        let channel_name = data.video.channel_name.as_deref().unwrap_or("Unknown");
        avatar.set_text(
            &channel_name
                .chars()
                .next()
                .unwrap_or('C')
                .to_uppercase()
                .to_string(),
        );
        let meta = text_box.last_child().and_downcast::<GtkBox>().unwrap();
        let title = meta.first_child().and_downcast::<Label>().unwrap();
        let channel = title.next_sibling().and_downcast::<Label>().unwrap();
        title.set_text(&data.video.title);
        channel.set_text(&if let Some(progress) = data.progress {
            format!(
                "{} • {}",
                channel_name,
                i18n::format(
                    "watched",
                    &[("percent", &format!("{:.0}", progress * 100.0))],
                )
            )
        } else {
            channel_name.to_string()
        });
    });

    grid.set_factory(Some(&factory));
    let selection_activate = selection.clone();
    grid.connect_activate(move |_, position| {
        if let Some(object) = selection_activate
            .item(position)
            .and_downcast::<glib::BoxedAnyObject>()
        {
            let data = object.borrow::<VideoItem>();
            on_activate(data.video.id.clone());
        }
    });

    (grid, store)
}

pub fn update_texture(store: &gio::ListStore, video_id: &str, texture: gdk::Texture) {
    for index in 0..store.n_items() {
        let Some(object) = store.item(index).and_downcast::<glib::BoxedAnyObject>() else {
            continue;
        };
        let matches = object.borrow::<VideoItem>().video.id == video_id;
        if matches {
            let old = object.borrow::<VideoItem>().clone();
            let replacement = VideoItem {
                video: old.video,
                texture,
                progress: old.progress,
            };
            store.splice(index, 1, &[glib::BoxedAnyObject::new(replacement)]);
            break;
        }
    }
}

pub fn format_duration(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{}:{:02}", minutes, seconds)
    }
}

pub fn set_columns(grid: &GridView, columns: u32) {
    let columns = columns.clamp(1, 4);
    grid.set_min_columns(columns);
    grid.set_max_columns(columns);
}

pub fn fallback_texture() -> gdk::Texture {
    static TEXTURE: std::sync::OnceLock<gdk::Texture> = std::sync::OnceLock::new();
    TEXTURE
        .get_or_init(|| {
            let width = 320i32;
            let height = 180i32;
            let stride = width as usize * 3;
            let data = vec![0x1eu8; stride * height as usize];
            let bytes = glib::Bytes::from(&data[..]);
            gdk::MemoryTexture::new(width, height, gdk::MemoryFormat::R8g8b8, &bytes, stride)
                .upcast()
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_grid_rectangle_is_always_sixteen_by_nine() {
        assert_eq!(card_height(160), 90);
        assert_eq!(card_height(320), 180);
        assert_eq!(card_height(480), 270);
    }
}
