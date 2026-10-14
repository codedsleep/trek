//! Icon, thumbnail and compact views: `GridView` with cheap factories.
//!
//! Widgets are built in `setup` and only filled in `bind`. No signal is
//! connected in `bind`; the one thing it starts, a thumbnail request, is
//! cancelled in `unbind`. No cell closure refers to the tab.

use super::{FileItem, FolderModel, Style, dnd};
use gtk::{pango, prelude::*};
use trek_core::config::ViewMode;

/// Lines of the name shown under an icon unless full names are on (§8.6).
const NAME_LINES: i32 = 3;
/// Extra cell width beside the icon, giving 110 px cells at the 48 px default.
const CELL_PADDING: u32 = 62;
/// Width of a compact-view name in `width-chars` units. GridView columns are
/// all the same width, so this is fixed rather than fitted to each column.
const COMPACT_NAME_CHARS: i32 = 20;
/// Columns allowed before the first allocation tells us how many fit.
const INITIAL_COLUMNS: u32 = 8;

fn compact(style: Style) -> bool {
    style.mode == ViewMode::Compact
}

/// Cell geometry derived from the style and the font.
#[derive(Debug, Clone, Copy)]
struct Metrics {
    /// Width of the name, in characters.
    name_chars: i32,
    /// Extent of a cell across the scrolling direction, in pixels.
    extent: f64,
}

impl Metrics {
    fn new(widget: &gtk::GridView, style: Style) -> Self {
        if compact(style) {
            return Self {
                name_chars: COMPACT_NAME_CHARS,
                // Row height: the icon or one line of text.
                extent: f64::from(style.icon_size.max(16)),
            };
        }
        // GtkLabel sizes `width-chars` with the wider of these two.
        let font = widget.pango_context().metrics(None, None);
        let char_width = f64::from(
            font.approximate_char_width()
                .max(font.approximate_digit_width()),
        ) / f64::from(pango::SCALE);
        let wanted = f64::from(style.icon_size.max(48) + CELL_PADDING);
        let name_chars = (wanted / char_width.max(1.0)).round().max(4.0);
        Self {
            name_chars: name_chars as i32,
            extent: (name_chars * char_width).max(f64::from(style.icon_size)),
        }
    }
}

fn setup(
    list_item: &gtk::ListItem,
    style: Style,
    metrics: Metrics,
    selection: &gtk::MultiSelection,
    hooks: &dnd::Hooks,
) {
    let image = icon_image(style);
    let label = gtk::Label::new(None);
    let cell = if compact(style) {
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_width_chars(metrics.name_chars);
        label.set_max_width_chars(metrics.name_chars);
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        cell.set_halign(gtk::Align::Start);
        cell
    } else {
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
        label.set_justify(gtk::Justification::Center);
        // The name's width in characters fixes the cell width. Equal
        // minimum and maximum also keep measuring cheap: a small maximum
        // makes GtkLabel lay the text out one character per line just to
        // find its natural width, which doubled the cost of a screenful.
        label.set_width_chars(metrics.name_chars);
        label.set_max_width_chars(metrics.name_chars);
        if !style.full_names {
            label.set_lines(NAME_LINES);
            label.set_ellipsize(pango::EllipsizeMode::End);
        }
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cell.set_valign(gtk::Align::Start);
        cell.set_halign(gtk::Align::Center);
        cell
    };
    cell.append(&image);
    cell.append(&label);
    dnd::attach(&cell, list_item, selection, &image, style, hooks);
    super::lines::follow_item(list_item, &cell, &image, None);
    list_item.set_child(Some(&cell));
}

/// The image of a cell. It always takes a square of the icon size: a
/// thumbnail keeps its own proportions inside it, so a cell does not change
/// size, and its row does not move, when a thumbnail replaces the icon.
pub(super) fn icon_image(style: Style) -> gtk::Image {
    let image = gtk::Image::new();
    let size = style.icon_size as i32;
    image.set_pixel_size(size);
    image.set_size_request(size, size);
    image
}

/// Shows or hides the symbolic-link emblem on a cell. The emblem widget is
/// created the first time a cell shows a link, so ordinary cells stay at two
/// widgets. It overlaps the icon through a negative margin from `style.css`.
pub(super) fn set_link_emblem(cell: &gtk::Box, image: &gtk::Image, is_link: bool, style: Style) {
    let existing = image
        .next_sibling()
        .filter(|w| w.has_css_class("item-emblem"));
    match (existing, is_link) {
        (Some(emblem), show) => emblem.set_visible(show),
        (None, false) => {}
        (None, true) => {
            let emblem = gtk::Image::from_icon_name("emblem-symbolic-link");
            emblem.add_css_class("item-emblem");
            emblem.set_can_target(false);
            if cell.orientation() == gtk::Orientation::Horizontal {
                emblem.set_pixel_size(12);
                emblem.add_css_class("item-emblem-side");
                emblem.set_valign(gtk::Align::End);
            } else {
                let size = if style.icon_size >= 32 { 16 } else { 8 };
                emblem.set_pixel_size(size);
                emblem.add_css_class(if size == 16 {
                    "item-emblem-16"
                } else {
                    "item-emblem-8"
                });
                // Centred with this margin, its left edge meets the icon's.
                emblem.set_halign(gtk::Align::Center);
                emblem.set_margin_end((style.icon_size as i32 - size).max(0));
            }
            cell.insert_child_after(&emblem, Some(image));
        }
    }
}

fn bind(list_item: &gtk::ListItem, style: Style, hooks: &dnd::Hooks) {
    let (Some(file), Some(cell)) = (
        list_item.item().and_downcast::<FileItem>(),
        list_item.child().and_downcast::<gtk::Box>(),
    ) else {
        return;
    };
    let (Some(image), Some(label)) = (
        cell.first_child().and_downcast::<gtk::Image>(),
        cell.last_child().and_downcast::<gtk::Label>(),
    ) else {
        return;
    };
    let meta = file.meta();
    // A thumbnail in memory is shown at once; otherwise the type icon
    // stands in until the worker delivers one (§8.19).
    let shown = hooks
        .thumbs
        .as_ref()
        .is_some_and(|thumbs| thumbs.bind(list_item, &image, &meta, style.icon_size));
    if !shown {
        image.set_from_gicon(&file.icon());
    }
    label.set_text(&meta.display_name);
    set_link_emblem(&cell, &image, meta.symlink, style);
    cell.set_tooltip_text(super::trash_tooltip(&meta).as_deref());
    if style.shadow_hidden && meta.hidden {
        cell.add_css_class("hidden-item");
    } else {
        cell.remove_css_class("hidden-item");
    }
}

/// Builds a grid over the tab's model.
///
/// `across` is the adjustment for the direction the grid does *not* scroll
/// in: its page size is the space cells are laid out across. `max-columns`
/// follows it, because GridView keeps `max-columns × 8` cells alive around
/// its anchor whether or not they are visible: a generous fixed maximum makes
/// it build a widget for every file in a large folder.
///
/// Returns the grid and the handler connected to `across`; the caller
/// disconnects it when it replaces the grid.
pub fn build(
    model: &FolderModel,
    style: Style,
    across: &gtk::Adjustment,
    hooks: dnd::Hooks,
) -> (gtk::GridView, gtk::glib::SignalHandlerId) {
    let factory = gtk::SignalListItemFactory::new();
    let grid = gtk::GridView::new(Some(model.selection().clone()), Some(factory.clone()));
    let metrics = Metrics::new(&grid, style);

    let selection = model.selection().downgrade();
    let (for_bind, for_unbind) = (hooks.clone(), hooks.thumbs.clone());
    factory.connect_setup(move |_, object| {
        if let (Some(list_item), Some(selection)) =
            (object.downcast_ref::<gtk::ListItem>(), selection.upgrade())
        {
            setup(list_item, style, metrics, &selection, &hooks);
        }
    });
    factory.connect_bind(move |_, object| {
        if let Some(list_item) = object.downcast_ref::<gtk::ListItem>() {
            bind(list_item, style, &for_bind);
        }
    });
    // The only thing bind starts is a thumbnail request; unbind cancels it.
    factory.connect_unbind(move |_, object| {
        if let (Some(list_item), Some(thumbs)) =
            (object.downcast_ref::<gtk::ListItem>(), &for_unbind)
        {
            thumbs.unbind(list_item);
        }
    });

    grid.set_min_columns(1);
    grid.set_max_columns(INITIAL_COLUMNS);
    grid.set_enable_rubberband(true);
    grid.add_css_class("folder-view");
    if compact(style) {
        grid.set_orientation(gtk::Orientation::Horizontal);
        grid.add_css_class("compact-view");
    }

    let weak = grid.downgrade();
    let fit = move |across: &gtk::Adjustment| {
        let Some(grid) = weak.upgrade() else { return };
        // Rounded up: one column too many only keeps a few more cells alive,
        // one too few would leave room for a column unused.
        let columns = ((across.page_size() / metrics.extent).ceil() as u32).clamp(1, 200);
        if columns != grid.max_columns() {
            grid.set_max_columns(columns);
        }
    };
    fit(across);
    let handler = across.connect_page_size_notify(fit);
    (grid, handler)
}
