//! Freedesktop image thumbnail cache and a widget-free worker pool.
mod cache;
mod pool;
pub use cache::{Cache, Size, Spec, skip};
pub use pool::{Decoded, Pool, Request};
/// Thumbnail failures; cancelled and unsupported requests are not failure-cached.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// File/decoder failure.
    #[error("Thumbnail: {0}")]
    Gio(#[from] glib::Error),
    /// Cache I/O error.
    #[error("Thumbnail cache: {0}")]
    Io(#[from] std::io::Error),
    /// Ineligible file.
    #[error("Thumbnail skipped by settings or unsupported image type")]
    Skipped,
    /// Cancelled request or stopped pool.
    #[error("Thumbnail request cancelled")]
    Cancelled,
    /// Allocation/checksum not available.
    #[error("Thumbnail allocation failed")]
    Allocation,
    /// URI rejected by location policy.
    #[error(transparent)]
    Location(#[from] tallboy_core::location::LocationError),
}
