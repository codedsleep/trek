use trek_core::{config::Config, location::Location};
use trek_fs::volumes::{automount, *};
fn entry() -> Entry {
    Entry {
        id: "usb".into(),
        name: "USB".into(),
        icon: None,
        root: None,
        device: None,
        mounted: false,
        removable: true,
        internal: false,
        can_eject: true,
        can_mount: true,
        should_automount: true,
    }
}
#[test]
fn filters_and_automount() {
    let mut e = entry();
    let mut c = Config::default();
    assert!(visible(&e, Filters::default()));
    assert!(!visible(
        &e,
        Filters {
            places_unmounted: false,
            ..Default::default()
        }
    ));
    assert_eq!(automount::plan(&[e.clone()], &c, false), vec!["usb"]);
    c.mount_removable = false;
    assert!(automount::plan(&[e.clone()], &c, false).is_empty());
    assert_eq!(automount::plan(&[e.clone()], &c, true), vec!["usb"]);
    e.internal = true;
    assert!(!visible(&e, Filters::default()));
    assert!(visible(
        &e,
        Filters {
            show_internal_volumes: true,
            ..Default::default()
        }
    ));
    assert!(automount::plan(&[e], &c, true).is_empty());
}
#[test]
fn mount_locations_and_busy_error() {
    let root = gio::File::for_uri("file:///mnt/usb");
    let locations = [
        "file:///mnt/usb",
        "file:///mnt/usb/folder",
        "file:///mnt/usb-other",
        "smb://host/usb",
    ]
    .map(|s| Location::Uri(s.into()));
    assert_eq!(locations_inside(&root, &locations), vec![0, 1]);
    let e = Error::from(glib::Error::new(gio::IOErrorEnum::Busy, "Device is busy"));
    assert!(e.0.matches(gio::IOErrorEnum::Busy));
    assert!(e.to_string().contains("busy"));
}

#[test]
fn automount_follows_the_backend_hint() {
    let c = Config::default();
    // A disk image the user attached: no drive, so neither removable nor
    // internal, and udisks asks for it to be mounted.
    let image = Entry {
        removable: false,
        can_eject: false,
        ..entry()
    };
    assert_eq!(
        automount::plan(std::slice::from_ref(&image), &c, false),
        vec!["usb"]
    );
    let held_back = Entry {
        should_automount: false,
        ..image
    };
    assert!(automount::plan(&[held_back], &c, false).is_empty());
}
