use super::*;

pub fn build(app: &Application, t0: Instant, conn: AppContext, theme: ThemeController) {
    let saved_language = conn.settings.get("language").ok().flatten();
    let language_controller =
        LanguageController::new(Language::from_setting(saved_language.as_deref()));
    let window = glib::Object::new::<AppWindow>();
    window.set_application(Some(app));
    window.set_title(Some("elxTube"));
    window.set_icon_name(Some("elxtube"));
    window.set_default_size(1200, 800);

    let main_layout = GtkBox::new(Orientation::Vertical, 0);
    window.set_child(Some(&main_layout));

    let top_bar = GtkBox::new(Orientation::Horizontal, 0);
    top_bar.add_css_class("topbar");
    main_layout.append(&top_bar);

    let top_bar_start = GtkBox::new(Orientation::Horizontal, 8);
    top_bar_start.add_css_class("topbar-start");
    let top_bar_center = GtkBox::new(Orientation::Horizontal, 0);
    top_bar_center.add_css_class("topbar-center");
    let top_bar_end = GtkBox::new(Orientation::Horizontal, 8);
    top_bar_end.add_css_class("topbar-end");

    let menu_button = Button::from_icon_name("open-menu-symbolic");
    menu_button.set_tooltip_text(Some(i18n::tr("guide")));
    menu_button.add_css_class("yt-icon-button");
    let back_button = Button::from_icon_name("go-previous-symbolic");
    back_button.set_tooltip_text(Some(i18n::tr("back")));
    back_button.add_css_class("yt-icon-button");
    back_button.set_sensitive(false);
    let brand = Label::builder()
        .label("<span foreground='#ff0033'><b>elx</b></span><span><b>Tube</b></span>")
        .use_markup(true)
        .build();
    brand.add_css_class("brand");

    let search = Entry::builder()
        .placeholder_text(i18n::tr("search"))
        .hexpand(true)
        .build();
    search.add_css_class("search-entry");
    search.set_max_width_chars(600);
    let search_button = Button::from_icon_name("system-search-symbolic");
    search_button.set_tooltip_text(Some(i18n::tr("search")));
    search_button.add_css_class("search-button");
    let search_shell = GtkBox::new(Orientation::Horizontal, 0);
    search_shell.add_css_class("search-shell");
    search_shell.append(&search);
    search_shell.append(&search_button);

    let theme_button = Button::from_icon_name("weather-clear-night-symbolic");
    theme_button.set_tooltip_text(Some(i18n::tr("theme")));
    theme_button.add_css_class("yt-icon-button");
    let language_button = Button::with_label(language_controller.current().short_label());
    language_button.set_tooltip_text(Some(i18n::tr("language")));
    language_button.add_css_class("account-button");

    top_bar_start.append(&menu_button);
    top_bar_start.append(&back_button);
    top_bar_start.append(&brand);
    top_bar_center.append(&search_shell);
    top_bar_end.append(&theme_button);
    top_bar_end.append(&language_button);
    language_button.connect_clicked({
        let controller = language_controller.clone();
        let settings = conn.settings.clone();
        move |button| {
            let next = controller.current().toggled();
            if controller.set(next) {
                button.set_label(next.short_label());
                let _ = settings.set(
                    "language",
                    match next {
                        Language::Indonesian => "id",
                        Language::English => "en",
                    },
                );
            }
        }
    });
    language_controller.bind({
        let menu_button = menu_button.clone();
        let back_button = back_button.clone();
        let search = search.clone();
        let search_button = search_button.clone();
        let theme_button = theme_button.clone();
        let language_button = language_button.clone();
        move |language| {
            menu_button.set_tooltip_text(Some(i18n::tr("guide")));
            back_button.set_tooltip_text(Some(i18n::tr("back")));
            search.set_placeholder_text(Some(i18n::tr("search")));
            search_button.set_tooltip_text(Some(i18n::tr("search")));
            theme_button.set_tooltip_text(Some(i18n::tr("theme")));
            language_button.set_tooltip_text(Some(i18n::tr("language")));
            language_button.set_label(language.short_label());
        }
    });
    top_bar.append(&top_bar_start);
    top_bar.append(&top_bar_center);
    top_bar.append(&top_bar_end);

    let body = GtkBox::new(Orientation::Horizontal, 0);
    body.set_vexpand(true);
    main_layout.append(&body);

    let sidebar_inner = GtkBox::new(Orientation::Vertical, 4);
    sidebar_inner.add_css_class("sidebar");
    let sidebar_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::None)
        .child(&sidebar_inner)
        .build();

    let stack = Stack::builder()
        .transition_type(StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();
    let navigation = Rc::new(RefCell::new(NavigationStack::new(Route::Home)));
    let channel_handler: Rc<RefCell<Option<ChannelOpenHandler>>> = Rc::new(RefCell::new(None));
    let search_generation = Rc::new(Cell::new(0u64));
    let channel_generation = Rc::new(Cell::new(0u64));
    let playlist_generation = Rc::new(Cell::new(0u64));
    let playback_generation = Rc::new(Cell::new(0u64));
    let task_runner = TaskRunner::new();
    let stream_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let playback_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let channel_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let playlist_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let search_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let subscription_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let discover_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let feed_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let cache_scope: Rc<TaskScope> = Rc::new(task_runner.scope());
    let current_video_id = Rc::new(RefCell::new(None::<String>));
    let pipeline = ThumbnailPipeline::new(conn.thumbnail.clone());

    let player = Rc::new(Player::new(
        conn.clone(),
        top_bar.clone(),
        sidebar_revealer.clone(),
    ));
    let selected_quality = Rc::new(Cell::new(player.quality()));
    player.widget.set_margin_top(24);
    player.widget.set_margin_bottom(24);
    player.widget.set_margin_start(24);
    player.widget.set_margin_end(24);
    stack.add_named(&player.widget, Some("player"));
    {
        let player = player.clone();
        stack.connect_notify_local(Some("visible-child-name"), move |stack, _| {
            player.set_active(stack.visible_child_name() == Some("player".into()));
        });
    }

    let load_stream: LoadStream = {
        let player = player.clone();
        let selected_quality = selected_quality.clone();
        let stream_scope = stream_scope.clone();
        let playback = conn.playback.clone();
        Rc::new(move |video_id, resume| {
            let quality = selected_quality.get();
            player.pause();
            match quality {
                StreamQuality::Auto => player.set_loading_key("best_quality", &[]),
                StreamQuality::Height(height) => {
                    player.set_loading_key("fetching_video", &[("quality", &height.to_string())])
                }
            }
            let id = video_id.clone();
            let playback_task = playback.clone();
            let handle = stream_scope.spawn(std::time::Duration::from_secs(90), move |ctx| {
                playback_task.resolve_stream(&id, quality, &ctx.cancellation_token())
            });
            let player = player.clone();
            glib::MainContext::default().spawn_local(async move {
                match handle.completion().await {
                    TaskOutcome::Completed((url, audio)) => {
                        player.play_with_resume(&url, audio.as_deref(), resume);
                    }
                    TaskOutcome::Failed(error) => {
                        player.set_loading_key("stream_failed", &[("error", &error)]);
                    }
                    TaskOutcome::TimedOut => {
                        player.set_loading_key("stream_timeout", &[]);
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => {}
                }
            });
        })
    };

    player.set_on_quality_change({
        let player = player.clone();
        let selected_quality = selected_quality.clone();
        let current_video_id = current_video_id.clone();
        let conn = conn.clone();
        let load_stream = load_stream.clone();
        move |quality| {
            player.apply_quality(quality);
            selected_quality.set(quality);
            let _ = conn.settings.set("quality", &quality.setting());
            let Some(video_id) = current_video_id.borrow().clone() else {
                return;
            };
            let resume = (player.position(), player.is_paused());
            load_stream(video_id, Some(resume));
        }
    });

    let on_activate = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let player = player.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let playback_scope = playback_scope.clone();
        let cache_scope = cache_scope.clone();
        let generation = playback_generation.clone();
        let back_button = back_button.clone();
        let current_video_id = current_video_id.clone();
        let load_stream = load_stream.clone();
        Rc::new(move |video_id: String| {
            let current = navigation.borrow().current().clone();
            navigation.borrow_mut().push(Route::Watch(video_id.clone()));
            let _ = current;
            player.prepare();
            stack.set_visible_child_name("player");
            back_button.set_sensitive(true);
            *current_video_id.borrow_mut() = Some(video_id.clone());
            player.begin_video(&video_id);
            player.pause();
            player.set_loading_key("connecting", &[]);
            let resume = conn
                .watch_history
                .resume_position(&video_id)
                .ok()
                .flatten()
                .map(|position| (position, false));
            load_stream(video_id.clone(), resume);
            let request_generation = generation.get().wrapping_add(1);
            generation.set(request_generation);
            let id_metadata = video_id.clone();
            let metadata_id = id_metadata.clone();
            let playback = conn.playback.clone();
            let handle = playback_scope.spawn(std::time::Duration::from_secs(45), move |ctx| {
                playback.metadata(&id_metadata, &ctx.cancellation_token())
            });
            let player = player.clone();
            let conn_recommendations = conn.clone();
            let pipeline = pipeline.clone();
            let active_video_id = current_video_id.clone();
            let cache_scope = cache_scope.clone();
            glib::MainContext::default().spawn_local(async move {
                let video = match handle.completion().await {
                    TaskOutcome::Completed(video) => video,
                    TaskOutcome::Failed(error) => {
                        eprintln!("[metadata] {error}");
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        eprintln!("[metadata] request timed out");
                        return;
                    }
                };
                let active_id = active_video_id.borrow().clone();
                if active_id.as_deref() != Some(metadata_id.as_str()) {
                    return;
                }
                let worker_conn = conn_recommendations.clone();
                let worker_video = video.clone();
                let recommendations_handle = cache_scope
                    .spawn(std::time::Duration::from_secs(15), move |_| {
                        worker_conn.feed.similar_up_next(&worker_video, 12)
                    });
                let recommendations = match recommendations_handle.completion().await {
                    TaskOutcome::Completed(recommendations) => recommendations,
                    _ => {
                        player.update_metadata(video);
                        return;
                    }
                };
                let textures =
                    load_thumbnails(conn_recommendations.feed.clone(), recommendations.clone())
                        .await;
                for recommendation in &recommendations {
                    if textures.contains_key(&recommendation.id) {
                        continue;
                    }
                    let Some(url) = recommendation.thumbnail_url.clone() else {
                        continue;
                    };
                    let id = recommendation.id.clone();
                    let player = player.clone();
                    let generation = request_generation;
                    pipeline.request(
                        id.clone(),
                        url,
                        generation,
                        move |_request_id, result_generation, result| {
                            if result_generation != generation {
                                return;
                            }
                            if let Ok(asset) = result {
                                let texture = thumbnail_texture(&asset);
                                player.update_recommendation_texture(&id, texture);
                            }
                        },
                    );
                }
                player.set_recommendations(recommendations, textures);
                player.update_metadata(video);
            });
        })
    };

    let on_channel_click: Rc<dyn Fn(String, String)> = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let handler = channel_handler.clone();
        Rc::new(move |channel_id: String, channel_name: String| {
            if let Some(callback) = handler.borrow().as_ref() {
                callback(channel_id, channel_name);
            } else {
                stack.set_visible_child_name("channel");
                navigation.borrow_mut().push(Route::Channel(channel_id));
            }
        })
    };

    let playlist_open_handler: Rc<RefCell<Option<PlaylistOpenHandler>>> =
        Rc::new(RefCell::new(None));
    let on_playlist_open: Rc<dyn Fn(String, String)> = {
        let handler = playlist_open_handler.clone();
        Rc::new(move |playlist_id, playlist_title| {
            if let Some(open) = handler.borrow().as_ref() {
                open(playlist_id, playlist_title);
            }
        })
    };
    let playlist_status = StatusView::new();
    let (playlist_content, playlist_store, playlist_title, playlist_count) = playlist::build_detail(
        on_activate.clone(),
        on_channel_click.clone(),
        on_playlist_open.clone(),
    );
    let playlist_wrapper = GtkBox::new(Orientation::Vertical, 0);
    playlist_wrapper.append(&playlist_content);
    playlist_wrapper.append(playlist_status.widget());
    stack.add_named(&playlist_wrapper, Some("playlist"));

    let channel_view = channel::build(
        t0,
        on_activate.clone(),
        on_channel_click.clone(),
        on_playlist_open,
    );
    let channel_widget = channel_view.widget.clone();
    let channel_store = channel_view.store.clone();
    let channel_grid = channel_view.grid.clone();
    let channel_title = channel_view.title.clone();
    let channel_avatar = channel_view.avatar.clone();
    let channel_banner = channel_view.banner.clone();
    let channel_handle = channel_view.handle.clone();
    let channel_description = channel_view.description.clone();
    let channel_subscribe = channel_view.subscribe.clone();
    let channel_playlist_store = channel_view.playlist_store.clone();
    let channel_status = StatusView::new();
    let channel_wrapper = GtkBox::new(Orientation::Vertical, 0);
    channel_wrapper.append(&channel_widget);
    channel_wrapper.append(channel_status.widget());
    stack.add_named(&channel_wrapper, Some("channel"));

    let channel_current = Rc::new(RefCell::new(None::<(String, String)>));

    let playlist_open: Rc<dyn Fn(String, String)> = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let back_button = back_button.clone();
        let store = playlist_store.clone();
        let title = playlist_title.clone();
        let count = playlist_count.clone();
        let status = playlist_status.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let scope = playlist_scope.clone();
        let generation = playlist_generation.clone();
        Rc::new(move |playlist_id: String, playlist_title: String| {
            let Some(playlist_id) = conn.playlist.normalize_id(&playlist_id) else {
                return;
            };
            let request_generation = generation.get().wrapping_add(1);
            generation.set(request_generation);
            stack.set_visible_child_name("playlist");
            back_button.set_sensitive(true);
            navigation
                .borrow_mut()
                .push(Route::Playlist(playlist_id.clone(), playlist_title.clone()));
            title.set_text(&playlist_title);
            count.set_text(i18n::tr("loading"));
            store.remove_all();
            status.loading_key("loading_playlist");
            let service = conn.playlist.clone();
            let id = playlist_id.clone();
            let handle = scope.spawn(std::time::Duration::from_secs(90), move |ctx| {
                service.load(&id, 200, &ctx.cancellation_token())
            });
            let store = store.clone();
            let title = title.clone();
            let count = count.clone();
            let status = status.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let current_generation = generation.clone();
            glib::MainContext::default().spawn_local(async move {
                let (playlist, videos) = match handle.completion().await {
                    TaskOutcome::Completed(result) => result,
                    TaskOutcome::Failed(error) => {
                        eprintln!("[playlist] {error}");
                        status.error_key("playlist_failed", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        status.error_key("playlist_timeout", &[]);
                        return;
                    }
                };
                if request_generation != current_generation.get() {
                    return;
                }
                title.set_text(&playlist.title);
                let video_count = playlist.video_count.unwrap_or(videos.len() as u64);
                count.set_text(&i18n::format(
                    "video_count",
                    &[("count", &video_count.to_string())],
                ));
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                for video in &videos {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                        video.clone(),
                        texture,
                    )));
                }
                request_missing_thumbnails(
                    &pipeline,
                    &textures,
                    &videos,
                    &store,
                    request_generation,
                );
                if videos.is_empty() {
                    status.empty_key("empty_playlist");
                } else {
                    status.hide();
                }
            });
        }) as Rc<dyn Fn(String, String)>
    };
    *playlist_open_handler.borrow_mut() = Some(playlist_open);

    channel_view.playlist_tab.connect_clicked({
        let current = channel_current.clone();
        let conn = conn.clone();
        let scope = channel_scope.clone();
        let store = channel_playlist_store.clone();
        let status = channel_status.clone();
        let pipeline = pipeline.clone();
        let generation = channel_generation.clone();
        move |_| {
            let Some((channel_id, _)) = current.borrow().clone() else {
                return;
            };
            store.remove_all();
            status.loading_key("loading_channel_playlists");
            let service = conn.playlist.clone();
            let id = channel_id.clone();
            let request_generation = generation.get();
            let handle = scope.spawn(std::time::Duration::from_secs(60), move |ctx| {
                service.channel_playlists(&id, 50, &ctx.cancellation_token())
            });
            let store = store.clone();
            let status = status.clone();
            let pipeline = pipeline.clone();
            let current_generation = generation.clone();
            glib::MainContext::default().spawn_local(async move {
                let playlists = match handle.completion().await {
                    TaskOutcome::Completed(playlists) => playlists,
                    TaskOutcome::Failed(error) => {
                        eprintln!("[channel-playlists] {error}");
                        status.error_key("channel_playlists_failed", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        status.error_key("channel_playlists_timeout", &[]);
                        return;
                    }
                };
                if request_generation != current_generation.get() {
                    return;
                }
                for (position, item) in playlists.into_iter().enumerate() {
                    let position = position as u32;
                    let playlist_id = item.id.clone();
                    store.append(&glib::BoxedAnyObject::new(playlist::PlaylistItem {
                        playlist: item,
                        texture: home::fallback_texture(),
                    }));
                    let Some(url) = store
                        .item(position)
                        .and_downcast::<glib::BoxedAnyObject>()
                        .and_then(|object| {
                            object
                                .borrow::<playlist::PlaylistItem>()
                                .playlist
                                .thumbnail_url
                                .clone()
                        })
                    else {
                        continue;
                    };
                    let store = store.clone();
                    pipeline.request(
                        playlist_id,
                        url,
                        request_generation,
                        move |_id, result_generation, result| {
                            if result_generation != request_generation {
                                return;
                            }
                            if let Ok(asset) = result {
                                playlist::set_texture(&store, position, thumbnail_texture(&asset));
                            }
                        },
                    );
                }
                if store.n_items() == 0 {
                    status.empty_key("empty_channel_playlists");
                } else {
                    status.hide();
                }
            });
        }
    });
    let sidebar_refresh_hook: Rc<RefCell<Option<SidebarRefreshHook>>> = Rc::new(RefCell::new(None));
    let subscribe_state = channel_current.clone();
    let subscribe_db = conn.clone();
    let subscribe_sidebar_hook = sidebar_refresh_hook.clone();
    let subscribe_player = player.clone();
    let subscribe_scope = subscription_scope.clone();
    let subscribe_service = conn.subscription.clone();
    channel_subscribe.connect_clicked(move |button| {
        let Some((channel_id, channel_name)) = subscribe_state.borrow().clone() else {
            return;
        };
        let subscribed = subscribe_db
            .subscription
            .is_subscribed(&channel_id)
            .unwrap_or(false);
        if subscribed {
            if subscribe_db.subscription.unsubscribe(&channel_id).is_ok() {
                button.set_label(i18n::tr("subscribe"));
                button.remove_css_class("btn-subscribed");
            }
        } else {
            let channel = crate::core::model::Channel {
                id: channel_id.clone(),
                name: channel_name,
                avatar_url: None,
            };
            if subscribe_db.subscription.subscribe(&channel).is_ok() {
                button.set_label(i18n::tr("subscribed"));
                button.add_css_class("btn-subscribed");
                let service = subscribe_service.clone();
                let refresh_channel = channel.clone();
                let _handle =
                    subscribe_scope.spawn(std::time::Duration::from_secs(90), move |ctx| {
                        service.refresh_channel(&refresh_channel, 15, &ctx.cancellation_token())
                    });
            }
        }
        if let Some(callback) = subscribe_sidebar_hook.borrow().as_ref() {
            callback();
        }
        subscribe_player.refresh_subscription_state();
    });

    let on_channel_open: Rc<dyn Fn(String, String)> = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let back_button = back_button.clone();
        let title = channel_title.clone();
        let avatar = channel_avatar.clone();
        let banner = channel_banner.clone();
        let handle = channel_handle.clone();
        let description = channel_description.clone();
        let subscribe = channel_subscribe.clone();
        let current = channel_current.clone();
        let store = channel_store.clone();
        let playlist_store = channel_playlist_store.clone();
        let conn = conn.clone();
        let generation = channel_generation.clone();
        let channel_scope = channel_scope.clone();
        let pipeline = pipeline.clone();
        let cache_scope = cache_scope.clone();
        let status = channel_status.clone();
        Rc::new(move |channel_id: String, channel_name: String| {
            let request_generation = generation.get().wrapping_add(1);
            generation.set(request_generation);
            stack.set_visible_child_name("channel");
            back_button.set_sensitive(true);
            navigation
                .borrow_mut()
                .push(Route::Channel(channel_id.clone()));
            title.set_text(if channel_name.is_empty() {
                i18n::tr("loading")
            } else {
                &channel_name
            });
            *current.borrow_mut() = Some((channel_id.clone(), channel_name.clone()));
            let subscribed = conn
                .subscription
                .is_subscribed(&channel_id)
                .unwrap_or(false);
            if subscribed {
                subscribe.set_label(i18n::tr("subscribed"));
                subscribe.add_css_class("btn-subscribed");
            } else {
                subscribe.set_label(i18n::tr("subscribe"));
                subscribe.remove_css_class("btn-subscribed");
            }
            store.remove_all();
            playlist_store.remove_all();
            status.loading_key("loading_channel_videos");
            let cached_feed = conn.feed.clone();
            let cached_channel_id = channel_id.clone();
            let cached_handle = cache_scope.spawn(std::time::Duration::from_secs(15), move |_| {
                cached_feed.channel_videos(&cached_channel_id, 30)
            });
            let cached_store = store.clone();
            let cached_conn = conn.clone();
            let cached_pipeline = pipeline.clone();
            let cached_generation = request_generation;
            let current_generation = generation.clone();
            glib::MainContext::default().spawn_local(async move {
                let cached = match cached_handle.completion().await {
                    TaskOutcome::Completed(cached) => cached,
                    _ => return,
                };
                if cached_generation != current_generation.get() {
                    return;
                }
                let textures = load_thumbnails(cached_conn.feed.clone(), cached.clone()).await;
                let cached_videos = cached.clone();
                for video in cached {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    cached_store.append(&glib::BoxedAnyObject::new(VideoItem::new(video, texture)));
                }
                request_missing_thumbnails(
                    &cached_pipeline,
                    &textures,
                    &cached_videos,
                    &cached_store,
                    cached_generation,
                );
            });
            let id = channel_id.clone();
            let display_channel_id = channel_id.clone();
            let fallback = channel_name.clone();
            let fallback_name = channel_name.clone();
            let channel_service = conn.channel.clone();
            let handle_task = channel_scope.spawn(std::time::Duration::from_secs(90), move |ctx| {
                channel_service.load(&id, Some(&fallback), 30, &ctx.cancellation_token())
            });
            let title = title.clone();
            let avatar = avatar.clone();
            let banner = banner.clone();
            let handle = handle.clone();
            let description = description.clone();
            let store = store.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let status = status.clone();
            glib::MainContext::default().spawn_local(async move {
                let (meta, videos) = match handle_task.completion().await {
                    TaskOutcome::Completed(result) => result,
                    TaskOutcome::Failed(error) => {
                        eprintln!("[channel] {error}");
                        status.error_key("channel_failed", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        status.error_key("channel_timeout", &[]);
                        return;
                    }
                };
                let real_name = meta
                    .as_ref()
                    .map(|item| item.title.clone())
                    .filter(|value| !value.is_empty() && value != "Unknown")
                    .unwrap_or_else(|| fallback_name.clone());
                title.set_text(&real_name);
                if let Some(meta) = meta {
                    handle.set_text(&format!(
                        "@{} • {}",
                        meta.title.to_lowercase().replace(' ', ""),
                        i18n::format("subscriber", &[("count", &meta.follower_count.to_string())],)
                    ));
                    description.set_text(&meta.description);
                    if let Some(url) = meta.avatar_url {
                        let avatar = avatar.clone();
                        let generation = request_generation;
                        let key = format!("channel:{display_channel_id}:avatar");
                        if let Some(texture) = load_thumbnail_texture(&conn.feed, &key) {
                            avatar.set_paintable(Some(&texture));
                        } else {
                            pipeline.request_sized(
                                key,
                                url,
                                generation,
                                96,
                                96,
                                move |_id, result_generation, result| {
                                    if result_generation != generation {
                                        return;
                                    }
                                    if let Ok(asset) = result {
                                        let texture = thumbnail_texture(&asset);
                                        avatar.set_paintable(Some(&texture));
                                    }
                                },
                            );
                        }
                    }
                    if let Some(url) = meta.banner_url {
                        let banner = banner.clone();
                        let generation = request_generation;
                        let key = format!("channel:{display_channel_id}:banner");
                        if let Some(texture) = load_thumbnail_texture(&conn.feed, &key) {
                            banner.set_paintable(Some(&texture));
                        } else {
                            pipeline.request_sized(
                                key,
                                url,
                                generation,
                                960,
                                180,
                                move |_id, result_generation, result| {
                                    if result_generation != generation {
                                        return;
                                    }
                                    if let Ok(asset) = result {
                                        let texture = thumbnail_texture(&asset);
                                        banner.set_paintable(Some(&texture));
                                    }
                                },
                            );
                        }
                    }
                }
                store.remove_all();
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                for video in &videos {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                        video.clone(),
                        texture,
                    )));
                }
                request_missing_thumbnails(
                    &pipeline,
                    &textures,
                    &videos,
                    &store,
                    request_generation,
                );
                if videos.is_empty() {
                    status.empty_key("empty_channel");
                } else {
                    status.hide();
                }
            });
        })
    };
    *channel_handler.borrow_mut() = Some(on_channel_open.clone());
    channel_status.set_retry_handler({
        let current = channel_current.clone();
        let handler = channel_handler.clone();
        move || {
            if let Some((id, name)) = current.borrow().clone() {
                if let Some(open) = handler.borrow().as_ref() {
                    open(id, name);
                }
            }
        }
    });
    player.set_on_channel_click({
        let callback = on_channel_click.clone();
        move |id, name| callback(id, name)
    });

    let home_wrapper = GtkBox::new(Orientation::Vertical, 0);
    let home_status = StatusView::new();
    let home_header = GtkBox::new(Orientation::Horizontal, 12);
    home_header.add_css_class("chipbar");
    let all_chip = Button::with_label(i18n::tr("all"));
    let subs_chip = Button::with_label(i18n::tr("subscription"));
    let continue_chip = Button::with_label(i18n::tr("continue_watching"));
    for chip in [&all_chip, &subs_chip, &continue_chip] {
        chip.add_css_class("chip");
        home_header.append(chip);
    }
    home_wrapper.append(&home_header);
    home_status.loading_key("loading_feed");
    let (home_grid, home_store) = home::build(
        t0,
        Vec::new(),
        HashMap::new(),
        on_activate.clone(),
        on_channel_click.clone(),
    );
    let home_mode = Rc::new(Cell::new(HomeFeed::All));
    let render_home = {
        let store = home_store.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let status = home_status.clone();
        let feed_scope = feed_scope.clone();
        move |mode: HomeFeed| {
            status.loading_key("loading_feed");
            let worker_conn = conn.clone();
            let handle = feed_scope.spawn(std::time::Duration::from_secs(30), move |_| {
                let videos: Vec<(Video, Option<f64>)> = match mode {
                    HomeFeed::All => worker_conn
                        .feed
                        .all(30)?
                        .into_iter()
                        .map(|video| (video, None))
                        .collect(),
                    HomeFeed::Subscription => worker_conn
                        .feed
                        .subscriptions(30)?
                        .into_iter()
                        .map(|video| (video, None))
                        .collect(),
                    HomeFeed::Continue => {
                        let continue_videos = worker_conn.feed.continue_watching(30)?;
                        let fractions = worker_conn
                            .feed
                            .history(30)?
                            .into_iter()
                            .map(|entry| {
                                let fraction =
                                    entry.video.duration.filter(|duration| *duration > 0).map(
                                        |duration| {
                                            (entry.last_position / duration as f64).clamp(0.0, 1.0)
                                        },
                                    );
                                (entry.video.id, fraction)
                            })
                            .collect::<std::collections::HashMap<_, _>>();
                        continue_videos
                            .into_iter()
                            .map(|video| {
                                let fraction = fractions.get(&video.id).copied().flatten();
                                (video, fraction)
                            })
                            .collect()
                    }
                };
                Ok(videos)
            });
            let store = store.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let status = status.clone();
            glib::MainContext::default().spawn_local(async move {
                let videos = match handle.completion().await {
                    TaskOutcome::Completed(videos) => videos,
                    TaskOutcome::Failed(error) => {
                        status.error_key("feed_failed", &[("error", &error)]);
                        return;
                    }
                    TaskOutcome::TimedOut => {
                        status.error_key("feed_timeout", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                };
                store.remove_all();
                if videos.is_empty() {
                    status.empty_key("empty_feed");
                } else {
                    status.hide();
                }
                let video_list: Vec<Video> =
                    videos.iter().map(|(video, _)| video.clone()).collect();
                let textures = load_thumbnails(conn.feed.clone(), video_list.clone()).await;
                for (video, progress) in &videos {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    let item = match progress {
                        Some(progress) => {
                            VideoItem::with_progress(video.clone(), texture, *progress)
                        }
                        None => VideoItem::new(video.clone(), texture),
                    };
                    store.append(&glib::BoxedAnyObject::new(item));
                }
                request_missing_thumbnails(&pipeline, &textures, &video_list, &store, 0);
            });
        }
    };
    let initial_home_render = render_home.clone();
    glib::idle_add_local_once(move || initial_home_render(HomeFeed::All));

    let activate_home_chip = {
        let render_home = render_home.clone();
        let all_chip = all_chip.clone();
        let subs_chip = subs_chip.clone();
        let continue_chip = continue_chip.clone();
        let home_mode = home_mode.clone();
        move |mode: HomeFeed| {
            home_mode.set(mode);
            for chip in [&all_chip, &subs_chip, &continue_chip] {
                chip.remove_css_class("active");
            }
            match mode {
                HomeFeed::All => all_chip.add_css_class("active"),
                HomeFeed::Subscription => subs_chip.add_css_class("active"),
                HomeFeed::Continue => continue_chip.add_css_class("active"),
            }
            render_home(mode);
        }
    };
    all_chip.connect_clicked({
        let activate = activate_home_chip.clone();
        move |_| activate(HomeFeed::All)
    });
    subs_chip.connect_clicked({
        let activate = activate_home_chip.clone();
        move |_| activate(HomeFeed::Subscription)
    });
    continue_chip.connect_clicked({
        let activate = activate_home_chip.clone();
        move |_| activate(HomeFeed::Continue)
    });
    all_chip.add_css_class("active");

    let home_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .build();
    home_scroll.set_child(Some(&home_grid));
    home_wrapper.append(&home_scroll);
    home_wrapper.append(home_status.widget());
    stack.add_named(&home_wrapper, Some("home"));

    let subscription_wrapper = GtkBox::new(Orientation::Vertical, 0);
    let subscription_header = GtkBox::new(Orientation::Horizontal, 12);
    subscription_header.add_css_class("page-header");
    let subscription_title = Label::builder()
        .label(i18n::tr("subscription"))
        .xalign(0.0)
        .hexpand(true)
        .build();
    subscription_title.add_css_class("page-title");
    subscription_header.append(&subscription_title);
    let subscription_count = Label::new(Some(&i18n::format("channel_count", &[("count", "0")])));
    let subscription_channel_count = Rc::new(Cell::new(0u64));
    subscription_count.add_css_class("video-meta");
    subscription_header.append(&subscription_count);
    let refresh_button = Button::with_label(i18n::tr("pull_videos"));
    refresh_button.add_css_class("chip");
    subscription_header.append(&refresh_button);
    subscription_wrapper.append(&subscription_header);
    let subscription_status = StatusView::new();
    let (subscription_grid, subscription_store) = home::build(
        t0,
        Vec::new(),
        HashMap::new(),
        on_activate.clone(),
        on_channel_click.clone(),
    );
    let subscription_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();
    subscription_scroll.set_child(Some(&subscription_grid));
    let subscription_content = GtkBox::new(Orientation::Vertical, 0);
    subscription_content.append(&subscription_scroll);
    subscription_content.append(subscription_status.widget());
    subscription_wrapper.append(&subscription_content);
    stack.add_named(&subscription_wrapper, Some("subs"));

    let history_wrapper = GtkBox::new(Orientation::Vertical, 0);
    let history_header = Label::builder()
        .label(i18n::tr("history"))
        .xalign(0.0)
        .build();
    history_header.add_css_class("page-title");
    let history_header_box = GtkBox::new(Orientation::Horizontal, 0);
    history_header_box.add_css_class("page-header");
    history_header_box.append(&history_header);
    history_wrapper.append(&history_header_box);
    let history_status = StatusView::new();
    let (history_widget, history_store) = list_view::build(
        Vec::new(),
        HashMap::new(),
        on_activate.clone(),
        on_channel_click.clone(),
    );
    let refresh_history = {
        let store = history_store.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let status = history_status.clone();
        let cache_scope = cache_scope.clone();
        move || {
            status.loading_key("loading_history");
            let worker_conn = conn.clone();
            let handle = cache_scope.spawn(std::time::Duration::from_secs(15), move |_| {
                worker_conn.feed.history(30)
            });
            let store = store.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let status = status.clone();
            glib::MainContext::default().spawn_local(async move {
                let entries = match handle.completion().await {
                    TaskOutcome::Completed(entries) => entries,
                    TaskOutcome::Failed(error) => {
                        status.error_key("history_failed", &[("error", &error)]);
                        return;
                    }
                    TaskOutcome::TimedOut => {
                        status.error_key("history_timeout", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                };
                let videos: Vec<Video> = entries.iter().map(|entry| entry.video.clone()).collect();
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                store.remove_all();
                for entry in &entries {
                    let texture = textures
                        .get(&entry.video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    let fraction = entry
                        .video
                        .duration
                        .filter(|duration| *duration > 0)
                        .map(|duration| (entry.last_position / duration as f64).clamp(0.0, 1.0))
                        .unwrap_or(0.0);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::with_progress(
                        entry.video.clone(),
                        texture,
                        fraction,
                    )));
                }
                request_missing_thumbnails(&pipeline, &textures, &videos, &store, 0);
                if entries.is_empty() {
                    status.empty_key("empty_history");
                } else {
                    status.hide();
                }
            });
        }
    };
    history_wrapper.append(&history_widget);
    history_wrapper.append(history_status.widget());
    stack.add_named(&history_wrapper, Some("history"));

    let search_wrapper = GtkBox::new(Orientation::Vertical, 0);
    let search_status = StatusView::new();
    let (search_widget, search_store) = list_view::build(
        Vec::new(),
        HashMap::new(),
        on_activate.clone(),
        on_channel_click.clone(),
    );
    search_wrapper.append(&search_widget);
    search_wrapper.append(search_status.widget());
    stack.add_named(&search_wrapper, Some("search"));

    let btn_home = build_sidebar_button("go-home-symbolic", i18n::tr("home"));
    let btn_subs = build_sidebar_button("view-list-symbolic", i18n::tr("subscription"));
    let btn_history = build_sidebar_button("document-open-recent-symbolic", i18n::tr("history"));
    let btn_playlist = build_sidebar_button("view-list-symbolic", i18n::tr("playlist"));
    let separator = gtk::Separator::new(Orientation::Horizontal);
    sidebar_inner.append(&btn_home);
    sidebar_inner.append(&btn_subs);
    sidebar_inner.append(&btn_history);
    sidebar_inner.append(&btn_playlist);
    sidebar_inner.append(&separator);

    let subscription_list = GtkBox::new(Orientation::Vertical, 2);
    sidebar_inner.append(&subscription_list);
    let refresh_sidebar_channels = {
        let subscription_list = subscription_list.clone();
        let sidebar_inner = sidebar_inner.clone();
        let conn = conn.clone();
        let stack = stack.clone();
        let navigation = navigation.clone();
        move || {
            while let Some(child) = subscription_list.first_child() {
                subscription_list.remove(&child);
            }
            if let Ok(channels) = conn.subscription.list() {
                for channel in channels.into_iter().take(8) {
                    let button = build_sidebar_button("user-symbolic", &channel.name);
                    let id = channel.id;
                    let name = channel.name;
                    let handler = channel_handler.clone();
                    let stack = stack.clone();
                    let navigation = navigation.clone();
                    button.connect_clicked(move |_| {
                        stack.set_visible_child_name("channel");
                        navigation.borrow_mut().push(Route::Channel(id.clone()));
                        if let Some(callback) = handler.borrow().as_ref() {
                            callback(id.clone(), name.clone());
                        }
                    });
                    subscription_list.append(&button);
                    set_sidebar_content_visible(
                        button.upcast_ref::<gtk::Widget>(),
                        !sidebar_inner.has_css_class("sidebar-rail"),
                    );
                }
            }
        }
    };
    refresh_sidebar_channels();
    *sidebar_refresh_hook.borrow_mut() = Some(Box::new(refresh_sidebar_channels.clone()));
    player.set_on_subscription_change({
        let refresh = refresh_sidebar_channels.clone();
        move || refresh()
    });

    body.append(&sidebar_revealer);
    body.append(&stack);
    stack.set_visible_child_name("home");

    let navigate = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let back_button = back_button.clone();
        let btn_home = btn_home.clone();
        let btn_subs = btn_subs.clone();
        let btn_history = btn_history.clone();
        let btn_playlist = btn_playlist.clone();
        move |route: Route| {
            navigation.borrow_mut().push(route.clone());
            stack.set_visible_child_name(route.page_name());
            back_button.set_sensitive(!matches!(route, Route::Home));
            for (button, active) in [
                (&btn_home, matches!(route, Route::Home)),
                (&btn_subs, matches!(route, Route::Subscription)),
                (&btn_history, matches!(route, Route::History)),
                (
                    &btn_playlist,
                    matches!(route, Route::PlaylistInput | Route::Playlist(_, _)),
                ),
            ] {
                if active {
                    button.add_css_class("active");
                } else {
                    button.remove_css_class("active");
                }
            }
        }
    };
    btn_home.add_css_class("active");

    btn_home.connect_clicked({
        let navigate = navigate.clone();
        move |_| {
            navigate(Route::Home);
        }
    });
    btn_subs.connect_clicked({
        let navigate = navigate.clone();
        let store = subscription_store.clone();
        let status = subscription_status.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let cache_scope = cache_scope.clone();
        let count_label = subscription_count.clone();
        let channel_count_value = subscription_channel_count.clone();
        move |_| {
            navigate(Route::Subscription);
            let channel_count = conn
                .subscription
                .list()
                .map(|channels| channels.len())
                .unwrap_or(0);
            channel_count_value.set(channel_count as u64);
            count_label.set_text(&i18n::format(
                "channel_count",
                &[("count", &channel_count.to_string())],
            ));
            status.loading_key("loading_subscription");
            let worker_conn = conn.clone();
            let handle = cache_scope.spawn(std::time::Duration::from_secs(15), move |_| {
                worker_conn.feed.subscriptions(30)
            });
            let store = store.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let status = status.clone();
            glib::MainContext::default().spawn_local(async move {
                let videos = match handle.completion().await {
                    TaskOutcome::Completed(videos) => videos,
                    TaskOutcome::Failed(error) => {
                        status.error_key("subscription_failed", &[("error", &error)]);
                        return;
                    }
                    TaskOutcome::TimedOut => {
                        status.error_key("subscription_timeout", &[]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                };
                store.remove_all();
                if videos.is_empty() {
                    status.empty_key("empty_subscription");
                    return;
                }
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                for video in videos.iter() {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                        video.clone(),
                        texture,
                    )));
                }
                request_missing_thumbnails(&pipeline, &textures, &videos, &store, 0);
                status.hide();
            });
        }
    });
    btn_history.connect_clicked({
        let navigate = navigate.clone();
        let refresh_history = refresh_history.clone();
        move |_| {
            navigate(Route::History);
            refresh_history();
        }
    });
    btn_playlist.connect_clicked({
        let navigate = navigate.clone();
        move |_| {
            navigate(Route::PlaylistInput);
        }
    });

    back_button.connect_clicked({
        let navigation = navigation.clone();
        let stack = stack.clone();
        let back_button = back_button.clone();
        move |_| {
            if navigation.borrow_mut().back() {
                let route = navigation.borrow().current().clone();
                stack.set_visible_child_name(route.page_name());
                back_button.set_sensitive(!matches!(route, Route::Home));
            }
        }
    });

    let theme_for_button = theme.clone();
    let conn_theme = conn.clone();
    theme_button.connect_clicked(move |button| {
        let next = theme_for_button.current().toggled();
        theme_for_button.apply(next);
        let _ = conn_theme.settings.set("theme", next.as_str());
        button.set_icon_name(match next {
            Theme::Dark => "weather-clear-night-symbolic",
            Theme::Light => "weather-clear-day-symbolic",
        });
    });

    let manual_sidebar = Rc::new(Cell::new(None::<bool>));
    menu_button.connect_clicked({
        let sidebar = sidebar_revealer.clone();
        let manual = manual_sidebar.clone();
        move |_| {
            let visible = !sidebar.reveals_child();
            sidebar.set_reveal_child(visible);
            manual.set(Some(visible));
        }
    });

    let last_layout = Rc::new(Cell::new(None::<(LayoutSpec, SidebarMode, Option<bool>)>));
    let layout_sidebar = sidebar_revealer.clone();
    let layout_sidebar_inner = sidebar_inner.clone();
    let layout_home = home_grid.clone();
    let layout_subs = subscription_grid.clone();
    let layout_channel = channel_grid.clone();
    let layout_player = player.clone();
    let layout_search = search_widget.clone();
    let layout_main = main_layout.clone();
    let manual_layout = manual_sidebar.clone();
    let apply_layout = move |width: i32| {
        let spec = LayoutSpec::for_width(width);
        let manual = manual_layout.get();
        let mode = match manual {
            Some(true) if width >= 800 => SidebarMode::Expanded,
            Some(false) if width >= 800 => SidebarMode::Rail,
            Some(true) => SidebarMode::Rail,
            Some(false) => SidebarMode::Hidden,
            None => spec.sidebar,
        };
        let layout_key = (spec, mode, manual);
        if last_layout.get() == Some(layout_key) {
            return;
        }
        last_layout.set(Some(layout_key));
        let sidebar_width = match mode {
            SidebarMode::Expanded => 240,
            SidebarMode::Rail => 72,
            SidebarMode::Hidden => 0,
        };
        layout_sidebar.set_width_request(sidebar_width);
        layout_sidebar.set_reveal_child(mode != SidebarMode::Hidden);
        layout_sidebar_inner.set_visible(mode != SidebarMode::Hidden);
        let expanded = mode == SidebarMode::Expanded;
        set_sidebar_content_visible(layout_sidebar_inner.upcast_ref::<gtk::Widget>(), expanded);
        if expanded {
            layout_sidebar_inner.remove_css_class("sidebar-rail");
        } else {
            layout_sidebar_inner.add_css_class("sidebar-rail");
        }
        home::set_columns(&layout_home, spec.columns);
        home::set_columns(&layout_subs, spec.columns);
        home::set_columns(&layout_channel, spec.columns);
        list_view::set_compact(&layout_search, spec.compact);
        if spec.compact {
            layout_main.add_css_class("compact-layout");
        } else {
            layout_main.remove_css_class("compact-layout");
        }
        layout_player.set_layout(
            spec.watch_compact,
            if spec.breakpoint >= 2 { 360 } else { 320 },
        );
    };
    window.connect_local("size-changed", false, move |values| {
        let width = values[1].get::<i32>().unwrap_or_default();
        apply_layout(width);
        None
    });

    let activate_search = {
        let stack = stack.clone();
        let navigation = navigation.clone();
        let search_store = search_store.clone();
        let status = search_status.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let generation = search_generation.clone();
        let search_scope = search_scope.clone();
        move |query: String| {
            let query = query.trim().to_string();
            if query.is_empty() {
                return;
            }
            let request_generation = generation.get().wrapping_add(1);
            generation.set(request_generation);
            stack.set_visible_child_name("search");
            navigation.borrow_mut().push(Route::Search(query.clone()));
            back_button.set_sensitive(true);
            search_store.remove_all();
            status.loading_key("searching");
            let search_query = query.clone();
            let search_service = conn.search.clone();
            let handle = search_scope.spawn(std::time::Duration::from_secs(60), move |ctx| {
                search_service.search(&search_query, 20, &ctx.cancellation_token())
            });
            let store = search_store.clone();
            let status = status.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            glib::MainContext::default().spawn_local(async move {
                let videos = match handle.completion().await {
                    TaskOutcome::Completed(videos) => videos,
                    TaskOutcome::Failed(error) => {
                        status.error_key("search_failed", &[("error", &error)]);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        status.error_key("search_timeout", &[]);
                        return;
                    }
                };
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                for video in videos.iter() {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                        video.clone(),
                        texture,
                    )));
                }
                request_missing_thumbnails(
                    &pipeline,
                    &textures,
                    &videos,
                    &store,
                    request_generation,
                );
                status.hide();
                if videos.is_empty() {
                    status.empty_key("empty_search");
                }
            });
        }
    };
    search_status.set_retry_handler({
        let activate = activate_search.clone();
        let search = search.clone();
        move || activate(search.text().to_string())
    });
    search.connect_activate({
        let activate = activate_search.clone();
        move |entry| activate(entry.text().to_string())
    });
    search_button.connect_clicked({
        let activate = activate_search.clone();
        let search = search.clone();
        move |_| activate(search.text().to_string())
    });

    let refresh_subscriptions = {
        let store = subscription_store.clone();
        let status = subscription_status.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let refresh_sidebar = refresh_sidebar_channels.clone();
        let subscription_scope = subscription_scope.clone();
        let cache_scope = cache_scope.clone();
        move |button: &Button| {
            status.loading_key("refresh_subscription");
            let button = button.clone();
            let subscription_service = conn.subscription.clone();
            let handle = subscription_scope
                .spawn(std::time::Duration::from_secs(180), move |ctx| {
                    subscription_service.refresh(15, &ctx.cancellation_token())
                });
            let store = store.clone();
            let status = status.clone();
            let conn = conn.clone();
            let pipeline = pipeline.clone();
            let refresh_sidebar = refresh_sidebar.clone();
            let cache_scope = cache_scope.clone();
            let button = button.clone();
            glib::MainContext::default().spawn_local(async move {
                let (count, failed) = match handle.completion().await {
                    TaskOutcome::Completed(result) => result,
                    TaskOutcome::Failed(error) => {
                        status.error_key("refresh_failed", &[("error", &error)]);
                        button.set_sensitive(true);
                        return;
                    }
                    TaskOutcome::Cancelled | TaskOutcome::Stale => return,
                    TaskOutcome::TimedOut => {
                        status.error_key("refresh_timeout", &[]);
                        button.set_sensitive(true);
                        return;
                    }
                };
                let worker_conn = conn.clone();
                let videos_handle = cache_scope
                    .spawn(std::time::Duration::from_secs(15), move |_| {
                        worker_conn.feed.subscriptions(30)
                    });
                let videos = match videos_handle.completion().await {
                    TaskOutcome::Completed(videos) => videos,
                    _ => {
                        button.set_sensitive(true);
                        return;
                    }
                };
                store.remove_all();
                let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                for video in videos.iter() {
                    let texture = textures
                        .get(&video.id)
                        .cloned()
                        .unwrap_or_else(home::fallback_texture);
                    store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                        video.clone(),
                        texture,
                    )));
                }
                request_missing_thumbnails(&pipeline, &textures, &videos, &store, 0);
                status.empty_key_values(
                    "refresh_done",
                    &[
                        ("count", &count.to_string()),
                        ("failed", &failed.to_string()),
                    ],
                );
                refresh_sidebar();
                button.set_sensitive(true);
            });
        }
    };
    refresh_button.connect_clicked({
        let refresh = refresh_subscriptions.clone();
        move |button| {
            button.set_sensitive(false);
            refresh(button);
        }
    });

    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let search_shortcut = search.clone();
    let player_shortcut = player.clone();
    let window_shortcut = window.clone();
    let stack_shortcut = stack.clone();
    let home_scroll_shortcut = home_scroll.clone();
    let subscription_scroll_shortcut = subscription_scroll.clone();
    let channel_scroll_shortcut = channel_widget.clone();
    let history_scroll_shortcut = history_widget.clone();
    let search_scroll_shortcut = search_widget.clone();
    key_controller.connect_key_pressed(move |_controller, key, _keycode, modifier| {
        let search_has_focus =
            widget_or_descendant_has_focus(search_shortcut.upcast_ref::<gtk::Widget>());
        if search_has_focus {
            return glib::Propagation::Proceed;
        }
        let page = stack_shortcut
            .visible_child_name()
            .unwrap_or_default()
            .to_string();
        match key {
            gdk::Key::slash | gdk::Key::l if modifier.contains(gdk::ModifierType::CONTROL_MASK) => {
                search_shortcut.grab_focus();
                glib::Propagation::Stop
            }
            gdk::Key::slash => {
                search_shortcut.grab_focus();
                glib::Propagation::Stop
            }
            gdk::Key::space if page == "player" => {
                player_shortcut.toggle_pause();
                glib::Propagation::Stop
            }
            gdk::Key::f if page == "player" => {
                player_shortcut.toggle_fullscreen(&window_shortcut);
                glib::Propagation::Stop
            }
            gdk::Key::m if page == "player" => {
                player_shortcut.toggle_mute();
                glib::Propagation::Stop
            }
            gdk::Key::Left if page == "player" && !player_shortcut.quality_control_has_focus() => {
                player_shortcut.seek_by(-5.0);
                glib::Propagation::Stop
            }
            gdk::Key::Right if page == "player" && !player_shortcut.quality_control_has_focus() => {
                player_shortcut.seek_by(5.0);
                glib::Propagation::Stop
            }
            gdk::Key::Up => {
                match page.as_str() {
                    "home" => scroll_by(&home_scroll_shortcut, -160.0),
                    "subs" => scroll_by(&subscription_scroll_shortcut, -160.0),
                    "channel" => scroll_by(&channel_scroll_shortcut, -160.0),
                    "history" => scroll_by(&history_scroll_shortcut, -160.0),
                    "search" => scroll_by(&search_scroll_shortcut, -160.0),
                    "player" => player_shortcut.scroll_recommendations(-0.5),
                    _ => {}
                }
                glib::Propagation::Stop
            }
            gdk::Key::Down => {
                match page.as_str() {
                    "home" => scroll_by(&home_scroll_shortcut, 160.0),
                    "subs" => scroll_by(&subscription_scroll_shortcut, 160.0),
                    "channel" => scroll_by(&channel_scroll_shortcut, 160.0),
                    "history" => scroll_by(&history_scroll_shortcut, 160.0),
                    "search" => scroll_by(&search_scroll_shortcut, 160.0),
                    "player" => player_shortcut.scroll_recommendations(0.5),
                    _ => {}
                }
                glib::Propagation::Stop
            }
            gdk::Key::Page_Up => {
                match page.as_str() {
                    "home" => scroll_by_page(&home_scroll_shortcut, true),
                    "subs" => scroll_by_page(&subscription_scroll_shortcut, true),
                    "channel" => scroll_by_page(&channel_scroll_shortcut, true),
                    "history" => scroll_by_page(&history_scroll_shortcut, true),
                    "search" => scroll_by_page(&search_scroll_shortcut, true),
                    "player" => player_shortcut.scroll_recommendations(-1.0),
                    _ => {}
                }
                glib::Propagation::Stop
            }
            gdk::Key::Page_Down => {
                match page.as_str() {
                    "home" => scroll_by_page(&home_scroll_shortcut, false),
                    "subs" => scroll_by_page(&subscription_scroll_shortcut, false),
                    "channel" => scroll_by_page(&channel_scroll_shortcut, false),
                    "history" => scroll_by_page(&history_scroll_shortcut, false),
                    "search" => scroll_by_page(&search_scroll_shortcut, false),
                    "player" => player_shortcut.scroll_recommendations(1.0),
                    _ => {}
                }
                glib::Propagation::Stop
            }
            gdk::Key::Home => {
                if page == "player" {
                    player_shortcut.seek_to_start();
                } else {
                    match page.as_str() {
                        "home" => scroll_to_edge(&home_scroll_shortcut, false),
                        "subs" => scroll_to_edge(&subscription_scroll_shortcut, false),
                        "channel" => scroll_to_edge(&channel_scroll_shortcut, false),
                        "history" => scroll_to_edge(&history_scroll_shortcut, false),
                        "search" => scroll_to_edge(&search_scroll_shortcut, false),
                        _ => {}
                    }
                }
                glib::Propagation::Stop
            }
            gdk::Key::End => {
                if page == "player" {
                    player_shortcut.seek_to_end();
                } else {
                    match page.as_str() {
                        "home" => scroll_to_edge(&home_scroll_shortcut, true),
                        "subs" => scroll_to_edge(&subscription_scroll_shortcut, true),
                        "channel" => scroll_to_edge(&channel_scroll_shortcut, true),
                        "history" => scroll_to_edge(&history_scroll_shortcut, true),
                        "search" => scroll_to_edge(&search_scroll_shortcut, true),
                        _ => {}
                    }
                }
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    });
    window.add_controller(key_controller);

    player.set_on_up_next({
        let on_activate = on_activate.clone();
        move |video_id| on_activate(video_id)
    });

    let discover_count = conn.feed.count_source("discover").unwrap_or(0);
    if discover_count < 10 {
        home_status.loading_key("discover_loading");
        use rand::prelude::*;
        let mut rng = rand::rng();
        let keywords: Vec<String> = crate::application::services::DISCOVER_KEYWORDS
            .choose_multiple(&mut rng, 5)
            .map(|keyword| keyword.to_string())
            .collect();
        let search_service = conn.search.clone();
        let handle = discover_scope.spawn(std::time::Duration::from_secs(90), move |ctx| {
            let keywords: Vec<&str> = keywords.iter().map(|keyword| keyword.as_str()).collect();
            search_service.discover(&keywords, 10, &ctx.cancellation_token())
        });
        let home_store = home_store.clone();
        let conn = conn.clone();
        let pipeline = pipeline.clone();
        let home_mode = home_mode.clone();
        let status = home_status.clone();
        let render_home = render_home.clone();
        let cache_scope = cache_scope.clone();
        glib::MainContext::default().spawn_local(async move {
            match handle.completion().await {
                TaskOutcome::Completed(_) => {
                    if home_mode.get() == HomeFeed::Continue {
                        render_home(HomeFeed::Continue);
                        return;
                    }
                    if home_mode.get() != HomeFeed::All {
                        return;
                    }
                    let worker_conn = conn.clone();
                    let videos_handle = cache_scope
                        .spawn(std::time::Duration::from_secs(15), move |_| {
                            worker_conn.feed.all(30)
                        });
                    let videos = match videos_handle.completion().await {
                        TaskOutcome::Completed(videos) => videos,
                        _ => return,
                    };
                    home_store.remove_all();
                    let textures = load_thumbnails(conn.feed.clone(), videos.clone()).await;
                    for video in videos.iter() {
                        let texture = textures
                            .get(&video.id)
                            .cloned()
                            .unwrap_or_else(home::fallback_texture);
                        home_store.append(&glib::BoxedAnyObject::new(VideoItem::new(
                            video.clone(),
                            texture,
                        )));
                    }
                    request_missing_thumbnails(&pipeline, &textures, &videos, &home_store, 0);
                    if videos.is_empty() {
                        status.empty_key("discover_empty");
                    } else {
                        status.hide();
                    }
                }
                TaskOutcome::Failed(error) => {
                    eprintln!("[discover] {error}");
                    status.error_key("discover_failed", &[("error", &error)]);
                }
                TaskOutcome::Cancelled | TaskOutcome::Stale => {}
                TaskOutcome::TimedOut => {
                    status.error_key("discover_timeout", &[]);
                }
            }
        });
    }

    language_controller.bind({
        let all_chip = all_chip.clone();
        let subs_chip = subs_chip.clone();
        let continue_chip = continue_chip.clone();
        let refresh_button = refresh_button.clone();
        let subscription_title = subscription_title.clone();
        let subscription_count = subscription_count.clone();
        let subscription_channel_count = subscription_channel_count.clone();
        let history_header = history_header.clone();
        let btn_home = btn_home.clone();
        let btn_subs = btn_subs.clone();
        let btn_history = btn_history.clone();
        let btn_playlist = btn_playlist.clone();
        let channel_view = channel_view.clone();
        let player = player.clone();
        let home_status = home_status.clone();
        let channel_status = channel_status.clone();
        let history_status = history_status.clone();
        let subscription_status = subscription_status.clone();
        let search_status = search_status.clone();
        let playlist_status = playlist_status.clone();
        move |_| {
            all_chip.set_label(i18n::tr("all"));
            subs_chip.set_label(i18n::tr("subscription"));
            continue_chip.set_label(i18n::tr("continue_watching"));
            refresh_button.set_label(i18n::tr("pull_videos"));
            subscription_title.set_text(i18n::tr("subscription"));
            subscription_count.set_text(&i18n::format(
                "channel_count",
                &[("count", &subscription_channel_count.get().to_string())],
            ));
            history_header.set_text(i18n::tr("history"));
            set_sidebar_button_text(&btn_home, i18n::tr("home"));
            set_sidebar_button_text(&btn_subs, i18n::tr("subscription"));
            set_sidebar_button_text(&btn_history, i18n::tr("history"));
            set_sidebar_button_text(&btn_playlist, i18n::tr("playlist"));
            channel_view.apply_language();
            player.apply_language();
            home_status.apply_language();
            channel_status.apply_language();
            history_status.apply_language();
            subscription_status.apply_language();
            search_status.apply_language();
            playlist_status.apply_language();
        }
    });

    window.present();
    let maintenance_feed = conn.feed.clone();
    std::thread::spawn(move || {
        if let Err(error) = maintenance_feed.prune() {
            eprintln!("[maintenance] prune gagal: {error}");
        }
    });
    eprintln!(
        "[startup] window presented {}ms (RSS {} KB)",
        t0.elapsed().as_millis(),
        rss_kb()
    );
}
