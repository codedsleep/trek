//! A column that shows a folder without being the place commands act on:
//! the parent folder to the left of a tab's files, and the folder under the
//! cursor to their right.
//!
//! It owns its own `Folder`, so it loads, sorts and follows changes the way
//! a tab does, and it can mark one file — the folder the tab is in, seen
//! from its parent — with the cursor's highlight.

use super::{FileItem, FolderModel, Style, lines};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tallboy_core::sort::SortSpec;
use tallboy_fs::folder::{Folder, FolderEvent, Options};

type Activate = Rc<dyn Fn(FileItem)>;

struct Inner {
    scroller: gtk::ScrolledWindow,
    view: gtk::ListView,
    model: FolderModel,
    folder: RefCell<Option<Folder>>,
    /// URI of the folder shown.
    shown: RefCell<Option<String>>,
    /// URI of the file to mark, and the item once it is listed.
    mark: RefCell<Option<String>>,
    marked: RefCell<Option<FileItem>>,
    generation: Cell<u64>,
    activate: RefCell<Option<Activate>>,
}

/// A read-only folder column. Clones share the same column.
#[derive(Clone)]
pub struct MiniColumn(Rc<Inner>);

impl MiniColumn {
    pub fn new(style: Style, sort: SortSpec, show_hidden: bool) -> Self {
        let model = FolderModel::new(sort, show_hidden);
        let view = lines::build_plain(&model, style);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&view));
        let this = Self(Rc::new(Inner {
            scroller,
            view,
            model,
            folder: RefCell::default(),
            shown: RefCell::default(),
            mark: RefCell::default(),
            marked: RefCell::default(),
            generation: Cell::new(0),
            activate: RefCell::default(),
        }));
        let weak = Rc::downgrade(&this.0);
        this.0.view.connect_activate(move |_, position| {
            let Some(inner) = weak.upgrade() else { return };
            let activate = inner.activate.borrow().clone();
            let item = inner
                .view
                .model()
                .and_then(|model| model.item(position))
                .and_downcast::<FileItem>();
            if let (Some(activate), Some(item)) = (activate, item) {
                activate(item);
            }
        });
        this
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.0.scroller
    }

    /// Sets what a click on a row does. It must capture its owner weakly.
    pub fn connect_activate(&self, activate: impl Fn(FileItem) + 'static) {
        self.0.activate.replace(Some(Rc::new(activate)));
    }

    /// The order and the hidden files follow the tab's.
    pub fn set_view(&self, sort: SortSpec, show_hidden: bool) {
        self.0.model.set_sort(sort);
        self.0.model.set_show_hidden(show_hidden);
    }

    /// The folder shown, by URI.
    pub fn shown(&self) -> Option<String> {
        self.0.shown.borrow().clone()
    }

    /// Shows nothing and stops watching the folder.
    pub fn clear(&self) {
        let inner = &self.0;
        inner.generation.set(inner.generation.get() + 1);
        inner.folder.replace(None);
        inner.shown.replace(None);
        inner.mark.replace(None);
        inner.marked.replace(None);
        inner.model.clear();
    }

    /// Shows the folder at `uri`, marking the file `mark` in it. Showing the
    /// folder that is already shown only moves the mark.
    pub fn show(&self, uri: &str, mark: Option<&str>, options: Options) {
        let inner = &self.0;
        if inner.shown.borrow().as_deref() == Some(uri) {
            if inner.mark.borrow().as_deref() != mark {
                inner.mark.replace(mark.map(str::to_owned));
                self.apply_mark();
            }
            return;
        }
        self.clear();
        let generation = inner.generation.get();
        inner.shown.replace(Some(uri.to_owned()));
        inner.mark.replace(mark.map(str::to_owned));
        // No mount operation: a column beside the files never asks for a
        // password or mounts anything; an unreachable folder stays empty.
        let Ok(folder) = Folder::new(&gio::File::for_uri(uri), options, None) else {
            return;
        };
        let events = folder.events.clone();
        inner.folder.replace(Some(folder));
        let weak = Rc::downgrade(&self.0);
        glib::spawn_future_local(async move {
            while let Ok(event) = events.recv().await {
                let Some(this) = weak.upgrade().map(Self) else {
                    break;
                };
                if this.0.generation.get() != generation {
                    break;
                }
                this.apply(event);
            }
        });
    }

    fn apply(&self, event: FolderEvent) {
        let model = &self.0.model;
        match event {
            FolderEvent::Batch(batch) => model.append_batch(batch),
            FolderEvent::Added(meta) | FolderEvent::Changed(meta) => model.upsert(meta),
            FolderEvent::Removed(uri) => {
                model.remove(&uri);
            }
            FolderEvent::Renamed { from, to } => model.rename(&from, to),
            FolderEvent::Finished => self.mark_when_sorted(),
            FolderEvent::RootGone => self.clear(),
            FolderEvent::Started | FolderEvent::Error(_) | FolderEvent::FsInfo(_) => {}
        }
    }

    /// The mark's position is only final once the incremental sort is.
    fn mark_when_sorted(&self) {
        let weak = Rc::downgrade(&self.0);
        let generation = self.0.generation.get();
        glib::idle_add_local(move || {
            let Some(this) = weak.upgrade().map(Self) else {
                return glib::ControlFlow::Break;
            };
            if this.0.generation.get() != generation {
                return glib::ControlFlow::Break;
            }
            if this.0.model.sorted().pending() > 0 {
                return glib::ControlFlow::Continue;
            }
            this.apply_mark();
            glib::ControlFlow::Break
        });
    }

    fn apply_mark(&self) {
        let inner = &self.0;
        let wanted = inner
            .mark
            .borrow()
            .as_deref()
            .and_then(|uri| Some((inner.model.position_of(uri)?, uri.to_owned())));
        let item = wanted
            .as_ref()
            .and_then(|(position, _)| inner.model.item_at(*position));
        let previous = inner.marked.replace(item.clone());
        if previous != item
            && let Some(previous) = previous
        {
            previous.hover(false);
        }
        if let (Some(item), Some((position, _))) = (item, wanted) {
            item.hover(true);
            inner
                .view
                .scroll_to(position, gtk::ListScrollFlags::NONE, None);
        }
    }

    /// Scrolls by `lines` rows; negative scrolls up.
    pub fn scroll_by(&self, lines: i32) {
        scroll_lines(&self.0.scroller, lines);
    }
}

/// Scrolls a scrolled window by a number of text lines.
pub fn scroll_lines(scroller: &gtk::ScrolledWindow, lines: i32) {
    let adjustment = scroller.vadjustment();
    let line = f64::from(
        scroller
            .pango_context()
            .metrics(None, None)
            .height()
            .max(gtk::pango::SCALE * 12),
    ) / f64::from(gtk::pango::SCALE);
    let limit = (adjustment.upper() - adjustment.page_size()).max(0.0);
    adjustment.set_value((adjustment.value() + f64::from(lines) * (line + 6.0)).clamp(0.0, limit));
}
