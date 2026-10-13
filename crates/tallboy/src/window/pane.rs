//! A pane: one notebook and its tabs (PROMPT §8.4).

use crate::{settings::Settings, tab::TabPage};
use gtk::{gdk, pango, prelude::*};
use std::{cell::RefCell, rc::Rc};

/// What a pane tells its window.
pub enum PaneEvent {
    /// Another tab became current.
    Switched,
    /// The user dragged a tab to another place.
    Reordered,
    /// The user asked to close this tab (close button or middle-click).
    CloseRequested(TabPage),
}

type Handler = Rc<dyn Fn(PaneEvent)>;

pub struct Pane {
    /// The pane's frame; it carries the `active-pane` class (§8.5).
    frame: gtk::Box,
    notebook: gtk::Notebook,
    tabs: RefCell<Vec<TabPage>>,
    /// Pages in the order they were last shown, most recent last.
    recent: RefCell<Vec<gtk::Widget>>,
    handler: Rc<RefCell<Option<Handler>>>,
}

impl Pane {
    pub fn new() -> Self {
        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);
        notebook.set_show_tabs(false);
        notebook.set_hexpand(true);
        notebook.set_vexpand(true);
        let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
        frame.add_css_class("pane");
        frame.append(&notebook);
        let pane = Self {
            frame,
            notebook,
            tabs: RefCell::default(),
            recent: RefCell::default(),
            handler: Rc::default(),
        };
        // `notify::page` fires after the switch; `switch-page` fires before.
        let handler = pane.handler.clone();
        pane.notebook.connect_page_notify(move |_| {
            let handler = handler.borrow().clone();
            if let Some(handler) = handler {
                handler(PaneEvent::Switched);
            }
        });
        let handler = pane.handler.clone();
        pane.notebook.connect_page_reordered(move |_, _, _| {
            let handler = handler.borrow().clone();
            if let Some(handler) = handler {
                handler(PaneEvent::Reordered);
            }
        });
        pane
    }

    /// Sets the window's handler. It must capture the window weakly.
    pub fn connect(&self, handler: impl Fn(PaneEvent) + 'static) {
        self.handler.replace(Some(Rc::new(handler)));
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.frame
    }

    /// Marks this pane as the one window commands act on.
    pub fn set_active(&self, active: bool) {
        if active {
            self.frame.add_css_class("active-pane");
        } else {
            self.frame.remove_css_class("active-pane");
        }
    }

    /// Whether `tab` lives in this pane.
    pub fn holds(&self, tab: &TabPage) -> bool {
        self.tabs
            .borrow()
            .iter()
            .any(|other| other.widget() == tab.widget())
    }

    fn emit_close(handler: &Rc<RefCell<Option<Handler>>>, tab: &crate::tab::WeakTab) {
        let handler = handler.borrow().clone();
        if let (Some(handler), Some(tab)) = (handler, tab.upgrade()) {
            handler(PaneEvent::CloseRequested(tab));
        }
    }

    /// Appends a tab and makes it current.
    pub fn add(&self, tab: TabPage, settings: &Settings) {
        let text = gtk::Label::new(None);
        text.set_ellipsize(pango::EllipsizeMode::End);
        text.set_hexpand(true);
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_focus_on_click(false);
        close.set_tooltip_text(Some("Close Tab"));
        let label = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        label.add_css_class("tab-label");
        label.append(&text);
        label.append(&close);

        let weak = tab.downgrade();
        let handler = self.handler.clone();
        close.connect_clicked(move |_| Self::emit_close(&handler, &weak));
        let middle = gtk::GestureClick::new();
        middle.set_button(gdk::BUTTON_MIDDLE);
        let weak = tab.downgrade();
        let handler = self.handler.clone();
        middle.connect_released(move |_, _, _, _| Self::emit_close(&handler, &weak));
        label.add_controller(middle);

        // A tab label takes drops for its folder, and a drag resting on it
        // brings that tab forward (§8.12).
        let weak = tab.downgrade();
        label.add_controller(crate::view::dnd::drop_target(
            move || {
                let tab = weak.upgrade()?;
                Some((tab.location(), tab.filesystem_id()))
            },
            &tab.drop_hooks(),
        ));
        label.add_controller(self.hover_switch(&tab, settings));

        let page = self.notebook.append_page(tab.widget(), Some(&label));
        self.notebook.set_tab_reorderable(tab.widget(), true);
        self.tabs.borrow_mut().push(tab.clone());
        self.renumber();
        self.apply_settings(settings);
        self.notebook.set_current_page(Some(page));
    }

    fn hover_switch(&self, tab: &TabPage, settings: &Settings) -> gtk::DropControllerMotion {
        const REST: std::time::Duration = std::time::Duration::from_millis(600);
        let motion = gtk::DropControllerMotion::new();
        let timer: Rc<RefCell<Option<gtk::glib::SourceId>>> = Rc::default();
        let pending = timer.clone();
        let notebook = self.notebook.downgrade();
        let weak = tab.downgrade();
        let settings = settings.clone();
        motion.connect_enter(move |_, _, _| {
            if !settings.change_tab_on_drop() {
                return;
            }
            let (notebook, weak, slot) = (notebook.clone(), weak.clone(), pending.clone());
            let source = gtk::glib::timeout_add_local_once(REST, move || {
                slot.replace(None);
                if let (Some(notebook), Some(tab)) = (notebook.upgrade(), weak.upgrade()) {
                    notebook.set_current_page(notebook.page_num(tab.widget()));
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

    /// Removes a tab; returns how many are left. With `focus_previous` the
    /// tab that was current before it becomes current, otherwise GTK picks
    /// the neighbour.
    pub fn close(&self, tab: &TabPage, settings: &Settings) -> usize {
        let widget: gtk::Widget = tab.widget().clone().upcast();
        self.recent.borrow_mut().retain(|page| page != &widget);
        let Some(index) = self.notebook.page_num(&widget) else {
            return self.tabs.borrow().len();
        };
        if settings.focus_previous()
            && self.notebook.current_page() == Some(index)
            && let Some(previous) = self.recent.borrow().last()
        {
            self.notebook
                .set_current_page(self.notebook.page_num(previous));
        }
        self.notebook.remove_page(Some(index));
        self.tabs
            .borrow_mut()
            .retain(|other| other.widget() != tab.widget());
        self.renumber();
        self.apply_settings(settings);
        self.tabs.borrow().len()
    }

    /// The tab shown in this pane.
    pub fn current(&self) -> Option<TabPage> {
        let page = self.notebook.nth_page(self.notebook.current_page())?;
        self.tabs
            .borrow()
            .iter()
            .find(|tab| tab.widget().upcast_ref::<gtk::Widget>() == &page)
            .cloned()
    }

    /// Records the current page as most recently shown.
    pub fn note_current(&self) {
        if let Some(page) = self.notebook.nth_page(self.notebook.current_page()) {
            let mut recent = self.recent.borrow_mut();
            recent.retain(|other| other != &page);
            recent.push(page);
        }
    }

    /// Updates a tab's label and tooltip after it navigated.
    pub fn set_label(&self, tab: &TabPage) {
        let Some(label) = self.notebook.tab_label(tab.widget()) else {
            return;
        };
        if let Some(text) = label.first_child().and_downcast::<gtk::Label>() {
            // The number is the key that switches to the tab.
            let number = self
                .notebook
                .page_num(tab.widget())
                .map_or(0, |page| page + 1);
            text.set_text(&format!("{number}  {}", tab.title()));
        }
        label.set_tooltip_text(Some(&tab.location().display()));
    }

    /// Numbers every tab again after one was added, closed or moved.
    pub fn renumber(&self) {
        for tab in self.tabs.borrow().iter() {
            self.set_label(tab);
        }
    }

    /// `{` and `}`: moves `tab` one place left (`-1`) or right (`+1`).
    pub fn swap(&self, tab: &TabPage, by: i32) {
        let count = self.notebook.n_pages() as i32;
        let Some(at) = self.notebook.page_num(tab.widget()) else {
            return;
        };
        let to = at as i32 + by;
        if (0..count).contains(&to) {
            self.notebook.reorder_child(tab.widget(), Some(to as u32));
            self.renumber();
        }
    }

    /// Applies `always_show_tabs`, `hide_close_btn` and `max_tab_chars`.
    pub fn apply_settings(&self, settings: &Settings) {
        let tabs = self.tabs.borrow();
        self.notebook
            .set_show_tabs(settings.always_show_tabs() || tabs.len() > 1);
        for tab in tabs.iter() {
            let Some(label) = self.notebook.tab_label(tab.widget()) else {
                continue;
            };
            if let Some(text) = label.first_child().and_downcast::<gtk::Label>() {
                text.set_max_width_chars(settings.max_tab_chars().max(1) as i32);
            }
            if let Some(close) = label.last_child() {
                close.set_visible(!settings.hide_close_btn());
            }
        }
    }

    /// Steps to the next (`+1`) or previous (`-1`) tab, wrapping around.
    pub fn step(&self, by: i32) {
        let count = self.notebook.n_pages() as i32;
        if count > 1
            && let Some(current) = self.notebook.current_page()
        {
            let next = (current as i32 + by).rem_euclid(count);
            self.notebook.set_current_page(Some(next as u32));
        }
    }

    /// Jumps to the tab at `index`, if there is one.
    pub fn go_to(&self, index: u32) {
        if index < self.notebook.n_pages() {
            self.notebook.set_current_page(Some(index));
        }
    }

    /// Brings `tab` to the front.
    pub fn show(&self, tab: &TabPage) {
        self.notebook
            .set_current_page(self.notebook.page_num(tab.widget()));
    }

    pub fn tabs(&self) -> Vec<TabPage> {
        self.tabs.borrow().clone()
    }
}
