use std::os::fd::BorrowedFd;
use std::sync::atomic::{AtomicBool, AtomicPtr};
use std::sync::Arc;

use super::*;

fn remove_recommendations(
    store: &gio::ListStore,
    predicate: impl Fn(&crate::core::model::Video) -> bool,
) {
    for index in (0..store.n_items()).rev() {
        let remove = store
            .item(index)
            .and_downcast::<glib::BoxedAnyObject>()
            .map(|object| {
                let data = object.borrow::<VideoItem>();
                predicate(&data.video)
            })
            .unwrap_or(false);
        if remove {
            store.remove(index);
        }
    }
}

impl Player {
    pub fn new(app_ctx: AppContext, top_bar: GtkBox, sidebar_revealer: gtk::Revealer) -> Self {
        let current_video = Rc::new(RefCell::new(None::<Video>));
        let active = Rc::new(Cell::new(false));
        let widget = GtkBox::new(Orientation::Vertical, 0);
        widget.set_hexpand(true);
        widget.set_vexpand(true);

        let watch_body = GtkBox::new(Orientation::Horizontal, 16);
        watch_body.set_hexpand(true);
        watch_body.set_vexpand(true);
        widget.append(&watch_body);

        let main_column = GtkBox::new(Orientation::Vertical, 0);
        main_column.set_hexpand(true);
        watch_body.append(&main_column);

        let video_overlay = Overlay::new();
        video_overlay.set_hexpand(true);
        video_overlay.set_vexpand(true);

        let gl_area = GLArea::builder().hexpand(true).vexpand(true).build();
        gl_area.set_required_version(3, 2);
        video_overlay.set_child(Some(&gl_area));

        let status_label = Label::new(Some(i18n::tr("loading")));
        status_label.add_css_class("title-2");
        status_label.set_halign(gtk::Align::Center);
        status_label.set_valign(gtk::Align::Center);
        video_overlay.add_overlay(&status_label);

        let session_id = format!("{}-{}", crate::core::now_secs(), std::process::id());
        let subscription_scope = Rc::new(TaskRunner::new().scope());
        let history_state = HistoryState::new(
            app_ctx.watch_history.clone(),
            current_video.clone(),
            session_id,
            status_label.clone(),
        );
        let subscribed_channels = Rc::new(RefCell::new(HashSet::new()));
        let subscription_checked = Rc::new(RefCell::new(HashSet::new()));
        match app_ctx.subscription.list() {
            Ok(channels) => {
                let mut subscribed = subscribed_channels.borrow_mut();
                let mut checked = subscription_checked.borrow_mut();
                for channel in channels {
                    checked.insert(channel.id.clone());
                    subscribed.insert(channel.id);
                }
            }
            Err(error) => eprintln!("[play] gagal memuat status subscription: {error:#}"),
        }

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

        let quality = Rc::new(Cell::new(
            app_ctx
                .settings
                .get("quality")
                .ok()
                .flatten()
                .and_then(|value| StreamQuality::from_setting(&value))
                .unwrap_or(StreamQuality::Height(720)),
        ));
        let on_quality_change: Rc<RefCell<Option<QualityChangeHandler>>> =
            Rc::new(RefCell::new(None));
        let res_list = StringList::new(&[
            "Auto (terbaik)",
            "Maks. 1080p",
            "Maks. 720p",
            "Maks. 480p",
            "Maks. 360p",
        ]);
        let res_dropdown = DropDown::new(Some(res_list), gtk::Expression::NONE);
        res_dropdown.set_selected(quality.get().index());
        res_dropdown.set_tooltip_text(Some(i18n::tr("resolution")));
        let quality_state = quality.clone();
        let quality_handler = on_quality_change.clone();
        res_dropdown.connect_notify_local(Some("selected"), move |dropdown, _| {
            let Some(selected) = StreamQuality::from_index(dropdown.selected()) else {
                return;
            };
            if quality_state.get() == selected {
                return;
            }
            if let Some(handler) = quality_handler.borrow().as_ref() {
                handler(selected);
            }
        });

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

        // Pertahankan rasio 16:9 tanpa tinggi tetap agar responsif.
        video_overlay.set_vexpand(false);
        let video_frame = AspectFrame::new(0.5, 0.5, 16.0 / 9.0, false);
        video_frame.set_hexpand(true);
        video_frame.set_vexpand(false);
        video_frame.set_child(Some(&video_overlay));
        main_column.append(&video_frame);

        // Info Area
        let info_box = GtkBox::new(Orientation::Vertical, 8);
        info_box.set_margin_top(12);
        info_box.set_margin_bottom(24);
        info_box.set_margin_start(24);
        info_box.set_margin_end(24);
        info_box.set_vexpand(false);

        let title_lbl = Label::new(Some(i18n::tr("loading_title")));
        title_lbl.add_css_class("player-title");
        title_lbl.set_xalign(0.0);
        title_lbl.set_wrap(true);
        title_lbl.set_lines(2);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info_box.append(&title_lbl);

        let action_row = GtkBox::new(Orientation::Horizontal, 12);
        let on_channel_click: Rc<RefCell<Option<ChannelClickCallback>>> =
            Rc::new(RefCell::new(None));

        let ch_box = GtkBox::new(Orientation::Horizontal, 12);
        let avatar_lbl = Label::new(Some("C"));
        avatar_lbl.add_css_class("avatar");
        avatar_lbl.set_size_request(40, 40);
        avatar_lbl.set_valign(gtk::Align::Center);
        avatar_lbl.set_cursor_from_name(Some("pointer"));

        let ch_text = GtkBox::new(Orientation::Vertical, 0);
        let ch_name_lbl = Label::new(Some(i18n::tr("loading_channel")));
        ch_name_lbl.add_css_class("player-channel");
        ch_name_lbl.set_xalign(0.0);
        ch_name_lbl.set_cursor_from_name(Some("pointer"));
        let ch_subs = Label::new(Some(""));
        ch_subs.add_css_class("player-subs");
        ch_subs.set_xalign(0.0);
        ch_text.append(&ch_name_lbl);
        ch_text.append(&ch_subs);

        ch_box.append(&avatar_lbl);
        ch_box.append(&ch_text);
        action_row.append(&ch_box);

        let btn_sub = Button::with_label(i18n::tr("subscribe"));
        btn_sub.add_css_class("btn-subscribe");
        action_row.append(&btn_sub);

        let spacer2 = GtkBox::new(Orientation::Horizontal, 0);
        spacer2.set_hexpand(true);
        action_row.append(&spacer2);

        let btn_like = Button::with_label("👍");
        btn_like.add_css_class("btn-action");
        let btn_not_interested = Button::with_label(i18n::tr("not_interested"));
        btn_not_interested.add_css_class("btn-action");
        btn_not_interested.set_tooltip_text(Some(i18n::tr("not_interested")));
        let btn_hide_channel = Button::with_label(i18n::tr("hide_channel"));
        btn_hide_channel.add_css_class("btn-action");
        btn_hide_channel.set_tooltip_text(Some(i18n::tr("hide_channel")));
        let btn_share = Button::with_label(i18n::tr("share"));
        btn_share.add_css_class("btn-action");
        action_row.append(&btn_like);
        action_row.append(&btn_not_interested);
        action_row.append(&btn_hide_channel);
        action_row.append(&btn_share);

        info_box.append(&action_row);
        main_column.append(&info_box);

        let up_next = GtkBox::new(Orientation::Vertical, 0);
        up_next.add_css_class("up-next");
        up_next.set_width_request(340);
        let up_next_title = Label::builder()
            .label(i18n::tr("up_next"))
            .xalign(0.0)
            .build();
        up_next_title.add_css_class("up-next-title");
        up_next.append(&up_next_title);
        let up_next_scroll = ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(false)
            .vexpand(true)
            .build();
        let up_next_store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let feedback_feed = app_ctx.feed.clone();
        let feedback_current = current_video.clone();
        let feedback_store = up_next_store.clone();
        btn_not_interested.connect_clicked({
            let feed = feedback_feed.clone();
            let current = feedback_current.clone();
            let store = feedback_store.clone();
            move |_| {
                let Some(video) = current.borrow().clone() else {
                    return;
                };
                if feed
                    .add_feedback(&FeedbackTarget::Video(video.id.clone()))
                    .is_ok()
                {
                    remove_recommendations(&store, |candidate| candidate.id == video.id);
                }
            }
        });
        btn_hide_channel.connect_clicked({
            let feed = feedback_feed.clone();
            let current = feedback_current.clone();
            let store = feedback_store.clone();
            move |_| {
                let Some(channel_id) = current
                    .borrow()
                    .as_ref()
                    .and_then(|video| video.channel_id.clone())
                else {
                    return;
                };
                if feed
                    .add_feedback(&FeedbackTarget::Channel(channel_id.clone()))
                    .is_ok()
                {
                    remove_recommendations(&store, |candidate| {
                        candidate.channel_id.as_deref() == Some(channel_id.as_str())
                    });
                }
            }
        });
        let up_next_selection = SingleSelection::builder().model(&up_next_store).build();
        let up_next_list = ListView::builder().model(&up_next_selection).build();
        up_next_list.set_single_click_activate(true);
        let up_next_factory = SignalListItemFactory::new();
        up_next_factory.connect_setup(|_, list_item| {
            let item = list_item.downcast_ref::<ListItem>().unwrap();
            let row = GtkBox::new(Orientation::Horizontal, 8);
            row.add_css_class("up-next-item");
            let (frame, _overlay, _picture) = home::thumbnail_frame();
            frame.add_css_class("up-next-thumb");
            row.append(&frame);
            let info = GtkBox::new(Orientation::Vertical, 4);
            info.set_hexpand(true);
            let title = Label::builder()
                .xalign(0.0)
                .wrap(true)
                .lines(2)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            title.add_css_class("video-title");
            let channel = Label::builder()
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            channel.add_css_class("video-meta");
            info.append(&title);
            info.append(&channel);
            row.append(&info);
            item.set_child(Some(&row));
        });
        up_next_factory.connect_bind(|_, list_item| {
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
            let info = row.last_child().and_downcast::<GtkBox>().unwrap();
            let title = info.first_child().and_downcast::<Label>().unwrap();
            let channel = title.next_sibling().and_downcast::<Label>().unwrap();
            title.set_text(&data.video.title);
            channel.set_text(data.video.channel_name.as_deref().unwrap_or("Unknown"));
        });
        up_next_list.set_factory(Some(&up_next_factory));
        let on_up_next: Rc<RefCell<Option<VideoClickCallback>>> = Rc::new(RefCell::new(None));
        let on_subscription_change: Rc<RefCell<Option<SubscriptionChangeHandler>>> =
            Rc::new(RefCell::new(None));
        let up_next_click = on_up_next.clone();
        let up_next_selection_click = up_next_selection.clone();
        up_next_list.connect_activate(move |_, position| {
            if let Some(object) = up_next_selection_click
                .item(position)
                .and_downcast::<glib::BoxedAnyObject>()
            {
                let video_id = object.borrow::<VideoItem>().video.id.clone();
                if let Some(callback) = up_next_click.borrow().as_ref() {
                    callback(video_id);
                }
            }
        });
        up_next_scroll.set_child(Some(&up_next_list));
        up_next.append(&up_next_scroll);
        watch_body.append(&up_next);

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
                    }
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
        let pending_resume: Rc<RefCell<Option<(f64, bool)>>> = Rc::new(RefCell::new(None));
        let pending_resume_handler = pending_resume.clone();
        let mpv = unsafe { mpv_create() };
        let mpv_initialized = Rc::new(Cell::new(false));
        let set_opt = |name: &str, val: &str| {
            let n = CString::new(name).unwrap();
            let v = CString::new(val).unwrap();
            unsafe {
                mpv_set_option_string(mpv, n.as_ptr(), v.as_ptr());
            }
        };
        set_opt("vo", "libmpv");
        set_opt("hwdec", "auto-safe");
        set_opt("keep-open", "yes");
        set_opt("cache", "yes");
        set_opt("hr-seek", "exact");
        set_opt("input-default-bindings", "no");
        set_opt("input-vo-keyboard", "no");
        set_opt("user-agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36");
        // FASTLOAD: matikan hook yang memicu subprocess & ~2-3 detik delay per load
        set_opt("ytdl", "no");
        set_opt("load-scripts", "no");
        set_opt("auto-profiles", "no");

        // Event drain + deferred audio-add (production, no logging)
        {
            let wakeup_fd = unsafe { mpv_get_wakeup_pipe(mpv) };
            if wakeup_fd >= 0 {
                let wakeup_stream =
                    unsafe { gio::UnixInputStream::with_fd(BorrowedFd::borrow_raw(wakeup_fd)) };
                let mpv_dbg = mpv;
                let history_end = history_state.clone();
                glib::MainContext::default().spawn_local(async move {
                    loop {
                        let buffer = vec![0u8; 64];
                        let read = wakeup_stream
                            .read_future(buffer, glib::Priority::DEFAULT)
                            .await;
                        match read {
                            Ok((_, 0)) => break,
                            Ok(_) => {}
                            Err(_) => break,
                        }
                        loop {
                            let event = unsafe { mpv_wait_event(mpv_dbg, 0.0) };
                            if event.is_null() {
                                break;
                            }
                            let event = unsafe { &*event };
                            if event.event_id == 0 {
                                break;
                            }
                            if event.event_id == 7 {
                                let end_file = unsafe { &*event.data.cast::<mpv_event_end_file>() };
                                history_end.flush(mpv_position(mpv_dbg));
                                eprintln!(
                                    "[play] end-file reason={} error={}",
                                    end_file.reason, end_file.error
                                );
                            }
                            if event.event_id == 8 {
                                eprintln!("[play] file-loaded");
                                history_end.activate();
                                if let Some(audio) = pending_audio_handler.borrow_mut().take() {
                                    let command = format!("audio-add \"{audio}\"");
                                    if let Ok(command) = CString::new(command) {
                                        unsafe {
                                            mpv_command_string(mpv_dbg, command.as_ptr());
                                        }
                                    }
                                }
                                let resume = pending_resume_handler.borrow_mut().take();
                                match resume {
                                    Some((position, paused)) => {
                                        history_end.set_paused(paused);
                                        let seek =
                                            CString::new(format!("seek {position:.3} absolute"))
                                                .unwrap();
                                        unsafe {
                                            mpv_command_string(mpv_dbg, seek.as_ptr());
                                        }
                                        let pause = CString::new(if paused {
                                            "set pause yes"
                                        } else {
                                            "set pause no"
                                        })
                                        .unwrap();
                                        unsafe {
                                            mpv_command_string(mpv_dbg, pause.as_ptr());
                                        }
                                    }
                                    None => {
                                        history_end.set_paused(false);
                                        let unpause = CString::new("set pause no").unwrap();
                                        unsafe {
                                            mpv_command_string(mpv_dbg, unpause.as_ptr());
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }

        unsafe {
            let n_vol = CString::new("volume").unwrap();
            let mut v: f64 = 100.0;
            mpv_set_property(
                mpv,
                n_vol.as_ptr(),
                MPV_FORMAT_DOUBLE,
                &mut v as *mut _ as *mut c_void,
            );
        }

        // Opsi A: pause saat unmap, resume saat map (respect manual pause)
        {
            let mpv_pause_state = Rc::new(RefCell::new(false));
            let mpv_vis = mpv;
            let state_unmap = mpv_pause_state.clone();
            let history_unmap = history_state.clone();
            widget.connect_unmap(move |_| {
                history_unmap.flush(mpv_position(mpv_vis));
                history_unmap.set_paused(true);
                let mut pause_val: c_int = 0;
                let n_pause = CString::new("pause").unwrap();
                unsafe {
                    mpv_get_property(
                        mpv_vis,
                        n_pause.as_ptr(),
                        3,
                        &mut pause_val as *mut _ as *mut c_void,
                    );
                }
                *state_unmap.borrow_mut() = pause_val == 1;
                let cmd = CString::new("set pause yes").unwrap();
                unsafe {
                    mpv_command_string(mpv_vis, cmd.as_ptr());
                }
            });
            let mpv_vis2 = mpv;
            let state_map = mpv_pause_state.clone();
            let history_map = history_state.clone();
            widget.connect_map(move |_| {
                if !*state_map.borrow() {
                    history_map.set_paused(false);
                    let cmd = CString::new("set pause no").unwrap();
                    unsafe {
                        mpv_command_string(mpv_vis2, cmd.as_ptr());
                    }
                }
            });
        }

        let ctx = Rc::new(RefCell::new(ptr::null_mut()));
        let mpv_ptr = mpv;
        let ctx_clone = ctx.clone();
        let render_area = Arc::new(AtomicPtr::new(ptr::null_mut()));
        let render_active = Arc::new(AtomicBool::new(true));
        let render_signal = RenderSignal {
            main_context: glib::MainContext::default(),
            pending: Arc::new(AtomicBool::new(false)),
            active: render_active.clone(),
            area: render_area.clone(),
        };
        let update_ctx = Box::into_raw(Box::new(render_signal));

        let render_area_realize = render_area.clone();
        gl_area.connect_realize(move |area| {
            render_area_realize.store(area.as_ptr().cast(), Ordering::Release);
            area.make_current();
            if area.error().is_some() {
                return;
            }
            let mut init_params = mpv_opengl_init_params {
                get_proc_address: Some(get_proc_address),
                get_proc_address_ctx: ptr::null_mut(),
                extra_exts: ptr::null(),
            };
            let mut render_params = [
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_API_TYPE,
                    data: MPV_RENDER_API_TYPE_OPENGL.as_ptr() as *mut c_void,
                },
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                    data: &mut init_params as *mut _ as *mut c_void,
                },
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_INVALID,
                    data: ptr::null_mut(),
                },
            ];
            let mut render_ctx: *mut mpv_render_context = ptr::null_mut();
            let res = unsafe {
                mpv_render_context_create(&mut render_ctx, mpv_ptr, render_params.as_mut_ptr())
            };
            if res < 0 {
                return;
            }
            *ctx_clone.borrow_mut() = render_ctx;
            unsafe {
                mpv_render_context_set_update_callback(
                    render_ctx,
                    Some(update_callback),
                    update_ctx.cast(),
                );
            }
            // LOGIC-01(A): force paint cycle pertama setelah recreate
            area.queue_render();
        });

        // FIX: Bersihkan context saat widget dihancurkan (resize/fullscreen)
        let ctx_unrealize = ctx.clone();
        let render_area_unrealize = render_area.clone();
        gl_area.connect_unrealize(move |_| {
            render_area_unrealize.store(ptr::null_mut(), Ordering::Release);
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
                    let p = get_proc_address(ptr::null_mut(), c"glClearColor".as_ptr());
                    if !p.is_null() {
                        let f: unsafe extern "C" fn(f32, f32, f32, f32) = std::mem::transmute(p);
                        f(0.0, 0.0, 0.0, 1.0);
                        let c = get_proc_address(ptr::null_mut(), c"glClear".as_ptr());
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
                let get_gl_int_ptr = get_proc_address(ptr::null_mut(), c"glGetIntegerv".as_ptr());
                if !get_gl_int_ptr.is_null() {
                    let get_gl_int: unsafe extern "C" fn(u32, *mut i32) =
                        std::mem::transmute(get_gl_int_ptr);
                    get_gl_int(0x8CA6, &mut current_fbo);
                }
            }
            let scale = area.scale_factor();
            let mut fbo = mpv_opengl_fbo {
                fbo: current_fbo,
                w: area.width() * scale,
                h: area.height() * scale,
                internal_format: 0,
            };
            let mut flip_y: c_int = 1;
            let mut render_params = [
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_OPENGL_FBO,
                    data: &mut fbo as *mut _ as *mut c_void,
                },
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_FLIP_Y,
                    data: &mut flip_y as *mut _ as *mut c_void,
                },
                mpv_render_param {
                    type_: MPV_RENDER_PARAM_INVALID,
                    data: ptr::null_mut(),
                },
            ];
            unsafe {
                mpv_render_context_render(render_ctx, render_params.as_mut_ptr());
                // LOGIC-09: release internal reference, tanpa ini mpv stall & white screen
                mpv_render_context_report_swap(render_ctx);
            }

            glib::Propagation::Stop
        });
        // Render wake-up ditangani callback MPV secara event-driven.

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
                mpv_set_property(
                    mpv_vol,
                    n_vol.as_ptr(),
                    MPV_FORMAT_DOUBLE,
                    &mut val as *mut _ as *mut c_void,
                );
            }
            vol_pct_clone.set_text(&format!("{:.0}%", v));
        });

        let is_seeking = Rc::new(RefCell::new(false));
        let mpv_play = mpv;
        let play_icon_clone = play_icon.clone();
        let history_pause = history_state.clone();
        play_btn.connect_clicked(move |_| {
            let mut pause_val: c_int = 0;
            let n_pause = CString::new("pause").unwrap();
            unsafe {
                mpv_get_property(
                    mpv_play,
                    n_pause.as_ptr(),
                    3,
                    &mut pause_val as *mut _ as *mut c_void,
                );
                history_pause.set_paused(pause_val == 1);
                let cmd = CString::new(if pause_val == 1 {
                    "set pause no"
                } else {
                    "set pause yes"
                })
                .unwrap();
                mpv_command_string(mpv_play, cmd.as_ptr());
            }
            history_pause.flush(mpv_position(mpv_play));
            if pause_val == 0 {
                play_icon_clone.set_icon_name(Some("media-playback-pause-symbolic"));
            } else {
                play_icon_clone.set_icon_name(Some("media-playback-start-symbolic"));
            }
        });

        let mpv_seek = mpv;
        let is_seeking_clone = is_seeking.clone();
        seek_slider.connect_change_value(move |_, _, val| {
            *is_seeking_clone.borrow_mut() = true;
            let cmd = format!("seek {:.2} absolute", val);
            let c_cmd = CString::new(cmd).unwrap();
            unsafe {
                mpv_command_string(mpv_seek, c_cmd.as_ptr());
            }
            glib::Propagation::Proceed
        });

        let is_seeking_value = is_seeking.clone();
        seek_slider.connect_value_changed(move |_| {
            *is_seeking_value.borrow_mut() = false;
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
                    if is_fs {
                        win.fullscreen();
                    } else {
                        win.unfullscreen();
                    }
                }
            }
        });

        // FS-01/FS-02/FS-04: sync layout saat fullscreen berubah (cover keyboard/WM shortcut juga)
        {
            let top_c = top_bar.clone();
            let side_c = sidebar_revealer.clone();
            let info_c = info_box.clone();
            let frame_c = video_frame.clone();
            let up_next_c = up_next.clone();
            let pw_c = widget.clone();
            let gl_c = gl_area.clone();
            widget.connect_realize(move |w| {
                if let Some(win) = w.root().and_downcast::<gtk::Window>() {
                    let top = top_c.clone();
                    let side = side_c.clone();
                    let info = info_c.clone();
                    let frame = frame_c.clone();
                    let up_next = up_next_c.clone();
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
                        up_next.set_visible(!is_fs);
                        frame.set_vexpand(is_fs);
                        pw.set_margin_top(if is_fs { 0 } else { 24 });
                        pw.set_margin_bottom(if is_fs { 0 } else { 24 });
                        pw.set_margin_start(if is_fs { 0 } else { 24 });
                        pw.set_margin_end(if is_fs { 0 } else { 24 });
                        pw.set_vexpand(true);

                        let gl2 = gl.clone();
                        for ms in [16u64, 100, 300, 600] {
                            let gl3 = gl2.clone();
                            glib::timeout_add_local_once(
                                std::time::Duration::from_millis(ms),
                                move || {
                                    gl3.queue_render();
                                },
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

        let history_timer = history_state.clone();
        let active_timer = active.clone();

        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if !active_timer.get() {
                return glib::ControlFlow::Continue;
            }
            let mut pos: f64 = 0.0;
            let mut dur: f64 = 0.0;
            let mut pause_val: c_int = 0;
            unsafe {
                let n_pos = CString::new("time-pos").unwrap();
                mpv_get_property(
                    mpv_time,
                    n_pos.as_ptr(),
                    MPV_FORMAT_DOUBLE,
                    &mut pos as *mut _ as *mut c_void,
                );
                let n_dur = CString::new("duration").unwrap();
                mpv_get_property(
                    mpv_time,
                    n_dur.as_ptr(),
                    MPV_FORMAT_DOUBLE,
                    &mut dur as *mut _ as *mut c_void,
                );
                let n_pause = CString::new("pause").unwrap();
                mpv_get_property(
                    mpv_time,
                    n_pause.as_ptr(),
                    3,
                    &mut pause_val as *mut _ as *mut c_void,
                );
            }
            if pause_val == 1 {
                play_icon_timer.set_icon_name(Some("media-playback-start-symbolic"));
            } else {
                play_icon_timer.set_icon_name(Some("media-playback-pause-symbolic"));
            }
            if dur > 0.0 {
                if (seek_clone.value() - dur).abs() > 1.0 && seek_clone.adjustment().upper() != dur
                {
                    seek_clone.adjustment().set_upper(dur);
                }
                if !*is_seeking_clone3.borrow() {
                    seek_clone.set_value(pos);
                }
                time_lbl_clone.set_text(&format!("{} / {}", format_time(pos), format_time(dur)));
            }

            history_timer.checkpoint(pos, pause_val == 1);

            glib::ControlFlow::Continue
        });

        let current_vid_sub = current_video.clone();
        let subscription = app_ctx.subscription.clone();
        let subscription_scope = subscription_scope.clone();
        let subscription_task_scope = subscription_scope.clone();
        let btn_sub_clone = btn_sub.clone();
        let subscribed_channels_click = subscribed_channels.clone();
        let subscribed_channels_state = subscribed_channels.clone();
        let subscription_hook = on_subscription_change.clone();
        btn_sub.connect_clicked(move |_| {
            let v_opt = current_vid_sub.borrow().clone();

            if let Some(v) = v_opt {
                if let Some(ch_id) = v.channel_id.clone() {
                    let is_subbed = subscription
                        .is_subscribed(&ch_id)
                        .unwrap_or_else(|_| subscribed_channels_state.borrow().contains(&ch_id));

                    if is_subbed {
                        if subscription.unsubscribe(&ch_id).is_ok() {
                            subscribed_channels_click.borrow_mut().remove(&ch_id);
                            btn_sub_clone.set_label(i18n::tr("subscribe"));
                            btn_sub_clone.remove_css_class("btn-subscribed");
                        }
                    } else {
                        let channel = crate::core::model::Channel {
                            id: ch_id.clone(),
                            name: v.channel_name.clone().unwrap_or_else(|| "Unknown".into()),
                            avatar_url: None,
                        };
                        if subscription.subscribe(&channel).is_ok() {
                            subscribed_channels_click.borrow_mut().insert(ch_id.clone());
                            btn_sub_clone.set_label(i18n::tr("subscribed"));
                            btn_sub_clone.add_css_class("btn-subscribed");

                            let service = subscription.clone();
                            let channel_task = channel.clone();
                            let _handle = subscription_task_scope.spawn(
                                std::time::Duration::from_secs(90),
                                move |ctx| {
                                    service.refresh_channel(
                                        &channel_task,
                                        15,
                                        &ctx.cancellation_token(),
                                    )
                                },
                            );
                        }
                    }
                }
                if let Some(callback) = subscription_hook.borrow().as_ref() {
                    callback();
                }
            }
        });

        Self {
            widget,
            status_label,
            status_key: Rc::new(Cell::new(None)),
            status_values: Rc::new(RefCell::new(Vec::new())),
            mpv,
            mpv_initialized,
            ctx,
            update_ctx,
            current_video,
            history_state,
            subscription: app_ctx.subscription.clone(),
            subscribed_channels,
            pending_audio,
            pending_resume,
            title_lbl,
            ch_name_lbl,
            avatar_lbl,
            btn_sub,
            btn_share,
            btn_not_interested,
            btn_hide_channel,
            up_next_title,
            on_channel_click,
            watch_body,
            up_next,
            up_next_scroll,
            up_next_store,
            on_up_next,
            on_subscription_change,
            res_dropdown,
            quality,
            active,
            on_quality_change,
        }
    }
}
