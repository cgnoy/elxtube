pub mod db;
pub mod feed;
pub mod model;
pub mod paths;
pub mod thumbs;
pub mod ytdlp;

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
