use gtk::gio;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, GridView, Label, Orientation, Picture, ScrolledWindow, Separator,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use super::home;
use super::i18n;
use super::playlist;

#[derive(Clone)]
pub struct ChannelView {
    pub widget: ScrolledWindow,
    pub store: gio::ListStore,
    pub grid: GridView,
    pub title: Label,
    pub avatar: Picture,
    pub banner: Picture,
    pub handle: Label,
    pub description: Label,
    pub subscribe: Button,
    pub playlist_store: gio::ListStore,
    pub playlist_tab: Button,
    tab_buttons: Vec<(Button, &'static str)>,
    filter_buttons: Vec<(Button, &'static str)>,
}

pub fn build(
    t0: Instant,
    on_activate: Rc<dyn Fn(String)>,
    on_channel_click: Rc<dyn Fn(String, String)>,
    on_playlist_open: Rc<dyn Fn(String, String)>,
) -> ChannelView {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();
    let container = GtkBox::new(Orientation::Vertical, 0);

    let banner = Picture::new();
    banner.add_css_class("channel-banner");
    banner.set_height_request(180);
    banner.set_content_fit(gtk::ContentFit::Cover);
    banner.set_paintable(Some(&home::fallback_texture()));
    container.append(&banner);

    let header = GtkBox::new(Orientation::Horizontal, 20);
    header.set_margin_top(20);
    header.set_margin_start(48);
    header.set_margin_end(48);
    header.set_margin_bottom(16);
    let avatar = Picture::new();
    avatar.add_css_class("channel-avatar-large");
    avatar.set_size_request(96, 96);
    avatar.set_content_fit(gtk::ContentFit::Cover);
    avatar.set_paintable(Some(&home::fallback_texture()));
    header.append(&avatar);

    let info = GtkBox::new(Orientation::Vertical, 4);
    info.set_hexpand(true);
    info.set_valign(gtk::Align::Center);
    let title = Label::builder()
        .label(i18n::tr("channel_name"))
        .xalign(0.0)
        .build();
    title.add_css_class("channel-title-large");
    info.append(&title);
    let handle = Label::builder().label("@channel").xalign(0.0).build();
    handle.add_css_class("video-meta");
    info.append(&handle);
    let description = Label::builder()
        .xalign(0.0)
        .wrap(true)
        .lines(2)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    description.add_css_class("video-meta");
    description.set_margin_top(6);
    info.append(&description);
    let subscribe = Button::with_label("Subscribe");
    subscribe.add_css_class("btn-subscribe");
    subscribe.set_halign(gtk::Align::Start);
    subscribe.set_margin_top(10);
    info.append(&subscribe);
    header.append(&info);
    container.append(&header);

    let tabs = GtkBox::new(Orientation::Horizontal, 28);
    tabs.set_margin_start(48);
    tabs.set_margin_bottom(8);
    let home_tab = Button::with_label(i18n::tr("home"));
    let video_tab = Button::with_label(i18n::tr("videos"));
    let shorts_tab = Button::with_label(i18n::tr("shorts"));
    let playlist_tab = Button::with_label(i18n::tr("playlist"));
    let posts_tab = Button::with_label(i18n::tr("posts"));
    for button in [
        &home_tab,
        &video_tab,
        &shorts_tab,
        &playlist_tab,
        &posts_tab,
    ] {
        button.add_css_class("channel-tab");
        tabs.append(button);
    }
    video_tab.add_css_class("active-tab");
    container.append(&tabs);
    container.append(&Separator::new(Orientation::Horizontal));

    let filters = GtkBox::new(Orientation::Horizontal, 12);
    filters.set_margin_top(14);
    filters.set_margin_start(48);
    filters.set_margin_bottom(8);
    let mut filter_buttons = Vec::new();
    for (index, (text, key)) in [
        (i18n::tr("latest"), "latest"),
        (i18n::tr("popular"), "popular"),
        (i18n::tr("oldest"), "oldest"),
    ]
    .iter()
    .enumerate()
    {
        let button = Button::with_label(text);
        button.add_css_class("btn-action");
        if index == 0 {
            button.add_css_class("active-filter");
        }
        filters.append(&button);
        filter_buttons.push((button.clone(), *key));
    }
    container.append(&filters);

    let (grid, store) = home::build(
        t0,
        Vec::new(),
        HashMap::new(),
        on_activate,
        on_channel_click,
    );
    grid.set_margin_start(40);
    grid.set_margin_end(40);
    grid.set_margin_top(8);
    grid.set_margin_bottom(24);
    container.append(&grid);

    let (playlist_scroll, playlist_store) = playlist::build_list(on_playlist_open);
    playlist_scroll.set_visible(false);
    container.append(&playlist_scroll);

    let tab_buttons = vec![
        (home_tab.clone(), "home"),
        (video_tab.clone(), "videos"),
        (shorts_tab.clone(), "shorts"),
        (playlist_tab.clone(), "playlist"),
        (posts_tab.clone(), "posts"),
    ];
    let show_videos = {
        let filters = filters.clone();
        let grid = grid.clone();
        let playlist_scroll = playlist_scroll.clone();
        let playlist_tab = playlist_tab.clone();
        let video_tab = video_tab.clone();
        move |show_playlist: bool| {
            filters.set_visible(!show_playlist);
            grid.set_visible(!show_playlist);
            playlist_scroll.set_visible(show_playlist);
            if show_playlist {
                video_tab.remove_css_class("active-tab");
                playlist_tab.add_css_class("active-tab");
            } else {
                playlist_tab.remove_css_class("active-tab");
                video_tab.add_css_class("active-tab");
            }
        }
    };
    let show_videos_tab = show_videos.clone();
    video_tab.connect_clicked(move |_| show_videos_tab(false));
    let show_playlists_tab = show_videos.clone();
    playlist_tab.connect_clicked(move |_| show_playlists_tab(true));

    scroll.set_child(Some(&container));
    ChannelView {
        widget: scroll,
        store,
        grid,
        title,
        avatar,
        banner,
        handle,
        description,
        subscribe,
        playlist_store,
        playlist_tab,
        tab_buttons,
        filter_buttons,
    }
}

impl ChannelView {
    pub fn apply_language(&self) {
        for (button, key) in &self.tab_buttons {
            button.set_label(i18n::tr(key));
        }
        for (button, key) in &self.filter_buttons {
            button.set_label(i18n::tr(key));
        }
    }
}
