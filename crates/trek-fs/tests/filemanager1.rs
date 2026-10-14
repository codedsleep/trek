use glib::variant::ToVariant;
use std::process::Command;
use trek_fs::filemanager1::{self, Error, Event, NAME, PATH};
#[test]
fn private_bus_gdbus_requests_and_existing_owner() {
    let status = Command::new("timeout")
        .args(["--kill-after=2s", "20s", "dbus-run-session", "--"])
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "filemanager1_subprocess", "--nocapture"])
        .env("TREK_FILEMANAGER1_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success());
}
#[test]
fn filemanager1_subprocess() {
    if std::env::var("TREK_FILEMANAGER1_CHILD").as_deref() != Ok("1") {
        return;
    }
    let ctx = glib::MainContext::default();
    ctx.with_thread_default(|| {
        ctx.block_on(async {
            let connection = gio::bus_get_future(gio::BusType::Session).await.unwrap();
            let service = filemanager1::register(&connection).await.unwrap();
            assert!(matches!(
                filemanager1::register(&connection).await,
                Err(Error::AlreadyOwned)
            ));
            let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
            let other = gio::DBusConnection::for_address_future(
                &address,
                gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                    | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                None::<&gio::DBusAuthObserver>,
            )
            .await
            .unwrap();
            assert!(matches!(
                filemanager1::register(&other).await,
                Err(Error::AlreadyOwned)
            ));
            for method in ["ShowFolders", "ShowItems", "ShowItemProperties"] {
                let command = method.to_owned();
                let thread = std::thread::spawn(move || {
                    Command::new("gdbus")
                        .args([
                            "call",
                            "--session",
                            "--dest",
                            NAME,
                            "--object-path",
                            PATH,
                            "--method",
                        ])
                        .arg(format!("{NAME}.{command}"))
                        .args([
                            "['file:///tmp/a%0Ab', 'sftp://host/folder']",
                            "startup-token",
                        ])
                        .output()
                        .unwrap()
                });
                let event = service.events.recv().await.unwrap();
                let (locations, startup_id) = match (method, event) {
                    (
                        "ShowFolders",
                        Event::ShowFolders {
                            locations,
                            startup_id,
                        },
                    )
                    | (
                        "ShowItems",
                        Event::ShowItems {
                            locations,
                            startup_id,
                        },
                    )
                    | (
                        "ShowItemProperties",
                        Event::ShowItemProperties {
                            locations,
                            startup_id,
                        },
                    ) => (locations, startup_id),
                    value => panic!("Unexpected event: {value:?}"),
                };
                assert_eq!(locations.len(), 2);
                assert_eq!(
                    locations[0].path().unwrap(),
                    std::path::Path::new("/tmp/a\nb")
                );
                assert_eq!(locations[1].to_uri(), "sftp://host/folder");
                assert_eq!(startup_id, "startup-token");
                let output = thread.join().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let invalid = other
                .call_future(
                    Some(NAME),
                    PATH,
                    NAME,
                    "ShowItems",
                    Some(&(vec!["admin:///etc"], "").to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                )
                .await;
            assert!(invalid.is_err());
            assert!(service.events.try_recv().is_err());
            drop(service);
            // Calls from the original connection are ordered after ReleaseName.
            let reply = connection
                .call_future(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    Some(&(NAME,).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    5000,
                )
                .await
                .unwrap();
            assert_eq!(reply.get::<(bool,)>(), Some((false,)));
            let next = filemanager1::register(&other).await.unwrap();
            drop(next);
        })
    })
    .unwrap();
}
