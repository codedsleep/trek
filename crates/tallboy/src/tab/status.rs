//! Status-bar text for a tab (PROMPT §8.26).

use super::TabPage;
use tallboy_core::format;

/// The two halves of the status bar.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub left: String,
    pub right: String,
}

impl TabPage {
    /// Text for the status bar: counts or the selection on the left, free
    /// space on the right when the filesystem reports it.
    pub fn status(&self) -> Status {
        let inner = &self.0;
        let si = inner.app.settings().si_unit();
        let model = &inner.model;
        let left = match model.selected_count() {
            0 => format::status(
                model.visible_count(),
                model.hidden_count(),
                model.glob().as_deref(),
            ),
            1 => model.selected().first().map_or_else(String::new, |item| {
                let meta = item.meta();
                if item.is_dir() {
                    format!("\"{}\" {}", meta.display_name, meta.description)
                } else {
                    format::selected_one(&meta.display_name, meta.size, &meta.description, si)
                }
            }),
            count => {
                let bytes = model
                    .selected()
                    .iter()
                    .filter(|item| !item.is_dir())
                    .map(|item| item.meta().size)
                    .sum();
                format::selected_many(count, bytes, si)
            }
        };
        let info = inner.fs_info.borrow();
        let right = match (info.free, info.total) {
            (Some(free), Some(total)) => format::free_space(free, total, si),
            _ => String::new(),
        };
        Status { left, right }
    }
}
