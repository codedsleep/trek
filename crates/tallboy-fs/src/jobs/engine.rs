//! One worker per job, cooperative controls and bounded-rate progress.
use super::*;
use std::{
    sync::{Arc, Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};
pub(super) struct Control {
    pub cancel: gio::Cancellable,
    paused: Mutex<bool>,
    wake: Condvar,
}
impl Control {
    pub fn checkpoint(&self) -> Result<(), JobError> {
        let mut paused = self
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *paused && !self.cancel.is_cancelled() {
            paused = self
                .wake
                .wait(paused)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if self.cancel.is_cancelled() {
            Err(JobError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn cancel(&self) {
        self.cancel.cancel();
        self.wake.notify_all();
    }
}
/// Handle for a running job. Drop cancels it; the UI retains handles while jobs run.
pub struct JobHandle {
    control: Arc<Control>,
    /// GTK-free events; use recv().await on the main context.
    pub events: async_channel::Receiver<JobEvent>,
}
impl JobHandle {
    /// Pause between files and within GIO copy progress callbacks.
    pub fn pause(&self) {
        *self
            .control
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    }
    /// Resume and wake a paused worker.
    pub fn resume(&self) {
        *self
            .control
            .paused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        self.control.wake.notify_all();
    }
    /// Cancel pending GIO and wake pause/question waits.
    pub fn cancel(&self) {
        self.control.cancel();
    }
}
impl Drop for JobHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
/// Start a guarded worker using environment-derived allowed roots.
pub fn spawn(kind: JobKind) -> JobHandle {
    spawn_with_policy(kind, Policy::from_environment())
}
/// Start a guarded worker using an explicit policy (for isolated tests and runs).
pub fn spawn_with_policy(kind: JobKind, policy: Policy) -> JobHandle {
    spawn_with_options(kind, policy, JobOptions::default())
}
/// Start a guarded worker with preferences and initial pause state.
pub fn spawn_with_options(kind: JobKind, policy: Policy, options: JobOptions) -> JobHandle {
    let (tx, events) = async_channel::unbounded();
    let control = Arc::new(Control {
        cancel: gio::Cancellable::new(),
        paused: Mutex::new(options.start_paused),
        wake: Condvar::new(),
    });
    let worker = control.clone();
    let failure_tx = tx.clone();
    if let Err(e) = std::thread::Builder::new()
        .name("tallboy-job".into())
        .spawn(move || {
            let mut runner = Runner {
                control: worker,
                policy,
                tx,
                start: Instant::now(),
                last_progress: None,
                summary: JobSummary::default(),
                conflict: None,
                trash_choice: None,
                error_skip_all: false,
                options,
                prepared: Default::default(),
            };
            runner.send(JobEvent::Preparing);
            if let Err(e) = runner.run(&kind) {
                if matches!(e, JobError::Cancelled) || runner.control.cancel.is_cancelled() {
                    runner.summary.cancelled = true;
                } else {
                    runner.summary.errors.push(e.to_string());
                }
            }
            runner.summary.cancelled |= runner.control.cancel.is_cancelled();
            runner.send(JobEvent::Finished(runner.summary.clone()));
        })
    {
        let _ = failure_tx.try_send(JobEvent::Finished(JobSummary {
            errors: vec![e.to_string()],
            ..Default::default()
        }));
    }
    JobHandle { control, events }
}
pub(super) struct Runner {
    pub control: Arc<Control>,
    pub policy: Policy,
    pub tx: async_channel::Sender<JobEvent>,
    pub start: Instant,
    pub last_progress: Option<Instant>,
    pub summary: JobSummary,
    pub conflict: Option<Answer>,
    pub trash_choice: Option<Answer>,
    pub error_skip_all: bool,
    pub options: JobOptions,
    pub prepared: std::collections::BTreeMap<String, (u64, u64)>,
}
impl Runner {
    pub fn send(&self, event: JobEvent) {
        if self.tx.try_send(event).is_err() {
            self.control.cancel();
        }
    }
    pub fn checkpoint(&self) -> Result<(), JobError> {
        self.control.checkpoint()
    }
    pub fn progress(&mut self, inflight: u64) {
        let now = Instant::now();
        if self
            .last_progress
            .is_none_or(|last| now.duration_since(last) >= Duration::from_millis(100))
        {
            self.last_progress = Some(now);
            let bytes = self.summary.done_bytes.saturating_add(inflight);
            let seconds = self.start.elapsed().as_secs_f64().max(0.001);
            self.send(JobEvent::Progress {
                done_files: self.summary.done_files,
                done_bytes: bytes,
                bytes_per_sec: (bytes as f64 / seconds) as u64,
            });
        }
    }
    pub fn completed(&mut self, bytes: u64) {
        self.summary.done_files += 1;
        self.summary.done_bytes += bytes;
        self.progress(0);
    }
    pub fn ask(&mut self, q: Question) -> Result<Answer, JobError> {
        if let Question::Conflict { can_overwrite, .. } = &q
            && let Some(a) = &self.conflict
            // A remembered "overwrite all" never extends to replacing a
            // folder with a file; that conflict is always put to the user.
            && (*can_overwrite || !matches!(a, Answer::Overwrite { .. }))
        {
            return Ok(a.clone());
        }
        if matches!(q, Question::TrashUnsupported { .. })
            && let Some(a) = &self.trash_choice
        {
            return Ok(a.clone());
        }
        if matches!(q, Question::Error { .. }) && self.error_skip_all {
            return Ok(Answer::Skip { apply_to_all: true });
        }
        let (tx, rx) = mpsc::channel();
        let conflict = matches!(q, Question::Conflict { .. });
        let overwrite_refused = matches!(
            q,
            Question::Conflict {
                can_overwrite: false,
                ..
            }
        );
        let trash = matches!(q, Question::TrashUnsupported { .. });
        let error = matches!(q, Question::Error { .. });
        self.send(JobEvent::Ask(q, Reply(tx)));
        let answer = loop {
            self.checkpoint()?;
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok(a) => break a,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(JobError::Cancelled),
            }
        };
        match &answer {
            Answer::Cancel => {
                self.control.cancel();
                return Err(JobError::Cancelled);
            }
            Answer::Overwrite { .. } if overwrite_refused => return Err(JobError::ReplaceFolder),
            Answer::Overwrite { apply_to_all: true } | Answer::Skip { apply_to_all: true }
                if conflict =>
            {
                self.conflict = Some(answer.clone())
            }
            Answer::DeleteInstead { apply_to_all: true } | Answer::Skip { apply_to_all: true }
                if trash =>
            {
                self.trash_choice = Some(answer.clone())
            }
            Answer::Skip { apply_to_all: true } if error => self.error_skip_all = true,
            _ => {}
        }
        Ok(answer)
    }
    pub fn run(&mut self, kind: &JobKind) -> Result<(), JobError> {
        self.preflight(kind)?;
        let sources = if matches!(kind, JobKind::EmptyTrash) {
            children(&gio::File::for_uri("trash:///"), &self.control.cancel)?
                .iter()
                .map(|f| f.uri().to_string())
                .collect()
        } else {
            kind.sources()
        };
        let mut totals = (0, 0);
        for uri in &sources {
            let file = gio::File::for_uri(uri);
            self.count_prepare(&file, &mut totals)?;
        }
        self.send(JobEvent::Prepared {
            total_files: totals.0,
            total_bytes: totals.1,
        });
        for uri in sources {
            let file = gio::File::for_uri(&uri);
            loop {
                self.checkpoint()?;
                let result = match kind {
                    JobKind::Copy { dest_dir, .. } => {
                        self.transfer_item(&file, &gio::File::for_uri(dest_dir), false, false)
                    }
                    JobKind::Move { dest_dir, .. } => {
                        self.transfer_item(&file, &gio::File::for_uri(dest_dir), true, false)
                    }
                    JobKind::Link { dest_dir, .. } => {
                        self.transfer_item(&file, &gio::File::for_uri(dest_dir), false, true)
                    }
                    JobKind::Delete { .. } | JobKind::EmptyTrash => self.delete(&file, None),
                    JobKind::Trash { .. } => self.trash(&file),
                    JobKind::Restore { .. } => self.restore(&file),
                    JobKind::SetAttrs {
                        change, recursive, ..
                    } => self.attrs(&file, *change, *recursive),
                    JobKind::Count { .. } => self.count_execute(&file),
                };
                match result {
                    Ok(()) => break,
                    Err(e)
                        if self.control.cancel.is_cancelled()
                            || matches!(e, JobError::Cancelled) =>
                    {
                        return Err(JobError::Cancelled);
                    }
                    Err(e) => {
                        let message = e.to_string();
                        match self.ask(Question::Error {
                            uri: uri.clone(),
                            message: message.clone(),
                            retryable: true,
                        })? {
                            Answer::Retry => continue,
                            Answer::Cancel => return Err(JobError::Cancelled),
                            Answer::Skip { .. }
                            | Answer::Overwrite { .. }
                            | Answer::Rename(_)
                            | Answer::DeleteInstead { .. }
                            | Answer::EnterMount => {
                                self.summary.errors.push(message);
                                self.summary.skipped += 1;
                                break;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn preflight(&self, kind: &JobKind) -> Result<(), JobError> {
        let read_only = matches!(kind, JobKind::Count { .. });
        for uri in kind.sources() {
            let f = gio::File::for_uri(&uri);
            tallboy_core::location::Location::parse(
                &uri,
                std::path::Path::new("/"),
                std::path::Path::new("/"),
            )?;
            if !read_only {
                crate::guard::check(&f, &self.policy)?;
            }
            if let Some(dest) = kind.destination() {
                let dest = gio::File::for_uri(dest);
                let source_info = info(&f, &self.control.cancel)?;
                if source_info.file_type() == gio::FileType::Directory && !source_info.is_symlink()
                {
                    if dest.equal(&f) || dest.has_prefix(&f) {
                        return Err(JobError::IntoSelf);
                    }
                    if let (Some(src), Some(dst)) = (f.path(), dest.path()) {
                        let src = crate::guard::resolve(&src).map_err(GuardError::from)?;
                        let dst = crate::guard::resolve(&dst).map_err(GuardError::from)?;
                        if dst.starts_with(src) {
                            return Err(JobError::IntoSelf);
                        }
                    }
                }
                let basename = f.basename().ok_or(JobError::Invalid)?;
                crate::guard::check(&dest.child(basename), &self.policy)?;
            }
        }
        if let Some(dest) = kind.destination() {
            crate::guard::check(&gio::File::for_uri(dest), &self.policy)?;
        }
        if matches!(
            kind,
            JobKind::Trash { .. } | JobKind::EmptyTrash | JobKind::Restore { .. }
        ) {
            crate::guard::check(&gio::File::for_uri("trash:///"), &self.policy)?;
        }
        Ok(())
    }
}
impl From<tallboy_core::location::LocationError> for JobError {
    fn from(e: tallboy_core::location::LocationError) -> Self {
        Self::Guard(GuardError::Location(e))
    }
}
