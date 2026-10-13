//! Navigation history with selection and scroll restoration.
use crate::location::Location;
/// State to restore when revisiting a folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Folder.
    pub location: Location,
    /// Scroll offset.
    pub scroll: f64,
    /// Cursor URI, if any.
    pub cursor: Option<String>,
}
/// Back/forward history, owned by a tab.
#[derive(Debug, Clone, Default)]
pub struct History {
    entries: Vec<Entry>,
    index: usize,
}
impl History {
    /// Current mutable state, for saving scroll before navigation.
    pub fn current_mut(&mut self) -> Option<&mut Entry> {
        self.entries.get_mut(self.index)
    }
    /// Current entry.
    pub fn current(&self) -> Option<&Entry> {
        self.entries.get(self.index)
    }
    /// Navigate, truncating the forward branch. Revisit of current URI is a no-op.
    pub fn navigate(&mut self, location: Location) {
        if self.current().is_some_and(|e| e.location == location) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.index + 1);
        }
        self.entries.push(Entry {
            location,
            scroll: 0.0,
            cursor: None,
        });
        self.index = self.entries.len() - 1;
    }
    /// Every entry, oldest first, for the Back/Forward history menu.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    /// Index of the current entry within [`History::entries`].
    pub fn index(&self) -> usize {
        self.index
    }
    /// Jump straight to an entry chosen from the history menu. An index out
    /// of range leaves the history unchanged and returns `None`.
    pub fn go_to(&mut self, index: usize) -> Option<&Entry> {
        if index < self.entries.len() {
            self.index = index;
            self.current()
        } else {
            None
        }
    }
    /// Whether Back is available.
    pub fn can_back(&self) -> bool {
        !self.entries.is_empty() && self.index > 0
    }
    /// Whether Forward is available.
    pub fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }
    /// Move back, leaving the current entry unchanged at the boundary.
    pub fn back(&mut self) -> Option<&Entry> {
        if self.can_back() {
            self.index -= 1;
            self.current()
        } else {
            None
        }
    }
    /// Move forward.
    pub fn forward(&mut self) -> Option<&Entry> {
        if self.can_forward() {
            self.index += 1;
            self.current()
        } else {
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn branches() {
        let mut h = History::default();
        h.navigate(Location::Uri("file:///a".into()));
        h.current_mut().unwrap().scroll = 42.0;
        h.current_mut().unwrap().cursor = Some("x".into());
        h.navigate(Location::Uri("file:///b".into()));
        assert_eq!(h.back().unwrap().scroll, 42.0);
        assert_eq!(h.current().unwrap().cursor.as_deref(), Some("x"));
        assert!(h.forward().is_some());
        h.back();
        h.navigate(Location::Uri("file:///c".into()));
        assert!(!h.can_forward());
        assert!(h.back().is_some());
        assert!(h.back().is_none());
    }
    #[test]
    fn menu_jump() {
        let mut h = History::default();
        for uri in ["file:///a", "file:///b", "file:///c"] {
            h.navigate(Location::Uri(uri.into()));
        }
        assert_eq!(h.entries().len(), 3);
        assert_eq!(h.index(), 2);
        assert_eq!(h.go_to(0).unwrap().location.to_uri(), "file:///a");
        assert_eq!(h.index(), 0);
        assert!(h.can_forward() && !h.can_back());
        assert!(h.go_to(3).is_none());
        assert_eq!(h.index(), 0);
        // Jumping does not truncate: the forward entries are still there.
        assert_eq!(h.go_to(2).unwrap().location.to_uri(), "file:///c");
    }
}
