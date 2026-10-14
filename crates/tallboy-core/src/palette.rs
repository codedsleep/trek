//! Recolours GTK 4's built-in stylesheet with another theme's named colours.
//!
//! GTK 4 loads only a theme's `gtk-4.0` stylesheet, so a theme written for
//! GTK 3 leaves it with the built-in one. Both name the same palette
//! (`@define-color theme_bg_color …`), and the built-in stylesheet is that
//! palette with every other colour derived from it and written out as a
//! literal. Each literal is carried over to the other palette: a named
//! colour becomes the theme's, a grey keeps its place between the theme's
//! background and text colours, and a shade of the selection, error, success
//! or warning colour becomes the same shade of the theme's.
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Range;

/// Below this spread between channels a colour counts as a grey.
const GREY_CHROMA: f32 = 12.0;
/// How far, in degrees of hue, a shade may lie from the colour it derives from.
const HUE_TOLERANCE: f32 = 20.0;

/// Red, green and blue in 0–255, alpha in 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Colour {
    rgb: [f32; 3],
    alpha: f32,
}

impl Colour {
    const BLACK: Self = Self::opaque([0.0; 3]);
    const WHITE: Self = Self::opaque([255.0; 3]);

    const fn opaque(rgb: [f32; 3]) -> Self {
        Self { rgb, alpha: 1.0 }
    }

    /// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb(r, g, b)`,
    /// `rgba(r, g, b, a)`, `white` or `black`.
    fn literal(text: &str) -> Option<Self> {
        let text = text.trim();
        match text {
            "white" => return Some(Self::WHITE),
            "black" => return Some(Self::BLACK),
            _ => {}
        }
        if let Some(hex) = text.strip_prefix('#') {
            let digits: Vec<f32> = hex
                .chars()
                .map(|c| c.to_digit(16).map(|d| d as f32))
                .collect::<Option<_>>()?;
            let channels: Vec<f32> = match digits.len() {
                3 | 4 => digits.iter().map(|d| d * 17.0).collect(),
                6 | 8 => digits
                    .chunks(2)
                    .map(|pair| pair[0] * 16.0 + pair[1])
                    .collect(),
                _ => return None,
            };
            return Some(Self {
                rgb: [channels[0], channels[1], channels[2]],
                alpha: channels.get(3).map_or(1.0, |a| a / 255.0),
            });
        }
        let arguments = text
            .strip_prefix("rgba(")
            .or_else(|| text.strip_prefix("rgb("))?
            .strip_suffix(')')?;
        let numbers: Vec<f32> = arguments
            .split(',')
            .map(|n| n.trim().parse().ok())
            .collect::<Option<_>>()?;
        match numbers[..] {
            [r, g, b] => Some(Self::opaque([r, g, b])),
            [r, g, b, alpha] => Some(Self {
                rgb: [r, g, b],
                alpha,
            }),
            _ => None,
        }
    }

    fn key(self) -> [u8; 3] {
        self.rgb.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }

    fn min(self) -> f32 {
        self.rgb.into_iter().fold(f32::MAX, f32::min)
    }

    fn chroma(self) -> f32 {
        self.rgb.into_iter().fold(f32::MIN, f32::max) - self.min()
    }

    fn level(self) -> f32 {
        self.rgb.iter().sum::<f32>() / 3.0
    }

    fn hue(self) -> f32 {
        let [r, g, b] = self.rgb;
        let chroma = self.chroma();
        if chroma == 0.0 {
            return 0.0;
        }
        let max = chroma + self.min();
        let sixth = if max == r {
            ((g - b) / chroma).rem_euclid(6.0)
        } else if max == g {
            (b - r) / chroma + 2.0
        } else {
            (r - g) / chroma + 4.0
        };
        sixth * 60.0
    }

    /// The colour seen when this one lies on `under`.
    fn over(self, under: Self) -> Self {
        let mut rgb = [0.0; 3];
        for (i, channel) in rgb.iter_mut().enumerate() {
            *channel = self.rgb[i] * self.alpha + under.rgb[i] * (1.0 - self.alpha);
        }
        Self::opaque(rgb)
    }

    fn mix(self, other: Self, share: f32) -> Self {
        let mut rgb = [0.0; 3];
        for (i, channel) in rgb.iter_mut().enumerate() {
            *channel = self.rgb[i] + (other.rgb[i] - self.rgb[i]) * share;
        }
        Self::opaque(rgb)
    }

    fn css(self) -> String {
        let [r, g, b] = self.key();
        if self.alpha >= 1.0 {
            format!("rgb({r}, {g}, {b})")
        } else {
            let alpha = (self.alpha.max(0.0) * 1000.0).round() / 1000.0;
            format!("rgba({r}, {g}, {b}, {alpha})")
        }
    }
}

/// The colours a stylesheet names with `@define-color`.
#[derive(Debug, Default)]
pub struct NamedColours(Vec<(String, String)>);

impl NamedColours {
    /// Reads every `@define-color` statement of a stylesheet. A name defined
    /// twice has its last value, as in GTK.
    pub fn parse(css: &str) -> Self {
        let mut colours = vec![];
        for statement in css.split("@define-color").skip(1) {
            let statement = statement.split(';').next().unwrap_or_default().trim();
            if let Some((name, value)) = statement.split_once(char::is_whitespace) {
                colours.push((name.to_owned(), value.trim().to_owned()));
            }
        }
        Self(colours)
    }

    /// Whether the stylesheet names no colours at all.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the palette is a dark one; None when it has no background
    /// colour that can be read.
    pub fn is_dark(&self) -> Option<bool> {
        let [r, g, b] = self.get("theme_bg_color")?.rgb;
        Some(0.2126 * r + 0.7152 * g + 0.0722 * b < 127.5)
    }

    /// A named colour, when its value is a literal or another name. The
    /// colour functions of GTK 3 (`shade`, `mix`, `alpha`) are not evaluated.
    fn get(&self, name: &str) -> Option<Colour> {
        let mut name = name;
        for _ in 0..8 {
            let value = self
                .0
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.as_str())?;
            match value.strip_prefix('@') {
                Some(next) => name = next,
                None => return Colour::literal(value),
            }
        }
        None
    }
}

struct Mapping {
    exact: HashMap<[u8; 3], Colour>,
    /// Grey level of the built-in palette to the theme's colour, ascending.
    greys: Vec<(f32, Colour)>,
    /// A colour of the built-in palette and the theme's.
    families: Vec<(Colour, Colour)>,
    accent: Colour,
    selected_fg: Colour,
}

impl Mapping {
    fn new(builtin: &NamedColours, theme: &NamedColours) -> Option<Self> {
        let background = theme.get("theme_bg_color")?.over(Colour::BLACK);
        let solid = |name: &str| theme.get(name).map(|c| c.over(background));
        let foreground = solid("theme_fg_color")?;
        let text = solid("theme_text_color").unwrap_or(foreground);
        let accent = builtin.get("theme_selected_bg_color")?;

        let mut greys = vec![
            (builtin.get("theme_text_color")?.level(), text),
            (builtin.get("theme_fg_color")?.level(), foreground),
            (builtin.get("theme_bg_color")?.level(), background),
            (
                builtin.get("theme_base_color")?.level(),
                solid("theme_base_color").unwrap_or(background),
            ),
        ];
        // Past the palette's own ends there is only shadow and highlight.
        greys.push((0.0, Colour::BLACK));
        greys.push((255.0, Colour::WHITE));
        greys.sort_by(|a, b| a.0.total_cmp(&b.0));
        greys.dedup_by(|later, earlier| later.0 == earlier.0);

        let families = [
            "theme_selected_bg_color",
            "error_color",
            "success_color",
            "warning_color",
        ]
        .into_iter()
        .filter_map(|name| Some((builtin.get(name)?, solid(name)?)))
        .filter(|(from, _)| from.chroma() >= GREY_CHROMA)
        .collect();

        // White and black are left to the rules: they are too many things at
        // once (text, the text of a selection, highlights and shadows).
        let mut exact = HashMap::new();
        for (name, value) in &builtin.0 {
            let (Some(from), Some(to)) = (Colour::literal(value), theme.get(name)) else {
                continue;
            };
            if from.alpha == 1.0 && from != Colour::WHITE && from != Colour::BLACK {
                exact.entry(from.key()).or_insert(to);
            }
        }
        Some(Self {
            exact,
            greys,
            families,
            accent,
            selected_fg: theme
                .get("theme_selected_fg_color")
                .or_else(|| builtin.get("theme_selected_fg_color"))
                .unwrap_or(Colour::WHITE),
        })
    }

    fn apply(&self, colour: Colour) -> Colour {
        let alpha = colour.alpha;
        if let Some(to) = self.exact.get(&colour.key()) {
            return Colour {
                rgb: to.rgb,
                alpha: to.alpha * alpha,
            };
        }
        if colour.chroma() < GREY_CHROMA {
            return Colour {
                alpha,
                ..self.grey(colour.level())
            };
        }
        let Some((from, to)) = self.families.iter().find(|(from, _)| near(colour, *from)) else {
            return colour;
        };
        // The same hue is the original scaled, plus or minus some grey.
        let scale = colour.chroma() / from.chroma();
        let grey = colour.min() - scale * from.min();
        Colour {
            rgb: to.rgb.map(|c| (scale * c + grey).clamp(0.0, 255.0)),
            alpha,
        }
    }

    fn grey(&self, level: f32) -> Colour {
        let above = self
            .greys
            .iter()
            .position(|(l, _)| *l >= level)
            .unwrap_or(self.greys.len() - 1);
        let (high, to) = self.greys[above];
        let Some((low, from)) = above.checked_sub(1).map(|below| self.greys[below]) else {
            return to;
        };
        from.mix(to, (level - low) / (high - low))
    }

    fn is_accent(&self, colour: Colour) -> bool {
        colour.chroma() >= GREY_CHROMA && near(colour, self.accent)
    }
}

fn near(a: Colour, b: Colour) -> bool {
    let gap = (a.hue() - b.hue()).abs();
    gap.min(360.0 - gap) <= HUE_TOLERANCE
}

struct Token {
    range: Range<usize>,
    colour: Colour,
    /// Written as a word, `white` or `black`, rather than as a number.
    word: bool,
}

/// The colour literals in a declaration's value, skipping quoted strings.
fn tokens(value: &str) -> Vec<Token> {
    let bytes = value.as_bytes();
    let name = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    let starts_word = |i: usize| i == 0 || !name(bytes[i - 1]);
    let mut found = vec![];
    let mut i = 0;
    while i < bytes.len() {
        if !value.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &value[i..];
        let end = match bytes[i] {
            b'"' => {
                i += rest[1..].find('"').map_or(rest.len(), |close| close + 2);
                continue;
            }
            b'#' => Some(i + 1 + rest[1..].bytes().take_while(|b| name(*b)).count()),
            b'r' if starts_word(i) && (rest.starts_with("rgb(") || rest.starts_with("rgba(")) => {
                rest.find(')').map(|close| i + close + 1)
            }
            b'w' | b'b' if starts_word(i) => ["white", "black"]
                .into_iter()
                .find(|word| rest.starts_with(word))
                .map(|word| i + word.len())
                .filter(|end| bytes.get(*end).is_none_or(|b| !name(*b))),
            _ => None,
        };
        match end.and_then(|end| Some((end, Colour::literal(&value[i..end])?))) {
            Some((end, colour)) => {
                found.push(Token {
                    range: i..end,
                    colour,
                    word: matches!(bytes[i], b'w' | b'b'),
                });
                i = end;
            }
            None => i += 1,
        }
    }
    found
}

fn rewrite(value: &str, out: &mut String, mut colour: impl FnMut(&Token) -> Colour) {
    let mut done = 0;
    for token in tokens(value) {
        out.push_str(&value[done..token.range.start]);
        out.push_str(&colour(&token).css());
        done = token.range.end;
    }
    out.push_str(&value[done..]);
}

/// One rule's declarations.
fn block(body: &str, mapping: &Mapping, out: &mut String) {
    let declarations: Vec<(&str, Option<&str>)> = body
        .split(';')
        .map(|d| {
            d.split_once(':')
                .map_or((d, None), |(property, value)| (property, Some(value)))
        })
        .collect();
    // Text on the selection colour is the selection's text, whatever the
    // built-in theme happens to call it.
    let on_accent = declarations.iter().any(|(property, value)| {
        property.trim().starts_with("background")
            && tokens(value.unwrap_or_default())
                .iter()
                .any(|t| t.colour.alpha > 0.5 && mapping.is_accent(t.colour))
    });
    for (i, (property, value)) in declarations.into_iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        out.push_str(property);
        let Some(value) = value else { continue };
        out.push(':');
        let is_text = property.trim() == "color";
        rewrite(value, out, |token| {
            if is_text && token.colour == Colour::WHITE && (!token.word || on_accent) {
                mapping.selected_fg
            } else {
                mapping.apply(token.colour)
            }
        });
    }
}

/// What stands between two rules: selectors, at-rules and `@define-color`
/// statements. Only the last hold colours; `#name` in a selector is an id.
fn between(text: &str, mapping: &Mapping, out: &mut String) {
    let mut rest = text;
    while let Some(at) = rest.find("@define-color") {
        let end = rest[at..]
            .find(';')
            .map_or(rest.len(), |semicolon| at + semicolon);
        out.push_str(&rest[..at]);
        rewrite(&rest[at..end], out, |token| mapping.apply(token.colour));
        rest = &rest[end..];
    }
    out.push_str(rest);
}

/// GTK's built-in stylesheet in the colours of `theme`, or None when either
/// lacks a background, text or selection colour to go by.
///
/// The names `theme` defines are defined in the result too, so that a
/// stylesheet of the application's own which uses `@theme_bg_color` and the
/// like follows.
pub fn recolour(builtin: &str, theme: &NamedColours) -> Option<String> {
    let mapping = Mapping::new(&NamedColours::parse(builtin), theme)?;
    let mut out = String::with_capacity(builtin.len() + builtin.len() / 8);
    let mut rest = builtin;
    while let Some(open) = rest.find('{') {
        between(&rest[..open], &mapping, &mut out);
        out.push('{');
        rest = &rest[open + 1..];
        // A brace that opens before this one closes belongs to a rule nested
        // in an at-rule; what came before it was that at-rule's prelude.
        match rest.find(['{', '}']) {
            Some(close) if rest.as_bytes()[close] == b'}' => {
                block(&rest[..close], &mapping, &mut out);
                out.push('}');
                rest = &rest[close + 1..];
            }
            _ => {}
        }
    }
    between(rest, &mapping, &mut out);
    out.push('\n');
    let mut defined: Vec<&str> = vec![];
    for (name, _) in theme.0.iter().rev() {
        if defined.contains(&name.as_str()) {
            continue;
        }
        defined.push(name);
        if let Some(colour) = theme.get(name) {
            let _ = writeln!(out, "@define-color {name} {};", colour.css());
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTIN: &str = "\
@define-color theme_fg_color #eeeeec;
@define-color theme_text_color white;
@define-color theme_bg_color #353535;
@define-color theme_base_color #2d2d2d;
@define-color theme_selected_bg_color #15539e;
@define-color theme_selected_fg_color #ffffff;
@define-color borders #1b1b1b;
@define-color error_color #cc0000;
@define-color accent_color #3584E4;
";
    const NORD: &str = "\
@define-color theme_fg_color #D8DEE9;
@define-color theme_text_color @theme_fg_color;
@define-color theme_bg_color #2E3440;
@define-color theme_base_color #2E3440;
@define-color theme_selected_bg_color #81A1C1;
@define-color theme_selected_fg_color rgba(0, 0, 0, 0.87);
@define-color borders rgba(216, 222, 233, 0.2);
@define-color error_color #BF616A;
@define-color odd shade(@theme_bg_color, 1.2);
";

    fn nord(rules: &str) -> String {
        let css = format!("{BUILTIN}{rules}");
        let out = recolour(&css, &NamedColours::parse(NORD)).expect("both palettes are complete");
        let start = out
            .find(rules.split('{').next().expect("a selector"))
            .expect("the rule");
        out[start..].lines().next().expect("a line").to_owned()
    }

    #[test]
    fn literals_are_read_in_every_spelling() {
        let half = Colour {
            rgb: [255.0, 0.0, 17.0],
            alpha: 0.5,
        };
        assert_eq!(
            Colour::literal("#f01"),
            Some(Colour::opaque([255.0, 0.0, 17.0]))
        );
        assert_eq!(
            Colour::literal("#ff001180").map(|c| c.key()),
            Some([255, 0, 17])
        );
        assert_eq!(Colour::literal("rgba(255, 0, 17, 0.5)"), Some(half));
        assert_eq!(
            Colour::literal("rgb(255,0,17)"),
            Some(Colour::opaque([255.0, 0.0, 17.0]))
        );
        assert_eq!(Colour::literal("white"), Some(Colour::WHITE));
        for not_a_colour in [
            "#add-color-button",
            "#12",
            "rgb(from black r g b)",
            "silver",
            "",
        ] {
            assert_eq!(Colour::literal(not_a_colour), None, "{not_a_colour}");
        }
    }

    #[test]
    fn names_follow_references_and_the_last_definition_wins() {
        let named = NamedColours::parse(
            "@define-color a #000;\n@define-color b @a;\n@define-color a #fff;",
        );
        assert_eq!(named.get("b"), Some(Colour::WHITE));
        assert_eq!(NamedColours::parse("@define-color a @a;").get("a"), None);
        assert_eq!(NamedColours::parse(NORD).get("odd"), None);
        assert_eq!(NamedColours::parse(NORD).is_dark(), Some(true));
        assert_eq!(
            NamedColours::parse("@define-color theme_bg_color #f6f5f4;").is_dark(),
            Some(false)
        );
        assert!(NamedColours::parse("button { color: red; }").is_empty());
    }

    #[test]
    fn a_named_colour_becomes_the_themes() {
        assert_eq!(
            nord(".background { color: #eeeeec; background-color: #353535; }"),
            ".background { color: rgb(216, 222, 233); background-color: rgb(46, 52, 64); }"
        );
        // Translucent in the theme, and translucent again in the rule.
        assert_eq!(
            nord("entry { border: 1px solid #1b1b1b; outline-color: rgba(27, 27, 27, 0.5); }"),
            "entry { border: 1px solid rgba(216, 222, 233, 0.2); \
             outline-color: rgba(216, 222, 233, 0.1); }"
        );
    }

    #[test]
    fn a_grey_keeps_its_place_between_background_and_text() {
        // Halfway from #353535 to #eeeeec is halfway from #2E3440 to #D8DEE9.
        assert_eq!(
            nord("label { color: #929291; }"),
            "label { color: rgb(131, 137, 149); }"
        );
        // Darker than the base colour: towards black, as it was.
        assert_eq!(
            nord("trough { background: #161616; }"),
            "trough { background: rgb(22, 25, 31); }"
        );
        assert_eq!(
            nord("shadow { color: rgba(0, 0, 0, 0.7); }"),
            "shadow { color: rgba(0, 0, 0, 0.7); }"
        );
    }

    #[test]
    fn a_shade_of_the_selection_becomes_that_shade_of_the_themes() {
        assert_eq!(
            nord("row:selected { background-color: rgba(21, 83, 158, 0.7); }"),
            "row:selected { background-color: rgba(129, 161, 193, 0.7); }"
        );
        // #0f3b71 is the selection colour darkened to 0.715 of itself.
        assert_eq!(
            nord("rubberband { border-color: #0f3b71; }"),
            "rubberband { border-color: rgb(92, 115, 138); }"
        );
        assert_eq!(
            nord("button.destructive { background: #660000; }"),
            "button.destructive { background: rgb(96, 49, 53); }"
        );
        // No colour of the palette has this hue.
        assert_eq!(
            nord("expander { color: #a0a; }"),
            "expander { color: rgb(170, 0, 170); }"
        );
    }

    #[test]
    fn text_on_the_selection_is_the_selections_text() {
        assert_eq!(
            nord("row:selected label { color: #ffffff; }"),
            "row:selected label { color: rgba(0, 0, 0, 0.87); }"
        );
        assert_eq!(
            nord("check:checked { color: white; background-image: image(#15539e); }"),
            "check:checked { color: rgba(0, 0, 0, 0.87); \
             background-image: image(rgb(129, 161, 193)); }"
        );
        assert_eq!(
            nord(".view { color: white; caret-color: white; }"),
            ".view { color: rgb(216, 222, 233); caret-color: rgb(216, 222, 233); }"
        );
        // A focus ring is not a background.
        assert_eq!(
            nord("tooltip { color: white; outline-color: #15539e; }"),
            "tooltip { color: rgb(216, 222, 233); outline-color: rgb(129, 161, 193); }"
        );
    }

    #[test]
    fn only_colours_in_declarations_change() {
        assert_eq!(
            nord("colorswatch#add-color-button > overlay { color: #eeeeec; }"),
            "colorswatch#add-color-button > overlay { color: rgb(216, 222, 233); }"
        );
        assert_eq!(
            nord("check { -gtk-icon-source: url(\"assets/#353535.svg\"); white-space: nowrap; }"),
            "check { -gtk-icon-source: url(\"assets/#353535.svg\"); white-space: nowrap; }"
        );
        assert_eq!(
            nord("title { color: rgb(from #eeeeec r g b / calc(alpha * 0.4)); }"),
            "title { color: rgb(from rgb(216, 222, 233) r g b / calc(alpha * 0.4)); }"
        );
        assert_eq!(
            nord("@keyframes pulse { from { color: #353535; } to { color: #eeeeec; } }"),
            "@keyframes pulse { from { color: rgb(46, 52, 64); } \
             to { color: rgb(216, 222, 233); } }"
        );
        assert_eq!(
            nord("label { color: #eeeeec; content: \"é #353535\"; margin: 0 }"),
            "label { color: rgb(216, 222, 233); content: \"é #353535\"; margin: 0 }"
        );
        assert_eq!(
            nord("/* é */ label { color: #353535 }"),
            "/* é */ label { color: rgb(46, 52, 64) }"
        );
    }

    #[test]
    fn the_themes_names_are_defined_in_the_result() {
        let out =
            recolour(BUILTIN, &NamedColours::parse(NORD)).expect("both palettes are complete");
        // The built-in theme's own definitions are recoloured where they stand…
        assert!(
            out.contains("@define-color accent_color rgb(191, 232, 255);"),
            "{out}"
        );
        // …and the theme's follow them, so that they win.
        let own = out
            .rfind("@define-color theme_bg_color rgb(46, 52, 64);")
            .expect("the theme's");
        assert!(
            own > out
                .find("@define-color accent_color")
                .expect("the built-in's")
        );
        assert!(out.contains("@define-color theme_text_color rgb(216, 222, 233);"));
        assert!(out.contains("@define-color borders rgba(216, 222, 233, 0.2);"));
        assert!(
            !out.contains("odd"),
            "a value that cannot be read is left out"
        );
    }

    #[test]
    fn a_theme_without_a_palette_is_not_used() {
        let no_background = NamedColours::parse("@define-color theme_fg_color #fff;");
        assert_eq!(recolour(BUILTIN, &no_background), None);
        assert_eq!(
            recolour("button { color: red; }", &NamedColours::parse(NORD)),
            None
        );
    }

    #[test]
    fn a_light_palette_runs_the_other_way() {
        let builtin = "\
@define-color theme_fg_color #2e3436;
@define-color theme_text_color black;
@define-color theme_bg_color #f6f5f4;
@define-color theme_base_color #ffffff;
@define-color theme_selected_bg_color #3584e4;
.view { color: black; background-color: #ffffff; }
button { color: #2e3436; background-color: #f6f5f4; border-color: #929292; }
";
        let theme = NamedColours::parse(
            "@define-color theme_fg_color #5c616c;\n@define-color theme_text_color #3b3e45;\n\
             @define-color theme_bg_color #f5f6f7;\n@define-color theme_base_color #ffffff;\n\
             @define-color theme_selected_bg_color #5294e2;",
        );
        let out = recolour(builtin, &theme).expect("both palettes are complete");
        assert!(
            out.contains(".view { color: rgb(59, 62, 69); background-color: rgb(255, 255, 255); }"),
            "{out}"
        );
        let button = "button { color: rgb(92, 97, 108); background-color: rgb(245, 246, 247); \
                      border-color: rgb(167, 170, 176); }";
        assert!(out.contains(button), "{out}");
        assert_eq!(theme.is_dark(), Some(false));
    }
}
