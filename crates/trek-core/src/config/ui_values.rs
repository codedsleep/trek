//! ui setting validation.
use super::*;
impl Config {
    pub(super) fn apply_ui(&mut self, key: &str, value: &str, warnings: &mut Vec<String>) -> bool {
        match key {
            "always_show_tabs" => {
                self.always_show_tabs = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().always_show_tabs,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "hide_close_btn" => {
                self.hide_close_btn = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().hide_close_btn,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "maximized" => {
                self.maximized = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().maximized,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "desktop_folder_new_win" => {
                self.desktop_folder_new_win = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().desktop_folder_new_win,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "change_tab_on_drop" => {
                self.change_tab_on_drop = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().change_tab_on_drop,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "close_on_unmount" => {
                self.close_on_unmount = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().close_on_unmount,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "focus_previous" => {
                self.focus_previous = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().focus_previous,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "show_hidden" => {
                self.show_hidden = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_hidden,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "show_statusbar" => {
                self.show_statusbar = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_statusbar,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "pathbar_mode_buttons" => {
                self.pathbar_mode_buttons = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().pathbar_mode_buttons,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "dual_pane" => {
                self.dual_pane = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().dual_pane,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "show_menubar" => {
                self.show_menubar = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_menubar,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "miller_parent" => {
                self.miller_parent = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().miller_parent,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "miller_preview" => {
                self.miller_preview = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().miller_preview,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "show_thumbnail" => {
                self.show_thumbnail = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().show_thumbnail,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "shadow_hidden" => {
                self.shadow_hidden = fallback(
                    match value {
                        "0" => Some(false),
                        "1" => Some(true),
                        _ => None,
                    },
                    || Self::default().shadow_hidden,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "max_tab_chars" => {
                self.max_tab_chars = fallback(
                    value.parse::<u32>().ok().filter(|v| (1..=1024).contains(v)),
                    || Self::default().max_tab_chars,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "win_width" => {
                self.win_width = fallback(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (1..=32768).contains(v)),
                    || Self::default().win_width,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "win_height" => {
                self.win_height = fallback(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (1..=32768).contains(v)),
                    || Self::default().win_height,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "splitter_pos" => {
                self.splitter_pos = fallback(
                    value
                        .parse::<u32>()
                        .ok()
                        .filter(|v| (0..=32768).contains(v)),
                    || Self::default().splitter_pos,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "big_icon_size" => {
                self.big_icon_size = fallback(
                    value.parse::<u32>().ok().filter(|v| (8..=1024).contains(v)),
                    || Self::default().big_icon_size,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "small_icon_size" => {
                self.small_icon_size = fallback(
                    value.parse::<u32>().ok().filter(|v| (8..=1024).contains(v)),
                    || Self::default().small_icon_size,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "pane_icon_size" => {
                self.pane_icon_size = fallback(
                    value.parse::<u32>().ok().filter(|v| (8..=1024).contains(v)),
                    || Self::default().pane_icon_size,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "thumbnail_size" => {
                self.thumbnail_size = fallback(
                    value.parse::<u32>().ok().filter(|v| (8..=1024).contains(v)),
                    || Self::default().thumbnail_size,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "side_pane_mode" => {
                self.side_pane_mode = fallback(
                    value.parse().ok(),
                    || Self::default().side_pane_mode,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "view_mode" => {
                self.view_mode = fallback(
                    value.parse().ok(),
                    || Self::default().view_mode,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "sort" => {
                self.sort = fallback(
                    value.parse().ok(),
                    || Self::default().sort,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "columns" => {
                self.columns = fallback(
                    value.parse().ok(),
                    || Self::default().columns,
                    "ui",
                    key,
                    value,
                    warnings,
                )
            }
            "toolbar" => {
                self.toolbar = fallback(
                    value.parse().ok(),
                    || Self::default().toolbar,
                    "ui",
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
