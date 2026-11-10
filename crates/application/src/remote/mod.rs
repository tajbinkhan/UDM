//! Pullbay run from another program: the use cases behind `pullbay-cli`
//! and `pullbay-mcp` (`Assistant`, through the `Tools` port).
//! Each works on a `Session`, a connection of its own to the daemon through
//! the `DaemonClient` port, as the window's is, so the daemon starts if
//! it is not running and no window needs to be open. They ask the daemon
//! the way the window does and leave the rules to it.

mod add_link;
mod assistant;
mod control_queue;
mod forget_download;
mod get_file;
mod list_downloads;
mod pause_download;
mod resume_download;
mod session;

pub use add_link::{AddLink, AddRequest, Added};
pub use assistant::{Assistant, Reply, Request, Tools, Which};
pub use control_queue::ControlQueue;
pub use forget_download::ForgetDownload;
pub use get_file::{GetFile, GetRequest, Got, Watch};
pub use list_downloads::ListDownloads;
pub use pause_download::PauseDownload;
pub use resume_download::{ResumeDownload, Resumed};
pub use session::{Listing, RemoteError, Session};
