//! The other questions a job can ask (PROMPT §8.10): an error on one item,
//! trash that is not available, and a mount point inside a recursive delete.

use super::base;
use trek_fs::jobs::Answer;

/// An item failed: Skip, Skip All, Retry or Cancel.
pub fn error(
    parent: Option<&gtk::Window>,
    name: &str,
    message: &str,
    retryable: bool,
    answer: impl FnOnce(Answer) + 'static,
) {
    let detail = format!("{name}\n\n{message}");
    let buttons: &[&str] = if retryable {
        &["Cancel", "Skip", "Skip All", "Retry"]
    } else {
        &["Cancel", "Skip", "Skip All"]
    };
    base::choose(
        parent,
        "The file operation could not be completed",
        &detail,
        buttons,
        0,
        1,
        move |choice| {
            answer(match choice {
                1 => Answer::Skip {
                    apply_to_all: false,
                },
                2 => Answer::Skip { apply_to_all: true },
                3 => Answer::Retry,
                _ => Answer::Cancel,
            })
        },
    );
}

/// Trash is not available for this item: delete it instead?
pub fn trash_unsupported(
    parent: Option<&gtk::Window>,
    name: &str,
    answer: impl FnOnce(Answer) + 'static,
) {
    base::choose(
        parent,
        "Cannot move to trash can",
        &format!(
            "\"{name}\" cannot be moved to the trash can because the file system it is on does not support that, or it is on removable media.\n\nDo you want to delete it permanently instead? This applies to the other files of this operation too."
        ),
        &["Cancel", "Skip", "Delete"],
        0,
        0,
        move |choice| {
            answer(match choice {
                1 => Answer::Skip { apply_to_all: true },
                2 => Answer::DeleteInstead { apply_to_all: true },
                _ => Answer::Cancel,
            })
        },
    );
}

/// A recursive delete reached another filesystem mounted inside the folder.
pub fn mount_point(
    parent: Option<&gtk::Window>,
    name: &str,
    answer: impl FnOnce(Answer) + 'static,
) {
    base::choose(
        parent,
        "This folder is another volume",
        &format!(
            "\"{name}\" is a mount point: another disk or location is attached there.\n\nDeleting it would delete the files on that volume as well. Skip it unless that is what you want."
        ),
        &["Cancel", "Skip", "Delete Its Contents"],
        0,
        1,
        move |choice| {
            answer(match choice {
                1 => Answer::Skip {
                    apply_to_all: false,
                },
                2 => Answer::EnterMount,
                _ => Answer::Cancel,
            })
        },
    );
}
