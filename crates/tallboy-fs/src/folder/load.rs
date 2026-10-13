//! Initial async folder load and one-shot mount retry.
use super::*;
use crate::meta;
use std::time::Instant;
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
pub(super) async fn run(inner: Rc<Inner>, mount: Option<gio::MountOperation>) {
    inner.send(FolderEvent::Started);
    let start = Instant::now();
    let attrs = meta::attributes(inner.options.defer_content_test);
    let result = retry_mount(
        || {
            inner.file.enumerate_children_future(
                &attrs,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
        },
        || {
            inner
                .file
                .mount_enclosing_volume_future(gio::MountMountFlags::NONE, mount.as_ref())
        },
    )
    .await;
    let enumerator = match result {
        Ok(e) => e,
        Err(e) => {
            inner.send(FolderEvent::Error(e.into()));
            return;
        }
    };
    match inner
        .file
        .query_filesystem_info_future(
            "filesystem::free,filesystem::size,filesystem::type",
            glib::Priority::DEFAULT,
        )
        .await
    {
        Ok(i) => {
            // Identity is a file attribute, not a filesystem-info attribute.
            let id = inner
                .file
                .query_info_future(
                    "id::filesystem",
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await
                .ok()
                .and_then(|info| info.attribute_string("id::filesystem"))
                .map(String::from);
            let fs = FsInfo {
                free: i
                    .has_attribute("filesystem::free")
                    .then(|| i.attribute_uint64("filesystem::free")),
                total: i
                    .has_attribute("filesystem::size")
                    .then(|| i.attribute_uint64("filesystem::size")),
                filesystem_type: i
                    .attribute_string("filesystem::type")
                    .map(|s| s.to_string()),
                id,
            };
            *inner.filesystem.borrow_mut() = fs.filesystem_type.clone();
            inner.send(FolderEvent::FsInfo(fs));
        }
        Err(e) => log::warn!("Folder capacity query: {e}"),
    }
    let mut first = true;
    loop {
        if inner.cancel.is_cancelled() {
            return;
        }
        match enumerator
            .next_files_future(
                inner.options.batch_size.clamp(1, 1000),
                glib::Priority::DEFAULT,
            )
            .await
        {
            Ok(infos) if infos.is_empty() => break,
            Ok(infos) => {
                let fs = inner.filesystem.borrow().clone();
                let files = infos
                    .iter()
                    .map(|info| {
                        FileMeta::from_info(
                            &inner.file.child(info.name()),
                            info,
                            inner.options.backup_as_hidden,
                            fs.as_deref(),
                        )
                    })
                    .collect();
                inner.send(FolderEvent::Batch(files));
                if first {
                    crate::trace::elapsed("folder first batch", start);
                    first = false;
                }
            }
            Err(e) => {
                inner.send(FolderEvent::Error(e.into()));
                return;
            }
        }
    }
    if let Err(e) = enumerator.close_future(glib::Priority::DEFAULT).await {
        log::warn!("Folder enumerator close: {e}");
    }
    crate::trace::elapsed("folder load finished", start);
    inner.send(FolderEvent::Finished);
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
