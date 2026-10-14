//! GTK bookmarks with unknown-line preservation and external-change notification.
use gio::prelude::*;
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};
use trek_core::location::{Location, LocationError};
/// One validated bookmark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    /// Destination, passed through Location::parse.
    pub location: Location,
    /// Optional user label.
    pub label: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum Line {
    Bookmark(Bookmark),
    Unknown(String),
}
/// Parsed document; malformed, blank and comment lines survive edits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bookmarks {
    lines: Vec<Line>,
}
/// Bookmark I/O, invalid label, or invalid index.
#[derive(Debug, thiserror::Error)]
pub enum BookmarkError {
    /// File read/write failure.
    #[error("Bookmarks: {0}")]
    Io(#[from] std::io::Error),
    /// Backend error.
    #[error("Bookmarks: {0}")]
    Gio(#[from] glib::Error),
    /// Blocked destination.
    #[error(transparent)]
    Location(#[from] LocationError),
    /// Invalid label/index.
    #[error("Invalid bookmark label or index")]
    Invalid,
}
impl Bookmarks {
    /// Parse standard GTK URI + optional label lines. Unknown lines are retained.
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|line| {
                let (uri, label) = line
                    .split_once(' ')
                    .map_or((line, None), |(uri, label)| (uri, Some(label.to_owned())));
                if uri.contains("://")
                    && let Ok(location) = Location::parse(uri, Path::new("/"), Path::new("/"))
                {
                    return Line::Bookmark(Bookmark { location, label });
                }
                Line::Unknown(line.into())
            })
            .collect();
        Self { lines }
    }
    /// Render with a final newline, as GTK does.
    pub fn encode(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Bookmark(b) => {
                    out.push_str(b.location.to_uri());
                    if let Some(label) = &b.label {
                        out.push(' ');
                        out.push_str(label);
                    }
                }
                Line::Unknown(s) => out.push_str(s),
            }
            out.push('\n');
        }
        out
    }
    /// Ordered valid bookmark entries, excluding unknown lines.
    pub fn entries(&self) -> Vec<&Bookmark> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::Bookmark(b) => Some(b),
                Line::Unknown(_) => None,
            })
            .collect()
    }
    fn index(&self, index: usize) -> Result<usize, BookmarkError> {
        self.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| matches!(l, Line::Bookmark(_)))
            .nth(index)
            .map(|(i, _)| i)
            .ok_or(BookmarkError::Invalid)
    }
    /// Append a validated destination; labels may contain spaces, never newlines.
    pub fn add(&mut self, location: Location, label: Option<String>) -> Result<(), BookmarkError> {
        if label
            .as_ref()
            .is_some_and(|s| s.contains(['\r', '\n', '\0']))
        {
            return Err(BookmarkError::Invalid);
        }
        let location = Location::parse(location.to_uri(), Path::new("/"), Path::new("/"))?;
        self.lines
            .push(Line::Bookmark(Bookmark { location, label }));
        Ok(())
    }
    /// Set a bookmark label.
    pub fn rename(&mut self, index: usize, label: Option<String>) -> Result<(), BookmarkError> {
        if label
            .as_ref()
            .is_some_and(|s| s.contains(['\r', '\n', '\0']))
        {
            return Err(BookmarkError::Invalid);
        }
        let i = self.index(index)?;
        if let Line::Bookmark(b) = &mut self.lines[i] {
            b.label = label;
        }
        Ok(())
    }
    /// Remove one valid bookmark.
    pub fn remove(&mut self, index: usize) -> Result<(), BookmarkError> {
        let i = self.index(index)?;
        self.lines.remove(i);
        Ok(())
    }
    /// Reorder by valid-bookmark indices; unknown lines remain in the document.
    pub fn move_to(&mut self, from: usize, to: usize) -> Result<(), BookmarkError> {
        let from = self.index(from)?;
        let to = self.index(to)?;
        let entry = self.lines.remove(from);
        self.lines.insert(to, entry);
        Ok(())
    }
}
struct Inner {
    path: PathBuf,
    document: RefCell<Bookmarks>,
    tx: async_channel::Sender<Result<Bookmarks, String>>,
}
/// Persist bookmarks atomically and watch edits by GTK applications.
pub struct Store {
    inner: Rc<Inner>,
    monitor: gio::FileMonitor,
    /// One event per coalesced external edit.
    pub changes: async_channel::Receiver<Result<Bookmarks, String>>,
}
impl Store {
    /// Open gtk-3.0/bookmarks below an explicitly supplied XDG_CONFIG_HOME.
    pub fn open(config_home: &Path) -> Result<Self, BookmarkError> {
        let path = config_home.join("gtk-3.0/bookmarks");
        fs::create_dir_all(path.parent().ok_or(BookmarkError::Invalid)?)?;
        let text = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let document = RefCell::new(Bookmarks::parse(&text));
        let (tx, changes) = async_channel::unbounded();
        let inner = Rc::new(Inner {
            path: path.clone(),
            document,
            tx,
        });
        let file = gio::File::for_path(path.parent().ok_or(BookmarkError::Invalid)?);
        let monitor = file.monitor_directory(
            gio::FileMonitorFlags::WATCH_MOVES,
            None::<&gio::Cancellable>,
        )?;
        let weak = Rc::downgrade(&inner);
        let scheduled = Rc::new(Cell::new(false));
        monitor.connect_changed(move |_, file, other, _| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if file.path().as_deref() != Some(&inner.path)
                && other.and_then(|f| f.path()).as_deref() != Some(&inner.path)
            {
                return;
            }
            if scheduled.replace(true) {
                return;
            }
            let weak = weak.clone();
            let scheduled = scheduled.clone();
            glib::MainContext::ref_thread_default().spawn_local(async move {
                glib::timeout_future(Duration::from_millis(200)).await;
                scheduled.set(false);
                let Some(inner) = weak.upgrade() else {
                    return;
                };
                let result = match gio::File::for_path(&inner.path)
                    .load_contents_future()
                    .await
                {
                    Ok((bytes, _)) => match std::str::from_utf8(&bytes) {
                        Ok(text) => Ok(Bookmarks::parse(text)),
                        Err(e) => Err(e.to_string()),
                    },
                    Err(e) if e.matches(gio::IOErrorEnum::NotFound) => Ok(Bookmarks::default()),
                    Err(e) => Err(e.to_string()),
                };
                if let Ok(doc) = &result {
                    *inner.document.borrow_mut() = doc.clone();
                } else if let Err(e) = &result {
                    log::warn!("Bookmarks monitor: {e}");
                }
                let _ = inner.tx.try_send(result);
            });
        });
        Ok(Self {
            inner,
            monitor,
            changes,
        })
    }
    /// Current snapshot.
    pub fn snapshot(&self) -> Bookmarks {
        self.inner.document.borrow().clone()
    }
    /// Edit then save. Failed writes leave the in-memory document unchanged.
    pub fn edit(
        &self,
        edit: impl FnOnce(&mut Bookmarks) -> Result<(), BookmarkError>,
    ) -> Result<(), BookmarkError> {
        let mut next = self.snapshot();
        edit(&mut next)?;
        crate::atomic::write(&self.inner.path, next.encode().as_bytes())?;
        *self.inner.document.borrow_mut() = next;
        Ok(())
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        self.monitor.cancel();
        self.inner.tx.close();
    }
}
