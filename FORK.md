# Zed Fork: catatan perubahan

Dokumen ini mencatat apa saja yang fork ini ubah dari upstream Zed, supaya waktu upstream update dan fork mengikuti, konteks tiap perubahan tetap jelas.

## Basis

- Branch kerja: `custom` (remote `hehenugas/zed-fork`). Remote `upstream` = `zed-industries/zed`.
- Basis upstream saat ini: commit `aed37547d8` ("Bump to 1.24.4"), yaitu stable v1.24.x. Semua perbedaan fork dihitung dari sini:
  `git diff aed37547d8 custom --stat` (47 file).
- Branch lokal `main` BUKAN basis. Isinya upstream main yang lebih baru dan sudah menyimpang dari stable, jadi `git diff main custom` penuh noise upstream.
- Sinkron ke upstream: `script/fork-sync.sh` (jalan dari `custom`, merge tag stable terbaru lalu `cargo check -p zed`).

## Prinsip fork

1. **Kompatibel dengan Zed asli.** Fork memakai direktori config yang sama (`~/.config/zed`, atau data Flatpak `~/.var/app/dev.zed.Zed` bila ada) dan tidak boleh merusak `settings.json`/`keymap.json` yang juga dibaca Zed asli. Karena itu setting khusus fork dipisah ke `fork_settings.json`, dan migrasi otomatis dimatikan.
2. **Perubahan UI mengarah ke gaya VS Code / Codex / Cursor.**
3. **Identitas terpisah** (app id, nama, socket CLI, ikon) supaya bisa berjalan berdampingan dengan Zed asli.

## Perubahan fungsional

### 1. Setting khusus fork (`fork_settings.json`)
- File: `~/.config/zed/fork_settings.json`. Di-merge JSON-wise di atas `settings.json`, jadi Settings UI tidak menulis ke file ini (edit manual).
- `crates/paths/src/paths.rs`: `fork_settings_file()`.
- `crates/settings/src/settings_store.rs`: `fork_settings_content`, `with_fork_settings`, `set_fork_settings`, watcher kedua; status migrasi selalu `NotNeeded`.
- `crates/zed/src/zed/migrate.rs`: `should_migrate_settings`/`should_migrate_keymap` selalu `false` (Zed asli tidak bisa membaca format yang sudah dimigrasi).
- `crates/json_schema_store/src/json_schema_store.rs`: schema juga berlaku untuk file fork.
- Setting baru (default di `assets/settings/default.json`, tipe di `crates/settings_content`):
  - `activity_bar` (false): tombol panel di bar vertikal tepi window, bukan di status bar.
  - `island_layout` (true): dock dan area editor jadi panel bulat terpisah dengan jarak.
  - `agent.threads_sidebar.scope` (`current_project` | `all`): proyek mana yang tampil di sidebar threads dan riwayat.
  - `git_panel.multi_repo` (true), `git_panel.hide_clean_repos` (false).
- Catatan: nilai default tetap ada di `default.json`; menaruhnya di `settings.json` biasanya aman karena Zed asli mengabaikan key yang tidak dikenal, tapi sengaja dihindari.

### 2. Layout island (mirip VS Code)
- `crates/workspace/src/workspace.rs`: `ISLAND_MARGIN`, `render_dock`, `render_center`, warna pemisah (warna title bar); `render_activity_bar` bila `activity_bar` aktif.
- `crates/workspace/src/dock.rs`: border/rounded saat island; `ActivityBarButtons`; **tombol panel di baris atas dock kiri** (`Dock::top_buttons`, `PanelButtons::for_top_bar`), menggantikan tombol dock kiri di status bar. Dock kanan tetap memakai status bar.
- `crates/workspace/src/multi_workspace.rs`: sidebar selalu dirender saat terbuka; layout dock dibawa saat ganti workspace (`capture_shared_dock_state`/`apply_shared_dock_state`).

### 3. Tombol layout di title bar
- `crates/title_bar/src/title_bar.rs`: `render_layout_toggles` (hanya saat `island_layout`): chat (threads sidebar), kiri, bawah, kanan. Ikon terisi bila terbuka. State dock disimpan di field `docks` (lihat jebakan #1).
- Ikon baru: `assets/icons/{panel_left,panel_bottom,panel_right}[_filled].svg`, `chat_sparkle[_filled].svg`; terdaftar di `crates/icons/src/icons.rs`.

### 4. Threads sidebar gaya Codex + popover riwayat
- `crates/sidebar/src/sidebar.rs` (+ `sidebar_tests.rs`): grup berupa folder, baris kecil satu baris dengan logo agent, "Show more"/"Show less" (batas 5 baris, tidak dipersist: `COLLAPSED_ROW_LIMIT`, `expanded_row_groups`, `ListEntry::ShowMore`), overlay riwayat selalu aktif.
- `crates/agent_ui/src/threads_archive_view.rs`: mode `popover` (tanpa window controls).
- `crates/agent_ui/src/agent_panel.rs`, `crates/zed_actions/src/lib.rs`: tombol dan action `ToggleThreadHistory`.
- `crates/agent_settings`, `crates/settings_content/src/agent.rs`: `ThreadsSidebarScope`.
- Test sidebar memakai helper scope `all` di inisialisasi.

### 5. Git panel multi-repo gaya VS Code
- `crates/git_ui/src/git_panel.rs` (+ `git_panel_settings.rs`): tiap repo jadi section (`repository_override`), urutan: header → kotak commit → daftar file. Header: nama repo, branch (klik = branch picker), Refresh, Fetch, Pull, Push dengan hitungan incoming/outgoing, jumlah perubahan.
  - Daftar file tidak dibatasi tinggi, scroll tunggal di panel kanan.
  - Input commit di section mulai 2 baris, maksimal 6 (`section_commit_editor_mode`); tombol fill disembunyikan.
  - Subscription git store: section menyegarkan dirinya untuk event repo miliknya (flag `bool` di `RepositoryUpdated` hanya menandai repo aktif).
- `crates/project/src/git_store.rs`: `Repository::reload_git_state` (public, memanggil `schedule_scan`) untuk tombol refresh.

### 6. Agent chat
- `crates/agent_ui/src/message_editor.rs`: background composer transparan.
- `crates/agent_ui/src/conversation_view/thread_view.rs`: composer tumbuh otomatis (`AutoHeight`, scroll bila penuh); kartu terminal dan command block didesain ulang.
- `crates/agent_ui/src/ui/terminal_tool_header.rs`: header baru (ikon, "Running/Ran command", disclosure selalu terlihat).

### 7. Identitas fork
- `crates/release_channel/src/lib.rs`: nama "Zed Fork", app id `dev.zed.Zed-Fork[-Channel]`, tanpa polling update.
- `crates/cli/src/main.rs`, `crates/zed/src/zed/open_listener.rs`: path socket CLI memuat app id agar tidak bentrok dengan Zed asli.
- `crates/paths/src/paths.rs`: `APP_NAME` tetap "Zed" (sengaja, agar berbagi data); isolasi via `--user-data-dir` / `ZED_FORK_USER_DATA_DIR`.
- Ikon fork: `script/fork-icon/zed-fork-{512,1024}.png`.

## Build, rilis, instalasi

- `.github/workflows/fork_appimage.yml`: dijalankan manual (`gh workflow run fork_appimage.yml --ref custom -R hehenugas/zed-fork`) di runner self-hosted `[self-hosted, linux, x64, zed-fork]`. Hasil: prerelease `fork-<run>` + `SHA256SUMS.txt` + artifact 7 hari. `AppRun` men-share config/data Flatpak Zed bila ada.
- Build cepat (env di workflow): `ZED_FORK_FAST=1`, `CARGO_PROFILE_RELEASE_{LTO=off, CODEGEN_UNITS=16, DEBUG=0, INCREMENTAL=true}`. `Cargo.toml` tidak diubah. Hasil: dari ±25-30 menit jadi ±2,5-3 menit dengan cache hangat. Konsekuensi: performa runtime sedikit lebih rendah, lisensi pihak ketiga berisi placeholder.
- `script/bundle-linux`: knob `ZED_FORK_FAST` melewati `generate-licenses` dan build `remote_server` (SSH/WSL remoting tidak tersedia di AppImage fork).
- `script/build-fork-linux`: pembungkus `bundle-linux --skip-sentry`.
- `script/install-fork-appimage.sh`: unduh rilis terbaru, pasang ke `~/Applications/zed-fork.AppImage` (ganti atomik lewat `.new` + `mv -f`), buat `~/.local/share/applications/zed-fork.desktop`, symlink `~/.local/bin/zed-fork`, ikon di `~/.local/share/icons/hicolor/512x512/apps/zed-fork.png`. Path stabil, jadi update tidak perlu menyentuh `.desktop`.
- `README.md`: dua baris `> [!IMPORTANT]` di atas adalah aturan wajib dari `CLAUDE.md` saat mengubah source; jangan dihapus oleh agent.

## Jebakan yang pernah kena

1. **Jangan membaca `Workspace` dari render/konstruktor entity yang dipegang Workspace** (`workspace_entity.read(cx)`): panic `cannot read workspace::Workspace while it is already being updated`. Di `TitleBar::new` pakai `&Workspace` dari argumen dan simpan `Entity<Dock>` sendiri.
2. Setelah `cargo fmt`, pola ganti teks (replace script) bisa tidak cocok lagi; verifikasi dengan `grep`/`git diff` bahwa edit benar-benar masuk.
3. `gh` punya dua akun; yang berhak tulis: `hehenugas` (`gh auth switch -u hehenugas`). Selalu pakai `-R hehenugas/zed-fork` (default gh menunjuk upstream). Akun aktif pernah kembali sendiri ke `Aorus22` (403 saat push).
4. Langkah "Publish release" pernah gagal sekali dengan 403 "Resource not accessible by integration"; AppImage tetap ada di artifact run (`gh run download <id> -n zed-fork-appimage`).
5. Hanya satu build pada satu waktu: batalkan build lama (`gh run cancel`) bila commit baru menyusul.

## Titik rawan konflik saat merge upstream

Urut dari yang paling mungkin konflik:
`crates/git_ui/src/git_panel.rs`, `crates/workspace/src/workspace.rs`, `crates/workspace/src/dock.rs`, `crates/workspace/src/multi_workspace.rs`, `crates/sidebar/src/sidebar.rs` (+ tests), `crates/agent_ui/src/{agent_panel.rs,threads_archive_view.rs}`, `crates/agent_ui/src/conversation_view/thread_view.rs`, `crates/title_bar/src/title_bar.rs`, `crates/settings/src/settings_store.rs`, `crates/release_channel/src/lib.rs`, `script/bundle-linux`.

Checklist setelah sinkron: `cargo check -p zed`, `cargo test -p sidebar -p git_ui -p settings`, jalankan AppImage hasil build sebentar dari terminal (cek panic saat startup), lalu cek tampilan: island, tombol title bar, git panel multi-repo, sidebar threads.

## Belum dikerjakan / ide lanjutan

- Settings UI belum menulis ke `fork_settings.json`.
- Panel git satu repo belum punya tombol refresh.
- Test penuh `agent_ui` dan `workspace` belum pernah dijalankan.
- Tombol "customize layout" (VS Code) belum ada.
- Desain kartu terminal adalah tebakan gaya Codex.
