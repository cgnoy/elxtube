use rusqlite::OptionalExtension;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

use gtk::{gio, glib};
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Image, Label, Orientation, SearchEntry, Stack,
    StackTransitionType, ScrolledWindow, gdk,
};

use super::home;
use super::channel;
use super::list_view;
use super::player::Player;
use super::video_item::VideoItem;
use crate::core::{ytdlp, feed, model::Video};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::cell::Cell;

enum PlayerMsg {
    Stream(Result<(String, Option<String>), String>),
    Metadata(Result<Video, String>),
}

pub fn rss_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.trim().trim_end_matches("kB").trim().parse().unwrap_or(0);
        }
    }
    0
}

fn load_thumbnails(conn: &rusqlite::Connection, videos: &[Video]) -> HashMap<String, gdk::Texture> {
    let mut textures = HashMap::new();
    if videos.is_empty() { return textures; }
    let ids: Vec<String> = videos.iter().map(|v| v.id.clone()).collect();
    let placeholders = std::iter::repeat("?").take(ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!("SELECT video_id, jpeg FROM thumb_blob WHERE video_id IN ({})", placeholders);
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(params.as_slice(), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        }) {
            for r in rows {
                if let Ok((id, bytes)) = r {
                    let gb = glib::Bytes::from(bytes.as_slice());
                    if let Ok(tex) = gdk::Texture::from_bytes(&gb) {
                        textures.insert(id, tex);
                    }
                }
            }
        }
    }
    textures
}

fn create_player_callback(
    stack: gtk::Stack,
    back_btn: gtk::Button,
    player: Rc<Player>,
    conn: Rc<RefCell<rusqlite::Connection>>,
    prev_view: Rc<RefCell<String>>,
) -> impl Fn(String) + 'static {
    move |video_id| {
        // Simpan view sebelum pindah ke player
        let current = stack.visible_child_name().map(|s| s.to_string()).unwrap_or_else(|| "home".to_string());
        *prev_view.borrow_mut() = current;
        stack.set_visible_child_name("player");
        back_btn.set_visible(true);
        player.set_loading("Menghubungkan ke YouTube...");
        // PAUSE_ON_CLICK: pause video lama saat user klik video baru.
        // Tanpa ini, video lama tetap main selama resolve (5-8s).
        player.pause();

        let mut found_in_db = false;
        let mut has_channel = false;

        if let Ok(Some(v)) = conn.borrow().query_row(
            "SELECT video_id, title, channel_id, channel_name, duration FROM feed_cache WHERE video_id = ?1",
            rusqlite::params![video_id],
            |r| Ok(Video {
                id: r.get(0)?,
                title: r.get(1)?,
                channel_id: r.get(2)?,
                channel_name: r.get(3)?,
                duration: r.get(4)?,
                thumbnail_url: None,
                published_at: None,
            })
        ).optional() {
            has_channel = v.channel_id.is_some()
                && v.channel_name.as_ref().map_or(false, |n| !n.is_empty() && n != "Unknown");
            player.update_metadata(v);
            found_in_db = true;
        }

        let (tx, rx) = mpsc::channel();
        let vid_thread = video_id.clone();

        // URL_CACHE: 10 menit TTL. Rewatch / back-forward = 0 request ke YouTube.
        let cached_stream: Option<(String, Option<String>)> = {
            let conn_ref = conn.borrow();
            match crate::core::db::video_cache_get(&conn_ref, &vid_thread, 600) {
                Ok(Some(json)) => serde_json::from_str(&json).ok(),
                _ => None,
            }
        };

        if let Some((url, audio)) = cached_stream {
            eprintln!("[play] cache hit {}", vid_thread);
            // SKIP_DELAY_CACHE: cache hit = URL sudah >2s tua, tidak perlu delay anti-bot.
            // Langsung play di main thread (closure dipanggil dari grid activate).
            player.play(&url, audio.as_deref());
            // Metadata async (tidak blocking play)
            if !found_in_db || !has_channel {
                let tx_meta = tx.clone();
                let vid_meta = vid_thread.clone();
                std::thread::spawn(move || {
                    if let Ok(v) = ytdlp::fetch_metadata(&vid_meta) {
                        tx_meta.send(PlayerMsg::Metadata(Ok(v))).ok();
                    }
                });
            }
        } else {
            eprintln!("[play] start resolve {}", video_id);
            let t_resolve = std::time::Instant::now();
            std::thread::spawn(move || {
                if !found_in_db || !has_channel {
                    match ytdlp::fetch_metadata(&vid_thread) {
                        Ok(v) => { tx.send(PlayerMsg::Metadata(Ok(v))).ok(); }
                        Err(e) => { eprintln!("[metadata] gagal: {}", e); }
                    }
                }

                match ytdlp::resolve_stream(&vid_thread, 720) {
                    Ok((url, audio)) => {
                        // URL_CACHE: simpan hasil resolve (TTL 10 menit)
                        if let Ok(json) = serde_json::to_string(&(url.clone(), audio.clone())) {
                            if let Ok(db) = crate::core::db::open() {
                                let _ = crate::core::db::video_cache_put(&db, &vid_thread, &json);
                            }
                        }
                        eprintln!("[play] resolved in {:?}", t_resolve.elapsed());
                        tx.send(PlayerMsg::Stream(Ok((url, audio)))).ok();
                    }
                    Err(e) => {
                        eprintln!("[play] resolve FAILED in {:?}: {}", t_resolve.elapsed(), e);
                        tx.send(PlayerMsg::Stream(Err(e.to_string()))).ok();
                    }
                }
            });
        }

        let player_inner = player.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(PlayerMsg::Stream(Ok((url, audio_url)))) => {
                    // DELAY_TEST: 2s delay sebelum loadfile (hipotesis anti-bot timing)
                    let p = player_inner.clone();
                    glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
                        p.play(&url, audio_url.as_deref());
                    });
                }
                Ok(PlayerMsg::Stream(Err(e))) => {
                    player_inner.set_loading(&format!("Gagal stream: {}", e));
                }
                Ok(PlayerMsg::Metadata(Ok(video))) => {
                    player_inner.update_metadata(video);
                }
                Ok(PlayerMsg::Metadata(Err(e))) => {
                    eprintln!("[window] metadata error: {}", e);
                }
                Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
            }
            glib::ControlFlow::Continue
        });
    }
}

pub fn build(app: &Application, t0: Instant, conn: Rc<RefCell<rusqlite::Connection>>) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("elxTube")
        .default_width(1200)
        .default_height(800)
        .build();

    let main_layout = GtkBox::new(Orientation::Vertical, 0);
    window.set_child(Some(&main_layout));

    let top_bar = GtkBox::new(Orientation::Horizontal, 16);
    top_bar.add_css_class("topbar");

    let back_btn = Button::builder().icon_name("go-previous-symbolic").tooltip_text("Kembali").visible(false).build();
    let brand = Label::builder().label("<b>elxTube</b>").use_markup(true).build();
    brand.add_css_class("title-3");

    top_bar.append(&back_btn);
    top_bar.append(&brand);

    let search = SearchEntry::builder().placeholder_text("Telusuri YouTube...").hexpand(true).build();
    search.add_css_class("search-entry");
    search.set_max_width_chars(600);
    search.set_halign(gtk::Align::Center);
    top_bar.append(&search);
    main_layout.append(&top_bar);

    let body = GtkBox::new(Orientation::Horizontal, 0);
    body.set_vexpand(true);
    main_layout.append(&body);

    // Sidebar dibuat awal supaya bisa di-pass ke Player::new
    let sidebar_inner = GtkBox::new(Orientation::Vertical, 4);
    sidebar_inner.set_width_request(200);
    sidebar_inner.add_css_class("sidebar");
    let sidebar_revealer = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::None)
        .transition_duration(0)
        .reveal_child(true)
        .child(&sidebar_inner)
        .build();

    let prev_view: Rc<RefCell<String>> = Rc::new(RefCell::new("home".to_string()));

    let stack = Stack::builder()
        .transition_type(StackTransitionType::Crossfade)
        .hexpand(true)
        .vexpand(true)
        .build();

    // 1. PLAYER
    let player = Rc::new(Player::new(conn.clone(), top_bar.clone(), sidebar_revealer.clone()));
    player.widget.set_margin_top(24);
    player.widget.set_margin_bottom(24);
    player.widget.set_margin_start(24);
    player.widget.set_margin_end(24);

    stack.add_named(&player.widget, Some("player"));

    // 2. CHANNEL VIEW
    let on_ch_dummy = Rc::new(|_, _| {});
    let (channel_widget, channel_store, channel_title_lbl, channel_avatar_lbl, channel_banner_pic, channel_handle_lbl, channel_desc_lbl) = channel::build(
        t0,
        create_player_callback(stack.clone(), back_btn.clone(), player.clone(), conn.clone(), prev_view.clone()),
        on_ch_dummy,
    );
    stack.add_named(&channel_widget, Some("channel"));

    // 3. CALLBACK KLIK CHANNEL
    // CH_THUMB_BATCH: generation counter untuk progressive thumbnail reveal
    let ch_gen: Rc<Cell<u64>> = Rc::new(Cell::new(0));

    let on_ch_click: Rc<dyn Fn(String, String)> = {
        let stack_c = stack.clone();
        let channel_title_c = channel_title_lbl.clone();
        let channel_avatar_c = channel_avatar_lbl.clone();
        let channel_store_c = channel_store.clone();
        let conn_c = conn.clone();

        let gen_for_click = ch_gen.clone();
        Rc::new(move |ch_id, ch_name| {
            let my_gen = gen_for_click.get() + 1;
            gen_for_click.set(my_gen);
            stack_c.set_visible_child_name("channel");
            // CH_AUTH_NAME: jangan tampilkan "Unknown" sebagai judul — tampilkan "Memuat..."
            // Nama asli datang dari meta.title (satu fetch), bukan dari parameter ch_name
            let initial = if ch_name == "Unknown" || ch_name.is_empty() {
                "Memuat...".to_string()
            } else {
                ch_name.clone()
            };
            channel_title_c.set_text(&initial);
            channel_avatar_c.set_text(&initial.chars().next().unwrap_or('C').to_uppercase().to_string());
            channel_store_c.remove_all();


            // === CACHE (instant) ===
            if let Ok(cached) = feed::channel_videos(&conn_c.borrow(), &ch_id, 30) {
                let textures = load_thumbnails(&conn_c.borrow(), &cached);
                for v in cached {
                    let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                    channel_store_c.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
                }
            }

            // === CH_AUTH_NAME: fetch meta + videos dalam satu thread ===
            // Satu kali fetch meta → satu nama otoritatif untuk header + semua video
            let (tx, rx) = mpsc::channel::<(Option<crate::core::ytdlp::ChannelMeta>, String, Result<Vec<crate::core::model::Video>, String>)>();
            let ch_id_thread = ch_id.clone();
            let ch_name_fallback = ch_name.clone();
            let title_c = channel_title_c.clone();
            let avatar_c = channel_avatar_c.clone();
            let banner_c = channel_banner_pic.clone();
            let handle_c = channel_handle_lbl.clone();
            let desc_c = channel_desc_lbl.clone();
            std::thread::spawn(move || {
                let meta = ytdlp::fetch_channel_meta(&ch_id_thread).ok();
                let real_name = meta.as_ref()
                    .map(|m| m.title.clone())
                    .filter(|s| !s.is_empty() && s != "Unknown")
                    .unwrap_or(ch_name_fallback);
                let name_param: Option<&str> = if real_name == "Unknown" || real_name.is_empty() {
                    None  // biarkan parse_flat_entry coba sendiri
                } else {
                    Some(real_name.as_str())
                };
                let videos = ytdlp::fetch_channel_videos(&ch_id_thread, name_param, 30);
                if let Ok(ref vids) = videos {
                    if let Ok(db) = crate::core::db::open() {
                        let now = crate::core::now_secs();
                        for vid in vids {
                            let _ = crate::core::db::feed_upsert(&db, vid, now);
                        }
                        // Kalau channel ini subscribed dan namanya masih Unknown, update
                        if real_name != "Unknown" && !real_name.is_empty() {
                            let _ = db.execute(
                                "UPDATE channels SET name = ?1 WHERE id = ?2 AND (name = 'Unknown' OR name = '')",
                                rusqlite::params![&real_name, &ch_id_thread],
                            );
                        }
                    }
                }
                let _ = tx.send((meta, real_name, videos.map_err(|e| e.to_string())));
            });

            let conn_inner = conn_c.clone();
            let store_inner = channel_store_c.clone();
            let banner_c2 = banner_c.clone();
            let handle_c2 = handle_c.clone();
            let desc_c2 = desc_c.clone();
            let title_c2 = title_c.clone();
            let avatar_c2 = avatar_c.clone();
            let stack_for_thumb = stack_c.clone();
            let gen_for_thumb = gen_for_click.clone();
            let my_gen_thumb = my_gen;
            glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
                match rx.try_recv() {
                    Ok((meta, real_name, videos_res)) => {
                        // Update header dengan nama otoritatif
                        title_c2.set_text(&real_name);
                        avatar_c2.set_text(&real_name.chars().next().unwrap_or('C').to_uppercase().to_string());
                        // Update banner/handle/desc kalau meta ada
                        if let Some(meta) = meta {
                            handle_c2.set_text(&format!("@{} • {} subscriber", meta.title.to_lowercase().replace(" ", ""), meta.follower_count));
                            desc_c2.set_text(&meta.description);
                            if let Some(banner_url) = meta.banner_url {
                                if let Ok(raw) = crate::core::thumbs::fetch_bytes(&banner_url) {
                                    if let Ok(jpg) = crate::core::thumbs::resize_to_thumb_bytes(&raw) {
                                        let gb = glib::Bytes::from(&jpg[..]);
                                        if let Ok(tex) = gdk::Texture::from_bytes(&gb) {
                                            banner_c2.set_paintable(Some(&tex));
                                        }
                                    }
                                }
                            }
                        }
                        let videos = match videos_res {
                            Ok(v) => v,
                            Err(e) => {
                                eprintln!("[channel] fetch error: {}", e);
                                return glib::ControlFlow::Break;
                            }
                        };
                        store_inner.remove_all();
                        let textures = load_thumbnails(&conn_inner.borrow(), &videos);
                        // CH_THUMB_BATCH: antri video yang belum punya texture
                        let mut pending: VecDeque<crate::core::model::Video> = VecDeque::new();
                        for v in videos.iter() {
                            let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                            if !textures.contains_key(&v.id) && v.thumbnail_url.is_some() {
                                pending.push_back(v.clone());
                            }
                            store_inner.append(&glib::BoxedAnyObject::new(VideoItem { video: v.clone(), texture: tex }));
                        }
                        // spawn 4 worker + receiver batch
                        if !pending.is_empty() {
                            let queue = Arc::new(Mutex::new(pending));
                            let (tx_thumb, rx_thumb) = std::sync::mpsc::channel::<(u64, String, gtk::gdk::Texture)>();
                            for _ in 0..4 {
                                let q = queue.clone();
                                let tx = tx_thumb.clone();
                                let my_gen_worker = my_gen_thumb;
                                std::thread::spawn(move || {
                                    loop {
                                        // cek gen sebelum ambil kerja
                                        // (gen dicek juga di main saat splice, ini hemat kerja)
                                        let next = { q.lock().unwrap().pop_front() };
                                        let v = match next { Some(v) => v, None => break };
                                        let url = match &v.thumbnail_url { Some(u) => u.clone(), None => continue };
                                        let tex = match crate::core::thumbs::fetch_bytes(&url) {
                                            Ok(raw) => match crate::core::thumbs::resize_to_thumb_bytes(&raw) {
                                                Ok(jpg) => {
                                                    let gb = glib::Bytes::from(&jpg[..]);
                                                    gdk::Texture::from_bytes(&gb).unwrap_or_else(|_| home::fallback_texture())
                                                }
                                                Err(_) => continue,
                                            },
                                            Err(_) => continue,
                                        };
                                        if tx.send((my_gen_worker, v.id.clone(), tex)).is_err() { break; }
                                    }
                                });
                            }
                            drop(tx_thumb);
                            // receiver: batch 5 item atau 500ms flush
                            let store_b = store_inner.clone();
                            let stack_b = stack_for_thumb.clone();
                            let gen_b = gen_for_thumb.clone();
                            let mut batch: Vec<(String, gtk::gdk::Texture)> = Vec::with_capacity(5);
                            let mut last_flush = std::time::Instant::now();
                            glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                                // kumpulkan dari channel sampai batch penuh atau timeout
                                while batch.len() < 5 {
                                    match rx_thumb.try_recv() {
                                        Ok((g, vid, tex)) => {
                                            if g == gen_b.get() { batch.push((vid, tex)); }
                                        }
                                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                            // flush sisa lalu berhenti
                                            flush_batch(&store_b, &stack_b, &mut batch);
                                            return glib::ControlFlow::Break;
                                        }
                                    }
                                }
                                let timeout_hit = last_flush.elapsed() >= std::time::Duration::from_millis(500);
                                if batch.len() >= 5 || (timeout_hit && !batch.is_empty()) {
                                    flush_batch(&store_b, &stack_b, &mut batch);
                                    last_flush = std::time::Instant::now();
                                }
                                glib::ControlFlow::Continue
                            });
                        }
                        glib::ControlFlow::Break
                    }
                    Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
                }
            });
        })
    };

    let on_ch_player = on_ch_click.clone();
    player.set_on_channel_click(move |id, name| on_ch_player(id, name));

    // 4. HOME VIEW
    let home_videos = feed::discover_feed(&conn.borrow(), 30).unwrap_or_default();
    let home_textures = load_thumbnails(&conn.borrow(), &home_videos);
    let home_wrapper = GtkBox::new(Orientation::Vertical, 0);

    let on_ch_home = on_ch_click.clone();
    let (home_grid, home_store) = home::build(
        t0, home_videos, home_textures,
        create_player_callback(stack.clone(), back_btn.clone(), player.clone(), conn.clone(), prev_view.clone()),
        on_ch_home,
    );
    let home_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();
    home_scroll.set_child(Some(&home_grid));
    home_wrapper.append(&home_scroll);
    stack.add_named(&home_wrapper, Some("home"));

    // 5. SUBSCRIPTION VIEW
    let subs_videos = feed::subs_feed(&conn.borrow(), 30).unwrap_or_default();
    let subs_textures = load_thumbnails(&conn.borrow(), &subs_videos);
    let subs_container = GtkBox::new(Orientation::Vertical, 0);

    let subs_header = GtkBox::new(Orientation::Horizontal, 16);
    subs_header.set_margin_top(12);
    subs_header.set_margin_start(24);
    subs_header.set_margin_end(24);
    let refresh_btn = Button::with_label("🔄 Tarik Video Terbaru");
    refresh_btn.add_css_class("btn-subscribe");
    subs_header.append(&refresh_btn);
    subs_container.append(&subs_header);

    let on_ch_subs = on_ch_click.clone();
    let (subs_grid, subs_store) = home::build(
        t0, subs_videos, subs_textures,
        create_player_callback(stack.clone(), back_btn.clone(), player.clone(), conn.clone(), prev_view.clone()),
        on_ch_subs,
    );
    let subs_scroll = ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .hexpand(true)
        .build();
    subs_scroll.set_child(Some(&subs_grid));
    subs_container.append(&subs_scroll);
    stack.add_named(&subs_container, Some("subs"));

    // 6. SEARCH VIEW
    let on_ch_search = on_ch_click.clone();
    let (search_widget, search_store) = list_view::build(
        Vec::new(), HashMap::new(),
        create_player_callback(stack.clone(), back_btn.clone(), player.clone(), conn.clone(), prev_view.clone()),
        on_ch_search,
    );
    stack.add_named(&search_widget, Some("search"));

    // 7. SIDEBAR (sidebar_inner + revealer sudah dibuat di atas)
    let btn_home = make_sidebar_btn("go-home-symbolic", "Home");
    let btn_subs = make_sidebar_btn("view-list-symbolic", "Subscription");

    let stack_nav = stack.clone();
    let back_btn_nav = back_btn.clone();
    btn_home.connect_clicked(move |_| {
        stack_nav.set_visible_child_name("home");
        back_btn_nav.set_visible(false);
    });

    let stack_nav_subs = stack.clone();
    let back_btn_nav_subs = back_btn.clone();
    let subs_store_clone = subs_store.clone();
    let conn_subs_nav = conn.clone();
    btn_subs.connect_clicked(move |_| {
        stack_nav_subs.set_visible_child_name("subs");
        back_btn_nav_subs.set_visible(false);
        subs_store_clone.remove_all();
        if let Ok(videos) = feed::subs_feed(&conn_subs_nav.borrow(), 30) {
            let textures = load_thumbnails(&conn_subs_nav.borrow(), &videos);
            for v in videos {
                let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                subs_store_clone.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
            }
        }
    });

    sidebar_inner.append(&btn_home);
    sidebar_inner.append(&btn_subs);
    body.append(&sidebar_revealer);
    body.append(&stack);
    stack.set_visible_child_name("home");

    // Tombol Back
    let stack_back = stack.clone();
    let back_btn_clone = back_btn.clone();
    let prev_view_for_back = prev_view.clone();
    back_btn.connect_clicked(move |_| {
        let target = prev_view_for_back.borrow().clone();
        stack_back.set_visible_child_name(&target);
        back_btn_clone.set_visible(false);
    });

    // 8. REFRESH BUTTON
    let conn_refresh = conn.clone();
    let subs_store_refresh = subs_store.clone();
    let home_store_refresh = home_store.clone();
    refresh_btn.connect_clicked(move |btn| {
        btn.set_sensitive(false);
        btn.set_label("⏳ Menarik data...");

        let btn_clone = btn.clone();
        let store_clone = subs_store_refresh.clone();
        let conn_clone = conn_refresh.clone();
        let home_store_clone = home_store_refresh.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let db_thread = crate::core::db::open().unwrap();
            let channels = crate::core::db::channel_list(&db_thread).unwrap_or_default();
            let now = crate::core::now_secs();

            for ch in channels {
                match ytdlp::fetch_channel_videos(&ch.id, Some(&ch.name), 15) {
                    Ok(vids) => {
                        for mut vid in vids {
                            vid.channel_name = Some(ch.name.clone());
                            vid.channel_id = Some(ch.id.clone());
                            let _ = crate::core::db::feed_upsert(&db_thread, &vid, now);
                            if let Some(url) = &vid.thumbnail_url {
                                if let Ok(raw) = crate::core::thumbs::fetch_bytes(url) {
                                    if let Ok(jpg) = crate::core::thumbs::resize_to_thumb_bytes(&raw) {
                                        let _ = crate::core::db::thumb_blob_put(&db_thread, &vid.id, &jpg);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => eprintln!("[bg] GAGAL menarik video dari {}: {}", ch.name, e),
                }
            }
            tx.send(()).ok();
        });

        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            match rx.try_recv() {
                Ok(()) => {
                    store_clone.remove_all();
                    if let Ok(videos) = feed::subs_feed(&conn_clone.borrow(), 30) {
                        let textures = load_thumbnails(&conn_clone.borrow(), &videos);
                        for v in videos {
                            let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                            store_clone.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
                        }
                    }
                    home_store_clone.remove_all();
                    if let Ok(videos) = feed::discover_feed(&conn_clone.borrow(), 30) {
                        let textures = load_thumbnails(&conn_clone.borrow(), &videos);
                        for v in videos {
                            let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                            home_store_clone.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
                        }
                    }
                    btn_clone.set_label("✅ Tarik Video Terbaru");
                    btn_clone.set_sensitive(true);
                    glib::ControlFlow::Break
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    });

    // 9. SEARCH LOGIC
    let stack_search_act = stack.clone();
    let search_store_inner = search_store.clone();
    search.connect_activate(move |entry| {
        let query = entry.text().to_string();
        if query.is_empty() { return; }

        stack_search_act.set_visible_child_name("search");
        search_store_inner.remove_all();

        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            if let Ok(videos) = ytdlp::search(&query, 20) {
                if let Ok(db) = crate::core::db::open() {
                    let now = crate::core::now_secs();
                    for v in &videos {
                        let _ = crate::core::db::feed_upsert(&db, v, now);
                    }
                }

                let mut handles = vec![];
                for v in videos {
                    let tx_clone = tx.clone();
                    handles.push(std::thread::spawn(move || {
                        let texture = if let Some(url) = &v.thumbnail_url {
                            if let Ok(bytes) = crate::core::thumbs::fetch_bytes(url) {
                                if let Ok(thumb_bytes) = crate::core::thumbs::resize_to_thumb_bytes(&bytes) {
                                    let gb = glib::Bytes::from(&thumb_bytes[..]);
                                    gdk::Texture::from_bytes(&gb).unwrap_or_else(|_| home::fallback_texture())
                                } else { home::fallback_texture() }
                            } else { home::fallback_texture() }
                        } else { home::fallback_texture() };

                        let item = VideoItem { video: v, texture };
                        tx_clone.send(item).ok();
                    }));
                }
                for h in handles { let _ = h.join(); }
            }
        });

        let store_search_act = search_store_inner.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            loop {
                match rx.try_recv() {
                    Ok(item) => store_search_act.append(&glib::BoxedAnyObject::new(item)),
                    Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
                }
            }
        });
    });

    // 10. BACKGROUND FETCH DISCOVER
    let conn_discover_count = conn.clone();
    let count: i64 = conn_discover_count.borrow().query_row(
        "SELECT COUNT(*) FROM feed_cache WHERE channel_id = 'DISCOVER'", [], |r| r.get(0)
    ).unwrap_or(0);

    if count < 10 {
        let (tx, rx) = std::sync::mpsc::channel();
        use rand::prelude::*;
        let mut rng = rand::rng();
        let selected: Vec<String> = feed::DISCOVER_KEYWORDS.choose_multiple(&mut rng, 5).map(|s| s.to_string()).collect();

        std::thread::spawn(move || {
            let refs: Vec<&str> = selected.iter().map(|s| s.as_str()).collect();
            if let Ok(vids) = ytdlp::fetch_keywords(&refs, 10) {
                if let Ok(db) = crate::core::db::open() {
                    let now = crate::core::now_secs();
                    for vid in vids {
                        let _ = crate::core::db::feed_upsert(&db, &vid, now);
                        if let Some(url) = &vid.thumbnail_url {
                            if let Ok(raw) = crate::core::thumbs::fetch_bytes(url) {
                                if let Ok(jpg) = crate::core::thumbs::resize_to_thumb_bytes(&raw) {
                                    let _ = crate::core::db::thumb_blob_put(&db, &vid.id, &jpg);
                                }
                            }
                        }
                    }
                }
            }
            tx.send(()).ok();
        });

        let conn_fetch = conn.clone();
        let home_store_fetch = home_store.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            match rx.try_recv() {
                Ok(()) => {
                    home_store_fetch.remove_all();
                    if let Ok(videos) = feed::discover_feed(&conn_fetch.borrow(), 30) {
                        let textures = load_thumbnails(&conn_fetch.borrow(), &videos);
                        for v in videos {
                            let tex = textures.get(&v.id).cloned().unwrap_or_else(home::fallback_texture);
                            home_store_fetch.append(&glib::BoxedAnyObject::new(VideoItem { video: v, texture: tex }));
                        }
                    }
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            }
        });
    }

    window.present();
    eprintln!("[startup] window presented {}ms (RSS {} KB)", t0.elapsed().as_millis(), rss_kb());
}

// CH_THUMB_BATCH: ganti texture per item di store (batch)
fn flush_batch(
    store: &gio::ListStore,
    stack: &gtk::Stack,
    batch: &mut Vec<(String, gdk::Texture)>,
) {
    if batch.is_empty() { return; }
    if stack.visible_child_name().as_deref() != Some("channel") {
        batch.clear();
        return;
    }
    let n = store.n_items();
    // map video_id -> index sekali
    let mut idx: HashMap<String, u32> = HashMap::with_capacity(n as usize);
    for i in 0..n {
        if let Some(obj) = store.item(i).and_downcast::<glib::BoxedAnyObject>() {
            let item = obj.borrow::<VideoItem>();
            idx.insert(item.video.id.clone(), i);
        }
    }
    for (vid, tex) in batch.drain(..) {
        if let Some(&i) = idx.get(&vid) {
            if let Some(obj) = store.item(i).and_downcast::<glib::BoxedAnyObject>() {
                let old = obj.borrow::<VideoItem>().clone();
                let new_item = VideoItem { video: old.video, texture: tex };
                store.splice(i, 1, &[glib::BoxedAnyObject::new(new_item)]);
            }
        }
    }
}

fn make_sidebar_btn(icon: &str, text: &str) -> Button {
    let b = Button::new();
    b.add_css_class("sidebar-btn");
    let hbox = GtkBox::new(Orientation::Horizontal, 12);
    let img = Image::from_icon_name(icon);
    img.set_pixel_size(18);
    let lbl = Label::builder().label(text).xalign(0.0).build();
    hbox.append(&img);
    hbox.append(&lbl);
    b.set_child(Some(&hbox));
    b
}
