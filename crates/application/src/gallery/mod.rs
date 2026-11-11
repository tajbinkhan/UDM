//! Image galleries: reading a page's pictures with gallery-dl (through the
//! `Probes` port), one read of a page at a time. The pictures the user
//! ticks are added as one batch into one folder through `AddMany`, after
//! the window's batch questions, like the link picker's.

mod read_gallery;

pub use read_gallery::ReadGallery;
