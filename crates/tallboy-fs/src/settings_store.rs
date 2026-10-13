//! Layered GKeyFile settings with atomic saves and corrupt-file recovery.
use glib::{KeyFile, KeyFileFlags};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
use tallboy_core::config::Config;
/// Configuration loading and persistence failures.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// File access failed.
    #[error("Settings file: {0}")]
    Io(#[from] io::Error),
    /// GKeyFile operation failed.
    #[error("Settings format: {0}")]
    KeyFile(#[from] glib::Error),
    /// Profile could escape its directory.
    #[error("Invalid profile name")]
    Profile,
}
/// Parsed settings and retained unknown groups/keys.
pub struct Store {
    /// Typed v1 settings.
    pub config: Config,
    /// Parse/recovery warnings for the application to display or log.
    pub warnings: Vec<String>,
    key_file: KeyFile,
    path: PathBuf,
}
fn profile_valid(p: &str) -> bool {
    !p.is_empty()
        && p.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}
/// Read a config and preserve its comments and unknown entries. Missing files are empty.
pub fn read_key_file(path: &Path, warnings: &mut Vec<String>) -> Result<KeyFile, SettingsError> {
    let key = KeyFile::new();
    let data = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(key),
        Err(e) => return Err(e.into()),
    };
    if let Err(e) = key.load_from_bytes(
        &glib::Bytes::from_owned(data),
        KeyFileFlags::KEEP_COMMENTS | KeyFileFlags::KEEP_TRANSLATIONS,
    ) {
        let mut broken = path.with_extension("conf.broken");
        let mut n = 1;
        while broken.exists() {
            broken = path.with_extension(format!("conf.broken.{n}"));
            n += 1;
        }
        fs::rename(path, &broken)?;
        warnings.push(format!(
            "{} is corrupt ({e}); moved to {}",
            path.display(),
            broken.display()
        ));
        return Ok(KeyFile::new());
    }
    Ok(key)
}
impl Store {
    /// Load low-priority system directories first, then user settings. `system_dirs`
    /// uses XDG_CONFIG_DIRS order (highest priority first); no process environment is mutated.
    pub fn load(
        user_dir: &Path,
        system_dirs: &[PathBuf],
        profile: &str,
    ) -> Result<Self, SettingsError> {
        if !profile_valid(profile) {
            return Err(SettingsError::Profile);
        }
        let path = user_dir.join("tallboy").join(profile).join("tallboy.conf");
        let key_file = KeyFile::new();
        let mut warnings = Vec::new();
        let paths = system_dirs
            .iter()
            .rev()
            .map(|p| p.join("tallboy").join(profile).join("tallboy.conf"))
            .chain(std::iter::once(path.clone()));
        for source in paths {
            let key = read_key_file(&source, &mut warnings)?;
            for group in key.groups().iter() {
                for name in key.keys(group)?.iter() {
                    key_file.set_string(group, name, &key.string(group, name)?);
                }
            }
        }
        let mut config = Config::default();
        for group in key_file.groups().iter() {
            for name in key_file.keys(group)?.iter() {
                config.apply(group, name, &key_file.string(group, name)?, &mut warnings);
            }
        }
        for warning in &warnings {
            log::warn!("{warning}");
        }
        Ok(Self {
            config,
            warnings,
            key_file,
            path,
        })
    }
    /// Load the explicitly selected profile using GLib XDG directories.
    pub fn from_environment(profile: &str) -> Result<Self, SettingsError> {
        Self::load(
            &glib::user_config_dir(),
            &glib::system_config_dirs(),
            profile,
        )
    }
    /// Atomically persist all known settings while retaining unknown entries.
    pub fn save(&self) -> Result<(), SettingsError> {
        for (group, key, value) in self.config.entries() {
            self.key_file.set_string(&group, &key, &value);
        }
        crate::atomic::write(&self.path, self.key_file.to_data().as_bytes())?;
        Ok(())
    }
    /// Value of a preserved unknown key, if present.
    pub fn unknown(&self, group: &str, key: &str) -> Option<String> {
        self.key_file.string(group, key).ok().map(|v| v.to_string())
    }
    /// Profile-specific settings file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
