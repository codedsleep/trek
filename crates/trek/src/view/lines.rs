//! Line view: one file per row, the way Yazi draws a column.
//!
//! A row is a gutter, the icon, the name, a symbolic link's target and, at
//! the right, whatever the linemode asks for. The cursor and the yank mark
//! are properties of the item, not of the widget that happens to have the
//! keyboard: each row follows them through an expression set up once in
//! `setup`, so nothing is connected in `bind`.
//!
//! The same rows, without gestures, make the parent and preview columns.

use super::{FileItem, FolderModel, Style, dnd, grid, item};
use gtk::{glib, pango, prelude::*};
use trek_core::{format, keymap::Linemode};

/// The widget name a row's content takes while the cursor is on it.
pub const CURSOR: &str = "cursor";

/// Makes `image` follow the icon of the type of the item bound to
/// `list_item`, which changes when a file listed by name is read. `bind`
/// runs after this has set the icon and may put a thumbnail in its place.
fn follow_type(list_item: &impl IsA<glib::Object>, image: &gtk::Image) {
    list_item
        .property_expression("item")
        .chain_property::<FileItem>("gicon")
        .bind(image, "gicon", gtk::Widget::NONE);
}

/// Makes `gutter` show the yank mark of the item bound to `list_item`.
fn follow_yank(list_item: &impl IsA<glib::Object>, gutter: &gtk::Widget) {
    list_item
        .property_expression("item")
        .chain_property::<FileItem>("yank")
        .chain_closure::<String>(glib::closure!(|_: Option<glib::Object>, yank: u8| {
            match yank {
                item::YANK_COPY => "yank-copy",
                item::YANK_CUT => "yank-cut",
                _ => "",
            }
            .to_owned()
        }))
        .bind(gutter, "name", gtk::Widget::NONE);
}

/// Makes `content`, `image` and `gutter` follow the item bound to
/// `list_item`: the cursor, the type's icon and the yank mark. Shared with
/// the grid views, which have no gutter.
pub(super) fn follow_item(
    list_item: &impl IsA<glib::Object>,
    content: &gtk::Box,
    image: &gtk::Image,
    gutter: Option<&gtk::Widget>,
) {
    follow_type(list_item, image);
    list_item
        .property_expression("item")
        .chain_property::<FileItem>("hovered")
        .chain_closure::<String>(glib::closure!(|_: Option<glib::Object>, hovered: bool| {
            if hovered { CURSOR } else { "" }.to_owned()
        }))
        .bind(content, "name", gtk::Widget::NONE);
    if let Some(gutter) = gutter {
        follow_yank(list_item, gutter);
    }
}

/// What the linemode shows for a file.
pub fn detail(file: &FileItem, linemode: Linemode, si_unit: bool) -> String {
    let meta = file.meta();
    let time = |seconds: i64| {
        if seconds <= 0 {
            return String::new();
        }
        glib::DateTime::from_unix_local(seconds)
            .and_then(|time| time.format("%Y-%m-%d %H:%M"))
            .map(String::from)
            .unwrap_or_default()
    };
    match linemode {
        Linemode::None => String::new(),
        // A folder's size on disk says nothing about what is in it.
        Linemode::Size if file.is_dir() => String::new(),
        Linemode::Size => format::size(meta.size, si_unit),
        Linemode::Permissions => format::permissions(meta.mode),
        Linemode::Btime => time(meta.btime),
        Linemode::Mtime => time(meta.mtime),
        Linemode::Owner if meta.group.is_empty() => meta.owner.to_string(),
        Linemode::Owner => format!("{}:{}", meta.owner, meta.group),
    }
}

struct Row {
    content: gtk::Box,
    image: gtk::Image,
}

/// `cursor` is whether the row shows the cursor when it is on its item.
/// The preview column's rows do not: for a moment after a folder is entered
/// it still shows the listing the files column has taken over.
fn setup(list_item: &gtk::ListItem, style: Style, cursor: bool) -> Row {
    let gutter = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gutter.add_css_class("line-gutter");
    let image = grid::icon_image(style);
    let name = gtk::Label::new(None);
    name.set_xalign(0.0);
    name.set_ellipsize(pango::EllipsizeMode::Middle);
    let link = gtk::Label::new(None);
    link.set_xalign(0.0);
    link.set_ellipsize(pango::EllipsizeMode::End);
    link.set_hexpand(true);
    link.add_css_class("line-detail");
    let detail = gtk::Label::new(None);
    detail.set_xalign(1.0);
    detail.add_css_class("line-detail");
    detail.add_css_class("numeric");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.add_css_class("line");
    content.set_hexpand(true);
    for child in [
        gutter.upcast_ref::<gtk::Widget>(),
        image.upcast_ref(),
        name.upcast_ref(),
        link.upcast_ref(),
        detail.upcast_ref(),
    ] {
        content.append(child);
    }
    if cursor {
        follow_item(list_item, &content, &image, Some(gutter.upcast_ref()));
    } else {
        follow_type(list_item, &image);
        follow_yank(list_item, gutter.upcast_ref());
    }
    list_item.set_child(Some(&content));
    Row { content, image }
}

/// The children of a row's content, in the order `setup` appended them.
fn parts(content: &gtk::Box) -> Option<(gtk::Image, gtk::Label, gtk::Label, gtk::Label)> {
    let image = content.first_child()?.next_sibling()?;
    let name = image.next_sibling()?;
    let link = name.next_sibling()?;
    let detail = link.next_sibling()?;
    Some((
        image.downcast().ok()?,
        name.downcast().ok()?,
        link.downcast().ok()?,
        detail.downcast().ok()?,
    ))
}

fn bind(list_item: &gtk::ListItem, style: Style, linemode: Linemode, hooks: Option<&dnd::Hooks>) {
    let (Some(file), Some(content)) = (
        list_item.item().and_downcast::<FileItem>(),
        list_item.child().and_downcast::<gtk::Box>(),
    ) else {
        return;
    };
    let Some((image, name, link, detail)) = parts(&content) else {
        return;
    };
    let meta = file.meta();
    let shown = hooks
        .and_then(|hooks| hooks.thumbs.as_ref())
        .is_some_and(|thumbs| thumbs.bind(list_item, &image, &meta, style.icon_size));
    if !shown {
        image.set_from_gicon(&file.icon());
    }
    name.set_text(&meta.display_name);
    match meta.symlink_target.as_deref().filter(|_| meta.symlink) {
        Some(target) => link.set_text(&format!("→ {}", target.display())),
        None => link.set_text(""),
    }
    detail.set_text(&self::detail(&file, linemode, style.si_unit));
    content.set_tooltip_text(super::trash_tooltip(&meta).as_deref());
    if style.shadow_hidden && meta.hidden {
        content.add_css_class("hidden-item");
    } else {
        content.remove_css_class("hidden-item");
    }
}

/// Builds the line view over a tab's model, with the pointer gestures and
/// drag and drop every folder view has.
pub fn build(
    model: &FolderModel,
    style: Style,
    linemode: Linemode,
    hooks: dnd::Hooks,
) -> gtk::ListView {
    let factory = gtk::SignalListItemFactory::new();
    let selection = model.selection().downgrade();
    let (for_setup, for_bind, for_unbind) = (hooks.clone(), hooks.clone(), hooks.thumbs.clone());
    factory.connect_setup(move |_, object| {
        if let (Some(list_item), Some(selection)) =
            (object.downcast_ref::<gtk::ListItem>(), selection.upgrade())
        {
            let row = setup(list_item, style, true);
            dnd::attach(
                &row.content,
                list_item,
                &selection,
                &row.image,
                style,
                &for_setup,
            );
        }
    });
    factory.connect_bind(move |_, object| {
        if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
            bind(list_item, style, linemode, Some(&for_bind));
        }
    });
    factory.connect_unbind(move |_, object| {
        if let (Some(list_item), Some(thumbs)) =
            (object.downcast_ref::<gtk::ListItem>(), &for_unbind)
        {
            thumbs.unbind(list_item);
        }
    });
    let view = gtk::ListView::new(Some(model.selection().clone()), Some(factory));
    view.set_enable_rubberband(true);
    view.add_css_class("folder-view");
    view.add_css_class("lines-view");
    view
}

/// Builds a column that only shows a folder: no selection, no gestures.
/// A click activates the row. `cursor` as for `setup`.
pub fn build_plain(model: &FolderModel, style: Style, cursor: bool) -> gtk::ListView {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
            setup(list_item, style, cursor);
        }
    });
    factory.connect_bind(move |_, object| {
        if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
            bind(list_item, style, Linemode::None, None);
        }
    });
    // Over the column's selection model, not the folder's own list: that
    // one is replaced whenever the column shows another folder.
    let shown = gtk::NoSelection::new(Some(model.selection().clone()));
    let view = gtk::ListView::new(Some(shown), Some(factory));
    view.set_single_click_activate(true);
    view.set_can_focus(false);
    view.add_css_class("lines-view");
    view.add_css_class("side-column");
    view
}
