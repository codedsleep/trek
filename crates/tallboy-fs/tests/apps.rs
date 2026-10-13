use gio::prelude::*;
use std::{fs, path::PathBuf, process::Command, time::Duration};
use tallboy_fs::{apps, terminal};
#[test]
fn isolated_app_launches() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "apps_fixture_subprocess", "--nocapture"])
        .env("TALLBOY_APPS_CHILD", t.path())
        .env("XDG_DATA_HOME", t.path().join("data"))
        .env("XDG_CONFIG_HOME", t.path().join("config"))
        .env("XDG_CACHE_HOME", t.path().join("cache"))
        .env("XDG_DATA_DIRS", t.path().join("empty"))
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn apps_fixture_subprocess() {
    let Some(root) = std::env::var_os("TALLBOY_APPS_CHILD") else {
        return;
    };
    let root = PathBuf::from(root);
    let data = root.join("data/applications");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(root.join("config")).unwrap();
    let recorder = root.join("record.py");
    let output = root.join("launches.jsonl");
    fs::write(&recorder,"import json,sys\nwith open(sys.argv[1], 'a') as f: f.write(json.dumps(sys.argv[2:])+'\\n')\n").unwrap();
    for (id, mime) in [
        ("alpha", "application/x-tallboy-alpha"),
        ("beta", "application/x-tallboy-beta"),
    ] {
        fs::write(data.join(format!("{id}.desktop")),format!("[Desktop Entry]\nType=Application\nName={id}\nExec=/usr/bin/python3 {} {} %F\nMimeType={mime};\n",recorder.display(),output.display())).unwrap();
        let app =
            gio_unix::DesktopAppInfo::from_filename(data.join(format!("{id}.desktop"))).unwrap();
        apps::set_default(app.upcast_ref(), mime).unwrap();
    }
    let names = ["a", "; rm -rf ~", "b"];
    for name in names {
        fs::write(root.join(name), b"x").unwrap();
    }
    let files = vec![
        (
            gio::File::for_path(root.join(names[0])),
            "application/x-tallboy-alpha".into(),
        ),
        (
            gio::File::for_path(root.join(names[1])),
            "application/x-tallboy-alpha".into(),
        ),
        (
            gio::File::for_path(root.join(names[2])),
            "application/x-tallboy-beta".into(),
        ),
    ];
    let groups = apps::group(&files, apps::default).unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].files.len(), 2);
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            apps::launch(&groups, Some(&gio::AppLaunchContext::new()))
                .await
                .unwrap();
            for _ in 0..100 {
                if fs::read_to_string(&output).is_ok_and(|s| s.lines().count() == 2) {
                    break;
                }
                glib::timeout_future(Duration::from_millis(20)).await;
            }
        })
    })
    .unwrap();
    let text = fs::read_to_string(output).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(text.contains("; rm -rf ~"));
    assert!(apps::custom("/usr/bin/python3 --version", "Python", false).is_ok());
    assert!(apps::default("application/x-tallboy-none", false).is_none());
}
#[test]
fn terminal_argv_has_no_shell_interpolation() {
    let entries = terminal::parse_list(terminal::DEFAULT_LIST).unwrap();
    assert_eq!(entries.len(), 11);
    let terminal =
        terminal::resolve("kitty --single-instance", Some("foot"), &entries, |_| true).unwrap();
    let arg = std::ffi::OsString::from("; rm -rf ~");
    let argv = terminal
        .command_argv(&["printf".into(), arg.clone()], true)
        .unwrap();
    assert_eq!(argv.last(), Some(&arg));
    assert!(!argv.iter().any(|a| a == "sh"));
    assert_eq!(
        terminal::resolve("", Some("foot"), &entries, |_| true)
            .unwrap()
            .argv[0],
        "foot"
    );
    assert_eq!(
        terminal::resolve("", None, &entries, |p| p == "xterm")
            .unwrap()
            .argv[0],
        "xterm"
    );
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    let output = t.path().join("argument");
    let argv = vec![
        "/usr/bin/python3".into(),
        "-c".into(),
        "import sys;open(sys.argv[1],'w').write(sys.argv[2])".into(),
        output.clone().into_os_string(),
        arg,
    ];
    let mut child = terminal::spawn(&argv, &gio::File::for_path(t.path())).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(fs::read_to_string(output).unwrap(), "; rm -rf ~");
}
