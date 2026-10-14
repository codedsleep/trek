//! Display formatting with deterministic, locale-independent output.
/// Bytes rendered in IEC or SI units.
pub fn size(bytes: u64, si: bool) -> String {
    let base = if si { 1000.0 } else { 1024.0 };
    let units = if si {
        ["B", "kB", "MB", "GB", "TB", "PB", "EB"]
    } else {
        ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"]
    };
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= base && unit + 1 < units.len() {
        value /= base;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", units[unit])
    }
}
/// POSIX mode string, including special execute bits and common file types.
pub fn permissions(mode: u32) -> String {
    let mut out = String::from(match mode & 0o170000 {
        0o040000 => "d",
        0o120000 => "l",
        0o010000 => "p",
        0o140000 => "s",
        0o060000 => "b",
        0o020000 => "c",
        _ => "-",
    });
    for (shift, special) in [(6, 0o4000), (3, 0o2000), (0, 0o1000)] {
        out.push(if mode & (4 << shift) != 0 { 'r' } else { '-' });
        out.push(if mode & (2 << shift) != 0 { 'w' } else { '-' });
        let exec = mode & (1 << shift) != 0;
        out.push(if mode & special != 0 {
            if special == 0o1000 {
                if exec { 't' } else { 'T' }
            } else if exec {
                's'
            } else {
                'S'
            }
        } else if exec {
            'x'
        } else {
            '-'
        });
    }
    out
}
/// Epoch seconds as UTC `YYYY-MM-DD HH:MM`, including pre-epoch dates.
/// UI may use local GLib formatting instead when the user requests locale dates.
pub fn date(epoch: i64) -> String {
    let days = epoch.div_euclid(86400);
    let seconds = epoch.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}
/// Bytes per second.
pub fn speed(bytes_per_sec: u64, si: bool) -> String {
    format!("{}/s", size(bytes_per_sec, si))
}
/// Estimated time to completion, or unknown at zero speed.
pub fn remaining(bytes: u64, bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "Unknown".into();
    }
    let secs = bytes.div_ceil(bytes_per_sec);
    if secs >= 3600 {
        format!("{} h {} min", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{secs} s")
    }
}
/// Status bar for a folder, with optional hidden count and filter.
pub fn status(items: usize, hidden: usize, filter: Option<&str>) -> String {
    let mut s = format!("{items} item(s)");
    if hidden > 0 {
        s.push_str(&format!(" ({hidden} hidden)"));
    }
    if let Some(filter) = filter.filter(|s| !s.is_empty()) {
        s.push_str(&format!(" [filter: {filter}]"));
    }
    s
}
/// Status for one selected file.
pub fn selected_one(name: &str, bytes: u64, description: &str, si: bool) -> String {
    format!("\"{name}\" ({}) {description}", size(bytes, si))
}
/// Status for several selected files.
pub fn selected_many(count: usize, bytes: u64, si: bool) -> String {
    format!("{count} item(s) selected ({})", size(bytes, si))
}
/// Filesystem capacity status.
pub fn free_space(free: u64, total: u64, si: bool) -> String {
    format!(
        "Free space: {} (Total: {})",
        size(free, si),
        size(total, si)
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables() {
        for (b, si, want) in [
            (0, false, "0 B"),
            (1024, false, "1.0 KiB"),
            (1000, true, "1.0 kB"),
            (1048576, false, "1.0 MiB"),
        ] {
            assert_eq!(size(b, si), want);
        }
        for (m, want) in [
            (0o100644, "-rw-r--r--"),
            (0o040755, "drwxr-xr-x"),
            (0o104755, "-rwsr-xr-x"),
            (0o041700, "drwx-----T"),
        ] {
            assert_eq!(permissions(m), want);
        }
        assert_eq!(date(0), "1970-01-01 00:00");
        assert_eq!(date(-60), "1969-12-31 23:59");
        assert_eq!(date(951782400), "2000-02-29 00:00");
        assert_eq!(remaining(120, 2), "1 min 0 s");
        assert_eq!(remaining(1, 0), "Unknown");
        assert_eq!(speed(1024, false), "1.0 KiB/s");
        assert_eq!(
            status(2, 1, Some("*.rs")),
            "2 item(s) (1 hidden) [filter: *.rs]"
        );
        assert_eq!(selected_one("x", 1, "Text", false), "\"x\" (1 B) Text");
        assert_eq!(selected_many(2, 2, false), "2 item(s) selected (2 B)");
        assert_eq!(free_space(1, 2, false), "Free space: 1 B (Total: 2 B)");
    }
}
