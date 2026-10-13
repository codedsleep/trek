mod common;
use common::*;
use std::fs;
#[test]
fn owners_config_copies_and_explicit_quick_exec_advisory() {
    let t = temp();
    let pc = t.path().join("pcmanfm.conf");
    let lib = t.path().join("libfm.conf");
    fs::write(&pc, include_str!("fixtures/pcmanfm.conf")).unwrap();
    fs::write(&lib, include_str!("fixtures/libfm.conf")).unwrap();
    let report = tallboy_fs::import::read(&pc, &lib).unwrap();
    assert_eq!(report.config.terminal, "kitty");
    assert!(report.config.show_hidden);
    for key in [
        "middle_click",
        "force_startup_notify",
        "date_iso_8601",
        "smart_desktop_autodrop",
    ] {
        assert!(report.ignored.iter().any(|i| i.key == key));
    }
    let original = fs::read_to_string(&lib).unwrap();
    fs::write(&lib, original.replace("quick_exec=0", "quick_exec=1")).unwrap();
    let report = tallboy_fs::import::read(&pc, &lib).unwrap();
    assert!(report.config.quick_exec);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Imported quick_exec=1"))
    );
    assert_eq!(
        fs::read_to_string(&pc).unwrap(),
        include_str!("fixtures/pcmanfm.conf")
    );
}
