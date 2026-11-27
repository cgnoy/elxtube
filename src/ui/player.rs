use gtk::prelude::*;
use gtk::{
    gio, glib, AspectFrame, Box as GtkBox, Button, DropDown, GLArea, Image, Label, ListItem,
    ListView, Orientation, Overlay, Picture, Popover, Scale, ScrolledWindow, SignalListItemFactory,
    SingleSelection, StringList,
};
use libmpv_sys::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, CString};
use std::os::raw::c_int;
use std::ptr;
use std::rc::Rc;
use std::sync::atomic::Ordering;

use super::home;
use super::i18n;
use super::video_item::VideoItem;
use crate::application::ports::StreamQuality;
use crate::application::services::SubscriptionService;
use crate::application::tasks::TaskRunner;
use crate::application::AppContext;
use crate::core::model::{FeedbackTarget, Video};

mod controls;
mod history;
mod render;
mod view;

use history::HistoryState;
use render::*;

type ChannelClickCallback = Box<dyn Fn(String, String) + 'static>;
type VideoClickCallback = Box<dyn Fn(String) + 'static>;
type SubscriptionChangeHandler = Box<dyn Fn() + 'static>;
type QualityChangeHandler = Box<dyn Fn(StreamQuality) + 'static>;

const MPV_FORMAT_DOUBLE: u32 = 5;

fn seek_target(position: f64, seconds: f64) -> f64 {
    (position + seconds).max(0.0)
}

fn mpv_position(mpv: *mut mpv_handle) -> f64 {
    let mut position = 0.0;
    unsafe {
        let name = CString::new("time-pos").unwrap();
        mpv_get_property(
            mpv,
            name.as_ptr(),
            MPV_FORMAT_DOUBLE,
            &mut position as *mut _ as *mut c_void,
        );
    }
    position
}

fn format_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!(
        "{}:{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

pub struct Player {
    pub widget: GtkBox,
    pub status_label: Label,
    status_key: Rc<Cell<Option<&'static str>>>,
    status_values: Rc<RefCell<Vec<(&'static str, String)>>>,
    mpv: *mut mpv_handle,
    mpv_initialized: Rc<Cell<bool>>,
    ctx: Rc<RefCell<*mut mpv_render_context>>,
    update_ctx: *mut RenderSignal,
    pending_audio: Rc<RefCell<Option<String>>>,
    pending_resume: Rc<RefCell<Option<(f64, bool)>>>,

    current_video: Rc<RefCell<Option<Video>>>,
    history_state: HistoryState,
    subscription: SubscriptionService,
    subscribed_channels: Rc<RefCell<HashSet<String>>>,

    title_lbl: Label,
    ch_name_lbl: Label,
    avatar_lbl: Label,
    btn_sub: Button,
    btn_share: Button,
    btn_not_interested: Button,
    btn_hide_channel: Button,
    up_next_title: Label,
    on_channel_click: Rc<RefCell<Option<ChannelClickCallback>>>,
    watch_body: GtkBox,
    up_next: GtkBox,
    up_next_scroll: ScrolledWindow,
    up_next_store: gio::ListStore,
    on_up_next: Rc<RefCell<Option<VideoClickCallback>>>,
    on_subscription_change: Rc<RefCell<Option<SubscriptionChangeHandler>>>,
    res_dropdown: DropDown,
    quality: Rc<Cell<StreamQuality>>,
    active: Rc<Cell<bool>>,
    on_quality_change: Rc<RefCell<Option<QualityChangeHandler>>>,
}

impl Player {
    pub fn apply_language(&self) {
        self.res_dropdown
            .set_tooltip_text(Some(i18n::tr("resolution")));
        let subscribed = self
            .current_video
            .borrow()
            .as_ref()
            .and_then(|video| video.channel_id.as_ref())
            .is_some_and(|channel_id| self.subscribed_channels.borrow().contains(channel_id));
        self.btn_sub.set_label(i18n::tr(if subscribed {
            "subscribed"
        } else {
            "subscribe"
        }));
        self.btn_share.set_label(i18n::tr("share"));
        self.btn_not_interested
            .set_label(i18n::tr("not_interested"));
        self.btn_not_interested
            .set_tooltip_text(Some(i18n::tr("not_interested")));
        self.btn_hide_channel.set_label(i18n::tr("hide_channel"));
        self.btn_hide_channel
            .set_tooltip_text(Some(i18n::tr("hide_channel")));
        self.up_next_title.set_text(i18n::tr("up_next"));
        if let Some(key) = self.status_key.get() {
            let values = self.status_values.borrow();
            let values = values
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>();
            self.status_label.set_text(&i18n::format(key, &values));
        }
    }

    pub fn prepare(&self) {
        if self.mpv_initialized.get() {
            return;
        }
        let result = unsafe { mpv_initialize(self.mpv) };
        if result < 0 {
            eprintln!("[mpv] initialize gagal: {result}");
            return;
        }
        self.mpv_initialized.set(true);
    }

    pub fn set_active(&self, active: bool) {
        self.active.set(active);
    }

    pub fn set_layout(&self, compact: bool, recommendation_width: i32) {
        self.watch_body.set_orientation(if compact {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        });
        self.up_next
            .set_width_request(if compact { -1 } else { recommendation_width });
        self.up_next_scroll
            .set_height_request(if compact { 280 } else { -1 });
    }

    pub fn set_recommendations(
        &self,
        videos: Vec<Video>,
        textures: HashMap<String, gtk::gdk::Texture>,
    ) {
        self.up_next_store.remove_all();
        for video in videos.into_iter().take(12) {
            let texture = textures
                .get(&video.id)
                .cloned()
                .unwrap_or_else(home::fallback_texture);
            self.up_next_store
                .append(&glib::BoxedAnyObject::new(VideoItem::new(video, texture)));
        }
    }

    pub fn update_recommendation_texture(&self, video_id: &str, texture: gtk::gdk::Texture) {
        for index in 0..self.up_next_store.n_items() {
            let Some(object) = self
                .up_next_store
                .item(index)
                .and_downcast::<glib::BoxedAnyObject>()
            else {
                continue;
            };
            let old = object.borrow::<VideoItem>().clone();
            if old.video.id != video_id {
                continue;
            }
            self.up_next_store.splice(
                index,
                1,
                &[glib::BoxedAnyObject::new(VideoItem::new(
                    old.video,
                    texture.clone(),
                ))],
            );
            break;
        }
    }

    pub fn refresh_subscription_state(&self) {
        let Ok(channels) = self.subscription.list() else {
            return;
        };
        let subscribed: HashSet<String> = channels.into_iter().map(|channel| channel.id).collect();
        *self.subscribed_channels.borrow_mut() = subscribed;
        let Some(channel_id) = self
            .current_video
            .borrow()
            .as_ref()
            .and_then(|video| video.channel_id.clone())
        else {
            return;
        };
        let is_subscribed = self.subscribed_channels.borrow().contains(&channel_id);
        if is_subscribed {
            self.btn_sub.set_label(i18n::tr("subscribed"));
            self.btn_sub.add_css_class("btn-subscribed");
        } else {
            self.btn_sub.set_label(i18n::tr("subscribe"));
            self.btn_sub.remove_css_class("btn-subscribed");
        }
    }

    pub fn set_on_subscription_change<F: Fn() + 'static>(&self, callback: F) {
        *self.on_subscription_change.borrow_mut() = Some(Box::new(callback));
    }

    pub fn set_on_up_next<F: Fn(String) + 'static>(&self, callback: F) {
        *self.on_up_next.borrow_mut() = Some(Box::new(callback));
    }

    pub fn begin_video(&self, video_id: &str) {
        self.history_state.flush(self.position());
        let video = Video {
            id: video_id.to_string(),
            title: String::new(),
            ..Video::default()
        };
        *self.current_video.borrow_mut() = Some(video.clone());
        self.history_state.reset();
        self.history_state.video_changed(&video);
    }

    pub fn update_metadata(&self, video: Video) {
        self.title_lbl.set_text(&video.title);
        self.ch_name_lbl
            .set_text(video.channel_name.as_deref().unwrap_or("Unknown"));

        if let Some(name) = &video.channel_name {
            let initial = name
                .chars()
                .next()
                .unwrap_or('C')
                .to_uppercase()
                .to_string();
            self.avatar_lbl.set_text(&initial);
        }

        if let Some(ch_id) = &video.channel_id {
            let is_subbed = self.subscribed_channels.borrow().contains(ch_id);
            if is_subbed {
                self.btn_sub.set_label(i18n::tr("subscribed"));
                self.btn_sub.add_css_class("btn-subscribed");
            } else {
                self.btn_sub.set_label(i18n::tr("subscribe"));
                self.btn_sub.remove_css_class("btn-subscribed");
            }
        }

        self.history_state.video_changed(&video);
        *self.current_video.borrow_mut() = Some(video);
        self.refresh_subscription_state();
    }

    pub fn set_on_channel_click<F: Fn(String, String) + 'static>(&self, f: F) {
        *self.on_channel_click.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_loading_key(&self, key: &'static str, values: &[(&'static str, &str)]) {
        self.status_key.set(Some(key));
        *self.status_values.borrow_mut() = values
            .iter()
            .map(|(name, value)| (*name, (*value).to_string()))
            .collect();
        self.apply_language();
        self.status_label.set_visible(true);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            if !self.update_ctx.is_null() {
                (*self.update_ctx).active.store(false, Ordering::Release);
            }

            let ctx = *self.ctx.borrow();
            if !ctx.is_null() {
                mpv_render_context_set_update_callback(ctx, None, ptr::null_mut());
                mpv_render_context_free(ctx);
                *self.ctx.borrow_mut() = ptr::null_mut();
            }
            mpv_terminate_destroy(self.mpv);
            if !self.update_ctx.is_null() {
                drop(Box::from_raw(self.update_ctx));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::seek_target;

    #[test]
    fn arrow_seek_targets_exact_five_second_delta() {
        assert_eq!(seek_target(100.0, 5.0), 105.0);
        assert_eq!(seek_target(100.0, -5.0), 95.0);
        assert_eq!(seek_target(2.0, -5.0), 0.0);
    }
}
