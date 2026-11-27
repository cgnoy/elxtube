# Redesign GUI elxTube ala YouTube yang Tetap Ringan

## Tujuan dan keputusan

Redesign seluruh alur inti—Home, Watch, Search, Channel, Subscription, dan History—agar terasa seperti YouTube desktop, tanpa Electron, webview, libadwaita, atau dependensi berat baru. Pertahankan GTK4, SQLite, yt-dlp, libmpv, pemutar OpenGL, dan target perangkat lama.

Keputusan yang sudah disepakati:
- Sediakan **Dark (default) + Light**, dengan pilihan tema tersimpan di tabel `settings`.
- Layout responsif otomatis: nyaman di 1920×1080, optimal di 1366×768, dan menciut di bawahnya; sediakan tombol sidebar manual.
- Home berupa grid dengan chip **Semua / Subscription / Lanjutkan**, seluruhnya dari SQLite/cache tanpa request baru.
- Watch memiliki **Up Next** dari data nyata yang dapat diklik, tanpa autoplay.
- Kontrol placeholder yang sudah ada (views, like, share, kualitas, tab/filter channel) tetap dekoratif/nonfungsional sesuai permintaan. Jangan menambahkan like lokal, salin link, pergantian kualitas, autoplay, atau resume playback pada pekerjaan ini.
- Subscription tetap lokal tanpa login. Jangan mengubah model data YouTube atau menambah sinkronisasi akun.

## Batasan resource

- Tidak ada animasi crossfade, blur, shadow besar, video thumbnail preview, atau pekerjaan polling baru.
- Pertahankan virtualisasi `GridView`/`ListView`, thumbnail 320×180 JPEG q70, serta cache database.
- Batasi Home/Subscription/Channel/History masing-masing 30 item, Search 20, dan Up Next 12.
- Gunakan maksimal 4 worker bersama untuk thumbnail Search/Channel; jangan membuat satu thread per video.
- Tidak ada request jaringan saat berpindah chip Home, membuka History, atau mengganti tema.
- Jangan mempertahankan texture full-resolution; banner/avatar channel di-resize sebelum menjadi texture.

## Perubahan terurut

### 1. Siapkan validasi dan pecah orkestrasi UI secara minimal

- Perbaiki referensi lama `ytkw` menjadi `elxtube` di `tests/core_smoke.rs` dan nama binary di `run.sh`, agar aplikasi dan tes dapat divalidasi. Ini perbaikan mekanis, bukan redesign fitur.
- Pertahankan `window.rs` sebagai orkestrator, tetapi pindahkan pembangunan sidebar/top bar dan perhitungan mode layout ke modul UI kecil bila diperlukan agar redesign tidak menambah kompleksitas berlebihan.
- Tambahkan model presentasi seperlunya, misalnya constructor `VideoItem::new()` dan metadata progress opsional, daripada menduplikasi factory kartu.

### 2. Bangun sistem tema dan shell aplikasi

- Pisahkan CSS layout/komponen dari palet warna. Gunakan provider tema Dark/Light yang dapat diganti pada runtime; baca/tulis `settings.theme` melalui fungsi database yang sudah ada.
- Hentikan pemaksaan Light di `ui/mod.rs`. Terapkan tema tersimpan sebelum jendela dibuat, dengan fallback Dark bila nilai tidak valid.
- Top bar ala YouTube: tombol hamburger, brand elxTube dengan aksen merah, search rounded di tengah dengan tombol submit, dan tombol tema di kanan.
- Sidebar: Home, Subscription, History, pemisah, lalu maksimal 8 channel subscription dari SQLite dengan inisial avatar. Channel tambahan cukup tersedia melalui halaman Subscription; jangan menambah halaman manajemen baru.
- Tandai item navigasi aktif, sediakan tooltip saat sidebar menjadi rail, dan pertahankan navigasi keyboard/fokus GTK.

### 3. Terapkan layout responsif yang deterministik

Gunakan lebar konten aktual, bukan hanya ukuran layar, dan hitung ulang hanya ketika breakpoint berubah:

| Lebar jendela | Sidebar | Grid utama | Watch |
|---|---|---|---|
| ≥1600 px | Expanded ±240 px | 4 kolom, thumbnail elastis sampai ±320 px | Rekomendasi kanan ±360 px |
| 1180–1599 px | Expanded ±208 px | 3 kolom; optimal untuk 1366×768 | Rekomendasi kanan ±320 px |
| 800–1179 px | Rail ±72 px | 2–3 kolom sesuai ruang | Up Next pindah ke bawah bila kolom utama terlalu sempit |
| <800 px | Hidden; dibuka sementara lewat hamburger | 1–2 kolom | Satu kolom |

- Hamburger mengganti Expanded↔Rail pada layar lebar dan Hidden↔Rail pada layar sempit. Perubahan breakpoint berikutnya boleh menghitung ulang mode otomatis agar konten tidak terpotong.
- Hapus ukuran kartu tetap 480 px. Gunakan thumbnail 16:9, judul maksimal dua baris, metadata ringkas, margin/gutter konsisten, dan tinggi baris yang stabil agar tidak terjadi layout thrashing.
- Ganti tinggi player tetap 500 px dengan area 16:9 yang mengikuti lebar yang tersedia; pertahankan perilaku fullscreen dan render MPV yang sudah ada.

### 4. Redesign Home dan tambahkan filter lokal

- Susun header halaman sederhana dan chip **Semua / Subscription / Lanjutkan** dengan status aktif yang jelas.
- **Semua** memakai `discover_feed(30)`; **Subscription** memakai `subs_feed(30)`; **Lanjutkan** memakai history yang posisinya >15 detik dan belum melewati 90% durasi.
- Ekspos query continue sebagai fungsi core yang reusable; jangan menduplikasi SQL di UI dan jangan memakai `home_feed()` bila hasilnya mencampur kategori yang tidak sesuai chip.
- Filter hanya membaca database lalu mengganti isi `ListStore`. Tidak ada fetch Discover/Subscription baru dari chip.
- Pertahankan background Discover yang ada saat cache kurang dari 10 item. Setelah fetch selesai, refresh Home hanya bila chip Semua aktif.
- Tambahkan empty/error/loading state berbasis label GTK sederhana; jangan biarkan grid kosong tanpa penjelasan.

### 5. Redesign kartu, Search, dan History

- Home/Subscription/Channel memakai satu komponen kartu grid yang konsisten: thumbnail, durasi, inisial channel, judul, dan metadata. Search/History memakai kartu horizontal yang lebih ringkas.
- History mengambil `history_list(30)`, menampilkan thumbnail cache, judul/channel, posisi/durasi bila tersedia, dan progress bar tipis tanpa animasi. Klik item tetap membuka player melalui alur yang sudah ada; **jangan menambahkan resume otomatis**.
- Search memakai thumbnail 320×180 pada layar lebar dan lebih kecil pada mode compact. Pertahankan batas 20 hasil.
- Ganti thread thumbnail per hasil Search dengan queue `Arc<Mutex<VecDeque<_>>>` dan 4 worker, mengikuti pola Channel. Kirim item kembali ke main thread melalui channel yang sama.
- Tambahkan generation counter untuk Search agar respons query lama tidak masuk ke hasil query baru. Tampilkan kegagalan `yt-dlp` pada status halaman, bukan hanya log.
- Pertahankan teks views/deskripsi placeholder yang sudah ada sebagai dekorasi sesuai keputusan, tetapi jangan menambah placeholder baru pada History atau Up Next.

### 6. Redesign Watch dan Up Next

- Tata player, judul, baris channel/Subscribe, dan action row seperti YouTube. Action row boleh wrap pada layar sempit; kontrol like/share/kualitas tetap dekoratif/nonfungsional.
- Tambahkan kolom **Up Next** berisi `feed::up_next(current_id, current_channel, 12)`. Gunakan data dan thumbnail cache yang tersedia; item yang belum punya texture memakai fallback, tanpa fetch tambahan khusus rekomendasi.
- Klik Up Next memakai `create_player_callback()` yang sama sehingga cache URL, resolve, history, dan tombol Back tetap konsisten. Tidak ada autoplay setelah `end-file`.
- Muat ulang rekomendasi ketika video aktif berubah dan cegah item video saat ini/duplikat tampil.
- Saat berpindah video dengan cepat, abalkan hasil resolve/metadata/rekomendasi dari generation lama agar tidak menimpa video terbaru. Jangan mengubah mekanisme render OpenGL kecuali diperlukan oleh perubahan ukuran player.

### 7. Redesign Channel dan Subscription

- Channel: banner proporsional, avatar, nama/handle/subscriber/description, tab/filter dengan status aktif visual, dan grid video. Pertahankan tab/filter serta tombol Subscribe channel sebagai dekorasi bila memang belum memiliki handler.
- Ambil avatar dan banner di background bersama metadata channel. Resize avatar sekitar 96×96 dan banner maksimal sekitar 960×180; jangan download/resize di callback main thread seperti saat ini.
- Perluas generation counter Channel ke hasil header, bukan hanya thumbnail, agar respons channel lama tidak menimpa channel baru.
- Subscription: judul halaman, tombol refresh yang jelas, grid yang sama dengan Home, dan status jumlah channel/empty state. Refresh tetap mengambil 15 video per channel secara berurutan dan maksimal 4 worker thumbnail bila diparalelkan.
- Setelah refresh, muat ulang Subscription dan Home hanya jika filter Home yang aktif memang bergantung pada data subscription. Jangan memicu Discover fetch tambahan.

### 8. Rapikan cache dan jalur kegagalan tanpa mengubah fitur inti

- Sentralisasi helper thumbnail agar download, resize, fallback, penyimpanan blob, dan pengiriman texture tidak diduplikasi dengan perilaku berbeda.
- Semua hasil thread yang menyentuh GTK harus kembali melalui main-thread callback. Worker hanya menangani proses/HTTP/database dan data yang aman dikirim.
- Tampilkan status kegagalan Search, Channel, refresh Subscription, dan resolve stream di halaman terkait; selalu pulihkan tombol dari keadaan loading setelah sukses, gagal, atau channel terputus.
- Cache kosong/gagal tidak boleh menghapus data lama yang masih bisa ditampilkan. Jika pengguna berpindah halaman, hasil asynchronous lama harus dibuang melalui generation check.
- Tidak ada migrasi schema yang diperlukan: gunakan key `theme` pada tabel `settings` dan struktur history/feed yang sudah ada.

## Validasi

1. Jalankan `cargo fmt --check`, `cargo clippy --all-targets`, dan `cargo test --all-features`.
2. Uji manual pada jendela 1920×1080, 1366×768, 1024×768, dan 800×600: tidak ada kartu terpotong, watch tetap terbaca, sidebar/hamburger bekerja, dan fullscreen tetap benar.
3. Uji Dark/Light pada semua halaman, lalu restart aplikasi untuk memastikan tema bertahan dan kontras label/popover/dropdown tetap terbaca.
4. Uji chip Home tanpa jaringan tambahan, History dari data tontonan nyata, Subscription kosong/terisi/refresh, serta Discover pertama kali ketika cache kosong.
5. Uji pencarian cepat bergantian, perpindahan channel cepat, dan klik video/Up Next cepat bergantian; hasil lama tidak boleh menimpa tampilan terbaru.
6. Uji Search/Channel tanpa koneksi, `yt-dlp` gagal, thumbnail gagal, dan cache kosong; UI harus menampilkan status dan tetap responsif.
7. Putar video muxed serta video+audio terpisah, pindah halaman, kembali, fullscreen, lalu pilih Up Next. Pastikan tidak ada autoplay dan history tetap tersimpan.
8. Bandingkan startup time, RSS saat window tampil, RSS setelah semua halaman dibuka, serta CPU idle dengan baseline pada mesin yang sama. Targetkan tidak ada regresi median >10%; tidak boleh ada thread/timer baru yang terus aktif saat idle.

## Hasil yang diharapkan

Pengguna mendapatkan shell, kartu, watch page, channel, subscription, dan history yang lebih dekat dengan YouTube, dengan tema dan layout responsif, sementara pengambilan data tetap cache-first dan batas item/worker menjaga penggunaan resource. Perubahan tidak mencakup login, rekomendasi server, autoplay, resume otomatis, like lokal, salin link, atau kualitas video fungsional.
