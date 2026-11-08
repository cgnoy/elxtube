use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Label, ListItem, Orientation, Overlay, Picture, ScrolledWindow,
    SignalListItemFactory, SingleSelection, ListView,
};

use crate::core::model::Video;
use super::video_item::VideoItem;

pub fn build<F: Fn(String) + 'static>(
    videos: Vec<Video>,
    textures: HashMap<String, gdk::Texture>,
    on_activate: F,
    on_channel_click: Rc<dyn Fn(String, String)>,
) -> (ScrolledWindow, gio::ListStore) {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(24)
        .margin_end(24)
        .build();

    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for v in videos {
        let tex = textures.get(&v.id).cloned().unwrap_or_else(super::home::fallback_texture);
        store.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
    }

    let selection = SingleSelection::builder().model(&store).build();
    let list_view = ListView::builder()
        .model(&selection)
        .build();
    list_view.set_single_click_activate(true);

    let factory = SignalListItemFactory::new();
    let on_channel_click_setup = on_channel_click.clone();

    factory.connect_setup(move |_, list_item| {
        let li = list_item.downcast_ref::<ListItem>().unwrap();
        
        let row = GtkBox::new(Orientation::Horizontal, 16);
        row.set_margin_bottom(16);
        row.set_margin_top(8);

        let overlay = Overlay::new();
        overlay.set_size_request(320, 180);
        overlay.set_valign(gtk::Align::Start);
        
        let picture = Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .build();
        overlay.set_child(Some(&picture));

        let dur_lbl = Label::new(Some(""));
        dur_lbl.add_css_class("duration-badge");
        dur_lbl.set_halign(gtk::Align::End);
        dur_lbl.set_valign(gtk::Align::End);
        overlay.add_overlay(&dur_lbl);

        row.append(&overlay);

        let info_box = GtkBox::new(Orientation::Vertical, 4);
        info_box.set_hexpand(true);
        info_box.set_valign(gtk::Align::Start);

        let title = Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(2)
            .wrap(true)
            .xalign(0.0)
            .build();
        title.add_css_class("player-title");
        title.set_margin_bottom(4);
        info_box.append(&title);

        let ch_line = GtkBox::new(Orientation::Horizontal, 8);
        ch_line.set_margin_bottom(8);
        
        let avatar = Label::new(Some("C"));
        avatar.add_css_class("avatar");
        avatar.set_size_request(24, 24);
        avatar.set_valign(gtk::Align::Center);
        avatar.set_cursor_from_name(Some("pointer"));
        ch_line.append(&avatar);
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

        let ch_name = Label::new(None);
        ch_name.add_css_class("video-meta");
        ch_name.set_xalign(0.0);
        ch_line.append(&ch_name);

        info_box.append(&ch_line);

        let desc = Label::new(None);
        desc.add_css_class("video-meta");
        desc.set_ellipsize(gtk::pango::EllipsizeMode::End);
        desc.set_lines(1);
        desc.set_xalign(0.0);
        info_box.append(&desc);

        row.append(&info_box);
        li.set_child(Some(&row));
    });

    factory.connect_bind(move |_, list_item| {
        let li = list_item.downcast_ref::<ListItem>().unwrap();
        let obj = match li.item().and_downcast::<glib::BoxedAnyObject>() {
            Some(o) => o,
            None => return,
        };
        let item = obj.borrow::<VideoItem>();

        let row = li.child().and_downcast::<GtkBox>().unwrap();
        
        let overlay = row.first_child().and_downcast::<Overlay>().unwrap();
        let picture = overlay.child().and_downcast::<Picture>().unwrap();
        picture.set_paintable(Some(&item.texture));
        
        let dur_lbl = overlay.last_child().and_downcast::<Label>().unwrap();
        let dur_str = if let Some(d) = item.video.duration {
            format!("{}:{:02}", d / 60, d % 60)
        } else { "".to_string() };
        dur_lbl.set_text(&dur_str);

        let info_box = row.last_child().and_downcast::<GtkBox>().unwrap();
        let title = info_box.first_child().and_downcast::<Label>().unwrap();
        title.set_text(&item.video.title);

        let ch_line = title.next_sibling().and_downcast::<GtkBox>().unwrap();
        let avatar = ch_line.first_child().and_downcast::<Label>().unwrap();
        let ch_name = ch_line.last_child().and_downcast::<Label>().unwrap();
        
        let ch_str = item.video.channel_name.clone().unwrap_or_else(|| "Unknown".into());
        let initial = ch_str.chars().next().unwrap_or('C').to_uppercase().to_string();
        avatar.set_text(&initial);
        ch_name.set_text(&ch_str);

        // Klik Avatar

        let desc = ch_line.next_sibling().and_downcast::<Label>().unwrap();
        desc.set_text("1 jt x ditonton • Baru saja");
    });

    list_view.set_factory(Some(&factory));

    let on_activate = Rc::new(on_activate);
    let selection_clone = selection.clone();
    list_view.connect_activate(move |_, pos| {
        if let Some(obj) = selection_clone.item(pos).and_downcast::<glib::BoxedAnyObject>() {
            let item = obj.borrow::<VideoItem>();
            on_activate(item.video.id.clone());
        }
    });

    scroll.set_child(Some(&list_view));
    (scroll, store)
}
