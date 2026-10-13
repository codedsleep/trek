mod common;
use common::*;
use std::{fs, path::Path};
use tallboy_fs::{guard::check, jobs::*};
#[test]
fn native_roots_symlink_parents_and_uri_boundaries() {
    let t = temp();
    let allowed = t.path().join("allowed");
    let outside = t.path().join("outside");
    fs::create_dir(&allowed).unwrap();
    fs::create_dir(&outside).unwrap();
    let mut p = policy(&allowed);
    assert!(check(&gio::File::for_path(allowed.join("new")), &p).is_ok());
    assert!(check(&gio::File::for_path(outside.join("new")), &p).is_err());
    std::os::unix::fs::symlink(&outside, allowed.join("escape")).unwrap();
    assert!(check(&gio::File::for_path(allowed.join("escape/new")), &p).is_err());
    assert!(check(&gio::File::for_path(allowed.join("escape")), &p).is_ok());
    assert!(check(&gio::File::for_uri("trash:///item"), &p).is_ok());
    p.data_home = outside;
    assert!(check(&gio::File::for_uri("trash:///item"), &p).is_err());
    p.uri_roots.push("smb://host/scratch".into());
    assert!(check(&gio::File::for_uri("smb://host/scratch/file"), &p).is_ok());
    assert!(check(&gio::File::for_uri("smb://host/scratch-other/file"), &p).is_err());
    p.unlocked = true;
    assert!(check(&gio::File::for_path("/outside"), &p).is_ok());
    assert!(check(&gio::File::for_uri("admin:///etc"), &p).is_err());
}
#[test]
fn every_mutating_entry_point_uses_guard() {
    let t = temp();
    let p = policy(t.path());
    let outside = "file:///outside-tallboy-guard".to_owned();
    let kinds = vec![
        JobKind::Copy {
            sources: vec![outside.clone()],
            dest_dir: uri(t.path()),
        },
        JobKind::Move {
            sources: vec![outside.clone()],
            dest_dir: uri(t.path()),
        },
        JobKind::Link {
            sources: vec![outside.clone()],
            dest_dir: uri(t.path()),
        },
        JobKind::Delete {
            files: vec![outside.clone()],
        },
        JobKind::Trash {
            files: vec![outside.clone()],
        },
        JobKind::Restore {
            files: vec![outside.clone()],
        },
        JobKind::SetAttrs {
            files: vec![outside.clone()],
            change: Default::default(),
            recursive: true,
        },
    ];
    for kind in kinds {
        let h = spawn_with_policy(kind, p.clone());
        let s = finish(&h, |q| panic!("Guard must fail before any question: {q:?}"));
        assert!(
            s.errors.iter().any(|e| e.contains("locked outside")),
            "{s:?}"
        );
        assert_eq!(s.done_files, 0);
    }
    let mut p = p;
    p.data_home = Path::new("/not-allowed").into();
    let h = spawn_with_policy(JobKind::EmptyTrash, p.clone());
    assert!(
        finish(&h, |_| Answer::Cancel)
            .errors
            .iter()
            .any(|e| e.contains("locked outside"))
    );
    let file = gio::File::for_uri(&outside);
    assert!(rename(&file, "x", &p).is_err());
    assert!(create_file(&file, "x", &p).is_err());
    assert!(create_folder(&file, "x", &p).is_err());
}
