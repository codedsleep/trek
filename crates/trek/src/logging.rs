//! `log` → GLib bridge and the crash log (DISCOVERY D23).

use std::{fs, io::Write, path::PathBuf};

static LOGGER: glib::GlibLogger = glib::GlibLogger::new(
    glib::GlibLoggerFormat::Plain,
    glib::GlibLoggerDomain::CrateTarget,
);

/// Routes `log` records through GLib and installs the crash-log panic hook.
///
/// `log::error!` becomes a GLib critical, which is fatal under
/// `G_DEBUG=fatal-criticals`; runtime failures the user can cause are
/// therefore logged with `warn!`.
pub fn init() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Written first: a keybind-launched process has no visible stderr,
        // and the release profile aborts on panic.
        write_crash(&info.to_string());
        default(info);
    }));
}

/// `$XDG_STATE_HOME/trek/crash.log`.
pub fn crash_log_path() -> PathBuf {
    glib::user_state_dir().join("trek").join("crash.log")
}

fn write_crash(text: &str) {
    let path = crash_log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let stamp = glib::DateTime::now_local()
        .and_then(|t| t.format_iso8601())
        .map_or_else(|_| String::from("unknown time"), |s| s.to_string());
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "[{stamp}] trek {}: {text}", env!("CARGO_PKG_VERSION"));
    }
}
