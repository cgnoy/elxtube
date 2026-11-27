# Rencana Redesign GUI elxTube YouTube-like yang Ringan

## Konteks dan keputusan

Baseline adalah **working tree saat ini**, bukan hanya commit `94e8f74`: sudah ada implementasi parsial tema, layout breakpoint, komponen grid/list, responsive player, dan beberapa alur UI tambahan. Perubahan uncommitted tersebut harus dipertahankan dan diselesaikan, bukan di-reset.

Tujuan: menjadikan GUI native GTK4 terasa seperti YouTube desktop—shell, navigasi, hierarchy, responsivitas, tema, dan feedback status—tanpa mengorbankan playback, responsivitas, atau kenyamanan pada perangkat low-end/HDD.

Keputusan pengguna:
- Cakupan adalah **fokus UI/UX**. Tombol yang sebelumnya dekoratif (like, share, kualitas) tetap nonfungsional; tidak menambah login, autoplay, like lokal, salin link, atau playback resume otomatis.
- Cache feed/URL/thumbnail boleh dibatasi dan di-evict; riwayat tontonan tetap disimpan penuh.
- Tetap memakai stack ringan yang sudah ada: GTK4 + SQLite/WAL + yt-dlp + libmpv. Tidak menambah Electron, webview, libadwaita, framework frontend, atau dependency berat.

## Temuan penting dari alur saat ini

- Arsitektur native satu proses: `src/ui/window.rs` mengorkestrasi stack halaman; data/cache berada di `src/core/db.rs` dan `src/core/feed.rs`; pencarian/channel/stream melalui `src/core/ytdlp.rs`; playback memakai libmpv langsung tanpa download/transcode lokal (`src/ui/player.rs`).
- `src/ui/layout.rs:18-58` sudah memiliki breakpoint, tetapi belum dipakai. Sidebar tetap 200 px, grid tetap 1–4 kolom, dan Up Next belum dip wiring (`src/ui/window.rs:255-264`, `src/ui/home.rs:32-40`, `src/ui/player.rs:946-976`).
- Tema Dark/Light dan persistensi `settings.theme` sudah ada, tetapi runtime toggle tidak tersedia (`src/ui/theme.rs:34-70`, `src/ui/window.rs:886`).
- Search membuat satu thread per thumbnail (`src/ui/window.rs:779-807`), mengurutkan hasil berdasarkan waktu thumbnail selesai, dan tidak memiliki generation guard.
- Thumbnail cache di-decode sinkron pada GTK thread (`src/ui/window.rs:45-74`); banner channel di-download/resize di callback GTK (`src/ui/window.rs:417-427`).
- History di-upsert setiap 500 ms saat posisi >5 detik (`src/ui/player.rs:784-839`), menciptakan write amplification/WAL churn pada HDD.
- Feed/thumbnail/stream URL tidak memiliki batas produksi; beberapa helper cleanup tidak pernah dipanggil (`src/core/db.rs:84-91`, `src/core/db.rs:183-230`).
- Resolve stream, Search, dan Channel tidak memiliki generation token, sehingga hasil lama dapat menimpa pilihan baru (`src/ui/window.rs:77-206`, `src/ui/window.rs:315-538`).

## Target pengalaman

### Shell dan navigasi

- Top bar: tombol hamburger/back, brand `elxTube` dengan aksen merah, search pill dengan tombol submit, dan tombol tema di kanan.
- Sidebar: **Home**, **Subscription**, **History**, separator, lalu maksimal 8 subscription dari SQLite dengan avatar inisial. Tombol subscribe tambahan tetap tersedia di halaman Subscription.
- Sidebar mengikuti mode **Expanded / Rail / Hidden**, dengan item aktif, tooltip pada rail, dan tombol hamburger untuk toggle manual.
- Back memakai navigation stack nyata, bukan satu `prev_view`; Home/Sidebar/Back/search/channel selalu kembali ke konteks yang benar.
- Theme tombol bekerja tanpa network: toggle CSS provider, simpan `settings.theme`, dan tema tersimpan dibaca sebelum window dibuat.
- Tambahkan shortcut low-cost tanpa timer tambahan: `/` atau `Ctrl+L` fokus search; `Space` play/pause; `F` fullscreen; `M` mute; `←/→` seek 5 detik. Shortcut tidak aktif saat mengetik di entry.

### Layout responsif

Hitung dari lebar konten aktual setelah sidebar, bukan lebar monitor; hitung ulang hanya ketika breakpoint berubah.

| Lebar konten | Sidebar | Grid | Watch |
|---|---|---|---|
| `>=1600` | Expanded ±240 px | 4 kolom | Rekomendasi kanan ±360 px |
| `1180-1599` | Expanded ±208 px | 3 kolom | Rekomendasi kanan ±320 px |
| `800-1179` | Rail ±72 px | 2–3 kolom | Up Next pindah ke bawah |
| `<800` | Hidden/overlay | 1–2 kolom | Satu kolom |

- Pertahankan virtualisasi `GridView`/`ListView`, rasio thumbnail 16:9, dan tinggi kartu stabil.
- Watch memakai area 16:9 yang mengikuti lebar; hilangkan ketergantungan player pada tinggi tetap.
- Pada mode narrow, Up Next pindah ke bawah dan search/result card menjadi compact.

### Halaman

- **Home:** header, chip **Semua / Subscription / Lanjutkan**, grid seragam, serta loading/empty/error state. Pergantian chip hanya query SQLite; tidak ada request network.
  - `Semua` → `discover_feed(30)`.
  - `Subscription` → `subs_feed(30)`.
  - `Lanjutkan` → `continue_feed(30)`.
- **Subscription:** jumlah channel subscribed, refresh dengan status sukses/gagal, empty state, dan grid yang sama dengan Home.
- **History:** route baru dari sidebar, 30 item terakhir, thumbnail cache, channel, posisi/durasi, progress bar tipis tanpa animasi. Klik membuka video dari awal; tidak ada resume otomatis.
- **Search:** 20 hasil dengan urutan relevansi stabil, loading/error/empty state, dan generation guard agar query lama tidak menimpa query baru.
- **Channel:** banner proporsional, avatar, nama/handle/subscriber/description, tab/filter visual, grid video, dan tombol subscribe yang memakai operasi DB yang sudah ada. Tabs/filter yang belum punya data tetap dekoratif.
- **Watch:** judul membungkus dua baris, metadata channel, subscribe, action row, player responsif, dan **Up Next** cache-first berisi 12 item dari `feed::up_next`. Klik Up Next memakai callback playback yang sama; tidak ada autoplay.
- Hapus metadata views palsu seperti `1 jt x ditonton`; tampilkan metadata yang benar-benar tersedia, atau channel saja.
- Like, share, dan kualitas tetap dekoratif/nonfungsional sesuai cakupan; jangan memberi affordance handler palsu.

## Batas resource dan HDD

- Tidak ada animasi, blur, shadow besar, preview video, dependency baru, atau polling periodik baru.
- Maksimal **4 worker thumbnail bersama** untuk Search, Channel, Discover, dan refresh Subscription; jangan membuat thread per hasil.
- Thumbnail video tetap 320×180 JPEG q70; avatar sekitar 96×96 dan banner maksimal 960×180.
- Cache SQLite BLOB dipertahankan (menghindari metadata/inode per-file pada HDD); jangan mengganti thumbnail menjadi satu file per thumbnail.
- Batasi item: Home/Subscription/Channel/History 30, Search 20, Up Next 12.
- Jangan menambah video lokal, transcode, atau file cache video.

Target operasional:
- Tidak menambah thread idle permanen; idealnya timer render 16 ms yang selalu hidup di `src/ui/player.rs:600-607` diganti wake-up event-driven yang dikoalesikan dan hanya aktif saat player perlu render.
- History write maksimal sekitar setiap 5 detik selama playback, lalu flush saat pause, halaman ditinggalkan/unmap, atau playback berakhir—mengurangi update 500 ms sekitar 90%.
- Batch insert feed/thumbnail dalam satu transaction; gunakan satu connection per background operation.
- Startup tidak boleh eagerly query/decode Subscription yang belum terlihat; muat data/texture halaman saat aktivasi.

## Langkah implementasi

### 1. Stabilisasi baseline dan kontrak navigasi

- Audit diff working tree dan pertahankan perubahan valid; perbaiki hanya bagian yang bertentangan dengan kontrak baru.
- Tambahkan state navigasi berstack di `src/ui/window.rs`: nama halaman, navigation history, state sidebar manual, generation token per alur async (Home/Discover, Search, Channel, Playback).
- Pastikan setiap hasil async hanya boleh mengubah UI bila generation dan halaman aktif masih cocok; hasil lama boleh dibuang.
- Tambahkan helper status page (loading/empty/error/retry) yang dipakai Home, Search, Channel, Subscription, dan History agar perilakunya konsisten.

### 2. Rapikan data layer dan cache

Di `src/core/db.rs` dan `src/core/schema.sql`:
- Pertahankan schema tanpa migrasi; tambahkan `busy_timeout` dan konfigurasi runtime yang konsisten untuk semua connection.
- Pisahkan initialization schema dari koneksi worker bila praktis, sehingga background connection tidak mengulang seluruh schema batch.
- Tambahkan batch API ber-transaction untuk feed dan thumbnail (`feed_upsert_many`, `thumb_blob_put_many`) serta API prune yang dapat diuji.
- Retention awal:
  - `video_cache`: hapus yang lebih lama dari TTL 10 menit.
  - `feed_cache`: hapus cache non-history yang lebih lama dari 30 hari.
  - `thumb_blob`: pertahankan semua thumbnail yang direferensikan `history`; untuk thumbnail lain, pertahankan 500 terbaru dan buang yang lebih lama dari 30 hari.
  - `history`: tidak di-evict.
- Jalankan prune pada startup dan setelah refresh/batch besar, bukan setiap tick.
- Gunakan query ber-index; hindari pola `NOT IN` yang semakin mahal saat tabel membesar. Untuk dedupe/filter history, gunakan `NOT EXISTS` atau filter terstruktur yang setara.
- Ganti `ORDER BY RANDOM()` pada jalur yang sering dipanggil dengan sampling berbasis ID/offset yang stabil dan bounded.

Di `src/core/feed.rs`:
- Pertahankan query `continue_feed`, `subs_feed`, `discover_feed`, `channel_videos`, dan `up_next`.
- Pastikan `up_next` mengecualikan video aktif dan duplikat, serta tidak melakukan fetch khusus rekomendasi.
- Gunakan `history_list` beserta `HistoryEntry.last_position` untuk progress UI; jangan menambah query progress terpisah.

### 3. Bangun pipeline thumbnail terpusat dan bounded

- Tambahkan modul UI kecil `src/ui/thumbnails.rs` yang menjadi satu-satunya jalur download/resize/cache/decode thumbnail.
- Request membawa `video_id`, URL, dan page generation. Empat worker memproses queue; setiap worker melakukan HTTP dengan timeout dan batas ukuran response, resize di worker, lalu menyimpan JPEG ke `thumb_blob` dalam batch/transaksi.
- Cache hit cukup mengembalikan JPEG terkompresi; decode ke RGB dan pembuatan texture dilakukan di worker sejauh aman, lalu widget GTK hanya diubah melalui callback main thread.
- Batasi response gambar (misalnya 2 MiB) agar metadata server yang anomalous tidak menghabiskan RAM.
- Patch texture berdasarkan `video_id`, bukan append; ini mempertahankan urutan Search/Discover/Channel dan mencegah duplikasi.
- Channel banner/avatar diproses di background dengan dimensi khusus. Cache asset channel memakai key namespace di `thumb_blob` (misalnya `channel:<id>:banner`) agar tidak memerlukan tabel baru dan tetap ikut bounded pruning.
- Hapus alur thumbnail bespoke di `src/ui/window.rs`, `src/ui/list_view.rs`, dan `src/ui/player.rs`; semua halaman memakai pipeline yang sama.

### 4. Bangun shell YouTube-like

Di `src/ui/window.rs`, `src/ui/layout.rs`, `src/ui/theme.rs`, `src/ui/style.css`, `src/ui/theme_dark.css`, dan `src/ui/theme_light.css`:
- Bentuk top bar, search form, sidebar, dan action buttons sebagai komponen yang dapat dipakai ulang.
- Hubungkan `LayoutSpec::for_width` ke breakpoint nyata; set min/max columns pada grid, set orientation/width Up Next, set compact class pada result list, dan ubah sidebar expanded/rail/hidden.
- Sidebar manual override berlaku sampai breakpoint berikutnya; pada narrow, hamburger membuka overlay/revealer sementara.
- Isi sidebar subscription maksimal 8 dari SQLite, cache-first, dan update setelah subscribe/refresh.
- Theme toggle memanggil `ThemeController::apply` dan `db::settings_set`; theme tidak melakukan network atau rebuild widget.
- Pertahankan CSS provider berjenjang: layout/component CSS terpisah dari palette Dark/Light; hindari selector global yang merusak popover/dropdown.

### 5. Rapikan komponen presentasi

- `src/ui/home.rs`: jadikan grid card sebagai satu komponen Home/Subscription/Channel; title dua baris, thumbnail 16:9, durasi, channel yang dapat diklik lewat button/accessibility action, progress optional, tanpa fake views.
- `src/ui/list_view.rs`: gunakan compact row untuk Search/History; pertahankan thumbnail virtualized, text wrapping stabil, loading/empty/error slots, dan urutan hasil stabil.
- `src/ui/channel.rs`: refactor header agar banner/avatar/header widget dapat di-update tanpa fetch di main thread; aktifkan subscribe dengan DB yang sama.
- `src/ui/player.rs`: aktifkan `set_layout`, `set_recommendations`, dan `set_on_up_next`; rapikan title/action row/time format H:MM:SS; tambahkan tooltip control; perbaiki seek agar keyboard dapat melepas state seeking.
- `src/ui/video_item.rs`: izinkan progress dan metadata presentasi tanpa menduplikasi model; texture atau bytes thumbnail ditangani pipeline, bukan logika per halaman.

### 6. Wiring seluruh halaman

- Home: query chip saat aktivasi, pertahankan chip aktif, dan refresh “Semua” hanya setelah background Discover selesai bila masih aktif. Jangan fetch Discover tambahan saat pindah chip.
- Subscription: refresh channel tetap berurutan untuk menjaga beban `yt-dlp`, tetapi thumbnail boleh diproses oleh pool bounded; batch write dan update status.
- Search: jalankan `yt-dlp`, simpan feed dalam batch, tampilkan fallback dalam urutan relevansi, lalu patch thumbnail; query generation baru membuang hasil lama.
- Channel: tampilkan cache lama segera, lalu header + video + asset background; generation guard melindungi seluruh header/grid; subscribe me-refresh state lokal tanpa reset navigasi.
- History: tampilkan 30 row dengan progress; tidak ada request network untuk membuka halaman.
- Watch/Up Next: saat metadata video diterima, query `feed::up_next` dari cache, muat thumbnail cache tanpa fetch tambahan, dan hubungkan klik ke playback callback yang sama.

### 7. Optimalkan playback, event, dan I/O

Di `src/ui/player.rs`:
- Ganti history write 500 ms dengan checkpoint 5 detik + flush pada pause/unmap/end-file; jangan menulis ulang posisi yang tidak berubah.
- Hilangkan query `channel_list` berulang dari setiap update metadata; gunakan status subscription yang di-cache atau query kecil saat video berubah.
- Ganti render wake-up timer 16 ms permanen dengan mekanisme event-driven yang aman untuk GTK thread; koalesikan beberapa update MPV menjadi satu render. Jika API GTK yang tersedia tidak memungkinkan, batasi polling hanya saat widget player ter-map/playing dan ukur dampaknya—jangan menambah timer lain.
- Pastikan timer/event async berhenti atau menjadi one-shot setelah request selesai; tidak boleh ada search/channel/discover poll yang tetap hidup setelah completion.
- Jangan mengubah streaming strategy: tetap URL langsung via yt-dlp + libmpv, tanpa download/transcode lokal.

### 8. Error handling dan failure modes

- `yt-dlp` gagal: tampilkan pesan ringkas dan tombol retry di halaman terkait; jangan menghapus cache lama yang masih bisa dibaca.
- Thumbnail gagal: pertahankan fallback, patch hanya hasil valid, dan lanjutkan item berikutnya.
- Channel/Search/Discover diganti cepat: generation check membuang metadata, grid, banner, dan thumbnail dari request lama.
- Resolve stream gagal: tampilkan status error pada player dan izinkan klik ulang video; URL lama hanya dipakai bila masih valid sesuai TTL.
- SQLite busy: gunakan `busy_timeout`, transaction, dan error state; jangan diam-diam kehilangan update history.
- Tema tidak valid/DB tidak tersedia: fallback ke Dark tanpa membuat window crash.

## Batas dan non-goals

- Tidak ada login/sinkronisasi akun YouTube.
- Tidak ada like lokal, komentar, share/copy-link, playlists, notifications, watch-later, atau server-side recommendation.
- Tidak ada kualitas selectable fungsional, autoplay, atau resume playback otomatis.
- Tidak ada migrasi database destruktif dan tidak ada perubahan format cache thumbnail.
- Tidak ada framework/dependency UI baru; GTK4 native tetap menjadi batas implementasi.

## Urutan rollout yang aman

1. Implementasikan data-layer helpers, generation state, dan thumbnail pipeline; pertahankan UI lama sebagai fallback sementara.
2. Bangun shell/responsive/theme tanpa mengganti route lama.
3. Migrasikan Home/Subscription ke komponen dan pipeline baru.
4. Tambahkan Search stable-order, History, lalu Channel/Watch Up Next.
5. Aktifkan pruning setelah semua jalur tulis thumbnail konsisten.
6. Ubah persistence history ke 5 detik setelah jalur flush pause/unmap/end-file stabil.
7. Hilangkan implementasi thumbnail/worker lama setelah semua halaman memakai pipeline bersama.

## Validasi

### Otomatis

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test --all-features`
- Tambahkan test core untuk: batas/prune cache tanpa menghapus history, batch upsert, `continue_feed`, dedupe `up_next`, settings theme, serta query feed setelah cleanup.
- Tambahkan unit test layout untuk 1920, 1366, 1024, 800, 640 dan perubahan breakpoint hanya ketika mode berubah.

### Manual

- Resolusi 1920×1080, 1366×768, 1024×768, 800×600, dan 640×480: sidebar, grid, Watch, dan Up Next tidak terpotong.
- Dark/Light di semua halaman, restart, dan verifikasi tema bertahan serta popover/dropdown tetap kontras.
- Home chips tidak memicu request tambahan; History membuka dari cache; Subscription empty/filled/refresh; Search rapid-query; Channel rapid-switch; video/Up Next rapid-switch.
- Offline, `yt-dlp` gagal, thumbnail gagal, DB locked/busy, cache kosong: UI tetap responsif dan menampilkan status yang benar.
- Video muxed serta video+audio terpisah, fullscreen, pause/unmap, page switch, dan Up Next: playback benar, tidak ada stream basi, tidak ada autoplay, history tetap akurat.

### Baseline performa/HDD

- Rekam pada mesin yang sama sebelum dan sesudah: cold/warm startup, RSS setelah window tampil, RSS setelah semua halaman dibuka, idle CPU, thread count, peak I/O, ukuran `state.db`/WAL, dan waktu Search/Channel/Discover.
- Verifikasi maksimal 4 thumbnail worker, tidak ada thread per hasil, tidak ada timer async setelah completion, dan update history turun dari ±2 write/detik ke ≤1 write/5 detik.
- Verifikasi cache mencapai plateau: prune tidak menghapus history dan feed/thumbnail dapat di-fetch ulang saat diperlukan.
- Targetkan median startup/RSS tidak regresi >10% dibanding baseline working tree; jika ada regresi, penyebab utama harus diperbaiki sebelum rollout selesai.
