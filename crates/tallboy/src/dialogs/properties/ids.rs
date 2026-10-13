//! User and group names ↔ numeric ids, from `/etc/passwd` and `/etc/group`.
//! Accounts that only a directory service knows are not found; a number can
//! always be typed instead.

use std::os::unix::fs::MetadataExt;

fn lookup(file: &str, name: &str) -> Option<u32> {
    std::fs::read_to_string(file)
        .ok()?
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            (fields.next() == Some(name))
                .then(|| fields.nth(1)?.parse().ok())
                .flatten()
        })
}

/// A user name or a number, as a uid.
pub fn uid(text: &str) -> Option<u32> {
    let text = text.trim();
    text.parse().ok().or_else(|| lookup("/etc/passwd", text))
}

/// A group name or a number, as a gid.
pub fn gid(text: &str) -> Option<u32> {
    let text = text.trim();
    text.parse().ok().or_else(|| lookup("/etc/group", text))
}

/// The uid this process runs as.
pub fn current_uid() -> Option<u32> {
    std::fs::metadata("/proc/self").ok().map(|meta| meta.uid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_numbers() {
        assert_eq!(uid("0"), Some(0));
        assert_eq!(uid("root"), Some(0));
        assert_eq!(gid("root"), Some(0));
        assert_eq!(gid(" 1000 "), Some(1000));
        assert_eq!(uid("no-such-user-tb"), None);
        assert!(current_uid().is_some());
    }
}
