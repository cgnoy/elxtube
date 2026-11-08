# elxTube

YouTube client vibecoded using GTK4-Rust, libmpv, and yt-dlp. Built for
old hardware (Passmark 500–2000, 2–4 GB RAM, HDD).

## Features

- Embedded libmpv playback with hardware decode when available
- Search, channel browsing, local subscriptions (no login)
- Discover feed, history with resume, thumbnail cache
- Lightweight: no Electron, no Chromium, no webview

## Requirements

- Linux (X11)
- `gtk4`, `libmpv`, `yt-dlp`
- Rust 1.80+ (build only)

Arch: `sudo pacman -S gtk4 mpv yt-dlp`

## Build & Run

    cargo build --release
    ./target/release/elxtube

Optional cookies (reduce YouTube rate-limiting):

    YTDLP_COOKIES_BROWSER=brave ./target/release/elxtube

## Known limitations

- No YouTube login; subscriptions are stored locally in SQLite.
- YouTube rate-limits IPs that resolve many videos quickly. If playback
  fails, wait a few minutes.
- Anti-bot delay: ~2 seconds between resolve and playback.

## License

GPL-3.0-or-later. See `LICENSE`.

Dynamically links against system `libmpv` (dual-licensed LGPL-2.1+ /
GPL-2.0+).
