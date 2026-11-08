use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Orientation, GLArea, Label, DropDown, StringList, Image, Scale, Overlay, Popover, AspectFrame};
use libmpv_sys::*;
use std::ffi::{CString, c_char, c_void};
use std::os::raw::c_int;
use std::rc::Rc;
use std::cell::RefCell;
use std::ptr;
use std::sync::OnceLock;
use libloading::Library;

use crate::core::{db, model::Video};

const MPV_RENDER_PARAM_INVALID: u32 = 0;
const MPV_RENDER_PARAM_API_TYPE: u32 = 1;
const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: u32 = 2;
const MPV_RENDER_PARAM_OPENGL_FBO: u32 = 3;
const MPV_RENDER_PARAM_FLIP_Y: u32 = 4;
const MPV_RENDER_API_TYPE_OPENGL: &[u8] = b"opengl\0";
const MPV_FORMAT_DOUBLE: u32 = 5;

static EPOXY_LIB: OnceLock<Option<Library>> = OnceLock::new();
static EGL_LIB: OnceLock<Option<Library>> = OnceLock::new();
static GLX_LIB: OnceLock<Option<Library>> = OnceLock::new();

unsafe extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let epoxy = EPOXY_LIB.get_or_init(|| unsafe { Library::new("libepoxy.so.0").ok() });
    if let Some(lib) = epoxy {
        if let Ok(sym) = lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"epoxy_get_proc_address\0") {
            let ptr = sym(name);
            if !ptr.is_null() { return ptr; }
        }
    }
    let egl = EGL_LIB.get_or_init(|| unsafe { Library::new("libEGL.so.1").ok() });
    if let Some(lib) = egl {
        if let Ok(sym) = lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"eglGetProcAddress\0") {
            let ptr = sym(name);
            if !ptr.is_null() { return ptr; }
        }
    }
    let glx = GLX_LIB.get_or_init(|| unsafe { Library::new("libGL.so.1").ok() });
    if let Some(lib) = glx {
        if let Ok(sym) = lib.get::<unsafe extern "C" fn(*const c_char) -> *mut c_void>(b"glXGetProcAddressARB\0") {
            let ptr = sym(name);
            if !ptr.is_null() { return ptr; }
        }
    }
    ptr::null_mut()
}

unsafe extern "C" fn update_callback(ctx: *mut c_void) {
    let tx = &*(ctx as *const std::sync::mpsc::Sender<()>);
    // Kirim sinyal ke channel. Abaikan jika receiver sudah di-drop.
    let _ = tx.send(());
}

pub struct Player {
    pub widget: GtkBox,
    pub status_label: Label,
    mpv: *mut mpv_handle,
    ctx: Rc<RefCell<*mut mpv_render_context>>,
    update_ctx: *mut c_void,
    seek_slider: Scale,
    is_seeking: Rc<RefCell<bool>>,
    pending_audio: Rc<RefCell<Option<String>>>,
    
    db_conn: Rc<RefCell<rusqlite::Connection>>,
    current_video: Rc<RefCell<Option<Video>>>,
    
    title_lbl: Label,
    ch_name_lbl: Label,
    avatar_lbl: Label,
    btn_sub: Button,
    info_box: GtkBox,
    on_channel_click: Rc<RefCell<Option<Box<dyn Fn(String, String) + 'static>>>>,
}

impl Player {
    pub fn new(db_conn: Rc<RefCell<rusqlite::Connection>>, top_bar: GtkBox, sidebar_revealer: gtk::Revealer) -> Self {
        let current_video = Rc::new(RefCell::new(None::<Video>));
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(false); // Biarkan aspect frame yang atur tinggi

        let video_overlay = Overlay::new();
        video_overlay.set_hexpand(true);
        video_overlay.set_vexpand(true);

        let gl_area = GLArea::builder().hexpand(true).vexpand(true).build();
        gl_area.set_required_version(3, 2);
        video_overlay.set_child(Some(&gl_area));

        let status_label = Label::new(Some("Memuat..."));
        status_label.add_css_class("title-2");
        status_label.set_halign(gtk::Align::Center);
        status_label.set_valign(gtk::Align::Center);
        video_overlay.add_overlay(&status_label);

        let controls_box = GtkBox::new(Orientation::Vertical, 4);
        controls_box.add_css_class("player-controls");
        controls_box.set_valign(gtk::Align::End);
        controls_box.set_vexpand(false);

        let seek_slider = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        seek_slider.add_css_class("seek-bar");
        seek_slider.set_draw_value(false);
        seek_slider.set_hexpand(true);
        controls_box.append(&seek_slider);

        let btn_row = GtkBox::new(Orientation::Horizontal, 16);
        let play_icon = Image::from_icon_name("media-playback-start-symbolic");
        play_icon.set_pixel_size(24);
        let play_btn = Button::builder().child(&play_icon).build();
        
        let vol_icon = Image::from_icon_name("audio-volume-high-symbolic");
        vol_icon.set_pixel_size(20);
        let vol_btn = Button::builder().child(&vol_icon).build();

        let time_lbl = Label::new(Some("00:00 / 00:00"));
        time_lbl.add_css_class("player-time");

        let res_list = StringList::new(&["Auto", "1080p", "720p", "480p", "360p"]);
        let res_dropdown = DropDown::new(Some(res_list), gtk::Expression::NONE);
        res_dropdown.set_selected(0);

        let full_icon = Image::from_icon_name("view-fullscreen-symbolic");
        full_icon.set_pixel_size(20);
        let full_btn = Button::builder().child(&full_icon).build();

        let spacer = GtkBox::new(Orientation::Horizontal, 0);
        spacer.set_hexpand(true);

        btn_row.append(&play_btn);
        btn_row.append(&vol_btn);
        btn_row.append(&time_lbl);
        btn_row.append(&spacer);
        btn_row.append(&res_dropdown);
        btn_row.append(&full_btn);
        
        controls_box.append(&btn_row);
        video_overlay.add_overlay(&controls_box);

        // Kunci tinggi 500px (tanpa AspectFrame agar tidak putih saat resize)
        video_overlay.set_height_request(500);
        widget.append(&video_overlay);

        // Info Area
        let info_box = GtkBox::new(Orientation::Vertical, 8);
        info_box.set_margin_top(12);
        info_box.set_margin_bottom(24);
        info_box.set_margin_start(24);
        info_box.set_margin_end(24);
        info_box.set_vexpand(false);

        let title_lbl = Label::new(Some("Memuat judul..."));
        title_lbl.add_css_class("player-title");
        title_lbl.set_xalign(0.0);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info_box.append(&title_lbl);

        let action_row = GtkBox::new(Orientation::Horizontal, 12);
        let on_channel_click: Rc<RefCell<Option<Box<dyn Fn(String, String) + 'static>>>> = Rc::new(RefCell::new(None));

        let ch_box = GtkBox::new(Orientation::Horizontal, 12);
        let avatar_lbl = Label::new(Some("C"));
        avatar_lbl.add_css_class("avatar");
        avatar_lbl.set_size_request(40, 40);
        avatar_lbl.set_valign(gtk::Align::Center);
        avatar_lbl.set_cursor_from_name(Some("pointer"));
        
        let ch_text = GtkBox::new(Orientation::Vertical, 0);
        let ch_name_lbl = Label::new(Some("Memuat channel..."));
        ch_name_lbl.add_css_class("player-channel");
        ch_name_lbl.set_xalign(0.0);
        ch_name_lbl.set_cursor_from_name(Some("pointer"));
        let ch_subs = Label::new(Some("1 jt subscriber"));
        ch_subs.add_css_class("player-subs");
        ch_subs.set_xalign(0.0);
        ch_text.append(&ch_name_lbl);
        ch_text.append(&ch_subs);
        
        ch_box.append(&avatar_lbl);
        ch_box.append(&ch_text);
        action_row.append(&ch_box);

        let btn_sub = Button::with_label("Subscribe");
        btn_sub.add_css_class("btn-subscribe");
        action_row.append(&btn_sub);

        let spacer2 = GtkBox::new(Orientation::Horizontal, 0);
        spacer2.set_hexpand(true);
        action_row.append(&spacer2);

        let btn_like = Button::with_label("👍 1,2 rb");
        btn_like.add_css_class("btn-action");
        let btn_share = Button::with_label("↗ Bagikan");
        btn_share.add_css_class("btn-action");
        action_row.append(&btn_like);
        action_row.append(&btn_share);

        info_box.append(&action_row);
        widget.append(&info_box);

        // LISTENER KLIK: Avatar & Nama Channel
        let on_click_avatar = on_channel_click.clone();
        let vid_click_avatar = current_video.clone();
        let avatar_gesture = gtk::GestureClick::new();
        avatar_gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        avatar_gesture.connect_released(move |_, _, _, _| {
            if let Some(v) = vid_click_avatar.borrow().as_ref() {
                if let (Some(ch_id), Some(ch_name)) = (&v.channel_id, &v.channel_name) {
                    if let Some(cb) = on_click_avatar.borrow().as_ref() {
                        cb(ch_id.clone(), ch_name.clone());
                    } else {
                    }
                } else {
                }
            }
        });
        avatar_lbl.add_controller(avatar_gesture);

        let on_click_name = on_channel_click.clone();
        let vid_click_name = current_video.clone();
        let name_gesture = gtk::GestureClick::new();
        name_gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
        name_gesture.connect_released(move |_, _, _, _| {
            if let Some(v) = vid_click_name.borrow().as_ref() {
                if let (Some(ch_id), Some(ch_name)) = (&v.channel_id, &v.channel_name) {
                    if let Some(cb) = on_click_name.borrow().as_ref() {
                        cb(ch_id.clone(), ch_name.clone());
                    }
                }
            }
        });
        ch_name_lbl.add_controller(name_gesture);

        let pending_audio: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let pending_audio_handler = pending_audio.clone();
        let (render_tx, render_rx) = std::sync::mpsc::channel::<()>();
        let mpv = unsafe { mpv_create() };
        let set_opt = |name: &str, val: &str| {
            let n = CString::new(name).unwrap();
            let v = CString::new(val).unwrap();
            unsafe { mpv_set_option_string(mpv, n.as_ptr(), v.as_ptr()); }
        };
        set_opt("vo", "libmpv");
        set_opt("hwdec", "auto-safe");
        set_opt("keep-open", "yes");
        set_opt("cache", "yes");
        set_opt("input-default-bindings", "no");
        set_opt("input-vo-keyboard", "no");
        set_opt("user-agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36");
        // FASTLOAD: matikan hook yang memicu subprocess & ~2-3 detik delay per load
        set_opt("ytdl", "no");
        set_opt("load-scripts", "no");
        set_opt("auto-profiles", "no");
        unsafe { mpv_initialize(mpv); }

        // Event drain + deferred audio-add (production, no logging)
        {
            let mpv_dbg = mpv;
            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                loop {
                    let ev = unsafe { mpv_wait_event(mpv_dbg, 0.0) };
                    if ev.is_null() { break; }
                    let e = unsafe { &*ev };
                    if e.event_id == 0 { break; }
                    if e.event_id == 7 {
                        let ef = unsafe { &*e.data.cast::<mpv_event_end_file>() };
                        eprintln!("[play] end-file reason={} error={}", ef.reason, ef.error);
                    }
                    if e.event_id == 8 {
                        eprintln!("[play] file-loaded");
                        // file-loaded: pasang audio yang di-queue
                        if let Some(audio) = pending_audio_handler.borrow_mut().take() {
                            let cmd = format!("audio-add \"{}\"", audio);
                            if let Ok(c) = CString::new(cmd) {
                                unsafe { mpv_command_string(mpv_dbg, c.as_ptr()); }
                            }
                        }
                    }
                }
                glib::ControlFlow::Continue
            });
        }

        unsafe {
            let n_vol = CString::new("volume").unwrap();
            let mut v: f64 = 100.0;
            mpv_set_property(mpv, n_vol.as_ptr(), MPV_FORMAT_DOUBLE, &mut v as *mut _ as *mut c_void);
        }

        // Opsi A: pause saat unmap, resume saat map (respect manual pause)
        {
            let mpv_pause_state = Rc::new(RefCell::new(false));
            let mpv_vis = mpv;
            let state_unmap = mpv_pause_state.clone();
            widget.connect_unmap(move |_| {
                let mut pause_val: c_int = 0;
                let n_pause = CString::new("pause").unwrap();
                unsafe {
                    mpv_get_property(mpv_vis, n_pause.as_ptr(), 3, &mut pause_val as *mut _ as *mut c_void);
                }
                *state_unmap.borrow_mut() = pause_val == 1;
                let cmd = CString::new("set pause yes").unwrap();
                unsafe { mpv_command_string(mpv_vis, cmd.as_ptr()); }
            });
            let mpv_vis2 = mpv;
            let state_map = mpv_pause_state.clone();
            widget.connect_map(move |_| {
                if !*state_map.borrow() {
                    let cmd = CString::new("set pause no").unwrap();
                    unsafe { mpv_command_string(mpv_vis2, cmd.as_ptr()); }
                }
            });
        }

        let ctx = Rc::new(RefCell::new(ptr::null_mut()));
        let mpv_ptr = mpv;
        let ctx_clone = ctx.clone();
        let update_ctx = Box::into_raw(Box::new(render_tx.clone())) as *mut c_void;
        
        gl_area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() { return; }
            let mut init_params = mpv_opengl_init_params {
                get_proc_address: Some(get_proc_address),
                get_proc_address_ctx: ptr::null_mut(),
                extra_exts: ptr::null(),
            };
            let mut render_params = [
                mpv_render_param { type_: MPV_RENDER_PARAM_API_TYPE, data: MPV_RENDER_API_TYPE_OPENGL.as_ptr() as *mut c_void },
                mpv_render_param { type_: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS, data: &mut init_params as *mut _ as *mut c_void },
                mpv_render_param { type_: MPV_RENDER_PARAM_INVALID, data: ptr::null_mut() },
            ];
            let mut render_ctx: *mut mpv_render_context = ptr::null_mut();
            let res = unsafe { mpv_render_context_create(&mut render_ctx, mpv_ptr, render_params.as_mut_ptr()) };
            if res < 0 { return; }
            *ctx_clone.borrow_mut() = render_ctx;
            unsafe { mpv_render_context_set_update_callback(render_ctx, Some(update_callback), update_ctx); }
            // LOGIC-01(A): force paint cycle pertama setelah recreate
            area.queue_render();
        });

        // FIX: Bersihkan context saat widget dihancurkan (resize/fullscreen)
        let ctx_unrealize = ctx.clone();
        gl_area.connect_unrealize(move |_| {
            let mut render_ctx = ctx_unrealize.borrow_mut();
            if !render_ctx.is_null() {
                unsafe {
                    mpv_render_context_set_update_callback(*render_ctx, None, std::ptr::null_mut());
                    mpv_render_context_free(*render_ctx);
                }
                *render_ctx = std::ptr::null_mut();
            }
        });

        // FIX RESIZE PUTIH: Paksa GTK menggambar ulang saat ukuran berubah

        // LOGIC-10: resize handler yang sebelumnya hilang
        let gl_area_resize = gl_area.clone();
        gl_area.connect_resize(move |_, w, h| {
            if w > 0 && h > 0 {
                gl_area_resize.queue_render();
            }
        });

        let ctx_render = ctx.clone();
        gl_area.connect_render(move |area, _gl_context| {
            if area.width() == 0 || area.height() == 0 {
                return glib::Propagation::Stop;
            }
            let render_ctx = *ctx_render.borrow();
            if render_ctx.is_null() {
                // LOGIC-01(B): clear hitam agar tidak putih saat transisi
                area.make_current();
                unsafe {
                    let p = get_proc_address(ptr::null_mut(), b"glClearColor\0".as_ptr() as *const c_char);
                    if !p.is_null() {
                        let f: unsafe extern "C" fn(f32,f32,f32,f32) = std::mem::transmute(p);
                        f(0.0, 0.0, 0.0, 1.0);
                        let c = get_proc_address(ptr::null_mut(), b"glClear\0".as_ptr() as *const c_char);
                        if !c.is_null() {
                            let g: unsafe extern "C" fn(u32) = std::mem::transmute(c);
                            g(0x4000); // GL_COLOR_BUFFER_BIT
                        }
                    }
                }
                return glib::Propagation::Stop;
            }
            area.make_current();
            let mut current_fbo: i32 = 0;
            unsafe {
                let get_gl_int_ptr = get_proc_address(ptr::null_mut(), b"glGetIntegerv\0".as_ptr() as *const c_char);
                if !get_gl_int_ptr.is_null() {
                    let get_gl_int: unsafe extern "C" fn(u32, *mut i32) = std::mem::transmute(get_gl_int_ptr);
                    get_gl_int(0x8CA6, &mut current_fbo);
                }
            }
            let scale = area.scale_factor();
            let mut fbo = mpv_opengl_fbo { 
                fbo: current_fbo, 
                w: area.width() * scale, 
                h: area.height() * scale, 
                internal_format: 0 
            };
            let mut flip_y: c_int = 1;
            let mut render_params = [
                mpv_render_param { type_: MPV_RENDER_PARAM_OPENGL_FBO, data: &mut fbo as *mut _ as *mut c_void },
                mpv_render_param { type_: MPV_RENDER_PARAM_FLIP_Y, data: &mut flip_y as *mut _ as *mut c_void },
                mpv_render_param { type_: MPV_RENDER_PARAM_INVALID, data: ptr::null_mut() },
            ];
            unsafe {
                mpv_render_context_render(render_ctx, render_params.as_mut_ptr());
                // LOGIC-09: release internal reference, tanpa ini mpv stall & white screen
                mpv_render_context_report_swap(render_ctx);
            }
            
            glib::Propagation::Stop
        });
        // Timer di Main Thread: pantau sinyal dari MPV
        let gl_area_render = gl_area.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            if render_rx.try_recv().is_ok() {
                gl_area_render.queue_render();
            }
            glib::ControlFlow::Continue
        });

        // Timer 16ms dihapus. Render dipicu oleh frame baru dari MPV.
        // Tambahkan connect_resize untuk merender ulang saat ukuran berubah


        // Volume Popover
        let vol_popover = Popover::new();
        vol_popover.set_parent(&vol_btn);
        vol_popover.set_has_arrow(true);
        
        let vol_box = GtkBox::new(Orientation::Vertical, 8);
        vol_box.set_margin_top(12);
        vol_box.set_margin_bottom(12);
        vol_box.set_margin_start(12);
        vol_box.set_margin_end(12);
        
        let vol_slider = Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
        vol_slider.set_width_request(140);
        vol_slider.set_draw_value(false);
        vol_slider.set_value(100.0);
        vol_slider.add_css_class("vol-slider");
        
        let vol_pct = Label::new(Some("100%"));
        vol_pct.add_css_class("vol-pct");
        vol_pct.set_halign(gtk::Align::Center);
        
        vol_box.append(&vol_slider);
        vol_box.append(&vol_pct);
        vol_popover.set_child(Some(&vol_box));
        
        let vol_popover_clone = vol_popover.clone();
        vol_btn.connect_clicked(move |_| {
            vol_popover_clone.popup();
        });

        let mpv_vol = mpv;
        let vol_pct_clone = vol_pct.clone();
        vol_slider.connect_value_changed(move |s| {
            let v = s.value();
            unsafe {
                let n_vol = CString::new("volume").unwrap();
                let mut val = v;
                mpv_set_property(mpv_vol, n_vol.as_ptr(), MPV_FORMAT_DOUBLE, &mut val as *mut _ as *mut c_void);
            }
            vol_pct_clone.set_text(&format!("{:.0}%", v));
        });

        let is_seeking = Rc::new(RefCell::new(false));
        let mpv_play = mpv;
        let play_icon_clone = play_icon.clone();
        play_btn.connect_clicked(move |_| {
            let mut pause_val: c_int = 0;
            let n_pause = CString::new("pause").unwrap();
            unsafe {
                mpv_get_property(mpv_play, n_pause.as_ptr(), 3, &mut pause_val as *mut _ as *mut c_void);
                let cmd = CString::new(if pause_val == 1 { "set pause no" } else { "set pause yes" }).unwrap();
                mpv_command_string(mpv_play, cmd.as_ptr());
            }
            if pause_val == 1 { play_icon_clone.set_icon_name(Some("media-playback-pause-symbolic")); }
            else { play_icon_clone.set_icon_name(Some("media-playback-start-symbolic")); }
        });

        let mpv_seek = mpv;
        let is_seeking_clone = is_seeking.clone();
        seek_slider.connect_change_value(move |_, _, val| {
            *is_seeking_clone.borrow_mut() = true;
            let cmd = format!("seek {:.2} absolute", val);
            let c_cmd = CString::new(cmd).unwrap();
            unsafe { mpv_command_string(mpv_seek, c_cmd.as_ptr()); }
            glib::Propagation::Proceed
        });
        
        let is_seeking_clone2 = is_seeking.clone();
        let gesture = gtk::GestureClick::new();
        gesture.connect_released(move |gesture, _, _, _| {
            *is_seeking_clone2.borrow_mut() = false;
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        seek_slider.add_controller(gesture);

        // Fullscreen Logic
        // FS-01/FS-02/FS-04: handler tombol hanya toggle window.
        // Semua perubahan layout di-handle oleh signal notify::fullscreen.
        let window_clone = widget.clone();
        full_btn.connect_clicked(move |_| {
            if let Some(root) = window_clone.root() {
                if let Some(win) = root.downcast_ref::<gtk::Window>() {
                    let is_fs = !win.is_fullscreen();
                    if is_fs { win.fullscreen(); } else { win.unfullscreen(); }
                }
            }
        });

        // FS-01/FS-02/FS-04: sync layout saat fullscreen berubah (cover keyboard/WM shortcut juga)
        {
            let top_c = top_bar.clone();
            let side_c = sidebar_revealer.clone();
            let info_c = info_box.clone();
            let overlay_c = video_overlay.clone();
            let pw_c = widget.clone();
            let gl_c = gl_area.clone();
            widget.connect_realize(move |w| {
                if let Some(win) = w.root().and_downcast::<gtk::Window>() {
                    let top = top_c.clone();
                    let side = side_c.clone();
                    let info = info_c.clone();
                    let overlay = overlay_c.clone();
                    let pw = pw_c.clone();
                    let gl = gl_c.clone();
                    win.connect_notify_local(Some("fullscreened"), move |win, _| {
                        let is_fs = win.is_fullscreen();
                        // Iterasi 2d: hide Revealer sepenuhnya (remove dari layout tree)
                        side.set_visible(!is_fs);
                        // Tetap set reveal_child untuk konsistensi state internal
                        side.set_reveal_child(!is_fs);

                        // Chrome
                        top.set_visible(!is_fs);
                        info.set_visible(!is_fs);
                        pw.set_margin_top(if is_fs { 0 } else { 24 });
                        pw.set_margin_bottom(if is_fs { 0 } else { 24 });
                        pw.set_margin_start(if is_fs { 0 } else { 24 });
                        pw.set_margin_end(if is_fs { 0 } else { 24 });
                        overlay.set_height_request(if is_fs { -1 } else { 500 });
                        pw.set_vexpand(is_fs);

                        let gl2 = gl.clone();
                        for ms in [16u64, 100, 300, 600] {
                            let gl3 = gl2.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(ms),
                                move || { gl3.queue_render(); }
                            );
                        }
                    });
                }
            });
        }

        let mpv_time = mpv;
        let time_lbl_clone = time_lbl.clone();
        let seek_clone = seek_slider.clone();
        let is_seeking_clone3 = is_seeking.clone();
        let play_icon_timer = play_icon.clone();
        
        let db_save = db_conn.clone();
        let current_video_timer = current_video.clone();
        
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            let mut pos: f64 = 0.0;
            let mut dur: f64 = 0.0;
            let mut pause_val: c_int = 0;
            unsafe {
                let n_pos = CString::new("time-pos").unwrap();
                mpv_get_property(mpv_time, n_pos.as_ptr(), MPV_FORMAT_DOUBLE, &mut pos as *mut _ as *mut c_void);
                let n_dur = CString::new("duration").unwrap();
                mpv_get_property(mpv_time, n_dur.as_ptr(), MPV_FORMAT_DOUBLE, &mut dur as *mut _ as *mut c_void);
                let n_pause = CString::new("pause").unwrap();
                mpv_get_property(mpv_time, n_pause.as_ptr(), 3, &mut pause_val as *mut _ as *mut c_void);
            }
            if pause_val == 1 { play_icon_timer.set_icon_name(Some("media-playback-start-symbolic")); }
            else { play_icon_timer.set_icon_name(Some("media-playback-pause-symbolic")); }
            if dur > 0.0 {
                if (seek_clone.value() - dur).abs() > 1.0 && seek_clone.adjustment().upper() != dur { seek_clone.adjustment().set_upper(dur); }
                if !*is_seeking_clone3.borrow() { seek_clone.set_value(pos); }
                let cur_m = (pos / 60.0) as u32; let cur_s = (pos % 60.0) as u32;
                let tot_m = (dur / 60.0) as u32; let tot_s = (dur % 60.0) as u32;
                time_lbl_clone.set_text(&format!("{}:{:02} / {}:{:02}", cur_m, cur_s, tot_m, tot_s));
            }
            
            if pos > 5.0 {
                if let Some(ref v) = *current_video_timer.borrow() {
                    let _ = db::history_upsert(&db_save.borrow(), v, pos);
                }
            }
            
            glib::ControlFlow::Continue
        });

        let current_vid_sub = current_video.clone();
        let db_sub = db_conn.clone();
        let btn_sub_clone = btn_sub.clone();
        btn_sub.connect_clicked(move |_| {
            let v_opt = current_vid_sub.borrow().clone();
            
            if let Some(v) = v_opt {
                if let Some(ch_id) = v.channel_id.clone() {
                    let is_subbed = {
                        let conn = db_sub.borrow();
                        db::channel_list(&conn).unwrap_or_default().iter().any(|c| c.id == ch_id)
                    };

                    if is_subbed {
                        let conn = db_sub.borrow();
                        let _ = db::channel_delete(&conn, &ch_id);
                        btn_sub_clone.set_label("Subscribe");
                        btn_sub_clone.remove_css_class("btn-subscribed");
                    } else {
                        {
                            let conn = db_sub.borrow();
                            let ch = crate::core::model::Channel {
                                id: ch_id.clone(),
                                name: v.channel_name.clone().unwrap_or_else(|| "Unknown".into()),
                                avatar_url: None,
                            };
                            let _ = db::channel_upsert(&conn, &ch);
                        }
                        
                        btn_sub_clone.set_label("Subscribed");
                        btn_sub_clone.add_css_class("btn-subscribed");

                        let ch_id_fetch: String = ch_id.clone();
                        let ch_name_fetch: String = v.channel_name.clone().unwrap_or_else(|| "Unknown".into());
                        
                        std::thread::spawn(move || {
                            eprintln!("[bg] fetching videos for channel {}", ch_id_fetch);
                            match crate::core::ytdlp::fetch_channel_videos(&ch_id_fetch, Some(&ch_name_fetch), 15) {
                                Ok(vids) => {
                                    if let Ok(conn) = crate::core::db::open() {
                                        let now = crate::core::now_secs();
                                        for mut vid in vids {
                                            vid.channel_id = Some(ch_id_fetch.clone());
                                            vid.channel_name = Some(ch_name_fetch.clone());
                                            let _ = db::feed_upsert(&conn, &vid, now);
                                            if let Some(url) = &vid.thumbnail_url {
                                                if let Ok(raw) = crate::core::thumbs::fetch_bytes(url) {
                                                    if let Ok(jpg) = crate::core::thumbs::resize_to_thumb_bytes(&raw) {
                                                        let _ = db::thumb_blob_put(&conn, &vid.id, &jpg);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => eprintln!("[bg] GAGAL: {}", e),
                            }
                        });
                    }
                }
            }
        });

        Self {
            widget, status_label, mpv, ctx, update_ctx,
            seek_slider, is_seeking, db_conn, current_video, pending_audio,
            title_lbl, ch_name_lbl, avatar_lbl, btn_sub, info_box,
            on_channel_click,
        }
    }
    
    pub fn update_metadata(&self, video: Video) {
        self.title_lbl.set_text(&video.title);
        self.ch_name_lbl.set_text(video.channel_name.as_deref().unwrap_or("Unknown"));
        
        if let Some(name) = &video.channel_name {
            let initial = name.chars().next().unwrap_or('C').to_uppercase().to_string();
            self.avatar_lbl.set_text(&initial);
        }

        if let Some(ch_id) = &video.channel_id {
            let conn = self.db_conn.borrow();
            let is_subbed = db::channel_list(&conn).unwrap_or_default().iter().any(|c| c.id == *ch_id);
            if is_subbed {
                self.btn_sub.set_label("Subscribed");
                self.btn_sub.add_css_class("btn-subscribed");
            } else {
                self.btn_sub.set_label("Subscribe");
                self.btn_sub.remove_css_class("btn-subscribed");
            }
        }

        *self.current_video.borrow_mut() = Some(video);
    }
    
    pub fn pause(&self) {
        let n = CString::new("pause").unwrap();
        let mut val: c_int = 1;
        let rc = unsafe { mpv_set_property(self.mpv, n.as_ptr(), 3, &mut val as *mut _ as *mut c_void) };
        eprintln!("[play] pause rc={}", rc);
    }

    pub fn play(&self, url: &str, audio_url: Option<&str>) {
        self.status_label.set_visible(false);
        let cmd = format!("loadfile \"{}\" replace", url);
        let c_cmd = CString::new(cmd).unwrap();
        eprintln!("[play] loadfile issued");
        let _ = unsafe { mpv_command_string(self.mpv, c_cmd.as_ptr()) };
        // Unpause: pause-on-click mungkin set pause=yes untuk video lama.
        let n = CString::new("pause").unwrap();
        let mut val: c_int = 0;
        let rc = unsafe { mpv_set_property(self.mpv, n.as_ptr(), 3, &mut val as *mut _ as *mut c_void) };
        eprintln!("[play] unpause rc={}", rc);
        if let Some(audio) = audio_url {
            *self.pending_audio.borrow_mut() = Some(audio.to_string());
            eprintln!("[play] audio queued");
        }
    }
    
    pub fn set_on_channel_click<F: Fn(String, String) + 'static>(&self, f: F) {
        *self.on_channel_click.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_loading(&self, msg: &str) {
        self.status_label.set_text(msg);
        self.status_label.set_visible(true);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe {
            let ctx = *self.ctx.borrow();
            if !ctx.is_null() { mpv_render_context_free(ctx); }
            mpv_terminate_destroy(self.mpv);
            if !self.update_ctx.is_null() { drop(Box::from_raw(self.update_ctx as *mut std::sync::mpsc::Sender<()>)); }
        }
    }
}
