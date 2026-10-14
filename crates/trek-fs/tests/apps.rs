use gio::prelude::*;
use std::{fs, path::PathBuf, process::Command, time::Duration};
use trek_fs::{apps, terminal};
#[test]
fn isolated_app_launches() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "apps_fixture_subprocess", "--nocapture"])
        .env("TREK_APPS_CHILD", t.path())
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
    let Some(root) = std::env::var_os("TREK_APPS_CHILD") else {
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
        ("alpha", "application/x-trek-alpha"),
        ("beta", "application/x-trek-beta"),
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
            "application/x-trek-alpha".into(),
        ),
        (
            gio::File::for_path(root.join(names[1])),
            "application/x-trek-alpha".into(),
        ),
        (
            gio::File::for_path(root.join(names[2])),
            "application/x-trek-beta".into(),
        ),
    ];
    let groups = apps::group(&files, apps::default).unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].files.len(), 2);
    let ctx = glib::MainContext::new();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let none = || Err(terminal::TerminalError::Missing);
            apps::launch(&groups, Some(&gio::AppLaunchContext::new()), none)
                .await
                .unwrap();
            // An entry with Terminal=true runs in the terminal it is given,
            // which GIO's own list of terminals would never find.
            fs::write(
                data.join("gamma.desktop"),
                "[Desktop Entry]\nType=Application\nName=gamma\nExec=/usr/bin/true %F\nTerminal=true\n",
            )
            .unwrap();
            let gamma =
                gio_unix::DesktopAppInfo::from_filename(data.join("gamma.desktop")).unwrap();
            let in_terminal = vec![apps::LaunchGroup {
                app: gamma.upcast(),
                files: vec![gio::File::for_path(root.join(names[1]))],
            }];
            assert!(matches!(
                apps::launch(&in_terminal, None, none).await,
                Err(apps::AppError::Terminal(terminal::TerminalError::Missing))
            ));
            let recording = terminal::Terminal {
                argv: vec![
                    "/usr/bin/python3".into(),
                    recorder.clone().into(),
                    output.clone().into(),
                    "100%".into(),
                ],
                exec_args: vec!["-e".into()],
                hold_args: vec![],
            };
            apps::launch(&in_terminal, None, || Ok(recording.clone()))
                .await
                .unwrap();
            for _ in 0..100 {
                if fs::read_to_string(&output).is_ok_and(|s| s.lines().count() == 3) {
                    break;
                }
                glib::timeout_future(Duration::from_millis(20)).await;
            }
        })
    })
    .unwrap();
    let text = fs::read_to_string(output).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("; rm -rf ~"));
    let wrapped = format!(
        r#"["100%", "-e", "/usr/bin/true", "{}"]"#,
        root.join(names[1]).display()
    );
    assert!(text.lines().any(|line| line == wrapped), "{text}");
    assert!(apps::custom("/usr/bin/python3 --version", "Python", false).is_ok());
    assert!(apps::default("application/x-trek-none", false).is_none());
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
    let editor = |visual, editor| terminal::editor(visual, editor, |p| p != "missing");
    assert_eq!(editor(None, None).unwrap(), ["nvim"]);
    assert_eq!(editor(Some(" "), Some("hx")).unwrap(), ["hx"]);
    assert_eq!(
        editor(Some("code --wait"), Some("hx")).unwrap(),
        ["code", "--wait"]
    );
    assert!(matches!(
        editor(None, Some("missing -p")),
        Err(terminal::TerminalError::NoEditor(program)) if program == "missing"
    ));
    let host = terminal::host_environment(
        [
            ("TREK_HOST_XDG_CONFIG_HOME", "/home/owner/.config"),
            ("TREK_HOST_XDG_DATA_HOME", ""),
            ("XDG_CACHE_HOME", "/isolated/cache"),
        ]
        .map(|(name, value)| (name.into(), value.into())),
    );
    assert_eq!(
        host,
        [
            ("XDG_CONFIG_HOME".into(), Some("/home/owner/.config".into())),
            ("TREK_HOST_XDG_CONFIG_HOME".into(), None),
            ("XDG_DATA_HOME".into(), None),
            ("TREK_HOST_XDG_DATA_HOME".into(), None),
        ]
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
