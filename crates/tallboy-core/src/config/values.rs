//! Config encoding and warning policy.
use super::*;
pub(super) fn fallback<T>(
    value: Option<T>,
    default: impl FnOnce() -> T,
    group: &str,
    key: &str,
    input: &str,
    warnings: &mut Vec<String>,
) -> T {
    value.unwrap_or_else(|| {
        warnings.push(format!("Invalid {group}/{key}: {input}; using default"));
        default()
    })
}
macro_rules! entry {
    ($self:ident,$g:literal,$field:ident,bool) => {
        (
            $g.into(),
            stringify!($field).into(),
            if $self.$field { "1".into() } else { "0".into() },
        )
    };
    ($self:ident,$g:literal,$field:ident,$t:ty) => {
        (
            $g.into(),
            stringify!($field).into(),
            $self.$field.to_string(),
        )
    };
}
impl Config {
    /// Apply a known key. Invalid values restore its default and append one warning.
    /// Returns false for unknown keys so storage can retain them.
    pub fn apply(
        &mut self,
        group: &str,
        key: &str,
        value: &str,
        warnings: &mut Vec<String>,
    ) -> bool {
        match group {
            "config" => self.apply_config(key, value, warnings),
            "volume" => self.apply_volume(key, value, warnings),
            "ui" => self.apply_ui(key, value, warnings),
            "places" => self.apply_places(key, value, warnings),
            "autorun" => {
                self.autorun_choices
                    .insert(key.into(), AutorunChoice::parse(key, value));
                true
            }
            _ => false,
        }
    }
    /// Encode all known keys in upstream format.
    pub fn entries(&self) -> Vec<(String, String, String)> {
        let mut entries = vec![
            entry!(self, "config", single_click, bool),
            entry!(self, "config", use_trash, bool),
            entry!(self, "config", confirm_del, bool),
            entry!(self, "config", confirm_trash, bool),
            entry!(self, "config", no_usb_trash, bool),
            entry!(self, "config", quick_exec, bool),
            entry!(self, "config", si_unit, bool),
            entry!(self, "config", backup_as_hidden, bool),
            entry!(self, "config", show_full_names, bool),
            entry!(self, "config", advanced_mode, bool),
            entry!(self, "config", only_user_templates, bool),
            entry!(self, "config", template_type_once, bool),
            entry!(self, "config", template_run_app, bool),
            entry!(self, "config", defer_content_test, bool),
            entry!(self, "config", no_child_non_expandable, bool),
            entry!(self, "config", show_internal_volumes, bool),
            entry!(self, "config", thumbnail_local, bool),
            entry!(self, "volume", mount_on_startup, bool),
            entry!(self, "volume", mount_removable, bool),
            entry!(self, "volume", autorun, bool),
            entry!(self, "ui", always_show_tabs, bool),
            entry!(self, "ui", hide_close_btn, bool),
            entry!(self, "ui", maximized, bool),
            entry!(self, "ui", media_in_new_tab, bool),
            entry!(self, "ui", desktop_folder_new_win, bool),
            entry!(self, "ui", change_tab_on_drop, bool),
            entry!(self, "ui", close_on_unmount, bool),
            entry!(self, "ui", focus_previous, bool),
            entry!(self, "ui", show_hidden, bool),
            entry!(self, "ui", show_statusbar, bool),
            entry!(self, "ui", pathbar_mode_buttons, bool),
            entry!(self, "ui", dual_pane, bool),
            entry!(self, "ui", show_thumbnail, bool),
            entry!(self, "ui", shadow_hidden, bool),
            entry!(self, "places", places_home, bool),
            entry!(self, "places", places_desktop, bool),
            entry!(self, "places", places_root, bool),
            entry!(self, "places", places_computer, bool),
            entry!(self, "places", places_trash, bool),
            entry!(self, "places", places_applications, bool),
            entry!(self, "places", places_network, bool),
            entry!(self, "places", places_unmounted, bool),
            entry!(self, "config", auto_selection_delay, u32),
            entry!(self, "config", thumbnail_max, u32),
            entry!(self, "ui", max_tab_chars, u32),
            entry!(self, "ui", win_width, u32),
            entry!(self, "ui", win_height, u32),
            entry!(self, "ui", splitter_pos, u32),
            entry!(self, "ui", big_icon_size, u32),
            entry!(self, "ui", small_icon_size, u32),
            entry!(self, "ui", pane_icon_size, u32),
            entry!(self, "ui", thumbnail_size, u32),
            entry!(self, "config", home_path, String),
            entry!(self, "config", terminal, String),
            entry!(self, "config", archiver, String),
            entry!(self, "config", bm_open_method, BookmarkOpen),
            entry!(self, "config", drop_default_action, DropAction),
            entry!(self, "ui", side_pane_mode, SidePane),
            entry!(self, "ui", view_mode, ViewMode),
            entry!(self, "ui", sort, SortSpec),
            entry!(self, "ui", columns, Columns),
            entry!(self, "ui", toolbar, Toolbar),
        ];
        entries.extend(
            self.autorun_choices
                .iter()
                .map(|(key, value)| ("autorun".into(), key.clone(), value.encode(key))),
        );
        entries
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_keys_roundtrip() {
        let mut original = Config::default();
        let mut warnings = vec![];
        for (g, k, v) in original.entries() {
            let changed = if v == "0" {
                "1"
            } else if v == "1" {
                "0"
            } else {
                &v
            };
            original.apply(&g, &k, changed, &mut warnings);
        }
        let mut other = Config::default();
        for (g, k, v) in original.entries() {
            assert!(other.apply(&g, &k, &v, &mut warnings));
        }
        assert_eq!(original, other);
        assert!(warnings.is_empty());
    }
    #[test]
    fn invalid_values_warn_once() {
        for (g, k, v) in [
            ("ui", "view_mode", "bad"),
            ("ui", "win_width", "0"),
            ("config", "use_trash", "true"),
            ("ui", "sort", "bad"),
        ] {
            let mut config = Config::default();
            let mut warnings = vec![];
            assert!(config.apply(g, k, v, &mut warnings));
            assert_eq!(config, Config::default());
            assert_eq!(warnings.len(), 1);
        }
        assert!(!Config::default().apply("desktop", "wallpaper", "x", &mut vec![]));
    }
}
