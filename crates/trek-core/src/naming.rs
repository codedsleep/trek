//! File name validation and collision naming, without filesystem access.
/// A name is unsuitable for one directory entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("A name must not be empty, contain '/' or NUL, or be '.' or '..'.")]
pub struct InvalidName;
/// Validate one display name; newline and leading dash are legal.
pub fn validate(name: &str) -> Result<(), InvalidName> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\0']) {
        Err(InvalidName)
    } else {
        Ok(())
    }
}
/// Split extension, treating common compressed tar extensions as one suffix.
pub fn split_extension(name: &str) -> (&str, &str) {
    for ext in [".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst"] {
        if let Some(base) = name.strip_suffix(ext)
            && !base.is_empty()
        {
            return (base, ext);
        }
    }
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}
/// UTF-8 byte range of the base name; UI may convert to character offsets.
pub fn rename_range(name: &str) -> std::ops::Range<usize> {
    0..split_extension(name).0.len()
}
/// Choose a free name with `(copy)`, `(copy 2)` suffixes; caller supplies existence.
pub fn unique_name(name: &str, mut exists: impl FnMut(&str) -> bool) -> String {
    if !exists(name) {
        return name.into();
    }
    let (base, ext) = split_extension(name);
    let mut n = 1u64;
    loop {
        let next = if n == 1 {
            format!("{base} (copy){ext}")
        } else {
            format!("{base} (copy {n}){ext}")
        };
        if !exists(&next) {
            return next;
        }
        n = n.saturating_add(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        for (name, base, ext) in [
            ("archive.tar.gz", "archive", ".tar.gz"),
            (".profile", ".profile", ""),
            ("plain", "plain", ""),
            ("a.txt", "a", ".txt"),
            ("tail.", "tail.", ""),
        ] {
            assert_eq!(split_extension(name), (base, ext));
            assert_eq!(rename_range(name), 0..base.len());
        }
        for name in ["", "/", ".", "..", "a/b", "a\0b"] {
            assert!(validate(name).is_err());
        }
        assert!(validate("line\nbreak").is_ok());
        assert_eq!(
            unique_name("a.txt", |s| ["a.txt", "a (copy).txt"].contains(&s)),
            "a (copy 2).txt"
        );
    }
}
