//! Default applications, grouping, and launches with caller-owned launch contexts.
use crate::terminal::{Terminal, TerminalError};
use gio::prelude::*;
use std::{ffi::OsString, path::Path};
use trek_core::location::Location;
/// Application lookup or launch failure.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// No MIME association.
    #[error("No default application for {0}")]
    NoDefault(String),
    /// Backend failure.
    #[error("Application launch: {0}")]
    Gio(#[from] glib::Error),
    /// Malformed or blocked file location.
    #[error(transparent)]
    Location(#[from] trek_core::location::LocationError),
    /// No terminal for an application that needs one.
    #[error(transparent)]
    Terminal(#[from] TerminalError),
    /// Invalid launcher or command.
    #[error("Invalid application launcher or empty command")]
    Invalid,
}
/// Files assigned to a single application launch.
pub struct LaunchGroup {
    /// Selected application.
    pub app: gio::AppInfo,
    /// Files passed in one invocation.
    pub files: Vec<gio::File>,
}
/// Default application for a MIME type, requiring URI support for remote files.
pub fn default(content_type: &str, remote: bool) -> Option<gio::AppInfo> {
    gio::AppInfo::default_for_type(content_type, remote)
}
/// Recommended apps followed by all other installed apps, without duplicates.
pub fn applications(content_type: &str) -> Vec<gio::AppInfo> {
    let mut apps = gio::AppInfo::recommended_for_type(content_type);
    for app in gio::AppInfo::all() {
        if !apps.iter().any(|a| a.equal(&app)) {
            apps.push(app);
        }
    }
    apps
}
/// Applications registered for a content type.
pub fn all_for_type(content_type: &str) -> Vec<gio::AppInfo> {
    gio::AppInfo::all_for_type(content_type)
}
/// Plan grouped launches. Lookup is injectable for isolated tests and Open With policy.
pub fn group(
    files: &[(gio::File, String)],
    mut lookup: impl FnMut(&str, bool) -> Option<gio::AppInfo>,
) -> Result<Vec<LaunchGroup>, AppError> {
    let mut groups: Vec<LaunchGroup> = vec![];
    for (file, content_type) in files {
        Location::parse(&file.uri(), Path::new("/"), Path::new("/"))?;
        let app = lookup(content_type, file.path().is_none())
            .ok_or_else(|| AppError::NoDefault(content_type.clone()))?;
        if let Some(g) = groups.iter_mut().find(|g| g.app.equal(&app)) {
            g.files.push(file.clone());
        } else {
            groups.push(LaunchGroup {
                app,
                files: vec![file.clone()],
            });
        }
    }
    Ok(groups)
}
/// The application to start for `app`: itself, or for a desktop entry with
/// `Terminal=true` a command line that runs it in `terminal`. GIO looks for a
/// terminal in a fixed list of its own (no kitty, alacritty or foot) and starts
/// nothing when none of them is installed. GIO still expands the field codes.
fn in_terminal(
    app: &gio::AppInfo,
    terminal: impl FnOnce() -> Result<Terminal, TerminalError>,
) -> Result<gio::AppInfo, AppError> {
    // An application made from a command line has no key file to ask.
    let wanted = app
        .downcast_ref::<gio_unix::DesktopAppInfo>()
        .is_some_and(|entry| entry.filename().is_some() && entry.boolean("Terminal"));
    let Some(exec) = app.commandline().filter(|_| wanted) else {
        return Ok(app.clone());
    };
    let terminal = terminal()?;
    let mut line = OsString::new();
    for argument in terminal.argv.iter().chain(&terminal.exec_args) {
        let quoted = glib::shell_quote(argument);
        line.push(quoted.to_string_lossy().replace('%', "%%"));
        line.push(" ");
    }
    line.push(exec);
    Ok(gio::AppInfo::create_from_commandline(
        line,
        Some(&app.name()),
        gio::AppInfoCreateFlags::NONE,
    )?)
}
/// Launch each group once, with startup/activation context supplied by the GTK layer.
/// `terminal` is asked only for applications that run in one.
pub async fn launch(
    groups: &[LaunchGroup],
    context: Option<&gio::AppLaunchContext>,
    terminal: impl Fn() -> Result<Terminal, TerminalError>,
) -> Result<(), AppError> {
    for g in groups {
        let app = in_terminal(&g.app, &terminal)?;
        let uris = g
            .files
            .iter()
            .map(|f| f.uri().to_string())
            .collect::<Vec<_>>();
        let refs = uris.iter().map(String::as_str).collect::<Vec<_>>();
        app.launch_uris_future(&refs, context).await?;
    }
    Ok(())
}
/// Store a MIME default, as explicitly chosen in Open With/Properties.
pub fn set_default(app: &gio::AppInfo, content_type: &str) -> Result<(), AppError> {
    app.set_as_default_for_type(content_type)?;
    Ok(())
}
/// Build a custom command application. GLib tokenizes argv; no shell is invoked.
/// File field codes are interpreted by GIO at launch, never concatenated into shell text.
pub fn custom(command: &str, name: &str, terminal: bool) -> Result<gio::AppInfo, AppError> {
    let argv = glib::shell_parse_argv(command)?;
    if argv.is_empty() {
        return Err(AppError::Invalid);
    }
    let flags = if terminal {
        gio::AppInfoCreateFlags::NEEDS_TERMINAL
    } else {
        gio::AppInfoCreateFlags::NONE
    };
    Ok(gio::AppInfo::create_from_commandline(
        command,
        Some(name),
        flags,
    )?)
}
/// Launch an already-confirmed native desktop file. Trust is decided by core::open.
pub async fn launcher(
    file: &gio::File,
    context: Option<&gio::AppLaunchContext>,
    terminal: impl FnOnce() -> Result<Terminal, TerminalError>,
) -> Result<(), AppError> {
    Location::parse(&file.uri(), Path::new("/"), Path::new("/"))?;
    let path = file.path().ok_or(AppError::Invalid)?;
    let app = gio_unix::DesktopAppInfo::from_filename(path).ok_or(AppError::Invalid)?;
    let app = in_terminal(app.upcast_ref(), terminal)?;
    app.launch_uris_future(&[], context).await?;
    Ok(())
}
