//! Configuration value codecs.
use std::{fmt, str::FromStr};
/// An invalid configuration value.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid setting value: {0}")]
pub struct ValueError(pub String);
macro_rules! codec {
 ($name:ident {$($variant:ident => $value:literal),+}) => {
  #[doc = concat!("Typed ", stringify!($name), " setting.")]
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum $name {$(#[doc = $value] $variant),+}
  impl fmt::Display for $name {fn fmt(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str(match self {$(Self::$variant => $value),+})}}
  impl FromStr for $name {type Err=ValueError;fn from_str(s:&str)->Result<Self,Self::Err>{match s {$($value=>Ok(Self::$variant)),+, _=>Err(ValueError(s.into()))}}}
 };
}
codec!(BookmarkOpen {Current=>"0", Tab=>"1", Window=>"2"});
codec!(DropAction {Auto=>"auto", Copy=>"copy", Move=>"move", Ask=>"ask"});
codec!(ViewMode {Icon=>"icon", Compact=>"compact", Thumbnail=>"thumbnail", List=>"list"});
codec!(PaneMode {Places=>"places", Tree=>"dirtree"});
/// Side pane visibility and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidePane {
    /// Whether it is hidden.
    pub hidden: bool,
    /// Content mode.
    pub mode: PaneMode,
}
impl fmt::Display for SidePane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}",
            if self.hidden { "hidden;" } else { "" },
            self.mode
        )
    }
}
impl FromStr for SidePane {
    type Err = ValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hidden, mode) = s.strip_prefix("hidden;").map_or((false, s), |s| (true, s));
        Ok(Self {
            hidden,
            mode: mode.parse()?,
        })
    }
}
/// Ordered toolbar groups, with optional hidden marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toolbar(pub Vec<String>);
impl fmt::Display for Toolbar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            Ok(())
        } else {
            write!(f, "{};", self.0.join(";"))
        }
    }
}
impl FromStr for Toolbar {
    type Err = ValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let values: Vec<_> = s
            .trim_end_matches(';')
            .split(';')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if values.iter().any(|s| {
            !matches!(
                s.as_str(),
                "hidden" | "newwin" | "newtab" | "navigation" | "home"
            )
        }) {
            return Err(ValueError(s.into()));
        }
        Ok(Self(values))
    }
}
/// Visible list columns and optional positive widths.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Columns(pub Vec<(String, Option<u32>)>);
impl fmt::Display for Columns {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, width) in &self.0 {
            write!(f, "{name}")?;
            if let Some(w) = width {
                write!(f, ":{w}")?;
            }
            write!(f, ";")?;
        }
        Ok(())
    }
}
impl FromStr for Columns {
    type Err = ValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut out = Vec::new();
        for item in s.split(';').filter(|s| !s.is_empty()) {
            let (name, width) = item
                .split_once(':')
                .map_or((item, None), |(n, w)| (n, Some(w)));
            if !matches!(
                name,
                "name" | "desc" | "size" | "mtime" | "perm" | "owner" | "group" | "ext"
            ) {
                return Err(ValueError(s.into()));
            }
            let width = width
                .map(|w| {
                    w.parse::<u32>()
                        .ok()
                        .filter(|w| *w > 0)
                        .ok_or_else(|| ValueError(s.into()))
                })
                .transpose()?;
            if out.iter().any(|(n, _)| n == name) {
                return Err(ValueError(s.into()));
            }
            out.push((name.into(), width));
        }
        Ok(Self(out))
    }
}
/// Autorun choice; software media cannot bypass confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutorunChoice {
    /// Application desktop id.
    pub app: String,
    /// Remember without asking.
    pub remembered: bool,
}
impl AutorunChoice {
    /// Decode an upstream choice, clearing software-media trust.
    pub fn parse(content_type: &str, value: &str) -> Self {
        Self {
            app: value.trim_start_matches('*').into(),
            remembered: value.starts_with('*') && content_type != "x-content/unix-software",
        }
    }
    /// Encode a choice, enforcing the software-media rule.
    pub fn encode(&self, content_type: &str) -> String {
        format!(
            "{}{}",
            if self.remembered && content_type != "x-content/unix-software" {
                "*"
            } else {
                ""
            },
            self.app
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codecs() {
        for s in ["0", "1", "2"] {
            assert_eq!(s.parse::<BookmarkOpen>().unwrap().to_string(), s);
        }
        for s in ["auto", "copy", "move", "ask"] {
            assert_eq!(s.parse::<DropAction>().unwrap().to_string(), s);
        }
        for s in ["icon", "compact", "thumbnail", "list"] {
            assert_eq!(s.parse::<ViewMode>().unwrap().to_string(), s);
        }
        for s in ["places", "dirtree", "hidden;places", "hidden;dirtree"] {
            assert_eq!(s.parse::<SidePane>().unwrap().to_string(), s);
        }
        for s in ["", "hidden;newwin;newtab;navigation;home;"] {
            assert_eq!(s.parse::<Toolbar>().unwrap().to_string(), s);
        }
        for s in ["", "name:240;desc;size;mtime;perm;owner;group;"] {
            assert_eq!(s.parse::<Columns>().unwrap().to_string(), s);
        }
        assert!("name:0;".parse::<Columns>().is_err());
        assert!("???".parse::<Toolbar>().is_err());
    }
    #[test]
    fn autorun() {
        let v = AutorunChoice::parse("x-content/image-dcf", "*a.desktop");
        assert!(v.remembered);
        assert_eq!(v.encode("x-content/image-dcf"), "*a.desktop");
        assert!(!AutorunChoice::parse("x-content/unix-software", "*a.desktop").remembered);
        assert_eq!(v.encode("x-content/unix-software"), "a.desktop");
    }
}
