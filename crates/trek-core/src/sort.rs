//! Sort specification and deterministic comparison of precomputed keys.
use crate::config::ValueError;
use std::{cmp::Ordering, fmt, str::FromStr};
/// Sort column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Name.
    Name,
    /// Description.
    Description,
    /// Size.
    Size,
    /// Modification time.
    Modified,
    /// Creation ("birth") time, where the filesystem records one.
    Created,
    /// Unix mode.
    Permissions,
    /// Owner.
    Owner,
    /// Group.
    Group,
    /// File name extension, compared without regard to ASCII case.
    Extension,
}
/// Sort direction, collation and directory policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    /// Column.
    pub column: Column,
    /// Descending within each group.
    pub descending: bool,
    /// Preserve case.
    pub case_sensitive: bool,
    /// Mix directories and files.
    pub mingle: bool,
}
impl Default for SortSpec {
    fn default() -> Self {
        Self {
            column: Column::Name,
            descending: false,
            case_sensitive: false,
            mingle: false,
        }
    }
}
impl fmt::Display for SortSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{};{};{}{}",
            match self.column {
                Column::Name => "name",
                Column::Description => "desc",
                Column::Size => "size",
                Column::Modified => "mtime",
                Column::Created => "btime",
                Column::Permissions => "perm",
                Column::Owner => "owner",
                Column::Group => "group",
                Column::Extension => "ext",
            },
            if self.descending {
                "descending"
            } else {
                "ascending"
            },
            if self.case_sensitive { "case;" } else { "" },
            if self.mingle { "mingle;" } else { "" }
        )
    }
}
impl FromStr for SortSpec {
    type Err = ValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(';');
        let column = match parts.next() {
            Some("name") => Column::Name,
            Some("desc") => Column::Description,
            Some("size") => Column::Size,
            Some("mtime") => Column::Modified,
            Some("btime") => Column::Created,
            Some("perm") => Column::Permissions,
            Some("owner") => Column::Owner,
            Some("group") => Column::Group,
            Some("ext") => Column::Extension,
            _ => return Err(ValueError(s.into())),
        };
        let descending = match parts.next() {
            Some("ascending") => false,
            Some("descending") => true,
            _ => return Err(ValueError(s.into())),
        };
        let mut result = Self {
            column,
            descending,
            ..Self::default()
        };
        for p in parts {
            match p {
                "case" => result.case_sensitive = true,
                "mingle" => result.mingle = true,
                "" => {}
                _ => return Err(ValueError(s.into())),
            }
        }
        Ok(result)
    }
}
/// All comparison inputs; GLib filename collation keys may be supplied by fs.
#[derive(Debug, Clone, Default)]
pub struct SortKeys {
    /// Display name.
    pub name: String,
    /// Raw and folded filename collation bytes.
    pub collation: Vec<u8>,
    /// Case-folded collation bytes.
    pub folded_collation: Vec<u8>,
    /// Directory status.
    pub directory: bool,
    /// Type description.
    pub description: String,
    /// Logical bytes.
    pub size: u64,
    /// Epoch seconds.
    pub modified: i64,
    /// Creation time in epoch seconds; 0 when unknown.
    pub created: i64,
    /// Mode bits.
    pub mode: u32,
    /// Owner display.
    pub owner: String,
    /// Group display.
    pub group: String,
}
/// Natural comparison with unbounded numeric runs (no integer overflow).
pub fn natural(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek(), b.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut x = String::new();
                let mut y = String::new();
                while a.peek().is_some_and(char::is_ascii_digit) {
                    if let Some(c) = a.next() {
                        x.push(c);
                    }
                }
                while b.peek().is_some_and(char::is_ascii_digit) {
                    if let Some(c) = b.next() {
                        y.push(c);
                    }
                }
                let xx = x.trim_start_matches('0');
                let yy = y.trim_start_matches('0');
                let cmp = xx.len().cmp(&yy.len()).then_with(|| xx.cmp(yy));
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            (Some(_), Some(_)) => {
                let cmp = a.next().cmp(&b.next());
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }
    }
}
/// Directories stay first even when descending; names break column ties.
pub fn compare(a: &SortKeys, b: &SortKeys, s: &SortSpec) -> Ordering {
    let names = || {
        let (ak, bk) = if s.case_sensitive {
            (&a.collation, &b.collation)
        } else {
            (&a.folded_collation, &b.folded_collation)
        };
        if !ak.is_empty() && !bk.is_empty() {
            ak.cmp(bk)
        } else if s.case_sensitive {
            natural(&a.name, &b.name)
        } else {
            natural(&a.name.to_lowercase(), &b.name.to_lowercase())
        }
    };
    compare_by_name(a, b, s, names)
}
/// Lower-cased extension bytes, including the leading dot; empty when there is none.
fn extension(name: &str) -> impl Iterator<Item = u8> + '_ {
    crate::naming::split_extension(name)
        .1
        .bytes()
        .map(|b| b.to_ascii_lowercase())
}
/// Borrowed comparison inputs. A caller that already owns these values (the
/// filesystem layer's metadata) sorts through this view instead of keeping a
/// second copy of every string per file.
#[derive(Debug, Clone, Copy)]
pub struct SortFacts<'a> {
    /// Display name.
    pub name: &'a str,
    /// Directory status.
    pub directory: bool,
    /// Type description.
    pub description: &'a str,
    /// Logical bytes.
    pub size: u64,
    /// Epoch seconds.
    pub modified: i64,
    /// Creation time in epoch seconds; 0 when unknown.
    pub created: i64,
    /// Mode bits.
    pub mode: u32,
    /// Owner name.
    pub owner: &'a str,
    /// Group name.
    pub group: &'a str,
}
impl SortKeys {
    /// The borrowed view of these keys.
    pub fn facts(&self) -> SortFacts<'_> {
        SortFacts {
            name: &self.name,
            directory: self.directory,
            description: &self.description,
            size: self.size,
            modified: self.modified,
            created: self.created,
            mode: self.mode,
            owner: &self.owner,
            group: &self.group,
        }
    }
}
/// Compare with precomputed opaque collation keys owned by the filesystem layer.
/// `names` must return ascending name order for the selected case policy.
pub fn compare_by_name(
    a: &SortKeys,
    b: &SortKeys,
    s: &SortSpec,
    names: impl Fn() -> Ordering,
) -> Ordering {
    compare_facts(&a.facts(), &b.facts(), s, names)
}
/// [`compare_by_name`] over borrowed inputs; the single implementation of the
/// column, direction and folders-first policy.
pub fn compare_facts(
    a: &SortFacts<'_>,
    b: &SortFacts<'_>,
    s: &SortSpec,
    names: impl Fn() -> Ordering,
) -> Ordering {
    if !s.mingle && a.directory != b.directory {
        return b.directory.cmp(&a.directory);
    }
    let cmp = match s.column {
        Column::Name => names(),
        Column::Description => a.description.cmp(b.description),
        Column::Size => a.size.cmp(&b.size),
        Column::Modified => a.modified.cmp(&b.modified),
        Column::Created => a.created.cmp(&b.created),
        Column::Permissions => a.mode.cmp(&b.mode),
        Column::Owner => a.owner.cmp(b.owner),
        Column::Group => a.group.cmp(b.group),
        Column::Extension => extension(a.name).cmp(extension(b.name)),
    }
    .then_with(names)
    .then_with(|| a.name.cmp(b.name));
    if s.descending { cmp.reverse() } else { cmp }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policies() {
        let a = SortKeys {
            name: "file2".into(),
            ..Default::default()
        };
        let b = SortKeys {
            name: "file10".into(),
            ..Default::default()
        };
        let d = SortKeys {
            name: "z".into(),
            directory: true,
            ..Default::default()
        };
        let mut s = SortSpec::default();
        assert_eq!(compare(&a, &b, &s), Ordering::Less);
        assert_eq!(compare(&d, &a, &s), Ordering::Less);
        s.descending = true;
        assert_eq!(compare(&d, &a, &s), Ordering::Less);
        s.mingle = true;
        assert_eq!(compare(&d, &a, &s), Ordering::Less);
        s.descending = false;
        assert_eq!(compare(&d, &a, &s), Ordering::Greater);
        s.column = Column::Size;
        assert_eq!(compare(&a, &b, &s), Ordering::Less);
    }
    #[test]
    fn case() {
        let a = SortKeys {
            name: "Zoo".into(),
            ..Default::default()
        };
        let b = SortKeys {
            name: "apple".into(),
            ..Default::default()
        };
        let mut s = SortSpec::default();
        assert_eq!(compare(&a, &b, &s), Ordering::Greater);
        s.case_sensitive = true;
        assert_eq!(compare(&a, &b, &s), Ordering::Less);
    }
    #[test]
    fn spec_codec() {
        for col in ["name", "desc", "size", "mtime", "perm", "owner", "group"] {
            for dir in ["ascending", "descending"] {
                for suffix in ["", "case;", "mingle;", "case;mingle;"] {
                    let text = format!("{col};{dir};{suffix}");
                    assert_eq!(text.parse::<SortSpec>().unwrap().to_string(), text);
                }
            }
        }
        assert_eq!(
            natural("x9999999999999999999999999999999", "x10"),
            Ordering::Greater
        );
    }
    #[test]
    fn extension_column() {
        let key = |name: &str| SortKeys {
            name: name.into(),
            ..Default::default()
        };
        let s = SortSpec {
            column: Column::Extension,
            ..Default::default()
        };
        assert_eq!(compare(&key("b.TXT"), &key("a.zip"), &s), Ordering::Less);
        assert_eq!(compare(&key("none"), &key("a.c"), &s), Ordering::Less);
        assert_eq!(compare(&key("a.rs"), &key("b.rs"), &s), Ordering::Less);
        assert_eq!(
            "ext;descending;".parse::<SortSpec>().unwrap().column,
            s.column
        );
        assert_eq!(s.to_string(), "ext;ascending;");
        assert!("name;ext:80;".parse::<crate::config::Columns>().is_ok());
    }
}
