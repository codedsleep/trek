use trek_core::{
    config::{AutorunChoice, Config},
    location::Location,
};
use trek_fs::volumes::{autorun, *};
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
fn filters_automount_and_autorun() {
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
    assert_eq!(
        autorun::automount_plan(&[e.clone()], &c, false),
        vec!["usb"]
    );
    c.mount_removable = false;
    assert!(autorun::automount_plan(&[e.clone()], &c, false).is_empty());
    assert_eq!(autorun::automount_plan(&[e.clone()], &c, true), vec!["usb"]);
    e.internal = true;
    assert!(!visible(&e, Filters::default()));
    assert!(visible(
        &e,
        Filters {
            show_internal_volumes: true,
            ..Default::default()
        }
    ));
    assert!(autorun::automount_plan(&[e], &c, true).is_empty());
    let choice = AutorunChoice {
        app: "run.desktop".into(),
        remembered: true,
    };
    autorun::remember(&mut c, "x-content/unix-software", choice.clone());
    assert!(
        !autorun::choice(&c, "x-content/unix-software")
            .unwrap()
            .remembered
    );
    autorun::remember(&mut c, "x-content/image-dcf", choice);
    assert!(
        autorun::choice(&c, "x-content/image-dcf")
            .unwrap()
            .remembered
    );
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
