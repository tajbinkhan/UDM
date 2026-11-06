# AGENTS.md

This file is the single source of truth for how any AI agent or developer writes code in UIDM. Read it before making any change. Rules use MUST, MUST NOT and SHOULD in their usual sense.

The architecture below is the target. The code today is one crate that does not follow it yet. Section 16 lists where it deviates. New code MUST follow these rules. Existing code MUST NOT be restructured to match them unless a task asks for it.

## 1. Project overview

- UIDM is a Rust desktop download manager that follows Internet Download Manager's behavior (see `GLOSSARY.md` for the names of its windows).
- It runs as three processes:
  - `uidm-host`, the native-messaging host the browser launches. It relays browser messages to the daemon.
  - `uidm-daemon`, the long-lived process that owns the download engine, so downloads outlive the browser.
  - `uidm-gui`, the desktop window. It is a client of the daemon, like the host.
- The browser extension lives in `extension/` (JavaScript). It is outside the Rust architecture but speaks the protocol defined in Rust.
- Architecture: Hexagonal (Ports and Adapters) as a Cargo workspace, with a UI layer at the edge.
- UI framework: **egui**, through `eframe` 0.31 (glow/OpenGL backend), with `egui_extras` for the download table, `tray-icon` for the tray, and `rfd` for native folder pickers.

## 2. Workspace layout

```
crates/
  domain/          entities, value objects, business rules, port traits
  application/     use cases that coordinate the domain through ports
  infrastructure/  adapters: file storage, HTTP/FTP engine, OS integration, IPC
  ui/              egui views, UI state, messages, event handling
  app/             binaries: uidm-host, uidm-daemon, uidm-gui (main.rs files, config, wiring)
```

How UIDM's current concerns map onto these crates:

| Concern | Crate |
| --- | --- |
| Download states, queues and schedules, categories, file-name rules, batch URL patterns | `domain` |
| Add a download, pause, resume, run a queue, grab a site, export the list | `application` |
| HTTP/FTP transfers, HLS/DASH/HDS, settings and history files, DPAPI, registry, tray, toasts, power, recycle bin, browser registration, daemon IPC | `infrastructure` |
| Main window, save-as dialog, download windows, Options, translations | `ui` |
| Process startup, logging setup, wiring adapters to use cases | `app` |

The `app` crate MAY hold more than one binary (`[[bin]]` entries), since UIDM ships three. Each binary is its own composition root.

## 3. Dependency rule

This is the most important rule. Dependencies point inward only.

- `domain` depends on nothing internal. Allowed external crates are pure ones such as `thiserror`, `uuid`, `chrono`, `url` (parsing only) and `serde` (derive only). It MUST NOT depend on `tokio`, `reqwest`, `rusqlite`, `sqlx`, `eframe`, `egui`, `windows`, `libc` or any other I/O, async runtime or UI crate.
- `application` depends only on `domain`.
- `infrastructure` depends on `domain`, and on `application` only to implement a port that `application` defines.
- `ui` depends on `application` and `domain`. It MUST NOT depend on `infrastructure`.
- `app` depends on everything and is the only crate that wires concrete adapters into use cases.

| Crate | MAY depend on | MUST NOT depend on |
| --- | --- | --- |
| `domain` | pure crates (`thiserror`, `uuid`, `chrono`, `url`, `serde` derive) | `application`, `infrastructure`, `ui`, `app`, any I/O, async or UI crate |
| `application` | `domain`, pure crates | `infrastructure`, `ui`, `app`, I/O crates, UI crates |
| `infrastructure` | `domain`, `application` (ports only), I/O crates (`reqwest`, `tokio`, `windows`, `rfd`, ...) | `ui`, `app`, UI crates |
| `ui` | `domain`, `application`, `eframe`, `egui`, `egui_extras` | `infrastructure`, `app` |
| `app` | every crate, `anyhow` | nothing is forbidden |

Platform-specific dependencies (`windows`, `libc`, `gtk`, `trash`) belong in `infrastructure` (or `ui`, when they only serve drawing, such as the tray or drag and drop) under `[target.'cfg(...)'.dependencies]`.

## 4. Domain layer rules

- MUST use newtypes for every meaningful value, for example `DownloadId(u64)`, `QueueId`, `ByteCount(u64)`, `Url`, `FileName`. MUST NOT pass raw `String`, `u64` or `i64` for domain concepts.
- Parse, don't validate. Constructors return `Result`, for example `FileName::parse(&str) -> Result<FileName, DomainError>`. A value that exists is always valid.
- MUST model states with enums so illegal states cannot be represented. A download is `Queued`, `Downloading { .. }`, `Paused { .. }`, `Complete { .. }` or `Failed { reason }`, not a set of booleans.
- Ports are traits defined here, for example `trait DownloadRepository: Send + Sync` and `trait Transfer: Send + Sync`.
- MUST NOT perform I/O, start threads, use an async runtime, read the clock or environment directly, or log. Time comes in as a parameter or through a `Clock` port.

## 5. Application layer rules

- One use case per file and struct, for example `AddDownload`, `PauseDownload`, `RunQueue`, `GrabSite`.
- Use cases are generic over ports (`AddDownload<R: DownloadRepository>`), or take `Arc<dyn Port>` when dynamic dispatch is needed.
- Use cases orchestrate. Business rules live in the domain.
- Input and output DTOs live here, not in `ui`.
- MUST NOT depend on the wire protocol's JSON shapes. Mapping protocol messages to DTOs is an adapter's job.

## 6. Infrastructure layer rules

- Each adapter implements a port trait from `domain` (or from `application`).
- File formats, file paths, HTTP details, Win32 calls and the IPC wire format MUST NOT leak out of this crate.
- UIDM stores its state in JSON files (`settings.json`, `history.json`, saved requests). Writes MUST be atomic (write to a temporary file, then rename). If a database is added later, its migrations live here and run at startup. Format changes to existing JSON files MUST read the old format.
- SHOULD use the `directories` crate for OS-specific data, config and log paths. MUST NOT hardcode paths or read `APPDATA`, `LOCALAPPDATA` or `XDG_*` outside the one module that resolves them.
- MUST map external errors (`io::Error`, `reqwest::Error`, `windows::core::Error`) into port error types at this boundary.

## 7. UI layer rules

- UI code is a thin adapter. It translates user events into use case calls and results into view state. MUST NOT put business logic in views, callbacks or update functions.
- MUST NOT block the UI thread. Daemon requests, file access and network work run on worker threads or tasks, and results come back as messages.
- Use unidirectional data flow: one state struct, changed only through messages.

egui-specific rules:

- Keep an `AppState` struct separate from the `eframe::App` implementation. `update` reads `AppState` to draw and turns clicks and keystrokes into `UiMessage` values. A single `apply(&mut AppState, UiMessage)` function changes the state.
- Talk to workers through channels (`std::sync::mpsc` or `crossbeam-channel`). Workers send results back and call `ctx.request_repaint()` so egui wakes up. `update` drains the channel with `try_recv`, never `recv`.
- MUST NOT call the daemon, read files or sleep inside `update` or any `show` function. egui calls these every frame.
- Every dialog and download window is drawn during the main window's frame (see `docs/adr/0001-cloak-the-main-window-in-the-tray.md`). Code that changes how windows are hidden MUST keep that frame running.
- User-facing text goes through the translation tables in `ui` (today `crates/app/src/bin/gui/lang/*.json`). MUST NOT hardcode English strings in views.
- Before copying an IDM window, dialog or tray behavior, observe IDM first (the `observe-idm` skill in `.claude/skills/`).

## 8. App (composition root) rules

- Each binary's `main.rs` loads config, initializes `tracing`, opens storage, runs any migrations, constructs adapters, injects them into use cases, and starts its loop (the egui window, the daemon's IPC server, or the host's relay).
- MUST NOT hold business logic.

## 9. Error handling

- Each layer has its own error enum built with `thiserror`, with `From` conversions between layers.
- `anyhow` is allowed only in the `app` crate.
- MUST NOT use `Result<_, String>` as an error type in new code.
- MUST NOT use `unwrap()`, `expect()` or `panic!` in non-test code, except for invariants that are truly guaranteed and documented with a comment next to the call. A poisoned mutex counts as such an invariant only when the comment says why.
- Errors shown to users are mapped to friendly, translated messages in the UI layer.

## 10. Async and concurrency

- SHOULD prefer generics (static dispatch). Use `Arc<dyn Trait>` only when needed, and `async-trait` for async traits used as trait objects.
- Shared services are wrapped in `Arc`. SHOULD avoid `Mutex` in domain and application code and prefer message passing.
- The engine runs async HTTP on one shared `tokio` runtime inside `infrastructure`. Other crates MUST NOT create their own runtime or depend on `tokio`.

## 11. Logging and observability

- Use `tracing` with structured fields, for example `tracing::info!(download_id = %id, bytes, "segment done")`.
- MUST NOT commit `println!`, `eprintln!` or `dbg!`. The exception is CLI output that is the program's actual output (the `uidm-host` test mode).
- Logs go to a file in the app's data directory. Logs MUST NOT contain cookies, passwords, auth headers or the daemon token. Download addresses are allowed, since the log is readable only by the user.

## 12. Testing

- Domain: pure unit tests, no mocks needed.
- Application: test use cases with in-memory adapters that implement the port traits.
- Infrastructure: integration tests against temporary files and directories (and a temporary database, if one is added), and against the local test HTTP server (today `crates/infrastructure/src/engine/testserver.rs`). Tests MUST NOT reach the internet.
- Every new use case MUST come with tests.
- Changes to the browser extension MUST pass `node --test extension/test/*.test.js`.

## 13. Quality gates

A task is not done until all of these pass:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

CI runs them on Windows, macOS and Linux (`.github/workflows/`). Code under `cfg(windows)` or `cfg(unix)` is only checked on that platform, so state which platforms you could not build on.

## 14. Workflow rules for AI agents

- Read this file before making any change.
- Before writing code, state which layer or layers the change belongs to.
- Add a feature in this order: domain types and ports, use case, adapter, UI, wiring in `app`, tests.
- MUST NOT add a dependency without stating why and confirming it respects the dependency rule.
- MUST NOT restructure crates or move files unless explicitly asked.
- If a request conflicts with these rules, stop and explain the conflict instead of silently breaking the architecture.
- Run the quality gates and report the results at the end of every task.

## 15. Forbidden patterns

- Business logic in egui views, `update` or click handlers.
- I/O, threads, clocks or logging in `domain`.
- `ui` importing `infrastructure`.
- Blocking the UI thread, including daemon calls or file reads inside `update`.
- `unwrap()`, `expect()` or `panic!` in production code without a documented invariant.
- Hardcoded paths, or reading `APPDATA` and similar variables outside the path module.
- Raw primitives for domain concepts.
- `Result<_, String>` errors in new code, and `anyhow` outside `app`.
- `println!` and `dbg!` in committed code.

## 16. Migration notes

Where the code on `main` deviates from the rules above. These are notes, not tasks.

- **Workspace in place, layers mostly empty.** The five crates exist. `ui` holds no code yet. `application` holds the first use cases: `queues::{SaveQueue, DeleteQueue, StartQueue, StopQueue, RunSchedules, RunQueues, FinishRun, MoveToQueue, RemoveFromQueue}` and `downloads::{AddLater, AddMany, HoldForAnswer, ConfirmAnswer, StartOrWait, CancelDownload, UseExisting, RemoveDownload, RetryDownload, Redownload, RefreshLink, SetProperties, SetSpeedLimit, SetLimitRemember, SetOnDone, AfterDownload, ConvertDownload, StopRunning, PauseAll, ResumeAll}`, `quota::{CheckQuota, QuotaStatus}`, `power::{RunWhenDone, CountdownNow, CancelCountdown}` and `update::{CheckForUpdate, ClaimWeeklyCheck, InstallUpdate}`, tested with the in-memory adapters in `application/src/memory.rs`. `domain` holds the pure rules moved so far: `queue` (queues, schedules, calendar arithmetic), `batch` (URL patterns), `site` (site and login patterns), `filename` (file-name cleaning and numbered names), `command` (the scanner command line), `settings` (the settings model, defaults, validation and folder rules), `media` (stream formats, qualities, tracks and the user's choice among them) `download` (`Download`, `DownloadEntry`, `Status`, `Answer` and the other download model types) `quota` (the byte count per minute and when the quota pauses, warns or resumes) and `update` (releases, version comparison and the weekly check). The domain still uses raw primitives and `Result<_, String>`. `infrastructure` is the rest of the old `uidm` library and re-exports some moved items (`net::site_matches`, `scan::parse_command`, `notify::Sound`, `media::convert::Target`, `media::hds::sniff`, and all of `uidm_domain::settings`, `uidm_domain::media` and `uidm_domain::download` from `settings`, `media` and `protocol`) so its callers did not change. `uidm_infrastructure::settings` keeps `load`, `save`, the file-format migrations, `settings_path`, `default_downloads_dir`, `ffmpeg` (searches `PATH`) and `validate`, which adds the checks that look at the disk. Domain folder rules (`category_folder`, `folders`, `folders_for`) take the system's Downloads folder as a parameter. `uidm_infrastructure::protocol` keeps the wire messages (`Incoming`, `Outgoing`, `CommandLine`) and the framing. `uidm_infrastructure::clock` reads the local time that `queue` takes as a parameter. `app` holds the three binaries, and the GUI's views still live there in `src/bin/gui/`. They import the settings and download models from `uidm_domain`, but still call `uidm_infrastructure` for I/O and the wire messages. Paths below are relative to `crates/infrastructure/` or `crates/app/`.
- **No layer boundaries.** `infrastructure/src/lib.rs` exposes about 30 flat modules. Domain rules and I/O sit in the same modules. Examples:
  - `engine/mod.rs` holds transfer code; its file-name rules are in `domain::filename` now.
- **Few ports.** `domain::ports` has what the queue use cases need: `QueueStore`, `DownloadList`, `DownloadControl`, `DownloadFiles`, `PausedDownloads`, `QueueRuns`, `QueueSounds`, `QueueFinish`, `SyncChecks`, `QuotaState`, `PowerControl`, `ReleaseSource`, `UpdateChecks`, `Converter`, `DownloadIntake`, `Attention`, `SettingsStore`, `EarlyStarts`, `Answers` and `Clock`. `uidm_infrastructure::clock::LocalClock` implements `Clock`, and `uidm_infrastructure::files::DiskFiles` implements `DownloadFiles` (moving, recycling and resume records). The daemon's `Hub` and `Daemon` implement the rest in `uidm-daemon.rs`, because the queues, the download list, the scheduler's runs and the engine live in their memory; that state should move behind adapters in `infrastructure`. The scheduler pass (`Daemon::tick`) is now `RunSchedules` followed by `RunQueues`. Everything else in the daemon still calls concrete functions. The use cases return `domain::queue::QueueError` (the first `thiserror` enum) and take raw `u64` ids.
- **Logic in the binaries.** `src/bin/uidm-daemon.rs` (about 6,000 lines) and `src/bin/uidm-gui.rs` (about 5,400 lines) hold most of the orchestration. The GUI also reads and writes its own files (`gui/columns.rs` writes `window.json`, `gui/droptarget.rs` writes `droptarget.txt`).
- **GUI threads.** The GUI already sends daemon work to `thread::spawn` workers. It does not yet route state changes through a single message type and `apply` function.
- **Errors.** `domain::queue::QueueError`, `domain::download::DownloadError`, `domain::update::UpdateError`, the port errors (`ControlError`, `FilesError`) and the use case errors in `application::downloads` (`StartOrWaitError`, `ConfirmError`) use `thiserror`, and no crate uses `anyhow`. `EngineError` and `MoveError` are hand-written enums. About 130 functions return `Result<_, String>`. Most modules return `io::Result`.
- **unwrap and panics.** About 1,250 `unwrap()` and 28 `expect()` calls in `src`. Most are in tests, but roughly 350 sit before the test module of their file, mostly in `uidm-daemon.rs` (about 190) and `uidm-gui.rs` (about 90).
- **Logging.** There is no `tracing`. The daemon writes its log with about 106 `eprintln!` calls, and `uidm-host` has 17 more, some of which are its CLI test mode's output. On Windows the daemon log is in the temp folder (`ipc::daemon_log`), not the data directory.
- **Paths.** `directories` is not used. `platform::data_dir`, `settings::settings_path`, `history::history_path` and the GUI read `APPDATA`, `LOCALAPPDATA` and `XDG_CONFIG_HOME` by hand. `browsers.rs` reads them to find browser profiles, which is a legitimate adapter concern but should go through the path module.
- **Raw primitives.** Download ids, byte counts, URLs and file names travel as `u64` and `String`.
- **Storage.** State is JSON files, not SQLite. Section 6 is written for files. Nothing needs to change unless a database is added.
- **Module cycles.** One cycle is left inside `infrastructure`: `net` and `pac` (proxy rules and PAC scripts) call each other. Both are adapters that stay in this crate, so it does not block a split.
