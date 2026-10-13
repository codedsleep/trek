//! The preview column: what is under the cursor, shown without opening it.
//!
//! A folder is listed, a text file shows its first lines, an image is drawn,
//! anything else gets a card with what the listing already knows about it.
//! Only local files are read; nothing is mounted, asked for or executed, and
//! a file that is not a regular file is never opened at all.
//!
//! The cursor moves faster than a disk answers, so a preview is asked for
//! only once the cursor has rested for a moment, and an answer for a file
//! the cursor has left is dropped.

use crate::view::{
    FileItem, Style,
    mini::{self, MiniColumn},
};
use gtk::{gdk, gdk_pixbuf, gio, glib, pango, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tallboy_core::{format, sort::SortSpec};
use tallboy_fs::{folder::Options, meta::FileMeta};

/// How long the cursor rests on a file before it is previewed.
const REST: Duration = Duration::from_millis(70);
/// How much of a text file is read.
const TEXT_BYTES: usize = 64 * 1024;
const TEXT_LINES: usize = 400;
const TEXT_COLUMNS: usize = 300;
/// Images larger than this are not decoded for a preview.
const IMAGE_BYTES: u64 = 64 * 1024 * 1024;
/// The longest side an image is decoded to.
const IMAGE_PIXELS: i32 = 1280;

const EMPTY: &str = "empty";
const FOLDER: &str = "folder";
const TEXT: &str = "text";
const IMAGE: &str = "image";
const INFO: &str = "info";

/// Content types that are text without descending from `text/plain`.
const TEXT_TYPES: &[&str] = &[
    "application/json",
    "application/xml",
    "application/toml",
    "application/yaml",
    "application/x-yaml",
    "application/javascript",
    "application/x-shellscript",
    "application/x-desktop",
    "application/x-zerosize",
    "application/sql",
    "application/x-perl",
    "application/x-php",
    "application/x-ruby",
    "application/x-wine-extension-ini",
];

fn is_text(content_type: &str) -> bool {
    gio::content_type_is_a(content_type, "text/plain") || TEXT_TYPES.contains(&content_type)
}

/// Decoded pixels, in a form that can leave the worker thread.
struct Pixels {
    bytes: glib::Bytes,
    width: i32,
    height: i32,
    stride: usize,
    alpha: bool,
}

fn decode(path: &std::path::Path) -> Option<Pixels> {
    let (_, width, height) = gdk_pixbuf::Pixbuf::file_info(path)?;
    let pixbuf = if width > IMAGE_PIXELS || height > IMAGE_PIXELS {
        gdk_pixbuf::Pixbuf::from_file_at_scale(path, IMAGE_PIXELS, IMAGE_PIXELS, true).ok()?
    } else {
        gdk_pixbuf::Pixbuf::from_file(path).ok()?
    };
    let pixbuf = pixbuf.apply_embedded_orientation().unwrap_or(pixbuf);
    Some(Pixels {
        bytes: pixbuf.read_pixel_bytes(),
        width: pixbuf.width(),
        height: pixbuf.height(),
        stride: pixbuf.rowstride() as usize,
        alpha: pixbuf.has_alpha(),
    })
}

/// The first lines of `bytes` as text, or `None` when it is not text.
fn text_of(bytes: &[u8]) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut shown = String::new();
    for line in text.lines().take(TEXT_LINES) {
        // A minified file is one line of megabytes.
        shown.extend(line.chars().take(TEXT_COLUMNS));
        shown.push('\n');
    }
    Some(shown)
}

struct Inner {
    stack: gtk::Stack,
    folder: MiniColumn,
    text: gtk::Label,
    text_scroller: gtk::ScrolledWindow,
    picture: gtk::Picture,
    info_icon: gtk::Image,
    info_name: gtk::Label,
    info_lines: gtk::Label,
    /// URI, modification time and size of what is shown or being loaded.
    shown: RefCell<Option<(String, i64, u64)>>,
    generation: Cell<u64>,
    rest: RefCell<Option<glib::SourceId>>,
    si_unit: Cell<bool>,
}

/// The preview column. Clones share the same column.
#[derive(Clone)]
pub struct Preview(Rc<Inner>);

impl Preview {
    pub fn new(style: Style, sort: SortSpec, show_hidden: bool) -> Self {
        let folder = MiniColumn::new(style, sort, show_hidden);
        let text = gtk::Label::new(None);
        text.set_xalign(0.0);
        text.set_yalign(0.0);
        text.add_css_class("preview-text");
        let text_scroller = gtk::ScrolledWindow::new();
        text_scroller.set_child(Some(&text));
        let picture = gtk::Picture::new();
        picture.set_can_shrink(true);
        picture.set_content_fit(gtk::ContentFit::ScaleDown);
        let info_icon = gtk::Image::new();
        info_icon.set_pixel_size(96);
        let info_name = gtk::Label::new(None);
        info_name.add_css_class("heading");
        info_name.set_wrap(true);
        info_name.set_wrap_mode(pango::WrapMode::WordChar);
        info_name.set_justify(gtk::Justification::Center);
        info_name.set_max_width_chars(30);
        let info_lines = gtk::Label::new(None);
        info_lines.set_justify(gtk::Justification::Center);
        info_lines.set_wrap(true);
        info_lines.set_wrap_mode(pango::WrapMode::WordChar);
        info_lines.set_max_width_chars(34);
        info_lines.add_css_class("line-detail");
        let info = gtk::Box::new(gtk::Orientation::Vertical, 10);
        info.add_css_class("preview-info");
        info.set_valign(gtk::Align::Center);
        info.append(&info_icon);
        info.append(&info_name);
        info.append(&info_lines);
        // Scrolled, so that the card's height is not the window's minimum.
        let info_scroller = gtk::ScrolledWindow::new();
        info_scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        info_scroller.set_child(Some(&info));

        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.set_vexpand(true);
        stack.add_css_class("preview");
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(EMPTY));
        stack.add_named(folder.widget(), Some(FOLDER));
        stack.add_named(&text_scroller, Some(TEXT));
        stack.add_named(&picture, Some(IMAGE));
        stack.add_named(&info_scroller, Some(INFO));
        Self(Rc::new(Inner {
            stack,
            folder,
            text,
            text_scroller,
            picture,
            info_icon,
            info_name,
            info_lines,
            shown: RefCell::default(),
            generation: Cell::new(0),
            rest: RefCell::default(),
            si_unit: Cell::new(style.si_unit),
        }))
    }

    pub fn widget(&self) -> &gtk::Stack {
        &self.0.stack
    }

    /// The folder listing inside the preview, for its click handler and for
    /// following the tab's sort order.
    pub fn folder(&self) -> &MiniColumn {
        &self.0.folder
    }

    /// What is on show, for agent checks.
    pub fn debug_state(&self) -> String {
        let page = self.0.stack.visible_child_name().unwrap_or_default();
        let of = self
            .0
            .shown
            .borrow()
            .as_ref()
            .map(|(uri, _, _)| uri.rsplit('/').next().unwrap_or_default().to_owned())
            .unwrap_or_default();
        format!("{page}:{of}")
    }

    /// Shows nothing and forgets what was shown.
    pub fn clear(&self) {
        let inner = &self.0;
        if let Some(timer) = inner.rest.take() {
            timer.remove();
        }
        inner.generation.set(inner.generation.get() + 1);
        inner.shown.replace(None);
        inner.folder.clear();
        inner.picture.set_paintable(gdk::Paintable::NONE);
        inner.text.set_text("");
        inner.stack.set_visible_child_name(EMPTY);
    }

    /// Previews `item` once the cursor has rested on it.
    pub fn show(&self, item: Option<&FileItem>, options: Options) {
        let inner = &self.0;
        let Some(item) = item else {
            self.clear();
            return;
        };
        let meta = item.meta().clone();
        let key = (meta.uri.clone(), meta.mtime, meta.size);
        if inner.shown.borrow().as_ref() == Some(&key) {
            return;
        }
        inner.shown.replace(Some(key));
        inner.generation.set(inner.generation.get() + 1);
        let generation = inner.generation.get();
        let icon = item.icon();
        let weak = Rc::downgrade(&self.0);
        let timer = glib::timeout_add_local_once(REST, move || {
            let Some(this) = weak.upgrade().map(Self) else {
                return;
            };
            this.0.rest.replace(None);
            if this.0.generation.get() == generation {
                this.load(meta, icon, options, generation);
            }
        });
        if let Some(old) = inner.rest.replace(Some(timer)) {
            old.remove();
        }
    }

    fn load(&self, meta: FileMeta, icon: gio::Icon, options: Options, generation: u64) {
        let inner = &self.0;
        let file = gio::File::for_uri(&meta.uri);
        let local = file.is_native();
        let in_trash = meta.uri.starts_with("trash://");
        if meta.file_type == gio::FileType::Directory && (local || in_trash) {
            // A trashed folder is listed through its own URI; a link to a
            // folder through the link, as entering it would.
            inner.folder.show(&meta.uri, None, options);
            inner.stack.set_visible_child_name(FOLDER);
            return;
        }
        inner.folder.clear();
        self.show_info(&meta, &icon);
        let Some(path) = file
            .path()
            .filter(|_| local && meta.file_type == gio::FileType::Regular)
        else {
            return;
        };
        if meta.content_type.starts_with("image/") && meta.size <= IMAGE_BYTES {
            let weak = Rc::downgrade(&self.0);
            glib::spawn_future_local(async move {
                let pixels = gio::spawn_blocking(move || decode(&path)).await;
                let Some(inner) = weak.upgrade() else { return };
                if inner.generation.get() != generation {
                    return;
                }
                if let Ok(Some(pixels)) = pixels {
                    let texture = gdk::MemoryTexture::new(
                        pixels.width,
                        pixels.height,
                        if pixels.alpha {
                            gdk::MemoryFormat::R8g8b8a8
                        } else {
                            gdk::MemoryFormat::R8g8b8
                        },
                        &pixels.bytes,
                        pixels.stride,
                    );
                    inner.picture.set_paintable(Some(&texture));
                    inner.stack.set_visible_child_name(IMAGE);
                }
            });
        } else if is_text(&meta.content_type) && meta.size > 0 {
            let weak = Rc::downgrade(&self.0);
            glib::spawn_future_local(async move {
                let bytes = async {
                    let stream = file.read_future(glib::Priority::DEFAULT).await.ok()?;
                    stream
                        .read_bytes_future(TEXT_BYTES, glib::Priority::DEFAULT)
                        .await
                        .ok()
                }
                .await;
                let Some(inner) = weak.upgrade() else { return };
                if inner.generation.get() != generation {
                    return;
                }
                if let Some(text) = bytes.as_deref().and_then(text_of) {
                    inner.text.set_text(&text);
                    inner.text_scroller.vadjustment().set_value(0.0);
                    inner.text_scroller.hadjustment().set_value(0.0);
                    inner.stack.set_visible_child_name(TEXT);
                }
            });
        }
    }

    /// The card every file gets, and the only preview of one that is not
    /// read: what the listing already knows.
    fn show_info(&self, meta: &FileMeta, icon: &gio::Icon) {
        let inner = &self.0;
        inner.info_icon.set_from_gicon(icon);
        inner.info_name.set_text(&meta.display_name);
        let mut lines = vec![meta.description.to_string()];
        if meta.file_type != gio::FileType::Directory {
            lines.push(format::size(meta.size, inner.si_unit.get()));
        }
        if let Some(modified) = glib::DateTime::from_unix_local(meta.mtime)
            .ok()
            .filter(|_| meta.mtime > 0)
            .and_then(|time| time.format("%Y-%m-%d %H:%M").ok())
        {
            lines.push(format!("Modified {modified}"));
        }
        let mut owner = format::permissions(meta.mode);
        if !meta.owner.is_empty() {
            owner.push_str(&format!("  {}:{}", meta.owner, meta.group));
        }
        lines.push(owner);
        if let Some(target) = meta.symlink_target.as_deref().filter(|_| meta.symlink) {
            lines.push(format!("→ {}", target.display()));
        }
        lines.retain(|line| !line.is_empty());
        inner.info_lines.set_text(&lines.join("\n"));
        inner.stack.set_visible_child_name(INFO);
    }

    /// `J` and `K`: scrolls what is shown by `lines` lines.
    pub fn seek(&self, lines: i32) {
        let inner = &self.0;
        match inner.stack.visible_child_name().as_deref() {
            Some(FOLDER) => inner.folder.scroll_by(lines),
            Some(TEXT) => mini::scroll_lines(&inner.text_scroller, lines),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cut_to_what_a_column_can_show() {
        assert_eq!(text_of(b"one\ntwo\n").as_deref(), Some("one\ntwo\n"));
        assert_eq!(text_of(b"\x7fELF\0\0").as_deref(), None);
        let long = "x".repeat(TEXT_COLUMNS * 3);
        assert_eq!(
            text_of(long.as_bytes()).map(|text| text.chars().count()),
            Some(TEXT_COLUMNS + 1)
        );
        let many = "line\n".repeat(TEXT_LINES * 2);
        assert_eq!(
            text_of(many.as_bytes()).map(|text| text.lines().count()),
            Some(TEXT_LINES)
        );
        // Bytes that are not UTF-8 are shown as best they can be.
        assert!(text_of(b"caf\xe9\n").is_some());
    }
}
