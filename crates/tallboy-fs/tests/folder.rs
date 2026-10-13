use gio::prelude::*;
use std::{fs, path::PathBuf, time::Duration};
use tallboy_fs::folder::{Folder, FolderEvent, Options, nearest_existing_ancestor};
fn temp() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
async fn finish(folder: &Folder) -> usize {
    let mut count = 0;
    loop {
        match folder.events.recv().await.unwrap() {
            FolderEvent::Batch(items) => count += items.len(),
            FolderEvent::Finished => return count,
            FolderEvent::Error(e) => panic!("{e}"),
            _ => {}
        }
    }
}
#[test]
fn folder_capacity_includes_the_file_filesystem_identity() {
    let t = temp();
    let file = gio::File::for_path(t.path());
    let expected = file
        .query_info(
            "id::filesystem",
            gio::FileQueryInfoFlags::NONE,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .attribute_string("id::filesystem")
        .unwrap()
        .to_string();
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let folder = Folder::new(&file, Options::default(), None).unwrap();
            let mut id = None;
            loop {
                match folder.events.recv().await.unwrap() {
                    FolderEvent::FsInfo(info) => id = info.id,
                    FolderEvent::Finished => break,
                    FolderEvent::Error(e) => panic!("{e}"),
                    _ => {}
                }
            }
            assert_eq!(id.as_deref(), Some(expected.as_str()));
        })
    })
    .unwrap();
}
#[test]
fn load_5000_and_cancel_mid_load() {
    let t = temp();
    for n in 0..5000 {
        fs::write(t.path().join(format!("f{n}")), b"x").unwrap();
    }
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let file = gio::File::for_path(t.path());
            let f = Folder::new(&file, Options::default(), None).unwrap();
            assert_eq!(finish(&f).await, 5000);
            drop(f);
            let f = Folder::new(&file, Options::default(), None).unwrap();
            while !matches!(f.events.recv().await.unwrap(), FolderEvent::Batch(_)) {}
            f.cancel();
            glib::timeout_future(Duration::from_millis(100)).await;
            assert!(f.events.try_recv().is_err());
            assert!(f.events.is_closed());
        })
    })
    .unwrap();
}
async fn one_event(folder: &Folder, expected: impl Fn(&FolderEvent) -> bool) {
    glib::timeout_future(Duration::from_millis(450)).await;
    let events: Vec<_> = std::iter::from_fn(|| folder.events.try_recv().ok()).collect();
    assert_eq!(events.len(), 1, "{events:?}");
    assert!(expected(&events[0]), "{events:?}");
}
#[test]
fn monitor_create_modify_rename_delete_and_root_gone() {
    let t = temp();
    let path = t.path().join("watched");
    fs::create_dir(&path).unwrap();
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(||ctx.block_on(async{let file=gio::File::for_path(&path);let f=Folder::new(&file,Options::default(),None).unwrap();finish(&f).await;fs::write(path.join("a"),b"one").unwrap();one_event(&f,|e|matches!(e,FolderEvent::Added(m) if m.display_name=="a")).await;fs::write(path.join("a"),b"updated").unwrap();one_event(&f,|e|matches!(e,FolderEvent::Changed(m) if m.size==7)).await;fs::rename(path.join("a"),path.join("b")).unwrap();one_event(&f,|e|matches!(e,FolderEvent::Renamed{from,to} if from.ends_with("/a") && to.display_name=="b")).await;fs::remove_file(path.join("b")).unwrap();one_event(&f,|e|matches!(e,FolderEvent::Removed(uri) if uri.ends_with("/b"))).await;fs::remove_dir(&path).unwrap();one_event(&f,|e|matches!(e,FolderEvent::RootGone)).await;assert!(nearest_existing_ancestor(&file).await.unwrap().equal(&gio::File::for_path(t.path())));})).unwrap();
}

#[test]
fn listing_follows_symlinks_for_display_metadata() {
    let t = temp();
    fs::create_dir(t.path().join("dir")).unwrap();
    fs::write(t.path().join("dir/inner"), b"12345").unwrap();
    std::os::unix::fs::symlink("dir", t.path().join("dirlink")).unwrap();
    std::os::unix::fs::symlink("dir/inner", t.path().join("filelink")).unwrap();
    std::os::unix::fs::symlink("missing", t.path().join("broken")).unwrap();
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let file = gio::File::for_path(t.path());
            let f = Folder::new(&file, Options::default(), None).unwrap();
            let mut seen = std::collections::HashMap::new();
            loop {
                match f.events.recv().await.unwrap() {
                    FolderEvent::Batch(batch) => {
                        for m in batch {
                            seen.insert(m.display_name.clone(), m);
                        }
                    }
                    FolderEvent::Finished => break,
                    _ => {}
                }
            }
            let dirlink = &seen["dirlink"];
            assert!(dirlink.symlink);
            assert_eq!(dirlink.file_type, gio::FileType::Directory);
            assert!(dirlink.sort_facts().directory);
            assert_eq!(dirlink.symlink_target.as_deref(), Some("dir".as_ref()));
            let filelink = &seen["filelink"];
            assert!(filelink.symlink);
            assert_eq!(filelink.file_type, gio::FileType::Regular);
            assert_eq!(filelink.size, 5);
            let broken = &seen["broken"];
            assert!(broken.symlink);
            assert_eq!(broken.file_type, gio::FileType::SymbolicLink);
            // A link created later is reported the same way by the monitor.
            std::os::unix::fs::symlink("dir", t.path().join("late")).unwrap();
            one_event(
                &f,
                |e| matches!(e, FolderEvent::Added(m) if m.symlink && m.file_type == gio::FileType::Directory),
            )
            .await;
        })
    })
    .unwrap();
}
#[test]
fn a_change_to_the_watched_folder_itself_is_not_a_child_event() {
    use std::os::unix::fs::PermissionsExt;
    let t = temp();
    let path = t.path().join("watched");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("child"), b"x").unwrap();
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let f = Folder::new(&gio::File::for_path(&path), Options::default(), None).unwrap();
            finish(&f).await;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            glib::timeout_future(Duration::from_millis(450)).await;
            let events: Vec<_> = std::iter::from_fn(|| f.events.try_recv().ok()).collect();
            assert!(events.is_empty(), "{events:?}");
            // A real child change still arrives afterwards.
            fs::write(path.join("child"), b"changed").unwrap();
            one_event(
                &f,
                |e| matches!(e, FolderEvent::Changed(m) if m.display_name == "child"),
            )
            .await;
        })
    })
    .unwrap();
}
