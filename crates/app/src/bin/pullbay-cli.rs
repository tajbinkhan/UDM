//! `pullbay-cli`, Pullbay's command-line tool: downloads a file and shows
//! its progress, lists, pauses, resumes and removes downloads, and starts
//! and stops queues, from a terminal or a script, with no window open.
//!
//! It is a client of the daemon like the window, and starts the daemon when
//! none runs. Its arguments, output and exit codes are
//! `pullbay_infrastructure::tool`; each command is a use case in
//! `pullbay_application::remote`. This binary wires them together
//! (AGENTS.md section 8). Standard output carries what scripts read (a
//! saved file's path, an id, the list); standard error what people read.
//! Its log is `pullbay-cli.log` in the data folder.

// Tests may use todo!, unimplemented! and unreachable!, which clippy.toml's
// test exemptions do not cover (AGENTS.md section 9).
#![cfg_attr(test, allow(clippy::todo, clippy::unimplemented, clippy::unreachable))]

use std::time::Duration;

use pullbay_application::remote::{
    ControlQueue, ForgetDownload, GetFile, ListDownloads, PauseDownload, ResumeDownload, Session,
};
use pullbay_infrastructure as infra;
use pullbay_infrastructure::tool::{self, exit, Invocation, Report, Which};

/// What the tool calls itself in its `hello`.
const CLIENT: &str = "pullbay-cli";

/// How long connecting, which may start the daemon, and each answer may
/// take.
const WAIT: Duration = Duration::from_secs(30);

fn main() {
    // Before the log, which would make Pullbay's data folder first.
    let moved = infra::from_uidm::move_from_uidm();
    infra::logging::init(infra::logging::Program::Cli);
    // A panic leaves a report in the data folder (`crash`).
    infra::crash::install(infra::logging::Program::Cli);
    moved.log();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let report = match tool::parse(&args, &cwd) {
        Ok(Invocation::Help) => Report { out: Some(tool::USAGE.to_string()), ..Report::ok() },
        Ok(Invocation::Version) => {
            Report { out: Some(format!("pullbay-cli {}", env!("CARGO_PKG_VERSION"))), ..Report::ok() }
        }
        Ok(invocation) => run(invocation),
        Err(e) => Report::usage(&e),
    };
    // The program's output, for the person or script that ran it.
    if let Some(out) = &report.out {
        println!("{out}");
    }
    if let Some(err) = &report.err {
        eprintln!("{err}");
    }
    std::process::exit(report.code);
}

/// Connect to the daemon and do what `invocation` asks.
fn run(invocation: Invocation) -> Report {
    let client = infra::daemon_client::IpcDaemonClient::program(CLIENT);
    let session = match Session::open(&client, WAIT) {
        Ok(session) => session,
        Err(e) => {
            tracing::warn!(error = %e, "could not reach the daemon");
            return Report::error(&e);
        }
    };
    let result = match invocation {
        Invocation::Get { request, quiet } => {
            tracing::info!(url = %request.url, "get");
            let mut terminal = tool::Terminal::new(quiet);
            let got = GetFile::new(&session, &infra::hash::DiskHashes).run(&request, &mut terminal);
            terminal.finish();
            got.map(|got| Report::got(&got))
        }
        Invocation::List { json } => ListDownloads::new(&session).run().map(|listing| {
            if json {
                match tool::json(&listing) {
                    Ok(text) => Report { out: Some(text), ..Report::ok() },
                    Err(e) => Report { code: exit::FAILED, out: None, err: Some(format!("pullbay-cli: {e}")) },
                }
            } else {
                Report { out: Some(tool::table(&listing)), ..Report::ok() }
            }
        }),
        Invocation::Pause(Which::One(id)) => PauseDownload::new(&session).run(id).map(|()| Report::ok()),
        Invocation::Pause(Which::All) => PauseDownload::new(&session).all().map(|()| Report::ok()),
        Invocation::Resume(Which::One(id)) => ResumeDownload::new(&session).run(id).map(Report::resumed),
        Invocation::Resume(Which::All) => ResumeDownload::new(&session).all().map(|()| Report::ok()),
        Invocation::Remove { id, delete_files } => {
            ForgetDownload::new(&session).run(id, delete_files).map(|()| Report::ok())
        }
        Invocation::Queue { start, name } => {
            ControlQueue::new(&session).run(name.as_deref(), start).map(|_| Report::ok())
        }
        Invocation::Help | Invocation::Version => Ok(Report::ok()),
    };
    result.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "refused");
        Report::error(&e)
    })
}
