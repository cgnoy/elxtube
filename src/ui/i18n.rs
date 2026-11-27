use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Indonesian,
    English,
}

impl Language {
    pub fn from_setting(value: Option<&str>) -> Self {
        match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("en" | "english") => Self::English,
            _ => Self::Indonesian,
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Indonesian => Self::English,
            Self::English => Self::Indonesian,
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Indonesian => "ID",
            Self::English => "EN",
        }
    }
}

static CURRENT_LANGUAGE: AtomicU8 = AtomicU8::new(0);

pub fn set_current(language: Language) {
    CURRENT_LANGUAGE.store(language as u8, Ordering::Relaxed);
}

pub fn current() -> Language {
    match CURRENT_LANGUAGE.load(Ordering::Relaxed) {
        1 => Language::English,
        _ => Language::Indonesian,
    }
}

pub fn tr(key: &'static str) -> &'static str {
    match (current(), key) {
        (Language::Indonesian, "guide") => "Panduan",
        (Language::English, "guide") => "Help",
        (Language::Indonesian, "back") => "Kembali",
        (Language::English, "back") => "Back",
        (Language::Indonesian, "search") => "Telusuri",
        (Language::English, "search") => "Search",
        (Language::Indonesian, "theme") => "Ganti tema",
        (Language::English, "theme") => "Change theme",
        (Language::Indonesian, "language") => "Bahasa",
        (Language::English, "language") => "Language",
        (Language::Indonesian, "home") => "Beranda",
        (Language::English, "home") => "Home",
        (Language::Indonesian, "history") => "Riwayat",
        (Language::English, "history") => "History",
        (Language::Indonesian, "subscription") => "Subscription",
        (Language::English, "subscription") => "Subscriptions",
        (Language::Indonesian, "all") => "Semua",
        (Language::English, "all") => "All",
        (Language::Indonesian, "continue_watching") => "Lanjutkan",
        (Language::English, "continue_watching") => "Continue watching",
        (Language::Indonesian, "subscribe") => "Subscribe",
        (Language::English, "subscribe") => "Subscribe",
        (Language::Indonesian, "subscribed") => "Subscribed",
        (Language::English, "subscribed") => "Subscribed",
        (Language::Indonesian, "channel_name") => "Nama Channel",
        (Language::English, "channel_name") => "Channel Name",
        (Language::Indonesian, "not_interested") => "Tidak tertarik",
        (Language::English, "not_interested") => "Not interested",
        (Language::Indonesian, "hide_channel") => "Sembunyikan channel",
        (Language::English, "hide_channel") => "Hide channel",
        (Language::Indonesian, "up_next") => "Berikutnya",
        (Language::English, "up_next") => "Up Next",
        (Language::Indonesian, "share") => "↗ Bagikan",
        (Language::English, "share") => "↗ Share",
        (Language::Indonesian, "resolution") => "Batasi resolusi stream video",
        (Language::English, "resolution") => "Limit video stream resolution",
        (Language::Indonesian, "retry") => "Coba lagi",
        (Language::English, "retry") => "Try again",
        (Language::Indonesian, "loading") => "Memuat...",
        (Language::English, "loading") => "Loading...",
        (Language::Indonesian, "loading_title") => "Memuat judul...",
        (Language::English, "loading_title") => "Loading title...",
        (Language::Indonesian, "loading_channel") => "Memuat channel...",
        (Language::English, "loading_channel") => "Loading channel...",
        (Language::Indonesian, "loading_feed") => "Memuat feed...",
        (Language::English, "loading_feed") => "Loading feed...",
        (Language::Indonesian, "loading_history") => "Memuat riwayat...",
        (Language::English, "loading_history") => "Loading history...",
        (Language::Indonesian, "loading_channel_videos") => "Memuat video channel...",
        (Language::English, "loading_channel_videos") => "Loading channel videos...",
        (Language::Indonesian, "loading_playlist") => "Memuat playlist...",
        (Language::English, "loading_playlist") => "Loading playlist...",
        (Language::Indonesian, "loading_channel_playlists") => "Memuat playlist channel...",
        (Language::English, "loading_channel_playlists") => "Loading channel playlists...",
        (Language::Indonesian, "loading_subscription") => "Memuat subscription...",
        (Language::English, "loading_subscription") => "Loading subscriptions...",
        (Language::Indonesian, "searching") => "Mencari video...",
        (Language::English, "searching") => "Searching videos...",
        (Language::Indonesian, "refresh_subscription") => "Menarik video subscription...",
        (Language::English, "refresh_subscription") => "Refreshing subscription videos...",
        (Language::Indonesian, "pull_videos") => "Tarik Video Terbaru",
        (Language::English, "pull_videos") => "Pull Latest Videos",
        (Language::Indonesian, "best_quality") => "Mengambil kualitas terbaik...",
        (Language::English, "best_quality") => "Fetching best quality...",
        (Language::Indonesian, "fetching_video") => "Mengambil video {quality}p...",
        (Language::English, "fetching_video") => "Fetching {quality}p video...",
        (Language::Indonesian, "connecting") => "Menghubungkan ke YouTube...",
        (Language::English, "connecting") => "Connecting to YouTube...",
        (Language::Indonesian, "stream_failed") => "Gagal stream: {error}",
        (Language::English, "stream_failed") => "Stream failed: {error}",
        (Language::Indonesian, "stream_timeout") => "Gagal stream: permintaan terlalu lama.",
        (Language::English, "stream_timeout") => "Stream failed: request timed out.",
        (Language::Indonesian, "channel_failed") => "Gagal memuat video channel.",
        (Language::English, "channel_failed") => "Failed to load channel videos.",
        (Language::Indonesian, "channel_timeout") => {
            "Gagal memuat video channel: permintaan terlalu lama."
        }
        (Language::English, "channel_timeout") => {
            "Failed to load channel videos: request timed out."
        }
        (Language::Indonesian, "feed_failed") => "Gagal memuat feed: {error}",
        (Language::English, "feed_failed") => "Failed to load feed: {error}",
        (Language::Indonesian, "feed_timeout") => "Gagal memuat feed: permintaan terlalu lama.",
        (Language::English, "feed_timeout") => "Failed to load feed: request timed out.",
        (Language::Indonesian, "history_failed") => "Gagal memuat riwayat: {error}",
        (Language::English, "history_failed") => "Failed to load history: {error}",
        (Language::Indonesian, "history_timeout") => {
            "Gagal memuat riwayat: permintaan terlalu lama."
        }
        (Language::English, "history_timeout") => "Failed to load history: request timed out.",
        (Language::Indonesian, "subscription_failed") => "Gagal memuat subscription: {error}",
        (Language::English, "subscription_failed") => "Failed to load subscriptions: {error}",
        (Language::Indonesian, "subscription_timeout") => {
            "Gagal memuat subscription: permintaan terlalu lama."
        }
        (Language::English, "subscription_timeout") => {
            "Failed to load subscriptions: request timed out."
        }
        (Language::Indonesian, "refresh_failed") => "Refresh gagal: {error}",
        (Language::English, "refresh_failed") => "Refresh failed: {error}",
        (Language::Indonesian, "refresh_timeout") => "Refresh gagal: permintaan terlalu lama.",
        (Language::English, "refresh_timeout") => "Refresh failed: request timed out.",
        (Language::Indonesian, "refresh_done") => {
            "Refresh selesai: {count} video, {failed} channel gagal."
        }
        (Language::English, "refresh_done") => {
            "Refresh complete: {count} videos, {failed} channels failed."
        }
        (Language::Indonesian, "discover_loading") => "Memuat Discover...",
        (Language::English, "discover_loading") => "Loading Discover...",
        (Language::Indonesian, "discover_empty") => "Discover gagal memuat video.",
        (Language::English, "discover_empty") => "Discover failed to load videos.",
        (Language::Indonesian, "discover_failed") => "Discover gagal: {error}",
        (Language::English, "discover_failed") => "Discover failed: {error}",
        (Language::Indonesian, "discover_timeout") => "Discover gagal: permintaan terlalu lama.",
        (Language::English, "discover_timeout") => "Discover failed: request timed out.",
        (Language::Indonesian, "empty_subscription") => "Belum ada subscription.",
        (Language::English, "empty_subscription") => "There are no subscriptions yet.",
        (Language::Indonesian, "channel_count") => "{count} channel",
        (Language::English, "channel_count") => "{count} channels",
        (Language::Indonesian, "playlist_input") => "URL atau ID playlist YouTube",
        (Language::English, "playlist_input") => "YouTube playlist URL or ID",
        (Language::Indonesian, "open_playlist") => "Buka Playlist",
        (Language::English, "open_playlist") => "Open Playlist",
        (Language::Indonesian, "empty_search") => "Tidak ada hasil ditemukan.",
        (Language::English, "empty_search") => "No results found.",
        (Language::Indonesian, "search_failed") => "Pencarian gagal: {error}",
        (Language::English, "search_failed") => "Search failed: {error}",
        (Language::Indonesian, "search_timeout") => "Pencarian gagal: permintaan terlalu lama.",
        (Language::English, "search_timeout") => "Search failed: request timed out.",
        (Language::Indonesian, "playlist_failed") => "Gagal memuat playlist.",
        (Language::English, "playlist_failed") => "Failed to load playlist.",
        (Language::Indonesian, "playlist_timeout") => {
            "Gagal memuat playlist: permintaan terlalu lama."
        }
        (Language::English, "playlist_timeout") => "Failed to load playlist: request timed out.",
        (Language::Indonesian, "channel_playlists_failed") => "Gagal memuat playlist channel.",
        (Language::English, "channel_playlists_failed") => "Failed to load channel playlists.",
        (Language::Indonesian, "channel_playlists_timeout") => {
            "Gagal memuat playlist channel: permintaan terlalu lama."
        }
        (Language::English, "channel_playlists_timeout") => {
            "Failed to load channel playlists: request timed out."
        }
        (Language::Indonesian, "empty_feed") => "Belum ada video untuk feed ini.",
        (Language::English, "empty_feed") => "There are no videos for this feed yet.",
        (Language::Indonesian, "empty_history") => "Belum ada riwayat tontonan.",
        (Language::English, "empty_history") => "There is no watch history yet.",
        (Language::Indonesian, "empty_channel") => "Channel ini belum memiliki video.",
        (Language::English, "empty_channel") => "This channel has no videos yet.",
        (Language::Indonesian, "empty_playlist") => "Playlist ini belum memiliki video.",
        (Language::English, "empty_playlist") => "This playlist has no videos yet.",
        (Language::Indonesian, "empty_channel_playlists") => "Channel ini belum memiliki playlist.",
        (Language::English, "empty_channel_playlists") => "This channel has no playlists yet.",
        (Language::Indonesian, "history_save_failed") => "Gagal menyimpan riwayat: {error}",
        (Language::English, "history_save_failed") => "Failed to save history: {error}",
        (Language::Indonesian, "history_writer_stopped") => {
            "Gagal menyimpan riwayat: writer database berhenti"
        }
        (Language::English, "history_writer_stopped") => {
            "Failed to save history: database writer stopped"
        }
        (Language::Indonesian, "playlist") => "Playlist",
        (Language::English, "playlist") => "Playlists",
        (Language::Indonesian, "videos") => "Video",
        (Language::English, "videos") => "Videos",
        (Language::Indonesian, "shorts") => "Shorts",
        (Language::English, "shorts") => "Shorts",
        (Language::Indonesian, "posts") => "Postingan",
        (Language::English, "posts") => "Posts",
        (Language::Indonesian, "latest") => "Terbaru",
        (Language::English, "latest") => "Latest",
        (Language::Indonesian, "popular") => "Populer",
        (Language::English, "popular") => "Popular",
        (Language::Indonesian, "oldest") => "Terlama",
        (Language::English, "oldest") => "Oldest",
        (Language::Indonesian, "watched") => "{percent}% ditonton",
        (Language::English, "watched") => "{percent}% watched",
        (Language::Indonesian, "subscriber") => "{count} subscriber",
        (Language::English, "subscriber") => "{count} subscribers",
        (Language::Indonesian, "video_count") => "{count} video",
        (Language::English, "video_count") => "{count} videos",
        _ => key,
    }
}

pub fn format(key: &'static str, values: &[(&str, &str)]) -> String {
    let mut message = tr(key).to_string();
    for (name, value) in values {
        message = message.replace(&format!("{{{name}}}"), value);
    }
    message
}

type LanguageListener = Rc<dyn Fn(Language)>;

#[derive(Clone)]
pub struct LanguageController {
    language: Rc<Cell<Language>>,
    listeners: Rc<RefCell<Vec<LanguageListener>>>,
}

impl LanguageController {
    pub fn new(language: Language) -> Self {
        set_current(language);
        Self {
            language: Rc::new(Cell::new(language)),
            listeners: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn current(&self) -> Language {
        self.language.get()
    }

    pub fn bind(&self, apply: impl Fn(Language) + 'static) {
        let apply = Rc::new(apply);
        apply(self.current());
        self.listeners.borrow_mut().push(apply);
    }

    pub fn set(&self, language: Language) -> bool {
        if self.language.get() == language {
            return false;
        }
        self.language.set(language);
        set_current(language);
        for listener in self.listeners.borrow().iter() {
            listener(language);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_between_indonesian_and_english() {
        set_current(Language::Indonesian);
        assert_eq!(tr("home"), "Beranda");
        set_current(Language::English);
        assert_eq!(tr("home"), "Home");
        assert_eq!(format("video_count", &[("count", "3")]), "3 videos");
    }
}
