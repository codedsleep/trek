//! Thumbnails in views (PROMPT §8.19): a request when an item is bound to a
//! cell, cancelled when it is unbound, and a small cache of textures so
//! that a second visit is instant. Generation, the shared on-disk cache and
//! the skip rules are `fs::thumbs`; workers hand back plain pixels (S8) and
//! the texture is made here.

use gtk::{gdk, glib, prelude::*};
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use tallboy_core::config::Config;
use tallboy_fs::{
    meta::FileMeta,
    thumbs::{Cache, Decoded, Pool, Request, Size, Spec},
};

/// Textures kept in memory, keyed by URI, mtime and size directory.
const KEPT: usize = 500;
/// And at most this much pixel data: 500 of the largest thumbnails would be
/// two gigabytes.
const KEPT_BYTES: usize = 96 * 1024 * 1024;

type Key = (String, i64, &'static str);

#[derive(Default)]
struct Textures {
    map: HashMap<Key, (gdk::Texture, u64)>,
    tick: u64,
    bytes: usize,
}

fn weight(texture: &gdk::Texture) -> usize {
    texture.width().max(0) as usize * texture.height().max(0) as usize * 4
}

impl Textures {
    fn get(&mut self, key: &Key) -> Option<gdk::Texture> {
        self.tick += 1;
        let tick = self.tick;
        self.map.get_mut(key).map(|(texture, used)| {
            *used = tick;
            texture.clone()
        })
    }

    fn insert(&mut self, key: Key, texture: gdk::Texture) {
        self.tick += 1;
        self.bytes += weight(&texture);
        if let Some((replaced, _)) = self.map.insert(key, (texture, self.tick)) {
            self.bytes = self.bytes.saturating_sub(weight(&replaced));
        }
        // Drop the least recently used until both limits hold. The scan is
        // over a few hundred entries and only runs once the cache is full.
        while self.map.len() > KEPT || (self.bytes > KEPT_BYTES && self.map.len() > 1) {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone());
            match oldest.and_then(|key| self.map.remove(&key)) {
                Some((dropped, _)) => self.bytes = self.bytes.saturating_sub(weight(&dropped)),
                None => break,
            }
        }
    }
}

/// The application's thumbnail service.
pub struct Thumbs {
    pool: Pool,
    textures: RefCell<Textures>,
}

/// A thumbnail on its way. Dropping it cancels the request.
pub struct Pending(#[allow(dead_code)] Request);

fn texture(decoded: &Decoded) -> gdk::Texture {
    let format = if decoded.channels == 4 {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    gdk::MemoryTexture::new(
        decoded.width,
        decoded.height,
        format,
        &glib::Bytes::from(&decoded.pixels[..]),
        decoded.rowstride as usize,
    )
    .upcast()
}

/// Whether a file is worth asking a thumbnail for; the backend applies the
/// size, locality and own-cache rules itself.
pub fn wanted(meta: &FileMeta) -> bool {
    meta.file_type == gtk::gio::FileType::Regular && meta.content_type.starts_with("image/")
}

impl Thumbs {
    /// Starts the worker pool on the shared cache under `$XDG_CACHE_HOME`.
    pub fn new() -> Option<Rc<Self>> {
        let pool = Pool::new(Cache::new(&glib::user_cache_dir()))
            .map_err(|e| log::warn!("Thumbnails are unavailable: {e}"))
            .ok()?;
        Some(Rc::new(Self {
            pool,
            textures: RefCell::default(),
        }))
    }

    fn key(meta: &FileMeta, pixels: u32, scale: f64) -> Key {
        (
            meta.uri.clone(),
            meta.mtime,
            Size::for_pixels(pixels, scale).directory(),
        )
    }

    /// A texture already in memory, for a bind that needs no request.
    pub fn cached(&self, meta: &FileMeta, pixels: u32, scale: f64) -> Option<gdk::Texture> {
        self.textures
            .borrow_mut()
            .get(&Self::key(meta, pixels, scale))
    }

    /// Asks for a thumbnail; `ready` runs on the main thread with it.
    pub fn request(
        self: &Rc<Self>,
        meta: &FileMeta,
        pixels: u32,
        scale: f64,
        config: &Config,
        ready: impl FnOnce(gdk::Texture) + 'static,
    ) -> Pending {
        let key = Self::key(meta, pixels, scale);
        let request = self.pool.request(Spec::from(meta), pixels, scale, config);
        let result = request.result.clone();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            // A cancelled or skipped request just ends here.
            if let Ok(Ok(decoded)) = result.recv().await {
                let texture = texture(&decoded);
                if let Some(thumbs) = weak.upgrade() {
                    thumbs.textures.borrow_mut().insert(key, texture.clone());
                }
                ready(texture);
            }
        });
        Pending(request)
    }
}

/// What a view needs to show thumbnails: the service and a snapshot of the
/// settings the backend's skip rules read.
#[derive(Clone)]
pub struct Loader {
    pub thumbs: Rc<Thumbs>,
    pub config: Rc<Config>,
    /// Requests of the cells currently bound, dropped on unbind. Each has
    /// a ticket, so that a result which was already on its way when its cell
    /// was bound to another file cannot be mistaken for the new request's.
    pending: Rc<RefCell<HashMap<glib::Object, (u64, Pending)>>>,
    ticket: Rc<std::cell::Cell<u64>>,
}

impl Loader {
    pub fn new(thumbs: Rc<Thumbs>, config: Config) -> Self {
        Self {
            thumbs,
            config: Rc::new(config),
            pending: Rc::default(),
            ticket: Rc::default(),
        }
    }

    /// Shows the file's thumbnail in `image`: at once when it is in memory,
    /// otherwise when the worker delivers it, provided `cell` still shows
    /// the same file by then. Returns whether the image was set now.
    pub fn bind(
        &self,
        cell: &impl IsA<glib::Object>,
        image: &gtk::Image,
        meta: &FileMeta,
        pixels: u32,
    ) -> bool {
        if !wanted(meta) {
            return false;
        }
        // Fractional scales (1.5 on the owner's monitors) come from the
        // surface; before the cell is on one, the integer factor will do.
        let scale = image
            .native()
            .and_then(|native| native.surface())
            .map_or_else(
                || f64::from(image.scale_factor()),
                |surface| surface.scale(),
            );
        if let Some(texture) = self.thumbs.cached(meta, pixels, scale) {
            image.set_paintable(Some(&texture));
            return true;
        }
        let weak_image = image.downgrade();
        let pending = self.pending.clone();
        let cell_object: glib::Object = cell.clone().upcast();
        let weak_cell = cell_object.downgrade();
        let ticket = self.ticket.get() + 1;
        self.ticket.set(ticket);
        let request = self
            .thumbs
            .request(meta, pixels, scale, &self.config, move |texture| {
                let (Some(image), Some(cell)) = (weak_image.upgrade(), weak_cell.upgrade()) else {
                    return;
                };
                // Only if the cell is still bound to the file this request
                // was made for.
                let mut pending = pending.borrow_mut();
                if pending
                    .get(&cell)
                    .is_some_and(|(current, _)| *current == ticket)
                {
                    pending.remove(&cell);
                    image.set_paintable(Some(&texture));
                }
            });
        self.pending
            .borrow_mut()
            .insert(cell_object, (ticket, request));
        false
    }

    /// The cell no longer shows the file: cancel what was asked for it.
    pub fn unbind(&self, cell: &impl IsA<glib::Object>) {
        self.pending.borrow_mut().remove(cell.upcast_ref());
    }
}
