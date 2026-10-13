//! volume setting validation.
use super::*;
impl Config {
    pub(super) fn apply_volume(
        &mut self,
        key: &str,
        value: &str,
        warnings: &mut Vec<String>,
    ) -> bool {
        match key {
            "mount_on_startup" => {
                self.mount_on_startup = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().mount_on_startup,
                    "volume",
                    key,
                    value,
                    warnings,
                )
            }
            "mount_removable" => {
                self.mount_removable = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().mount_removable,
                    "volume",
                    key,
                    value,
                    warnings,
                )
            }
            "autorun" => {
                self.autorun = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().autorun,
                    "volume",
                    key,
                    value,
                    warnings,
                )
            }
            _ => return false,
        }
        true
    }
}
