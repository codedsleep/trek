//! The columns beside a tab's files: the parent folder on the left and the
//! preview of what is under the cursor on the right, in Yazi's 1 : 4 : 3.

use super::{TabPage, location_from_uri};
use gtk::{gio, prelude::*};
use trek_fs::folder::Options;

impl TabPage {
    /// Shows or hides the two side columns. A hidden column loads nothing.
    pub fn set_columns_shown(&self, parent: bool, preview: bool) {
        let inner = &self.0;
        if inner.columns_shown.replace((parent, preview)) == (parent, preview) {
            return;
        }
        inner.parent.widget().set_visible(parent);
        inner.preview.widget().set_visible(preview);
        self.update_parent();
        self.update_preview();
    }

    /// How every column of the tab lists a folder. They are the same for
    /// all three, so that a listing can move from one column to the next.
    pub(super) fn listing_options(&self) -> Options {
        let settings = self.0.app.settings();
        Options {
            backup_as_hidden: settings.backup_as_hidden(),
            defer_content_test: settings.defer_content_test(),
            ..Options::default()
        }
    }

    /// Wires clicks in the side columns: a click goes where it points.
    pub(super) fn connect_columns(&self) {
        let weak = self.downgrade();
        self.0.parent.connect_activate(move |item| {
            let Some(tab) = weak.upgrade() else { return };
            let uri = item.meta().uri.clone();
            if item.is_dir() {
                if let Some(location) = location_from_uri(&uri) {
                    tab.navigate(location);
                }
            } else {
                tab.go_up();
                tab.select_when_listed(&uri);
            }
        });
        let weak = self.downgrade();
        self.0.preview.folder().connect_activate(move |item| {
            let Some(tab) = weak.upgrade() else { return };
            let folder = tab.0.preview.folder().shown();
            if let Some(location) = folder.as_deref().and_then(location_from_uri) {
                let uri = item.meta().uri.clone();
                tab.navigate(location);
                tab.select_when_listed(&uri);
            }
        });
    }

    /// The parent column lists the folder above this one and marks this one
    /// in it. At a root it is empty.
    pub(super) fn update_parent(&self) {
        let inner = &self.0;
        if !inner.columns_shown.get().0 {
            inner.parent.clear();
            return;
        }
        let here = gio::File::for_uri(inner.location.borrow().to_uri());
        match here.parent() {
            Some(parent) => {
                inner
                    .parent
                    .show(&parent.uri(), Some(&here.uri()), self.listing_options())
            }
            None => inner.parent.clear(),
        }
    }

    /// The preview follows the cursor.
    pub(super) fn update_preview(&self) {
        let inner = &self.0;
        if !inner.columns_shown.get().1 {
            inner.preview.clear();
            return;
        }
        inner
            .preview
            .show(self.cursor_item().as_ref(), self.listing_options());
    }

    /// The side columns list in the tab's order, hidden files included or
    /// not as the tab has them.
    pub(super) fn sync_side_views(&self) {
        let (sort, hidden) = (self.0.model.sort(), self.0.model.show_hidden());
        self.0.parent.set_view(sort.clone(), hidden);
        self.0.preview.folder().set_view(sort, hidden);
    }

    /// Brings the yank marks of all three columns in line with the yanked
    /// set. Folders the tab keeps without showing them catch up when shown.
    pub fn refresh_yank(&self) {
        self.0.model.refresh_yank();
        self.0.parent.refresh_yank();
        self.0.preview.folder().refresh_yank();
    }

    /// `J` and `K`: scrolls the preview by `by` lines.
    pub fn seek_preview(&self, by: i32) {
        self.0.preview.seek(by);
    }

    /// What the side columns show, for agent checks.
    pub(super) fn columns_debug(&self) -> String {
        let (parent, preview) = self.0.columns_shown.get();
        format!(
            "parent={} preview={}",
            if parent {
                self.0
                    .parent
                    .shown()
                    .map(|uri| crate::ops::display(&uri))
                    .unwrap_or_else(|| "empty".into())
            } else {
                "off".into()
            },
            if preview {
                self.0.preview.debug_state()
            } else {
                "off".into()
            },
        )
    }
}
