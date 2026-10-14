//! Timing, memory and stall probes behind `TALLBOY_TRACE=1`, and the debug
//! counters used for leak checks.

use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    rc::Rc,
    sync::OnceLock,
    sync::atomic::{AtomicI64, Ordering},
    time::{Duration, Instant},
};

static START: OnceLock<Instant> = OnceLock::new();

/// Number of live tab pages; must return to zero when every window is closed.
pub static LIVE_TABS: AtomicI64 = AtomicI64::new(0);

/// Number of folders shown in the files column of a tab or watched by the
/// directory tree; must return to zero with the tabs.
pub static LIVE_FOLDERS: AtomicI64 = AtomicI64::new(0);

/// Number of folder listings alive: the ones columns show and the ones
/// their tabs keep for going back to; must return to zero with the tabs.
pub static LIVE_LISTINGS: AtomicI64 = AtomicI64::new(0);

/// A value counted in one of the leak counters for as long as it lives.
pub struct Counted<T> {
    value: T,
    counter: &'static AtomicI64,
}

impl<T> Counted<T> {
    pub fn new(value: T, counter: &'static AtomicI64) -> Self {
        count(counter, 1);
        Self { value, counter }
    }
}

impl<T> std::ops::Deref for Counted<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> Drop for Counted<T> {
    fn drop(&mut self) {
        count(self.counter, -1);
    }
}

/// The counters, as the state line and `TALLBOY_TRACE` print them.
pub fn live() -> String {
    format!(
        "live_tabs={} live_folders={} live_listings={}",
        LIVE_TABS.load(Ordering::Relaxed),
        LIVE_FOLDERS.load(Ordering::Relaxed),
        LIVE_LISTINGS.load(Ordering::Relaxed)
    )
}

/// Records when the process started; every trace line is relative to it.
///
/// `main` cannot see the time spent in `exec` and the dynamic linker, so a
/// benchmark may pass the wall-clock time just before `exec` in `TALLBOY_T0`
/// (nanoseconds since the epoch) and the traces then include it.
pub fn mark_start() {
    let now = Instant::now();
    let before_main = std::env::var("TALLBOY_T0")
        .ok()
        .and_then(|t0| t0.parse::<u128>().ok())
        .and_then(|t0| {
            let wall = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            u64::try_from(wall.checked_sub(t0)?).ok()
        })
        .map(Duration::from_nanos)
        .filter(|d| *d < Duration::from_secs(60));
    let start = before_main.and_then(|d| now.checked_sub(d)).unwrap_or(now);
    let _ = START.set(start);
}

/// The instant recorded by [`mark_start`].
pub fn start() -> Instant {
    *START.get_or_init(Instant::now)
}

/// Whether `TALLBOY_TRACE=1` is set.
pub fn tracing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("TALLBOY_TRACE").is_some_and(|v| v == "1"))
}

/// Logs milliseconds since process start under `label` when tracing.
pub fn trace(label: &str) {
    tallboy_fs::trace::elapsed(label, start());
}

/// Logs a free-form trace line when tracing.
pub fn note(text: &str) {
    if tracing() {
        eprintln!("tallboy trace: {text}");
    }
}

/// Resident set size in KiB, read from `/proc/self/statm`.
pub fn rss_kib() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4)
}

/// Measures the longest gap between main-loop iterations while it is alive.
pub struct StallWatch {
    worst: Rc<Cell<Duration>>,
    source: Option<glib::SourceId>,
}

impl StallWatch {
    const TICK: Duration = Duration::from_millis(2);

    /// Starts sampling on the default main context.
    pub fn start() -> Self {
        let worst = Rc::new(Cell::new(Duration::ZERO));
        let last = Cell::new(Instant::now());
        let seen = worst.clone();
        let source = glib::timeout_add_local(Self::TICK, move || {
            let now = Instant::now();
            let gap = now
                .duration_since(last.replace(now))
                .saturating_sub(Self::TICK);
            if gap > seen.get() {
                seen.set(gap);
            }
            glib::ControlFlow::Continue
        });
        Self {
            worst,
            source: Some(source),
        }
    }

    /// Stops sampling and returns the longest stall seen.
    pub fn finish(mut self) -> Duration {
        if let Some(source) = self.source.take() {
            source.remove();
        }
        self.worst.get()
    }
}

impl Drop for StallWatch {
    fn drop(&mut self) {
        if let Some(source) = self.source.take() {
            source.remove();
        }
    }
}

/// Adjusts a live-object counter.
pub fn count(counter: &AtomicI64, delta: i64) {
    counter.fetch_add(delta, Ordering::Relaxed);
}

/// Runs `done` once, after the next frame of `widget` has been painted.
pub fn after_next_paint(widget: &impl IsA<gtk::Widget>, done: impl FnOnce() + 'static) {
    let Some(clock) = widget.frame_clock() else {
        return;
    };
    let done = Cell::new(Some(done));
    let handler: Rc<Cell<Option<glib::SignalHandlerId>>> = Rc::default();
    let slot = handler.clone();
    handler.set(Some(clock.connect_after_paint(move |clock| {
        if let Some(id) = slot.take() {
            clock.disconnect(id);
        }
        if let Some(done) = done.take() {
            done();
        }
    })));
    clock.request_phase(gtk::gdk::FrameClockPhase::PAINT);
}
