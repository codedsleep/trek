//! Command-line handling around `core::invocation`: the profile pre-scan that
//! must happen before the application ID exists, and request execution.

use crate::{app::App, open::Opener, view::FileItem};
use gtk::{gio, glib, prelude::*};
use std::ffi::OsString;
use tallboy_core::{invocation::Request, location::Location};
use tallboy_fs::meta::{self, FileMeta};

/// Finds `-p`/`--profile` before GApplication is constructed, because the
/// profile is part of the application ID (§8.27).
pub fn prescan_profile(args: &[OsString]) -> Result<String, String> {
    let mut profile = String::from("default");
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let Some(arg) = arg.to_str() else { continue };
        if arg == "--" {
            break;
        }
        let value = if arg == "-p" || arg == "--profile" {
            iter.next().and_then(|v| v.to_str()).map(String::from)
        } else if let Some(value) = arg.strip_prefix("--profile=") {
            Some(value.to_owned())
        } else if let Some(value) = arg.strip_prefix("-p=") {
            Some(value.to_owned())
        } else {
            continue;
        };
        profile = value.ok_or("--profile requires a value")?;
    }
    let valid = !profile.is_empty()
        && profile
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    if valid {
        Ok(profile)
    } else {
        Err("Profile must contain letters, digits, '_' or '-'.".into())
    }
}

/// A smoke run must never touch the owner's configuration, trash or
/// thumbnail cache (DISCOVERY D14), so it refuses to start without the
/// isolated XDG directories.
pub fn check_smoke_isolation() -> Result<(), String> {
    if std::env::var_os("TALLBOY_SMOKE").is_none_or(|v| v != "1") {
        return Ok(());
    }
    for name in ["XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME"] {
        if std::env::var_os(name).is_none_or(|v| v.is_empty()) {
            return Err(format!("TALLBOY_SMOKE needs an isolated {name}"));
        }
    }
    Ok(())
}

/// Carries out the requests of a first or forwarded invocation, in order.
///
/// A path may name a folder or a file, and finding out can mean asking a
/// slow filesystem, so the requests run in one asynchronous task. The
/// application is held open meanwhile: `tallboy file.txt` with no window
/// open must live long enough to start the file's application.
pub fn execute(app: &App, requests: Vec<Request>) {
    let hold = app.gtk().hold();
    let app = app.clone();
    glib::spawn_future_local(async move {
        for request in requests {
            match request {
                Request::Open {
                    location,
                    new_window,
                } => open(&app, location, new_window).await,
                Request::Preferences(page) => {
                    let parent = app.active_window().map(|window| window.widget().clone());
                    crate::dialogs::prefs::show(
                        &app,
                        parent.as_ref().map(|window| window.upcast_ref()),
                        page,
                    );
                }
                // Done before this point, where the caller can be answered.
                Request::Import => {}
            }
        }
        drop(hold);
    });
}

/// Folders open as tabs in the last active window, or in a new window with
/// `-n` or when there is none; files are opened with their applications.
async fn open(app: &App, location: Location, new_window: bool) {
    let file = gio::File::for_uri(location.to_uri());
    let info = file
        .query_info_future(
            &meta::attributes(app.settings().defer_content_test()),
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await;
    match info {
        Ok(info) if info.file_type() != gio::FileType::Directory => {
            let meta = FileMeta::from_info(&file, &info, app.settings().backup_as_hidden(), None);
            Opener::new(app, app.active_window().as_ref()).open(vec![FileItem::new(meta)]);
        }
        // A folder — or something that cannot be queried yet, such as an
        // unmounted location: the tab mounts it or shows the error.
        _ => app.open_folder(location, new_window),
    }
}

/// `--import-pcmanfm` (§8.24): reads PCManFM's and libfm's settings, takes
/// over the keys tallboy has, saves them to this profile and reports on the
/// caller's terminal. Never run automatically; the source files are only
/// read. Returns the exit status.
pub fn import(app: &App, caller: &gio::ApplicationCommandLine) -> u8 {
    let config = glib::user_config_dir();
    let profile = app.settings().profile();
    // PCManFM keeps one folder per profile; ours may have no namesake there.
    let pcmanfm = [profile.as_str(), "default"]
        .iter()
        .map(|name| config.join("pcmanfm").join(name).join("pcmanfm.conf"))
        .find(|path| path.is_file());
    let libfm = Some(config.join("libfm").join("libfm.conf")).filter(|path| path.is_file());
    if pcmanfm.is_none() && libfm.is_none() {
        caller.printerr_literal(&format!(
            "tallboy: nothing to import: no PCManFM or libfm settings under {}\n",
            config.display()
        ));
        return 1;
    }
    // The importer wants both files; an absent one is read as empty.
    let nothing = std::path::Path::new("/dev/null");
    let report = match tallboy_fs::import::read(
        pcmanfm.as_deref().unwrap_or(nothing),
        libfm.as_deref().unwrap_or(nothing),
    ) {
        Ok(report) => report,
        Err(e) => {
            caller.printerr_literal(&format!("tallboy: {e}\n"));
            return 1;
        }
    };
    let keys: Vec<(String, String, String)> = report
        .imported
        .iter()
        .map(|item| (item.group.clone(), item.key.clone(), item.value.clone()))
        .collect();
    app.settings().import(&keys);

    let mut out = String::new();
    for path in [&pcmanfm, &libfm].into_iter().flatten() {
        out.push_str(&format!("Read {}\n", path.display()));
    }
    let line = |item: &tallboy_fs::import::Key| {
        format!(
            "  [{}] {}={}  ({})\n",
            item.group, item.key, item.value, item.source
        )
    };
    out.push_str(&format!(
        "\nImported into profile \"{profile}\" ({}):\n",
        report.imported.len()
    ));
    report
        .imported
        .iter()
        .for_each(|item| out.push_str(&line(item)));
    if !report.ignored.is_empty() {
        out.push_str(&format!(
            "\nNot imported, tallboy has no such setting ({}):\n",
            report.ignored.len()
        ));
        report
            .ignored
            .iter()
            .for_each(|item| out.push_str(&line(item)));
    }
    if !report.warnings.is_empty() {
        out.push_str("\nPlease note:\n");
        for warning in &report.warnings {
            out.push_str(&format!("  {warning}\n"));
        }
    }
    caller.print_literal(&out);
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(args: &[&str]) -> Result<String, String> {
        prescan_profile(&args.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn profile_prescan() {
        assert_eq!(scan(&[]).as_deref(), Ok("default"));
        assert_eq!(scan(&["-p", "work"]).as_deref(), Ok("work"));
        assert_eq!(scan(&["--profile=a-b", "x"]).as_deref(), Ok("a-b"));
        assert_eq!(scan(&["--", "-p", "x"]).as_deref(), Ok("default"));
        assert!(scan(&["--profile"]).is_err());
        assert!(scan(&["-p", "a b"]).is_err());
    }
}
