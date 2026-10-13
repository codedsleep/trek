//! Detailed list view: `ColumnView` with libfm's columns (PROMPT §8.6).
//!
//! Header sorting: `ColumnViewSorter` inverts the whole comparison when
//! descending, which would put folders last. The sort model therefore keeps
//! our own sorter. Each column has a trivial sorter only so that its header
//! is clickable and shows an arrow; we listen to the column view's sorter,
//! read the primary column and order, and hand them to the tab. When the
//! order changes elsewhere, `show_sort` moves the arrow, guarded against the
//! echo.

use super::{FileItem, FolderModel, Style, dnd, grid};
use gtk::{gio, glib, pango, prelude::*};
use std::{cell::Cell, rc::Rc};
use tallboy_core::{config::Columns, format, naming, sort};

/// One list column. The ids are libfm's, as stored in `columns=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Name,
    Desc,
    Size,
    Perm,
    Owner,
    Mtime,
    Ext,
}

impl Col {
    pub const ALL: [Col; 7] = [
        Col::Name,
        Col::Desc,
        Col::Size,
        Col::Perm,
        Col::Owner,
        Col::Mtime,
        Col::Ext,
    ];
    /// Shown when `columns=` is empty.
    const DEFAULT: [Col; 4] = [Col::Name, Col::Desc, Col::Size, Col::Mtime];

    pub fn id(self) -> &'static str {
        match self {
            Col::Name => "name",
            Col::Desc => "desc",
            Col::Size => "size",
            Col::Perm => "perm",
            Col::Owner => "owner",
            Col::Mtime => "mtime",
            Col::Ext => "ext",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Col::Name => "Name",
            Col::Desc => "Description",
            Col::Size => "Size",
            Col::Perm => "Permissions",
            Col::Owner => "Owner",
            Col::Mtime => "Modified",
            Col::Ext => "Extension",
        }
    }

    fn sort_column(self) -> sort::Column {
        match self {
            Col::Name => sort::Column::Name,
            Col::Desc => sort::Column::Description,
            Col::Size => sort::Column::Size,
            Col::Perm => sort::Column::Permissions,
            Col::Owner => sort::Column::Owner,
            Col::Mtime => sort::Column::Modified,
            Col::Ext => sort::Column::Extension,
        }
    }

    fn text(self, file: &FileItem, style: Style) -> String {
        let meta = file.meta();
        match self {
            Col::Name => meta.display_name.clone(),
            Col::Desc => meta.description.to_string(),
            // Directories show no size (§8.6).
            Col::Size if file.is_dir() => String::new(),
            Col::Size => format::size(meta.size, style.si_unit),
            Col::Perm => format::permissions(meta.mode),
            Col::Owner => meta.owner.to_string(),
            Col::Mtime => glib::DateTime::from_unix_local(meta.mtime)
                .and_then(|time| time.format("%x %R"))
                .map(String::from)
                .unwrap_or_default(),
            Col::Ext => naming::split_extension(&meta.display_name)
                .1
                .trim_start_matches('.')
                .to_owned(),
        }
    }
}

/// What the list view tells its tab.
pub enum ListEvent {
    /// A header was clicked: sort by this column, in this direction.
    Sort(sort::Column, bool),
    /// Column visibility or a width changed.
    Columns(Columns),
}

pub struct ListView {
    view: gtk::ColumnView,
    columns: Vec<(Col, gtk::ColumnViewColumn)>,
    /// Set while the header arrow is moved by code.
    syncing: Rc<Cell<bool>>,
}

fn name_factory(
    style: Style,
    selection: &gtk::MultiSelection,
    hooks: dnd::Hooks,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    let selection = selection.downgrade();
    let (for_bind, for_unbind) = (hooks.thumbs.clone(), hooks.thumbs.clone());
    factory.connect_setup(move |_, object| {
        let (Some(cell), Some(selection)) = (
            object.downcast_ref::<gtk::ColumnViewCell>(),
            selection.upgrade(),
        ) else {
            return;
        };
        let image = grid::icon_image(style);
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        content.set_halign(gtk::Align::Start);
        content.append(&image);
        content.append(&label);
        dnd::attach(&content, cell, &selection, &image, style, &hooks);
        cell.set_child(Some(&content));
    });
    factory.connect_unbind(move |_, object| {
        if let (Some(cell), Some(thumbs)) =
            (object.downcast_ref::<gtk::ColumnViewCell>(), &for_unbind)
        {
            thumbs.unbind(cell);
        }
    });
    factory.connect_bind(move |_, object| {
        let Some(cell) = object.downcast_ref::<gtk::ColumnViewCell>() else {
            return;
        };
        let (Some(file), Some(content)) = (
            cell.item().and_downcast::<FileItem>(),
            cell.child().and_downcast::<gtk::Box>(),
        ) else {
            return;
        };
        let (Some(image), Some(label)) = (
            content.first_child().and_downcast::<gtk::Image>(),
            content.last_child().and_downcast::<gtk::Label>(),
        ) else {
            return;
        };
        let meta = file.meta();
        let shown = for_bind
            .as_ref()
            .is_some_and(|thumbs| thumbs.bind(cell, &image, &meta, style.icon_size));
        if !shown {
            image.set_from_gicon(&file.icon());
        }
        label.set_text(&meta.display_name);
        grid::set_link_emblem(&content, &image, meta.symlink, style);
        content.set_tooltip_text(super::trash_tooltip(&meta).as_deref());
        if style.shadow_hidden && meta.hidden {
            content.add_css_class("hidden-item");
        } else {
            content.remove_css_class("hidden-item");
        }
    });
    factory
}

fn text_factory(col: Col, style: Style) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        if let Some(cell) = object.downcast_ref::<gtk::ColumnViewCell>() {
            let label = gtk::Label::new(None);
            label.set_xalign(if col == Col::Size { 1.0 } else { 0.0 });
            label.set_ellipsize(pango::EllipsizeMode::End);
            cell.set_child(Some(&label));
        }
    });
    factory.connect_bind(move |_, object| {
        let Some(cell) = object.downcast_ref::<gtk::ColumnViewCell>() else {
            return;
        };
        if let (Some(file), Some(label)) = (
            cell.item().and_downcast::<FileItem>(),
            cell.child().and_downcast::<gtk::Label>(),
        ) {
            label.set_text(&col.text(&file, style));
        }
    });
    factory
}

impl ListView {
    /// Builds the list over the tab's model. `emit` must capture its tab weakly.
    pub fn new(
        model: &FolderModel,
        style: Style,
        shown: &Columns,
        hooks: dnd::Hooks,
        emit: impl Fn(ListEvent) + 'static,
    ) -> Self {
        let emit: Rc<dyn Fn(ListEvent)> = Rc::new(emit);
        let view = gtk::ColumnView::new(Some(model.selection().clone()));
        view.set_enable_rubberband(true);
        view.set_show_column_separators(false);
        view.add_css_class("folder-view");
        view.add_css_class("list-view");

        let wanted: Vec<(Col, Option<u32>)> = if shown.0.is_empty() {
            Col::DEFAULT.iter().map(|col| (*col, None)).collect()
        } else {
            Col::ALL
                .iter()
                .filter_map(|col| {
                    let entry = shown.0.iter().find(|(id, _)| id == col.id())?;
                    Some((*col, entry.1))
                })
                .collect()
        };

        let toggles = gio::SimpleActionGroup::new();
        let menu = gio::Menu::new();
        let mut columns = Vec::new();
        for col in Col::ALL {
            let factory = if col == Col::Name {
                name_factory(style, model.selection(), hooks.clone())
            } else {
                text_factory(col, style)
            };
            let column = gtk::ColumnViewColumn::new(Some(col.title()), Some(factory));
            column.set_id(Some(col.id()));
            column.set_resizable(true);
            column.set_expand(col == Col::Name);
            // Trivial: it only makes the header clickable and shows the arrow.
            column.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
            let setting = wanted.iter().find(|(c, _)| *c == col);
            column.set_visible(col == Col::Name || setting.is_some());
            if let Some((_, Some(width))) = setting {
                column.set_fixed_width(*width as i32);
            }
            column.set_header_menu(Some(&menu));
            view.append_column(&column);
            if col != Col::Name {
                menu.append(Some(col.title()), Some(&format!("columns.{}", col.id())));
                let toggle = gio::SimpleAction::new_stateful(
                    col.id(),
                    None,
                    &column.is_visible().to_variant(),
                );
                let weak = column.downgrade();
                toggle.connect_activate(move |action, _| {
                    if let Some(column) = weak.upgrade() {
                        let show = !column.is_visible();
                        column.set_visible(show);
                        action.set_state(&show.to_variant());
                    }
                });
                toggles.add_action(&toggle);
            }
            columns.push((col, column));
        }
        view.insert_action_group("columns", Some(&toggles));

        let list = Self {
            view,
            columns,
            syncing: Rc::default(),
        };
        list.connect(emit);
        list
    }

    fn connect(&self, emit: Rc<dyn Fn(ListEvent)>) {
        // Report visibility and width changes as a `columns=` value.
        let snapshot = {
            let columns: Vec<_> = self
                .columns
                .iter()
                .map(|(col, column)| (*col, column.downgrade()))
                .collect();
            move || {
                Columns(
                    columns
                        .iter()
                        .filter_map(|(col, column)| {
                            let column = column.upgrade()?;
                            let width = column.fixed_width();
                            column
                                .is_visible()
                                .then(|| (col.id().to_owned(), (width > 0).then_some(width as u32)))
                        })
                        .collect(),
                )
            }
        };
        for (_, column) in &self.columns {
            let on_visible = (emit.clone(), snapshot.clone());
            column
                .connect_visible_notify(move |_| on_visible.0(ListEvent::Columns(on_visible.1())));
            let on_width = (emit.clone(), snapshot.clone());
            column
                .connect_fixed_width_notify(move |_| on_width.0(ListEvent::Columns(on_width.1())));
        }

        let Some(sorter) = self.view.sorter().and_downcast::<gtk::ColumnViewSorter>() else {
            return;
        };
        let syncing = self.syncing.clone();
        let by_id: Vec<_> = self.columns.iter().map(|(col, _)| *col).collect();
        sorter.connect_changed(move |sorter, _| {
            if syncing.get() {
                return;
            }
            let sorter = sorter.downcast_ref::<gtk::ColumnViewSorter>();
            let Some(column) = sorter.and_then(|s| s.primary_sort_column()) else {
                return;
            };
            let id = column.id();
            if let Some(col) = by_id.iter().find(|col| Some(col.id()) == id.as_deref()) {
                let descending =
                    sorter.map(|s| s.primary_sort_order()) == Some(gtk::SortType::Descending);
                emit(ListEvent::Sort(col.sort_column(), descending));
            }
        });
    }

    pub fn widget(&self) -> &gtk::ColumnView {
        &self.view
    }

    /// Moves the header arrow to match a sort order set elsewhere.
    pub fn show_sort(&self, spec: &sort::SortSpec) {
        let column = self
            .columns
            .iter()
            .find(|(col, _)| col.sort_column() == spec.column)
            .map(|(_, column)| column);
        self.syncing.set(true);
        // Clear first: the sorter keeps earlier columns as secondary keys
        // and would show an arrow on each.
        self.view.sort_by_column(None, gtk::SortType::Ascending);
        self.view.sort_by_column(
            column,
            if spec.descending {
                gtk::SortType::Descending
            } else {
                gtk::SortType::Ascending
            },
        );
        self.syncing.set(false);
    }
}
