//! Most-recent-first image work and a decoded-pixel LRU capped at 500 entries / 96 MiB.
use super::{cache::Spec, *};
use gio::prelude::*;
use std::{
    collections::{BTreeMap, VecDeque},
    path::Path,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
/// Decoded RGB/RGBA pixels. UI turns this value into gdk::MemoryTexture.
#[derive(Debug, Clone)]
pub struct Decoded {
    /// Pixel width.
    pub width: i32,
    /// Pixel height.
    pub height: i32,
    /// Bytes per scanline (including padding).
    pub rowstride: i32,
    /// 3 (RGB) or 4 (RGBA) channels.
    pub channels: i32,
    /// Owned pixel bytes, never a borrowed pixbuf pointer.
    pub pixels: Vec<u8>,
}
impl Decoded {
    fn from_pixbuf(p: &gdk_pixbuf::Pixbuf) -> Self {
        Self {
            width: p.width(),
            height: p.height(),
            rowstride: p.rowstride(),
            channels: p.n_channels(),
            pixels: p.read_pixel_bytes().as_ref().to_vec(),
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Key {
    uri: String,
    mtime: i64,
    size: Size,
}
struct Work {
    id: u64,
    spec: Spec,
    size: Size,
    config: tallboy_core::config::Config,
    cancel: gio::Cancellable,
    tx: async_channel::Sender<Result<Arc<Decoded>, Error>>,
}
struct State {
    queue: Vec<Work>,
    pending: BTreeMap<u64, gio::Cancellable>,
    stopped: bool,
    lru: VecDeque<(Key, Arc<Decoded>)>,
}
const CACHE_BYTES: usize = 96 * 1024 * 1024;

impl State {
    fn remember(&mut self, key: Key, decoded: Arc<Decoded>) {
        // Two workers may finish the same key concurrently.
        self.lru.retain(|(existing, _)| *existing != key);
        self.lru.push_front((key, decoded));
        while self.lru.len() > 500
            || self.lru.iter().map(|(_, d)| d.pixels.len()).sum::<usize>() > CACHE_BYTES
        {
            self.lru.pop_back();
        }
    }
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    cache: Cache,
    next: AtomicU64,
}
/// Cancellable thumbnail request. Dropping it cancels queued and in-flight I/O.
pub struct Request {
    cancel: gio::Cancellable,
    /// Exactly one result unless cancelled; receives owned decoded pixels.
    pub result: async_channel::Receiver<Result<Arc<Decoded>, Error>>,
}
impl Request {
    /// Cancel without waiting for a worker.
    pub fn cancel(&self) {
        self.cancel.cancel();
        self.result.close();
        while self.result.try_recv().is_ok() {}
    }
}
impl Drop for Request {
    fn drop(&mut self) {
        self.cancel();
    }
}
/// Shared image pool. Workers own no GTK objects and stop when the pool drops.
pub struct Pool {
    shared: Arc<Shared>,
    workers: usize,
}
impl Pool {
    /// Start min(4, available cores) workers. Thread startup failure is returned.
    pub fn new(cache: Cache) -> Result<Self, Error> {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(4);
        Self::with_workers(cache, workers)
    }
    /// Explicit worker count, clamped to 1–4, for isolated scheduling tests.
    pub fn with_workers(cache: Cache, workers: usize) -> Result<Self, Error> {
        let workers = workers.clamp(1, 4);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: vec![],
                pending: BTreeMap::new(),
                stopped: false,
                lru: VecDeque::new(),
            }),
            wake: Condvar::new(),
            cache,
            next: AtomicU64::new(0),
        });
        for _ in 0..workers {
            let task = shared.clone();
            if let Err(e) = std::thread::Builder::new()
                .name("tallboy-thumbnail".into())
                .spawn(move || worker(task))
            {
                let mut state = shared
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.stopped = true;
                shared.wake.notify_all();
                return Err(e.into());
            }
        }
        Ok(Self { shared, workers })
    }
    /// Number of running workers.
    pub fn worker_count(&self) -> usize {
        self.workers
    }
    /// Queue newest work first; config and source identity are captured for this request.
    pub fn request(
        &self,
        spec: Spec,
        pixels: u32,
        scale: f64,
        config: &tallboy_core::config::Config,
    ) -> Request {
        let cancel = gio::Cancellable::new();
        let (tx, result) = async_channel::bounded(1);
        let id = self.shared.next.fetch_add(1, Ordering::Relaxed);
        let work = Work {
            id,
            spec,
            size: Size::for_pixels(pixels, scale),
            config: config.clone(),
            cancel: cancel.clone(),
            tx,
        };
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.stopped {
            let _ = work.tx.try_send(Err(Error::Cancelled));
        } else {
            state.pending.insert(id, cancel.clone());
            state.queue.push(work);
            self.shared.wake.notify_one();
        }
        Request { cancel, result }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.stopped = true;
        for cancel in state.pending.values() {
            cancel.cancel();
        }
        state.queue.clear();
        self.shared.wake.notify_all();
    }
}
fn worker(shared: Arc<Shared>) {
    loop {
        let work = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while state.queue.is_empty() && !state.stopped {
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.stopped {
                return;
            }
            state.queue.pop()
        };
        let Some(work) = work else {
            continue;
        };
        let result = generate(&shared, &work);
        {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.pending.remove(&work.id);
        }
        if !work.cancel.is_cancelled() {
            let _ = work.tx.try_send(result);
        }
    }
}
fn generate(shared: &Shared, work: &Work) -> Result<Arc<Decoded>, Error> {
    if work.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    if skip(&shared.cache, &work.spec, &work.config) {
        return Err(Error::Skipped);
    }
    let key = Key {
        uri: work.spec.uri.clone(),
        mtime: work.spec.mtime,
        size: work.size,
    };
    {
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(index) = state.lru.iter().position(|(k, _)| k == &key)
            && let Some(entry) = state.lru.remove(index)
        {
            let decoded = entry.1.clone();
            state.lru.push_front(entry);
            return Ok(decoded);
        }
    }
    let pixbuf = if let Some(cached) = shared.cache.load(&work.spec, work.size)? {
        cached
    } else {
        if shared.cache.failed(&work.spec)? {
            return Err(Error::Skipped);
        }
        let location = tallboy_core::location::Location::parse(
            &work.spec.uri,
            Path::new("/"),
            Path::new("/"),
        )?;
        let file = gio::File::for_uri(location.to_uri());
        let generation = (|| {
            let stream = file.read(Some(&work.cancel))?;
            let decoded = gdk_pixbuf::Pixbuf::from_stream_at_scale(
                &stream,
                work.size.pixels(),
                work.size.pixels(),
                true,
                Some(&work.cancel),
            )?;
            stream.close(None::<&gio::Cancellable>)?;
            Ok::<_, glib::Error>(decoded.apply_embedded_orientation().unwrap_or(decoded))
        })();
        match generation {
            Ok(p) => {
                if work.cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                shared.cache.save(&work.spec, work.size, &p)?;
                p
            }
            Err(e) => {
                if work.cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                shared.cache.record_failure(&work.spec)?;
                return Err(e.into());
            }
        }
    };
    if work.cancel.is_cancelled() {
        return Err(Error::Cancelled);
    }
    let decoded = Arc::new(Decoded::from_pixbuf(&pixbuf));
    let mut state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.remember(key, decoded.clone());
    Ok(decoded)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoded_cache_evicts_large_images_and_keeps_the_newest() {
        let mut state = State {
            queue: vec![],
            pending: BTreeMap::new(),
            stopped: false,
            lru: VecDeque::new(),
        };
        let image = Arc::new(Decoded {
            width: 1024,
            height: 1024,
            rowstride: 4096,
            channels: 4,
            pixels: vec![0; 4 * 1024 * 1024],
        });
        for n in 0..30 {
            state.remember(
                Key {
                    uri: format!("file:///image-{n}.png"),
                    mtime: 0,
                    size: Size::XXLarge,
                },
                image.clone(),
            );
        }
        assert_eq!(state.lru.len(), 24);
        assert_eq!(state.lru.front().unwrap().0.uri, "file:///image-29.png");
        assert_eq!(state.lru.back().unwrap().0.uri, "file:///image-6.png");
        // Replacing a cached key does not duplicate it.
        state.remember(state.lru.back().unwrap().0.clone(), image);
        assert_eq!(state.lru.len(), 24);
        assert_eq!(state.lru.front().unwrap().0.uri, "file:///image-6.png");
    }
    #[test]
    fn cancelled_before_worker_starts_does_no_io() {
        let t = tempfile::tempdir().unwrap();
        let cache = Cache::new(t.path());
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: vec![],
                pending: BTreeMap::new(),
                stopped: false,
                lru: VecDeque::new(),
            }),
            wake: Condvar::new(),
            cache: cache.clone(),
            next: AtomicU64::new(0),
        });
        let pool = Pool {
            shared: shared.clone(),
            workers: 0,
        };
        let request = pool.request(
            Spec {
                uri: "file:///nonexistent-image".into(),
                mtime: 0,
                bytes: 1,
                mimetype: "image/png".into(),
            },
            128,
            1.0,
            &Default::default(),
        );
        request.cancel();
        let task = shared.clone();
        let thread = std::thread::spawn(move || worker(task));
        let start = std::time::Instant::now();
        while !shared.state.lock().unwrap().pending.is_empty() {
            assert!(start.elapsed() < std::time::Duration::from_secs(1));
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!cache.root().exists());
        assert!(request.result.try_recv().is_err());
        drop(pool);
        thread.join().unwrap();
    }
    #[test]
    fn embedded_orientation_rotates_pixels() {
        let p = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 2, 3).unwrap();
        p.fill(0xff0000ff);
        assert!(p.set_option("orientation", "6"));
        let oriented = p.apply_embedded_orientation().unwrap();
        assert_eq!((oriented.width(), oriented.height()), (3, 2));
    }
}
