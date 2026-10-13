//! The model stack shared by every view mode (PROMPT §7.3):
//! list store → filter → sort → multi-selection.

use super::{FileItem, index::UriIndex};
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use tallboy_core::{glob, keymap, sort::SortSpec};
use tallboy_fs::meta::{self, FileMeta};

struct State {
    show_hidden: Cell<bool>,
    glob: RefCell<Option<String>>,
    /// The `f` filter: a smart-case substring every item must contain.
    needle: RefCell<Option<String>>,
    sort: RefCell<SortSpec>,
}

/// One folder's items with the tab's filter, sort order and selection.
pub struct FolderModel {
    store: gio::ListStore,
    filter: gtk::CustomFilter,
    sorter: gtk::CustomSorter,
    sorted: gtk::SortListModel,
    selection: gtk::MultiSelection,
    state: Rc<State>,
    by_uri: RefCell<UriIndex>,
    hidden: Cell<usize>,
}

fn file_item(object: &glib::Object) -> Option<&FileItem> {
    object.downcast_ref::<FileItem>()
}

impl FolderModel {
    /// Creates an empty model.
    pub fn new(sort: SortSpec, show_hidden: bool) -> Self {
        let state = Rc::new(State {
            show_hidden: Cell::new(show_hidden),
            glob: RefCell::new(None),
            needle: RefCell::new(None),
            sort: RefCell::new(sort),
        });
        let store = gio::ListStore::new::<FileItem>();
        let filter = gtk::CustomFilter::new({
            let state = state.clone();
            move |object| {
                let Some(item) = file_item(object) else {
                    return false;
                };
                let meta = item.meta();
                if meta.hidden && !state.show_hidden.get() {
                    return false;
                }
                if let Some(needle) = state.needle.borrow().as_deref()
                    && !keymap::smart_matches(&meta.display_name, needle)
                {
                    return false;
                }
                match state.glob.borrow().as_deref() {
                    // Folders always pass the name filter (§8.7).
                    Some(pattern) if meta.file_type != gio::FileType::Directory => {
                        glob::matches(pattern, &meta.display_name)
                    }
                    _ => true,
                }
            }
        });
        let sorter = gtk::CustomSorter::new({
            let state = state.clone();
            move |a, b| match (file_item(a), file_item(b)) {
                (Some(a), Some(b)) => {
                    meta::compare(&a.meta(), &b.meta(), &state.sort.borrow()).into()
                }
                _ => gtk::Ordering::Equal,
            }
        });
        let filtered = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
        let sorted = gtk::SortListModel::new(Some(filtered), Some(sorter.clone()));
        // Sorting 50,000 items in one go would stall the main loop.
        sorted.set_incremental(true);
        let selection = gtk::MultiSelection::new(Some(sorted.clone()));
        Self {
            store,
            filter,
            sorter,
            sorted,
            selection,
            state,
            by_uri: RefCell::default(),
            hidden: Cell::new(0),
        }
    }

    /// The selection model that views display.
    pub fn selection(&self) -> &gtk::MultiSelection {
        &self.selection
    }

    /// The sort model, exposed for its `pending` property.
    pub fn sorted(&self) -> &gtk::SortListModel {
        &self.sorted
    }

    /// Removes every item.
    pub fn clear(&self) {
        self.store.remove_all();
        self.by_uri.borrow_mut().clear();
        self.hidden.set(0);
    }

    /// Appends one loader batch with a single `splice`.
    pub fn append_batch(&self, batch: Vec<FileMeta>) {
        let mut hidden = self.hidden.get();
        let mut by_uri = self.by_uri.borrow_mut();
        let items: Vec<FileItem> = batch
            .into_iter()
            .map(|meta| {
                hidden += usize::from(meta.hidden);
                let item = FileItem::new(meta);
                by_uri.insert(item.clone());
                item
            })
            .collect();
        drop(by_uri);
        self.hidden.set(hidden);
        self.store.extend_from_slice(&items);
    }

    /// Adds a file reported by the monitor, or updates it when already listed.
    pub fn upsert(&self, meta: FileMeta) {
        let existing = self.by_uri.borrow().get(&meta.uri);
        let Some(item) = existing else {
            self.hidden
                .set(self.hidden.get() + usize::from(meta.hidden));
            let item = FileItem::new(meta);
            self.by_uri.borrow_mut().insert(item.clone());
            self.store.append(&item);
            return;
        };
        let was_selected = self
            .position_of_item(&item)
            .is_some_and(|position| self.selection.is_selected(position));
        let was_hidden = item.meta().hidden;
        let hidden = self.hidden.get() - usize::from(was_hidden) + usize::from(meta.hidden);
        self.hidden.set(hidden);
        item.set_meta(meta);
        if let Some(position) = self.store.find(&item) {
            // Re-announce the item so that filter, sort and bound cells refresh.
            self.store.items_changed(position, 1, 1);
        }
        if was_selected && let Some(position) = self.position_of_item(&item) {
            self.selection.select_item(position, false);
        }
    }

    /// Removes the file with this URI; returns whether it was selected.
    pub fn remove(&self, uri: &str) -> bool {
        let Some(item) = self.by_uri.borrow_mut().remove(uri) else {
            return false;
        };
        let was_selected = self
            .position_of_item(&item)
            .is_some_and(|position| self.selection.is_selected(position));
        self.hidden
            .set(self.hidden.get() - usize::from(item.meta().hidden));
        if let Some(position) = self.store.find(&item) {
            self.store.remove(position);
        }
        was_selected
    }

    /// Replaces `from` with `to`, keeping the item selected if it was.
    pub fn rename(&self, from: &str, to: FileMeta) {
        let uri = to.uri.clone();
        let was_selected = self.remove(from);
        self.upsert(to);
        if was_selected && let Some(position) = self.position_of(&uri) {
            self.selection.select_item(position, false);
        }
    }

    /// Shows or hides hidden files.
    pub fn set_show_hidden(&self, show: bool) {
        if self.state.show_hidden.replace(show) != show {
            self.filter.changed(if show {
                gtk::FilterChange::LessStrict
            } else {
                gtk::FilterChange::MoreStrict
            });
        }
    }

    /// Whether hidden files are shown.
    pub fn show_hidden(&self) -> bool {
        self.state.show_hidden.get()
    }

    /// Sets or clears the name filter.
    pub fn set_glob(&self, pattern: Option<String>) {
        let pattern = pattern.filter(|p| !p.is_empty());
        if *self.state.glob.borrow() != pattern {
            self.state.glob.replace(pattern);
            self.filter.changed(gtk::FilterChange::Different);
        }
    }

    /// The active name filter.
    pub fn glob(&self) -> Option<String> {
        self.state.glob.borrow().clone()
    }

    /// Sets or clears the `f` filter. Unlike the glob it also hides folders.
    pub fn set_needle(&self, needle: Option<String>) {
        let needle = needle.filter(|n| !n.is_empty());
        if *self.state.needle.borrow() != needle {
            self.state.needle.replace(needle);
            self.filter.changed(gtk::FilterChange::Different);
        }
    }

    /// The active `f` filter.
    pub fn needle(&self) -> Option<String> {
        self.state.needle.borrow().clone()
    }

    /// Changes the sort order.
    pub fn set_sort(&self, sort: SortSpec) {
        if *self.state.sort.borrow() != sort {
            self.state.sort.replace(sort);
            self.sorter.changed(gtk::SorterChange::Different);
        }
    }

    /// The active sort order.
    pub fn sort(&self) -> SortSpec {
        self.state.sort.borrow().clone()
    }

    /// Number of items shown.
    pub fn visible_count(&self) -> usize {
        self.selection.n_items() as usize
    }

    /// Number of items in the folder, shown or not.
    pub fn total_count(&self) -> usize {
        self.store.n_items() as usize
    }

    /// Number of hidden files that are currently not shown.
    pub fn hidden_count(&self) -> usize {
        if self.state.show_hidden.get() {
            0
        } else {
            self.hidden.get()
        }
    }

    /// The item at a view position.
    pub fn item_at(&self, position: u32) -> Option<FileItem> {
        self.selection.item(position).and_downcast()
    }

    /// View position of the item with this URI, if it is shown.
    pub fn position_of(&self, uri: &str) -> Option<u32> {
        let item = self.by_uri.borrow().get(uri)?;
        self.position_of_item(&item)
    }

    /// View position of an item, if it is shown.
    pub fn position_of_item(&self, item: &FileItem) -> Option<u32> {
        (0..self.selection.n_items())
            .find(|position| self.selection.item(*position).as_ref() == Some(item.upcast_ref()))
    }

    /// Names in view order, for the find commands.
    pub fn names(&self) -> Vec<String> {
        (0..self.selection.n_items())
            .filter_map(|position| self.item_at(position))
            .map(|item| item.meta().display_name.clone())
            .collect()
    }

    /// Brings every item's yank mark in line with the yanked set.
    pub fn refresh_yank(&self) {
        for item in self.store.iter::<FileItem>().flatten() {
            item.refresh_yank();
        }
    }

    /// Selected items in view order.
    pub fn selected(&self) -> Vec<FileItem> {
        let bitset = self.selection.selection();
        (0..bitset.size())
            .filter_map(|index| self.item_at(bitset.nth(index as u32)))
            .collect()
    }

    /// Number of selected items, without materialising them.
    pub fn selected_count(&self) -> usize {
        self.selection.selection().size() as usize
    }
}
