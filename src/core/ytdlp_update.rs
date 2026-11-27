use anyhow::{bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RELEASE_API: &str =
    "https://api.github.com/repos/yt-dlp/yt-dlp-nightly-builds/releases/latest";
const MAX_RELEASE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BINARY_BYTES: u64 = 16 * 1024 * 1024;
const UPDATE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    Current(String),
    Updated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NightlyRelease {
    tag: String,
    asset_url: String,
    sha256: String,
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .context("read update response")?;
    if bytes.len() as u64 > limit {
        bail!("update response exceeds {limit} bytes");
    }
    Ok(bytes)
}

fn parse_release(value: &Value) -> Result<NightlyRelease> {
    let tag = value
        .get("tag_name")
        .and_then(Value::as_str)
        .filter(|tag| !tag.trim().is_empty())
        .context("nightly release tag missing")?
        .to_string();
    let assets = value
        .get("assets")
        .and_then(Value::as_array)
        .context("nightly release assets missing")?;
    let asset = assets
        .iter()
        .find(|asset| asset.get("name").and_then(Value::as_str) == Some("yt-dlp"))
        .context("nightly yt-dlp asset missing")?;
    let asset_url = asset
        .get("browser_download_url")
        .and_then(Value::as_str)
        .filter(|url| {
            url.starts_with("https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/download/")
        })
        .context("nightly yt-dlp asset URL invalid")?
        .to_string();
    let sha256 = asset
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .context("nightly yt-dlp SHA-256 digest missing")?
        .to_ascii_lowercase();
    Ok(NightlyRelease {
        tag,
        asset_url,
        sha256,
    })
}

fn fetch_release() -> Result<NightlyRelease> {
    let response = ureq::get(RELEASE_API)
        .set("User-Agent", "elxtube-nightly-updater")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .call()
        .context("fetch latest yt-dlp nightly release")?;
    let body = read_limited(response.into_reader(), MAX_RELEASE_BYTES)?;
    let value: Value = serde_json::from_slice(&body).context("parse yt-dlp nightly release")?;
    parse_release(&value)
}

fn fetch_binary(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .set("User-Agent", "elxtube-nightly-updater")
        .timeout(UPDATE_TIMEOUT)
        .call()
        .context("download yt-dlp nightly")?;
    read_limited(response.into_reader(), MAX_BINARY_BYTES)
}

fn binary_version(path: &Path) -> Result<String> {
    let mut child = Command::new(path)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("run {}", path.display()))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait().context("poll yt-dlp version")?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("yt-dlp version timed out");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().context("read yt-dlp version")?;
    if !output.status.success() {
        bail!("yt-dlp version failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn install_binary(bytes: &[u8], destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .context("managed yt-dlp directory missing")?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".yt-dlp-download-{}", std::process::id()));
    let _ = std::fs::remove_file(&temporary);
    let result = (|| -> Result<()> {
        std::fs::write(&temporary, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))?;
        }
        let version = binary_version(&temporary)?;
        if version.is_empty() {
            bail!("downloaded yt-dlp has no version");
        }
        std::fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn update_managed_nightly() -> Result<UpdateOutcome> {
    let release = fetch_release()?;
    let destination = super::paths::managed_ytdlp_path()?;
    if destination.is_file() {
        if let Ok(version) = binary_version(&destination) {
            if version == release.tag {
                return Ok(UpdateOutcome::Current(release.tag));
            }
        }
    }
    let bytes = fetch_binary(&release.asset_url)?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != release.sha256 {
        bail!("yt-dlp nightly SHA-256 mismatch");
    }
    install_binary(&bytes, &destination)?;
    let installed_version = binary_version(&destination)?;
    if installed_version != release.tag {
        bail!(
            "installed yt-dlp version {installed_version} does not match nightly {}",
            release.tag
        );
    }
    Ok(UpdateOutcome::Updated(release.tag))
}

pub fn managed_path() -> Result<PathBuf> {
    super::paths::managed_ytdlp_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_asset_and_digest() {
        let value = serde_json::json!({
            "tag_name": "2026.11.24.232653",
            "assets": [{
                "name": "yt-dlp",
                "browser_download_url": "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/download/2026.11.24.232653/yt-dlp",
                "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }]
        });
        let release = parse_release(&value).unwrap();
        assert_eq!(release.tag, "2026.11.24.232653");
        assert_eq!(
            release.sha256,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn rejects_release_asset_from_unexpected_host() {
        let value = serde_json::json!({
            "tag_name": "2026.11.24.232653",
            "assets": [{
                "name": "yt-dlp",
                "browser_download_url": "https://example.com/yt-dlp",
                "digest": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }]
        });
        assert!(parse_release(&value).is_err());
    }
}
