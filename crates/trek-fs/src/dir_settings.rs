//! Per-folder view overrides in dir-settings.conf.
use crate::settings_store::{SettingsError, read_key_file};
use glib::KeyFile;
use std::path::{Path, PathBuf};
use trek_core::config::{Columns, Config, ViewMode};
use trek_core::sort::SortSpec;
/// Settings saved when Preserve This Folder's Settings is enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderSettings {
    /// View.
    pub view_mode: ViewMode,
    /// Sort comparator.
    pub sort: SortSpec,
    /// Hidden-file toggle.
    pub show_hidden: bool,
    /// Detailed-view columns.
    pub columns: Columns,
}
impl From<&Config> for FolderSettings {
    fn from(c: &Config) -> Self {
        Self {
            view_mode: c.view_mode,
            sort: c.sort.clone(),
            show_hidden: c.show_hidden,
            columns: c.columns.clone(),
        }
    }
}
/// Atomic per-folder settings store; group keys are path or URI strings.
pub struct DirectoryStore {
    key_file: KeyFile,
    path: PathBuf,
}
impl DirectoryStore {
    /// Load, recovering corrupt files by moving them aside.
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        let mut warnings = vec![];
        let key_file = read_key_file(path, &mut warnings)?;
        for w in warnings {
            log::warn!("{w}");
        }
        Ok(Self {
            key_file,
            path: path.into(),
        })
    }
    /// Read an override; malformed individual values use supplied defaults.
    pub fn get(
        &self,
        group: &str,
        defaults: &Config,
    ) -> Result<Option<FolderSettings>, SettingsError> {
        if !self.key_file.has_group(group) {
            return Ok(None);
        }
        let mut config = defaults.clone();
        let mut warnings = vec![];
        for key in self.key_file.keys(group)?.iter() {
            config.apply("ui", key, &self.key_file.string(group, key)?, &mut warnings);
        }
        for w in warnings {
            log::warn!("{w}");
        }
        Ok(Some(FolderSettings::from(&config)))
    }
    /// Write one group, preserving other folders and unknown keys.
    pub fn set(&self, group: &str, settings: &FolderSettings) -> Result<(), SettingsError> {
        for (key, value) in [
            ("view_mode", settings.view_mode.to_string()),
            ("sort", settings.sort.to_string()),
            (
                "show_hidden",
                if settings.show_hidden {
                    "1".into()
                } else {
                    "0".into()
                },
            ),
            ("columns", settings.columns.to_string()),
        ] {
            self.key_file.set_string(group, key, &value);
        }
        crate::atomic::write(&self.path, self.key_file.to_data().as_bytes())?;
        Ok(())
    }
    /// Remove a folder override.
    pub fn remove(&self, group: &str) -> Result<(), SettingsError> {
        if self.key_file.has_group(group) {
            self.key_file.remove_group(group)?;
            crate::atomic::write(&self.path, self.key_file.to_data().as_bytes())?;
        }
        Ok(())
    }
}
