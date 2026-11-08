use anyhow::{Context, Result};
use std::io::Cursor;
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

pub const THUMB_W: u32 = 320;
pub const THUMB_H: u32 = 180;
pub const THUMB_QUALITY: u8 = 70;

/// Download bytes from URL (blocking, ringan). 15s timeout.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .with_context(|| format!("HTTP GET {}", url))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf)?;
    Ok(buf)
}

/// Resize any image bytes → 160×90 JPEG (q70). Returns compressed JPEG bytes.
pub fn resize_to_thumb_bytes(src_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(src_bytes).context("decode image")?;
    let thumb = img.resize_to_fill(THUMB_W, THUMB_H, FilterType::Triangle);
    let mut out = Vec::with_capacity(8 * 1024);
    {
        let mut enc = JpegEncoder::new_with_quality(Cursor::new(&mut out), THUMB_QUALITY);
        enc.encode_image(&thumb).context("encode jpeg")?;
    }
    Ok(out)
}

/// Resize a file path → JPEG bytes.
pub fn resize_file_to_thumb_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    resize_to_thumb_bytes(&bytes)
}

/// Convenience: fetch URL, resize, write to disk (cache path).
pub fn download_to_thumb_file(url: &str, dst: &Path) -> Result<u64> {
    let raw = fetch_bytes(url)?;
    let jpg = resize_to_thumb_bytes(&raw)?;
    std::fs::write(dst, &jpg)?;
    Ok(jpg.len() as u64)
}

/// Delete thumbs dir files until only `keep` newest remain (by mtime).
pub fn enforce_lru(max_files: usize) -> Result<usize> {
    let dir = super::paths::thumbs_dir()?;
    let mut entries: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        entries.push((mtime, entry.path()));
    }
    if entries.len() <= max_files {
        return Ok(0);
    }
    entries.sort_by_key(|(t, _)| *t);
    let to_remove = entries.len() - max_files;
    let mut removed = 0;
    for (_, p) in entries.into_iter().take(to_remove) {
        if std::fs::remove_file(&p).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
