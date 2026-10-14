//! Public job protocol, independent of GTK.
use crate::meta::FileMeta;
use trek_core::perms::AttrChange;
/// Operation to run on a dedicated worker. URI strings preserve remote and raw names.
#[derive(Debug, Clone)]
pub enum JobKind {
    /// Copy sources into a directory.
    Copy {
        /// Source URIs.
        sources: Vec<String>,
        /// Destination directory URI.
        dest_dir: String,
    },
    /// Move sources, preferring a native rename.
    Move {
        /// Source URIs.
        sources: Vec<String>,
        /// Destination directory URI.
        dest_dir: String,
    },
    /// Create symlinks to sources.
    Link {
        /// Source URIs.
        sources: Vec<String>,
        /// Destination directory URI.
        dest_dir: String,
    },
    /// Move entries to the GVFS trash.
    Trash {
        /// Item URIs.
        files: Vec<String>,
    },
    /// Remove entries recursively without following symlinks.
    Delete {
        /// Item URIs.
        files: Vec<String>,
    },
    /// Restore trash items to their original paths.
    Restore {
        /// Trash URIs.
        files: Vec<String>,
    },
    /// Delete all entries in the isolated trash.
    EmptyTrash,
    /// Apply masked chmod/chown, optionally recursively.
    SetAttrs {
        /// Item URIs.
        files: Vec<String>,
        /// Requested changes.
        change: AttrChange,
        /// Descend into directories, never symlinks.
        recursive: bool,
    },
    /// Read-only recursive size and item count.
    Count {
        /// Item URIs.
        files: Vec<String>,
    },
}
impl JobKind {
    pub(crate) fn sources(&self) -> Vec<String> {
        match self {
            Self::Copy { sources, .. }
            | Self::Move { sources, .. }
            | Self::Link { sources, .. } => sources.clone(),
            Self::Trash { files }
            | Self::Delete { files }
            | Self::Restore { files }
            | Self::SetAttrs { files, .. }
            | Self::Count { files } => files.clone(),
            Self::EmptyTrash => vec!["trash:///".into()],
        }
    }
    pub(crate) fn destination(&self) -> Option<&str> {
        match self {
            Self::Copy { dest_dir, .. }
            | Self::Move { dest_dir, .. }
            | Self::Link { dest_dir, .. } => Some(dest_dir),
            Self::Trash { .. }
            | Self::Delete { .. }
            | Self::Restore { .. }
            | Self::EmptyTrash
            | Self::SetAttrs { .. }
            | Self::Count { .. } => None,
        }
    }
}
/// Worker question. The paired reply is single-use.
#[derive(Debug, Clone)]
pub enum Question {
    /// File collision (directories merge).
    Conflict {
        /// Source metadata.
        src: Box<FileMeta>,
        /// Existing destination.
        dest: Box<FileMeta>,
        /// Unique initial rename suggestion.
        suggested_name: String,
        /// Whether `Answer::Overwrite` is accepted. It is not when the
        /// destination is a folder and the source is not: replacing it would
        /// delete everything inside. Offer Rename, Skip and Cancel only.
        can_overwrite: bool,
    },
    /// Recoverable operation error.
    Error {
        /// Affected URI.
        uri: String,
        /// User-facing text.
        message: String,
        /// Retry is meaningful.
        retryable: bool,
    },
    /// No safe trash support for this item.
    TrashUnsupported {
        /// Item URI.
        uri: String,
    },
    /// Recursive delete reached a filesystem boundary.
    MountPoint {
        /// Mounted directory URI.
        uri: String,
    },
}
/// User answer. Apply-to-all is scoped to one job and question category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Replace an existing file.
    Overwrite {
        /// Remember conflict choice.
        apply_to_all: bool,
    },
    /// Skip this item.
    Skip {
        /// Remember for this question category.
        apply_to_all: bool,
    },
    /// Use a validated different basename.
    Rename(String),
    /// Retry the failed operation.
    Retry,
    /// Permanently delete when trash is unavailable.
    DeleteInstead {
        /// Remember for unsupported trash.
        apply_to_all: bool,
    },
    /// Cross a mount boundary after explicit confirmation.
    EnterMount,
    /// Cancel the job.
    Cancel,
}
/// Single-use response channel. Dropping unanswered replies cancels the question.
#[derive(Debug)]
pub struct Reply(pub(crate) std::sync::mpsc::Sender<Answer>);
impl Reply {
    /// Answer once without blocking the UI.
    pub fn answer(self, answer: Answer) -> Result<(), std::sync::mpsc::SendError<Answer>> {
        self.0.send(answer)
    }
}
/// Job outcome; errors and warnings are retained for the progress dialog.
#[derive(Debug, Clone, Default)]
pub struct JobSummary {
    /// User cancelled.
    pub cancelled: bool,
    /// Completed entries.
    pub done_files: u64,
    /// Completed logical bytes.
    pub done_bytes: u64,
    /// Total allocated bytes for Count.
    pub allocated_bytes: u64,
    /// Skipped items.
    pub skipped: u64,
    /// Failed items.
    pub errors: Vec<String>,
    /// Nonfatal metadata preservation failures.
    pub warnings: Vec<String>,
}
/// Worker events delivered over async-channel.
#[derive(Debug)]
pub enum JobEvent {
    /// Prepare walk begins.
    Preparing,
    /// Prepare totals.
    Prepared {
        /// Entries including directories and links.
        total_files: u64,
        /// Logical bytes.
        total_bytes: u64,
    },
    /// Current item.
    Current {
        /// Source URI.
        src: String,
        /// Destination if present.
        dest: Option<String>,
    },
    /// Progress, emitted no more than 10 times per second.
    Progress {
        /// Complete entries.
        done_files: u64,
        /// Complete/in-flight bytes.
        done_bytes: u64,
        /// Average transfer rate.
        bytes_per_sec: u64,
    },
    /// User response required.
    Ask(Question, Reply),
    /// Final outcome, exactly once.
    Finished(JobSummary),
}

/// Behaviour shared by one job, copied from the current application preferences.
#[derive(Debug, Clone, Copy)]
pub struct JobOptions {
    /// Offer permanent deletion for removable-media trash requests.
    pub no_usb_trash: bool,
    /// Start paused, useful for queued jobs and deterministic tests.
    pub start_paused: bool,
}
impl Default for JobOptions {
    fn default() -> Self {
        Self {
            no_usb_trash: true,
            start_paused: false,
        }
    }
}
