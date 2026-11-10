//! `pullbay-mcp`, Pullbay's server for AI assistants: it speaks the Model
//! Context Protocol on standard input and output, so an assistant such as
//! Claude Desktop, Claude Code, Cursor or VS Code that starts it can list,
//! add, pause, resume and remove downloads and start and stop queues.
//!
//! It is a client of the daemon like the window, and starts the daemon when
//! none runs. The protocol and its tools are `pullbay_infrastructure::mcp`;
//! each tool is a use case in `pullbay_application::remote`, run by its
//! `Assistant`. This binary wires them together (AGENTS.md section 8).
//! Standard output carries the protocol alone; the log is `pullbay-mcp.log`
//! in the data folder.

// Tests may use todo!, unimplemented! and unreachable!, which clippy.toml's
// test exemptions do not cover (AGENTS.md section 9).
#![cfg_attr(test, allow(clippy::todo, clippy::unimplemented, clippy::unreachable))]

use std::time::Duration;

use anyhow::Context as _;

use pullbay_application::remote::Assistant;
use pullbay_infrastructure as infra;

/// What the server calls itself in its `hello` to the daemon.
const CLIENT: &str = "pullbay-mcp";

/// How long connecting, which may start the daemon, and each answer may
/// take.
const WAIT: Duration = Duration::from_secs(30);

fn main() -> anyhow::Result<()> {
    // Before the log, which would make Pullbay's data folder first.
    let moved = infra::from_uidm::move_from_uidm();
    infra::logging::init(infra::logging::Program::Mcp);
    // A panic leaves a report in the data folder (`crash`).
    infra::crash::install(infra::logging::Program::Mcp);
    moved.log();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "pullbay-mcp starting");
    let client = infra::daemon_client::IpcDaemonClient::assistant(CLIENT);
    let mut assistant = Assistant::new(&client, WAIT);
    let served =
        infra::mcp::serve(std::io::stdin().lock(), std::io::stdout().lock(), &mut assistant, env!("CARGO_PKG_VERSION"));
    match &served {
        Ok(()) => tracing::info!("the assistant closed standard input; exiting"),
        Err(e) => tracing::warn!(error = %e, "standard input or output failed"),
    }
    served.context("talking to the assistant over standard input and output")
}
