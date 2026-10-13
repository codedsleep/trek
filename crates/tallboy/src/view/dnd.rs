//! Pointer gestures on a folder item: selection, activation and drag source.
//!
//! The design comes from the Phase 5 spike (SCOPE §10 "Gestures"). GTK's list
//! widgets select on button release and start a rubber band from any drag,
//! including one that begins on an item; a drag source only starts after the
//! button has been held for 100 ms. Left alone, a quick drag of an item
//! becomes a rubber band and the selection collapses.
//!
//! So the cell's content box — icon and name, not the whole cell — carries
//! its own click gesture and drag source in one gesture group:
//!
//! - A primary press on the content claims the event sequence. GTK's own
//!   click handling and the rubber band never see it.
//! - Pressing an unselected item selects it at once. Ctrl and Shift toggle
//!   and extend through GTK's `list.select-item` action, so the rules are
//!   GTK's.
//! - Pressing an item that is already selected changes nothing until
//!   release, so a drag carries the whole selection; a plain click then
//!   narrows the selection to that item.
//! - A press on the padding around the content is not ours: GTK selects on
//!   release as usual and a drag from there is a rubber band.

use super::{FileItem, Style};
use gtk::{gdk, gio, glib, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
use tallboy_core::{
    config::DropAction,
    location::Location,
    transfer::{self, Intent},
};

/// Files dropped on a folder: what, where, which modifier keys were held and
/// what the drag negotiated. The window decides copy, move or link and
/// starts the job.
pub struct DropRequest {
    pub files: Vec<Location>,
    pub destination: Location,
    pub ctrl: bool,
    pub shift: bool,
    /// The action the drag settled on. On Wayland this is the one action
    /// negotiated with the source, so a move is only allowed if it is here.
    pub negotiated: gdk::DragAction,
    /// Where the "ask" popover goes.
    pub widget: gtk::Widget,
    pub x: f64,
    pub y: f64,
}

/// What a view's cells need from their tab for drag and drop.
#[derive(Clone)]
pub struct Hooks {
    pub drop: Rc<dyn Fn(DropRequest)>,
    /// The `drop_default_action` setting, read when needed.
    pub default_action: Rc<dyn Fn() -> DropAction>,
    /// Shows the context menu at a point of a widget.
    pub menu: Rc<dyn Fn(gtk::Widget, f64, f64)>,
    /// Thumbnails for the cells, when `show_thumbnail` is on.
    pub thumbs: Option<super::thumbs::Loader>,
    /// The pointer pressed the item at this position: the cursor goes there.
    pub pressed: Rc<dyn Fn(u32, Pressed)>,
}

/// What a press did to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    /// The item became the whole selection, as a plain click does.
    Selected,
    /// Ctrl or Shift changed the selection deliberately.
    Modified,
    /// The selection was left alone.
    Kept,
}

thread_local! {
    /// Filesystem id of the files being dragged out of this process, so that
    /// the cursor can show move or copy before the drop.
    static DRAGGED_FROM: RefCell<Option<String>> = const { RefCell::new(None) };
    /// The same for a drag that started in another program: the drop it
    /// belongs to and, once looked up, the filesystem of its first file.
    static FOREIGN_FROM: RefCell<Option<(gdk::Drop, Option<String>)>> =
        const { RefCell::new(None) };
}

/// Filesystem id of the files a drop carries, when it is known yet.
fn source_filesystem(drop: &gdk::Drop) -> Option<String> {
    DRAGGED_FROM.with_borrow(Clone::clone).or_else(|| {
        FOREIGN_FROM.with_borrow(|foreign| match foreign {
            Some((known, fs)) if known == drop => fs.clone(),
            _ => None,
        })
    })
}

/// For a drag from another program, reads its file list while it hovers and
/// looks up the filesystem of the first file, so that "auto" can offer a move
/// within one filesystem as it does for our own drags. Asked once per drag.
fn learn_foreign_source(drop: &gdk::Drop) {
    let ours = DRAGGED_FROM.with_borrow(Option::is_some);
    let known = FOREIGN_FROM.with_borrow(|f| f.as_ref().is_some_and(|(d, _)| d == drop));
    if ours || known {
        return;
    }
    FOREIGN_FROM.set(Some((drop.clone(), None)));
    let drop = drop.clone();
    glib::spawn_future_local(async move {
        let Ok(value) = drop
            .read_value_future(gdk::FileList::static_type(), glib::Priority::DEFAULT)
            .await
        else {
            return;
        };
        let Some(first) = value
            .get::<gdk::FileList>()
            .ok()
            .and_then(|list| list.files().into_iter().next())
        else {
            return;
        };
        let fs = first
            .query_info_future(
                "id::filesystem",
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await
            .ok()
            .and_then(|info| info.attribute_string("id::filesystem"))
            .map(String::from);
        FOREIGN_FROM.with_borrow_mut(|foreign| {
            if let Some((known, slot)) = foreign
                && *known == drop
            {
                *slot = fs;
            }
        });
    });
}

/// Copy, move and link, plus "ask": when a drag settles on ask, the drop
/// reports everything the source allows, which the ask menu needs.
fn all_file_actions() -> gdk::DragAction {
    gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK | gdk::DragAction::ASK
}

/// Whether Ctrl and Shift are held (§8.12: Ctrl copies, Shift moves, both
/// link). A drag source on Wayland does not narrow its offer for these keys
/// and drag events carry no modifier state, so the keyboard is asked.
fn modifiers() -> (bool, bool) {
    let state = gdk::Display::default()
        .and_then(|display| display.default_seat())
        .and_then(|seat| seat.keyboard())
        .map_or(gdk::ModifierType::empty(), |keyboard| {
            keyboard.modifier_state()
        });
    (
        state.contains(gdk::ModifierType::CONTROL_MASK),
        state.contains(gdk::ModifierType::SHIFT_MASK),
    )
}

/// The action to negotiate while hovering, from the modifier keys and
/// `drop_default_action`. Link has no counterpart in the Wayland protocol;
/// it shows as a copy and is settled at the drop.
fn preferred(drop: &gdk::Drop, default: DropAction, dest_fs: Option<&str>) -> gdk::DragAction {
    let (ctrl, shift) = modifiers();
    let from = source_filesystem(drop);
    let intent = transfer::resolve(ctrl, shift, default, from.as_deref(), dest_fs);
    match intent {
        Intent::Move => gdk::DragAction::MOVE,
        Intent::Ask => gdk::DragAction::ASK,
        Intent::Copy | Intent::Link => gdk::DragAction::COPY,
    }
}

/// A drop target for a folder. `destination` names the folder and its
/// filesystem id, or `None` when the widget is not a folder right now.
pub fn drop_target(
    destination: impl Fn() -> Option<(Location, Option<String>)> + 'static,
    hooks: &Hooks,
) -> gtk::DropTarget {
    let target = gtk::DropTarget::new(gdk::FileList::static_type(), all_file_actions());
    let destination = Rc::new(destination);
    let folder = destination.clone();
    target.connect_accept(move |_, drop| {
        drop.formats().contains_type(gdk::FileList::static_type()) && folder().is_some()
    });
    let folder = destination.clone();
    let default_action = hooks.default_action.clone();
    let hover = move |target: &gtk::DropTarget| match (target.current_drop(), folder()) {
        (Some(drop), Some((_, fs))) => {
            learn_foreign_source(&drop);
            preferred(&drop, default_action(), fs.as_deref())
        }
        _ => gdk::DragAction::empty(),
    };
    let on_enter = hover.clone();
    target.connect_enter(move |target, _, _| on_enter(target));
    target.connect_motion(move |target, _, _| hover(target));
    let handler = hooks.drop.clone();
    target.connect_drop(move |target, value, x, y| {
        let (Some((destination, _)), Ok(list), Some(drop)) = (
            destination(),
            value.get::<gdk::FileList>(),
            target.current_drop(),
        ) else {
            return false;
        };
        let files: Vec<Location> = list
            .files()
            .iter()
            .filter_map(|file| crate::tab::location_from_uri(&file.uri()))
            .collect();
        let Some(widget) = target.widget() else {
            return false;
        };
        if files.is_empty() {
            return false;
        }
        let (ctrl, shift) = modifiers();
        handler(DropRequest {
            files,
            destination,
            ctrl,
            shift,
            negotiated: drop.actions(),
            widget,
            x,
            y,
        });
        true
    });
    target
}

/// The part of `ListItem` and `ColumnViewCell` the gestures need.
pub trait CellHandle: IsA<glib::Object> {
    fn cell_position(&self) -> u32;
    fn cell_selected(&self) -> bool;
    fn cell_item(&self) -> Option<glib::Object>;
}

impl CellHandle for gtk::ListItem {
    fn cell_position(&self) -> u32 {
        self.position()
    }
    fn cell_selected(&self) -> bool {
        self.is_selected()
    }
    fn cell_item(&self) -> Option<glib::Object> {
        self.item()
    }
}

impl CellHandle for gtk::ColumnViewCell {
    fn cell_position(&self) -> u32 {
        self.position()
    }
    fn cell_selected(&self) -> bool {
        self.is_selected()
    }
    fn cell_item(&self) -> Option<glib::Object> {
        self.item()
    }
}

/// The files a drag from `cell` carries: the whole selection when the item
/// is part of it, otherwise only that item (§8.12).
fn dragged(cell: &impl CellHandle, selection: &gtk::MultiSelection) -> Vec<FileItem> {
    if cell.cell_selected() {
        let bitset = selection.selection();
        (0..bitset.size())
            .filter_map(|index| selection.item(bitset.nth(index as u32)).and_downcast())
            .collect()
    } else {
        cell.cell_item().and_downcast().into_iter().collect()
    }
}

fn select(widget: &gtk::Widget, position: u32, modify: bool, extend: bool) {
    let _ = widget.activate_action(
        "list.select-item",
        Some(&(position, modify, extend).to_variant()),
    );
}

fn activate(widget: &gtk::Widget, position: u32) {
    let _ = widget.activate_action("list.activate-item", Some(&position.to_variant()));
}

/// GTK's own handler would have focused the row; ours replaces it.
fn focus_row(widget: &gtk::Widget) {
    let mut ancestor = widget.parent();
    while let Some(candidate) = ancestor {
        if candidate.is_focusable() {
            candidate.grab_focus();
            return;
        }
        ancestor = candidate.parent();
    }
}

#[derive(Default)]
struct Press {
    /// A selected item was pressed without modifiers: narrow on release.
    narrow: Cell<bool>,
    /// Single-click mode: open on release.
    open: Cell<bool>,
}

fn click_gesture<C: CellHandle>(cell: &C, style: Style, hooks: &Hooks) -> gtk::GestureClick {
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_PRIMARY);
    let press = Rc::new(Press::default());

    let weak_cell = cell.downgrade();
    let pending = press.clone();
    let (on_press, on_release) = (hooks.pressed.clone(), hooks.pressed.clone());
    click.connect_pressed(move |gesture, n_press, _, _| {
        let (Some(cell), Some(widget)) = (weak_cell.upgrade(), gesture.widget()) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        focus_row(&widget);
        let position = cell.cell_position();
        let state = gesture.current_event_state();
        let modify = state.contains(gdk::ModifierType::CONTROL_MASK);
        let extend = state.contains(gdk::ModifierType::SHIFT_MASK);
        pending.narrow.set(false);
        pending
            .open
            .set(style.single_click && n_press == 1 && !modify && !extend);
        if n_press == 2 && !style.single_click {
            activate(&widget, position);
        } else if modify || extend {
            select(&widget, position, modify, extend);
            on_press(position, Pressed::Modified);
        } else if cell.cell_selected() {
            pending.narrow.set(true);
            on_press(position, Pressed::Kept);
        } else {
            select(&widget, position, false, false);
            on_press(position, Pressed::Selected);
        }
    });

    let weak_cell = cell.downgrade();
    click.connect_released(move |gesture, _, _, _| {
        let (narrow, open) = (press.narrow.take(), press.open.take());
        let (Some(cell), Some(widget)) = (weak_cell.upgrade(), gesture.widget()) else {
            return;
        };
        let position = cell.cell_position();
        if narrow {
            select(&widget, position, false, false);
            on_release(position, Pressed::Selected);
        }
        if open {
            activate(&widget, position);
        }
    });
    click
}

/// Middle-click opens a folder in a new tab (§8.9).
fn middle_click<C: CellHandle>(cell: &C) -> gtk::GestureClick {
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_MIDDLE);
    let weak_cell = cell.downgrade();
    click.connect_released(move |gesture, _, _, _| {
        let (Some(cell), Some(widget)) = (weak_cell.upgrade(), gesture.widget()) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        select(&widget, cell.cell_position(), false, false);
        let _ = widget.activate_action("win.open-new-tab", None);
    });
    click
}

/// Right-click: an unselected item becomes the selection, a selected one
/// keeps the whole selection; then the context menu opens (§8.13).
fn secondary_click<C: CellHandle>(cell: &C, hooks: &Hooks) -> gtk::GestureClick {
    let click = gtk::GestureClick::new();
    click.set_button(gdk::BUTTON_SECONDARY);
    let weak_cell = cell.downgrade();
    let menu = hooks.menu.clone();
    let pressed = hooks.pressed.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let (Some(cell), Some(widget)) = (weak_cell.upgrade(), gesture.widget()) else {
            return;
        };
        gesture.set_state(gtk::EventSequenceState::Claimed);
        focus_row(&widget);
        let position = cell.cell_position();
        if cell.cell_selected() {
            pressed(position, Pressed::Kept);
        } else {
            select(&widget, position, false, false);
            pressed(position, Pressed::Selected);
        }
        menu(widget, x, y);
    });
    click
}

/// Single-click mode: resting the pointer on an item selects it (§8.6).
fn hover_select<C: CellHandle>(cell: &C, delay_ms: u32) -> gtk::EventControllerMotion {
    let motion = gtk::EventControllerMotion::new();
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let weak_cell = cell.downgrade();
    let pending = timer.clone();
    motion.connect_enter(move |motion, _, _| {
        let Some(widget) = motion.widget() else {
            return;
        };
        // Hovering must not disturb a selection being built with Ctrl or Shift.
        let state = motion.current_event_state();
        if state.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK) {
            return;
        }
        let weak_cell = weak_cell.clone();
        let weak_widget = widget.downgrade();
        let slot = pending.clone();
        let source =
            glib::timeout_add_local_once(Duration::from_millis(delay_ms.into()), move || {
                slot.replace(None);
                if let (Some(cell), Some(widget)) = (weak_cell.upgrade(), weak_widget.upgrade())
                    && !cell.cell_selected()
                {
                    select(&widget, cell.cell_position(), false, false);
                }
            });
        if let Some(old) = pending.replace(Some(source)) {
            old.remove();
        }
    });
    motion.connect_leave(move |_| {
        if let Some(source) = timer.take() {
            source.remove();
        }
    });
    motion
}

fn drag_source<C: CellHandle>(
    cell: &C,
    selection: &gtk::MultiSelection,
    icon: &gtk::Image,
) -> gtk::DragSource {
    let source = gtk::DragSource::new();
    source.set_actions(all_file_actions());
    let weak_cell = cell.downgrade();
    let weak_selection = selection.downgrade();
    source.connect_prepare(move |_, _, _| {
        let files = dragged(&weak_cell.upgrade()?, &weak_selection.upgrade()?);
        if files.is_empty() {
            return None;
        }
        let files: Vec<gio::File> = files
            .iter()
            .map(|file| gio::File::for_uri(&file.meta().uri))
            .collect();
        log::debug!("drag prepared with {} file(s)", files.len());
        // A file list also serialises as text/uri-list for other programs.
        let list = gdk::FileList::from_array(&files);
        Some(gdk::ContentProvider::for_value(&list.to_value()))
    });
    let weak_cell = cell.downgrade();
    let weak_selection = selection.downgrade();
    let weak_icon = icon.downgrade();
    source.connect_drag_begin(move |_, drag| {
        let files = match (weak_cell.upgrade(), weak_selection.upgrade()) {
            (Some(cell), Some(selection)) => dragged(&cell, &selection),
            _ => Vec::new(),
        };
        DRAGGED_FROM.set(
            files
                .first()
                .and_then(|file| file.meta().filesystem_id.as_deref().map(String::from)),
        );
        // The item's icon, with the number of files when there are several.
        let picture = gtk::Image::new();
        if let Some(icon) = weak_icon.upgrade() {
            picture.set_pixel_size(icon.pixel_size().max(32));
            if let Some(gicon) = files.first().map(FileItem::icon) {
                picture.set_from_gicon(&gicon);
            }
        }
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        content.append(&picture);
        if files.len() > 1 {
            let count = gtk::Label::new(Some(&files.len().to_string()));
            count.add_css_class("drag-count");
            count.set_valign(gtk::Align::End);
            content.append(&count);
        }
        gtk::DragIcon::for_drag(drag).set_child(Some(&content));
    });
    source.connect_drag_end(|_, _, _| DRAGGED_FROM.set(None));
    source
}

/// Adds the gestures to a cell's content box. Called from factory `setup`;
/// the closures hold only weak references.
pub fn attach<C: CellHandle>(
    content: &impl IsA<gtk::Widget>,
    cell: &C,
    selection: &gtk::MultiSelection,
    icon: &gtk::Image,
    style: Style,
    hooks: &Hooks,
) {
    // Dropping on a folder item puts the files into that folder.
    let weak_cell = cell.downgrade();
    content.add_controller(drop_target(
        move || {
            let item = weak_cell
                .upgrade()?
                .cell_item()
                .and_downcast::<FileItem>()?;
            if !item.is_dir() {
                return None;
            }
            let meta = item.meta();
            let location = crate::tab::location_from_uri(&meta.uri)?;
            Some((location, meta.filesystem_id.as_deref().map(String::from)))
        },
        hooks,
    ));
    let click = click_gesture(cell, style, hooks);
    let source = drag_source(cell, selection, icon);
    // One group: the click's claim must not cancel the drag source.
    source.group_with(&click);
    content.add_controller(click);
    content.add_controller(source);
    content.add_controller(middle_click(cell));
    content.add_controller(secondary_click(cell, hooks));
    if style.single_click && style.hover_delay_ms > 0 {
        content.add_controller(hover_select(cell, style.hover_delay_ms));
    }
}

/// Makes the wheel scroll a horizontally scrolling view sideways, as
/// upstream's compact view does.
pub fn wheel_scrolls_sideways(view: &impl IsA<gtk::Widget>, horizontal: &gtk::Adjustment) {
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let adjustment = horizontal.downgrade();
    scroll.connect_scroll(move |_, _, dy| {
        let Some(adjustment) = adjustment.upgrade() else {
            return glib::Propagation::Proceed;
        };
        // The step GtkScrolledWindow itself uses for one wheel notch.
        let step = adjustment.page_size().powf(2.0 / 3.0);
        adjustment.set_value(adjustment.value() + dy * step);
        glib::Propagation::Stop
    });
    view.add_controller(scroll);
}
