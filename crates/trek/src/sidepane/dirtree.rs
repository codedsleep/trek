//! The Directory Tree (PROMPT §8.14): folders only, loaded when a row is
//! expanded and watched while it stays expanded.

use crate::{app::App, tab::location_from_uri};
use gtk::{gio, glib, pango, prelude::*, subclass::prelude::*};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::{Rc, Weak},
    time::Duration,
};
use trek_core::location::Location;
use trek_fs::{
    complete::{self, Children},
    folder::FolderEvent,
    meta::FileMeta,
};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TreeNode {
        pub uri: RefCell<String>,
        pub name: RefCell<String>,
        pub icon: RefCell<Option<gio::Icon>>,
        /// The row's children; filled while the row is expanded.
        pub children: RefCell<Option<gio::ListStore>>,
        /// Bumped to stop the task that fills `children`.
        pub load: Cell<u64>,
        pub loaded: Cell<bool>,
        /// The task that lists and follows this folder. It owns the folder
        /// monitor, so it is aborted, not left to notice on its next event,
        /// which for a quiet folder would never come.
        pub task: RefCell<Option<glib::JoinHandle<()>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TreeNode {
        const NAME: &'static str = "TrekTreeNode";
        type Type = super::TreeNode;
    }

    impl ObjectImpl for TreeNode {
        fn dispose(&self) {
            if let Some(task) = self.task.take() {
                task.abort();
            }
        }
    }
}

glib::wrapper! {
    /// One folder in the tree.
    pub struct TreeNode(ObjectSubclass<imp::TreeNode>);
}

impl TreeNode {
    fn new(uri: &str, name: &str, icon: Option<gio::Icon>) -> Self {
        let node: Self = glib::Object::new();
        node.imp().uri.replace(uri.to_owned());
        node.imp().name.replace(name.to_owned());
        node.imp().icon.replace(icon);
        node
    }

    fn from_meta(meta: &FileMeta) -> Self {
        let icon = meta
            .icon
            .as_deref()
            .and_then(|s| gio::Icon::for_string(s).ok());
        Self::new(&meta.uri, &meta.display_name, icon)
    }

    fn uri(&self) -> String {
        self.imp().uri.borrow().clone()
    }

    fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }

    /// The store the tree model shows under this row, created on demand.
    fn children(&self) -> gio::ListStore {
        self.imp()
            .children
            .borrow_mut()
            .get_or_insert_with(gio::ListStore::new::<TreeNode>)
            .clone()
    }

    fn insert_sorted(&self, child: &TreeNode) {
        self.children().insert_sorted(child, |a, b| {
            let name = |o: &glib::Object| o.downcast_ref::<TreeNode>().map(TreeNode::name);
            trek_core::sort::natural(
                &name(a).unwrap_or_default().to_lowercase(),
                &name(b).unwrap_or_default().to_lowercase(),
            )
        });
    }

    fn remove_child(&self, uri: &str) {
        let store = self.children();
        let position = (0..store.n_items()).find(|index| {
            store
                .item(*index)
                .and_downcast::<TreeNode>()
                .is_some_and(|n| n.uri() == uri)
        });
        if let Some(position) = position {
            store.remove(position);
        }
    }

    /// Starts listing this folder's subfolders and keeps following changes
    /// until [`TreeNode::unload`]. The task owns the monitor.
    fn load(&self, show_hidden: bool) {
        let run = self.imp().load.get() + 1;
        self.imp().load.set(run);
        self.imp().loaded.set(false);
        self.stop();
        self.children().remove_all();
        let Ok(children) = Children::new(&gio::File::for_uri(&self.uri()), show_hidden, None)
        else {
            self.imp().loaded.set(true);
            return;
        };
        let children = crate::debug::Counted::new(children, &crate::debug::LIVE_FOLDERS);
        let weak = self.downgrade();
        let task = glib::spawn_future_local(async move {
            while let Ok(event) = children.recv().await {
                let Some(node) = weak.upgrade() else { break };
                if node.imp().load.get() != run {
                    break;
                }
                match event {
                    FolderEvent::Batch(batch) => {
                        for meta in &batch {
                            node.insert_sorted(&TreeNode::from_meta(meta));
                        }
                    }
                    FolderEvent::Added(meta) => {
                        node.remove_child(&meta.uri);
                        node.insert_sorted(&TreeNode::from_meta(&meta));
                    }
                    FolderEvent::Removed(uri) => node.remove_child(&uri),
                    FolderEvent::Renamed { from, to } => {
                        node.remove_child(&from);
                        node.insert_sorted(&TreeNode::from_meta(&to));
                    }
                    FolderEvent::Finished | FolderEvent::Error(_) => node.imp().loaded.set(true),
                    FolderEvent::Started
                    | FolderEvent::Changed(_)
                    | FolderEvent::FsInfo(_)
                    | FolderEvent::Typed(_)
                    | FolderEvent::RootGone => {}
                }
            }
            children.cancel();
        });
        self.imp().task.replace(Some(task));
    }

    /// Ends the listing task and with it the folder monitor.
    fn stop(&self) {
        if let Some(task) = self.imp().task.take() {
            task.abort();
        }
    }

    /// Stops following the folder and forgets its subfolders.
    fn unload(&self) {
        self.imp().load.set(self.imp().load.get() + 1);
        self.imp().loaded.set(false);
        self.stop();
        self.children().remove_all();
    }
}

struct Inner {
    app: App,
    view: gtk::ListView,
    scroller: gtk::ScrolledWindow,
    roots: gio::ListStore,
    tree: gtk::TreeListModel,
    selection: gtk::SingleSelection,
    show_hidden: Cell<bool>,
    follow: Cell<u64>,
    emit: Box<dyn Fn(Location)>,
}

/// The directory tree. Clones share one tree.
#[derive(Clone)]
pub struct DirTree(Rc<Inner>);

type Bound = Rc<RefCell<HashMap<gtk::ListItem, (gtk::TreeListRow, glib::SignalHandlerId)>>>;

fn apply_probe(expander: &gtk::TreeExpander, row: &gtk::TreeListRow, has_children: bool) {
    if expander.list_row().as_ref() == Some(row) {
        expander.set_hide_expander(!has_children);
    }
}

impl DirTree {
    /// `emit` is called when the user picks a folder.
    pub fn new(app: &App, emit: impl Fn(Location) + 'static) -> Self {
        let roots = gio::ListStore::new::<TreeNode>();
        let tree = gtk::TreeListModel::new(roots.clone(), false, false, |object| {
            // Every row can be expanded; whether it has anything inside is
            // only looked up when it is, or shown by hiding the expander.
            object
                .downcast_ref::<TreeNode>()
                .map(|node| node.children().upcast())
        });
        let selection = gtk::SingleSelection::new(Some(tree.clone()));
        selection.set_autoselect(false);
        selection.set_can_unselect(true);
        let factory = gtk::SignalListItemFactory::new();
        let view = gtk::ListView::new(Some(selection.clone()), Some(factory.clone()));
        view.set_single_click_activate(true);
        view.add_css_class("navigation-sidebar");
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_vexpand(true);
        scroller.set_child(Some(&view));
        let this = Self(Rc::new(Inner {
            app: app.clone(),
            view,
            scroller,
            roots,
            tree,
            selection,
            show_hidden: Cell::new(false),
            follow: Cell::new(0),
            emit: Box::new(emit),
        }));
        this.fill_roots();
        this.connect(&factory);
        this
    }

    fn weak(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }

    pub fn widget(&self) -> &gtk::ScrolledWindow {
        &self.0.scroller
    }

    fn fill_roots(&self) {
        let roots = &self.0.roots;
        roots.remove_all();
        if let Some(home) = self.0.app.home_location() {
            roots.append(&TreeNode::new(
                home.to_uri(),
                "Home Folder",
                Some(gio::ThemedIcon::new("user-home").upcast()),
            ));
        }
        roots.append(&TreeNode::new(
            "file:///",
            "Filesystem Root",
            Some(gio::ThemedIcon::new("drive-harddisk").upcast()),
        ));
    }

    fn connect(&self, factory: &gtk::SignalListItemFactory) {
        let icon_size = self.0.app.settings().pane_icon_size() as i32;
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let image = gtk::Image::new();
            image.set_pixel_size(icon_size);
            let label = gtk::Label::new(None);
            label.set_xalign(0.0);
            label.set_ellipsize(pango::EllipsizeMode::End);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            content.append(&image);
            content.append(&label);
            let expander = gtk::TreeExpander::new();
            expander.set_child(Some(&content));
            item.set_child(Some(&expander));
        });
        // Expanding a row loads it; collapsing stops watching it. The
        // handler is connected in bind and disconnected in unbind.
        let bound: Bound = Rc::default();
        let weak = self.weak();
        let binding = bound.clone();
        factory.connect_bind(move |_, object| {
            let (Some(item), Some(inner)) =
                (object.downcast_ref::<gtk::ListItem>(), weak.upgrade())
            else {
                return;
            };
            let (Some(row), Some(expander)) = (
                item.item().and_downcast::<gtk::TreeListRow>(),
                item.child().and_downcast::<gtk::TreeExpander>(),
            ) else {
                return;
            };
            let Some(node) = row.item().and_downcast::<TreeNode>() else {
                return;
            };
            expander.set_list_row(Some(&row));
            if let Some(content) = expander.child() {
                if let Some(image) = content.first_child().and_downcast::<gtk::Image>() {
                    match node.imp().icon.borrow().as_ref() {
                        Some(icon) => image.set_from_gicon(icon),
                        None => image.set_icon_name(Some("folder")),
                    }
                }
                if let Some(label) = content.last_child().and_downcast::<gtk::Label>() {
                    label.set_text(&node.name());
                }
            }
            // `no_child_non_expandable`: no expander for a folder without
            // subfolders. One cheap probe per row that comes into view.
            expander.set_hide_expander(false);
            if inner.app.settings().no_child_non_expandable() {
                let weak_expander = expander.downgrade();
                let weak_row = row.downgrade();
                let (uri, hidden) = (node.uri(), inner.show_hidden.get());
                glib::spawn_future_local(async move {
                    let has = complete::has_subdirectories(&gio::File::for_uri(&uri), hidden)
                        .await
                        .unwrap_or(true);
                    if let (Some(expander), Some(row)) =
                        (weak_expander.upgrade(), weak_row.upgrade())
                    {
                        apply_probe(&expander, &row, has);
                    }
                });
            }
            let weak = Rc::downgrade(&inner);
            let handler = row.connect_expanded_notify(move |row| {
                let (Some(inner), Some(node)) =
                    (weak.upgrade(), row.item().and_downcast::<TreeNode>())
                else {
                    return;
                };
                if row.is_expanded() {
                    node.load(inner.show_hidden.get());
                } else {
                    node.unload();
                }
            });
            binding.borrow_mut().insert(item.clone(), (row, handler));
        });
        factory.connect_unbind(move |_, object| {
            if let Some(item) = object.downcast_ref::<gtk::ListItem>()
                && let Some((row, handler)) = bound.borrow_mut().remove(item)
            {
                row.disconnect(handler);
                if let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() {
                    expander.set_list_row(None::<&gtk::TreeListRow>);
                }
            }
        });

        let weak = self.weak();
        self.0.view.connect_activate(move |_, position| {
            let Some(inner) = weak.upgrade() else { return };
            let node = inner
                .tree
                .item(position)
                .and_downcast::<gtk::TreeListRow>()
                .and_then(|row| row.item())
                .and_downcast::<TreeNode>();
            if let Some(location) = node.and_then(|node| location_from_uri(&node.uri())) {
                (inner.emit)(location);
            }
        });
    }

    fn row_of(&self, uri: &str) -> Option<(u32, gtk::TreeListRow)> {
        let tree = &self.0.tree;
        (0..tree.n_items()).find_map(|position| {
            let row = tree.item(position).and_downcast::<gtk::TreeListRow>()?;
            let node = row.item().and_downcast::<TreeNode>()?;
            (node.uri() == uri).then_some((position, row))
        })
    }

    /// Expands the tree down to `location` and selects it (§8.14).
    pub fn follow(&self, location: &Location, show_hidden: bool) {
        if self.0.show_hidden.replace(show_hidden) != show_hidden {
            // Hidden folders appeared or went: start over.
            self.fill_roots();
        }
        let run = self.0.follow.get() + 1;
        self.0.follow.set(run);
        let crumbs = location.breadcrumbs();
        // The deepest root that contains the location: Home before "/".
        let roots: Vec<String> = (0..self.0.roots.n_items())
            .filter_map(|i| self.0.roots.item(i).and_downcast::<TreeNode>())
            .map(|node| node.uri())
            .collect();
        let Some(start) = crumbs
            .iter()
            .rposition(|crumb| roots.iter().any(|root| root == crumb.location.to_uri()))
        else {
            self.0.selection.unselect_all();
            return;
        };
        let path: Vec<(String, String)> = crumbs[start..]
            .iter()
            .map(|crumb| (crumb.location.to_uri().to_owned(), crumb.label.clone()))
            .collect();
        let weak = self.weak();
        glib::spawn_future_local(async move {
            let mut parent: Option<TreeNode> = None;
            for (depth, (uri, label)) in path.iter().enumerate() {
                let last = depth + 1 == path.len();
                // A row appears once its parent has listed it.
                let mut found = None;
                for _ in 0..150 {
                    let Some(tree) = weak.upgrade().map(DirTree) else {
                        return;
                    };
                    if tree.0.follow.get() != run {
                        return;
                    }
                    found = tree.row_of(uri);
                    if found.is_some() {
                        break;
                    }
                    // The parent has listed its folders and this one is not
                    // among them: it is hidden. Add it, so that the folder
                    // being shown can always be found in the tree.
                    if let Some(parent) = &parent
                        && parent.imp().loaded.get()
                    {
                        parent.insert_sorted(&TreeNode::new(uri, label, None));
                        continue;
                    }
                    glib::timeout_future(Duration::from_millis(20)).await;
                }
                let (Some((position, row)), Some(tree)) = (found, weak.upgrade().map(DirTree))
                else {
                    return;
                };
                parent = row.item().and_downcast::<TreeNode>();
                if last {
                    tree.0.selection.set_selected(position);
                    tree.0
                        .view
                        .scroll_to(position, gtk::ListScrollFlags::NONE, None);
                } else if !row.is_expanded() {
                    // Not bound rows have no handler yet: load directly too.
                    row.set_expanded(true);
                    if let Some(node) = row.item().and_downcast::<TreeNode>()
                        && !node.imp().loaded.get()
                        && node.children().n_items() == 0
                    {
                        node.load(tree.0.show_hidden.get());
                    }
                }
            }
        });
    }

    /// Visible rows as indented names, for agent checks.
    pub fn debug_state(&self) -> String {
        let tree = &self.0.tree;
        let rows: Vec<String> = (0..tree.n_items())
            .filter_map(|position| {
                let row = tree.item(position).and_downcast::<gtk::TreeListRow>()?;
                let node = row.item().and_downcast::<TreeNode>()?;
                Some(format!(
                    "{}{}",
                    ">".repeat(row.depth() as usize),
                    node.name()
                ))
            })
            .collect();
        let selected = self.0.selection.selected();
        format!(
            "tree={rows:?} tree_selected={}",
            if selected == gtk::INVALID_LIST_POSITION {
                -1
            } else {
                selected as i64
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the isolated headless GTK session from check.sh"]
    fn stale_directory_probe_does_not_change_a_rebound_cell() {
        gtk::init().unwrap();
        let roots = gio::ListStore::new::<TreeNode>();
        roots.append(&TreeNode::new("file:///first", "first", None));
        roots.append(&TreeNode::new("file:///second", "second", None));
        let tree = gtk::TreeListModel::new(roots, false, false, |_| None);
        let first = tree.item(0).and_downcast::<gtk::TreeListRow>().unwrap();
        let second = tree.item(1).and_downcast::<gtk::TreeListRow>().unwrap();
        let expander = gtk::TreeExpander::new();
        expander.set_list_row(Some(&first));
        // The cell is recycled before the first folder's asynchronous probe finishes.
        expander.set_list_row(Some(&second));
        apply_probe(&expander, &first, false);
        assert!(!expander.hides_expander());
        apply_probe(&expander, &second, false);
        assert!(expander.hides_expander());
        // Unbinding releases the row and ignores any result still arriving.
        expander.set_list_row(None::<&gtk::TreeListRow>);
        apply_probe(&expander, &second, true);
        assert!(expander.hides_expander());
    }
}
