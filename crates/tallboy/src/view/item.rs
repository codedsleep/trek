//! `FileItem`: the only item type in a folder's list store.

use gtk::{gio, glib, subclass::prelude::*};
use std::{
    cell::{Ref, RefCell},
    collections::{HashMap, VecDeque},
};
use tallboy_fs::meta::FileMeta;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct FileItem {
        pub meta: RefCell<Option<FileMeta>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileItem {
        const NAME: &'static str = "TallboyFileItem";
        type Type = super::FileItem;
    }

    impl ObjectImpl for FileItem {}
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
        item.imp().meta.replace(Some(meta));
        item
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
    use gtk::prelude::*;

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
