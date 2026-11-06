//! What happens when a queue or a download is done: nothing, closing UIDM,
//! or a countdown before sleeping, hibernating or shutting down.

mod cancel_countdown;
mod countdown_now;
mod run_when_done;

pub use cancel_countdown::CancelCountdown;
pub use countdown_now::CountdownNow;
pub use run_when_done::RunWhenDone;
