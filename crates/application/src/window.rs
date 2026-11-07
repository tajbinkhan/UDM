//! What the desktop window needs besides its daemon connection (see
//! `daemon::DaemonClient`): ports for its own files.

/// The window's own files, such as its layout: small, rewritten whole, and
/// kept per user.
pub trait WindowFiles: Send + Sync {
    /// The window's file `name`, or `None` when it was never saved or
    /// cannot be read.
    fn read(&self, name: &str) -> Option<Vec<u8>>;

    /// Save `bytes` as the window's file `name`, without waiting: the
    /// write happens on another thread and replaces the file whole, so a
    /// crash never leaves half a file. Of two saves of one name, the later
    /// wins.
    fn save(&self, name: &str, bytes: Vec<u8>);
}
