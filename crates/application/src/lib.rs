//! Use cases (add a download, pause, run a queue, ...) that coordinate the
//! domain through its ports, one per file.

pub mod downloads;
pub mod grab;
pub mod power;
pub mod queues;
pub mod quota;
pub mod update;

#[cfg(test)]
mod memory;
