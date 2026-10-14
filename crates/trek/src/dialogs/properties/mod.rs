//! The Properties dialog (PROMPT §8.18): one window for one or many files,
//! with a General and a Permissions page.

mod ids;
mod permissions;

use super::base::{self, Dialog, Role};
use crate::{app::App, ops};
use gtk::{gio, glib, pango, prelude::*};
use permissions::Permissions;
use std::{cell::RefCell, rc::Rc};
use trek_core::{format, naming};
use trek_fs::{
    apps,
    guard::Policy,
    jobs::{self, JobEvent, JobHandle, JobKind},
    meta::FileMeta,
};

fn is_folder(meta: &FileMeta) -> bool {
    meta.file_type == gio::FileType::Directory
}

fn local_time(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    glib::DateTime::from_unix_local(seconds)
        .and_then(|time| time.format("%x %X"))
        .map(String::from)
        .unwrap_or_default()
}

fn exact(bytes: u64, si: bool) -> String {
    format!("{} ({bytes} bytes)", format::size(bytes, si))
}

/// The General page's rows, built up one after another.
struct Rows {
    grid: gtk::Grid,
    next: i32,
}

impl Rows {
    fn add(&mut self, caption: &str, value: &impl IsA<gtk::Widget>) {
        let label = gtk::Label::new(Some(caption));
        label.set_xalign(0.0);
        label.set_valign(gtk::Align::Start);
        self.grid.attach(&label, 0, self.next, 1, 1);
        self.grid.attach(value, 1, self.next, 1, 1);
        self.next += 1;
    }

    fn text(&mut self, caption: &str, value: &str) -> gtk::Label {
        let label = gtk::Label::new(Some(value));
        label.set_xalign(0.0);
        label.set_selectable(true);
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_max_width_chars(48);
        self.add(caption, &label);
        label
    }
}

/// The Open-with dropdown: the applications for the type, the default one
/// selected, and "Customize…" for the Open With dialog.
struct OpenWith {
    dropdown: gtk::DropDown,
    apps: Rc<RefCell<Vec<gio::AppInfo>>>,
    initial: Option<gio::AppInfo>,
    content_type: String,
}

impl OpenWith {
    fn new(content_type: &str, parent: &gtk::Window) -> Self {
        let apps = Rc::new(RefCell::new(apps::all_for_type(content_type)));
        let initial = apps::default(content_type, false);
        if let Some(default) = &initial
            && !apps.borrow().iter().any(|app| app.equal(default))
        {
            apps.borrow_mut().insert(0, default.clone());
        }
        let names = gtk::StringList::new(&[]);
        for app in apps.borrow().iter() {
            names.append(&app.display_name());
        }
        names.append("Customize...");
        let dropdown = gtk::DropDown::new(Some(names.clone()), gtk::Expression::NONE);
        let selected = initial
            .as_ref()
            .and_then(|default| apps.borrow().iter().position(|app| app.equal(default)));
        dropdown.set_selected(selected.map_or(gtk::INVALID_LIST_POSITION, |index| index as u32));
        // "Customize…" hands over to the Open With dialog and then selects
        // whatever was chosen there.
        let (list, parent, content) = (apps.clone(), parent.downgrade(), content_type.to_owned());
        let previous = std::cell::Cell::new(dropdown.selected());
        dropdown.connect_selected_notify(move |dropdown| {
            let selected = dropdown.selected();
            if selected as usize != list.borrow().len() {
                previous.set(selected);
                return;
            }
            dropdown.set_selected(previous.get());
            let (list, names, dropdown) = (list.clone(), names.clone(), dropdown.downgrade());
            super::open_with::show(parent.upgrade().as_ref(), &content, move |app| {
                let Some(dropdown) = dropdown.upgrade() else {
                    return;
                };
                let known = list.borrow().iter().position(|other| other.equal(&app));
                let index = known.unwrap_or_else(|| {
                    let at = list.borrow().len();
                    names.splice(at as u32, 0, &[&app.display_name()]);
                    list.borrow_mut().push(app);
                    at
                });
                dropdown.set_selected(index as u32);
            });
        });
        Self {
            dropdown,
            apps,
            initial,
            content_type: content_type.to_owned(),
        }
    }

    /// Makes the selected application the default if that is a change.
    fn apply(&self) -> Result<(), String> {
        let apps = self.apps.borrow();
        let Some(chosen) = apps.get(self.dropdown.selected() as usize) else {
            return Ok(());
        };
        if self.initial.as_ref().is_some_and(|was| was.equal(chosen)) {
            return Ok(());
        }
        apps::set_default(chosen, &self.content_type).map_err(|e| super::error::app(&e))
    }
}

/// Counts a folder's contents in the background, updating two labels.
fn count_into(files: &[FileMeta], total: &gtk::Label, on_disk: &gtk::Label, si: bool) -> JobHandle {
    total.set_text("Calculating...");
    let handle = jobs::spawn(JobKind::Count {
        files: files.iter().map(|f| f.uri.clone()).collect(),
    });
    let events = handle.events.clone();
    let (total, on_disk) = (total.downgrade(), on_disk.downgrade());
    glib::spawn_future_local(async move {
        while let Ok(event) = events.recv().await {
            let (Some(total), Some(on_disk)) = (total.upgrade(), on_disk.upgrade()) else {
                break;
            };
            match event {
                JobEvent::Progress {
                    done_files,
                    done_bytes,
                    ..
                } => total.set_text(&format!(
                    "{} in {done_files} items, counting...",
                    format::size(done_bytes, si)
                )),
                JobEvent::Finished(summary) => {
                    if summary.cancelled {
                        break;
                    }
                    total.set_text(&format!(
                        "{} in {} items",
                        exact(summary.done_bytes, si),
                        summary.done_files
                    ));
                    on_disk.set_text(&exact(summary.allocated_bytes, si));
                    break;
                }
                JobEvent::Preparing
                | JobEvent::Prepared { .. }
                | JobEvent::Current { .. }
                | JobEvent::Ask(..) => {}
            }
        }
    });
    handle
}

/// Shows Properties for `files`. `renamed` gets the new URI when the name
/// of a single file was changed.
pub fn show(
    app: &App,
    parent: &gtk::Window,
    files: Vec<FileMeta>,
    renamed: impl Fn(String) + 'static,
) {
    let Some(first) = files.first().cloned() else {
        return;
    };
    let si = app.settings().si_unit();
    let single = files.len() == 1;
    let dialog = Dialog::new(Some(parent), "File Properties");
    let window = dialog.window().clone();

    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    grid.add_css_class("dialog-body");
    let mut rows = Rows { grid, next: 0 };

    // Name: editable for one file, which renames it on OK.
    let icon = gtk::Image::new();
    icon.set_pixel_size(48);
    icon.set_halign(gtk::Align::Start);
    let gicon = if single {
        first
            .icon
            .as_deref()
            .and_then(|s| gio::Icon::for_string(s).ok())
    } else {
        Some(gio::ThemedIcon::new("document-multiple").upcast())
    };
    match gicon {
        Some(gicon) => icon.set_from_gicon(&gicon),
        None => icon.set_icon_name(Some("text-x-generic")),
    }
    let name = gtk::Entry::new();
    name.set_hexpand(true);
    name.set_width_chars(36);
    name.set_activates_default(true);
    if single {
        name.set_text(&first.edit_name);
        name.set_sensitive(first.access.rename && first.trash_orig_path.is_none());
    } else {
        name.set_text("Multiple files");
        name.set_sensitive(false);
    }
    rows.grid.attach(&icon, 0, 0, 1, 1);
    rows.grid.attach(&name, 1, 0, 1, 1);
    rows.next = 1;

    let folder = gio::File::for_uri(&first.uri).parent();
    rows.text(
        "Location:",
        &folder.map_or_else(String::new, |f| ops::display(&f.uri())),
    );
    let one_type = files.iter().all(|f| f.content_type == first.content_type);
    rows.text(
        "File type:",
        &if one_type {
            format!("{} ({})", first.description, first.content_type)
        } else {
            "Files of different types".to_owned()
        },
    );
    // Open with: not for folders and not for a mixed selection.
    let open_with = (one_type && !is_folder(&first)).then(|| {
        let open_with = OpenWith::new(&first.content_type, &window);
        rows.add("Open with:", &open_with.dropdown);
        open_with
    });
    if single && let Some(target) = &first.symlink_target {
        rows.text("Link target:", &target.to_string_lossy());
    }
    if single && let Some(original) = &first.trash_orig_path {
        rows.text("Original location:", &original.to_string_lossy());
        rows.text(
            "Deleted:",
            first.trash_deletion_date.as_deref().unwrap_or_default(),
        );
    }

    // Sizes: known at once for plain files, counted for folders.
    let total = rows.text("Total size of files:", "");
    let on_disk = rows.text("Size on disk:", "");
    let counting = if files.iter().any(is_folder) {
        Some(count_into(&files, &total, &on_disk, si))
    } else {
        total.set_text(&exact(files.iter().map(|f| f.size).sum(), si));
        on_disk.set_text(&exact(files.iter().map(|f| f.allocated_size).sum(), si));
        None
    };
    // Closing the window drops the handle, which cancels the count.
    let counting = Rc::new(RefCell::new(counting));

    if single {
        rows.text("Last modification:", &local_time(first.mtime));
        rows.text("Last access:", &local_time(first.atime));
        let changed = rows.text("Last permissions change:", "");
        let file = gio::File::for_uri(&first.uri);
        let changed = changed.downgrade();
        glib::spawn_future_local(async move {
            let info = file
                .query_info_future(
                    "time::changed",
                    gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                    glib::Priority::DEFAULT,
                )
                .await;
            if let (Ok(info), Some(label)) = (info, changed.upgrade())
                && info.has_attribute("time::changed")
            {
                label.set_text(&local_time(info.attribute_uint64("time::changed") as i64));
            }
        });
    }

    let permissions = Permissions::new(&files, app.settings().advanced_mode());
    let pages = gtk::Notebook::new();
    pages.append_page(&rows.grid, Some(&gtk::Label::with_mnemonic("_General")));
    pages.append_page(
        permissions.widget(),
        Some(&gtk::Label::with_mnemonic("_Permissions")),
    );
    dialog.content().append(&pages);
    let problem = gtk::Label::new(None);
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_max_width_chars(60);
    problem.add_css_class("error");
    problem.set_visible(false);
    dialog.content().append(&problem);

    let stop = counting.clone();
    window.connect_close_request(move |_| {
        stop.replace(None);
        glib::Propagation::Proceed
    });
    dialog.cancel_button();
    let ok = dialog.button("_OK", Role::Default);
    let (focus_name, focus_ok) = (name.clone(), ok.clone());
    let (app, parent) = (app.clone(), parent.clone());
    let renamed = Rc::new(renamed);
    ok.connect_clicked(move |_| {
        let fail = |message: String| {
            problem.set_text(&message);
            problem.set_visible(true);
        };
        // Check everything first, so that nothing is half applied.
        let new_name = name.text().to_string();
        let rename = single && name.is_sensitive() && new_name != first.edit_name;
        if rename && naming::validate(&new_name).is_err() {
            return fail(super::error::job(&jobs::JobError::Name(
                naming::InvalidName,
            )));
        }
        let change = match permissions.change() {
            Ok(change) => change,
            Err(message) => return fail(message),
        };
        if let Some(open_with) = &open_with
            && let Err(message) = open_with.apply()
        {
            return fail(message);
        }
        window.close();

        let uris: Vec<String> = files.iter().map(|f| f.uri.clone()).collect();
        let rename_after = {
            let (uri, parent, renamed) = (first.uri.clone(), parent.clone(), renamed.clone());
            move || {
                if !rename {
                    return;
                }
                glib::spawn_future_local(async move {
                    let target = new_name.clone();
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
                        Ok(Ok(uri)) => renamed(uri),
                        Ok(Err(e)) => base::alert(
                            Some(&parent),
                            &format!("\"{new_name}\" could not be used as the new name"),
                            &super::error::job(&e),
                        ),
                        Err(_) => log::warn!("rename worker stopped unexpectedly"),
                    }
                });
            }
        };
        if !permissions::is_empty(&change) {
            let start = {
                let (app, parent) = (app.clone(), parent.clone());
                move |recursive: bool| {
                    ops::start(
                        &app,
                        Some(&parent),
                        JobKind::SetAttrs {
                            files: uris.clone(),
                            change,
                            recursive,
                        },
                        "Changing file attributes",
                        false,
                        move |summary| {
                            // Both changes name the original URI. Renaming first
                            // races the worker (or its pending recursion question).
                            if !summary.cancelled
                                && summary.errors.is_empty()
                                && summary.skipped == 0
                            {
                                rename_after();
                            }
                        },
                    );
                }
            };
            if files.iter().any(is_folder) {
                base::choose(
                    Some(&parent),
                    "Confirm",
                    "Do you want to recursively apply these changes to all files and sub-folders?",
                    &["No", "Yes"],
                    0,
                    0,
                    move |choice| start(choice == 1),
                );
            } else {
                start(false);
            }
        } else {
            rename_after();
        }
    });
    dialog.present();
    // Initial focus: the name when it can be edited, with its base name
    // selected as in the Rename dialog; otherwise the OK button.
    if focus_name.is_sensitive() {
        focus_name.grab_focus();
        let text = focus_name.text();
        let base = naming::rename_range(&text).end;
        focus_name.select_region(0, text[..base].chars().count() as i32);
    } else {
        focus_ok.grab_focus();
    }
}
