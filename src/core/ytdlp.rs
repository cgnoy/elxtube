use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use super::cancellation::CancellationToken;

const MAX_STDOUT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_STDERR_BYTES: u64 = 1024 * 1024;
use super::model::{Playlist, Video};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamQuality {
    Auto,
    Height(u32),
}

impl StreamQuality {
    pub fn from_setting(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            value => value
                .strip_suffix('p')
                .and_then(|height| height.parse().ok())
                .filter(|height| *height > 0)
                .map(Self::Height),
        }
    }

    pub fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Auto),
            1 => Some(Self::Height(1080)),
            2 => Some(Self::Height(720)),
            3 => Some(Self::Height(480)),
            4 => Some(Self::Height(360)),
            _ => None,
        }
    }

    pub fn index(self) -> u32 {
        match self {
            Self::Auto => 0,
            Self::Height(1080) => 1,
            Self::Height(720) => 2,
            Self::Height(480) => 3,
            Self::Height(360) => 4,
            Self::Height(_) => 4,
        }
    }

    pub fn setting(self) -> String {
        match self {
            Self::Auto => "auto".to_string(),
            Self::Height(height) => format!("{height}p"),
        }
    }

    pub fn format_selector(self) -> String {
        match self {
            Self::Auto => "bestvideo+bestaudio/best".to_string(),
            Self::Height(height) => format!(
                "bestvideo[height<={height}][vcodec^=avc1]+bestaudio/best[height<={height}][vcodec^=avc1]/bestvideo[height<={height}]+bestaudio/best[height<={height}]"
            ),
        }
    }
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .context("read subprocess output")?;
    if bytes.len() as u64 > limit {
        bail!("output subprocess melebihi batas {limit} byte");
    }
    Ok(bytes)
}

fn run_command(command: &mut Command, cancellation: &CancellationToken) -> Result<Output> {
    if cancellation.is_cancelled() {
        bail!("permintaan dibatalkan");
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn yt-dlp")?;
    let stdout = child.stdout.take().context("capture yt-dlp stdout")?;
    let stderr = child.stderr.take().context("capture yt-dlp stderr")?;
    let stdout_reader = std::thread::spawn(move || read_limited(stdout, MAX_STDOUT_BYTES));
    let stderr_reader = std::thread::spawn(move || read_limited(stderr, MAX_STDERR_BYTES));
    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            bail!("permintaan dibatalkan");
        }
        if let Some(status) = child.try_wait().context("poll yt-dlp")? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| anyhow::anyhow!("reader stdout gagal"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| anyhow::anyhow!("reader stderr gagal"))??;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn ytdlp_executable() -> PathBuf {
    if let Some(path) = std::env::var_os("ELXTUBE_YTDLP_PATH") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Ok(managed) = super::paths::managed_ytdlp_path() {
        if managed.is_file() {
            return managed;
        }
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(directory) = current_exe.parent() {
            let bundled = directory.join("yt-dlp");
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    PathBuf::from("yt-dlp")
}

fn ytdlp_command() -> Command {
    Command::new(ytdlp_executable())
}

pub fn version(cancellation: &CancellationToken) -> Result<String> {
    let mut command = ytdlp_command();
    command.arg("--version");
    let output = run_command(&mut command, cancellation)?;
    if !output.status.success() {
        bail!(
            "yt-dlp version gagal: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_ytdlp(args: &[&str], cancellation: &CancellationToken) -> Result<Value> {
    let mut command = ytdlp_command();
    command.args(args).args(cookie_flags());
    let out = run_command(&mut command, cancellation)?;
    if !out.status.success() {
        bail!(
            "yt-dlp gagal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let v: Value = serde_json::from_slice(&out.stdout).context("parse JSON")?;
    Ok(v)
}

fn pick_thumb(v: &Value) -> Option<String> {
    let id = v.get("id")?.as_str()?;
    Some(format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id))
}

fn parse_playlist_entry(v: &Value, fallback_channel_id: Option<&str>) -> Option<Playlist> {
    let id = v.get("id").and_then(Value::as_str)?.to_string();
    let title = v
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let channel_id = v
        .get("channel_id")
        .or_else(|| v.get("uploader_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| fallback_channel_id.map(str::to_string));
    let channel_name = v
        .get("channel")
        .or_else(|| v.get("uploader"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let video_count = v
        .get("playlist_count")
        .or_else(|| v.get("n_entries"))
        .and_then(Value::as_u64);
    let thumbnail_url = v
        .get("thumbnails")
        .and_then(Value::as_array)
        .and_then(|thumbs| thumbs.last())
        .and_then(|thumb| thumb.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(Playlist {
        id,
        title,
        channel_id,
        channel_name,
        video_count,
        thumbnail_url,
    })
}

fn parse_flat_entry(v: &Value) -> Option<Video> {
    let id = v.get("id")?.as_str()?.to_string();
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();
    Some(Video {
        id: id.clone(),
        title,
        channel_id: v
            .get("channel_id")
            .or_else(|| v.get("uploader_id"))
            .and_then(|s| s.as_str())
            .map(String::from),
        channel_name: v
            .get("channel")
            .or_else(|| v.get("uploader"))
            .and_then(|s| s.as_str())
            .map(String::from),
        duration: v.get("duration").and_then(|d| d.as_i64()),
        thumbnail_url: pick_thumb(v),
        published_at: v.get("timestamp").and_then(|t| t.as_i64()),
    })
}

fn cookie_flags() -> Vec<String> {
    let Some(browser) = std::env::var("YTDLP_COOKIES_BROWSER")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Vec::new();
    };
    vec!["--cookies-from-browser".to_string(), browser]
}

pub fn search(query: &str, limit: u32, cancellation: &CancellationToken) -> Result<Vec<Video>> {
    let url = format!("ytsearch{}:{}", limit, query);
    let v = run_ytdlp(
        &["--flat-playlist", "-J", "--no-warnings", &url],
        cancellation,
    )?;
    let entries = v.get("entries").and_then(|e| e.as_array());
    let mut out = Vec::new();
    if let Some(arr) = entries {
        for e in arr {
            if let Some(vid) = parse_flat_entry(e) {
                out.push(vid);
            }
        }
    }
    Ok(out)
}

// HANYA mengambil URL video untuk MPV (anti 403)
pub fn resolve_stream(
    video_id: &str,
    quality: StreamQuality,
    cancellation: &CancellationToken,
) -> Result<(String, Option<String>)> {
    eprintln!(
        "[resolve] PATH: {}",
        std::env::var("PATH").unwrap_or_default()
    );
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let fmt = quality.format_selector();

    let mut command = ytdlp_command();
    command
        .args(["-f", &fmt, "-g", "--no-warnings"])
        .args(cookie_flags())
        .arg(&url);
    let out = run_command(&mut command, cancellation)?;

    if !out.status.success() {
        bail!(
            "yt-dlp resolve gagal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let s = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();

    if lines.is_empty() {
        bail!("URL kosong");
    }

    if lines.len() >= 2 {
        // Video + audio terpisah (DASH/HLS)
        Ok((lines[0].to_string(), Some(lines[1].to_string())))
    } else {
        // Muxed (video+audio dalam satu URL)
        Ok((lines[0].to_string(), None))
    }
}

// HANYA mengambil Metadata Teks (Judul, Channel, Durasi)
pub fn fetch_metadata(video_id: &str, cancellation: &CancellationToken) -> Result<Video> {
    let url = format!("https://www.youtube.com/watch?v={}", video_id);
    let mut command = ytdlp_command();
    command
        .args([
            "--print",
            "%(id)s\t%(title)s\t%(channel_id)s\t%(channel)s\t%(duration)s",
            "--skip-download",
            "--no-warnings",
        ])
        .args(cookie_flags())
        .arg(&url);
    let out = run_command(&mut command, cancellation)?;

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
        channel_id: if parts[2] == "NA" {
            None
        } else {
            Some(parts[2].to_string())
        },
        channel_name: if parts[3] == "NA" {
            None
        } else {
            Some(parts[3].to_string())
        },
        duration: parts[4].parse::<i64>().ok(),
        thumbnail_url: None,
        published_at: None,
    })
}

pub fn fetch_channel_videos(
    channel_id: &str,
    channel_name: Option<&str>,
    limit: u32,
    cancellation: &CancellationToken,
) -> Result<Vec<Video>> {
    let url = if channel_id.starts_with("UC") {
        format!("https://www.youtube.com/channel/{}/videos", channel_id)
    } else {
        format!("https://www.youtube.com/{}/videos", channel_id)
    };
    let lim = limit.to_string();

    let mut command = ytdlp_command();
    command
        .args([
            "--flat-playlist",
            "--playlist-end",
            &lim,
            "-J",
            "--no-warnings",
        ])
        .args(cookie_flags())
        .arg(&url);
    let out = run_command(&mut command, cancellation)?;

    if !out.status.success() {
        bail!(
            "yt-dlp fetch channel gagal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
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

pub fn fetch_channel_playlists(
    channel_id: &str,
    limit: u32,
    cancellation: &CancellationToken,
) -> Result<Vec<Playlist>> {
    let url = format!("https://www.youtube.com/channel/{channel_id}/playlists");
    let limit = limit.to_string();
    let mut command = ytdlp_command();
    command
        .args([
            "--flat-playlist",
            "--playlist-end",
            &limit,
            "-J",
            "--no-warnings",
        ])
        .args(cookie_flags())
        .arg(&url);
    let out = run_command(&mut command, cancellation)?;
    if !out.status.success() {
        bail!(
            "yt-dlp fetch playlists gagal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let value: Value =
        serde_json::from_slice(&out.stdout).context("parse JSON playlist channel")?;
    Ok(value
        .get("entries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| parse_playlist_entry(entry, Some(channel_id)))
        .collect())
}

pub fn fetch_playlist(
    playlist_id: &str,
    limit: u32,
    cancellation: &CancellationToken,
) -> Result<(Playlist, Vec<Video>)> {
    let url = format!("https://www.youtube.com/playlist?list={playlist_id}");
    let limit = limit.to_string();
    let mut command = ytdlp_command();
    command
        .args([
            "--flat-playlist",
            "--playlist-end",
            &limit,
            "-J",
            "--no-warnings",
        ])
        .args(cookie_flags())
        .arg(&url);
    let out = run_command(&mut command, cancellation)?;
    if !out.status.success() {
        bail!(
            "yt-dlp fetch playlist gagal: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let value: Value = serde_json::from_slice(&out.stdout).context("parse JSON playlist")?;
    let entries = value.get("entries").and_then(Value::as_array);
    let videos = entries
        .into_iter()
        .flatten()
        .filter_map(parse_flat_entry)
        .collect::<Vec<_>>();
    let playlist = parse_playlist_entry(&value, None).unwrap_or_else(|| Playlist {
        id: playlist_id.to_string(),
        title: playlist_id.to_string(),
        ..Playlist::default()
    });
    Ok((playlist, videos))
}

pub fn fetch_keywords(
    keywords: &[&str],
    limit_per_kw: u32,
    cancellation: &CancellationToken,
) -> Result<Vec<Video>> {
    const MAX_PARALLEL_KEYWORDS: usize = 3;
    let mut out = Vec::new();
    for chunk in keywords.chunks(MAX_PARALLEL_KEYWORDS) {
        let results = std::thread::scope(|scope| {
            let handles = chunk
                .iter()
                .map(|keyword| {
                    let cancellation = cancellation.clone();
                    scope.spawn(move || search(keyword, limit_per_kw, &cancellation))
                })
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(anyhow::anyhow!("worker search gagal")))
                })
                .collect::<Vec<_>>()
        });
        for videos in results.into_iter().flatten() {
            out.extend(videos);
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

pub fn fetch_channel_meta(
    channel_id: &str,
    cancellation: &CancellationToken,
) -> Result<ChannelMeta> {
    let url = if channel_id.starts_with("UC") {
        format!("https://www.youtube.com/channel/{}/videos", channel_id)
    } else {
        format!("https://www.youtube.com/{}/videos", channel_id)
    };
    let v = run_ytdlp(
        &["-J", "--no-warnings", "--playlist-end", "1", &url],
        cancellation,
    )?;

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
        banner_url = v
            .get("banner_img")
            .and_then(|b| b.as_str())
            .map(String::from);
    }

    Ok(ChannelMeta {
        banner_url,
        avatar_url,
        title: v
            .get("title")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string(),
        follower_count: v
            .get("channel_follower_count")
            .and_then(|c| c.as_i64())
            .unwrap_or(0),
        description: v
            .get("description")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_playlist_entry, StreamQuality};
    use crate::core::model::normalize_playlist_id;

    #[test]
    fn normalizes_playlist_urls_and_ids() {
        assert_eq!(
            normalize_playlist_id("PLabcdef123"),
            Some("PLabcdef123".to_string())
        );
        assert_eq!(
            normalize_playlist_id("https://www.youtube.com/playlist?list=PLabcdef123&x=1"),
            Some("PLabcdef123".to_string())
        );
        assert_eq!(
            normalize_playlist_id("OLAK5uy_ExampleMix"),
            Some("OLAK5uy_ExampleMix".to_string())
        );
        assert_eq!(normalize_playlist_id("not-a-playlist"), None);
    }

    #[test]
    fn parses_playlist_metadata_and_fallback_channel() {
        let value = serde_json::json!({
            "id": "PL123",
            "title": "Playlist",
            "playlist_count": 12,
            "channel": "Creator",
            "thumbnails": [{"url": "https://example.test/thumb.jpg"}]
        });
        let playlist = parse_playlist_entry(&value, Some("UC123")).expect("playlist");
        assert_eq!(playlist.id, "PL123");
        assert_eq!(playlist.title, "Playlist");
        assert_eq!(playlist.video_count, Some(12));
        assert_eq!(playlist.channel_id.as_deref(), Some("UC123"));
        assert_eq!(
            playlist.thumbnail_url.as_deref(),
            Some("https://example.test/thumb.jpg")
        );
    }

    #[test]
    fn quality_parsing_and_round_trip() {
        assert_eq!(
            StreamQuality::from_setting("auto"),
            Some(StreamQuality::Auto)
        );
        assert_eq!(
            StreamQuality::from_setting("1080p"),
            Some(StreamQuality::Height(1080))
        );
        assert_eq!(StreamQuality::Height(720).setting(), "720p");
        assert_eq!(
            StreamQuality::from_setting("720p"),
            Some(StreamQuality::Height(720))
        );
    }

    #[test]
    fn fixed_quality_selector_never_falls_back_above_limit() {
        let selector = StreamQuality::Height(720).format_selector();
        assert!(selector.contains("height<=720"));
        assert!(!selector.contains("/best,"));
        assert!(!selector.ends_with("/best"));
    }
}
