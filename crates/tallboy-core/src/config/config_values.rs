//! config setting validation.
use super::*;
impl Config {
    pub(super) fn apply_config(
        &mut self,
        key: &str,
        value: &str,
        warnings: &mut Vec<String>,
    ) -> bool {
        match key {
            "single_click" => {
                self.single_click = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().single_click,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "use_trash" => {
                self.use_trash = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().use_trash,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "confirm_del" => {
                self.confirm_del = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().confirm_del,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "confirm_trash" => {
                self.confirm_trash = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().confirm_trash,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "no_usb_trash" => {
                self.no_usb_trash = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().no_usb_trash,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "quick_exec" => {
                self.quick_exec = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().quick_exec,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "si_unit" => {
                self.si_unit = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().si_unit,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "backup_as_hidden" => {
                self.backup_as_hidden = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().backup_as_hidden,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "show_full_names" => {
                self.show_full_names = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_full_names,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "advanced_mode" => {
                self.advanced_mode = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().advanced_mode,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "only_user_templates" => {
                self.only_user_templates = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().only_user_templates,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "template_type_once" => {
                self.template_type_once = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().template_type_once,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "template_run_app" => {
                self.template_run_app = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().template_run_app,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "defer_content_test" => {
                self.defer_content_test = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().defer_content_test,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "no_child_non_expandable" => {
                self.no_child_non_expandable = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().no_child_non_expandable,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "show_internal_volumes" => {
                self.show_internal_volumes = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_internal_volumes,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "thumbnail_local" => {
                self.thumbnail_local = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().thumbnail_local,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "auto_selection_delay" => {
                self.auto_selection_delay = fallback(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (0..=60000).contains(v)),
                    || Self::default().auto_selection_delay,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "thumbnail_max" => {
                self.thumbnail_max = fallback(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (0..=4294967295).contains(v)),
                    || Self::default().thumbnail_max,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "home_path" => {
                self.home_path = fallback(
                    Some(value.to_owned()),
                    || Self::default().home_path,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "terminal" => {
                self.terminal = fallback(
                    Some(value.to_owned()),
                    || Self::default().terminal,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "archiver" => {
                self.archiver = fallback(
                    Some(value.to_owned()),
                    || Self::default().archiver,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "bm_open_method" => {
                self.bm_open_method = fallback(
                    value.parse().ok(),
                    || Self::default().bm_open_method,
                    "config",
                    key,
                    value,
                    warnings,
                )
            }
            "drop_default_action" => {
                self.drop_default_action = fallback(
                    value.parse().ok(),
                    || Self::default().drop_default_action,
                    "config",
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
