use std::ffi::{c_void, CString};
use std::os::raw::c_int;

use gtk::prelude::*;
use libmpv_sys::*;

use super::{mpv_position, seek_target, Player};
use crate::application::ports::StreamQuality;

impl Player {
    pub fn toggle_pause(&self) {
        let mut paused: c_int = 0;
        let name = CString::new("pause").unwrap();
        unsafe {
            mpv_get_property(
                self.mpv,
                name.as_ptr(),
                3,
                &mut paused as *mut _ as *mut c_void,
            );
        }
        self.history_state.set_paused(paused != 1);
        let command = CString::new(if paused == 1 {
            "set pause no"
        } else {
            "set pause yes"
        })
        .unwrap();
        unsafe {
            mpv_command_string(self.mpv, command.as_ptr());
        }
        self.history_state.flush(mpv_position(self.mpv));
    }

    pub fn seek_by(&self, seconds: f64) {
        let target = seek_target(mpv_position(self.mpv), seconds);
        let command = CString::new(format!("seek {target:.3} absolute")).unwrap();
        unsafe {
            mpv_command_string(self.mpv, command.as_ptr());
        }
    }

    pub fn toggle_mute(&self) {
        let mut muted: c_int = 0;
        let name = CString::new("mute").unwrap();
        unsafe {
            mpv_get_property(
                self.mpv,
                name.as_ptr(),
                3,
                &mut muted as *mut _ as *mut c_void,
            );
        }
        let mut value: c_int = if muted == 1 { 0 } else { 1 };
        unsafe {
            mpv_set_property(
                self.mpv,
                name.as_ptr(),
                3,
                &mut value as *mut _ as *mut c_void,
            );
        }
    }

    pub fn toggle_fullscreen<W>(&self, window: &W)
    where
        W: glib::prelude::IsA<gtk::Window>,
    {
        let window = window.as_ref();
        if window.is_fullscreen() {
            window.unfullscreen();
        } else {
            window.fullscreen();
        }
    }

    pub fn pause(&self) {
        let name = CString::new("pause").unwrap();
        let mut value: c_int = 1;
        let result = unsafe {
            mpv_set_property(
                self.mpv,
                name.as_ptr(),
                3,
                &mut value as *mut _ as *mut c_void,
            )
        };
        eprintln!("[play] pause rc={result}");
    }

    pub fn play_with_resume(
        &self,
        url: &str,
        audio_url: Option<&str>,
        resume: Option<(f64, bool)>,
    ) {
        self.prepare();
        self.status_label.set_visible(false);
        *self.pending_audio.borrow_mut() = audio_url.map(str::to_string);
        *self.pending_resume.borrow_mut() = resume;
        if audio_url.is_some() {
            eprintln!("[play] audio queued");
        }
        if resume.is_some() {
            let pause = CString::new("set pause yes").unwrap();
            let _ = unsafe { mpv_command_string(self.mpv, pause.as_ptr()) };
        }
        let command = format!("loadfile \"{url}\" replace");
        let command = CString::new(command).unwrap();
        eprintln!("[play] loadfile issued");
        let _ = unsafe { mpv_command_string(self.mpv, command.as_ptr()) };
    }

    pub fn position(&self) -> f64 {
        mpv_position(self.mpv)
    }

    pub fn is_paused(&self) -> bool {
        let mut paused: c_int = 0;
        let name = CString::new("pause").unwrap();
        unsafe {
            mpv_get_property(
                self.mpv,
                name.as_ptr(),
                3,
                &mut paused as *mut _ as *mut c_void,
            );
        }
        paused == 1
    }

    pub fn quality(&self) -> StreamQuality {
        self.quality.get()
    }

    pub fn set_on_quality_change<F: Fn(StreamQuality) + 'static>(&self, callback: F) {
        *self.on_quality_change.borrow_mut() = Some(Box::new(callback));
    }

    pub fn apply_quality(&self, selected: StreamQuality) {
        if self.quality.get() == selected {
            return;
        }
        self.quality.set(selected);
        if self.res_dropdown.selected() != selected.index() {
            self.res_dropdown.set_selected(selected.index());
        }
    }

    pub fn quality_control_has_focus(&self) -> bool {
        self.res_dropdown.has_focus()
    }

    pub fn seek_to_start(&self) {
        let command = CString::new("seek 0 absolute").unwrap();
        unsafe {
            mpv_command_string(self.mpv, command.as_ptr());
        }
    }

    pub fn seek_to_end(&self) {
        let mut duration: f64 = 0.0;
        let name = CString::new("duration").unwrap();
        unsafe {
            mpv_get_property(
                self.mpv,
                name.as_ptr(),
                super::MPV_FORMAT_DOUBLE,
                &mut duration as *mut _ as *mut c_void,
            );
        }
        let command =
            CString::new(format!("seek {:.3} absolute", (duration - 1.0).max(0.0))).unwrap();
        unsafe {
            mpv_command_string(self.mpv, command.as_ptr());
        }
    }

    pub fn scroll_recommendations(&self, pages: f64) {
        let adjustment = self.up_next_scroll.vadjustment();
        let page_size = adjustment.page_size().max(200.0);
        adjustment.set_value(adjustment.value() + pages * page_size * 0.9);
    }
}
