use std::time::Instant;

fn rss_kb() -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    0
}

fn main() -> anyhow::Result<()> {
    let t0 = Instant::now();
    eprintln!("[startup] begin (RSS {} KB)", rss_kb());

    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "gl");
    }
    if std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    let database = elxtube::core::database::Database::open()?;
    eprintln!("[startup] db ready {}ms", t0.elapsed().as_millis());

    let custom_ytdlp = std::env::var_os("ELXTUBE_YTDLP_PATH").is_some();
    let update_disabled = std::env::var_os("ELXTUBE_DISABLE_YTDLP_UPDATE").is_some();
    if custom_ytdlp {
        eprintln!("[startup] update yt-dlp nightly dilewati karena ELXTUBE_YTDLP_PATH");
    } else if update_disabled {
        eprintln!("[startup] update yt-dlp nightly dinonaktifkan");
    } else {
        match elxtube::core::ytdlp_update::update_managed_nightly() {
            Ok(elxtube::core::ytdlp_update::UpdateOutcome::Current(version)) => {
                eprintln!("[startup] yt-dlp nightly {version} sudah terbaru");
            }
            Ok(elxtube::core::ytdlp_update::UpdateOutcome::Updated(version)) => {
                eprintln!("[startup] yt-dlp nightly diperbarui ke {version}");
            }
            Err(error) => {
                eprintln!("[startup] update yt-dlp nightly gagal: {error:#}");
            }
        }
    }

    match elxtube::core::ytdlp::version(&elxtube::core::cancellation::CancellationToken::default())
    {
        Ok(version) => eprintln!("[startup] yt-dlp {version}"),
        Err(error) => eprintln!("[startup] yt-dlp tidak tersedia: {error:#}"),
    }

    let app = elxtube::application::AppContext::new(database);

    elxtube::ui::run(t0, app)
}
