//! Acting on many downloads at once: stop everything that runs, pause all
//! and resume all.

mod pause_all;
mod resume_all;
mod stop_running;

pub use pause_all::PauseAll;
pub use resume_all::{ResumeAll, Resumed};
pub use stop_running::{StopRunning, Stopped};
