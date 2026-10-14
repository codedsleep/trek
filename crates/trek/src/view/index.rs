//! URI → item lookup for monitor events.
//!
//! Keyed by a 64-bit hash of the URI rather than the URI itself: a second
//! copy of every URI would cost about a megabyte per 10,000 files. A hit is
//! always confirmed against the item's own URI, and the rare colliding item
//! goes to an overflow list, so a collision can never return the wrong file.

use super::FileItem;
use std::{
    collections::{HashMap, hash_map::Entry},
    hash::{DefaultHasher, Hash, Hasher},
};

#[derive(Default)]
pub struct UriIndex {
    by_hash: HashMap<u64, FileItem>,
    overflow: Vec<FileItem>,
}

fn hash(uri: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    uri.hash(&mut hasher);
    hasher.finish()
}

impl UriIndex {
    /// Adds `item`, replacing an entry with the same URI.
    pub fn insert(&mut self, item: FileItem) {
        let key = hash(&item.meta().uri);
        match self.by_hash.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(item);
            }
            Entry::Occupied(mut slot) => {
                if slot.get().meta().uri == item.meta().uri {
                    slot.insert(item);
                } else {
                    self.overflow
                        .retain(|other| other.meta().uri != item.meta().uri);
                    self.overflow.push(item);
                }
            }
        }
    }

    /// The item with this URI.
    pub fn get(&self, uri: &str) -> Option<FileItem> {
        self.by_hash
            .get(&hash(uri))
            .filter(|item| item.meta().uri == uri)
            .or_else(|| self.overflow.iter().find(|item| item.meta().uri == uri))
            .cloned()
    }

    /// Removes and returns the item with this URI.
    pub fn remove(&mut self, uri: &str) -> Option<FileItem> {
        let key = hash(uri);
        if self
            .by_hash
            .get(&key)
            .is_some_and(|item| item.meta().uri == uri)
        {
            return self.by_hash.remove(&key);
        }
        let position = self
            .overflow
            .iter()
            .position(|item| item.meta().uri == uri)?;
        Some(self.overflow.swap_remove(position))
    }

    pub fn clear(&mut self) {
        self.by_hash = HashMap::new();
        self.overflow = Vec::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gio;
    use trek_fs::meta::FileMeta;

    fn item(dir: &std::path::Path, name: &str) -> FileItem {
        let path = dir.join(name);
        std::fs::write(&path, b"x").expect("temp file");
        let meta = FileMeta::query(&gio::File::for_path(&path), true, false, None).expect("meta");
        FileItem::new(meta)
    }

    #[test]
    fn insert_get_replace_remove() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut index = UriIndex::default();
        let (a, b) = (item(dir.path(), "a"), item(dir.path(), "b"));
        let (a_uri, b_uri) = (a.meta().uri.clone(), b.meta().uri.clone());
        index.insert(a.clone());
        index.insert(b);
        assert_eq!(index.get(&a_uri), Some(a));
        assert!(index.get("file:///nowhere").is_none());
        let replacement = item(dir.path(), "a");
        index.insert(replacement.clone());
        assert_eq!(index.get(&a_uri), Some(replacement));
        assert!(index.remove(&a_uri).is_some());
        assert!(index.get(&a_uri).is_none());
        assert!(index.remove(&a_uri).is_none());
        assert!(index.get(&b_uri).is_some());
        index.clear();
        assert!(index.get(&b_uri).is_none());
    }

    #[test]
    fn colliding_hashes_never_return_the_wrong_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (a, b) = (item(dir.path(), "a"), item(dir.path(), "b"));
        let (a_uri, b_uri) = (a.meta().uri.clone(), b.meta().uri.clone());
        // Force both items under one key, as a real collision would.
        let mut index = UriIndex::default();
        index.by_hash.insert(hash(&b_uri), a.clone());
        assert!(index.get(&b_uri).is_none());
        index.insert(b.clone());
        assert_eq!(index.get(&b_uri), Some(b));
        assert_eq!(
            index.remove(&b_uri).map(|i| i.meta().uri.clone()),
            Some(b_uri.clone())
        );
        assert!(index.get(&b_uri).is_none());
        let _ = a_uri;
    }
}
