//! Guarded, cancellable worker jobs, driven by a GTK-free protocol.
mod engine;
mod operations;
mod transfer;
mod types;
mod walk;
use crate::guard::{GuardError, Policy};
pub use engine::{JobHandle, spawn, spawn_with_options, spawn_with_policy};
use gio::prelude::*;
pub use types::*;
/// Typed worker failure.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    /// Backend error.
    #[error("{0}")]
    Gio(#[from] glib::Error),
    /// Operation outside the fixture/test roots.
    #[error(transparent)]
    Guard(#[from] GuardError),
    /// Invalid basename.
    #[error(transparent)]
    Name(#[from] tallboy_core::naming::InvalidName),
    /// Unsafe transfer topology.
    #[error("A directory cannot be copied or moved into itself.")]
    IntoSelf,
    /// Overwrite was answered for a folder that a file would replace.
    #[error("A folder cannot be replaced by a file. Rename or skip the item.")]
    ReplaceFolder,
    /// URI lacks a basename or native link target.
    #[error("This operation requires a named native file.")]
    Invalid,
    /// Job was cancelled.
    #[error("Operation cancelled")]
    Cancelled,
}
pub(crate) fn info(file: &gio::File, c: &gio::Cancellable) -> Result<gio::FileInfo, JobError> {
    Ok(file.query_info(
        "standard::name,standard::edit-name,standard::type,standard::is-symlink,standard::size,standard::allocated-size,unix::is-mountpoint,unix::mode,id::filesystem,trash::orig-path",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        Some(c),
    )?)
}
pub(crate) fn children(file: &gio::File, c: &gio::Cancellable) -> Result<Vec<gio::File>, JobError> {
    let e = file.enumerate_children(
        "standard::name",
        gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
        Some(c),
    )?;
    let mut out = vec![];
    while let Some(i) = e.next_file(Some(c))? {
        out.push(file.child(i.name()));
    }
    let (_, error) = e.close(Some(c));
    if let Some(error) = error {
        return Err(error.into());
    }
    Ok(out)
}
/// Rename one entry with GIO display-name semantics. Intended for a worker;
/// UI should dispatch blocking operations off its main loop.
pub fn rename(file: &gio::File, name: &str, policy: &Policy) -> Result<gio::File, JobError> {
    tallboy_core::naming::validate(name)?;
    crate::guard::check(file, policy)?;
    let parent = file.parent().ok_or(JobError::Invalid)?;
    crate::guard::check(&parent.child(name), policy)?;
    Ok(file.set_display_name(name, None::<&gio::Cancellable>)?)
}
/// Create a unique folder, retrying collisions without overwriting any entry.
pub fn create_folder(
    parent: &gio::File,
    name: &str,
    policy: &Policy,
) -> Result<gio::File, JobError> {
    create(parent, name, policy, true)
}
/// Create a unique empty file using exclusive GIO creation.
pub fn create_file(parent: &gio::File, name: &str, policy: &Policy) -> Result<gio::File, JobError> {
    create(parent, name, policy, false)
}
fn create(
    parent: &gio::File,
    name: &str,
    policy: &Policy,
    directory: bool,
) -> Result<gio::File, JobError> {
    tallboy_core::naming::validate(name)?;
    loop {
        let unique = tallboy_core::naming::unique_name(name, |n| {
            parent.child(n).query_exists(None::<&gio::Cancellable>)
        });
        let file = parent.child(unique);
        crate::guard::check(&file, policy)?;
        let result = if directory {
            file.make_directory(None::<&gio::Cancellable>)
        } else {
            file.create(gio::FileCreateFlags::NONE, None::<&gio::Cancellable>)
                .and_then(|stream| stream.close(None::<&gio::Cancellable>))
        };
        match result {
            Ok(_) => return Ok(file),
            Err(e) if e.matches(gio::IOErrorEnum::Exists) => continue,
            Err(e) => return Err(e.into()),
        }
    }
}
