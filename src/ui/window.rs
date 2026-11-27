use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{
    gdk, gio, glib, Application, Box as GtkBox, Button, Entry, Image, Label, Orientation,
    ScrolledWindow, Stack, StackTransitionType,
};

use super::channel;
use super::home;
use super::i18n::{self, Language, LanguageController};
use super::layout::{LayoutSpec, SidebarMode};
use super::list_view;
use super::navigation::{NavigationStack, Route};
use super::player::Player;
use super::playlist;

mod construction;
mod helpers;

use super::status::StatusView;
use super::theme::{Theme, ThemeController};
use super::thumbnails::ThumbnailPipeline;
use super::video_item::VideoItem;
use crate::application::ports::StreamQuality;
use crate::application::{
    tasks::{TaskOutcome, TaskRunner, TaskScope},
    AppContext,
};
use crate::core::model::Video;
use helpers::*;

type ChannelOpenHandler = Rc<dyn Fn(String, String)>;
type PlaylistOpenHandler = Rc<dyn Fn(String, String)>;
type SidebarRefreshHook = Box<dyn Fn()>;
type LoadStream = Rc<dyn Fn(String, Option<(f64, bool)>)>;

mod app_window {
    use super::*;

    #[derive(Default)]
    pub struct AppWindow;

    #[glib::object_subclass]
    impl ObjectSubclass for AppWindow {
        const NAME: &'static str = "ElxtubeApplicationWindow";
        type Type = super::AppWindow;
        type ParentType = gtk::ApplicationWindow;
    }

    impl ObjectImpl for AppWindow {
        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![glib::subclass::Signal::builder("size-changed")
                    .param_types([i32::static_type()])
                    .build()]
            })
        }
    }

    impl WidgetImpl for AppWindow {
        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            self.obj().emit_by_name::<()>("size-changed", &[&width]);
        }
    }

    impl WindowImpl for AppWindow {}
    impl ApplicationWindowImpl for AppWindow {}
}

glib::wrapper! {
    pub struct AppWindow(ObjectSubclass<app_window::AppWindow>)
        @extends gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native,
                    gtk::Root, gtk::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HomeFeed {
    All,
    Subscription,
    Continue,
}

pub fn build(app: &Application, t0: Instant, conn: AppContext, theme: ThemeController) {
    construction::build(app, t0, conn, theme)
}
