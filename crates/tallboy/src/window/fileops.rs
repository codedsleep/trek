//! File commands of the window: which files, which folder, which question
//! to ask first. The operations themselves are jobs in `tallboy-fs`.

use super::{
    MainWindow,
    prompt::{Caret, Session},
};
use crate::{
    clipboard,
    dialogs::base,
    ops,
    tab::{TabPage, location_from_uri},
    view::{FileItem, dnd::DropRequest},
};
use gtk::{gio, glib, prelude::*};
use tallboy_core::{
    config::DropAction,
    location::Location,
    naming,
    transfer::{self, DropCheck, Intent},
};
use tallboy_fs::{
    guard::Policy,
    jobs::{self, JobKind},
};

/// How a set of files is to be put into a folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transfer {
    Copy,
    Move,
    Link,
}

async fn filesystem_id(file: &gio::File) -> Option<String> {
    file.query_info_future(
        "id::filesystem",
        gio::FileQueryInfoFlags::NONE,
        glib::Priority::DEFAULT,
    )
    .await
    .ok()?
    .attribute_string("id::filesystem")
    .map(String::from)
}

fn uris(items: &[FileItem]) -> Vec<String> {
    items.iter().map(|item| item.meta().uri.clone()).collect()
}

fn names(items: &[FileItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| item.meta().display_name.clone())
        .collect()
}

impl MainWindow {
    fn parent(&self) -> gtk::Window {
        self.0.window.clone().upcast()
    }

    /// What a file command acts on: the selected files, or else the file
    /// under the cursor.
    pub(super) fn selected(&self) -> Vec<FileItem> {
        self.active_tab()
            .map(|tab| tab.targets())
            .unwrap_or_default()
    }

    pub(super) fn start_job(&self, kind: JobKind, title: &str) {
        ops::start(
            &self.0.app,
            Some(&self.parent()),
            kind,
            title,
            false,
            |_| {},
        );
    }

    // ---- trash and delete ----

    /// Move to Trash, which is Remove when the trash is switched off or the
    /// files are already in it (§8.10, §8.16).
    pub(super) fn trash_selection(&self) {
        let settings = self.0.app.settings();
        let in_trash = self.active_tab().is_some_and(|tab| tab.in_trash());
        if !settings.use_trash() || in_trash {
            self.delete_selection();
            return;
        }
        let items = self.selected();
        if items.is_empty() {
            return;
        }
        let files = uris(&items);
        let this = self.clone();
        let run = move || this.start_job(JobKind::Trash { files }, "Moving files to trash can");
        if !settings.confirm_trash() {
            run();
            return;
        }
        let question = match items.len() {
            1 => "Move this file to the trash can?".to_owned(),
            count => format!("Move these {count} files to the trash can?"),
        };
        self.ask_yes_no(&question, &names(&items), run);
    }

    /// Remove: permanent deletion, confirmed unless `confirm_del` is off.
    pub(super) fn delete_selection(&self) {
        let items = self.selected();
        if items.is_empty() {
            return;
        }
        let files = uris(&items);
        let this = self.clone();
        let run = move || this.start_job(JobKind::Delete { files }, "Deleting files");
        if !self.0.app.settings().confirm_del() {
            run();
            return;
        }
        let question = match items.len() {
            1 => "Delete this file permanently?".to_owned(),
            count => format!("Delete these {count} files permanently?"),
        };
        self.ask_yes_no(&question, &names(&items), run);
    }

    /// A question over the files: `y` or Enter for yes, `n` or Esc for no.
    fn ask_yes_no(&self, question: &str, names: &[String], yes: impl FnOnce() + 'static) {
        self.clear_pending();
        self.0.confirm.ask(question, names, yes);
    }

    // ---- properties ----

    /// Properties of the selection, or of the folder being shown (§8.18).
    pub(super) fn show_properties(&self, of_folder: bool) {
        let Some(tab) = self.active_tab() else { return };
        let app = self.0.app.clone();
        let parent = self.parent();
        let weak_tab = tab.downgrade();
        let renamed = move |uri: String| {
            if let Some(tab) = weak_tab.upgrade() {
                tab.select_when_listed(&uri);
            }
        };
        let selected = self.selected();
        if !of_folder && !selected.is_empty() {
            crate::view::item::settle_types(&selected);
            let files = selected.iter().map(|item| item.meta().clone()).collect();
            crate::dialogs::properties::show(&app, &parent, files, renamed);
            return;
        }
        // The folder itself is not an item of its own listing: ask for it.
        let file = gio::File::for_uri(tab.location().to_uri());
        let settings = app.settings().clone();
        glib::spawn_future_local(async move {
            let info = file
                .query_info_future(
                    &tallboy_fs::meta::attributes(settings.defer_content_test()),
                    gio::FileQueryInfoFlags::NONE,
                    glib::Priority::DEFAULT,
                )
                .await;
            match info {
                Ok(info) => {
                    let meta = tallboy_fs::meta::FileMeta::from_info(
                        &file,
                        &info,
                        settings.backup_as_hidden(),
                        None,
                    );
                    crate::dialogs::properties::show(&app, &parent, vec![meta], |_| {});
                }
                Err(e) => base::alert(
                    Some(&parent),
                    "The properties could not be read",
                    &crate::dialogs::error::io(&e),
                ),
            }
        });
    }

    // ---- trash can ----

    /// Restore: trashed files back to where they were deleted from (§8.16).
    pub(super) fn restore_selection(&self) {
        let files = uris(&self.selected());
        if !files.is_empty() {
            self.start_job(JobKind::Restore { files }, "Restoring files");
        }
    }

    /// Empty Trash Can, after confirming.
    pub(super) fn empty_trash(&self) {
        let this = self.clone();
        self.ask_yes_no(
            "Are you sure you want to empty the trash can?",
            &[],
            move || this.start_job(JobKind::EmptyTrash, "Emptying trash can"),
        );
    }

    // ---- rename and create ----

    /// `r`: an input line holding the name, the text cursor before the
    /// extension as Yazi's `rename --cursor=before_ext` puts it.
    pub(super) fn rename_selection(&self) {
        let selected = self.selected();
        let (Some(tab), [item]) = (self.active_tab(), selected.as_slice()) else {
            if selected.len() > 1 {
                self.notice("Renaming several files at once is not available: select one");
            }
            return;
        };
        let (uri, current) = {
            let meta = item.meta();
            (meta.uri.clone(), meta.edit_name.clone())
        };
        // `rename_range` counts bytes; the entry counts characters.
        let base = naming::rename_range(&current);
        let before_extension = current[..base.end].chars().count() as i32;
        let this = self.clone();
        let tab = tab.downgrade();
        let unchanged = current.clone();
        self.0.input.open(
            "Rename:",
            &current,
            Caret::At(before_extension),
            Session {
                changed: None,
                cancel: None,
                submit: Box::new(move |name| {
                    if name == unchanged {
                        return None;
                    }
                    if naming::validate(name).is_err() {
                        return Some(crate::dialogs::error::job(&jobs::JobError::Name(
                            naming::InvalidName,
                        )));
                    }
                    let (uri, name, this, tab) =
                        (uri.clone(), name.to_owned(), this.clone(), tab.clone());
                    glib::spawn_future_local(async move {
                        let target = name.clone();
                        let result = gio::spawn_blocking(move || {
                            jobs::rename(
                                &gio::File::for_uri(&uri),
                                &target,
                                &Policy::from_environment(),
                            )
                            .map(|file| file.uri().to_string())
                        })
                        .await;
                        match result {
                            Ok(Ok(renamed)) => {
                                if let Some(tab) = tab.upgrade() {
                                    tab.select_when_listed(&renamed);
                                }
                            }
                            Ok(Err(e)) => base::alert(
                                Some(&this.parent()),
                                &format!("\"{name}\" could not be used as the new name"),
                                &crate::dialogs::error::job(&e),
                            ),
                            Err(_) => log::warn!("rename worker stopped unexpectedly"),
                        }
                    });
                    None
                }),
            },
        );
    }

    /// Creates a folder or an empty file in the tab's folder; the backend
    /// picks a unique name if this one is taken (§8.10). Returns what is
    /// wrong with the name, if anything; a failure to create is reported
    /// once it is known.
    pub(super) fn create_named(&self, tab: &TabPage, name: &str, folder: bool) -> Option<String> {
        let parent_uri = tab.location().to_uri().to_owned();
        let this = self.clone();
        let tab = tab.downgrade();
        {
            {
                if naming::validate(name).is_err() {
                    return Some(crate::dialogs::error::job(&jobs::JobError::Name(
                        naming::InvalidName,
                    )));
                }
                let name = name.to_owned();
                glib::spawn_future_local(async move {
                    let result = gio::spawn_blocking(move || {
                        let parent = gio::File::for_uri(&parent_uri);
                        let policy = Policy::from_environment();
                        if folder {
                            jobs::create_folder(&parent, &name, &policy)
                        } else {
                            jobs::create_file(&parent, &name, &policy)
                        }
                        .map(|file| file.uri().to_string())
                    })
                    .await;
                    match result {
                        Ok(Ok(created)) => {
                            if let Some(tab) = tab.upgrade() {
                                tab.select_when_listed(&created);
                            }
                        }
                        Ok(Err(e)) => base::alert(
                            Some(&this.parent()),
                            if folder {
                                "The folder could not be created"
                            } else {
                                "The file could not be created"
                            },
                            &crate::dialogs::error::job(&e),
                        ),
                        Err(_) => log::warn!("create worker stopped unexpectedly"),
                    }
                });
                None
            }
        }
    }

    // ---- copy, move, link ----

    /// Puts `sources` into `destination`, after the checks that need no
    /// disk: a move within the same folder does nothing, and a folder cannot
    /// go into itself (§8.12).
    pub fn transfer(&self, sources: Vec<Location>, destination: &Location, how: Transfer) {
        self.transfer_with(sources, destination, how, false);
    }

    /// [`Self::transfer`]; with `overwrite`, files that already exist at the
    /// destination are replaced without asking (`P`).
    pub fn transfer_with(
        &self,
        sources: Vec<Location>,
        destination: &Location,
        how: Transfer,
        overwrite: bool,
    ) {
        let intent = match how {
            Transfer::Copy => Intent::Copy,
            Transfer::Move => Intent::Move,
            Transfer::Link => Intent::Link,
        };
        let mut wanted = Vec::new();
        for source in sources {
            // Lexically a folder cannot be told from a file; treating every
            // source as a possible folder only refuses what cannot work.
            match transfer::check_drop(&source, destination, true, intent) {
                DropCheck::Proceed => wanted.push(source.to_uri().to_owned()),
                DropCheck::Noop => {}
                DropCheck::IntoSelf => {
                    base::alert(
                        Some(&self.parent()),
                        "This cannot be done",
                        &format!(
                            "\"{}\" cannot be copied or moved into itself.",
                            source.display()
                        ),
                    );
                    return;
                }
            }
        }
        if wanted.is_empty() {
            return;
        }
        let dest_dir = destination.to_uri().to_owned();
        let (kind, title) = match how {
            Transfer::Copy => (
                JobKind::Copy {
                    sources: wanted,
                    dest_dir,
                },
                "Copying files",
            ),
            Transfer::Move => (
                JobKind::Move {
                    sources: wanted,
                    dest_dir,
                },
                "Moving files",
            ),
            Transfer::Link => (
                JobKind::Link {
                    sources: wanted,
                    dest_dir,
                },
                "Creating symbolic links",
            ),
        };
        ops::start(
            &self.0.app,
            Some(&self.parent()),
            kind,
            title,
            overwrite,
            |_| {},
        );
    }

    /// Create Link…, Move to…, Copy to…: choose a folder, then run the job.
    pub(super) fn transfer_selection_to_chosen_folder(&self, how: Transfer) {
        let sources: Vec<Location> = self
            .selected()
            .iter()
            .filter_map(|item| location_from_uri(&item.meta().uri))
            .collect();
        if sources.is_empty() {
            return;
        }
        let dialog = gtk::FileDialog::builder()
            .title(match how {
                Transfer::Copy => "Copy to...",
                Transfer::Move => "Move to...",
                Transfer::Link => "Create Link...",
            })
            .accept_label("_Select")
            .modal(true)
            .build();
        if let Some(folder) = self.current_folder() {
            dialog.set_initial_folder(Some(&folder));
        }
        let this = self.clone();
        dialog.select_folder(
            Some(&self.0.window),
            gio::Cancellable::NONE,
            move |chosen| {
                if let Ok(folder) = chosen
                    && let Some(destination) = location_from_uri(&folder.uri())
                {
                    this.transfer(sources, &destination, how);
                }
            },
        );
    }

    // ---- drops ----

    /// Files were dropped on a folder: choose copy, move or link (§8.12).
    /// Ctrl copies, Shift moves, both link; with neither,
    /// `drop_default_action` decides.
    pub(super) fn handle_drop(&self, request: DropRequest) {
        // A source that did not agree to a move gets a copy instead.
        let may_move = request.negotiated.contains(gtk::gdk::DragAction::MOVE);
        let allowed = move |how: Transfer| how != Transfer::Move || may_move;
        let from_intent = move |intent: Intent| match intent {
            Intent::Move if may_move => Transfer::Move,
            Intent::Link => Transfer::Link,
            Intent::Move | Intent::Copy | Intent::Ask => Transfer::Copy,
        };
        let default = self.0.app.settings().drop_default_action();
        if request.ctrl || request.shift {
            let intent = transfer::resolve(request.ctrl, request.shift, default, None, None);
            self.transfer(request.files, &request.destination, from_intent(intent));
            return;
        }
        match default {
            DropAction::Copy | DropAction::Move => {
                let intent = transfer::resolve(false, false, default, None, None);
                self.transfer(request.files, &request.destination, from_intent(intent));
            }
            DropAction::Ask => self.ask_drop(request, allowed),
            DropAction::Auto => {
                // Same filesystem moves, another one copies; finding out
                // means asking both, off the main loop.
                let this = self.clone();
                glib::spawn_future_local(async move {
                    let source = request
                        .files
                        .first()
                        .map(|l| gio::File::for_uri(l.to_uri()));
                    let destination = gio::File::for_uri(request.destination.to_uri());
                    let (from, to) = match source {
                        Some(source) => (
                            filesystem_id(&source).await,
                            filesystem_id(&destination).await,
                        ),
                        None => (None, None),
                    };
                    let intent = transfer::resolve(
                        false,
                        false,
                        DropAction::Auto,
                        from.as_deref(),
                        to.as_deref(),
                    );
                    this.transfer(request.files, &request.destination, from_intent(intent));
                });
            }
        }
    }

    /// `drop_default_action=ask`: a small menu at the drop point.
    fn ask_drop(&self, request: DropRequest, allowed: impl Fn(Transfer) -> bool) {
        let popover = gtk::Popover::new();
        popover.set_has_arrow(false);
        popover.set_parent(&request.widget);
        popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
            request.x as i32,
            request.y as i32,
            1,
            1,
        )));
        let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let files = std::rc::Rc::new(request.files);
        let destination = request.destination;
        for (label, how) in [
            ("Copy Here", Some(Transfer::Copy)),
            ("Move Here", Some(Transfer::Move)),
            ("Link Here", Some(Transfer::Link)),
            ("Cancel", None),
        ] {
            if how.is_some_and(|how| !allowed(how)) {
                continue;
            }
            let button = gtk::Button::with_label(label);
            button.add_css_class("flat");
            let (this, files, destination) = (self.clone(), files.clone(), destination.clone());
            let weak = popover.downgrade();
            button.connect_clicked(move |_| {
                if let Some(popover) = weak.upgrade() {
                    popover.popdown();
                }
                if let Some(how) = how {
                    this.transfer(files.to_vec(), &destination, how);
                }
            });
            choices.append(&button);
        }
        popover.set_child(Some(&choices));
        popover.connect_closed(|popover| popover.unparent());
        popover.popup();
    }

    // ---- clipboard ----

    pub(super) fn copy_selection(&self, cut: bool) {
        let files: Vec<Location> = self
            .selected()
            .iter()
            .filter_map(|item| location_from_uri(&item.meta().uri))
            .collect();
        if !files.is_empty() {
            clipboard::set_files(files, cut);
        }
    }

    /// Copy Path(s): one native path per line, URIs for other locations.
    pub(super) fn copy_paths(&self) {
        let files: Vec<Location> = self
            .selected()
            .iter()
            .filter_map(|item| location_from_uri(&item.meta().uri))
            .collect();
        if !files.is_empty() {
            let payload = transfer::Payload {
                intent: Intent::Copy,
                files,
            };
            clipboard::set_text(&payload.plain());
        }
    }

    /// Paste into the current folder. `force` replaces existing files
    /// without asking.
    pub(super) fn paste(&self, force: bool) {
        if let Some(destination) = self.active_tab().map(|tab| tab.location()) {
            self.paste_into(destination, force);
        }
    }

    /// Pastes into `destination`: the current folder, or the single selected
    /// folder from its context menu. A cut becomes a move and empties the
    /// clipboard (§8.11).
    pub(super) fn paste_into(&self, destination: Location, force: bool) {
        let this = self.clone();
        clipboard::read(move |payload| {
            let Some(payload) = payload else {
                this.notice("Nothing to paste");
                return;
            };
            let how = if payload.intent == Intent::Move {
                clipboard::clear_own();
                Transfer::Move
            } else {
                Transfer::Copy
            };
            this.transfer_with(payload.files, &destination, how, force);
        });
    }
}
