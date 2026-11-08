use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::process::Command;

use super::model::Video;

fn run_ytdlp(args: &[&str]) -> Result<Value> {
    let out = Command::new("yt-dlp")
        .args(args)
        .args(cookie_flags())
        .output()
        .context("spawn yt-dlp")?;
    if !out.status.success() {
        bail!("yt-dlp gagal: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("parse JSON")?;
    Ok(v)
}

fn pick_thumb(v: &Value) -> Option<String> {
    let id = v.get("id")?.as_str()?;
    Some(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
}

fn parse_flat_entry(v: &Value) -> Option<Video> {
    let id = v.get("id")?.as_str()?.to_string();
    let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string();
    Some(Video {
        id: id.clone(),
        title,
        channel_id: v.get("channel_id").or_else(|| v.get("uploader_id")).and_then(|s| s.as_str()).map(String::from),
        channel_name: v.get("channel").or_else(|| v.get("uploader")).and_then(|s| s.as_str()).map(String::from),
        duration: v.get("duration").and_then(|d| d.as_i64()),
        thumbnail_url: pick_thumb(v),
        published_at: v.get("timestamp").and_then(|t| t.as_i64()),
    })
}

fn cookie_flags() -> Vec<String> {
    match std::env::var("YTDLP_COOKIES_BROWSER") {
        Ok(b) => vec!["--cookies-from-browser".to_string(), b],
        Err(_) => vec![],
    }
}

pub fn search(query: &str, limit: u32) -> Result<Vec<Video>> {
    let url = format!("ytsearch{}:{}", limit, query);
    let v = run_ytdlp(&["--flat-playlist", "-J", "--no-warnings", &url])?;
    let entries = v.get("entries").and_then(|e| e.as_array());
    let mut out = Vec::new();
    if let Some(arr) = entries {
        for e in arr {
            if let Some(vid) = parse_flat_entry(e) { out.push(vid); }
        }
    }
    Ok(out)
}

// HANYA mengambil URL video untuk MPV (anti 403)
pub fn resolve_stream(video_id: &str, max_height: u32) -> Result<(String, Option<String>)> {
    eprintln!("[resolve] PATH: {}", std::env::var("PATH").unwrap_or_default());
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    // Selector toleran: tidak memaksa m4a, tidak memaksa muxed
    let fmt = format!(
        "bestvideo[height<={h}][vcodec^=avc1]+bestaudio/best[height<={h}][vcodec^=avc1]/bestvideo[height<={h}]+bestaudio/best[height<={h}]/best",
        h = max_height
    );
    
    let out = Command::new("yt-dlp")
        .args(["-f", &fmt, "-g", "--no-warnings"]).args(cookie_flags()).arg(&url)
        .output()?;
        
    if !out.status.success() {
        bail!("yt-dlp resolve gagal: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    
    let s = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    
    if lines.is_empty() { bail!("URL kosong"); }
    
    if lines.len() >= 2 {
        // Video + audio terpisah (DASH/HLS)
        Ok((lines[0].to_string(), Some(lines[1].to_string())))
    } else {
        // Muxed (video+audio dalam satu URL)
        Ok((lines[0].to_string(), None))
    }
}

// HANYA mengambil Metadata Teks (Judul, Channel, Durasi)
pub fn fetch_metadata(video_id: &str) -> Result<Video> {
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let out = Command::new("yt-dlp")
        .args(["--print", "%(id)s\t%(title)s\t%(channel_id)s\t%(channel)s\t%(duration)s", "--skip-download", "--no-warnings"]).args(cookie_flags()).arg(&url)
        .output()?;

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("fetch_metadata gagal: {}", err.trim());
    }

    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = s.trim().split('\t').collect();
    if parts.len() < 5 {
        bail!("format tidak valid: {}", s);
    }

    Ok(Video {
        id: parts[0].to_string(),
        title: parts[1].to_string(),
        channel_id: if parts[2] == "NA" { None } else { Some(parts[2].to_string()) },
        channel_name: if parts[3] == "NA" { None } else { Some(parts[3].to_string()) },
        duration: parts[4].parse::<i64>().ok(),
        thumbnail_url: None,
        published_at: None,
    })
}

pub fn fetch_channel_videos(channel_id: &str, channel_name: Option<&str>, limit: u32) -> Result<Vec<Video>> {
    let url = if channel_id.starts_with("UC") {
        format!("https://www.youtube.com/channel/{}/videos", channel_id)
    } else {
        format!("https://www.youtube.com/{}/videos", channel_id)
    };
    let lim = limit.to_string();
    
    let out = Command::new("yt-dlp")
        .args([
            "--flat-playlist",
            "--playlist-end", &lim,
            "-J",
            "--no-warnings",
        ])
        .args(cookie_flags())
        .arg(&url)
        .output()
        .context("spawn yt-dlp (fetch channel)")?;
        
    if !out.status.success() {
        bail!("yt-dlp fetch channel gagal: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    
    let v: Value = serde_json::from_slice(&out.stdout).context("parse JSON")?;
    let entries = v.get("entries").and_then(|e| e.as_array());
    let mut out_vids = Vec::new();
    
    if let Some(arr) = entries {
        for e in arr {
            if let Some(mut vid) = parse_flat_entry(e) {
                vid.channel_id = Some(channel_id.to_string());
                // KABOOM: pakai channel_name yang kita sudah tahu, jangan biarkan None
                if let Some(name) = channel_name {
                    vid.channel_name = Some(name.to_string());
                }
                out_vids.push(vid);
            }
        }
    }
    Ok(out_vids)
}

pub fn fetch_keywords(keywords: &[&str], limit_per_kw: u32) -> Result<Vec<Video>> {
    let mut out = Vec::new();
    for kw in keywords {
        let url = format!("ytsearch{}:{}", limit_per_kw, kw);
        if let Ok(v) = run_ytdlp(&["--flat-playlist", "-J", "--no-warnings", &url]) {
            if let Some(entries) = v.get("entries").and_then(|e| e.as_array()) {
                for e in entries {
                    if let Some(mut vid) = parse_flat_entry(e) {
                        vid.channel_id = Some("DISCOVER".to_string());
                        vid.channel_name = Some(format!("Trending: {}", kw));
                        out.push(vid);
                    }
                }
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default)]
pub struct ChannelMeta {
    pub banner_url: Option<String>,
    pub avatar_url: Option<String>,
    pub title: String,
    pub follower_count: i64,
    pub description: String,
}

pub fn fetch_channel_meta(channel_id: &str) -> Result<ChannelMeta> {
    let url = if channel_id.starts_with("UC") {
        format!("https://www.youtube.com/channel/{}/videos", channel_id)
    } else {
        format!("https://www.youtube.com/{}/videos", channel_id)
    };
    let v = run_ytdlp(&["-J", "--no-warnings", "--playlist-end", "1", &url])?;

    let mut banner_url = None;
    let mut avatar_url = None;

    if let Some(thumbs) = v.get("thumbnails").and_then(|t| t.as_array()) {
        for t in thumbs {
            let id = t.get("id").and_then(|i| i.as_str()).unwrap_or("");
            let url = t.get("url").and_then(|u| u.as_str());
            if id.contains("banner") {
                banner_url = url.map(String::from);
            } else if id.contains("avatar") {
                avatar_url = url.map(String::from);
            }
        }
    }

    // Fallback: Jika banner tidak ada di thumbnails, cari di field banner_img
    if banner_url.is_none() {
        banner_url = v.get("banner_img").and_then(|b| b.as_str()).map(String::from);
    }

    Ok(ChannelMeta {
        banner_url,
        avatar_url,
        title: v.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string(),
        follower_count: v.get("channel_follower_count").and_then(|c| c.as_i64()).unwrap_or(0),
        description: v.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
    })
}
