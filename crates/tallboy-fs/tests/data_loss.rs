mod common;
use common::*;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};
use tallboy_fs::jobs::*;
fn large_source(root: &std::path::Path) -> std::path::PathBuf {
    let src = root.join("large");
    let mut f = fs::File::create(&src).unwrap();
    let bytes = vec![0x5a; 1024 * 1024];
    for _ in 0..128 {
        f.write_all(&bytes).unwrap();
    }
    src
}
#[test]
fn unreadable_directory() {
    let t = temp();
    let src = t.path().join("unreadable");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    fs::set_permissions(&src, fs::Permissions::from_mode(0o0)).unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let s = finish(&h, |_| Answer::Skip { apply_to_all: true });
    fs::set_permissions(&src, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!s.errors.is_empty());
    assert!(fs::read_dir(&dst).unwrap().next().is_none());
}
#[test]
fn read_only_destination() {
    let t = temp();
    let src = t.path().join("source");
    let dst = t.path().join("dest");
    fs::write(&src, b"important").unwrap();
    fs::create_dir(&dst).unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o555)).unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let s = finish(&h, |_| Answer::Skip { apply_to_all: true });
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!s.errors.is_empty());
    assert_eq!(fs::read(src).unwrap(), b"important");
}
#[test]
fn newline_in_name() {
    let t = temp();
    let src = t.path().join("line\nbreak");
    let dst = t.path().join("dest");
    fs::write(&src, b"content").unwrap();
    fs::create_dir(&dst).unwrap();
    success(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        t.path(),
    );
    assert_eq!(fs::read(dst.join("line\nbreak")).unwrap(), b"content");
}
#[test]
fn destination_disappears_mid_job() {
    let source = tempfile::tempdir_in("/tmp").unwrap();
    let src = large_source(source.path());
    let t = temp();
    let dst = t.path().join("dest");
    fs::create_dir(&dst).unwrap();
    let mut p = policy(t.path());
    p.roots.push(source.path().into());
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        p,
    );
    let mut removed = false;
    let s = drive(&h, |event| {
        match event {
            JobEvent::Progress { .. } if !removed => {
                fs::remove_dir_all(&dst).unwrap();
                removed = true;
            }
            JobEvent::Ask(_, reply) => reply.answer(Answer::Skip { apply_to_all: true }).unwrap(),
            _ => {}
        }
        None
    });
    assert!(removed);
    assert!(!s.errors.is_empty());
    assert_eq!(fs::metadata(src).unwrap().len(), 128 * 1024 * 1024);
}
#[test]
fn pause_then_cancel() {
    let t = temp();
    let src = t.path().join("source");
    let dst = t.path().join("dest");
    fs::write(&src, b"important").unwrap();
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
    h.pause();
    let start = Instant::now();
    h.cancel();
    let s = finish(&h, |_| Answer::Cancel);
    assert!(s.cancelled);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(fs::read_dir(dst).unwrap().next().is_none());
}
#[test]
fn cancel_mid_file_preserves_existing_destination() {
    let source = tempfile::tempdir_in("/tmp").unwrap();
    let src = large_source(source.path());
    let t = temp();
    let dst = t.path().join("dest");
    fs::create_dir(&dst).unwrap();
    fs::write(dst.join("large"), b"old").unwrap();
    let mut p = policy(t.path());
    p.roots.push(source.path().into());
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        p,
    );
    let mut cancelled = false;
    let s = drive(&h, |event| {
        match event {
            JobEvent::Ask(_, reply) => reply
                .answer(Answer::Overwrite { apply_to_all: true })
                .unwrap(),
            JobEvent::Progress { .. } if !cancelled => {
                h.pause();
                h.cancel();
                cancelled = true;
            }
            _ => {}
        }
        None
    });
    assert!(cancelled);
    assert!(s.cancelled);
    assert_eq!(fs::read(dst.join("large")).unwrap(), b"old");
    assert_eq!(fs::read_dir(dst).unwrap().count(), 1);
}
#[test]
fn two_concurrent_jobs_on_same_tree() {
    let t = temp();
    let src = t.path().join("source");
    let dst = t.path().join("dest");
    fs::create_dir(&src).unwrap();
    fs::create_dir(&dst).unwrap();
    for n in 0..50 {
        fs::write(src.join(format!("f{n}")), format!("content{n}")).unwrap();
    }
    let job = JobKind::Copy {
        sources: vec![uri(&src)],
        dest_dir: uri(&dst),
    };
    let a = spawn_with_policy(job.clone(), policy(t.path()));
    let b = spawn_with_policy(job, policy(t.path()));
    for h in [&a, &b] {
        let s = finish(h, |_| Answer::Overwrite { apply_to_all: true });
        assert!(s.errors.is_empty(), "{s:?}");
    }
    for n in 0..50 {
        assert_eq!(
            fs::read_to_string(dst.join(format!("source/f{n}"))).unwrap(),
            format!("content{n}")
        );
    }
    assert_eq!(fs::read_dir(dst.join("source")).unwrap().count(), 50);
}
#[test]
fn real_cross_filesystem_move_and_injected_copy_failure() {
    use std::os::unix::fs::MetadataExt;
    for fail in [false, true] {
        let source = tempfile::tempdir_in("/tmp").unwrap();
        let t = temp();
        assert_ne!(
            fs::metadata(source.path()).unwrap().dev(),
            fs::metadata(t.path()).unwrap().dev()
        );
        let src = source.path().join("source");
        fs::write(&src, b"important").unwrap();
        if fail {
            fs::set_permissions(&src, fs::Permissions::from_mode(0o0)).unwrap();
        }
        let mut p = policy(t.path());
        p.roots.push(source.path().into());
        let h = spawn_with_policy(
            JobKind::Move {
                sources: vec![uri(&src)],
                dest_dir: uri(t.path()),
            },
            p,
        );
        let s = finish(&h, |_| Answer::Skip { apply_to_all: true });
        if fail {
            fs::set_permissions(&src, fs::Permissions::from_mode(0o600)).unwrap();
            assert!(!s.errors.is_empty());
            assert_eq!(fs::read(&src).unwrap(), b"important");
            assert!(!t.path().join("source").exists());
        } else {
            assert!(s.errors.is_empty(), "{s:?}");
            assert!(!src.exists());
            assert_eq!(fs::read(t.path().join("source")).unwrap(), b"important");
        }
    }
}

#[test]
fn overwrite_cannot_delete_a_folder_created_after_the_conflict_question() {
    let t = temp();
    let src = t.path().join("source");
    let dst = t.path().join("dest");
    fs::write(&src, b"new content").unwrap();
    fs::create_dir(&dst).unwrap();
    let target = dst.join("source");
    fs::write(&target, b"old content").unwrap();
    let h = spawn_with_policy(
        JobKind::Copy {
            sources: vec![uri(&src)],
            dest_dir: uri(&dst),
        },
        policy(t.path()),
    );
    let mut replaced = false;
    let summary = finish(&h, |question| match question {
        Question::Conflict { can_overwrite, .. } => {
            assert!(can_overwrite);
            assert!(!replaced);
            fs::remove_file(&target).unwrap();
            fs::create_dir(&target).unwrap();
            fs::write(target.join("keep"), b"irreplaceable").unwrap();
            replaced = true;
            Answer::Overwrite { apply_to_all: true }
        }
        Question::Error { .. } => Answer::Skip { apply_to_all: true },
        other => panic!("unexpected question: {other:?}"),
    });
    assert!(replaced);
    assert!(!summary.errors.is_empty());
    assert_eq!(fs::read(&src).unwrap(), b"new content");
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"irreplaceable");
    assert_eq!(fs::read_dir(&dst).unwrap().count(), 1);
}
