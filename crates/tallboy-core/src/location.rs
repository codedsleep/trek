//! Locations and URI construction, independent of GIO.
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};
/// A location error suitable for a user-facing message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LocationError {
    /// Privileged access deliberately unavailable.
    #[error("The admin:// scheme is disabled.")]
    Admin,
    /// Credentials belong to the mount prompt, never to stored or logged URIs.
    #[error("Enter the password in the connection prompt, rather than in the location.")]
    Credentials,
    /// Deferred virtual scheme.
    #[error("{0} is not supported in this version.")]
    Deferred(String),
    /// Malformed URI.
    #[error("Invalid location: {0}")]
    Invalid(String),
}
/// A validated GIO URI. Deferred virtual locations are rejected at parsing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    /// Native or GVFS URI, escaped for transport.
    Uri(String),
}
/// One breadcrumb destination and display label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breadcrumb {
    /// Visible segment.
    pub label: String,
    /// Destination.
    pub location: Location,
}
/// Percent-encode bytes; `/` can be preserved for URI paths.
pub fn escape(bytes: &[u8], preserve_slash: bool) -> String {
    let mut out = String::new();
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || (preserve_slash && b == b'/') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
/// Decode percent escapes without assuming UTF-8.
pub fn unescape(s: &str) -> Result<Vec<u8>, LocationError> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(LocationError::Invalid(s.into()));
            }
            let h = (b[i + 1] as char).to_digit(16);
            let l = (b[i + 2] as char).to_digit(16);
            match (h, l) {
                (Some(h), Some(l)) => out.push((h * 16 + l) as u8),
                _ => return Err(LocationError::Invalid(s.into())),
            }
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}
/// Lexically normalize a path; no filesystem or symlink traversal occurs.
pub fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                result.push(c.as_os_str())
            }
        }
    }
    result
}
impl Location {
    /// Resolve a path, `~`, or URI against the caller's absolute cwd and home.
    pub fn parse(input: &str, cwd: &Path, home: &Path) -> Result<Self, LocationError> {
        if input.contains('\0') {
            return Err(LocationError::Invalid(input.into()));
        }
        if let Some((scheme, rest)) = input.split_once("://") {
            let scheme = scheme.to_ascii_lowercase();
            if scheme == "admin" {
                return Err(LocationError::Admin);
            }
            if matches!(scheme.as_str(), "search" | "menu") {
                return Err(LocationError::Deferred(scheme));
            }
            if scheme.is_empty()
                || !scheme.chars().enumerate().all(|(i, c)| {
                    c.is_ascii_alphabetic()
                        || (i > 0 && (c.is_ascii_digit() || matches!(c, '+' | '-' | '.')))
                })
            {
                return Err(LocationError::Invalid(input.into()));
            }
            if let Some((userinfo, _)) = rest.split('/').next().unwrap_or_default().rsplit_once('@')
                && (userinfo.contains(':')
                    || unescape(userinfo).is_ok_and(|bytes| bytes.contains(&b':')))
            {
                return Err(LocationError::Credentials);
            }
            let raw = unescape(rest)?;
            if raw.contains(&0) || input.chars().any(char::is_control) || input.contains(' ') {
                return Err(LocationError::Invalid(input.into()));
            }
            if scheme == "file" && !rest.starts_with('/') && !rest.starts_with("localhost/") {
                return Err(LocationError::Invalid(input.into()));
            }
            return Ok(Self::Uri(format!("{scheme}://{rest}")));
        }
        let path = if input == "~" {
            home.to_path_buf()
        } else if let Some(p) = input.strip_prefix("~/") {
            home.join(p)
        } else {
            let p = Path::new(input);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                cwd.join(p)
            }
        };
        Self::from_path(&path)
    }
    /// Convert an absolute native path, including non-UTF-8 names, into a URI.
    pub fn from_path(path: &Path) -> Result<Self, LocationError> {
        if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
            return Err(LocationError::Invalid(path.display().to_string()));
        }
        Ok(Self::Uri(format!(
            "file://{}",
            escape(normalize(path).as_os_str().as_bytes(), true)
        )))
    }
    /// URI for GIO or clipboard interchange.
    pub fn to_uri(&self) -> &str {
        match self {
            Self::Uri(uri) => uri,
        }
    }
    /// Native path, if this is a local file URI.
    pub fn path(&self) -> Option<PathBuf> {
        let uri = self.to_uri();
        let path = uri
            .strip_prefix("file:///")
            .map(|p| format!("/{p}"))
            .or_else(|| {
                uri.strip_prefix("file://localhost/")
                    .map(|p| format!("/{p}"))
            })?;
        unescape(&path)
            .ok()
            .map(|b| PathBuf::from(std::ffi::OsString::from_vec(b)))
    }
    /// Paths display natively; remote locations display as URIs.
    pub fn display(&self) -> String {
        self.path().map_or_else(
            || self.to_uri().into(),
            |p| p.to_string_lossy().into_owned(),
        )
    }
    /// Breadcrumbs from the URI root through every path component.
    pub fn breadcrumbs(&self) -> Vec<Breadcrumb> {
        let uri = self.to_uri();
        let Some((scheme, rest)) = uri.split_once("://") else {
            return vec![];
        };
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let root = format!("{scheme}://{host}/");
        let mut result = vec![Breadcrumb {
            label: if scheme == "file" {
                "/".into()
            } else {
                format!("{scheme}://{host}")
            },
            location: Self::Uri(root.clone()),
        }];
        let mut current = root;
        for part in path.split('/').filter(|s| !s.is_empty()) {
            current.push_str(part);
            result.push(Breadcrumb {
                label: unescape(part).map_or_else(
                    |_| part.into(),
                    |b| String::from_utf8_lossy(&b).into_owned(),
                ),
                location: Self::Uri(current.clone()),
            });
            current.push('/');
        }
        result
    }
}
/// Connect-to-Server inputs; passwords belong to MountOperation, never the URI.
#[derive(Debug, Clone)]
pub struct Server {
    /// Protocol such as sftp/smb/ftp/dav/davs.
    pub scheme: String,
    /// Host or IPv6 address.
    pub host: String,
    /// Optional port.
    pub port: Option<u16>,
    /// Optional username.
    pub user: String,
    /// Share and path.
    pub path: String,
}
impl Server {
    /// Build and validate a server URI.
    pub fn location(&self) -> Result<Location, LocationError> {
        if !matches!(
            self.scheme.as_str(),
            "sftp" | "smb" | "ftp" | "dav" | "davs"
        ) || self.host.is_empty()
            || self.host.contains(['/', '@', ' ', '\0'])
            || self.port == Some(0)
        {
            return Err(LocationError::Invalid(self.host.clone()));
        }
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let auth = if self.user.is_empty() {
            String::new()
        } else {
            format!("{}@", escape(self.user.as_bytes(), false))
        };
        let port = self.port.map_or(String::new(), |p| format!(":{p}"));
        Location::parse(
            &format!(
                "{}://{auth}{host}{port}/{}",
                self.scheme,
                escape(self.path.trim_start_matches('/').as_bytes(), true)
            ),
            Path::new("/"),
            Path::new("/"),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        for s in [
            "/tmp/line\nbreak",
            "~/two words",
            "../é",
            "sftp://host/folder",
            "trash:///",
        ] {
            let l = Location::parse(s, Path::new("/home/u"), Path::new("/home/u")).unwrap();
            assert_eq!(
                Location::parse(l.to_uri(), Path::new("/"), Path::new("/")),
                Ok(l)
            );
        }
        assert_eq!(
            Location::parse("admin:///etc", Path::new("/"), Path::new("/")),
            Err(LocationError::Admin)
        );
        for s in ["search:///", "menu://applications/"] {
            assert!(matches!(
                Location::parse(s, Path::new("/"), Path::new("/")),
                Err(LocationError::Deferred(_))
            ));
        }
    }
    #[test]
    fn raw_path() {
        let p = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
        assert_eq!(Location::from_path(&p).unwrap().path(), Some(p));
    }
    #[test]
    fn embedded_passwords_never_enter_stored_or_logged_locations() {
        for uri in [
            "sftp://user:secret@host/folder",
            "FTP://user:secret@host/folder",
            "davs://user%3Asecret@host/folder",
            "smb://user:secret@[::1]/share",
        ] {
            let error = Location::parse(uri, Path::new("/"), Path::new("/")).unwrap_err();
            assert_eq!(error, LocationError::Credentials);
            assert!(!error.to_string().contains("secret"));
        }
        assert!(
            Location::parse(
                "sftp://user@[::1]:22/folder",
                Path::new("/"),
                Path::new("/")
            )
            .is_ok()
        );
    }
    #[test]
    fn server_and_crumbs() {
        let l = Server {
            scheme: "sftp".into(),
            host: "::1".into(),
            port: Some(22),
            user: "a b".into(),
            path: "one/two words".into(),
        }
        .location()
        .unwrap();
        assert_eq!(l.to_uri(), "sftp://a%20b@[::1]:22/one/two%20words");
        assert_eq!(l.breadcrumbs().len(), 3);
    }
}
