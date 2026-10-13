//! What the status line says about a tab.

use super::{Mode, TabPage};
use crate::view::item;
use tallboy_core::{format, keymap::Linemode};

/// The parts of the status line that come from the tab.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Status {
    pub mode: Mode,
    /// The file under the cursor, or the selection when there is one.
    pub left: String,
    /// Filter, selection and yank counts, and whatever else is in force.
    pub segments: Vec<String>,
    /// The cursor's position among the files shown: `12/348`.
    pub position: String,
    /// Free space, when the filesystem reports it.
    pub right: String,
}

impl TabPage {
    /// Text for the status line.
    pub fn status(&self) -> Status {
        let inner = &self.0;
        let si = inner.app.settings().si_unit();
        let model = &inner.model;
        let selected = model.selected_count();
        let left = match (selected, self.cursor_item()) {
            (0, Some(item)) => {
                let meta = item.meta();
                let mut text = if item.is_dir() {
                    format!("{}  {}", meta.display_name, meta.description)
                } else {
                    format!(
                        "{}  {}  {}",
                        meta.display_name,
                        format::size(meta.size, si),
                        meta.description
                    )
                };
                if let Some(target) = meta.symlink_target.as_deref().filter(|_| meta.symlink) {
                    text.push_str(&format!("  → {}", target.display()));
                }
                text.push_str(&format!("  {}", format::permissions(meta.mode)));
                text
            }
            (0, None) => format::status(
                model.visible_count(),
                model.hidden_count(),
                model.glob().as_deref(),
            ),
            (1, _) => model.selected().first().map_or_else(String::new, |item| {
                let meta = item.meta();
                if item.is_dir() {
                    format!("\"{}\" {}", meta.display_name, meta.description)
                } else {
                    format::selected_one(&meta.display_name, meta.size, &meta.description, si)
                }
            }),
            (count, _) => {
                let bytes = model
                    .selected()
                    .iter()
                    .filter(|item| !item.is_dir())
                    .map(|item| item.meta().size)
                    .sum();
                format::selected_many(count, bytes, si)
            }
        };

        let mut segments = Vec::new();
        if let Some(needle) = model.needle() {
            segments.push(format!("filter: {needle}"));
        }
        if let Some(glob) = model.glob() {
            segments.push(format!("pattern: {glob}"));
        }
        if selected > 0 {
            segments.push(format!("{selected} selected"));
        }
        if let Some((count, cut)) = item::yanked() {
            segments.push(format!("{count} {}", if cut { "cut" } else { "yanked" }));
        }
        if model.hidden_count() > 0 {
            segments.push(format!("{} hidden", model.hidden_count()));
        }
        if inner.linemode.get() != Linemode::None {
            segments.push(inner.linemode.get().name().to_owned());
        }

        let position = match (self.cursor(), model.visible_count()) {
            (Some(at), count) if count > 0 => format!("{}/{count}", at + 1),
            _ => "0/0".to_owned(),
        };
        let info = inner.fs_info.borrow();
        let right = match (info.free, info.total) {
            (Some(free), Some(total)) => format::free_space(free, total, si),
            _ => String::new(),
        };
        Status {
            mode: self.vim_mode(),
            left,
            segments,
            position,
            right,
        }
    }
}
