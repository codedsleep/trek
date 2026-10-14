//! One-shot PCManFM/libfm importer; source files are never modified.
use std::path::Path;
use trek_core::config::Config;
/// One key accounted for in the importer report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    /// Source application.
    pub source: String,
    /// Original group.
    pub group: String,
    /// Original key.
    pub key: String,
    /// Value read from the source.
    pub value: String,
}
/// Import results; caller decides whether and where to save config.
#[derive(Debug, Clone)]
pub struct Report {
    /// Imported v1 settings.
    pub config: Config,
    /// Imported known keys.
    pub imported: Vec<Key>,
    /// Unsupported keys (including desktop and deprecated libfm keys).
    pub ignored: Vec<Key>,
    /// Invalid values and explicit quick_exec=1 advisory.
    pub warnings: Vec<String>,
}
/// Source access/parse failures, without corrupt-file recovery on the originals.
#[derive(Debug, thiserror::Error)]
#[error("PCManFM import: {0}")]
pub struct Error(#[from] pub glib::Error);
/// Import libfm first, then PCManFM's profile. Values use the normal Config codecs.
/// Unknown keys are reported, not imported; quick_exec=1 is called out explicitly.
pub fn read(pcmanfm: &Path, libfm: &Path) -> Result<Report, Error> {
    let mut report = Report {
        config: Config::default(),
        imported: vec![],
        ignored: vec![],
        warnings: vec![],
    };
    for (name, path) in [("libfm", libfm), ("pcmanfm", pcmanfm)] {
        let key_file = glib::KeyFile::new();
        key_file.load_from_file(path, glib::KeyFileFlags::NONE)?;
        for group in key_file.groups().iter() {
            for key in key_file.keys(group)?.iter() {
                let value = key_file.string(group, key)?.to_string();
                let item = Key {
                    source: name.into(),
                    group: group.to_string(),
                    key: key.to_string(),
                    value: value.clone(),
                };
                if report
                    .config
                    .apply(group, key, &value, &mut report.warnings)
                {
                    if key.as_str() == "quick_exec" && value == "1" {
                        report.warnings.push("Imported quick_exec=1: executable files run without a confirmation prompt on trusted native filesystems.".into());
                    }
                    report.imported.push(item);
                } else {
                    report.ignored.push(item);
                }
            }
        }
    }
    Ok(report)
}
