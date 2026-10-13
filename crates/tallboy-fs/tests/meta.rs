use std::{fs, os::unix::ffi::OsStringExt, path::PathBuf};
use tallboy_fs::meta::{FileMeta, compare};
#[test]
fn metadata_variants() {
    fn send<T: Send>() {}
    send::<FileMeta>();
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    fs::write(t.path().join("regular"), b"abc").unwrap();
    fs::create_dir(t.path().join("dir")).unwrap();
    std::os::unix::fs::symlink("dir", t.path().join("link")).unwrap();
    std::os::unix::fs::symlink("missing", t.path().join("broken")).unwrap();
    for name in [".hidden", "backup~", "file2", "file10"] {
        fs::write(t.path().join(name), b"x").unwrap();
    }
    let raw = std::ffi::OsString::from_vec(vec![0xff]);
    fs::write(t.path().join(&raw), b"x").unwrap();
    let q = |name: &std::ffi::OsStr| {
        FileMeta::query(&gio::File::for_path(t.path().join(name)), true, false, None).unwrap()
    };
    assert_eq!(q("regular".as_ref()).size, 3);
    assert_eq!(q("dir".as_ref()).file_type, gio::FileType::Directory);
    assert!(q("link".as_ref()).symlink);
    assert!(q("broken".as_ref()).symlink);
    assert!(q(".hidden".as_ref()).hidden);
    assert!(q("backup~".as_ref()).hidden);
    assert_eq!(q(&raw).name, PathBuf::from(raw));
    assert_eq!(
        compare(
            &q("file2".as_ref()),
            &q("file10".as_ref()),
            &Default::default()
        ),
        std::cmp::Ordering::Less
    );
    // Values that repeat across files share one allocation.
    let (a, b) = (q("file2".as_ref()), q("file10".as_ref()));
    assert!(std::sync::Arc::ptr_eq(&a.content_type, &b.content_type));
    assert!(std::sync::Arc::ptr_eq(&a.description, &b.description));
    assert!(std::sync::Arc::ptr_eq(&a.owner, &b.owner));
    assert_eq!(a.icon.as_deref(), b.icon.as_deref());
    assert!(q("dir".as_ref()).sort_facts().directory);
    assert!(!a.sort_facts().directory);
    assert!(
        FileMeta::query(
            &gio::File::for_path(t.path().join("regular")),
            false,
            true,
            None
        )
        .is_ok()
    );
}
#[test]
fn sparse_info_from_a_backend_is_not_fatal() {
    // Some GVFS backends leave out attributes that were asked for; the trash
    // root has no edit name or size. Run under G_DEBUG=fatal-criticals.
    let file = gio::File::for_uri("sftp://example.invalid/folder/item");
    let m = FileMeta::from_info(&file, &gio::FileInfo::new(), true, None);
    assert_eq!(m.name, PathBuf::from("item"));
    assert_eq!(m.display_name, "item");
    assert_eq!(m.edit_name, "item");
    assert_eq!(m.file_type, gio::FileType::Unknown);
    assert_eq!(m.size, 0);
    assert!(!m.hidden && !m.symlink && m.icon.is_none());
    // Unreported rights are assumed, except the right to execute.
    assert!(m.access.read && m.access.delete && m.access.rename && m.access.trash);
    assert!(!m.access.execute);
}
