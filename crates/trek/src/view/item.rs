//! `FileItem`: the only item type in a folder's list store.

use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, Ref, RefCell},
    collections::{HashMap, HashSet, VecDeque},
    marker::PhantomData,
};
use trek_fs::meta::{self, FileMeta, TypeFacts};

mod imp {
    use super::*;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::FileItem)]
    pub struct FileItem {
        pub meta: RefCell<Option<FileMeta>>,
        /// The cursor is on this item. Cells follow the property, so the
        /// cursor is drawn whether or not the view has the keyboard.
        #[property(get, set, explicit_notify)]
        pub hovered: Cell<bool>,
        /// 0, or [`YANK_COPY`] or [`YANK_CUT`] while the file is yanked.
        #[property(get, set, explicit_notify)]
        pub yank: Cell<u8>,
        /// The icon of the file's type. A folder is listed by name and the
        /// files a name does not settle are read afterwards, so a cell
        /// follows this instead of taking the icon once.
        #[property(get = Self::type_icon)]
        pub gicon: PhantomData<gio::Icon>,
        /// What the file's type is called; it changes with `gicon`.
        #[property(get = Self::type_description)]
        pub description: PhantomData<String>,
    }

    impl FileItem {
        fn type_icon(&self) -> gio::Icon {
            self.obj().icon()
        }

        fn type_description(&self) -> String {
            self.obj().meta().description.to_string()
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileItem {
        const NAME: &'static str = "TrekFileItem";
        type Type = super::FileItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for FileItem {}
}

/// `yank` of a file that `y` put on the clipboard.
pub const YANK_COPY: u8 = 1;
/// `yank` of a file that `x` put on the clipboard.
pub const YANK_CUT: u8 = 2;

/// The files this process yanked, and whether they were cut.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Yanked {
    pub uris: HashSet<String>,
    pub cut: bool,
}

thread_local! {
    /// What `y` or `x` last put on the clipboard, until it is pasted as a
    /// move, unyanked, or replaced by another program's clipboard.
    static YANKED: RefCell<Option<Yanked>> = const { RefCell::new(None) };
    /// Counts the changes of `YANKED`, so that a listing nobody was showing
    /// can tell whether its marks are out of date.
    static YANK_CHANGES: Cell<u64> = const { Cell::new(0) };
}

/// Replaces the yanked set. Listed items are told by their tab.
pub fn set_yanked(yanked: Option<Yanked>) {
    YANKED.set(yanked);
    YANK_CHANGES.set(YANK_CHANGES.get() + 1);
}

/// How often the yanked set has been replaced.
pub fn yank_changes() -> u64 {
    YANK_CHANGES.get()
}

/// How many files are yanked, and whether they were cut.
pub fn yanked() -> Option<(usize, bool)> {
    YANKED.with_borrow(|yanked| yanked.as_ref().map(|y| (y.uris.len(), y.cut)))
}

fn yank_state(uri: &str) -> u8 {
    YANKED.with_borrow(|yanked| match yanked {
        Some(yanked) if yanked.uris.contains(uri) => {
            if yanked.cut {
                YANK_CUT
            } else {
                YANK_COPY
            }
        }
        _ => 0,
    })
}

/// Files read at most when the type of each of several is needed at once.
const SETTLED_AT_ONCE: usize = 2000;

/// `FileItem::settle_type` for files a command is about to act on by type.
/// A huge selection is not read through on the main loop; past the limit
/// files keep the type their names suggest.
pub fn settle_types(items: &[FileItem]) {
    for item in items.iter().take(SETTLED_AT_ONCE) {
        item.settle_type();
    }
}

glib::wrapper! {
    /// One file in a folder: a GObject around one `FileMeta`.
    pub struct FileItem(ObjectSubclass<imp::FileItem>);
}

thread_local! {
    /// Icons are shared between items of the same type, so a folder of
    /// 50,000 text files holds one `GIcon`, not 50,000.
    static ICONS: RefCell<Icons> = RefCell::default();
}

const FALLBACK_ICON: &str = "text-x-generic";
const ICON_LIMIT: usize = 256;

#[derive(Default)]
struct Icons {
    values: HashMap<String, gio::Icon>,
    order: VecDeque<String>,
}

impl Icons {
    fn get(&mut self, key: &str) -> gio::Icon {
        if let Some(icon) = self.values.get(key) {
            return icon.clone();
        }
        if self.values.len() == ICON_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.values.remove(&oldest);
        }
        let icon = gio::Icon::for_string(key)
            .unwrap_or_else(|_| gio::ThemedIcon::new(FALLBACK_ICON).into());
        self.values.insert(key.to_owned(), icon.clone());
        self.order.push_back(key.to_owned());
        icon
    }
}

impl FileItem {
    /// Wraps `meta`.
    pub fn new(meta: FileMeta) -> Self {
        let item: Self = glib::Object::new();
        item.imp().yank.set(yank_state(&meta.uri));
        item.imp().meta.replace(Some(meta));
        item
    }

    /// Puts the cursor on this item or takes it off.
    pub fn hover(&self, hovered: bool) {
        if self.imp().hovered.replace(hovered) != hovered {
            self.notify_hovered();
        }
    }

    /// Brings the yank mark in line with the yanked set.
    pub fn refresh_yank(&self) {
        let state = yank_state(&self.meta().uri);
        if self.imp().yank.replace(state) != state {
            self.notify_yank();
        }
    }

    /// The file's metadata. The borrow must not be held across a call that
    /// can replace it (`set_meta`).
    pub fn meta(&self) -> Ref<'_, FileMeta> {
        Ref::map(self.imp().meta.borrow(), |meta| {
            meta.as_ref()
                .expect("a FileItem holds metadata from construction onward")
        })
    }

    /// Replaces the metadata after a monitor change.
    pub fn set_meta(&self, meta: FileMeta) {
        self.imp().meta.replace(Some(meta));
    }

    /// Takes the type found by reading the file. Returns whether it
    /// differs from the one the name suggested; cells showing the item
    /// follow.
    pub fn set_type(&self, facts: &TypeFacts) -> bool {
        let changed = self
            .imp()
            .meta
            .borrow_mut()
            .as_mut()
            .is_some_and(|meta| meta.set_type(facts));
        if changed {
            self.notify_gicon();
            self.notify_description();
        }
        changed
    }

    /// Makes sure the type is the file's own before something is decided by
    /// it: a file listed by name that has not been read yet is read now.
    /// Only local files, where that is one short read.
    pub fn settle_type(&self) {
        let uri = {
            let meta = self.meta();
            if !meta.type_uncertain {
                return;
            }
            meta.uri.clone()
        };
        let file = gio::File::for_uri(&uri);
        if file.is_native()
            && let Some(facts) = meta::sniff(&file, gio::Cancellable::NONE)
        {
            self.set_type(&facts);
        }
    }

    /// Whether activating the item enters it. A symbolic link to a folder
    /// counts, because listings describe the link's target.
    pub fn is_dir(&self) -> bool {
        self.meta().file_type == gio::FileType::Directory
    }

    /// The themed icon for this file's type.
    pub fn icon(&self) -> gio::Icon {
        let meta = self.meta();
        let key = meta.icon.as_deref().unwrap_or(FALLBACK_ICON);
        ICONS.with_borrow_mut(|icons| icons.get(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_file_icons_do_not_accumulate_after_tabs_close() {
        let mut icons = Icons::default();
        let first = icons.get("/tmp/first-icon.png");
        assert_eq!(first, icons.get("/tmp/first-icon.png"));
        for n in 0..ICON_LIMIT * 2 {
            icons.get(&format!("/tmp/unique-icon-{n}.png"));
        }
        assert_eq!(icons.values.len(), ICON_LIMIT);
        assert_eq!(icons.order.len(), ICON_LIMIT);
        assert_eq!(first.to_string().unwrap().as_str(), "/tmp/first-icon.png");
    }
}
