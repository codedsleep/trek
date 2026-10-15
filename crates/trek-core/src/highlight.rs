//! Syntax highlighting for the preview column, the way bat colours a file.
//!
//! The grammars and themes are bat's own, through `two-face`; the result is
//! Pango markup, which is plain text the toolkit can show as it is. Loading
//! the grammars takes a moment, so [`warm_up`] does it before the first file
//! is previewed; everything here may run on any thread.

use std::{fmt::Write as _, sync::OnceLock};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, FontStyle, Theme},
    parsing::{SyntaxReference, SyntaxSet},
    util::LinesWithEndings,
};
use two_face::theme::EmbeddedThemeName;

/// bat's grammars, with TOML, TypeScript, Dockerfile and the rest.
fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// Nord on a dark background; it has no light variant, so a light one gets
/// bat's default light theme.
fn themes() -> &'static (Theme, Theme) {
    static THEMES: OnceLock<(Theme, Theme)> = OnceLock::new();
    THEMES.get_or_init(|| {
        let set = two_face::theme::extra();
        (
            set.get(EmbeddedThemeName::Nord).clone(),
            set.get(EmbeddedThemeName::MonokaiExtendedLight).clone(),
        )
    })
}

/// Loads the grammars and themes, so that the first preview does not wait.
pub fn warm_up() {
    syntaxes();
    themes();
}

/// The grammar for a file called `name` whose first line is `first_line`:
/// a `#!` line names its interpreter; otherwise the whole name (`Makefile`),
/// the extension, then whatever else the first line gives away (`<?xml`).
fn syntax_for<'a>(set: &'a SyntaxSet, name: &str, first_line: &str) -> Option<&'a SyntaxReference> {
    let extension = name.rsplit_once('.').map(|(_, extension)| extension);
    let by_first_line = || set.find_syntax_by_first_line(first_line);
    first_line
        .starts_with("#!")
        .then(by_first_line)
        .flatten()
        .or_else(|| set.find_syntax_by_extension(name))
        .or_else(|| extension.and_then(|extension| set.find_syntax_by_extension(extension)))
        .or_else(by_first_line)
        .filter(|syntax| syntax.name != "Plain Text")
}

/// Escapes text for Pango markup.
fn escape(text: &str, into: &mut String) {
    for c in text.chars() {
        match c {
            '&' => into.push_str("&amp;"),
            '<' => into.push_str("&lt;"),
            '>' => into.push_str("&gt;"),
            '\'' => into.push_str("&#39;"),
            '"' => into.push_str("&quot;"),
            c => into.push(c),
        }
    }
}

/// `text` from a file called `name` as Pango markup, coloured for a dark
/// background or a light one; `None` when no grammar knows the file.
///
/// The theme's plain foreground is left out, so that unhighlighted text has
/// the toolkit's own colour and stays readable whatever the background.
pub fn markup(name: &str, text: &str, dark: bool) -> Option<String> {
    let set = syntaxes();
    let first_line = text.lines().next().unwrap_or_default();
    let syntax = syntax_for(set, name, first_line)?;
    let (dark_theme, light_theme) = themes();
    let theme = if dark { dark_theme } else { light_theme };
    let plain = theme.settings.foreground;
    let mut lines = HighlightLines::new(syntax, theme);
    let mut markup = String::with_capacity(text.len() * 3);
    for line in LinesWithEndings::from(text) {
        // A grammar that fails mid-file leaves the rest uncoloured.
        let Ok(spans) = lines.highlight_line(line, set) else {
            escape(line, &mut markup);
            continue;
        };
        for (style, piece) in spans {
            span(
                &mut markup,
                piece,
                style.foreground,
                plain,
                style.font_style,
            );
        }
    }
    Some(markup)
}

fn span(markup: &mut String, text: &str, color: Color, plain: Option<Color>, font: FontStyle) {
    let colored = Some(color) != plain && color.a > 0;
    let bold = font.contains(FontStyle::BOLD);
    let italic = font.contains(FontStyle::ITALIC);
    if !(colored || bold || italic) || text.trim().is_empty() {
        escape(text, markup);
        return;
    }
    markup.push_str("<span");
    if colored {
        let _ = write!(
            markup,
            " foreground=\"#{:02x}{:02x}{:02x}\"",
            color.r, color.g, color.b
        );
    }
    if bold {
        markup.push_str(" weight=\"bold\"");
    }
    if italic {
        markup.push_str(" style=\"italic\"");
    }
    markup.push('>');
    escape(text, markup);
    markup.push_str("</span>");
}

/// The line numbers for `lines` lines, right-aligned, one per line.
pub fn gutter(lines: usize) -> String {
    let width = lines.max(1).to_string().len();
    let mut gutter = String::with_capacity(lines * (width + 1));
    for number in 1..=lines {
        if number > 1 {
            gutter.push('\n');
        }
        let _ = write!(gutter, "{number:>width$}");
    }
    gutter
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip(markup: &str) -> String {
        let mut text = String::new();
        let mut in_tag = false;
        for c in markup.chars() {
            match c {
                '<' => in_tag = true,
                '>' if in_tag => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&amp;", "&")
    }

    #[test]
    fn code_is_coloured_and_keeps_its_text() {
        let code = "fn main() {\n    let s = \"<a & b>\";\n}\n";
        let markup = markup("main.rs", code, true).expect("Rust is known");
        assert!(markup.contains("<span foreground=\"#"));
        assert_eq!(strip(&markup), code);
        assert!(markup.contains("&lt;a &amp; b&gt;"));
    }

    #[test]
    fn grammars_are_found_by_name_extension_and_first_line() {
        let set = syntaxes();
        let name = |file: &str, first: &str| syntax_for(set, file, first).map(|s| s.name.clone());
        assert_eq!(name("Makefile", "all:").as_deref(), Some("Makefile"));
        assert_eq!(name("Cargo.toml", "[package]").as_deref(), Some("TOML"));
        assert_eq!(name("app.ts", "").as_deref(), Some("TypeScript"));
        assert_eq!(
            name("build", "#!/usr/bin/env bash").as_deref(),
            Some("Bourne Again Shell (bash)")
        );
        assert_eq!(name("notes.txt", "hello"), None);
        assert_eq!(name("README", "hello"), None);
    }

    #[test]
    fn both_backgrounds_have_a_theme() {
        let code = "x = 1  # one\n";
        let dark = markup("a.py", code, true).expect("Python is known");
        let light = markup("a.py", code, false).expect("Python is known");
        assert_ne!(dark, light);
    }

    #[test]
    fn the_gutter_has_one_right_aligned_number_per_line() {
        assert_eq!(gutter(3), "1\n2\n3");
        assert_eq!(gutter(10).lines().next(), Some(" 1"));
        assert_eq!(gutter(10).lines().last(), Some("10"));
        assert_eq!(gutter(0), "");
    }
}
