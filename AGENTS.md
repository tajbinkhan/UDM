# AGENTS.md

This file is the single source of truth for how any AI agent or developer writes code in Pullbay. Read it before making any change. Rules use MUST, MUST NOT and SHOULD in their usual sense.

The architecture below is the target. The code today is one crate that does not follow it yet. Section 16 lists where it deviates. New code MUST follow these rules. Existing code MUST NOT be restructured to match them unless a task asks for it.

## 1. Project overview

- Pullbay is a Rust desktop download manager that follows Internet Download Manager's behavior (see `GLOSSARY.md` for the names of its windows).
- It runs as three processes, and a command-line tool:
  - `pullbay-host`, the native-messaging host the browser launches. It relays browser messages to the daemon.
  - `pullbay-daemon`, the long-lived process that owns the download engine, so downloads outlive the browser.
  - `pullbay-gui`, the desktop window. It is a client of the daemon, like the host.
  - `pullbay-cli`, the command-line tool, another client of the daemon for terminals and scripts.
- The browser extension lives in `extension/` (JavaScript). It is outside the Rust architecture but speaks the protocol defined in Rust.
- Architecture: Hexagonal (Ports and Adapters) as a Cargo workspace, with a UI layer at the edge.
- UI framework: **egui**, through `eframe` 0.31 (glow/OpenGL backend), with `egui_extras` for the download table, `tray-icon` for the tray, and `rfd` (behind the `FilePicker` port) for native file and folder pickers.

## 2. Workspace layout

```
crates/
  domain/          entities, value objects, business rules, port traits
  application/     use cases that coordinate the domain through ports
  infrastructure/  adapters: file storage, HTTP/FTP engine, OS integration, IPC
  ui/              egui views, UI state, messages, event handling
  app/             binaries: pullbay-host, pullbay-daemon, pullbay-gui, pullbay-cli (main.rs files, config, wiring)
```

How Pullbay's current concerns map onto these crates:

| Concern | Crate |
| --- | --- |
| Download states, queues and schedules, categories, file-name rules, batch URL patterns | `domain` |
| Add a download, pause, resume, run a queue, grab a site, export the list | `application` |
| HTTP/FTP transfers, HLS/DASH/HDS, settings and history files, DPAPI, registry, tray, toasts, power, recycle bin, browser registration, daemon IPC | `infrastructure` |
| Main window, save-as dialog, download windows, Options, translations | `ui` |
| Process startup, logging setup, wiring adapters to use cases | `app` |

The `app` crate MAY hold more than one binary (`[[bin]]` entries), since Pullbay ships four. Each binary is its own composition root.

## 3. Dependency rule

This is the most important rule. Dependencies point inward only.

- `domain` depends on nothing internal. Allowed external crates are pure ones such as `thiserror`, `uuid`, `chrono`, `url` (parsing only) and `serde` (derive only). It MUST NOT depend on `tokio`, `reqwest`, `rusqlite`, `sqlx`, `eframe`, `egui`, `windows`, `libc` or any other I/O, async runtime or UI crate.
- `application` depends only on `domain`, plus `tracing` for logging. It MAY start threads with `std::thread` (see section 5).
- `infrastructure` depends on `domain`, and on `application` only to implement a port that `application` defines.
- `ui` depends on `application` and `domain`. It MUST NOT depend on `infrastructure`.
- `app` depends on everything and is the only crate that wires concrete adapters into use cases.

| Crate | MAY depend on | MUST NOT depend on |
| --- | --- | --- |
| `domain` | pure crates (`thiserror`, `uuid`, `chrono`, `url`, `serde` derive) | `application`, `infrastructure`, `ui`, `app`, any I/O, async or UI crate |
| `application` | `domain`, pure crates, `tracing` | `infrastructure`, `ui`, `app`, I/O crates, UI crates |
| `infrastructure` | `domain`, `application` (ports only), I/O crates (`reqwest`, `tokio`, `windows`, `rfd`, ...) | `ui`, `app`, UI crates |
| `ui` | `domain`, `application`, `eframe`, `egui`, `egui_extras`, pure crates (`serde`, `serde_json`, `url`, `percent-encoding`, `thiserror`), drawing crates (`tray-icon`, `raw-window-handle`, platform crates under `cfg`) | `infrastructure`, `app`, I/O crates |
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
- MAY log with `tracing` and MAY run work on `std::thread` threads, such as a service that answers a request in the background. It MUST NOT do I/O itself: the work on those threads goes through ports.

## 6. Infrastructure layer rules

- Each adapter implements a port trait from `domain` (or from `application`).
- File formats, file paths, HTTP details, Win32 calls and the IPC wire format MUST NOT leak out of this crate.
- Pullbay stores its state in JSON files (`settings.json`, `history.json`, saved requests). Writes MUST be atomic (write to a temporary file, then rename). If a database is added later, its migrations live here and run at startup. Format changes to existing JSON files MUST read the old format.
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
- User-facing text goes through the translation tables in `ui` (`crates/ui/src/lang/*.json`). MUST NOT hardcode English strings in views.
- Before copying an IDM window, dialog or tray behavior, observe IDM first (the `observe-idm` skill in `.claude/skills/`). Learn IDM only by running it as a user does. MUST NOT decompile, disassemble or unpack IDM's files, extract their icons, dialogs or strings, or copy IDM's text or images into Pullbay.

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
- MUST NOT commit `println!`, `eprintln!` or `dbg!`. The exception is CLI output that is the program's actual output (the `pullbay-host` test mode, `pullbay-cli`).
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
- Decompiling or extracting resources from IDM, or copying its text, icons or images.
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

- **Workspace in place, layers mostly empty.** The five crates exist. `ui` holds the window (see The daemon connection). `application` holds the first use cases: `queues::{SaveQueue, DeleteQueue, StartQueue, StopQueue, RunSchedules, RunQueues, FinishRun, MoveToQueue, RemoveFromQueue}` and `downloads::{AddLater, AddMany, HoldForAnswer, ConfirmAnswer, StartOrWait, CancelDownload, UseExisting, RemoveDownload, RetryDownload, Redownload, RefreshLink, SetProperties, SetSpeedLimit, SetLimitRemember, LiftLimitWhileAway, SetOnDone, AfterDownload, ConvertDownload, StopRunning, PauseAll, ResumeAll}`, `quota::{CheckQuota, QuotaStatus}`, `power::{RunWhenDone, CountdownNow, CancelCountdown}` `update::{CheckForUpdate, ClaimWeeklyCheck, InstallUpdate}`, `grab::RunGrabSchedules`, `themes::ReadThemes` (the theme files and the system's contrast colors, for the window), `report::{FindNewReports, SaveReport, DiscardReport}` (the window's problem reports) and `remote::{GetFile, ListDownloads, PauseDownload, ResumeDownload, ForgetDownload, ControlQueue}` (`pullbay-cli`'s commands, on a `remote::Session` over the `DaemonClient` port, with `remote::Watch` for a followed download's progress), tested with the in-memory adapters in `application/src/memory.rs`. `domain` holds the pure rules moved so far: `queue` (queues, schedules, calendar arithmetic), `batch` (URL patterns), `site` (site and login patterns), `filename` (file-name cleaning, numbered names and `FileName`), `address` (download addresses), `command` (the scanner command line), `settings` (the settings model, defaults, validation and folder rules), `media` (stream formats, qualities, tracks and the user's choice among them) `download` (`Download`, `DownloadEntry`, `Status`, `Answer` and the other download model types) `quota` (the byte count per minute and when the quota pauses, warns or resumes) `update` (releases, version comparison and the weekly check) and `grab` (the site grabber's request, filters, mirror paths, projects, templates and filters, without their files) and `theme` (the window's looks: colors, sizes, the built-in Light, Dark and High contrast themes, the settings a theme file can give through `ThemeDraft`, the system's contrast colors, and which look the Theme setting gives, `appearance`) and `report` (problem reports: their text and file names, the system and install they name, scrubbing secrets, the prefilled issue address, the `Watch` that tells a stall, and `expecting_panic` for panics the code catches). `settings::Theme` is `System`, `Light`, `Dark`, `HighContrast` or `Custom(ThemeName)`; `settings.json` keeps the old words and `{"custom": name}`, and reads what it does not know as `System`. The domain types its ids (`id`), byte counts (`bytes`), download addresses (`address`) and file names (`filename::FileName`); see Raw primitives for what stays raw. `infrastructure` is the rest of the old `pullbay` library. Its callers name moved items by their `pullbay_domain` paths; it re-exports none of them. `pullbay_infrastructure::settings` keeps `load`, `save`, the file-format migrations, `settings_path`, `default_downloads_dir`, `ffmpeg` (searches `PATH`) and `validate`, which adds the checks that look at the disk. Domain folder rules (`category_folder`, `folders`, `folders_for`) take the system's Downloads folder as a parameter. `pullbay_infrastructure::protocol` keeps the wire messages (`Incoming`, `Outgoing`) and the framing; `CommandLine` and `ZipEntry` are in `pullbay_domain::download`. `pullbay_infrastructure::clock` reads the local time that `queue` takes as a parameter. `app` holds the four binaries; `pullbay-cli`'s `main` only builds a `daemon_client::IpcDaemonClient`, opens a `remote::Session` on it and runs the use case `pullbay_infrastructure::tool::parse` names, and `pullbay-gui`'s `main` only builds the window's adapters and calls `pullbay_ui::run`. Paths below are relative to `crates/infrastructure/`, `crates/app/` or `crates/ui/`.
- **No layer boundaries.** `infrastructure/src/lib.rs` exposes about 30 flat modules. Domain rules and I/O sit in the same modules. Examples:
  - `engine/mod.rs` holds transfer code; its file-name rules are in `domain::filename` now.
- **Few ports.** `domain::ports` has what the queue use cases need: `QueueStore`, `DownloadList`, `DownloadControl`, `DownloadFiles`, `PausedDownloads`, `QueueRuns`, `QueueSounds`, `QueueFinish`, `SyncChecks`, `QuotaState`, `PowerControl`, `ReleaseSource`, `UpdateChecks`, `Converter`, `DownloadIntake`, `Attention`, `SettingsStore`, `EarlyStarts`, `Answers`, `GrabProjects`, `GrabRuns`, `Probes`, `Browser`, `SiteCrawler`, `SaveFolders`, `VirusScanner`, `FileOpener`, `Timestamps`, `TransferEngine`, `Notifications`, `ProxyFallback`, `GrabDownloads`, `UserIdle`, `LimitLift`, `ProblemReports` and `Clock`; `pullbay_application::daemon` adds `ClientList`, `DownloadFeed` and `Countdowns`, which carry `Event`s. `pullbay_infrastructure::grab::ProjectFiles` implements `GrabProjects`; project files keep a request's login cookies sealed with DPAPI, which `save_project` and `load_project` do (the domain model holds them plain). `pullbay_infrastructure::clock::LocalClock` implements `Clock`, and `pullbay_infrastructure::files::DiskFiles` implements `DownloadFiles` (moving, recycling and resume records). `pullbay_infrastructure::store::DownloadStore` holds the download list, the kept requests and the queues, saves them (`history.json`, the saved requests, the quota's usage), and implements `QueueStore`, `DownloadList` and, with `store::Intake`, `DownloadIntake`; it hands each change to a `StoreEvents` listener, which `server::Clients` implements by sending the change to every connected client. Other adapters in `infrastructure` hold the daemon's state: `live_settings::LiveSettings` (the current settings and their file, `UpdateChecks`) and `QueueChimes` (`QueueSounds`), `queue_runs::RunBook` (`QueueRuns`) and `PauseBook` (`PausedDownloads`), `quota::QuotaHold` (`QuotaState`), `power_control::PowerOff` (`PowerControl`, with the countdown), `update::GitHub` (`ReleaseSource`), and `connect::config_for` with `ProxyRetries` (the proxy and logins a download uses). `transfers::Transfers` owns the engine and implements `DownloadControl` and `SettingsStore`, with the early downloads started while a save-as dialog is open; `transfers::SyncChecker` (`SyncChecks`) and `EarlyHandOver` (`EarlyStarts`) sit beside it. `front::Front` (`Attention`), `AnswerWait` (`Answers`) and `Notifier` reach the windows and notifications through the `front::Windows` trait, which `server::Clients` implements. `conversions::Conversions` (`Converter`), `grab::ScheduledGrabs` (`GrabRuns`), `power_control::RunEnds` (`QueueFinish`) and `clock::StoppedClock` complete the set, so `pullbay-daemon.rs` holds no port implementations. Where an adapter has to go on through a use case (starting a download after its early copy is handed over, answering a dialog nobody answered, retrying from a notification, removing a converted original, a queue's when-done action, a scheduled grab), the daemon passes it a closure at wiring. `probes::ServerProbes` (`Probes`) asks servers for the link picker, the save-as dialog, the media probe and the zip preview; `browser_bridge::BrowserBridge` (`Browser`) asks the browser extension for cookies and rendered pages through the client list; `crawler::Crawler` (`SiteCrawler`) runs a crawl. `files::DiskFiles` also implements `SaveFolders`, `scan::Antivirus` implements `VirusScanner`, `idle::SystemIdle` implements `UserIdle` (`transfers::Transfers` implements `LimitLift`), `platform::Desktop` implements `FileOpener`, `crash::ReportFiles` implements `ProblemReports`, `clock::LocalClock` implements `Timestamps`, and `server::Clients` implements `pullbay_application::daemon::ClientList`. The service runs those requests on threads of its own and keeps each client's stop flags. The scheduler pass (`DaemonService::tick`) is `CheckQuota` and `LiftLimitWhileAway`, then `RunSchedules` followed by `RunQueues`. The use cases return `domain::queue::QueueError` (the first `thiserror` enum) and take `DownloadId` and `QueueId`.
- **The daemon connection.** `pullbay_application::daemon` defines what clients send the daemon (`Command`) and what they hear back (`Event`), with a port on each side: `DaemonClient`, a client's connection (the window's, and `pullbay-cli`'s), and `Requests`, which the daemon's server hands each client's commands to, with a `Reply` for its answers. `pullbay_infrastructure::wire` maps both to and from the wire messages (`protocol::Incoming`, `Outgoing`); `daemon_client::IpcDaemonClient` implements `DaemonClient` over `ipc` and `protocol` (connection, handshake, `hello`); `new` hears what a window needs, `program` every event (`pullbay-cli` waits for `ack` and `pong`). `pullbay_infrastructure::tool` holds `pullbay-cli`'s arguments, output and exit codes: it maps the command line to the `remote` use cases' requests and their results to text, as `wire` maps messages, and implements `remote::Watch` (`Terminal`). The documented part of the messages is the automation API (`docs/command-line.md`, `docs/automation-api.schema.json`, version `daemon::AUTOMATION_API`, sent in the daemon's `hello` as `api`); `infrastructure/tests/automation_api.rs` checks the schema against `protocol`'s messages. The download list's rules are in `pullbay_domain::table`, which the window feeds with `Event`s and the daemon (through `pullbay_infrastructure::state`) with wire messages. Its views reach the clock, the Downloads folder, the write check, opening and revealing files and the window's own files (`window.json`, `droptarget.txt`, through `pullbay_application::window::WindowFiles`, written on a thread by `pullbay_infrastructure::window_files::WindowDir`) through `ports.rs`' `Ports`, which `main` hands `pullbay_ui::run` in a `Start`. The grabber's projects, templates and filters (`GrabLibrary`), list import and export (`ListFiles`), the settings checks that read the disk (`SettingsChecks`) and the operating system's proxy, wake timers, sounds, credential store, fonts, capabilities and high-contrast colors (`SystemServices`; Windows' come from `SystemParametersInfoW` and `GetSysColor`, other systems have none), the theme files (`ThemeFiles`, `pullbay_infrastructure::themes::ThemeDir`, which reads the `key = value` `.theme` files in `paths::themes_dir`) and the file and folder pickers (`FilePicker`, `pullbay_infrastructure::pickers::NativePicker`) are ports in `pullbay_application::window` too. Browser registration is `BrowserRegistry` (`pullbay_infrastructure::browsers::Registry`), and the problem reports `ProblemReports` (`Ports::reports`); `pullbay-ui` does not depend on `pullbay-infrastructure`; `main` also builds the daemon client and hands it to the window. Workers read the grabber's projects and templates (every few seconds, every second while the grabber is open) and Options' browsers (when it opens) into `AppState`, and a thread runs `ReadThemes` every two seconds and sends its result when it changes (`UiMessage::ThemesRead`); each frame works out the look from it and the settings (`theme::appearance`) and restyles egui only when that changes; the category tree, the grabber's menus and Options draw from there. The grabber saves and opens projects, templates and filters, keeps a new password, Options identifies an added browser and opens the themes folder, and Save checks the settings on disk and registers browsers, each on a worker whose result a dialog takes with `pending.rs`' `Pending` (a one-shot channel read with `try_recv`). The Queues dialog's wake-timer check, Options' Get System proxy read and a batch's questions (the taken-name check among them) run on workers too, so the window reads the disk on its own thread only once at start, for its layout and the drop target's place.
- **Logic in the binaries.** None is left. The window's orchestration is in `pullbay-ui`'s `lib.rs` (about 5,300 lines), which a later split can break up. The daemon is done: `pullbay_application::daemon::service::DaemonService` does its work (each client's `Command`, the scheduler pass, the notification thread's `Note`s, start-up and idle checks) through ports, generic over a `Wiring` trait whose associated types name one adapter per port group. `src/bin/pullbay-daemon.rs` implements `Wiring`, builds the adapters (`new_daemon`), starts the background threads (`pullbay_infrastructure::background`) and serves clients (`pullbay_infrastructure::server`, which reads each frame as a `Command` for the service's `Requests` port, `Served`). Its behavior tests run the service on the real adapters.
- **GUI threads.** The GUI sends daemon work to `thread::spawn` workers. `state.rs` holds `AppState` (the list, the daemon's news and the threads' results), `UiMessage` and `apply`: the connection thread, the clipboard watcher, the tray menu and the failed-download timer send `UiMessage`s on a channel that `update` drains with `try_recv`, and nothing outside the frame changes the state. Those threads keep a `Watch` of the daemon's events (the settings, which downloads asked for a dialog or failed) to wake a hidden window, which on macOS and Linux gets no frames until it is shown. The frame changes `AppState` only through `apply` too: what it is done with is a `UiMessage::Forget`, and what it takes to show (the next completion dialog's news, the crawl's progress, picked links) a `UiMessage::Take`, which `apply` answers with what it took. The dialogs' drafts are view state: they live in `App` and their dialogs edit them in place, and a dialog's result is an `Action` that `App::perform` carries out (#199).
- **Errors.** The domain returns no `Result<_, String>`: its rules fail with `thiserror` enums (`QueueError`, `InvalidQueue`, `InvalidSettings`, `InvalidGrab`, `BatchError`, `CommandError`, `AddressError`, `FileNameError`, `DownloadError`, `UpdateError`, `Mismatch`, `InvalidTheme`), as do the port errors (`ControlError`, `FilesError`, `SettingsError`) and the use case errors in `application::downloads` (`StartOrWaitError`, `ConfirmError`). The daemon service's commands fail with `pullbay_application::daemon::service::ServiceError`, which wraps the use cases' errors. The window translates the domain's enums in `pullbay-ui`'s `errors.rs`. The daemon tells clients of a failure with `pullbay_application::daemon::Failure` (`Event::Error`, `Rejected`, `Update`, `UpdateFailed`, `ZipPreview`, `LinkInfo`, `RefreshMismatch`, and `DaemonError::Refused`): a `ServiceError` or `SettingsError`, a new link's `pullbay_domain::download::Mismatch` (which the engine's `partial_matches` and the `check_partial` port return), an unreadable message, a probe's own words, or `Other` for text alone. `pullbay_infrastructure::wire` sends each as its English text, as before, for the clients that do not translate (the browser extension, `pullbay-host send`), plus an `error_kind` with the error's kind and values, which mirrors the application's enums around the domain's (these derive `serde`, so their variant names are wire format, pinned by `wire`'s golden tests); a message without a kind it can read (an older daemon's, or a newer kind) becomes `Other` with the text. The window translates every one (`errors::daemon`). A download's own failure (`Event::Failed`, kept in the list) and why a crawl stopped (`GrabDone`) are still English text from the engine and the crawler. `anyhow` carries the binaries' startup errors in `app` and nowhere else. `infrastructure` has an enum per module (`HlsError`, `DashError`, `HdsError`, `FfmpegError`, `CliError`, `ImportError`, `ZipError`, `ReleaseError`, `ScanError`, `NotifyError`, `EditError`, `TransferError`, `SaveError`, `settings::CheckError`, `EngineError`, `MoveError`), which keep an `io::Error`, `serde_json::Error` or `windows::core::Error` as their source where there is one. Port errors that are still a message (`ControlError`, `ProbeError`, `FilesError`, `ListError`, `SettingsError::Refused`) carry those enums' text. No function returns `Result<_, String>`. Why a theme file gives no theme is a `pullbay_application::window::ThemeFileError` (with the line and the domain's `InvalidTheme`), which `errors::theme_file` translates for Options. `ui` fails with `UiError`, a message for the user that the domain's enums turn into through `From` (translating them), and the tray with `TrayError`. Most modules return `io::Result`.
- **unwrap and panics.** None are left outside tests but documented invariants: the workspace's clippy lints (`[workspace.lints.clippy]` in the root `Cargo.toml`, which each crate takes with `[lints] workspace = true`) warn on `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` and `unreachable`, so the quality gate's `-D warnings` refuses them, and `clippy.toml` lets tests (`#[cfg(test)]` modules, test-only modules such as `memory.rs` and `testserver.rs`, and `#[test]` functions) unwrap, expect and panic, and each crate and binary root lets tests use `todo!`, `unimplemented!` and `unreachable!` (`#![cfg_attr(test, allow(...))]`), which clippy.toml cannot exempt. A documented invariant is an `expect("why")` with a comment stating it and a local `#[allow(clippy::expect_used)]`; eight are left, at seven sites (the installed ports in `pullbay-ui`'s `ports.rs`, HMAC keys in `dpapi.rs` (two) and `ipc.rs`, piped child output in `engine/page.rs` (stdout and stderr in one statement) and `media/convert.rs` (two)). Poisoned locks have one policy, `pullbay_domain::sync`: `LockExt::lock_or_recover`, `RwLockExt::read_or_recover` and `write_or_recover`, and `recover` for a `Condvar` wait, take the guard back, because every lock guards plain data, a panic in the middle of a change leaves at worst stale bookkeeping, and the panic is already in the log. It is in `domain`, the one crate every layer depends on, as pure `std` code without I/O; the modules' own lock helpers call it. CI lints the Windows and macOS code; with those targets added through `rustup`, `cargo clippy --target x86_64-pc-windows-gnu` or `--target x86_64-apple-darwin` lints it from Linux too.
- **Logging.** The four binaries log with `tracing`. Each `main` calls `pullbay_infrastructure::logging::init`, which writes `pullbay-daemon.log`, `pullbay-host.log`, `pullbay-gui.log` or `pullbay-cli.log` in the data folder (`paths::daemon_log`, `host_log`, `gui_log`, `cli_log`), appending each line in one write so many hosts can share a file, and hides passwords in addresses. The daemon moves a log over 5 MB to `.old` once it wins the bind and then hourly; the host also writes warnings to standard error, which the browser reads. Each `main` then calls `pullbay_infrastructure::crash::install`, a panic hook after `logging::init`'s that writes a problem report (version, system, install, the panic, its backtrace, the log's last lines, scrubbed of the token, passwords, cookies and authorization headers) to `paths::reports_dir`; the window's watchdog thread (`pullbay-ui`'s `watchdog.rs`) writes one when a frame stops and pings the daemon to see it answer, and the window offers reports through `UiMessage`s (`docs/problem-reports.md`). The release profile strips debug info but keeps symbols, for the backtraces. `domain` does not log; `application` logs in the daemon service and the report use cases. The `eprintln!` and `println!` calls left are `pullbay-host`'s `download` and `send` modes and `pullbay-cli`'s report (with `tool::Terminal`'s progress on standard error), whose output is the program's actual output, and three in tests. Event fields log ids as their numbers (`download_id = id.get()`) and addresses as their text.
- **Paths.** `pullbay_infrastructure::paths` resolves every folder Pullbay uses (settings, data, grabber projects, theme files, the logs, the problem reports, the window's files, the sign-in entry, app packages' AppData copies and browser profile roots) on top of `directories`' `BaseDirs`, and is the only module that reads `APPDATA`, `LOCALAPPDATA`, `USERPROFILE` or `XDG_*` (`desktop_session` reads `XDG_CURRENT_DESKTOP` and `XDG_SESSION_TYPE` for problem reports). On Windows it reads those variables before the known folders, because end-to-end runs point them at a scratch folder, and on Linux it uses any non-empty `XDG_CONFIG_HOME` and `XDG_DATA_HOME` as given (`directories` ignores relative ones), so no folder moved. It also owns `PULLBAY_INSTANCE` (`paths::instance`) and portable mode (`paths::portable_root`): a `portable.txt` beside the programs, or above `target\release` as in the portable zip, puts the settings and data folders, the window's files and the engine's default parts folder (`paths::parts_dir`, otherwise `Pullbay` in the system's temporary folder) in the copy's `Data` folder and the default Downloads folder beside it (`docs/portable.md`); `autostart`, `notify::register_app` and `from_uidm` then leave the system alone, `browsers::register_found` runs on Windows too, and the window learns of it through `SystemServices::is_portable`.
- **Raw primitives.** Ids (`pullbay_domain::id::{DownloadId, QueueId}`, `pullbay_application::daemon::ClientId`), byte counts (`pullbay_domain::bytes::ByteCount`, saturating), download addresses (`pullbay_domain::address::{Address, Link}`) and file names (`pullbay_domain::filename::FileName`) are newtypes through the domain, the ports, the use cases, `Command` and `Event`, `protocol` and the window. Each serializes as the plain number or string, so `history.json`, the saved requests, settings, grabber projects, list files and the wire keep their shape (tests in `history` and `protocol` pin it, and `wire`'s golden `error_kind` JSON is unchanged). `Address` is what the daemon takes, `http://`, `https://`, `ftp://` or `ftps://` in any case (`batch::is_download_url`, with `batch::is_ftp_url` for the two FTP schemes), kept as given; `Link`, a download's address as a client sent it or a file holds it, is an `Address` or other text, so a refused request still decodes (and is `rejected` by name) and an old row still loads. `FileName` refuses empty names, `.`, `..`, `/`, `\` and NUL; `sanitize_filename`, `name_from_url` and `proposed_name` return one. What stays raw: the engine's insides (`u64` counters, offsets and sidecars, and `DownloadRequest`'s address and name), converted where they meet the domain (`app::to_request`, `progress_to_outgoing`, `Inspection`, `fetch`); a client's request numbers; speeds and the settings' KB and MB fields; connection counts (the settings', an entry's, and a `Download`'s own `connections`, which `Download::connections_or` brings into range); addresses that are not a download's (referrers, pages, `PageLink`, proxies, the grabber's `GrabRequest` and `GrabFile`, stream segments); names as typed or sent (`Download::filename`, `Answer::filename`, `Properties::filename`), which are cleaned into a `FileName` when used; error values that echo what was refused (`NotDownloadAddress`, `NotDownloadLink`), whose wire shape is pinned; and paths.
- **Storage.** State is JSON files, not SQLite. Section 6 is written for files. Nothing needs to change unless a database is added.
