//! Opening things (PROMPT §8.9). `core::open` decides what activating an
//! item means; this module carries the decision out. It works with or
//! without a window, because `tallboy file.txt` opens a file without one.

use crate::{
    app::App,
    dialogs::{self, base},
    tab::location_from_uri,
    view::FileItem,
    window::MainWindow,
};
use gtk::{gio, glib, prelude::*};
use std::{ffi::OsString, process::Child};
use tallboy_core::{
    location::Location,
    open::{self, Activation, Kind},
};
use tallboy_fs::{apps, terminal};

/// Opens items on behalf of a window, or of the application alone.
#[derive(Clone)]
pub struct Opener {
    app: App,
    window: Option<MainWindow>,
}

const TERMINALS: &str = include_str!("../../../data/terminals.list");

/// The terminal to use: the setting, then `$TERMINAL`, then the first
/// installed entry of `data/terminals.list`.
pub fn resolve_terminal(app: &App) -> Result<terminal::Terminal, terminal::TerminalError> {
    let entries = terminal::parse_list(TERMINALS)
        .or_else(|_| terminal::parse_list(terminal::DEFAULT_LIST))?;
    let from_environment = std::env::var("TERMINAL").ok();
    terminal::resolve(
        &app.settings().terminal(),
        from_environment.as_deref(),
        &entries,
        terminal::installed,
    )
}

/// The text editor's command: `$VISUAL`, then `$EDITOR`, then nvim.
fn resolve_editor() -> Result<Vec<OsString>, terminal::TerminalError> {
    let (visual, editor) = (std::env::var("VISUAL").ok(), std::env::var("EDITOR").ok());
    terminal::editor(visual.as_deref(), editor.as_deref(), terminal::installed)
}

/// Whether a file of this type is text to edit. Most XML formats descend
/// from `text/plain`, SVG pictures among them; those keep their applications.
/// An empty file has no type of its own and is text about to be written.
fn is_text(content_type: &str) -> bool {
    const MEDIA: [&str; 4] = ["image/", "audio/", "video/", "font/"];
    content_type == "application/x-zerosize"
        || gio::content_type_is_a(content_type, "text/plain")
            && !MEDIA.iter().any(|media| content_type.starts_with(media))
}

/// Waits for a started process on a thread of its own so that it never
/// lingers as a zombie.
pub fn reap(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// The launch context that carries the activation token, so the started
/// application can take focus under Wayland.
fn launch_context() -> Option<gio::AppLaunchContext> {
    gtk::gdk::Display::default().map(|display| display.app_launch_context().upcast())
}

/// Whether a path lies under an XDG applications directory (launcher trust).
fn in_applications(file: &gio::File) -> bool {
    let Some(path) = file.path() else {
        return false;
    };
    glib::system_data_dirs()
        .into_iter()
        .chain([glib::user_data_dir()])
        .any(|dir| path.starts_with(dir.join("applications")))
}

async fn facts(item: &FileItem, file: &gio::File) -> open::Item {
    let native = file.is_native();
    let mut facts = {
        let meta = item.meta();
        let content_type = &*meta.content_type;
        let text = is_text(content_type);
        let kind = match meta.file_type {
            gio::FileType::Directory => Kind::Directory,
            gio::FileType::Mountable => Kind::Mountable,
            gio::FileType::Shortcut => Kind::Shortcut,
            _ if content_type == "application/x-desktop" => Kind::Launcher,
            _ if text => Kind::Text,
            _ => Kind::Binary,
        };
        open::Item {
            kind,
            // A script or a program; never a picture that happens to be XML.
            executable: meta.access.execute
                && (text || gio::content_type_is_a(content_type, "application/x-executable")),
            filesystem: meta.filesystem_type.as_deref().map(String::from),
            native,
            in_applications: in_applications(file),
            has_default: apps::default(content_type, !native).is_some(),
        }
    };
    // Listings inherit the containing folder's filesystem type. A symlink
    // can point to another mount, and command-line files have no inherited
    // type at all. Execution trust must use the activated file's filesystem.
    if native && (facts.kind == Kind::Launcher || facts.executable) {
        facts.filesystem = file
            .query_filesystem_info_future("filesystem::type", glib::Priority::DEFAULT)
            .await
            .ok()
            .and_then(|info| info.attribute_string("filesystem::type"))
            .map(String::from);
        // Unknown execution permissions do not establish launcher trust.
        if facts.filesystem.is_none() {
            facts.native = false;
        }
    }
    facts
}

impl Opener {
    pub fn new(app: &App, window: Option<&MainWindow>) -> Self {
        Self {
            app: app.clone(),
            window: window.cloned(),
        }
    }

    fn parent(&self) -> Option<gtk::Window> {
        self.window.as_ref().map(|w| w.widget().clone().upcast())
    }

    fn alert(&self, message: &str, detail: &str) {
        base::alert(self.parent().as_ref(), message, detail);
    }

    /// A terminal that could not be started. When none is configured or
    /// installed, the dialog offers the page where one is set (§8.11).
    fn terminal_failed(&self, message: &str, error: &terminal::TerminalError) {
        let detail = dialogs::error::terminal(error);
        if !matches!(error, terminal::TerminalError::Missing) {
            return self.alert(message, &detail);
        }
        let (app, parent) = (self.app.clone(), self.parent());
        base::choose(
            parent.clone().as_ref(),
            message,
            &detail,
            &["_Close", "_Preferences"],
            0,
            1,
            move |answer| {
                if answer == 1 {
                    dialogs::prefs::show(&app, parent.as_ref(), dialogs::prefs::ADVANCED);
                }
            },
        );
    }

    /// Opens activated items: one folder is entered in place, several open
    /// as tabs, text files go to the editor together, other files go to their
    /// applications in one launch per application.
    pub fn open(&self, items: Vec<FileItem>) {
        let this = self.clone();
        let hold = self.app.gtk().hold();
        glib::spawn_future_local(async move {
            this.open_items(items).await;
            drop(hold);
        });
    }

    async fn open_items(&self, items: Vec<FileItem>) {
        let quick_exec = self.app.settings().quick_exec();
        let mut folders = Vec::new();
        let mut launch = Vec::new();
        let mut edit = Vec::new();
        for item in items {
            let file = gio::File::for_uri(&item.meta().uri);
            match open::classify(&facts(&item, &file).await, quick_exec) {
                Activation::Navigate => folders.push(item.meta().uri.clone()),
                Activation::FollowTarget => {
                    let target = item.meta().target_uri.clone();
                    folders.push(target.unwrap_or_else(|| item.meta().uri.clone()));
                }
                Activation::Mount => self.mount(file),
                Activation::LaunchDefault => {
                    launch.push((file, item.meta().content_type.to_string()))
                }
                Activation::OpenWith => self.open_with(vec![file], &item.meta().content_type),
                Activation::Edit => edit.push(file),
                Activation::Execute => self.execute(&file, false),
                Activation::AskBinary => self.ask_execute(file, &item),
                Activation::LaunchLauncher => self.launch_launcher(file),
                Activation::ConfirmLauncher => self.confirm_launcher(file, &item),
            }
        }
        self.show_folders(folders);
        self.edit(&edit);
        if !launch.is_empty() {
            match apps::group(&launch, apps::default) {
                Ok(groups) => self.launch(groups),
                Err(e) => self.alert("Cannot open the file", &dialogs::error::app(&e)),
            }
        }
    }

    fn show_folders(&self, uris: Vec<String>) {
        let locations: Vec<Location> = uris.iter().filter_map(|u| location_from_uri(u)).collect();
        match (&self.window, locations.as_slice()) {
            (_, []) => {}
            (Some(window), [only]) => {
                if let Some(tab) = window.active_tab() {
                    tab.navigate(only.clone());
                }
            }
            (Some(window), several) => {
                for location in several {
                    window.add_tab(location.clone());
                }
            }
            (None, all) => {
                for location in all {
                    self.app.open_folder(location.clone(), false);
                }
            }
        }
    }

    /// Starts each group's application once with all its files. The
    /// application is held open until the launches have been handed over.
    fn launch(&self, groups: Vec<apps::LaunchGroup>) {
        let hold = self.app.gtk().hold();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let context = launch_context();
            let terminal = || resolve_terminal(&this.app);
            if let Err(e) = apps::launch(&groups, context.as_ref(), terminal).await {
                this.alert(
                    "The application could not be started",
                    &dialogs::error::app(&e),
                );
            }
            drop(hold);
        });
    }

    /// Shows the Open With dialog for these files and opens them with the
    /// chosen application.
    pub fn open_with(&self, files: Vec<gio::File>, content_type: &str) {
        let this = self.clone();
        dialogs::open_with::show(self.parent().as_ref(), content_type, move |app| {
            this.launch(vec![apps::LaunchGroup {
                app,
                files: files.clone(),
            }]);
        });
    }

    /// Opens files with one application chosen from the Open With submenu.
    pub fn open_with_app(&self, files: Vec<gio::File>, app: gio::AppInfo) {
        self.launch(vec![apps::LaunchGroup { app, files }]);
    }

    fn execute(&self, file: &gio::File, in_terminal: bool) {
        let (Some(path), Some(directory)) = (file.path(), file.parent()) else {
            return;
        };
        let argv: Vec<OsString> = vec![path.into()];
        let started = if in_terminal {
            resolve_terminal(&self.app).and_then(|t| t.execute(&argv, &directory, false))
        } else {
            terminal::spawn(&argv, &directory)
        };
        match started {
            Ok(child) => reap(child),
            Err(e) => self.terminal_failed("The file could not be executed", &e),
        }
    }

    /// Opens text files in the editor: one terminal for all of them, started
    /// in the first file's folder.
    fn edit(&self, files: &[gio::File]) {
        let Some(directory) = files.first().and_then(|file| file.parent()) else {
            return;
        };
        let started = resolve_editor().and_then(|mut argv| {
            argv.extend(
                files
                    .iter()
                    .filter_map(|file| file.path())
                    .map(OsString::from),
            );
            resolve_terminal(&self.app)?.execute(&argv, &directory, false)
        });
        match started {
            Ok(child) => reap(child),
            Err(e) => self.terminal_failed("The text editor could not be started", &e),
        }
    }

    /// Upstream's "Execute File" question for an executable program.
    fn ask_execute(&self, file: gio::File, item: &FileItem) {
        let name = item.meta().display_name.clone();
        let this = self.clone();
        base::choose(
            self.parent().as_ref(),
            "Execute File",
            &format!("This file '{name}' is executable. Do you want to execute it?"),
            &["Cancel", "Execute in Terminal", "Execute"],
            0,
            0,
            move |choice| match choice {
                1 => this.execute(&file, true),
                2 => this.execute(&file, false),
                _ => {}
            },
        );
    }

    fn launch_launcher(&self, file: gio::File) {
        let hold = self.app.gtk().hold();
        let this = self.clone();
        glib::spawn_future_local(async move {
            let context = launch_context();
            let terminal = || resolve_terminal(&this.app);
            if let Err(e) = apps::launcher(&file, context.as_ref(), terminal).await {
                this.alert(
                    "The launcher could not be started",
                    &dialogs::error::app(&e),
                );
            }
            drop(hold);
        });
    }

    /// A launcher that is neither executable nor installed is only started
    /// after the user confirms; on removable media it is never trusted (D15).
    fn confirm_launcher(&self, file: gio::File, item: &FileItem) {
        let name = item.meta().display_name.clone();
        let this = self.clone();
        base::confirm(
            self.parent().as_ref(),
            "Untrusted application launcher",
            &format!(
                "The application launcher \"{name}\" is not marked as trusted. If you do not know where this file came from, launching it may be unsafe."
            ),
            "Launch Anyway",
            move |launch| {
                if launch {
                    this.launch_launcher(file);
                }
            },
        );
    }

    /// Mounts a mountable item, then shows what it points to.
    fn mount(&self, file: gio::File) {
        let operation = gtk::MountOperation::new(self.parent().as_ref());
        let this = self.clone();
        glib::spawn_future_local(async move {
            match file
                .mount_mountable_future(gio::MountMountFlags::NONE, Some(&operation))
                .await
            {
                Ok(target) => this.show_folders(vec![target.uri().into()]),
                Err(e) if e.matches(gio::IOErrorEnum::FailedHandled) => {}
                Err(e) => this.alert("The volume could not be mounted", &dialogs::error::io(&e)),
            }
        });
    }

    /// Opens a terminal in `directory` (Tools ▸ Open Current Folder in Terminal).
    pub fn terminal_in(&self, directory: &gio::File) {
        match resolve_terminal(&self.app).and_then(|terminal| terminal.open(directory)) {
            Ok(child) => reap(child),
            Err(e) => self.terminal_failed("The terminal could not be opened", &e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tallboy_fs::meta::FileMeta;

    fn launcher(file: &gio::File, inherited: Option<&str>) -> FileItem {
        let info = gio::FileInfo::new();
        info.set_file_type(gio::FileType::Regular);
        info.set_content_type("application/x-desktop");
        info.set_attribute_boolean("access::can-execute", true);
        FileItem::new(FileMeta::from_info(file, &info, true, inherited))
    }

    #[test]
    fn execution_trust_queries_the_target_filesystem_and_refuses_unknown_trust() {
        let target = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        std::fs::write(target.path().join("launcher.desktop"), b"[Desktop Entry]\n").unwrap();
        let path = links.path().join("launcher.desktop");
        std::os::unix::fs::symlink(target.path().join("launcher.desktop"), &path).unwrap();
        let file = gio::File::for_path(&path);
        let expected = file
            .query_filesystem_info("filesystem::type", gio::Cancellable::NONE)
            .unwrap()
            .attribute_string("filesystem::type")
            .unwrap()
            .to_string();
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            ctx.block_on(async {
                for inherited in [None, Some("vfat"), Some("invented-parent-filesystem")] {
                    let facts = facts(&launcher(&file, inherited), &file).await;
                    assert_eq!(facts.filesystem.as_deref(), Some(expected.as_str()));
                }
                let missing = gio::File::for_path(target.path().join("missing.desktop"));
                let facts = facts(&launcher(&missing, Some("btrfs")), &missing).await;
                assert_eq!(open::classify(&facts, true), Activation::ConfirmLauncher);
            })
        })
        .unwrap();
    }
}
