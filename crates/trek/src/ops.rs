//! Running file operations: the guard pre-check, the job itself, and its
//! progress and question dialogs. No file-operation logic lives here; this
//! module only starts `fs::jobs` and renders what they report.

use crate::{
    app::App,
    dialogs::{self, base, progress::ProgressDialog},
    tab::location_from_uri,
};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use trek_fs::{
    guard::{GuardError, Policy},
    jobs::{self, Answer, JobEvent, JobHandle, JobKind, JobOptions, JobSummary, Question, Reply},
};

/// §8.10: the progress window appears only if the job is still running then.
const SHOW_AFTER: Duration = Duration::from_millis(500);

/// A readable name for a URI: the native path when there is one.
pub fn display(uri: &str) -> String {
    location_from_uri(uri).map_or_else(|| uri.to_owned(), |location| location.display())
}

/// What to tell the user when the development guard refuses an operation
/// (SCOPE 7.12). The engine's own text never reaches the dialog.
pub fn locked_message(path: &str) -> String {
    // The folders that exist among the allowed roots; the build and
    // temporary directories the guard also allows are of no use to a user.
    let hidden = [std::env::temp_dir()];
    let allowed: Vec<String> = Policy::from_environment()
        .roots
        .iter()
        .filter(|root| root.is_dir() && !hidden.contains(root))
        .filter(|root| !root.components().any(|part| part.as_os_str() == "target"))
        .map(|root| root.display().to_string())
        .collect();
    format!(
        "This build of trek is locked: copying, moving, deleting, renaming and creating files only work inside its test folders until the review has been signed off.\n\nNot allowed here:\n{path}\n\nAllowed:\n{}\n\nTo try one more folder, such as a USB stick, start trek with TREK_ALLOW_ROOTS set to that folder.",
        allowed.join("\n")
    )
}

/// Message for a guard refusal.
pub fn guard_message(error: &GuardError) -> String {
    match error {
        GuardError::Denied(uri) => locked_message(&display(uri)),
        GuardError::Location(e) => e.to_string(),
        GuardError::Io(e) => format!("The location could not be checked: {e}."),
    }
}

/// A job's error lines are the engine's strings; a guard refusal that only
/// shows up mid-job is reworded here.
fn tidy(error: &str) -> String {
    match error.strip_prefix("File operations are locked outside the allowed roots: ") {
        Some(uri) => format!(
            "{}: not allowed while this build is locked to its test folder.",
            display(uri)
        ),
        None => error.to_owned(),
    }
}

/// Checks the guard for everything a job names before starting it, so that
/// a refusal is one clear dialog rather than a failed job.
fn precheck(kind: &JobKind, policy: &Policy) -> Result<(), GuardError> {
    let (uris, destination): (&[String], Option<&String>) = match kind {
        JobKind::Copy { sources, dest_dir }
        | JobKind::Move { sources, dest_dir }
        | JobKind::Link { sources, dest_dir } => (sources, Some(dest_dir)),
        JobKind::Trash { files }
        | JobKind::Delete { files }
        | JobKind::Restore { files }
        | JobKind::SetAttrs { files, .. } => (files, None),
        JobKind::EmptyTrash | JobKind::Count { .. } => (&[], None),
    };
    if let Some(destination) = destination {
        policy.check(&gio::File::for_uri(destination))?;
    }
    for uri in uris {
        policy.check(&gio::File::for_uri(uri))?;
    }
    Ok(())
}

type Done = Box<dyn FnOnce(&JobSummary)>;

thread_local! {
    /// The jobs that have not finished, for the task key (`w`).
    static RUNNING: RefCell<Vec<std::rc::Weak<Running>>> = const { RefCell::new(Vec::new()) };
}

/// Brings up the progress window of every job that is still running.
/// Returns whether there was one.
pub fn present_running() -> bool {
    let running: Vec<Rc<Running>> = RUNNING.with_borrow_mut(|jobs| {
        jobs.retain(|job| job.upgrade().is_some_and(|job| !job.finished.get()));
        jobs.iter().filter_map(std::rc::Weak::upgrade).collect()
    });
    for job in &running {
        job.show();
        job.dialog.present();
    }
    !running.is_empty()
}

struct Running {
    app: App,
    parent: Option<gtk::Window>,
    /// Existing files are replaced without asking (`P`).
    overwrite: bool,
    handle: JobHandle,
    dialog: ProgressDialog,
    shown: Cell<bool>,
    finished: Cell<bool>,
    paused: Cell<bool>,
    total: Cell<(u64, u64)>,
    done: RefCell<Option<Done>>,
}

impl Running {
    fn show(&self) {
        if !self.shown.replace(true) {
            self.dialog.present();
        }
    }

    /// Questions belong to the progress window once it is up.
    fn question_parent(&self) -> Option<gtk::Window> {
        if self.shown.get() {
            Some(self.dialog.window().clone())
        } else {
            self.parent.clone()
        }
    }

    fn ask(&self, question: Question, reply: Reply) {
        // The answer goes back to the worker; a failed send means it is gone.
        let answer = move |answer: Answer| {
            let _ = reply.answer(answer);
        };
        let parent = self.question_parent();
        match question {
            // A forced paste answers for itself wherever replacing is
            // possible at all; a folder in the way of a file still asks.
            Question::Conflict {
                can_overwrite: true,
                ..
            } if self.overwrite => answer(Answer::Overwrite { apply_to_all: true }),
            Question::Conflict {
                src,
                dest,
                suggested_name,
                can_overwrite,
            } => dialogs::conflict::show(
                parent.as_ref(),
                &src,
                &dest,
                &suggested_name,
                can_overwrite,
                self.app.settings().si_unit(),
                answer,
            ),
            Question::Error {
                uri,
                message,
                retryable,
            } => dialogs::questions::error(
                parent.as_ref(),
                &display(&uri),
                &tidy(&message),
                retryable,
                answer,
            ),
            Question::TrashUnsupported { uri } => {
                dialogs::questions::trash_unsupported(parent.as_ref(), &display(&uri), answer)
            }
            Question::MountPoint { uri } => {
                dialogs::questions::mount_point(parent.as_ref(), &display(&uri), answer)
            }
        }
    }

    fn finish(&self, summary: &JobSummary) {
        self.finished.set(true);
        for error in &summary.errors {
            self.dialog.add_error(&tidy(error));
        }
        // A warning is metadata that could not be carried over (R11): the
        // files themselves arrived. It is listed when the window stays open
        // for an error, and never keeps the window open by itself.
        for warning in &summary.warnings {
            log::warn!("{warning}");
            if !summary.errors.is_empty() {
                self.dialog.add_error(&tidy(warning));
            }
        }
        let outcome = if summary.cancelled {
            "The operation was cancelled."
        } else {
            "Finished, with errors."
        };
        self.dialog.finish(outcome, !summary.errors.is_empty());
        if let Some(done) = self.done.take() {
            done(summary);
        }
    }
}

/// Starts a job with its progress window and question dialogs. `done` runs
/// on the main thread when the job has finished, whatever the outcome.
///
/// The job is refused up front, with the locked-build explanation, when the
/// guard would refuse it.
pub fn start(
    app: &App,
    parent: Option<&gtk::Window>,
    kind: JobKind,
    title: &str,
    overwrite: bool,
    done: impl FnOnce(&JobSummary) + 'static,
) {
    let policy = Policy::from_environment();
    if let Err(e) = precheck(&kind, &policy) {
        base::alert(parent, "This operation is not allowed", &guard_message(&e));
        return;
    }
    let options = JobOptions {
        no_usb_trash: app.settings().no_usb_trash(),
        start_paused: false,
    };
    let running = Rc::new(Running {
        app: app.clone(),
        parent: parent.cloned(),
        overwrite,
        handle: jobs::spawn_with_options(kind, policy, options),
        dialog: ProgressDialog::new(app.gtk(), title),
        shown: Cell::new(false),
        finished: Cell::new(false),
        paused: Cell::new(false),
        total: Cell::new((0, 0)),
        done: RefCell::new(Some(Box::new(done))),
    });

    RUNNING.with_borrow_mut(|jobs| jobs.push(Rc::downgrade(&running)));

    let weak = Rc::downgrade(&running);
    running.dialog.pause_button().connect_clicked(move |_| {
        let Some(running) = weak.upgrade() else {
            return;
        };
        let paused = !running.paused.get();
        running.paused.set(paused);
        if paused {
            running.handle.pause();
        } else {
            running.handle.resume();
        }
        running.dialog.set_paused(paused);
    });
    let weak = Rc::downgrade(&running);
    running.dialog.stop_button().connect_clicked(move |_| {
        let Some(running) = weak.upgrade() else {
            return;
        };
        if running.finished.get() {
            running.dialog.window().close();
        } else {
            running.handle.cancel();
        }
    });
    // Closing the window of a running job cancels it rather than hiding it.
    let weak = Rc::downgrade(&running);
    running
        .dialog
        .window()
        .connect_close_request(move |_| match weak.upgrade() {
            Some(running) if !running.finished.get() => {
                running.handle.cancel();
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        });
    let weak = Rc::downgrade(&running);
    glib::timeout_add_local_once(SHOW_AFTER, move || {
        if let Some(running) = weak.upgrade()
            && !running.finished.get()
        {
            running.show();
        }
    });

    // The task owns the job until it has finished; the application stays
    // alive for as long (§8.27).
    let hold = app.gtk().hold();
    let si = app.settings().si_unit();
    glib::spawn_future_local(async move {
        while let Ok(event) = running.handle.events.recv().await {
            match event {
                JobEvent::Preparing => {}
                JobEvent::Prepared {
                    total_files,
                    total_bytes,
                } => {
                    running.total.set((total_files, total_bytes));
                    running
                        .dialog
                        .set_progress((0, 0), running.total.get(), 0, si);
                }
                JobEvent::Current { src, dest } => running
                    .dialog
                    .set_current(&display(&src), dest.as_deref().map(display).as_deref()),
                JobEvent::Progress {
                    done_files,
                    done_bytes,
                    bytes_per_sec,
                } => running.dialog.set_progress(
                    (done_files, done_bytes),
                    running.total.get(),
                    bytes_per_sec,
                    si,
                ),
                JobEvent::Ask(question, reply) => running.ask(question, reply),
                JobEvent::Finished(summary) => {
                    running.finish(&summary);
                    break;
                }
            }
        }
        drop(hold);
    });
}
