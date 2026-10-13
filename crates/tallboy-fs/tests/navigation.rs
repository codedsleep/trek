use std::{fs, path::PathBuf, time::Duration};
use tallboy_fs::{
    bookmarks::{Bookmarks, Store},
    complete,
};
#[test]
fn bookmarks_roundtrip_and_edits() {
    let text = "file:///home/u Home Label\nsftp://host/share Remote\nunknown line\n\nadmin:///etc Blocked\n";
    let mut b = Bookmarks::parse(text);
    assert_eq!(b.encode(), text);
    assert_eq!(b.entries().len(), 2);
    b.rename(0, Some("new label".into())).unwrap();
    b.move_to(0, 1).unwrap();
    assert_eq!(b.entries()[1].label.as_deref(), Some("new label"));
    b.remove(0).unwrap();
    assert!(b.encode().contains("unknown line"));
    assert!(b.encode().contains("admin:///etc"));
    assert!(b.rename(0, Some("bad\nlabel".into())).is_err());
}
#[test]
fn monitored_bookmarks_and_directory_completion() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let s = Store::open(t.path()).unwrap();
            fs::write(t.path().join("gtk-3.0/bookmarks"), "file:///tmp Scratch\n").unwrap();
            glib::timeout_future(Duration::from_millis(450)).await;
            let changes: Vec<_> = std::iter::from_fn(|| s.changes.try_recv().ok()).collect();
            assert_eq!(changes.len(), 1);
            assert_eq!(
                changes[0].as_ref().unwrap().entries()[0].label.as_deref(),
                Some("Scratch")
            );
            let deep = t.path().join("deep");
            fs::create_dir(&deep).unwrap();
            for dir in ["level-1", "level-2", "other", ".hidden"] {
                fs::create_dir(deep.join(dir)).unwrap();
            }
            fs::write(deep.join("level-file"), b"x").unwrap();
            let file = gio::File::for_path(&deep);
            assert!(complete::has_subdirectories(&file, false).await.unwrap());
            let items = complete::prefix(&file, "level", false).await.unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].display_name, "level-1");
            assert!(
                !complete::has_subdirectories(&gio::File::for_path(deep.join("level-1")), false)
                    .await
                    .unwrap()
            );
        })
    })
    .unwrap();
}
