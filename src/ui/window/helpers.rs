use super::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

type ThumbnailDecodeJob = Box<dyn FnOnce() + Send + 'static>;

struct ThumbnailDecodePool {
    queue: Arc<(Mutex<VecDeque<ThumbnailDecodeJob>>, Condvar)>,
}

impl ThumbnailDecodePool {
    fn new() -> Self {
        let queue = Arc::new((
            Mutex::new(VecDeque::<ThumbnailDecodeJob>::new()),
            Condvar::new(),
        ));
        for _ in 0..2 {
            let worker_queue = queue.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let (lock, condvar) = &*worker_queue;
                    let mut jobs = lock.lock().unwrap();
                    loop {
                        if let Some(job) = jobs.pop_front() {
                            break job;
                        }
                        jobs = condvar.wait(jobs).unwrap();
                    }
                };
                job();
            });
        }
        Self { queue }
    }

    fn execute(&self, job: ThumbnailDecodeJob) {
        let (lock, condvar) = &*self.queue;
        lock.lock().unwrap().push_back(job);
        condvar.notify_one();
    }
}

fn thumbnail_decode_pool() -> &'static ThumbnailDecodePool {
    static POOL: OnceLock<ThumbnailDecodePool> = OnceLock::new();
    POOL.get_or_init(ThumbnailDecodePool::new)
}

struct CancelThumbnailDecodeOnDrop(Arc<AtomicBool>);

impl Drop for CancelThumbnailDecodeOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

async fn yield_to_main_loop() {
    let (sender, receiver) = futures_channel::oneshot::channel();
    glib::idle_add_local_once(move || {
        let _ = sender.send(());
    });
    let _ = receiver.await;
}

pub fn rss_kb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
        .unwrap_or(0)
}

pub(super) fn thumbnail_texture(
    asset: &crate::application::services::ThumbnailAsset,
) -> gdk::Texture {
    let bytes = glib::Bytes::from(asset.rgba.as_ref());
    let stride = asset.width as usize * 4;
    gdk::MemoryTexture::new(
        asset.width as i32,
        asset.height as i32,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &bytes,
        stride,
    )
    .into()
}

pub(super) fn load_thumbnail_texture(
    feed: &crate::application::services::FeedService,
    key: &str,
) -> Option<gdk::Texture> {
    let bytes = feed.thumbnail(key).ok().flatten()?;
    let bytes = glib::Bytes::from(bytes.as_slice());
    gdk::Texture::from_bytes(&bytes).ok()
}

pub(super) async fn load_thumbnails(
    feed: crate::application::services::FeedService,
    videos: Vec<Video>,
) -> HashMap<String, gdk::Texture> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_guard = CancelThumbnailDecodeOnDrop(cancelled.clone());
    let worker_feed = feed.clone();
    thumbnail_decode_pool().execute(Box::new(move || {
        let keys = videos
            .iter()
            .map(|video| video.id.clone())
            .collect::<Vec<_>>();
        let blobs = worker_feed.thumbnails(&keys).unwrap_or_default();
        let mut decoded = Vec::with_capacity(blobs.len());
        for (key, bytes) in blobs {
            if cancelled.load(Ordering::Acquire) {
                break;
            }
            if let Ok(pixels) = crate::core::thumbs::decode_thumbnail(&bytes, None) {
                decoded.push((key, pixels.rgba, pixels.width, pixels.height));
            }
        }
        let _ = sender.send(decoded);
    }));
    let Ok(decoded) = receiver.await else {
        drop(cancel_guard);
        return HashMap::new();
    };
    drop(cancel_guard);
    let mut textures = HashMap::with_capacity(decoded.len());
    for (index, (key, rgba, width, height)) in decoded.into_iter().enumerate() {
        let bytes = glib::Bytes::from_owned(rgba);
        let texture = gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &bytes,
            width as usize * 4,
        )
        .into();
        textures.insert(key, texture);
        if index % 16 == 15 {
            yield_to_main_loop().await;
        }
    }
    textures
}

pub(super) fn widget_or_descendant_has_focus(widget: &gtk::Widget) -> bool {
    if widget.has_focus() {
        return true;
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if widget_or_descendant_has_focus(&current) {
            return true;
        }
        child = current.next_sibling();
    }
    false
}

pub(super) fn scroll_by_page(scroll: &ScrolledWindow, up: bool) {
    let adjustment = scroll.vadjustment();
    let page_size = adjustment.page_size().max(200.0);
    let offset = if up {
        -page_size * 0.9
    } else {
        page_size * 0.9
    };
    adjustment.set_value(adjustment.value() + offset);
}

pub(super) fn scroll_by(scroll: &ScrolledWindow, offset: f64) {
    let adjustment = scroll.vadjustment();
    adjustment.set_value(adjustment.value() + offset);
}

pub(super) fn scroll_to_edge(scroll: &ScrolledWindow, end: bool) {
    let adjustment = scroll.vadjustment();
    let target = if end {
        adjustment.upper()
    } else {
        adjustment.lower()
    };
    adjustment.set_value(target);
}

pub(super) fn request_missing_thumbnails(
    pipeline: &ThumbnailPipeline,
    cached: &HashMap<String, gdk::Texture>,
    videos: &[Video],
    store: &gio::ListStore,
    generation: u64,
) {
    pipeline.cancel_stale();
    for (index, video) in videos.iter().enumerate() {
        if cached.contains_key(&video.id) {
            continue;
        }
        let Some(url) = video.thumbnail_url.clone() else {
            continue;
        };
        let id = video.id.clone();
        let store = store.clone();
        let priority = match index {
            0..=11 => 2,
            12..=23 => 1,
            _ => 0,
        };
        pipeline.request_with_priority(
            id.clone(),
            url,
            generation,
            priority,
            move |_request_id, result_generation, result| {
                if result_generation != generation {
                    return;
                }
                if let Ok(asset) = result {
                    let texture = thumbnail_texture(&asset);
                    home::update_texture(&store, &id, texture);
                }
            },
        );
    }
}

pub(super) fn build_sidebar_button(icon: &str, text: &str) -> Button {
    let button = Button::new();
    button.add_css_class("sidebar-btn");
    button.set_tooltip_text(Some(text));
    let content = GtkBox::new(Orientation::Horizontal, 24);
    content.add_css_class("sidebar-btn-content");
    let image = Image::from_icon_name(icon);
    image.set_pixel_size(24);
    image.add_css_class("sidebar-icon");
    let label = Label::builder().label(text).xalign(0.0).build();
    label.add_css_class("sidebar-label");
    content.append(&image);
    content.append(&label);
    button.set_child(Some(&content));
    button
}

pub(super) fn set_sidebar_button_text(button: &Button, text: &str) {
    button.set_tooltip_text(Some(text));
    let mut child = button.first_child();
    while let Some(current) = child {
        if let Some(label) = current.downcast_ref::<Label>() {
            label.set_text(text);
            return;
        }
        if let Some(container) = current.downcast_ref::<GtkBox>() {
            let mut nested = container.first_child();
            while let Some(candidate) = nested {
                if let Some(label) = candidate.downcast_ref::<Label>() {
                    label.set_text(text);
                    return;
                }
                nested = candidate.next_sibling();
            }
        }
        child = current.next_sibling();
    }
}

pub(super) fn set_sidebar_content_visible(widget: &gtk::Widget, visible: bool) {
    let mut child = widget.first_child();
    while let Some(current) = child {
        let next = current.next_sibling();
        if current.downcast_ref::<Label>().is_some() {
            current.set_visible(visible);
        } else {
            set_sidebar_content_visible(&current, visible);
        }
        child = next;
    }
}
