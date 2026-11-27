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
- `gtk4`, `libmpv`, `python3`
- Rust 1.80+ (build only)
- Network access on first launch

On startup, elxtube downloads the latest yt-dlp nightly from the official
`yt-dlp-nightly-builds` GitHub release, verifies its SHA-256 digest, validates
the executable, and installs it under
`$XDG_DATA_HOME/elxtube/bin/yt-dlp` (normally
`~/.local/share/elxtube/bin/yt-dlp`). The system yt-dlp installation is never
modified. Failed updates keep the previously installed managed binary or fall
back to `yt-dlp` from `PATH`.

Set `ELXTUBE_YTDLP_PATH` to use a specific executable and skip the managed
update. Set `ELXTUBE_DISABLE_YTDLP_UPDATE=1` to disable automatic updates.

Arch: `sudo pacman -S gtk4 mpv python`

## Build & Run

    cargo build --release
    ./target/release/elxtube

Optional cookies (reduce YouTube rate-limiting):

    YTDLP_COOKIES_BROWSER=brave ./target/release/elxtube

## Packages

Release artifacts are built in `dist/`:

- `elxtube_<version>_amd64.deb` for Debian and Ubuntu
- `elxtube-<version>-1.x86_64.rpm` for Fedora, openSUSE, and compatible distributions
- `elxtube-<version>-1-x86_64.pkg.tar.zst` for Arch Linux
- `elxtube-<version>-x86_64.AppImage` for portable use

Build all formats with:

    bash packaging/build-all.sh

Individual builders are available as `packaging/build-deb.sh`,
`packaging/build-rpm.sh`, `packaging/build-arch.sh`, and
`packaging/build-appimage.sh`. Tagged GitHub releases run the same scripts and
publish all four artifacts.

## Known limitations

- No YouTube login; subscriptions are stored locally in SQLite.
- YouTube rate-limits IPs that resolve many videos quickly. If playback
  fails, wait a few minutes.
- Anti-bot delay: ~2 seconds between resolve and playback.

## License

GPL-3.0-or-later. See `LICENSE`.

Dynamically links against system `libmpv` (dual-licensed LGPL-2.1+ /
GPL-2.0+).
