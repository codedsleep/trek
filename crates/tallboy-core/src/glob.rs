//! Small Unicode glob matcher, with memoized wildcard expansion.
use std::collections::HashMap;
/// Match `*`, `?`, and character classes (ranges, `!`/`^` negation).
/// A malformed class is treated as a literal '['.
pub fn matches(pattern: &str, text: &str) -> bool {
    fn run(
        p: &[char],
        t: &[char],
        i: usize,
        j: usize,
        m: &mut HashMap<(usize, usize), bool>,
    ) -> bool {
        if let Some(v) = m.get(&(i, j)) {
            return *v;
        }
        let result = if i == p.len() {
            j == t.len()
        } else {
            match p[i] {
                '*' => run(p, t, i + 1, j, m) || (j < t.len() && run(p, t, i, j + 1, m)),
                '?' => j < t.len() && run(p, t, i + 1, j + 1, m),
                '[' => {
                    if let Some(end) = p[i + 1..].iter().position(|c| *c == ']').map(|n| n + i + 1)
                    {
                        let mut k = i + 1;
                        let neg = k < end && matches!(p[k], '!' | '^');
                        if neg {
                            k += 1;
                        }
                        let mut hit = false;
                        while k < end && j < t.len() {
                            if k + 2 < end && p[k + 1] == '-' {
                                hit |= p[k] <= t[j] && t[j] <= p[k + 2];
                                k += 3;
                            } else {
                                hit |= p[k] == t[j];
                                k += 1;
                            }
                        }
                        j < t.len() && (hit != neg) && run(p, t, end + 1, j + 1, m)
                    } else {
                        j < t.len() && t[j] == '[' && run(p, t, i + 1, j + 1, m)
                    }
                }
                c => j < t.len() && c == t[j] && run(p, t, i + 1, j + 1, m),
            }
        };
        m.insert((i, j), result);
        result
    }
    run(
        &pattern.chars().collect::<Vec<_>>(),
        &text.chars().collect::<Vec<_>>(),
        0,
        0,
        &mut HashMap::new(),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table() {
        for (p, t, want) in [
            ("IMG_??.jpg", "IMG_12.jpg", true),
            ("IMG_??.jpg", "IMG_1.jpg", false),
            ("[a-z]*", "hello", true),
            ("[!a-c]", "z", true),
            ("[!a-c]", "b", false),
            ("?.txt", "é.txt", true),
            ("*.rs", "main.rs", true),
            ("a[", "a[", true),
            ("*", "", true),
        ] {
            assert_eq!(matches(p, t), want, "{p} {t}");
        }
    }
}
