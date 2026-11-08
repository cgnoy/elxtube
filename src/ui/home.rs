use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, GridView, Label, ListItem, Orientation, Overlay, Picture,
    SignalListItemFactory, SingleSelection,
};

use crate::core::model::Video;
use super::video_item::VideoItem;

pub fn build<F: Fn(String) + 'static>(
    _t0: Instant,
    videos: Vec<Video>,
    textures: HashMap<String, gdk::Texture>,
    on_activate: F,
    on_channel_click: Rc<dyn Fn(String, String)>,
) -> (GridView, gio::ListStore) {
    
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for v in videos {
        let tex = textures.get(&v.id).cloned().unwrap_or_else(fallback_texture);
        store.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
    }

    let selection = SingleSelection::builder().model(&store).build();
    let grid = GridView::builder()
        .model(&selection)
        .min_columns(2)
        .max_columns(4)
        .build();
    grid.set_single_click_activate(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);

    let factory = SignalListItemFactory::new();
    let on_channel_click_setup = on_channel_click.clone();
    factory.connect_setup(move |_, list_item| {
        let li = list_item.downcast_ref::<ListItem>().unwrap();
        
        // KUNCI 1: Card tidak boleh melar
        let card = GtkBox::new(Orientation::Vertical, 0);
        card.add_css_class("video-card");
        card.set_size_request(480, -1);
        card.set_hexpand(false);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Start);

        // KUNCI 2: Overlay ukuran pas
        let overlay = Overlay::new();
        overlay.add_css_class("thumbnail-wrapper");
        overlay.set_size_request(480, 270);
        overlay.set_hexpand(false);
        overlay.set_vexpand(false);
        overlay.set_vexpand(false);

        // KUNCI 3: Picture mengikuti Overlay
        let picture = Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .build();
        picture.add_css_class("thumbnail");
        picture.set_hexpand(false);
        picture.set_vexpand(false);
        picture.set_vexpand(false);
        overlay.set_child(Some(&picture));

        let dur_lbl = Label::new(Some(""));
        dur_lbl.add_css_class("duration-badge");
        dur_lbl.set_halign(gtk::Align::End);
        dur_lbl.set_valign(gtk::Align::End);
        overlay.add_overlay(&dur_lbl);

        card.append(&overlay);

        let text_box = GtkBox::new(Orientation::Horizontal, 12);
        text_box.set_margin_top(12);

        let avatar = Label::new(Some("C"));
        avatar.add_css_class("avatar");
        avatar.set_size_request(36, 36);
        avatar.set_valign(gtk::Align::Start);
        avatar.set_halign(gtk::Align::Start);
        avatar.set_cursor_from_name(Some("pointer"));
        text_box.append(&avatar);
        let ch_click_setup = on_channel_click_setup.clone();
        let li_weak = li.downgrade();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        gesture.connect_released(move |g, _, _, _| {
            if let Some(li) = li_weak.upgrade() {
                if let Some(obj) = li.item().and_downcast::<glib::BoxedAnyObject>() {
                    let item = obj.borrow::<VideoItem>();
                    if let (Some(id), Some(name)) = (&item.video.channel_id, &item.video.channel_name) {
                        ch_click_setup(id.clone(), name.clone());
                    }
                }
            }
            g.set_state(gtk::EventSequenceState::Claimed);
        });
        avatar.add_controller(gesture);

        let meta_box = GtkBox::new(Orientation::Vertical, 4);
        meta_box.set_hexpand(true);

        let title = Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(2)
            .wrap(true)
            .xalign(0.0)
            .max_width_chars(30)
            .build();
        title.add_css_class("video-title");

        let channel = Label::builder().xalign(0.0).build();
        channel.add_css_class("video-meta");

        meta_box.append(&title);
        meta_box.append(&channel);
        text_box.append(&meta_box);
        card.append(&text_box);
        li.set_child(Some(&card));
    });

    factory.connect_bind(move |_, list_item| {
        let li = list_item.downcast_ref::<ListItem>().unwrap();
        let obj = match li.item().and_downcast::<glib::BoxedAnyObject>() {
            Some(o) => o,
            None => return,
        };
        let item = obj.borrow::<VideoItem>();

        let card = li.child().and_downcast::<GtkBox>().unwrap();
        let overlay = card.first_child().and_downcast::<Overlay>().unwrap();
        let picture = overlay.child().and_downcast::<Picture>().unwrap();
        picture.set_paintable(Some(&item.texture));

        let dur_lbl = overlay.last_child().and_downcast::<Label>().unwrap();
        let dur_str = if let Some(d) = item.video.duration {
            format!("{}:{:02}", d / 60, d % 60)
        } else { "".to_string() };
        dur_lbl.set_text(&dur_str);

        let text_box = card.last_child().and_downcast::<GtkBox>().unwrap();
        let meta_box = text_box.last_child().and_downcast::<GtkBox>().unwrap();
        let title = meta_box.first_child().and_downcast::<Label>().unwrap();
        let channel = title.next_sibling().and_downcast::<Label>().unwrap();

        title.set_text(&item.video.title);
        let ch_name = item.video.channel_name.clone().unwrap_or_else(|| "Unknown".into());
        channel.set_text(&format!("{} • 1 jt x ditonton", ch_name));
    });

    grid.set_factory(Some(&factory));

    let on_activate = Rc::new(on_activate);
    let selection_clone = selection.clone();
    grid.connect_activate(move |_, pos| {
        if let Some(obj) = selection_clone.item(pos).and_downcast::<glib::BoxedAnyObject>() {
            let item = obj.borrow::<VideoItem>();
            on_activate(item.video.id.clone());
        }
    });

    (grid, store)
}

pub fn fallback_texture() -> gdk::Texture {
    let w = 320i32;
    let h = 180i32;
    let stride = (w as usize) * 3;
    let data = vec![0x1eu8; stride * (h as usize)];
    let gb = glib::Bytes::from(&data[..]);
    gdk::MemoryTexture::new(w, h, gdk::MemoryFormat::R8g8b8, &gb, stride).upcast()
}
