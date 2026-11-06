//! Changing the download queues: create or edit one, delete one, and move
//! downloads into and out of them.

mod delete_queue;
mod move_to_queue;
mod remove_from_queue;
mod save_queue;

pub use delete_queue::DeleteQueue;
pub use move_to_queue::MoveToQueue;
pub use remove_from_queue::RemoveFromQueue;
pub use save_queue::SaveQueue;

#[cfg(test)]
pub(crate) mod memory;
