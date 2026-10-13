//! Development write guard. Every mutating user-file entry point checks this policy.
use gio::prelude::*;
use std::path::{Path, PathBuf};
use tallboy_core::location::{Location, LocationError, normalize};
/// A mutation is outside the allowed fixture/test roots.
#[derive(Debug, thiserror::Error)]
pub enum GuardError {
    /// Rejected location.
    #[error("File operations are locked outside the allowed roots: {0}")]
    Denied(String),
    /// Malformed or prohibited URI.
    #[error(transparent)]
    Location(#[from] LocationError),
    /// Cannot resolve the parent safely.
    #[error("Cannot verify operation path: {0}")]
    Io(#[from] std::io::Error),
}
/// Explicit guard inputs, injectable without mutating the process environment.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Native roots; descendants are allowed.
    pub roots: Vec<PathBuf>,
    /// Allowed non-native URI prefixes, matched at path boundaries.
    pub uri_roots: Vec<String>,
    /// User data directory; trash requires this to be under a root.
    pub data_home: PathBuf,
    /// Owner-controlled release override. Agents never enable it.
    pub unlocked: bool,
}
/// Resolve existing symlink ancestors, retaining the uncreated tail.
/// Canonicalization is only a security check; all user-file mutations use GIO.
pub(crate) fn resolve(path: &Path) -> Result<PathBuf, std::io::Error> {
    let mut head = path.to_path_buf();
    let mut tail = Vec::new();
    loop {
        match std::fs::canonicalize(&head) {
            Ok(mut base) => {
                for name in tail.iter().rev() {
                    base.push(name);
                }
                return Ok(base);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let name = head.file_name().ok_or(e)?.to_os_string();
                tail.push(name);
                if !head.pop() {
                    return Err(std::io::Error::other("No existing ancestor"));
                }
            }
            Err(e) => return Err(e),
        }
    }
}
impl Policy {
    /// Build the locked policy from the fixture, build test root and temp directory.
    /// TALLBOY_ALLOW_ROOTS is a semicolon/newline-separated list of native paths or URIs.
    pub fn from_environment() -> Self {
        let mut roots = vec![
            glib::user_cache_dir().join("tallboy-fixture"),
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/tmp"),
            std::env::temp_dir(),
        ];
        if let Some(root) = std::env::var_os("CARGO_TARGET_TMPDIR") {
            roots.push(root.into());
        }
        let mut uri_roots = vec![];
        if let Ok(value) = std::env::var("TALLBOY_ALLOW_ROOTS") {
            for root in value.split([';', '\n']).filter(|s| !s.is_empty()) {
                if root.contains("://") {
                    uri_roots.push(root.into());
                } else {
                    roots.push(PathBuf::from(root));
                }
            }
        }
        Self {
            roots,
            uri_roots,
            data_home: glib::user_data_dir(),
            unlocked: std::env::var_os("TALLBOY_UNLOCKED").as_deref()
                == Some(std::ffi::OsStr::new("1")),
        }
    }
    fn native_allowed(&self, path: &Path) -> Result<bool, GuardError> {
        let path = resolve(path)?;
        for root in &self.roots {
            let root = resolve(&normalize(root))?;
            if path.starts_with(root) {
                return Ok(true);
            }
        }
        Ok(false)
    }
    /// Check an entry without following its final symlink. Resolves parent symlinks,
    /// including a symlink used as the destination directory. admin is always blocked.
    pub fn check(&self, file: &gio::File) -> Result<(), GuardError> {
        let uri = file.uri();
        if file.path().is_none() {
            let decoded = tallboy_core::location::unescape(&uri)?;
            if decoded
                .split(|b| *b == b'/')
                .any(|s| s == b".." || s == b".")
            {
                return Err(GuardError::Denied(uri.into()));
            }
        }
        let location = Location::parse(&uri, Path::new("/"), Path::new("/"))?;
        if self.unlocked {
            return Ok(());
        }
        let allowed = if let Some(path) = location.path() {
            let parent = path.parent().unwrap_or(&path);
            {
                let mut entry = resolve(parent)?;
                if let Some(name) = path.file_name() {
                    entry.push(name);
                }
                self.roots
                    .iter()
                    .any(|root| resolve(&normalize(root)).is_ok_and(|r| entry.starts_with(r)))
            }
        } else if uri.starts_with("trash://") {
            self.native_allowed(&self.data_home)?
        } else {
            self.uri_roots.iter().any(|root| {
                let root = gio::File::for_uri(root);
                file.equal(&root) || file.has_prefix(&root)
            })
        };
        if allowed {
            Ok(())
        } else {
            Err(GuardError::Denied(uri.into()))
        }
    }
}
/// Shared entry point used by all mutations; custom test policies stay locked too.
pub fn check(file: &gio::File, policy: &Policy) -> Result<(), GuardError> {
    policy.check(file)
}
