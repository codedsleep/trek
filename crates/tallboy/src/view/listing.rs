//! One folder's files, read once and kept current, whichever column shows
//! them.
//!
//! A `Listing` owns the folder's load and monitor and everything computed
//! from them: the items, the filter and the sorted order. Entering a folder
//! moves the listing the preview column was showing into the files column,
//! and the one the files column was showing into the parent column; nothing
//! is read from disk again and nothing is sorted again. Listings no column
//! shows wait in the tab's `ListingCache`, still following their folders.

use super::{FileItem, index::UriIndex, item};
use crate::debug;
use gtk::{gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::Duration,
};
use tallboy_core::{
    glob, keymap,
    sort::{Column, SortSpec},
};
use tallboy_fs::{
    folder::{Folder, FolderError, FolderEvent, FsInfo, Options},
    meta::{self, FileMeta},
};

/// Files appended to the model in one go. The worker thread reads faster
/// than this, so batches that have queued up are joined up to this size.
const CHUNK: usize = 1000;
/// Files freed per idle turn when a large listing is thrown away.
const DISCARD_CHUNK: u32 = 4000;
/// How many listings a tab keeps beside the ones its columns show.
const KEPT_FOLDERS: usize = 12;
/// How many files those may hold between them.
const KEPT_FILES: usize = 100_000;

/// How a column shows a listing: its order and what it leaves out.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub sort: SortSpec,
    pub show_hidden: bool,
    /// The name filter; folders always pass it (§8.7).
    pub glob: Option<String>,
    /// The `f` filter: a smart-case substring every item must contain.
    pub needle: Option<String>,
}

/// Where a listing is with reading its folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Loaded,
    /// The folder could not be read at all.
    Failed,
    /// The folder was deleted or unmounted, or there never was one.
    Gone,
}

/// What a listing tells the column showing it, after it has applied the
/// change to its own items.
pub enum Notice<'a> {
    /// Files of the initial load were appended.
    Batch,
    /// The initial load is complete.
    Finished,
    /// The folder reported an error; `phase` says whether it was fatal.
    Error(&'a FolderError),
    /// This item is about to be announced again or renamed; `Listed` follows.
    Replacing(&'a FileItem),
    /// A file was added, changed or renamed to this URI.
    Listed(&'a str),
    /// The filesystem's capacity is known.
    FsInfo,
    /// Files listed by name were read and some turned out to be of another type.
    Typed,
    /// The folder was deleted or unmounted.
    RootGone,
}

pub type Handler = Rc<dyn Fn(Notice<'_>)>;

struct Inner {
    /// `None` for the listing of nothing that an empty column shows.
    uri: Option<String>,
    options: Options,
    store: gio::ListStore,
    filter: gtk::CustomFilter,
    filtered: gtk::FilterListModel,
    sorter: gtk::CustomSorter,
    sorted: gtk::SortListModel,
    shown: Rc<RefCell<Shown>>,
    by_uri: RefCell<UriIndex>,
    hidden: Cell<usize>,
    folder: RefCell<Option<debug::Counted<Folder>>>,
    phase: Cell<Phase>,
    /// The load broke off part of the way; what is listed is not all there is.
    broken: Cell<bool>,
    fs_info: RefCell<FsInfo>,
    handler: RefCell<Option<Handler>>,
    /// `item::yank_changes` when the yank marks were last brought up to date.
    yank_seen: Cell<u64>,
}

/// A folder's items with their filter and order. Clones share the listing.
#[derive(Clone)]
pub struct Listing(Rc<Inner>);

fn file_item(object: &glib::Object) -> Option<&FileItem> {
    object.downcast_ref::<FileItem>()
}

/// Lets the main loop get to everything more urgent than idle work —
/// input, layout, a frame — before the next chunk of a listing is applied.
async fn breathe() {
    glib::timeout_future_with_priority(glib::Priority::DEFAULT_IDLE, Duration::ZERO).await;
}

impl Listing {
    fn build(uri: Option<String>, options: Options, shown: &Shown, phase: Phase) -> Self {
        let shown = Rc::new(RefCell::new(shown.clone()));
        let store = gio::ListStore::new::<FileItem>();
        let filter = gtk::CustomFilter::new({
            let shown = shown.clone();
            move |object| {
                let Some(item) = file_item(object) else {
                    return false;
                };
                let meta = item.meta();
                let shown = shown.borrow();
                if meta.hidden && !shown.show_hidden {
                    return false;
                }
                if let Some(needle) = shown.needle.as_deref()
                    && !keymap::smart_matches(&meta.display_name, needle)
                {
                    return false;
                }
                match shown.glob.as_deref() {
                    // Folders always pass the name filter (§8.7).
                    Some(pattern) if meta.file_type != gio::FileType::Directory => {
                        glob::matches(pattern, &meta.display_name)
                    }
                    _ => true,
                }
            }
        });
        let sorter = gtk::CustomSorter::new({
            let shown = shown.clone();
            move |a, b| match (file_item(a), file_item(b)) {
                (Some(a), Some(b)) => {
                    meta::compare(&a.meta(), &b.meta(), &shown.borrow().sort).into()
                }
                _ => gtk::Ordering::Equal,
            }
        });
        let filtered = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
        let sorted = gtk::SortListModel::new(Some(filtered.clone()), Some(sorter.clone()));
        // Sorting 50,000 items in one go would stall the main loop.
        sorted.set_incremental(true);
        Self(Rc::new(Inner {
            uri,
            options,
            store,
            filter,
            filtered,
            sorter,
            sorted,
            shown,
            by_uri: RefCell::default(),
            hidden: Cell::new(0),
            folder: RefCell::default(),
            phase: Cell::new(phase),
            broken: Cell::new(false),
            fs_info: RefCell::default(),
            handler: RefCell::default(),
            yank_seen: Cell::new(item::yank_changes()),
        }))
    }

    /// The listing of nothing, for a column that shows no folder.
    pub fn empty(shown: &Shown) -> Self {
        Self::build(None, Options::default(), shown, Phase::Gone)
    }

    /// Starts reading and watching `file`. With no mount operation an
    /// unmounted location is an error rather than a question.
    pub fn load(
        file: &gio::File,
        options: Options,
        mount: Option<gio::MountOperation>,
        shown: &Shown,
    ) -> Result<Self, FolderError> {
        let folder = Folder::new(file, options, mount)?;
        let events = folder.events.clone();
        let this = Self::build(Some(file.uri().into()), options, shown, Phase::Loading);
        this.0
            .folder
            .replace(Some(debug::Counted::new(folder, &debug::LIVE_LISTINGS)));
        let weak = Rc::downgrade(&this.0);
        glib::spawn_future_local(async move {
            // Ends when the folder is dropped, which closes its channel.
            while let Ok(event) = events.recv().await {
                let Some(this) = weak.upgrade().map(Self) else {
                    break;
                };
                this.apply(event, || events.try_recv().ok());
                drop(this);
                breathe().await;
            }
        });
        Ok(this)
    }

    pub fn uri(&self) -> Option<&str> {
        self.0.uri.as_deref()
    }

    pub fn options(&self) -> Options {
        self.0.options
    }

    pub fn phase(&self) -> Phase {
        self.0.phase.get()
    }

    pub fn is_loaded(&self) -> bool {
        self.0.phase.get() == Phase::Loaded
    }

    pub fn fs_info(&self) -> FsInfo {
        self.0.fs_info.borrow().clone()
    }

    /// Whether another column may show this listing as it is: its folder is
    /// being, or has been, read in full and is watched for changes. A folder
    /// that cannot be watched is read again by whoever shows it next.
    pub fn reusable(&self) -> bool {
        let inner = &self.0;
        inner.uri.is_some()
            && !inner.broken.get()
            && matches!(inner.phase.get(), Phase::Loading | Phase::Loaded)
            && inner
                .folder
                .borrow()
                .as_ref()
                .is_some_and(|folder| folder.is_watched())
    }

    /// Sets who hears about changes; one column at a time.
    pub fn set_handler(&self, handler: Option<Handler>) {
        self.0.handler.replace(handler);
    }

    fn notify(&self, notice: Notice<'_>) {
        let handler = self.0.handler.borrow().clone();
        if let Some(handler) = handler {
            handler(notice);
        }
    }

    /// The items in the order and with the filters of the column showing them.
    pub fn sorted(&self) -> &gtk::SortListModel {
        &self.0.sorted
    }

    // ---- the folder's events ----

    /// Applies `event` and, when it is a batch of files, the batches
    /// `queued` has behind it, up to a chunk.
    fn apply(&self, event: FolderEvent, mut queued: impl FnMut() -> Option<FolderEvent>) {
        let mut next = Some(event);
        while let Some(event) = next.take() {
            let FolderEvent::Batch(mut batch) = event else {
                self.apply_change(event);
                continue;
            };
            while batch.len() < CHUNK {
                match queued() {
                    Some(FolderEvent::Batch(more)) => batch.extend(more),
                    other => {
                        next = other;
                        break;
                    }
                }
            }
            self.append(batch);
            self.notify(Notice::Batch);
        }
    }

    fn apply_change(&self, event: FolderEvent) {
        let inner = &self.0;
        match event {
            FolderEvent::Started => {}
            FolderEvent::Batch(batch) => self.append(batch),
            FolderEvent::Finished => {
                if inner.phase.get() == Phase::Loading {
                    inner.phase.set(Phase::Loaded);
                }
                self.notify(Notice::Finished);
            }
            FolderEvent::Error(e) => {
                let loading = inner.phase.get() == Phase::Loading;
                if loading && inner.store.n_items() == 0 {
                    inner.phase.set(Phase::Failed);
                } else {
                    inner.broken.set(loading);
                }
                self.notify(Notice::Error(&e));
            }
            FolderEvent::Added(meta) | FolderEvent::Changed(meta) => {
                let uri = meta.uri.clone();
                self.upsert(meta);
                self.notify(Notice::Listed(&uri));
            }
            FolderEvent::Removed(uri) => self.remove(&uri),
            FolderEvent::Renamed { from, to } => {
                let uri = to.uri.clone();
                let renamed = inner.by_uri.borrow().get(&from);
                if let Some(item) = renamed {
                    self.notify(Notice::Replacing(&item));
                }
                self.remove(&from);
                self.upsert(to);
                self.notify(Notice::Listed(&uri));
            }
            FolderEvent::FsInfo(info) => {
                inner.fs_info.replace(info);
                self.notify(Notice::FsInfo);
            }
            FolderEvent::Typed(typed) => {
                let mut changed = false;
                for (uri, facts) in &typed {
                    let item = inner.by_uri.borrow().get(uri);
                    if let Some(item) = item {
                        changed |= item.set_type(facts);
                    }
                }
                if changed {
                    if inner.shown.borrow().sort.column == Column::Description {
                        inner.sorter.changed(gtk::SorterChange::Different);
                    }
                    self.notify(Notice::Typed);
                }
            }
            FolderEvent::RootGone => {
                inner.phase.set(Phase::Gone);
                self.notify(Notice::RootGone);
            }
        }
    }

    /// Appends files of the initial load with a single `splice`.
    fn append(&self, batch: Vec<FileMeta>) {
        let inner = &self.0;
        let mut hidden = inner.hidden.get();
        let mut by_uri = inner.by_uri.borrow_mut();
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
        inner.hidden.set(hidden);
        inner.store.extend_from_slice(&items);
    }

    /// Adds a file reported by the monitor, or updates it when already listed.
    fn upsert(&self, meta: FileMeta) {
        let inner = &self.0;
        let existing = inner.by_uri.borrow().get(&meta.uri);
        let Some(item) = existing else {
            inner
                .hidden
                .set(inner.hidden.get() + usize::from(meta.hidden));
            let item = FileItem::new(meta);
            inner.by_uri.borrow_mut().insert(item.clone());
            inner.store.append(&item);
            return;
        };
        self.notify(Notice::Replacing(&item));
        let was_hidden = item.meta().hidden;
        inner
            .hidden
            .set(inner.hidden.get() - usize::from(was_hidden) + usize::from(meta.hidden));
        item.set_meta(meta);
        if let Some(position) = inner.store.find(&item) {
            // Re-announce the item so that filter, sort and bound cells refresh.
            inner.store.items_changed(position, 1, 1);
        }
    }

    fn remove(&self, uri: &str) {
        let inner = &self.0;
        let Some(item) = inner.by_uri.borrow_mut().remove(uri) else {
            return;
        };
        inner
            .hidden
            .set(inner.hidden.get() - usize::from(item.meta().hidden));
        if let Some(position) = inner.store.find(&item) {
            inner.store.remove(position);
        }
    }

    // ---- how the column shows it ----

    /// Brings order and filters in line with a column. What already agrees
    /// costs nothing, so a listing handed from one column of a tab to the
    /// next is neither filtered nor sorted again.
    pub fn show_as(&self, wanted: &Shown) {
        self.set_show_hidden(wanted.show_hidden);
        self.set_glob(wanted.glob.clone());
        self.set_needle(wanted.needle.clone());
        self.set_sort(wanted.sort.clone());
    }

    pub fn set_show_hidden(&self, show: bool) {
        if std::mem::replace(&mut self.0.shown.borrow_mut().show_hidden, show) != show {
            self.0.filter.changed(if show {
                gtk::FilterChange::LessStrict
            } else {
                gtk::FilterChange::MoreStrict
            });
        }
    }

    pub fn set_glob(&self, pattern: Option<String>) {
        if self.0.shown.borrow().glob != pattern {
            self.0.shown.borrow_mut().glob = pattern;
            self.0.filter.changed(gtk::FilterChange::Different);
        }
    }

    pub fn set_needle(&self, needle: Option<String>) {
        if self.0.shown.borrow().needle != needle {
            self.0.shown.borrow_mut().needle = needle;
            self.0.filter.changed(gtk::FilterChange::Different);
        }
    }

    pub fn set_sort(&self, sort: SortSpec) {
        if self.0.shown.borrow().sort != sort {
            self.0.shown.borrow_mut().sort = sort;
            self.0.sorter.changed(gtk::SorterChange::Different);
        }
    }

    /// Number of items in the folder, shown or not.
    pub fn total_count(&self) -> usize {
        self.0.store.n_items() as usize
    }

    /// Number of hidden files, shown or not.
    pub fn hidden_files(&self) -> usize {
        self.0.hidden.get()
    }

    /// The item with this URI, shown or not.
    pub fn by_uri(&self, uri: &str) -> Option<FileItem> {
        self.0.by_uri.borrow().get(uri)
    }

    /// Brings every item's yank mark in line with the yanked set.
    pub fn refresh_yank(&self) {
        self.0.yank_seen.set(item::yank_changes());
        for item in self.0.store.iter::<FileItem>().flatten() {
            item.refresh_yank();
        }
    }

    /// The same, for a listing that may have missed changes of the yanked
    /// set while no column showed it.
    pub fn catch_up_yank(&self) {
        if self.0.yank_seen.get() != item::yank_changes() {
            self.refresh_yank();
        }
    }

    /// Throws the listing away. Reading and watching stop at once; the
    /// items of a large folder are freed a slice at a time while the main
    /// loop is idle, so that leaving 50,000 files behind delays no frame.
    pub fn discard(self) {
        let inner = &self.0;
        inner.handler.replace(None);
        inner.folder.replace(None);
        inner.phase.set(Phase::Gone);
        if inner.store.n_items() <= DISCARD_CHUNK {
            return;
        }
        // Unhooked from filter and order, the store only has items to free.
        inner.filtered.set_model(gio::ListModel::NONE);
        inner.by_uri.borrow_mut().clear();
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            let left = self.0.store.n_items();
            let slice = left.min(DISCARD_CHUNK);
            self.0.store.splice(left - slice, slice, &[] as &[FileItem]);
            if left > slice {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
}

/// The listings a tab keeps beside the ones its columns show, most recently
/// shown last. Each still follows its folder, so going back to one shows
/// the folder as it is now.
#[derive(Default)]
pub struct ListingCache {
    kept: RefCell<VecDeque<Listing>>,
}

impl ListingCache {
    /// Keeps a listing a column has stopped showing, if it can be shown
    /// again as it is, and lets go of the oldest when there are too many.
    pub fn put(&self, listing: Listing) {
        if !listing.reusable() {
            listing.discard();
            return;
        }
        listing.set_handler(None);
        let mut kept = self.kept.borrow_mut();
        if let Some(same) = kept.iter().position(|old| old.uri() == listing.uri())
            && let Some(old) = kept.remove(same)
        {
            old.discard();
        }
        kept.push_back(listing);
        let mut files: usize = kept.iter().map(Listing::total_count).sum();
        // The newest always stays: it is where the user has just been.
        while kept.len() > 1 && (kept.len() > KEPT_FOLDERS || files > KEPT_FILES) {
            if let Some(oldest) = kept.pop_front() {
                files -= oldest.total_count();
                oldest.discard();
            }
        }
    }

    /// Hands out the listing of `uri` read with `options`, if it is kept
    /// and still good.
    pub fn take(&self, uri: &str, options: Options) -> Option<Listing> {
        let mut kept = self.kept.borrow_mut();
        let position = kept.iter().position(|kept| kept.uri() == Some(uri))?;
        let listing = kept.remove(position)?;
        if listing.reusable() && listing.options() == options {
            Some(listing)
        } else {
            listing.discard();
            None
        }
    }

    /// Whether `take` would find a listing of `uri`.
    pub fn holds(&self, uri: &str, options: Options) -> bool {
        self.kept
            .borrow()
            .iter()
            .any(|kept| kept.uri() == Some(uri) && kept.reusable() && kept.options() == options)
    }

    /// Forgets everything, for a reload: what is shown next is read afresh.
    pub fn clear(&self) {
        let kept = std::mem::take(&mut *self.kept.borrow_mut());
        for listing in kept {
            listing.discard();
        }
    }
}
