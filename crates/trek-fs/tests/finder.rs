mod common;
use common::*;
use std::{
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
    time::{Duration, Instant},
};
use trek_fs::finder::{Finder, Hit, Options};

fn tree(root: &Path, files: &[&str]) {
    for file in files {
        let path = root.join(file);
        if file.ends_with('/') {
            fs::create_dir_all(&path).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, file).unwrap();
        }
    }
}

fn settle(finder: &mut Finder) {
    let start = Instant::now();
    loop {
        finder.tick();
        if finder.settled() {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "finder timed out"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn search(root: &Path, options: Options, query: &str) -> Vec<Hit> {
    let mut finder = Finder::start(root, options);
    finder.set_query(query);
    settle(&mut finder);
    let hits = finder.hits(0, 100);
    assert_eq!(hits.len() as u32, finder.counts().matched);
    hits
}

fn texts(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.text.as_str()).collect()
}

#[test]
fn lists_everything_below_the_folder_in_walking_order() {
    let root = temp();
    tree(
        root.path(),
        &[
            "src/main.rs",
            "src/view/list.rs",
            "README.md",
            "build.rs",
            "empty/",
        ],
    );
    let hits = search(root.path(), Options::default(), "");
    assert_eq!(
        texts(&hits),
        [
            "build.rs",
            "empty/",
            "README.md",
            "src/",
            "src/main.rs",
            "src/view/",
            "src/view/list.rs"
        ]
    );
    assert!(hits[1].dir && hits[3].dir && !hits[4].dir);
    assert_eq!(hits[6].path, root.path().join("src/view/list.rs"));
    assert_eq!(hits[5].path, root.path().join("src/view"));
    assert!(hits.iter().all(|hit| hit.matched.is_empty()));
}

#[test]
fn hidden_files_are_listed_on_request_and_git_never() {
    let root = temp();
    tree(
        root.path(),
        &[".config/app.toml", ".env", "shown.txt", ".git/HEAD"],
    );
    let plain = search(root.path(), Options::default(), "");
    assert_eq!(texts(&plain), ["shown.txt"]);
    let all = search(root.path(), Options { hidden: true }, "");
    assert_eq!(
        texts(&all),
        [".config/", ".config/app.toml", ".env", "shown.txt"]
    );
}

#[test]
fn ignore_files_are_honoured() {
    let root = temp();
    tree(
        root.path(),
        &[
            ".git/HEAD",
            "target/debug/app",
            "node_modules/x/index.js",
            "src/lib.rs",
            "notes.log",
        ],
    );
    fs::write(root.path().join(".gitignore"), "/target/\n*.log\n").unwrap();
    fs::write(root.path().join(".ignore"), "node_modules\n").unwrap();
    assert_eq!(
        texts(&search(root.path(), Options::default(), "")),
        ["src/", "src/lib.rs"]
    );
    // Seen from inside an ignored folder its files are what was asked for.
    assert_eq!(
        texts(&search(&root.path().join("target"), Options::default(), "")),
        ["debug/", "debug/app"]
    );
}

#[test]
fn queries_are_fuzzy_ranked_and_marked() {
    let root = temp();
    tree(
        root.path(),
        &[
            "crates/app/src/window/mod.rs",
            "crates/app/src/window/vim.rs",
            "crates/core/src/keymap.rs",
            "docs/Vimwiki.md",
            "scripts/dev-run.sh",
        ],
    );
    let hits = search(root.path(), Options::default(), "wvim");
    assert_eq!(texts(&hits), ["crates/app/src/window/vim.rs"]);
    let marked: Vec<&str> = hits[0]
        .matched
        .iter()
        .map(|range| &hits[0].text[range.clone()])
        .collect();
    assert_eq!(marked, ["w", "vim"]);

    // Without capitals case is ignored; with one it is not. Of two equal
    // matches the shorter path comes first.
    assert_eq!(
        texts(&search(root.path(), Options::default(), "vim")),
        ["docs/Vimwiki.md", "crates/app/src/window/vim.rs"]
    );
    assert_eq!(
        texts(&search(root.path(), Options::default(), "Vim")),
        ["docs/Vimwiki.md"]
    );
    // fzf's syntax: words in any order, a suffix, a negation.
    assert_eq!(
        texts(&search(root.path(), Options::default(), "rs$ core")),
        ["crates/core/src/keymap.rs"]
    );
    assert_eq!(
        texts(&search(root.path(), Options::default(), "window !vim")),
        ["crates/app/src/window/", "crates/app/src/window/mod.rs"]
    );
    assert!(search(root.path(), Options::default(), "zzzz").is_empty());
}

#[test]
fn a_new_query_replaces_the_result() {
    let root = temp();
    tree(root.path(), &["alpha.txt", "beta.txt", "gamma.txt"]);
    let mut finder = Finder::start(root.path(), Options::default());
    settle(&mut finder);
    assert_eq!(finder.counts().matched, 3);
    finder.set_query("bet");
    settle(&mut finder);
    assert_eq!(texts(&finder.hits(0, 10)), ["beta.txt"]);
    assert_eq!((finder.counts().matched, finder.counts().total), (1, 3));
    finder.set_query("");
    settle(&mut finder);
    assert_eq!(finder.counts().matched, 3);
    // A window past the end is empty, one across it is cut short.
    assert_eq!(texts(&finder.hits(2, 10)), ["gamma.txt"]);
    assert!(finder.hits(3, 10).is_empty());
}

#[test]
fn paths_survive_names_the_matcher_cannot_hold() {
    let root = temp();
    // A decomposed "é", and a name that is not UTF-8.
    let accented = "cafe\u{301}.txt";
    fs::write(root.path().join(accented), "x").unwrap();
    let broken = std::ffi::OsStr::from_bytes(b"bro\xffken.txt");
    fs::write(root.path().join(broken), "x").unwrap();
    let hits = search(root.path(), Options::default(), "caf");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text, accented);
    assert_eq!(hits[0].path, root.path().join(accented));
    let hits = search(root.path(), Options::default(), "broken");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, root.path().join(broken));
    assert!(hits[0].path.exists());
}

#[test]
fn a_link_to_a_folder_is_a_folder_and_is_not_followed() {
    let root = temp();
    tree(root.path(), &["real/inside.txt"]);
    std::os::unix::fs::symlink("real", root.path().join("link")).unwrap();
    let hits = search(root.path(), Options::default(), "");
    assert_eq!(texts(&hits), ["link/", "real/", "real/inside.txt"]);
    assert!(hits[0].dir);
}

#[test]
fn wakes_once_until_ticked() {
    let root = temp();
    tree(root.path(), &["a", "b", "c"]);
    let mut finder = Finder::start(root.path(), Options::default());
    let wake = finder.wake();
    wake.recv_blocking().unwrap();
    settle(&mut finder);
    // Nothing is happening any more, so nothing wakes.
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        wake.try_recv().is_err() || {
            finder.tick();
            wake.try_recv().is_err()
        }
    );
}
