//! Pullbay's desktop window. The views live in `pullbay_ui`; this binary builds
//! the adapters they reach through ports and starts the window
//! (AGENTS.md section 8).

#![windows_subsystem = "windows"]
// Tests may use todo!, unimplemented! and unreachable!, which clippy.toml's
// test exemptions do not cover (AGENTS.md section 9).
#![cfg_attr(test, allow(clippy::todo, clippy::unimplemented, clippy::unreachable))]

use std::sync::Arc;

use pullbay_infrastructure as infra;

fn main() -> eframe::Result {
    // Before the log, which would make Pullbay's data folder first.
    let moved = infra::from_uidm::move_from_uidm();
    infra::logging::init(infra::logging::Program::Gui);
    // A panic leaves a report in the data folder, which the next window
    // offers (`crash`).
    infra::crash::install(infra::logging::Program::Gui);
    moved.log();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let ports = pullbay_ui::Ports {
        clock: Arc::new(infra::clock::LocalClock),
        times: Arc::new(infra::clock::LocalClock),
        folders: Arc::new(infra::files::DiskFiles),
        files: Arc::new(infra::files::DiskFiles),
        opener: Arc::new(infra::platform::Desktop),
        hashes: Arc::new(infra::hash::DiskHashes),
        window_files: Arc::new(infra::window_files::WindowDir::new()),
        grab: infra::grab::projects_dir().ok().map(|dir| Arc::new(infra::grab::ProjectFiles(dir)) as _),
        found: Arc::new(infra::grab::FoundFiles),
        lists: Arc::new(infra::listfile::ListFormats),
        checks: Arc::new(infra::settings::DiskChecks),
        system: Arc::new(infra::system::ThisSystem),
        browsers: Arc::new(infra::browsers::Registry),
        pickers: Arc::new(infra::pickers::NativePicker),
        reports: Arc::new(infra::crash::ReportFiles::new()),
        themes: Arc::new(infra::themes::ThemeDir::new()),
    };
    pullbay_ui::run(pullbay_ui::Start {
        ports,
        daemon: Arc::new(infra::daemon_client::IpcDaemonClient::new(pullbay_ui::WINDOW_CLIENT)),
        minimized: args.iter().any(|a| a == "--minimized"),
        in_tray: args.iter().any(|a| a == infra::cli::IN_TRAY),
        command_line: infra::cli::parse(&args, &cwd).map_err(|e| pullbay_ui::UiError::new(e.to_string())).transpose(),
        instance: infra::paths::instance(),
    })
}
