mod common;
use common::*;
use gio::prelude::*;
use std::{fs, path::PathBuf, process::Command};
use trek_fs::{jobs::*, trash};
#[test]
fn gvfs_trash_restore_empty_private_bus() {
    if std::env::var("TREK_TEST_GVFS").as_deref() != Ok("1") {
        eprintln!("GVFS trash test skipped; set TREK_TEST_GVFS=1");
        return;
    }
    let base = glib::user_cache_dir().join("trek-agent");
    fs::create_dir_all(&base).unwrap();
    let root = tempfile::tempdir_in(&base).unwrap();
    let status = Command::new("timeout")
        .args(["--kill-after=2s", "20s", "dbus-run-session"])
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "gvfs_trash_subprocess", "--nocapture"])
        .env("TREK_TRASH_CHILD", root.path())
        .env("XDG_DATA_HOME", root.path().join("data"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_CACHE_HOME", root.path().join("cache"))
        .env("TREK_ALLOW_ROOTS", root.path())
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn gvfs_trash_subprocess() {
    let Some(root) = std::env::var_os("TREK_TRASH_CHILD") else {
        return;
    };
    let root = PathBuf::from(root);
    fs::create_dir_all(root.join("data")).unwrap();
    let src = root.join("sample");
    fs::write(&src, b"original").unwrap();
    let s = success(
        JobKind::Trash {
            files: vec![uri(&src)],
        },
        &root,
    );
    assert!(!s.cancelled);
    assert!(!src.exists());
    let trash_root = gio::File::for_uri("trash:///");
    let e = trash_root
        .enumerate_children(
            "standard::name,trash::orig-path",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .unwrap();
    let item = e.next_file(None::<&gio::Cancellable>).unwrap().unwrap();
    assert_eq!(item.attribute_file_path("trash::orig-path").unwrap(), src);
    let trashed = e.child(&item);
    let (_, error) = e.close(None::<&gio::Cancellable>);
    assert!(error.is_none());
    let ctx = glib::MainContext::default();
    assert!(
        !ctx.with_thread_default(|| ctx.block_on(trash::is_empty()))
            .unwrap()
            .unwrap()
    );
    success(
        JobKind::Restore {
            files: vec![trashed.uri().into()],
        },
        &root,
    );
    assert_eq!(fs::read(&src).unwrap(), b"original");
    success(
        JobKind::Trash {
            files: vec![uri(&src)],
        },
        &root,
    );
    success(JobKind::EmptyTrash, &root);
    // A trashed folder with contents: items inside it cannot be deleted one
    // by one in trash://, only the folder as a whole.
    let folder = root.join("folder");
    fs::create_dir_all(folder.join("inner")).unwrap();
    fs::write(folder.join("inner/file"), b"x").unwrap();
    fs::write(folder.join("top"), b"y").unwrap();
    success(
        JobKind::Trash {
            files: vec![uri(&folder)],
        },
        &root,
    );
    success(JobKind::EmptyTrash, &root);
    assert!(
        ctx.with_thread_default(|| ctx.block_on(trash::is_empty()))
            .unwrap()
            .unwrap()
    );
    // The same for Remove on one trashed folder.
    fs::create_dir_all(folder.join("inner")).unwrap();
    fs::write(folder.join("inner/file"), b"x").unwrap();
    success(
        JobKind::Trash {
            files: vec![uri(&folder)],
        },
        &root,
    );
    let trashed_folder = children_of_trash().remove(0);
    success(
        JobKind::Delete {
            files: vec![trashed_folder],
        },
        &root,
    );
    assert!(
        ctx.with_thread_default(|| ctx.block_on(trash::is_empty()))
            .unwrap()
            .unwrap()
    );
    let tmp = tempfile::tempdir_in("/tmp").unwrap();
    let file = tmp.path().join("unsupported");
    fs::write(&file, b"tmpfs").unwrap();
    let mut p = policy(&root);
    p.roots.push(tmp.path().into());
    let h = spawn_with_policy(
        JobKind::Trash {
            files: vec![uri(&file)],
        },
        p,
    );
    let mut asked = false;
    let s = finish(&h, |q| {
        assert!(matches!(q, Question::TrashUnsupported { .. }));
        asked = true;
        Answer::DeleteInstead { apply_to_all: true }
    });
    assert!(asked);
    assert!(s.errors.is_empty());
    assert!(!file.exists());
    assert!(
        ctx.with_thread_default(|| ctx.block_on(trash::is_empty()))
            .unwrap()
            .unwrap()
    );
}
fn children_of_trash() -> Vec<String> {
    let trash_root = gio::File::for_uri("trash:///");
    let e = trash_root
        .enumerate_children(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            None::<&gio::Cancellable>,
        )
        .unwrap();
    let mut out = vec![];
    while let Some(info) = e.next_file(None::<&gio::Cancellable>).unwrap() {
        out.push(e.child(&info).uri().to_string());
    }
    out
}
#[test]
fn removable_policy() {
    assert!(trash::offer_delete(true, true));
    assert!(!trash::offer_delete(false, true));
    assert!(!trash::offer_delete(true, false));
}
