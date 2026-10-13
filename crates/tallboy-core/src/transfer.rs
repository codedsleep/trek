//! Clipboard interchange and drag action resolution.
use crate::{
    config::DropAction,
    location::{Location, LocationError},
};
use std::path::Path;
/// Transfer intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// Copy files.
    Copy,
    /// Move files.
    Move,
    /// Link files.
    Link,
    /// Ask the user.
    Ask,
}
/// Validated clipboard file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Payload {
    /// Copy or cut.
    pub intent: Intent,
    /// Escaped URIs, never raw newline-containing paths.
    pub files: Vec<Location>,
}
/// Invalid clipboard content.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    /// Unsupported header or invalid text.
    #[error("Invalid clipboard payload")]
    Invalid,
    /// Location rejected by policy.
    #[error(transparent)]
    Location(#[from] LocationError),
}
impl Payload {
    /// Encode GNOME cut/copy format. URI escaping keeps newline names intact.
    pub fn gnome(&self) -> String {
        format!(
            "{}\n{}",
            if self.intent == Intent::Move {
                "cut"
            } else {
                "copy"
            },
            self.files
                .iter()
                .map(Location::to_uri)
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
    /// Decode GNOME format, validating every URI.
    pub fn from_gnome(text: &str) -> Result<Self, TransferError> {
        let (header, body) = text.split_once('\n').ok_or(TransferError::Invalid)?;
        let intent = match header.trim_end_matches('\r') {
            "copy" => Intent::Copy,
            "cut" => Intent::Move,
            _ => return Err(TransferError::Invalid),
        };
        let mut payload = Self::from_uri_list(body)?;
        payload.intent = intent;
        Ok(payload)
    }
    /// Encode RFC URI list using CRLF.
    pub fn uri_list(&self) -> String {
        let mut text = self
            .files
            .iter()
            .map(Location::to_uri)
            .collect::<Vec<_>>()
            .join("\r\n");
        if !text.is_empty() {
            text.push_str("\r\n");
        }
        text
    }
    /// Parse URI list, ignoring comment and empty lines.
    pub fn from_uri_list(text: &str) -> Result<Self, TransferError> {
        let mut files = Vec::new();
        for line in text
            .lines()
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
        {
            if !line.contains("://") {
                return Err(TransferError::Invalid);
            }
            files.push(Location::parse(line, Path::new("/"), Path::new("/"))?);
        }
        Ok(Self {
            intent: Intent::Copy,
            files,
        })
    }
    /// KDE marker is a single ASCII digit.
    pub fn kde_cut(&self) -> &'static str {
        if self.intent == Intent::Move {
            "1"
        } else {
            "0"
        }
    }
    /// Apply KDE intent; all other markers are rejected.
    pub fn with_kde_cut(mut self, value: &str) -> Result<Self, TransferError> {
        self.intent = match value.trim() {
            "0" => Intent::Copy,
            "1" => Intent::Move,
            _ => return Err(TransferError::Invalid),
        };
        Ok(self)
    }
    /// Plain text is presentation only; multiline native names use a URI to stay one entry.
    pub fn plain(&self) -> String {
        self.files
            .iter()
            .map(|l| {
                let display = l.display();
                if display.contains(['\n', '\r']) {
                    l.to_uri().to_owned()
                } else {
                    display
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// Decode newline-separated plain paths against the caller's directory.
    pub fn from_plain(text: &str, cwd: &Path, home: &Path) -> Result<Self, TransferError> {
        let files = text
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| Location::parse(l, cwd, home))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            intent: Intent::Copy,
            files,
        })
    }
}
/// Modifier overrides have precedence over the configured drop default.
pub fn resolve(
    ctrl: bool,
    shift: bool,
    default: DropAction,
    source_fs: Option<&str>,
    dest_fs: Option<&str>,
) -> Intent {
    match (ctrl, shift) {
        (true, true) => Intent::Link,
        (true, false) => Intent::Copy,
        (false, true) => Intent::Move,
        (false, false) => match default {
            DropAction::Auto => {
                if source_fs.is_some() && source_fs == dest_fs {
                    Intent::Move
                } else {
                    Intent::Copy
                }
            }
            DropAction::Copy => Intent::Copy,
            DropAction::Move => Intent::Move,
            DropAction::Ask => Intent::Ask,
        },
    }
}
/// Drop validation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropCheck {
    /// Valid transfer.
    Proceed,
    /// Move within the source folder does nothing.
    Noop,
    /// Directory cannot be placed inside itself.
    IntoSelf,
}
/// Lexical native-path or URI boundary check; fs performs resolved checks before writes.
pub fn check_drop(
    source: &Location,
    dest_dir: &Location,
    directory: bool,
    intent: Intent,
) -> DropCheck {
    let src = source.to_uri().trim_end_matches('/');
    let dest = dest_dir.to_uri().trim_end_matches('/');
    if directory
        && (src == dest
            || dest
                .strip_prefix(src)
                .is_some_and(|suffix| suffix.starts_with('/')))
    {
        return DropCheck::IntoSelf;
    }
    if intent == Intent::Move
        && src
            .rsplit_once('/')
            .is_some_and(|(parent, _)| parent == dest)
    {
        DropCheck::Noop
    } else {
        DropCheck::Proceed
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newline_roundtrip() {
        let p = Payload {
            intent: Intent::Move,
            files: vec![Location::from_path(Path::new("/tmp/line\nbreak")).unwrap()],
        };
        assert_eq!(Payload::from_gnome(&p.gnome()).unwrap(), p);
        assert_eq!(
            Payload::from_uri_list(&p.uri_list())
                .unwrap()
                .with_kde_cut(p.kde_cut())
                .unwrap(),
            p
        );
        assert_eq!(
            Payload::from_plain(&p.plain(), Path::new("/"), Path::new("/"))
                .unwrap()
                .files,
            p.files
        );
        assert_eq!(p.gnome().lines().count(), 2);
    }
    #[test]
    fn actions() {
        assert_eq!(
            resolve(false, false, DropAction::Auto, None, None),
            Intent::Copy
        );
        assert_eq!(
            resolve(false, false, DropAction::Auto, Some("a"), Some("a")),
            Intent::Move
        );
        assert_eq!(
            resolve(true, true, DropAction::Ask, None, None),
            Intent::Link
        );
        let src = Location::Uri("file:///tmp/a".into());
        assert_eq!(
            check_drop(
                &src,
                &Location::Uri("file:///tmp".into()),
                true,
                Intent::Move
            ),
            DropCheck::Noop
        );
        assert_eq!(
            check_drop(
                &src,
                &Location::Uri("file:///tmp/a/b".into()),
                true,
                Intent::Copy
            ),
            DropCheck::IntoSelf
        );
        assert_eq!(
            check_drop(
                &src,
                &Location::Uri("file:///tmp/ab".into()),
                true,
                Intent::Copy
            ),
            DropCheck::Proceed
        );
    }
}
