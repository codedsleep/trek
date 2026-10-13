//! The fuzzy finder behind `z`: every file below a folder, matched against
//! a query as it is typed.
//!
//! The folder is walked on one thread (`ignore`, the walker of ripgrep and
//! fd) and matched on others (`nucleo`, Helix's picker), so the main loop
//! only ever asks for the rows it shows. Nothing here touches the files.

use ignore::WalkBuilder;
use nucleo::{
    Config, Injector, Matcher, Nucleo, Utf32String,
    pattern::{CaseMatching, Normalization},
};
use std::{
    ffi::OsStr,
    ops::Range,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

/// Files listed at most. A walk that reaches it stops and says so; what a
/// query is matched against is then the first million in walking order.
pub const LIMIT: u32 = 1_000_000;

/// How long `tick` waits for a match in progress, in milliseconds. Short
/// enough to go unnoticed, long enough that most queries answer at once.
const WAIT_MS: u64 = 10;

/// What a walk lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    /// Hidden files and the contents of hidden folders too.
    pub hidden: bool,
}

struct Entry {
    dir: bool,
    /// The path, when the text it is matched by cannot give it back: the
    /// matcher keeps only the first character of each grapheme, and a name
    /// that is not UTF-8 was listed with replacement characters.
    raw: Option<Box<Path>>,
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    walking: AtomicBool,
    truncated: AtomicBool,
    /// There is news since the last `tick`.
    dirty: AtomicBool,
}

/// One row of the result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The path relative to the folder searched, a folder's ending in `/`.
    pub text: String,
    /// The same path, absolute, as the file system has it.
    pub path: PathBuf,
    /// A folder, or a symbolic link to one.
    pub dir: bool,
    /// The parts of `text` the query matched, as byte ranges in order.
    pub matched: Vec<Range<usize>>,
}

/// How far a search has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// Files that match the query.
    pub matched: u32,
    /// Files listed so far.
    pub total: u32,
    /// The folder is still being walked.
    pub walking: bool,
    /// The query is still being matched against some of the files.
    pub matching: bool,
    /// The walk stopped at [`LIMIT`].
    pub truncated: bool,
}

/// A search below one folder. Dropping it stops the walk.
pub struct Finder {
    root: PathBuf,
    nucleo: Nucleo<Entry>,
    /// Finds which characters matched, for the rows that are shown.
    matcher: Matcher,
    shared: Arc<Shared>,
    wake: async_channel::Receiver<()>,
    walking: bool,
    matching: bool,
}

impl Finder {
    /// Starts walking `root`.
    pub fn start(root: &Path, options: Options) -> Self {
        let shared = Arc::new(Shared::default());
        shared.walking.store(true, Ordering::Relaxed);
        let (tx, wake) = async_channel::bounded(1);
        // Called for every file listed: all it may cost is one swap.
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::new({
            let shared = shared.clone();
            move || {
                if !shared.dirty.swap(true, Ordering::AcqRel) {
                    let _ = tx.try_send(());
                }
            }
        });
        let mut config = Config::DEFAULT;
        config.set_match_paths();
        let nucleo = Nucleo::new(config.clone(), notify.clone(), None, 1);
        let injector = nucleo.injector();
        let walk = {
            let (root, shared) = (root.to_owned(), shared.clone());
            move || {
                walk(&root, options, &injector, &shared);
                shared.walking.store(false, Ordering::Release);
                notify();
            }
        };
        if let Err(e) = thread::Builder::new()
            .name("finder-walk".into())
            .spawn(walk)
        {
            log::warn!("fuzzy finder: no thread to walk the folder: {e}");
            shared.walking.store(false, Ordering::Release);
        }
        Self {
            root: root.to_owned(),
            nucleo,
            matcher: Matcher::new(config),
            shared,
            wake,
            walking: true,
            matching: false,
        }
    }

    /// The folder searched.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Yields once whenever there is something new for [`Finder::tick`] to
    /// pick up, and not again before `tick` has been called.
    pub fn wake(&self) -> async_channel::Receiver<()> {
        self.wake.clone()
    }

    /// Sets the query, in fzf's syntax: words in any order, `'exact`,
    /// `^start`, `end$`, `!not`. Without capitals it ignores case.
    pub fn set_query(&mut self, query: &str) {
        self.nucleo
            .pattern
            .reparse(0, query, CaseMatching::Smart, Normalization::Smart, false);
    }

    /// Brings the result up to date with the query and with the files listed
    /// since the last call. Returns whether rows or counts changed.
    pub fn tick(&mut self) -> bool {
        self.shared.dirty.store(false, Ordering::Release);
        // Read before the tick: a walk seen to have ended has all its files
        // in what the tick picks up.
        let walking = self.shared.walking.load(Ordering::Acquire);
        let status = self.nucleo.tick(WAIT_MS);
        let settled = self.matching != status.running || self.walking != walking;
        self.matching = status.running;
        self.walking = walking;
        status.changed || settled
    }

    /// Whether the walk is over and the query has been matched against all
    /// of it, as of the last [`Finder::tick`].
    pub fn settled(&self) -> bool {
        !self.walking && !self.matching
    }

    /// How far the search has come, as of the last [`Finder::tick`].
    pub fn counts(&self) -> Counts {
        let snapshot = self.nucleo.snapshot();
        Counts {
            matched: snapshot.matched_item_count(),
            total: snapshot.item_count(),
            walking: self.walking,
            matching: self.matching,
            truncated: self.shared.truncated.load(Ordering::Acquire),
        }
    }

    /// Up to `count` rows of the result from `start`, best match first.
    pub fn hits(&mut self, start: u32, count: u32) -> Vec<Hit> {
        let snapshot = self.nucleo.snapshot();
        let end = start
            .saturating_add(count)
            .min(snapshot.matched_item_count());
        if start >= end {
            return Vec::new();
        }
        let pattern = snapshot.pattern().column_pattern(0);
        let mut indices = Vec::new();
        snapshot
            .matched_items(start..end)
            .map(|item| {
                let column = &item.matcher_columns[0];
                indices.clear();
                pattern.indices(column.slice(..), &mut self.matcher, &mut indices);
                indices.sort_unstable();
                indices.dedup();
                let text = match &item.data.raw {
                    Some(raw) => listed(raw, item.data.dir),
                    None => column.to_string(),
                };
                let relative = match &item.data.raw {
                    Some(raw) => raw.to_path_buf(),
                    None => PathBuf::from(text.trim_end_matches('/')),
                };
                Hit {
                    matched: byte_ranges(&text, column, &indices),
                    path: self.root.join(relative),
                    dir: item.data.dir,
                    text,
                }
            })
            .collect()
    }
}

impl Drop for Finder {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}

/// The text a path is listed and matched by.
fn listed(relative: &Path, dir: bool) -> String {
    let mut text = relative.to_string_lossy().into_owned();
    if dir {
        text.push('/');
    }
    text
}

/// Names compared without regard to ASCII case, so that `README` does not
/// come before everything in lower case.
fn by_name(a: &OsStr, b: &OsStr) -> std::cmp::Ordering {
    let (a_bytes, b_bytes) = (a.as_encoded_bytes(), b.as_encoded_bytes());
    a_bytes
        .iter()
        .map(u8::to_ascii_lowercase)
        .cmp(b_bytes.iter().map(u8::to_ascii_lowercase))
        .then_with(|| a.cmp(b))
}

/// Lists what is below `root`, depth first and each folder by name: the
/// order rows have before there is a query, and among equal matches.
///
/// Like fd it leaves out what `.gitignore` (inside a repository), `.ignore`
/// and the user's global ignore file exclude, and never descends into
/// `.git`. Symbolic links are listed and not followed.
fn walk(root: &Path, options: Options, injector: &Injector<Entry>, shared: &Shared) {
    let walker = WalkBuilder::new(root)
        .hidden(!options.hidden)
        .sort_by_file_name(by_name)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();
    let mut listed_count = 0;
    for entry in walker {
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        // A folder that cannot be read is left out, like one that is ignored.
        let Ok(entry) = entry else { continue };
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        if entry.depth() == 0 {
            continue;
        }
        if listed_count == LIMIT {
            shared.truncated.store(true, Ordering::Release);
            return;
        }
        listed_count += 1;
        let dir = match entry.file_type() {
            Some(kind) if kind.is_symlink() => entry.path().is_dir(),
            Some(kind) => kind.is_dir(),
            None => false,
        };
        let text = listed(relative, dir);
        let raw = (!text.is_ascii()).then(|| Box::from(relative));
        injector.push(Entry { dir, raw }, |_, columns| {
            columns[0] = Utf32String::from(text);
        });
    }
}

/// Turns positions in the matcher's text into byte ranges of `text`.
///
/// For ASCII the two are the same. Otherwise the matcher has one character
/// for each grapheme of `text`, its first, and a position covers the whole
/// grapheme.
fn byte_ranges(text: &str, column: &Utf32String, indices: &[u32]) -> Vec<Range<usize>> {
    let ranges: Vec<Range<usize>> = match column {
        Utf32String::Ascii(_) => indices
            .iter()
            .map(|&index| index as usize..index as usize + 1)
            .collect(),
        Utf32String::Unicode(firsts) => {
            // Where each grapheme starts: the next place its first
            // character occurs.
            let mut starts = Vec::with_capacity(firsts.len() + 1);
            let mut rest = text.char_indices();
            for first in firsts.iter() {
                match rest.by_ref().find(|(_, c)| c == first) {
                    Some((at, _)) => starts.push(at),
                    None => return Vec::new(),
                }
            }
            starts.push(text.len());
            indices
                .iter()
                .filter_map(|&index| {
                    let index = index as usize;
                    Some(*starts.get(index)?..*starts.get(index + 1)?)
                })
                .collect()
        }
    };
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => merged.push(range),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_positions_are_bytes_and_neighbours_merge() {
        let column = Utf32String::from("src/main.rs");
        assert_eq!(
            byte_ranges("src/main.rs", &column, &[0, 4, 5, 6, 10]),
            vec![0..1, 4..7, 10..11]
        );
    }

    #[test]
    fn a_position_covers_its_whole_grapheme() {
        // "é" written as "e" and a combining accent: one grapheme, of which
        // the matcher keeps the "e".
        let text = "cafe\u{301}/über.txt";
        let column = Utf32String::from(text);
        assert_eq!(column.len(), 13);
        let ranges = byte_ranges(text, &column, &[3, 5]);
        assert_eq!(ranges, vec![3..6, 7..9]);
        assert_eq!(&text[ranges[0].clone()], "e\u{301}");
        assert_eq!(&text[ranges[1].clone()], "ü");
    }

    #[test]
    fn names_sort_without_regard_to_case() {
        let mut names = ["src", "README.md", "Cargo.toml", "build.rs", "readme.md"].map(OsStr::new);
        names.sort_by(|a, b| by_name(a, b));
        assert_eq!(
            names.map(|name| name.to_str().unwrap_or_default()),
            ["build.rs", "Cargo.toml", "README.md", "readme.md", "src"]
        );
    }
}
