use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    AspectFrame, Box as GtkBox, Label, ListItem, ListView, Orientation, Overlay, Picture,
    ProgressBar, ScrolledWindow, SignalListItemFactory, SingleSelection,
};
use std::collections::HashMap;
use std::rc::Rc;

use super::home;
use super::i18n;
use super::video_item::VideoItem;
use crate::core::model::Video;

pub fn build(
    videos: Vec<Video>,
    textures: HashMap<String, gdk::Texture>,
    on_activate: Rc<dyn Fn(String)>,
    on_channel_click: Rc<dyn Fn(String, String)>,
) -> (ScrolledWindow, gio::ListStore) {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .margin_top(8)
        .margin_bottom(16)
        .margin_start(24)
        .margin_end(24)
        .build();

    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    for video in videos {
        let texture = textures
            .get(&video.id)
            .cloned()
            .unwrap_or_else(home::fallback_texture);
        store.append(&glib::BoxedAnyObject::new(VideoItem::new(video, texture)));
    }

    let selection = SingleSelection::builder().model(&store).build();
    let list = ListView::builder().model(&selection).build();
    list.set_single_click_activate(true);
    let factory = SignalListItemFactory::new();
    let channel_setup = on_channel_click.clone();

    factory.connect_setup(move |_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let row = GtkBox::new(Orientation::Horizontal, 16);
        row.add_css_class("result-row");
        row.set_margin_bottom(12);

        let (frame, overlay, _picture) = home::thumbnail_frame();
        frame.add_css_class("result-thumbnail");
        frame.set_hexpand(false);
        let progress = ProgressBar::new();
        progress.add_css_class("thumb-progress");
        progress.set_halign(gtk::Align::Fill);
        progress.set_valign(gtk::Align::End);
        progress.set_visible(false);
        overlay.add_overlay(&progress);
        let duration = Label::new(Some(""));
        duration.add_css_class("duration-badge");
        duration.set_halign(gtk::Align::End);
        duration.set_valign(gtk::Align::End);
        overlay.add_overlay(&duration);
        frame.set_child(Some(&overlay));
        row.append(&frame);

        let info = GtkBox::new(Orientation::Vertical, 5);
        info.set_hexpand(true);
        info.set_valign(gtk::Align::Start);
        let title = Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(2)
            .wrap(true)
            .xalign(0.0)
            .build();
        title.add_css_class("player-title");
        info.append(&title);

        let channel_line = GtkBox::new(Orientation::Horizontal, 8);
        let avatar = Label::new(Some("C"));
        avatar.add_css_class("avatar");
        avatar.set_size_request(24, 24);
        avatar.set_cursor_from_name(Some("pointer"));
        channel_line.append(&avatar);
        let channel_click = channel_setup.clone();
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
        let channel = Label::builder().xalign(0.0).build();
        channel.add_css_class("video-meta");
        channel_line.append(&channel);
        info.append(&channel_line);

        let description = Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        description.add_css_class("result-description");
        info.append(&description);
        row.append(&info);
        item.set_child(Some(&row));
    });

    factory.connect_bind(move |_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let data = object.borrow::<VideoItem>();
        let row = item.child().and_downcast::<GtkBox>().unwrap();
        let frame = row.first_child().and_downcast::<AspectFrame>().unwrap();
        let overlay = frame.child().and_downcast::<Overlay>().unwrap();
        let picture = overlay.child().and_downcast::<Picture>().unwrap();
        picture.set_paintable(Some(&data.texture));
        let duration_label = overlay.last_child().and_downcast::<Label>().unwrap();
        duration_label.set_text(
            &data
                .video
                .duration
                .map(home::format_duration)
                .unwrap_or_default(),
        );
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

        let info = row.last_child().and_downcast::<GtkBox>().unwrap();
        let title = info.first_child().and_downcast::<Label>().unwrap();
        title.set_text(&data.video.title);
        let channel_line = title.next_sibling().and_downcast::<GtkBox>().unwrap();
        let avatar = channel_line.first_child().and_downcast::<Label>().unwrap();
        let channel = channel_line.last_child().and_downcast::<Label>().unwrap();
        let channel_name = data.video.channel_name.as_deref().unwrap_or("Unknown");
        avatar.set_text(
            &channel_name
                .chars()
                .next()
                .unwrap_or('C')
                .to_uppercase()
                .to_string(),
        );
        channel.set_text(channel_name);
        let description = channel_line.next_sibling().and_downcast::<Label>().unwrap();
        description.set_text(&if let Some(progress) = data.progress {
            i18n::format(
                "watched",
                &[("percent", &format!("{:.0}", progress * 100.0))],
            )
        } else {
            String::new()
        });
    });

    list.set_factory(Some(&factory));
    let selection_activate = selection.clone();
    list.connect_activate(move |_, position| {
        if let Some(object) = selection_activate
            .item(position)
            .and_downcast::<glib::BoxedAnyObject>()
        {
            let data = object.borrow::<VideoItem>();
            on_activate(data.video.id.clone());
        }
    });
    scroll.set_child(Some(&list));
    (scroll, store)
}

pub fn set_compact(scroll: &ScrolledWindow, compact: bool) {
    if compact {
        scroll.add_css_class("compact-results");
    } else {
        scroll.remove_css_class("compact-results");
    }
}
