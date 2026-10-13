//! places setting validation.
use super::*;
impl Config {
    pub(super) fn apply_places(
        &mut self,
        key: &str,
        value: &str,
        warnings: &mut Vec<String>,
    ) -> bool {
        match key {
            "places_home" => {
                self.places_home = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_home,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_desktop" => {
                self.places_desktop = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_desktop,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_root" => {
                self.places_root = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_root,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_computer" => {
                self.places_computer = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_computer,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_trash" => {
                self.places_trash = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_trash,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_applications" => {
                self.places_applications = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_applications,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_network" => {
                self.places_network = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_network,
                    "places",
                    key,
                    value,
                    warnings,
                )
            }
            "places_unmounted" => {
                self.places_unmounted = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().places_unmounted,
                    "places",
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
