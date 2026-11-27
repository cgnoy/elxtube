use anyhow::{Context, Result};
use std::io::{Cursor, Read};
use std::path::Path;

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;

pub const THUMB_QUALITY: u8 = 70;
pub const DEFAULT_THUMB_WIDTH: u32 = 480;
pub const DEFAULT_THUMB_HEIGHT: u32 = 270;

pub struct ThumbnailPixels {
    pub jpeg: Vec<u8>,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
pub const MAX_DOWNLOAD_BYTES: usize = 2 * 1024 * 1024;

/// Download bytes from URL (blocking, ringan). 5s timeout.
pub fn fetch_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .with_context(|| format!("HTTP GET {}", url))?;
    if let Some(length) = resp.header("content-length") {
        if length
            .parse::<u64>()
            .is_ok_and(|length| length > MAX_DOWNLOAD_BYTES as u64)
        {
            anyhow::bail!("thumbnail response exceeds {} bytes", MAX_DOWNLOAD_BYTES);
        }
    }
    let mut buf = Vec::new();
    resp.into_reader()
        .take((MAX_DOWNLOAD_BYTES + 1) as u64)
        .read_to_end(&mut buf)?;
    if buf.len() > MAX_DOWNLOAD_BYTES {
        anyhow::bail!("thumbnail response exceeds {} bytes", MAX_DOWNLOAD_BYTES);
    }
    Ok(buf)
}

/// Resize any image bytes to the requested dimensions and JPEG quality.
pub fn resize_to_dimensions(
    src_bytes: &[u8],
    width: u32,
    height: u32,
    quality: u8,
) -> Result<Vec<u8>> {
    let img = image::load_from_memory(src_bytes).context("decode image")?;
    let thumb = img.resize_to_fill(width, height, FilterType::Triangle);
    let mut out = Vec::with_capacity(8 * 1024);
    {
        let mut enc = JpegEncoder::new_with_quality(Cursor::new(&mut out), quality);
        enc.encode_image(&thumb).context("encode jpeg")?;
    }
    Ok(out)
}

/// Encode an image as JPEG without changing its source dimensions.
pub fn preserve_resolution_to_jpeg_bytes(src_bytes: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(src_bytes).context("decode image")?;
    let mut out = Vec::with_capacity(16 * 1024);
    {
        let mut enc = JpegEncoder::new_with_quality(Cursor::new(&mut out), THUMB_QUALITY);
        enc.encode_image(&img).context("encode jpeg")?;
    }
    Ok(out)
}

pub fn decode_thumbnail(
    src_bytes: &[u8],
    dimensions: Option<(u32, u32)>,
) -> Result<ThumbnailPixels> {
    let image = image::load_from_memory(src_bytes).context("decode image")?;
    let (width, height) = dimensions.unwrap_or((DEFAULT_THUMB_WIDTH, DEFAULT_THUMB_HEIGHT));
    let thumbnail = image.resize_to_fill(width, height, FilterType::Triangle);
    let rgba = thumbnail.to_rgba8();
    let mut jpeg = Vec::with_capacity(16 * 1024);
    {
        let mut encoder = JpegEncoder::new_with_quality(Cursor::new(&mut jpeg), THUMB_QUALITY);
        encoder.encode_image(&thumbnail).context("encode jpeg")?;
    }
    Ok(ThumbnailPixels {
        jpeg,
        rgba: rgba.into_raw(),
        width,
        height,
    })
}

/// Convert a file to JPEG bytes while preserving its source dimensions.
pub fn file_to_thumb_bytes(path: &Path) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    preserve_resolution_to_jpeg_bytes(&bytes)
}

/// Convenience: fetch a URL and write a resolution-preserving JPEG to disk.
pub fn download_to_thumb_file(url: &str, dst: &Path) -> Result<u64> {
    let raw = fetch_bytes(url)?;
    let jpg = preserve_resolution_to_jpeg_bytes(&raw)?;
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
