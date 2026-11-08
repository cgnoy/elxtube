use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gtk::gio;
use gtk::prelude::*;
use gtk::{
    Box as GtkBox, Button, Label, Orientation, Picture, ScrolledWindow, Separator,
};

use super::home;

pub fn build<F: Fn(String) + 'static>(
    _t0: Instant,
    on_activate: F,
    on_channel_click: Rc<dyn Fn(String, String)>,
) -> (ScrolledWindow, gio::ListStore, Label, Label, Picture, Label, Label) {
    let scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .hexpand(true)
        .build();

    let container = GtkBox::new(Orientation::Vertical, 0);

    // 1. BANNER
    let banner = Picture::new();
    let banner_clone = banner.clone();
    banner.add_css_class("channel-banner");
    banner.set_height_request(200);
    banner.set_content_fit(gtk::ContentFit::Cover);
    banner.set_paintable(Some(&home::fallback_texture()));
    container.append(&banner);

    // 2. PROFIL
    let header_box = GtkBox::new(Orientation::Horizontal, 24);
    header_box.set_margin_top(24);
    header_box.set_margin_start(64);
    header_box.set_margin_end(64);
    header_box.set_margin_bottom(16);

    // Kita kembalikan label avatar & judul agar bisa diupdate dari window.rs
    let avatar = Label::new(Some("C"));
    avatar.add_css_class("channel-avatar-large");
    avatar.set_size_request(120, 120);
    avatar.set_valign(gtk::Align::Start);
    header_box.append(&avatar);

    let info_box = GtkBox::new(Orientation::Vertical, 4);
    info_box.set_valign(gtk::Align::Center);

    let title = Label::new(Some("Channel Name"));
    title.add_css_class("channel-title-large");
    title.set_xalign(0.0);
    info_box.append(&title);

    let handle = Label::new(Some("@channel"));
    let handle_clone = handle.clone();
    handle.add_css_class("video-meta");
    handle.set_xalign(0.0);
    info_box.append(&handle);

    let desc = Label::new(Some(""));
    let desc_clone = desc.clone();
    desc.add_css_class("video-meta");
    desc.set_xalign(0.0);
    desc.set_margin_top(8);
    info_box.append(&desc);

    let btn_sub = Button::with_label("Subscribe");
    btn_sub.add_css_class("btn-subscribe");
    btn_sub.set_halign(gtk::Align::Start);
    btn_sub.set_margin_top(12);
    info_box.append(&btn_sub);

    header_box.append(&info_box);
    container.append(&header_box);

    // 3. TABS
    let tab_box = GtkBox::new(Orientation::Horizontal, 32);
    tab_box.set_margin_start(64);
    tab_box.set_margin_end(64);
    tab_box.set_margin_bottom(8);

    let tabs = ["Beranda", "Video", "Shorts", "Playlist", "Postingan"];
    for (i, t) in tabs.iter().enumerate() {
        let lbl = Label::new(Some(t));
        lbl.add_css_class("channel-tab");
        if i == 1 { lbl.add_css_class("active-tab"); }
        tab_box.append(&lbl);
    }
    container.append(&tab_box);
    container.append(&Separator::new(Orientation::Horizontal));

    // 4. FILTER
    let filter_box = GtkBox::new(Orientation::Horizontal, 12);
    filter_box.set_margin_top(16);
    filter_box.set_margin_start(64);
    filter_box.set_margin_end(64);

    let filters = ["Terbaru", "Populer", "Terlama"];
    for (i, f) in filters.iter().enumerate() {
        let btn = Button::with_label(f);
        btn.add_css_class("btn-action");
        if i == 0 { btn.add_css_class("active-filter"); }
        filter_box.append(&btn);
    }
    container.append(&filter_box);

    // 5. GRID
    let (grid_widget, store) = home::build(
        _t0,
        Vec::new(),
        HashMap::new(),
        on_activate,
        on_channel_click,
    );
    grid_widget.set_margin_start(48);
    grid_widget.set_margin_end(48);
    grid_widget.set_margin_top(16);
    container.append(&grid_widget);

    scroll.set_child(Some(&container));
    (scroll, store, title, avatar, banner_clone, handle_clone, desc_clone)
}
