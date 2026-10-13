//! Converts backend errors into messages a user can act on. No `Debug`
//! output and no raw GIO text reaches the user except as a last resort.

use gtk::{gio, glib};
use tallboy_fs::{apps::AppError, folder::FolderError, jobs::JobError, terminal::TerminalError};

/// Message for a folder that could not be opened or listed.
pub fn folder(error: &FolderError) -> String {
    match error {
        FolderError::Io(e) => io(e),
        FolderError::Location(e) => e.to_string(),
    }
}

/// Message for a GIO error.
pub fn io(error: &glib::Error) -> String {
    use gio::IOErrorEnum as E;
    match error.kind::<E>() {
        Some(E::PermissionDenied) => "Permission denied.".into(),
        Some(E::NotFound) => "The folder does not exist.".into(),
        Some(E::NotDirectory) => "This location is not a folder.".into(),
        Some(E::NotMounted) => "The location is not mounted.".into(),
        Some(E::NotSupported) => "This location is not supported.".into(),
        Some(E::TimedOut) => "The connection timed out.".into(),
        Some(E::HostNotFound) => "The server could not be found.".into(),
        Some(E::ConnectionRefused) => "The server refused the connection.".into(),
        Some(E::Cancelled) => "The operation was cancelled.".into(),
        _ => error.message().to_owned(),
    }
}

/// Message for an application that could not be found or started.
pub fn app(error: &AppError) -> String {
    match error {
        AppError::NoDefault(content_type) => format!(
            "No application is set to open files of type \"{}\".",
            gio::content_type_get_description(content_type)
        ),
        AppError::Gio(e) => io(e),
        AppError::Location(e) => e.to_string(),
        AppError::Invalid => "This is not a valid application launcher or command.".into(),
    }
}

/// Message for a terminal or command that could not be started.
pub fn terminal(error: &TerminalError) -> String {
    match error {
        TerminalError::Parse(e) => format!("The command could not be understood: {}", e.message()),
        TerminalError::Io(e) => match e.kind() {
            std::io::ErrorKind::NotFound => "The program was not found.".into(),
            std::io::ErrorKind::PermissionDenied => {
                "The program is not allowed to run (permission denied).".into()
            }
            _ => format!("The command could not be started: {e}."),
        },
        TerminalError::Empty => "Enter a command to run.".into(),
        TerminalError::Missing => {
            "No terminal emulator was found. Set one in Preferences, on the Advanced page.".into()
        }
        TerminalError::Remote => {
            "A terminal can only be opened in a folder on this computer.".into()
        }
        TerminalError::NoHold => {
            "This terminal cannot keep its window open after the command.".into()
        }
        TerminalError::List(line) => format!("The terminal list has a bad line: {line}"),
    }
}

/// Message for a rename or create that failed.
pub fn job(error: &JobError) -> String {
    match error {
        JobError::Gio(e) => io_file(e),
        JobError::Guard(e) => crate::ops::guard_message(e),
        JobError::Name(_) => {
            "A name cannot be empty, cannot contain \"/\", and cannot be \".\" or \"..\".".into()
        }
        JobError::IntoSelf => "A folder cannot be copied or moved into itself.".into(),
        JobError::ReplaceFolder => "A folder cannot be replaced by a file.".into(),
        JobError::Invalid => "This operation is not possible for this item.".into(),
        JobError::Cancelled => "The operation was cancelled.".into(),
    }
}

/// Message for a GIO error about a file rather than a folder being opened.
pub fn io_file(error: &glib::Error) -> String {
    use gio::IOErrorEnum as E;
    match error.kind::<E>() {
        Some(E::Exists) => "A file with that name already exists.".into(),
        Some(E::PermissionDenied) => "Permission denied.".into(),
        Some(E::NotFound) => "The file no longer exists.".into(),
        Some(E::ReadOnly) => "The location is read-only.".into(),
        Some(E::NoSpace) => "There is no space left on the device.".into(),
        Some(E::FilenameTooLong) => "The name is too long.".into(),
        Some(E::InvalidFilename) => "The name is not valid on this file system.".into(),
        _ => error.message().to_owned(),
    }
}
