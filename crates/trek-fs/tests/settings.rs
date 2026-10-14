use std::{fs, path::PathBuf};
use trek_core::config::{Config, ViewMode};
use trek_fs::{
    dir_settings::{DirectoryStore, FolderSettings},
    settings_store::Store,
};
fn temp() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
#[test]
fn layered_roundtrip_unknown_and_profiles() {
    let t = temp();
    let user = t.path().join("user");
    let high = t.path().join("high");
    let low = t.path().join("low");
    for (p, text) in [
        (&low, "[ui]\nwin_width=700\nwin_height=900\n"),
        (&high, "[ui]\nwin_width=800\n"),
        (
            &user,
            "[config]\nterminal=kitty\\n--hold\n[unknown]\nkey=keep\n[ui]\nwin_width=1000\n",
        ),
    ] {
        let p = p.join("trek/default");
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("trek.conf"), text).unwrap();
    }
    let dirs = vec![high, low];
    let a = Store::load(&user, &dirs, "default").unwrap();
    assert_eq!(a.config.win_width, 1000);
    assert_eq!(a.config.win_height, 900);
    assert_eq!(a.config.terminal, "kitty\n--hold");
    a.save().unwrap();
    let b = Store::load(&user, &dirs, "default").unwrap();
    assert_eq!(a.config, b.config);
    assert_eq!(b.unknown("unknown", "key").as_deref(), Some("keep"));
    assert_eq!(
        Store::load(&user, &[], "agent").unwrap().config,
        Config::default()
    );
    assert!(Store::load(&user, &[], "../escape").is_err());
}
#[test]
fn corrupt_and_symlink() {
    let t = temp();
    let dir = t.path().join("trek/default");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trek.conf");
    fs::write(&path, "[broken").unwrap();
    let s = Store::load(t.path(), &[], "default").unwrap();
    assert_eq!(s.config, Config::default());
    assert!(dir.join("trek.conf.broken").exists());
    assert_eq!(s.warnings.len(), 1);
    let target = t.path().join("target");
    fs::write(&target, "[ui]\nwin_width=123\n").unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    let s = Store::load(t.path(), &[], "default").unwrap();
    s.save().unwrap();
    assert!(
        !fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        "[ui]\nwin_width=123\n"
    );
}
#[test]
fn per_folder_path_and_uri() {
    let t = temp();
    let path = t.path().join("dir-settings.conf");
    let s = DirectoryStore::load(&path).unwrap();
    let c = Config::default();
    let mut value = FolderSettings::from(&c);
    value.view_mode = ViewMode::List;
    for group in ["/tmp/a", "sftp://host/share"] {
        s.set(group, &value).unwrap();
        assert_eq!(s.get(group, &c).unwrap(), Some(value.clone()));
    }
    let s = DirectoryStore::load(&path).unwrap();
    assert_eq!(s.get("sftp://host/share", &c).unwrap(), Some(value));
    s.remove("/tmp/a").unwrap();
    assert_eq!(s.get("/tmp/a", &c).unwrap(), None);
}
