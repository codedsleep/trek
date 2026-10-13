//! Optional timing instrumentation for headless and application benchmarks.
use std::time::Instant;
/// Log elapsed milliseconds only when TALLBOY_TRACE=1.
pub fn elapsed(label: &str, start: Instant) {
    if std::env::var_os("TALLBOY_TRACE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        eprintln!(
            "tallboy trace: {label}: {:.3} ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}
