//! Folder views: the item object, the shared model stack and the widgets.

pub mod dnd;
pub mod grid;
mod index;
pub mod item;
pub mod keys;
pub mod lines;
pub mod list;
pub mod listing;
pub mod mini;
pub mod model;
pub mod thumbs;

pub use item::FileItem;
pub use model::FolderModel;

use gtk::prelude::*;
use trek_core::config::ViewMode;

/// What a view needs to know to draw its cells and react to the pointer.
/// It is a snapshot: when a setting changes, the tab builds a new view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub mode: ViewMode,
    pub icon_size: u32,
    pub full_names: bool,
    pub shadow_hidden: bool,
    /// A single click opens; hovering selects after `hover_delay_ms`.
    pub single_click: bool,
    pub hover_delay_ms: u32,
    pub si_unit: bool,
}

/// The widget showing a tab's model in its current mode.
pub enum View {
    Grid(gtk::GridView),
    List(list::ListView),
    Lines(gtk::ListView),
}

impl View {
    pub fn widget(&self) -> gtk::Widget {
        match self {
            Self::Grid(grid) => grid.clone().upcast(),
            Self::List(list) => list.widget().clone().upcast(),
            Self::Lines(lines) => lines.clone().upcast(),
        }
    }

    /// Scrolls to an item; `flags` may also select and focus it.
    pub fn scroll_to(&self, position: u32, flags: gtk::ListScrollFlags) {
        match self {
            Self::Grid(grid) => grid.scroll_to(position, flags, None),
            Self::List(list) => list.widget().scroll_to(position, None, flags, None),
            Self::Lines(lines) => lines.scroll_to(position, flags, None),
        }
    }

    pub fn grab_focus(&self) {
        self.widget().grab_focus();
    }
}

/// For an item in the trash can: when it was deleted and from where (§8.16).
pub fn trash_tooltip(meta: &trek_fs::meta::FileMeta) -> Option<String> {
    let deleted = meta.trash_deletion_date.as_deref()?;
    let mut text = format!("Deleted: {}", deleted.replace('T', " "));
    if let Some(original) = &meta.trash_orig_path {
        text.push_str(&format!("\nOriginal location: {}", original.display()));
    }
    Some(text)
}
