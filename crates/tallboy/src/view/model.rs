//! What a column shows (PROMPT §7.3): the listing of one folder under the
//! column's own selection.
//!
//! The list store, filter and sort order belong to the `Listing`, which can
//! move to another column; the selection model stays with the column, and
//! with it the view built on top. Showing another folder swaps the listing
//! underneath.

use super::{
    FileItem,
    listing::{Handler, Listing, Notice, Phase, Shown},
};
use gtk::{gio, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tallboy_core::sort::SortSpec;
use tallboy_fs::folder::FsInfo;

/// About how many rows a GTK list keeps alive around what is visible.
const ROWS_KEPT: u32 = 200;
/// When a list model is replaced, GTK looks every new item up among the
/// rows of the old one, to keep the widgets of items that stay. That is a
/// call into the model per new item, about a two-hundred-and-fortieth of
/// what a row costs to build: past this many new items per row that could
/// be kept, dropping the old rows first is the cheaper way round.
const ITEMS_PER_ROW_WORTH_KEEPING: u32 = 240;

struct Hub {
    selection: gtk::MultiSelection,
    listing: RefCell<Listing>,
    shown: RefCell<Shown>,
    /// The column's own interest in what its listing reports.
    watcher: RefCell<Option<Handler>>,
    /// An item being announced again was selected; select it again.
    reselect: Cell<bool>,
    /// The listing shown has been handed to another column, see `lend`.
    lent: Cell<bool>,
}

/// One column's items with its filter, sort order and selection.
pub struct FolderModel(Rc<Hub>);

impl Hub {
    fn position_of_item(&self, item: &FileItem) -> Option<u32> {
        (0..self.selection.n_items())
            .find(|position| self.selection.item(*position).as_ref() == Some(item.upcast_ref()))
    }

    fn position_of(&self, uri: &str) -> Option<u32> {
        let item = self.listing.borrow().by_uri(uri)?;
        self.position_of_item(&item)
    }

    fn heard(&self, notice: Notice<'_>) {
        match &notice {
            Notice::Replacing(item) => {
                let selected = self
                    .position_of_item(item)
                    .is_some_and(|position| self.selection.is_selected(position));
                self.reselect.set(self.reselect.get() || selected);
            }
            Notice::Listed(uri) => {
                if self.reselect.replace(false)
                    && let Some(position) = self.position_of(uri)
                {
                    self.selection.select_item(position, false);
                }
            }
            _ => {}
        }
        let watcher = self.watcher.borrow().clone();
        if let Some(watcher) = watcher {
            watcher(notice);
        }
    }
}

impl FolderModel {
    /// Creates a model that shows no folder.
    pub fn new(sort: SortSpec, show_hidden: bool) -> Self {
        let shown = Shown {
            sort,
            show_hidden,
            glob: None,
            needle: None,
        };
        let listing = Listing::empty(&shown);
        let selection = gtk::MultiSelection::new(Some(listing.sorted().clone()));
        Self(Rc::new(Hub {
            selection,
            listing: RefCell::new(listing),
            shown: RefCell::new(shown),
            watcher: RefCell::default(),
            reselect: Cell::new(false),
            lent: Cell::new(false),
        }))
    }

    /// The selection model that views display. It stays the same object
    /// whatever folder is shown.
    pub fn selection(&self) -> &gtk::MultiSelection {
        &self.0.selection
    }

    /// The sort model of the folder shown now, for its `pending` property.
    pub fn sorted(&self) -> gtk::SortListModel {
        self.0.listing.borrow().sorted().clone()
    }

    /// Sets who hears what the listing shown reports, now and after it is
    /// replaced. It must capture its owner weakly.
    pub fn connect_notice(&self, watcher: impl Fn(Notice<'_>) + 'static) {
        self.0.watcher.replace(Some(Rc::new(watcher)));
    }

    /// How this column shows a folder, for a listing made for it.
    pub fn shown(&self) -> Shown {
        self.0.shown.borrow().clone()
    }

    /// Shows `listing` in this column's order and with its filters, and
    /// returns the one shown before, which the caller keeps or discards.
    /// A listing that was lent is the other column's by now; the empty
    /// listing is returned in its place.
    pub fn adopt(&self, listing: Listing) -> Listing {
        let hub = &self.0;
        let old = hub.listing.replace(listing.clone());
        let old = if hub.lent.replace(false) {
            Listing::empty(&hub.shown.borrow())
        } else {
            old.set_handler(None);
            old
        };
        listing.show_as(&hub.shown.borrow());
        listing.catch_up_yank();
        let weak = Rc::downgrade(hub);
        listing.set_handler(Some(Rc::new(move |notice| {
            if let Some(hub) = weak.upgrade() {
                hub.heard(notice);
            }
        })));
        hub.reselect.set(false);
        let (before, after) = (hub.selection.n_items(), listing.sorted().n_items());
        if before > 0 && after / ITEMS_PER_ROW_WORTH_KEEPING > before.min(ROWS_KEPT) {
            hub.selection.set_model(gio::ListModel::NONE);
        }
        hub.selection.set_model(Some(listing.sorted()));
        old
    }

    /// Hands the listing shown to another column, which adopts it, while
    /// this one goes on showing it until it is given something else. The
    /// rows built for it are then reused instead of being torn down now
    /// and built again a moment later.
    pub fn lend(&self) -> Listing {
        self.0.lent.set(true);
        self.0.listing.borrow().clone()
    }

    /// Shows no folder and returns the listing shown before.
    pub fn clear(&self) -> Listing {
        let empty = Listing::empty(&self.0.shown.borrow());
        self.adopt(empty)
    }

    /// Where the listing shown is with reading its folder.
    pub fn phase(&self) -> Phase {
        self.0.listing.borrow().phase()
    }

    /// Capacity of the filesystem the folder shown is on, once known.
    pub fn fs_info(&self) -> FsInfo {
        self.0.listing.borrow().fs_info()
    }

    /// Shows or hides hidden files.
    pub fn set_show_hidden(&self, show: bool) {
        self.0.shown.borrow_mut().show_hidden = show;
        self.0.listing.borrow().set_show_hidden(show);
    }

    /// Whether hidden files are shown.
    pub fn show_hidden(&self) -> bool {
        self.0.shown.borrow().show_hidden
    }

    /// Sets or clears the name filter.
    pub fn set_glob(&self, pattern: Option<String>) {
        let pattern = pattern.filter(|p| !p.is_empty());
        self.0.shown.borrow_mut().glob = pattern.clone();
        self.0.listing.borrow().set_glob(pattern);
    }

    /// The active name filter.
    pub fn glob(&self) -> Option<String> {
        self.0.shown.borrow().glob.clone()
    }

    /// Sets or clears the `f` filter. Unlike the glob it also hides folders.
    pub fn set_needle(&self, needle: Option<String>) {
        let needle = needle.filter(|n| !n.is_empty());
        self.0.shown.borrow_mut().needle = needle.clone();
        self.0.listing.borrow().set_needle(needle);
    }

    /// The active `f` filter.
    pub fn needle(&self) -> Option<String> {
        self.0.shown.borrow().needle.clone()
    }

    /// Changes the sort order.
    pub fn set_sort(&self, sort: SortSpec) {
        self.0.shown.borrow_mut().sort = sort.clone();
        self.0.listing.borrow().set_sort(sort);
    }

    /// The active sort order.
    pub fn sort(&self) -> SortSpec {
        self.0.shown.borrow().sort.clone()
    }

    /// Number of items shown.
    pub fn visible_count(&self) -> usize {
        self.0.selection.n_items() as usize
    }

    /// Number of items in the folder, shown or not.
    pub fn total_count(&self) -> usize {
        self.0.listing.borrow().total_count()
    }

    /// Number of hidden files that are currently not shown.
    pub fn hidden_count(&self) -> usize {
        if self.show_hidden() {
            0
        } else {
            self.0.listing.borrow().hidden_files()
        }
    }

    /// The item at a view position.
    pub fn item_at(&self, position: u32) -> Option<FileItem> {
        self.0.selection.item(position).and_downcast()
    }

    /// View position of the item with this URI, if it is shown.
    pub fn position_of(&self, uri: &str) -> Option<u32> {
        self.0.position_of(uri)
    }

    /// View position of an item, if it is shown.
    pub fn position_of_item(&self, item: &FileItem) -> Option<u32> {
        self.0.position_of_item(item)
    }

    /// Names in view order, for the find commands.
    pub fn names(&self) -> Vec<String> {
        (0..self.0.selection.n_items())
            .filter_map(|position| self.item_at(position))
            .map(|item| item.meta().display_name.clone())
            .collect()
    }

    /// Brings every item's yank mark in line with the yanked set.
    pub fn refresh_yank(&self) {
        self.0.listing.borrow().refresh_yank();
    }

    /// Selected items in view order.
    pub fn selected(&self) -> Vec<FileItem> {
        let bitset = self.0.selection.selection();
        (0..bitset.size())
            .filter_map(|index| self.item_at(bitset.nth(index as u32)))
            .collect()
    }

    /// Number of selected items, without materialising them.
    pub fn selected_count(&self) -> usize {
        self.0.selection.selection().size() as usize
    }
}
