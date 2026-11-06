//! Use cases (add a download, pause, run a queue, ...) that coordinate the
//! domain through its ports, one per file.

pub mod downloads;
pub mod queues;
pub mod quota;

#[cfg(test)]
mod memory;
