//! Zoom and type-ahead helpers.
/// Supported visual size steps.
pub const ZOOM_STEPS: &[u32] = &[16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512];
/// Move to the next/previous supported step, clamped to the ends.
pub fn zoom(size: u32, increase: bool) -> u32 {
    if increase {
        ZOOM_STEPS
            .iter()
            .copied()
            .find(|s| *s > size)
            .unwrap_or(512)
    } else {
        ZOOM_STEPS
            .iter()
            .copied()
            .rev()
            .find(|s| *s < size)
            .unwrap_or(16)
    }
}
/// Prefix search with wraparound. Starts after the cursor when cycling.
pub fn type_ahead(
    names: &[impl AsRef<str>],
    prefix: &str,
    cursor: Option<usize>,
    cycle: bool,
) -> Option<usize> {
    if names.is_empty() || prefix.is_empty() {
        return None;
    }
    let start = cursor.map_or(0, |c| {
        if cycle {
            (c + 1) % names.len()
        } else {
            c % names.len()
        }
    });
    let prefix = prefix.to_lowercase();
    (0..names.len())
        .map(|offset| (start + offset) % names.len())
        .find(|&i| names[i].as_ref().to_lowercase().starts_with(&prefix))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn helpers() {
        assert_eq!(zoom(48, true), 64);
        assert_eq!(zoom(48, false), 32);
        assert_eq!(zoom(512, true), 512);
        assert_eq!(zoom(16, false), 16);
        let names = ["apple", "apricot", "banana"];
        assert_eq!(type_ahead(&names, "AP", None, false), Some(0));
        assert_eq!(type_ahead(&names, "ap", Some(0), true), Some(1));
        assert_eq!(type_ahead(&names, "ap", Some(1), true), Some(0));
        assert_eq!(type_ahead(&names, "z", None, false), None);
    }
}
