//! Changing the download queues: create or edit one, delete one, start and
//! stop one, and move downloads into and out of them.

mod delete_queue;
mod move_to_queue;
mod remove_from_queue;
mod save_queue;
mod start_queue;
mod stop_queue;

pub use delete_queue::DeleteQueue;
pub use move_to_queue::MoveToQueue;
pub use remove_from_queue::RemoveFromQueue;
pub use save_queue::SaveQueue;
pub use start_queue::StartQueue;
pub use stop_queue::StopQueue;

#[cfg(test)]
pub(crate) mod memory;
