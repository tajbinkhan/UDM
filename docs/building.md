# Building Pullbay

Where the code lives, how to build and package it, and how to add a language.

## Layout

The Rust code is a Cargo workspace with one crate per layer. `AGENTS.md`
gives the rules between them; its section 16 says where the code still
differs.

| Path | What it is |
| --- | --- |
| `crates/domain/` | Pure rules, no I/O: the download model (`download.rs`), the list kept by both the daemon and the window (`table.rs`), queues and schedules (`queue.rs`), URL patterns and pasted lists (`batch.rs`), site patterns (`site.rs`), file names (`filename.rs`), settings and their defaults (`settings.rs`), streams (`media.rs`), the grabber's model (`grab.rs`), and the port traits (`ports.rs`) |
| `crates/application/` | Use cases, one per file: `downloads/`, `queues/`, `quota/`, `power/`, `update/`, `grab/`; the daemon's `Command`s and `Event`s and its service (`daemon/`); the window's ports (`window.rs`); in-memory adapters for tests (`memory.rs`) |
| `crates/infrastructure/src/engine/` | Download engine: segments, resume sidecar, speed limiter, HTTP, FTP, streams |
| `crates/infrastructure/src/media/` | HLS playlists, DASH and HDS manifests, yt-dlp pages, ffmpeg conversion |
| `crates/infrastructure/src/protocol.rs`, `wire.rs` | Framing and wire messages, and their mapping to `Command` and `Event` |
| `crates/infrastructure/src/ipc.rs`, `server.rs`, `daemon_client.rs` | Daemon address, start-up, token file and HMAC handshake; the daemon's side and the window's side of the connection |
| `crates/infrastructure/src/store.rs`, `history.rs`, `state.rs` | The daemon's download list, queues and kept requests, and their files |
| `crates/infrastructure/src/settings.rs`, `live_settings.rs` | The settings file, its migrations and the checks that read the disk |
| `crates/infrastructure/src/transfers.rs`, `probes.rs`, `connect.rs` | The engine's adapter in the daemon, server probes, and the proxy and logins a download uses |
| `crates/infrastructure/src/net.rs`, `pac.rs`, `ntlm.rs` | Proxies, PAC scripts, site logins and HTTP authentication |
| `crates/infrastructure/src/grab.rs`, `crawler.rs` | The site grabber's crawl and `robots.txt`, and its project files |
| `crates/infrastructure/src/paths.rs`, `logging.rs` | Every folder Pullbay uses, and the log files |
| `crates/infrastructure/src/dpapi.rs`, `registry.rs`, `autostart.rs`, `browsers.rs` | Sealing saved requests and passwords, registry values, start at sign-in, and registering the host with each browser |
| `crates/infrastructure/src/notify.rs`, `scan.rs`, `power.rs`, `update.rs` | Notifications and sounds, virus scans, sleep and shut down, the update check |
| `crates/infrastructure/src/listfile.rs`, `from_uidm.rs` | Exporting and importing the download list; moving UIDM's files over |
| `crates/ui/src/` | The window: its state and messages (`state.rs`), views, dialogs, tray icon (`tray.rs`), tools for many downloads (`tools.rs`), and translations (`i18n.rs`, `lang/`) |
| `crates/app/src/bin/pullbay-daemon.rs` | Daemon: wires the adapters into the service, starts the background threads and serves clients |
| `crates/app/src/bin/pullbay-gui.rs` | Desktop window: builds its adapters and starts `pullbay-ui` |
| `crates/app/src/bin/pullbay-host.rs` | Native-messaging relay, `send` for other programs, and CLI test mode |
| `crates/app/src/bin/pullbay-cli.rs`, `crates/application/src/remote/`, `crates/infrastructure/src/tool.rs` | The command-line tool: its wiring, its commands (use cases over the daemon connection), and its arguments, output and exit codes |
| `extension/` | MV3 extension: capture, context menu, popup |
| `extension/_locales/` | The extension's text in each language |
| `extension/firefox.json` | What the Firefox copy changes in `manifest.json` |
| `extension/rules.js` | Capture rules (file types, minimum size, excluded sites) |
| `extension/links.js` | The links of a page for "Download all links" and "selected links" |
| `extension/post.js` | Matching downloads to the form POSTs that started them |
| `extension/request.js` | The request behind a download: headers, redirects, response, proxy setting |
| `extension/media.js` | Which responses are videos and streams, per tab, and the video menu |
| `extension/content.js` | The video button over playing videos |
| `extension/test/` | Node tests for the rules and the service worker |
| `assets/icon.svg` | The app icon's source; `installer/render-icons.js` renders every icon file below from it |
| `assets/icon-64.rgba` | Window and tray icon, raw RGBA rendered from `assets/icon.svg` |
| `host/com.pullbay.host.json` | Template of the native-messaging manifest |
| `install-host.ps1` | Builds, registers with Chrome, Edge and Firefox, adds the Start menu entry |
| `installer/pullbay.iss` | Inno Setup script for `Pullbay-Setup-<version>.exe` |
| `installer/build.ps1` | Builds the programs and the installer into `dist\` |
| `installer/linux/` | `package.sh` (the `.deb`, `.rpm`, AppImage and `.tar.gz`), the tarball's `install.sh`, the AppImage's `AppRun` and the desktop entry |
| `installer/macos/package.sh` | Builds the universal `.dmg` |
| `installer/make-firefox.ps1` | Makes the Firefox copy of the extension, or with `-Check` tells whether one is out of date |
| `installer/make-firefox.js` | The same for macOS and Linux, with `--check` |
| `installer/render-icons.js` | Renders `assets/icon.svg` to the extension's PNGs, `assets/icon-256.png`, `assets/icon-1024.png` and `assets/icon-64.rgba` with headless Chrome |
| `installer/pullbay.ico` | Setup and shortcut icon, packed from `extension/icons` by `make-icon.js` |
| `docs/adr/` | Architecture decisions |
| `GLOSSARY.md` | The names of the windows and dialogs |

## Build

On macOS and Linux, a stable Rust toolchain is enough, plus on Linux the GTK 3
and OpenSSL development packages (`sudo apt install libgtk-3-dev libssl-dev`
on Ubuntu):

```bash
cargo test --workspace
cargo build --release
```

`installer/linux/package.sh` makes the `.deb`, `.rpm`, AppImage and
`.tar.gz` in `dist/` (it needs `dpkg-deb`, `rpmbuild` and Node, and
downloads `appimagetool` unless `$APPIMAGETOOL` names one);
`installer/macos/package.sh` makes the universal `.dmg` on a Mac. Name
formats to make only those: `installer/linux/package.sh --skip-build deb`.

Rust on Windows needs a linker. This project builds with the
`stable-x86_64-pc-windows-gnu` toolchain plus MinGW-w64 (WinLibs) on `PATH`.

```bash
cargo test --workspace
cargo build --release
node --test extension/test/*.test.js
```

To try a build next to the installed Pullbay, give it a name in
`PULLBAY_INSTANCE`. That copy keeps its settings and list in `Pullbay-<name>`
folders, runs its daemon on a port of its own, and shows the name in the
window's title. It does not end other daemons when it restarts its own.

```powershell
$env:PULLBAY_INSTANCE = "test"; .\target\debug\pullbay-gui.exe
```

A string the window shows needs its German, Spanish and French translations
in `crates/ui/src/lang/`; a test fails for one without them.
`crates/ui/src/lang/untranslated.json` lists the strings that were untranslated before
that test came in, and an entry leaves the list once it is translated.

GitHub Actions builds and tests Pullbay on Windows, macOS and Linux on every
push and pull request (`.github/workflows/build.yml`), and the extension's
tests with Node. Those runs keep no files.

Each run also fails on a clippy warning or on code that rustfmt would
change. Run the same checks before pushing:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`rustfmt.toml` sets the line width to 120. Clippy checks only the code for
the platform it runs on, so a warning in a macOS-only or Linux-only block
shows up on that runner. With the target added through `rustup`,
`cargo clippy --target x86_64-pc-windows-gnu` or
`--target x86_64-apple-darwin` lints that code from Linux too.

The installers come from `.github/workflows/release.yml`. Set the version in
`Cargo.toml`, commit, and push a tag that matches it:

```bash
git tag v0.4.0
git push origin v0.4.0
```

It builds every file in the table under [Install](install.md) on Windows,
macOS and Linux runners and attaches them to a new GitHub Release for that
tag. Release files don't count against the repository's storage (each may be
up to 2 GB), and Pullbay's update check reads the latest release. The build
files are kept as artifacts for one day only, to hand them from the
builders to the release. Starting the workflow by hand
(**Actions > Release > Run workflow**) builds the same files without a
release, as artifacts for a day.

To build the installer, install [Inno Setup 6](https://jrsoftware.org/isinfo.php)
(`winget install JRSoftware.InnoSetup`), then run:

```powershell
.\installer\build.ps1
```

It builds the release programs and writes `dist\Pullbay-Setup-<version>.exe`,
with the version from `Cargo.toml`. Pass `-SkipBuild` when `target\release`
is already current.

The Rust tests also run on Linux under Wine, cross-compiled with
`cargo test --target x86_64-pc-windows-gnu` and
`CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine` (needs the `mingw-w64` and
`wine64` packages).

`cargo build --release` produces `pullbay-host.exe`, `pullbay-daemon.exe`,
`pullbay-gui.exe` and `pullbay-cli.exe` in `target\release`. The host, the window
and the command-line tool start the daemon from their own folder, so keep them
together. The window renders through
OpenGL, which needs a working graphics driver.

To run one download without the browser, window or daemon:

```bash
cargo run --release --bin pullbay-host -- download https://example.com/file.zip
```

### Adding a language

The window's strings are written in English in the code, and that English is
each string's ID: `tr("Save")`, or `trf!("{count} files", count = n)` for text
with values. A language is one JSON file in `crates/ui/src/lang/`, mapping each
English string to its translation, plus a line in `LANGUAGES` in
`crates/ui/src/i18n.rs`; a missing string shows in English. The tests check that
every key is still used and keeps its `{placeholders}`. The extension's text is
in `extension/_locales/<code>/messages.json`, as Chrome expects; its tests
check that every locale has the same messages as `en`. egui draws text without
shaping it, so scripts that need shaping, such as Bengali, Hindi or Arabic,
would not display correctly yet.

---

[← Back to the help pages](README.md)
