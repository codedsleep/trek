//! A column that shows a folder without being the place commands act on:
//! the parent folder to the left of a tab's files, and the folder under the
//! cursor to their right.
//!
//! It shows a `Listing` the way the files column does, and the two trade
//! them: the listing shown here becomes the tab's when the folder is
//! entered, and the tab's comes here when it is left. It can mark one file —
//! the folder the tab is in, seen from its parent — with the cursor's
//! highlight.

use super::{
    FileItem, FolderModel, Style, lines,
    listing::{Listing, ListingCache, Notice},
};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tallboy_core::sort::SortSpec;
use tallboy_fs::folder::Options;

type Activate = Rc<dyn Fn(FileItem)>;

struct Inner {
    scroller: gtk::ScrolledWindow,
    view: gtk::ListView,
    model: FolderModel,
    /// The tab's kept listings: where this column's go and come from.
    cache: Rc<ListingCache>,
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
    /// `marks` is whether the column can mark a file, as the parent column
    /// does and the preview does not.
    pub fn new(
        style: Style,
        sort: SortSpec,
        show_hidden: bool,
        cache: Rc<ListingCache>,
        marks: bool,
    ) -> Self {
        let model = FolderModel::new(sort, show_hidden);
        let view = lines::build_plain(&model, style, marks);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&view));
        let this = Self(Rc::new(Inner {
            scroller,
            view,
            model,
            cache,
            shown: RefCell::default(),
            mark: RefCell::default(),
            marked: RefCell::default(),
            generation: Cell::new(0),
            activate: RefCell::default(),
        }));
        let weak = Rc::downgrade(&this.0);
        this.0.model.connect_notice(move |notice| {
            let Some(this) = weak.upgrade().map(Self) else {
                return;
            };
            match notice {
                Notice::Finished => this.mark_when_sorted(),
                Notice::RootGone => this.clear(),
                _ => {}
            }
        });
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

    /// Brings the yank marks in line with the yanked set.
    pub fn refresh_yank(&self) {
        self.0.model.refresh_yank();
    }

    /// The folder shown, by URI.
    pub fn shown(&self) -> Option<String> {
        self.0.shown.borrow().clone()
    }

    /// Whether showing `uri` would take no reading: the tab has kept it.
    pub fn is_kept(&self, uri: &str, options: Options) -> bool {
        self.0.cache.holds(uri, options)
    }

    /// Forgets which folder it shows and takes the mark off.
    fn forget(&self) {
        let inner = &self.0;
        inner.generation.set(inner.generation.get() + 1);
        inner.shown.replace(None);
        inner.mark.replace(None);
        // The highlight is this column's; the listing may be shown elsewhere next.
        if let Some(marked) = inner.marked.take() {
            marked.hover(false);
        }
    }

    /// Shows nothing. The folder shown stays with the tab for a while.
    pub fn clear(&self) {
        self.forget();
        let listing = self.0.model.clear();
        self.0.cache.put(listing);
    }

    /// Gives up the listing shown if it is of `uri`, for the files column,
    /// which is about to show that folder. The rows stay as they are until
    /// this column is told what to show next.
    pub fn yield_listing(&self, uri: &str) -> Option<Listing> {
        let shown = self.0.shown.borrow().as_deref() == Some(uri);
        shown.then(|| {
            self.forget();
            self.0.model.lend()
        })
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
        // No mount operation: a column beside the files never asks for a
        // password or mounts anything; an unreachable folder stays empty.
        let listing = inner.cache.take(uri, options).or_else(|| {
            Listing::load(
                &gio::File::for_uri(uri),
                options,
                None,
                &inner.model.shown(),
            )
            .ok()
        });
        let Some(listing) = listing else {
            self.clear();
            inner.shown.replace(Some(uri.to_owned()));
            return;
        };
        self.forget();
        inner.shown.replace(Some(uri.to_owned()));
        inner.mark.replace(mark.map(str::to_owned));
        let loaded = listing.is_loaded();
        // Straight from one folder to the next: the rows are bound to the
        // new files instead of being built again.
        let old = inner.model.adopt(listing);
        inner.cache.put(old);
        if loaded && inner.model.sorted().pending() == 0 {
            // Kept in this order already: marked before the next frame.
            self.apply_mark();
        } else if loaded {
            self.mark_when_sorted();
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
