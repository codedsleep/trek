mod common;
use common::*;
use std::{fs, os::unix::fs::PermissionsExt};
use trek_core::perms::{AttrChange, Audience, Permission};
use trek_fs::jobs::*;
#[test]
fn copy_merge_self_copy_and_into_self_preflight() {
    let t = temp();
    let source = t.path().join("source");
    let dest = t.path().join("dest");
    fs::create_dir_all(source.join("child")).unwrap();
    fs::create_dir_all(dest.join("source")).unwrap();
    fs::write(source.join("child/file"), b"data").unwrap();
    fs::write(dest.join("source/keep"), b"keep").unwrap();
    success(
        JobKind::Copy {
            sources: vec![uri(&source)],
            dest_dir: uri(&dest),
        },
        t.path(),
    );
    assert_eq!(fs::read(dest.join("source/child/file")).unwrap(), b"data");
    assert!(dest.join("source/keep").exists());
    success(
        JobKind::Copy {
            sources: vec![uri(source.join("child/file"))],
            dest_dir: uri(source.join("child")),
        },
        t.path(),
    );
    assert!(source.join("child/file (copy)").exists());
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&source)],
            dest_dir: uri(source.join("child")),
        },
        policy(t.path()),
    );
    let s = finish(&h, |_| Answer::Cancel);
    assert!(s.errors.iter().any(|e| e.contains("into itself")));
    assert!(!source.join("child/source").exists());
}
#[test]
fn every_conflict_answer_and_apply_to_all() {
    for answer in [
        Answer::Overwrite {
            apply_to_all: false,
        },
        Answer::Skip {
            apply_to_all: false,
        },
        Answer::Rename("renamed".into()),
        Answer::Cancel,
    ] {
        let t = temp();
        let src = t.path().join("src");
        let dst = t.path().join("dst");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dst).unwrap();
        fs::write(src.join("a"), b"new").unwrap();
        fs::write(dst.join("a"), b"old").unwrap();
        let h = spawn_with_policy(
            JobKind::Copy {
                sources: vec![uri(src.join("a"))],
                dest_dir: uri(&dst),
            },
            policy(t.path()),
        );
        let s = finish(&h, |q| {
            assert!(matches!(q, Question::Conflict { .. }));
            answer.clone()
        });
        match answer {
            Answer::Overwrite { .. } => assert_eq!(fs::read(dst.join("a")).unwrap(), b"new"),
            Answer::Rename(_) => assert_eq!(fs::read(dst.join("renamed")).unwrap(), b"new"),
            Answer::Skip { .. } => assert_eq!(s.skipped, 1),
            Answer::Cancel => assert!(s.cancelled),
            _ => unreachable!(),
        }
    }
    for skip in [false, true] {
        let t = temp();
        let src = t.path().join("src");
        let dst = t.path().join("dst");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dst).unwrap();
        for name in ["a", "b"] {
            fs::write(src.join(name), b"new").unwrap();
            fs::write(dst.join(name), b"old").unwrap();
        }
        let h = spawn_with_policy(
            JobKind::Copy {
                sources: vec![uri(src.join("a")), uri(src.join("b"))],
                dest_dir: uri(&dst),
            },
            policy(t.path()),
        );
        let mut asks = 0;
        let s = finish(&h, |_| {
            asks += 1;
            if skip {
                Answer::Skip { apply_to_all: true }
            } else {
                Answer::Overwrite { apply_to_all: true }
            }
        });
        assert_eq!(asks, 1);
        assert!(s.errors.is_empty());
        assert_eq!(
            fs::read(dst.join("b")).unwrap(),
            if skip { b"old" } else { b"new" }
        );
    }
}
#[test]
fn links_and_recursive_delete_never_follow_symlinks() {
    let t = temp();
    let target = t.path().join("target");
    let tree = t.path().join("tree");
    fs::create_dir(&target).unwrap();
    fs::create_dir(&tree).unwrap();
    fs::write(target.join("important"), b"keep").unwrap();
    success(
        JobKind::Link {
            sources: vec![uri(&target)],
            dest_dir: uri(&tree),
        },
        t.path(),
    );
    std::os::unix::fs::symlink("loop", tree.join("loop")).unwrap();
    std::os::unix::fs::symlink("missing", tree.join("broken")).unwrap();
    success(
        JobKind::Delete {
            files: vec![uri(&tree)],
        },
        t.path(),
    );
    assert!(!tree.exists());
    assert_eq!(fs::read(target.join("important")).unwrap(), b"keep");
}
#[test]
fn setattrs_count_and_single_item_operations() {
    let t = temp();
    let root = t.path().join("deep");
    fs::create_dir(&root).unwrap();
    let mut path = root.clone();
    for n in 0..20 {
        path = path.join(format!("level{n}"));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("item"), vec![0u8; 123]).unwrap();
    }
    let s = success(
        JobKind::Count {
            files: vec![uri(&root)],
        },
        t.path(),
    );
    let du = std::process::Command::new("du")
        .args(["-sb"])
        .arg(&root)
        .output()
        .unwrap();
    let bytes = String::from_utf8(du.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert_eq!(s.done_bytes, bytes);
    let find = std::process::Command::new("find")
        .arg(&root)
        .args(["-printf", "x"])
        .output()
        .unwrap();
    assert_eq!(s.done_files, find.stdout.len() as u64);
    let mut change = AttrChange::default();
    change.set(Permission::Write, Audience::Nobody);
    success(
        JobKind::SetAttrs {
            files: vec![uri(path.join("item"))],
            change,
            recursive: false,
        },
        t.path(),
    );
    assert_eq!(
        fs::metadata(path.join("item"))
            .unwrap()
            .permissions()
            .mode()
            & 0o222,
        0
    );
    let p = policy(t.path());
    let parent = gio::File::for_path(t.path());
    let a = create_file(&parent, "new", &p).unwrap();
    let b = create_file(&parent, "new", &p).unwrap();
    use gio::prelude::*;
    assert_ne!(a.uri(), b.uri());
    let a = rename(&a, "renamed", &p).unwrap();
    assert!(a.query_exists(None::<&gio::Cancellable>));
    assert!(rename(&b, "renamed", &p).is_err());
    assert!(create_folder(&parent, "newdir", &p).is_ok());
}
#[test]
fn create_makes_the_folders_a_path_names() {
    use gio::prelude::*;
    let t = temp();
    let p = policy(t.path());
    let parent = gio::File::for_path(t.path());
    let made = create_folder(&parent, "test/.planning", &p).unwrap();
    assert_eq!(made.path().unwrap(), t.path().join("test/.planning"));
    assert!(t.path().join("test/.planning").is_dir());
    // Existing folders on the way are reused; the last part stays unique.
    let file = create_file(&parent, "test/.planning/notes.md", &p).unwrap();
    assert_eq!(
        file.path().unwrap(),
        t.path().join("test/.planning/notes.md")
    );
    let again = create_folder(&parent, "test/.planning", &p).unwrap();
    assert_eq!(
        again.path().unwrap(),
        t.path().join("test/.planning (copy)")
    );
    // A file in the way is an error, not overwritten.
    fs::write(t.path().join("plain"), "keep").unwrap();
    assert!(create_folder(&parent, "plain/inner", &p).is_err());
    assert_eq!(fs::read_to_string(t.path().join("plain")).unwrap(), "keep");
    for bad in ["a//b", "/abs", "a/../b", "a/"] {
        assert!(create_folder(&parent, bad, &p).is_err(), "{bad}");
    }
    assert!(!t.path().join("a").exists());
}
#[test]
fn a_folder_may_replace_a_file_and_keeps_its_source() {
    let t = temp();
    let src = t.path().join("src");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::create_dir(src.join("item")).unwrap();
    fs::write(src.join("item/new"), b"new").unwrap();
    fs::write(dst.join("item"), b"old").unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(src.join("item"))],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let s = finish(&h, |q| {
        assert!(matches!(
            q,
            Question::Conflict {
                can_overwrite: true,
                ..
            }
        ));
        Answer::Overwrite { apply_to_all: true }
    });
    assert!(s.errors.is_empty(), "{s:?}");
    assert!(src.join("item").exists());
    assert_eq!(fs::read(dst.join("item/new")).unwrap(), b"new");
    assert_eq!(fs::read_dir(dst).unwrap().count(), 1);
}
#[test]
fn a_file_never_replaces_a_folder() {
    // Overwrite answered directly: refused, the folder and its contents stay.
    let t = temp();
    let src = t.path().join("src");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir_all(dst.join("item")).unwrap();
    fs::write(src.join("item"), b"new").unwrap();
    fs::write(dst.join("item/old"), b"old").unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(src.join("item"))],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let mut conflicts = 0;
    let s = finish(&h, |q| match q {
        Question::Conflict { can_overwrite, .. } => {
            conflicts += 1;
            assert!(!can_overwrite);
            Answer::Overwrite {
                apply_to_all: false,
            }
        }
        Question::Error { message, .. } => {
            assert!(
                message.contains("cannot be replaced by a file"),
                "{message}"
            );
            Answer::Skip {
                apply_to_all: false,
            }
        }
        other => panic!("unexpected {other:?}"),
    });
    assert_eq!(conflicts, 1);
    assert_eq!(s.errors.len(), 1);
    assert_eq!(fs::read(dst.join("item/old")).unwrap(), b"old");
    assert_eq!(fs::read(src.join("item")).unwrap(), b"new");
    assert_eq!(fs::read_dir(&dst).unwrap().count(), 1, "no temporary left");

    // "Overwrite all" given for a file conflict does not reach the folder:
    // the job asks again, and Rename lands beside it.
    let t = temp();
    let src = t.path().join("src");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir_all(dst.join("item")).unwrap();
    fs::write(src.join("a"), b"new").unwrap();
    fs::write(dst.join("a"), b"old").unwrap();
    fs::write(src.join("item"), b"new").unwrap();
    fs::write(dst.join("item/old"), b"old").unwrap();
    for moving in [false, true] {
        let sources = vec![uri(src.join("a")), uri(src.join("item"))];
        let kind = if moving {
            JobKind::Move {
                sources,
                dest_dir: uri(&dst),
            }
        } else {
            JobKind::Copy {
                sources,
                dest_dir: uri(&dst),
            }
        };
        let h = spawn_with_policy(kind, policy(t.path()));
        let mut asked = vec![];
        let s = finish(&h, |q| match q {
            Question::Conflict { can_overwrite, .. } => {
                asked.push(can_overwrite);
                if can_overwrite {
                    Answer::Overwrite { apply_to_all: true }
                } else {
                    Answer::Skip {
                        apply_to_all: false,
                    }
                }
            }
            other => panic!("unexpected {other:?}"),
        });
        assert_eq!(asked, vec![true, false], "moving={moving}");
        assert!(s.errors.is_empty(), "{s:?}");
        assert_eq!(fs::read(dst.join("a")).unwrap(), b"new");
        assert_eq!(fs::read(dst.join("item/old")).unwrap(), b"old");
        // The skipped source is still there, also when moving.
        assert_eq!(fs::read(src.join("item")).unwrap(), b"new");
        fs::write(src.join("a"), b"new").unwrap();
        fs::write(dst.join("a"), b"old").unwrap();
    }
}
#[test]
fn recursive_attrs_skip_symlink_targets_and_non_utf8_self_copy() {
    use std::os::unix::ffi::OsStringExt;
    let t = temp();
    let tree = t.path().join("tree");
    fs::create_dir(&tree).unwrap();
    fs::write(tree.join("item"), b"data").unwrap();
    let target = t.path().join("target");
    fs::write(&target, b"keep").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
    std::os::unix::fs::symlink(&target, tree.join("link")).unwrap();
    let mut change = AttrChange::default();
    change.set(Permission::Write, Audience::Anyone);
    success(
        JobKind::SetAttrs {
            files: vec![uri(&tree)],
            change,
            recursive: true,
        },
        t.path(),
    );
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(
        fs::metadata(tree.join("item"))
            .unwrap()
            .permissions()
            .mode()
            & 0o222,
        0o222
    );
    let raw = std::ffi::OsString::from_vec(b"invalid-\xff".to_vec());
    fs::write(tree.join(&raw), b"raw").unwrap();
    success(
        JobKind::Copy {
            sources: vec![uri(tree.join(&raw))],
            dest_dir: uri(&tree),
        },
        t.path(),
    );
    let mut copied = raw;
    copied.push(" (copy)");
    assert_eq!(fs::read(tree.join(copied)).unwrap(), b"raw");
}
#[test]
fn pause_resume_and_retry_recover_job() {
    use std::time::Duration;
    let t = temp();
    let src = t.path().join("source");
    let dst = t.path().join("dest");
    fs::write(&src, b"data").unwrap();
    fs::create_dir(&dst).unwrap();
    let h = spawn_with_options(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
        JobOptions {
            start_paused: true,
            ..Default::default()
        },
    );
    std::thread::sleep(Duration::from_millis(25));
    assert!(!dst.join("source").exists());
    h.resume();
    let s = finish(&h, |q| panic!("{q:?}"));
    assert!(s.errors.is_empty());
    fs::remove_file(dst.join("source")).unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o555)).unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let s = finish(&h, |q| {
        assert!(matches!(
            q,
            Question::Error {
                retryable: true,
                ..
            }
        ));
        fs::set_permissions(&dst, fs::Permissions::from_mode(0o700)).unwrap();
        Answer::Retry
    });
    assert!(s.errors.is_empty());
    assert_eq!(fs::read(dst.join("source")).unwrap(), b"data");
}
#[test]
fn copying_a_folder_keeps_its_mode_and_time_without_a_warning() {
    use std::os::unix::fs::PermissionsExt;
    let t = temp();
    let src = t.path().join("folder");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::write(src.join("file"), b"x").unwrap();
    fs::set_permissions(&src, fs::Permissions::from_mode(0o750)).unwrap();
    let stamp = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_500_000_000);
    fs::File::open(&src).unwrap().set_modified(stamp).unwrap();
    let s = success(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        t.path(),
    );
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
    let copied = fs::metadata(dst.join("folder")).unwrap();
    assert_eq!(copied.permissions().mode() & 0o777, 0o750);
    assert_eq!(copied.modified().unwrap(), stamp);
}
