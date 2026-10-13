//! A minimal change signal for application-wide state (bookmarks, volumes).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Callback = Rc<dyn Fn()>;

/// Callbacks to run when something changed. Handlers must capture their
/// owners weakly and be disconnected when the owner goes away.
#[derive(Default)]
pub struct Listeners {
    next: Cell<u64>,
    list: RefCell<Vec<(u64, Callback)>>,
}

impl Listeners {
    pub fn connect(&self, callback: impl Fn() + 'static) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.list.borrow_mut().push((id, Rc::new(callback)));
        id
    }

    pub fn disconnect(&self, id: u64) {
        self.list.borrow_mut().retain(|(other, _)| *other != id);
    }

    pub fn emit(&self) {
        // Collected first: a callback may connect or disconnect others.
        let callbacks: Vec<Callback> = self.list.borrow().iter().map(|(_, c)| c.clone()).collect();
        for callback in callbacks {
            callback();
        }
    }
}
