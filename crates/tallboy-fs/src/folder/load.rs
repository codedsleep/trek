//! The initial listing, read on a worker thread, and the one-shot mount retry.
//!
//! Nothing here waits for the main loop: the worker opens the folder, asks
//! about its filesystem, turns every entry into a `FileMeta` and sends the
//! batches itself. The main context only hears whether the folder could be
//! opened, because mounting it needs the caller's mount operation.
use super::*;
use crate::meta;
use std::time::{Duration, Instant};

/// The first batch is small, so that a screenful is on its way while the
/// rest of a large folder is still being read.
const FIRST_BATCH: usize = 64;
/// A batch is sent early when the backend is this slow in filling it.
const SLOW_BATCH: Duration = Duration::from_millis(50);
/// How long tested types are collected before they are sent.
const TYPED_EVERY: Duration = Duration::from_millis(100);

/// Run one operation; NotMounted triggers one mount and one retry, never a loop.
pub(crate) async fn retry_mount<T, F, Fut, M, MFut>(
    mut operation: F,
    mount: M,
) -> Result<T, glib::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, glib::Error>>,
    M: FnOnce() -> MFut,
    MFut: std::future::Future<Output = Result<(), glib::Error>>,
{
    match operation().await {
        Err(e) if e.matches(gio::IOErrorEnum::NotMounted) => {
            mount().await?;
            operation().await
        }
        other => other,
    }
}

/// What the worker needs; everything in it may leave the main thread.
#[derive(Clone)]
struct Job {
    file: gio::File,
    options: Options,
    cancel: gio::Cancellable,
    tx: async_channel::Sender<FolderEvent>,
    filesystem: Arc<OnceLock<Option<String>>>,
    start: Instant,
}

impl Job {
    fn send(&self, event: FolderEvent) {
        if !self.cancel.is_cancelled() {
            let _ = self.tx.try_send(event);
        }
    }
}

pub(super) async fn run(inner: Rc<Inner>, mount: Option<gio::MountOperation>) {
    inner.send(FolderEvent::Started);
    let job = Job {
        file: inner.file.clone(),
        options: inner.options,
        cancel: inner.cancel.clone(),
        tx: inner.tx.clone(),
        filesystem: inner.filesystem.clone(),
        start: Instant::now(),
    };
    let opened = retry_mount(
        || open_on_worker(job.clone()),
        || {
            inner
                .file
                .mount_enclosing_volume_future(gio::MountMountFlags::NONE, mount.as_ref())
        },
    )
    .await;
    if let Err(e) = opened {
        inner.send(FolderEvent::Error(e.into()));
    }
}

/// Starts the worker and resolves once it has opened the folder, or failed
/// to; the listing itself goes on in the worker.
async fn open_on_worker(job: Job) -> Result<(), glib::Error> {
    let (opened_tx, opened) = async_channel::bounded(1);
    let spawned = std::thread::Builder::new()
        .name("tallboy-folder".into())
        .spawn(move || {
            let attrs = meta::listing_attributes(job.file.is_native());
            match job.file.enumerate_children(
                &attrs,
                gio::FileQueryInfoFlags::NONE,
                Some(&job.cancel),
            ) {
                Ok(enumerator) => {
                    let _ = opened_tx.try_send(Ok(()));
                    list(&job, &enumerator);
                }
                Err(e) => {
                    let _ = opened_tx.try_send(Err(e));
                }
            }
        });
    if let Err(e) = spawned {
        return Err(glib::Error::new(
            gio::IOErrorEnum::Failed,
            &format!("Could not start a thread to read the folder: {e}"),
        ));
    }
    opened.recv().await.unwrap_or_else(|_| {
        Err(glib::Error::new(
            gio::IOErrorEnum::Cancelled,
            "The folder was not read",
        ))
    })
}

fn filesystem_info(job: &Job) -> Option<FsInfo> {
    let cancel = Some(&job.cancel);
    let info = job
        .file
        .query_filesystem_info("filesystem::free,filesystem::size,filesystem::type", cancel)
        .map_err(|e| log::warn!("Folder capacity query: {e}"))
        .ok()?;
    // Identity is a file attribute, not a filesystem-info attribute.
    let id = job
        .file
        .query_info("id::filesystem", gio::FileQueryInfoFlags::NONE, cancel)
        .ok()
        .and_then(|info| info.attribute_string("id::filesystem"))
        .map(String::from);
    Some(FsInfo {
        free: info
            .has_attribute("filesystem::free")
            .then(|| info.attribute_uint64("filesystem::free")),
        total: info
            .has_attribute("filesystem::size")
            .then(|| info.attribute_uint64("filesystem::size")),
        filesystem_type: info
            .attribute_string("filesystem::type")
            .map(|s| s.to_string()),
        id,
    })
}

/// Reads the whole folder. Runs on the worker thread.
fn list(job: &Job, enumerator: &gio::FileEnumerator) {
    let cancel = Some(&job.cancel);
    let filesystem = filesystem_info(job);
    let filesystem_type = filesystem
        .as_ref()
        .and_then(|info| info.filesystem_type.clone());
    let _ = job.filesystem.set(filesystem_type.clone());
    if let Some(info) = filesystem {
        job.send(FolderEvent::FsInfo(info));
    }
    let size = job.options.batch_size.clamp(1, 1000) as usize;
    let mut wanted = FIRST_BATCH.min(size);
    let mut batch = Vec::with_capacity(wanted);
    let mut sent = Instant::now();
    // Files listed by the type their name suggests, still to be read.
    let mut untested = Vec::new();
    let flush = |batch: &mut Vec<FileMeta>, wanted: &mut usize, sent: &mut Instant| {
        if batch.is_empty() {
            return;
        }
        if *wanted != size {
            crate::trace::elapsed("folder first batch", job.start);
            *wanted = size;
        }
        job.send(FolderEvent::Batch(std::mem::replace(
            batch,
            Vec::with_capacity(size),
        )));
        *sent = Instant::now();
    };
    loop {
        if job.cancel.is_cancelled() {
            return;
        }
        match enumerator.next_file(cancel) {
            Ok(Some(info)) => {
                let meta = FileMeta::from_info(
                    &job.file.child(info.name()),
                    &info,
                    job.options.backup_as_hidden,
                    filesystem_type.as_deref(),
                );
                if meta.type_uncertain && !job.options.defer_content_test {
                    untested.push(meta.uri.clone());
                }
                batch.push(meta);
                if batch.len() >= wanted || sent.elapsed() >= SLOW_BATCH {
                    flush(&mut batch, &mut wanted, &mut sent);
                }
            }
            Ok(None) => break,
            Err(e) => {
                flush(&mut batch, &mut wanted, &mut sent);
                job.send(FolderEvent::Error(e.into()));
                return;
            }
        }
    }
    flush(&mut batch, &mut wanted, &mut sent);
    if let (_, Some(e)) = enumerator.close(cancel) {
        log::warn!("Folder enumerator close: {e}");
    }
    crate::trace::elapsed("folder load finished", job.start);
    job.send(FolderEvent::Finished);
    test_types(job, untested);
}

/// Reads the start of every file the listing typed by name alone, the way
/// GIO does for `standard::content-type`, and reports what it finds.
fn test_types(job: &Job, untested: Vec<String>) {
    if untested.is_empty() {
        return;
    }
    let mut typed = Vec::new();
    let mut sent = Instant::now();
    for uri in untested {
        if job.cancel.is_cancelled() {
            return;
        }
        // A file that has gone meanwhile is the monitor's to report.
        if let Some(facts) = meta::sniff(&gio::File::for_uri(&uri), Some(&job.cancel)) {
            typed.push((uri, facts));
        }
        if sent.elapsed() >= TYPED_EVERY && !typed.is_empty() {
            job.send(FolderEvent::Typed(std::mem::take(&mut typed)));
            sent = Instant::now();
        }
    }
    if !typed.is_empty() {
        job.send(FolderEvent::Typed(typed));
    }
    crate::trace::elapsed("folder types tested", job.start);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn failing_mount_and_one_retry() {
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            ctx.block_on(async {
                let attempts = Cell::new(0);
                let result = retry_mount(
                    || {
                        attempts.set(attempts.get() + 1);
                        async {
                            Err::<(), _>(glib::Error::new(gio::IOErrorEnum::NotMounted, "offline"))
                        }
                    },
                    || async { Err(glib::Error::new(gio::IOErrorEnum::Failed, "mount failed")) },
                )
                .await;
                assert!(result.unwrap_err().to_string().contains("mount failed"));
                assert_eq!(attempts.get(), 1);
                let result = retry_mount(
                    || {
                        attempts.set(attempts.get() + 1);
                        async {
                            Err::<(), _>(glib::Error::new(gio::IOErrorEnum::NotMounted, "offline"))
                        }
                    },
                    || async { Ok(()) },
                )
                .await;
                assert!(result.unwrap_err().matches(gio::IOErrorEnum::NotMounted));
                assert_eq!(attempts.get(), 3);
            })
        })
        .unwrap();
    }
}
