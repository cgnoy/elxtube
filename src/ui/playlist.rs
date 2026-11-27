use std::collections::HashMap;
use std::rc::Rc;

use gtk::gdk;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    AspectFrame, Box as GtkBox, Button, Entry, Label, ListItem, ListView, Orientation, Overlay,
    Picture, ScrolledWindow, SignalListItemFactory, SingleSelection,
};

use super::i18n;
use super::list_view;
use crate::core::model::{Playlist, Video};

#[derive(Clone)]
pub struct PlaylistItem {
    pub playlist: Playlist,
    pub texture: gdk::Texture,
}

pub fn build_list(on_open: Rc<dyn Fn(String, String)>) -> (ScrolledWindow, gio::ListStore) {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .margin_top(8)
        .margin_bottom(16)
        .margin_start(40)
        .margin_end(40)
        .build();
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = SingleSelection::builder().model(&store).build();
    let list = ListView::builder().model(&selection).build();
    list.set_single_click_activate(true);
    let factory = SignalListItemFactory::new();

    factory.connect_setup(|_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let row = GtkBox::new(Orientation::Horizontal, 16);
        row.add_css_class("result-row");
        row.set_margin_bottom(12);
        let frame = AspectFrame::builder()
            .ratio(16.0 / 9.0)
            .halign(gtk::Align::Start)
            .width_request(240)
            .build();
        let overlay = Overlay::new();
        let picture = Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        overlay.set_child(Some(&picture));
        frame.set_child(Some(&overlay));
        row.append(&frame);
        let info = GtkBox::new(Orientation::Vertical, 6);
        info.set_hexpand(true);
        let title = Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .lines(2)
            .wrap(true)
            .build();
        title.add_css_class("player-title");
        info.append(&title);
        let meta = Label::builder().xalign(0.0).build();
        meta.add_css_class("video-meta");
        info.append(&meta);
        row.append(&info);
        item.set_child(Some(&row));
    });

    factory.connect_bind(|_, list_item| {
        let item = list_item.downcast_ref::<ListItem>().unwrap();
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let data = object.borrow::<PlaylistItem>();
        let row = item.child().and_downcast::<GtkBox>().unwrap();
        let frame = row.first_child().and_downcast::<AspectFrame>().unwrap();
        let overlay = frame.child().and_downcast::<Overlay>().unwrap();
        let picture = overlay.child().and_downcast::<Picture>().unwrap();
        picture.set_paintable(Some(&data.texture));
        let info = row.last_child().and_downcast::<GtkBox>().unwrap();
        let title = info.first_child().and_downcast::<Label>().unwrap();
        title.set_text(&data.playlist.title);
        let meta = title.next_sibling().and_downcast::<Label>().unwrap();
        let channel = data.playlist.channel_name.as_deref().unwrap_or("Unknown");
        let count = data
            .playlist
            .video_count
            .map(|count| {
                format!(
                    "{channel} • {}",
                    i18n::format("video_count", &[("count", &count.to_string())])
                )
            })
            .unwrap_or_else(|| channel.to_string());
        meta.set_text(&count);
    });

    list.set_factory(Some(&factory));
    let selection_activate = selection.clone();
    list.connect_activate(move |_, position| {
        if let Some(object) = selection_activate
            .item(position)
            .and_downcast::<glib::BoxedAnyObject>()
        {
            let data = object.borrow::<PlaylistItem>();
            on_open(data.playlist.id.clone(), data.playlist.title.clone());
        }
    });
    scroll.set_child(Some(&list));
    (scroll, store)
}

pub fn set_texture(store: &gio::ListStore, position: u32, texture: gdk::Texture) {
    let Some(object) = store.item(position).and_downcast::<glib::BoxedAnyObject>() else {
        return;
    };
    let playlist = {
        let data = object.borrow::<PlaylistItem>();
        data.playlist.clone()
    };
    let replacement = PlaylistItem { playlist, texture };
    store.remove(position);
    store.insert(position, &glib::BoxedAnyObject::new(replacement));
}

pub fn build_detail(
    on_activate: Rc<dyn Fn(String)>,
    on_channel_click: Rc<dyn Fn(String, String)>,
    on_playlist_open: Rc<dyn Fn(String, String)>,
) -> (GtkBox, gio::ListStore, Label, Label) {
    let container = GtkBox::new(Orientation::Vertical, 0);
    container.set_vexpand(true);
    let open_bar = GtkBox::new(Orientation::Horizontal, 8);
    open_bar.set_margin_top(16);
    open_bar.set_margin_start(40);
    open_bar.set_margin_end(40);
    let playlist_input = Entry::builder()
        .placeholder_text(i18n::tr("playlist_input"))
        .hexpand(true)
        .build();
    playlist_input.add_css_class("search-entry");
    let open_button = Button::with_label(i18n::tr("open_playlist"));
    open_button.add_css_class("btn-action");
    open_bar.append(&playlist_input);
    open_bar.append(&open_button);
    container.append(&open_bar);
    let open_input = playlist_input.clone();
    open_button.connect_clicked(move |_| {
        on_playlist_open(
            open_input.text().to_string(),
            i18n::tr("playlist").to_string(),
        );
    });
    let title = Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    title.add_css_class("channel-title-large");
    title.set_margin_top(24);
    title.set_margin_start(40);
    title.set_margin_end(40);
    container.append(&title);
    let count = Label::builder().xalign(0.0).build();
    count.add_css_class("video-meta");
    count.set_margin_start(40);
    count.set_margin_end(40);
    count.set_margin_bottom(12);
    container.append(&count);
    let (scroll, store) = list_view::build(
        Vec::<Video>::new(),
        HashMap::new(),
        on_activate,
        on_channel_click,
    );
    container.append(&scroll);
    (container, store, title, count)
}
