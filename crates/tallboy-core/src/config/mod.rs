//! Pure typed configuration; storage preserves unknown keys.
mod codecs;
mod values;
use values::fallback;
mod config_values;
mod places_values;
mod ui_values;
mod volume_values;
use crate::sort::SortSpec;
pub use codecs::*;
use std::collections::BTreeMap;
/// All v1 settings, excluding deferred desktop configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `config/single_click`.
    pub single_click: bool,
    /// `config/use_trash`.
    pub use_trash: bool,
    /// `config/confirm_del`.
    pub confirm_del: bool,
    /// `config/confirm_trash`.
    pub confirm_trash: bool,
    /// `config/no_usb_trash`.
    pub no_usb_trash: bool,
    /// `config/quick_exec`.
    pub quick_exec: bool,
    /// `config/si_unit`.
    pub si_unit: bool,
    /// `config/backup_as_hidden`.
    pub backup_as_hidden: bool,
    /// `config/show_full_names`.
    pub show_full_names: bool,
    /// `config/advanced_mode`.
    pub advanced_mode: bool,
    /// `config/only_user_templates`.
    pub only_user_templates: bool,
    /// `config/template_type_once`.
    pub template_type_once: bool,
    /// `config/template_run_app`.
    pub template_run_app: bool,
    /// `config/defer_content_test`.
    pub defer_content_test: bool,
    /// `config/no_child_non_expandable`.
    pub no_child_non_expandable: bool,
    /// `config/show_internal_volumes`.
    pub show_internal_volumes: bool,
    /// `config/thumbnail_local`.
    pub thumbnail_local: bool,
    /// `volume/mount_on_startup`.
    pub mount_on_startup: bool,
    /// `volume/mount_removable`.
    pub mount_removable: bool,
    /// `volume/autorun`.
    pub autorun: bool,
    /// `ui/always_show_tabs`.
    pub always_show_tabs: bool,
    /// `ui/hide_close_btn`.
    pub hide_close_btn: bool,
    /// `ui/maximized`.
    pub maximized: bool,
    /// `ui/media_in_new_tab`.
    pub media_in_new_tab: bool,
    /// `ui/desktop_folder_new_win`.
    pub desktop_folder_new_win: bool,
    /// `ui/change_tab_on_drop`.
    pub change_tab_on_drop: bool,
    /// `ui/close_on_unmount`.
    pub close_on_unmount: bool,
    /// `ui/focus_previous`.
    pub focus_previous: bool,
    /// `ui/show_hidden`.
    pub show_hidden: bool,
    /// `ui/show_statusbar`.
    pub show_statusbar: bool,
    /// `ui/pathbar_mode_buttons`.
    pub pathbar_mode_buttons: bool,
    /// `ui/dual_pane`.
    pub dual_pane: bool,
    /// `ui/show_thumbnail`.
    pub show_thumbnail: bool,
    /// `ui/shadow_hidden`.
    pub shadow_hidden: bool,
    /// `places/places_home`.
    pub places_home: bool,
    /// `places/places_desktop`.
    pub places_desktop: bool,
    /// `places/places_root`.
    pub places_root: bool,
    /// `places/places_computer`.
    pub places_computer: bool,
    /// `places/places_trash`.
    pub places_trash: bool,
    /// `places/places_applications`.
    pub places_applications: bool,
    /// `places/places_network`.
    pub places_network: bool,
    /// `places/places_unmounted`.
    pub places_unmounted: bool,
    /// `config/auto_selection_delay`.
    pub auto_selection_delay: u32,
    /// `config/thumbnail_max`.
    pub thumbnail_max: u32,
    /// `ui/max_tab_chars`.
    pub max_tab_chars: u32,
    /// `ui/win_width`.
    pub win_width: u32,
    /// `ui/win_height`.
    pub win_height: u32,
    /// `ui/splitter_pos`.
    pub splitter_pos: u32,
    /// `ui/big_icon_size`.
    pub big_icon_size: u32,
    /// `ui/small_icon_size`.
    pub small_icon_size: u32,
    /// `ui/pane_icon_size`.
    pub pane_icon_size: u32,
    /// `ui/thumbnail_size`.
    pub thumbnail_size: u32,
    /// `config/home_path`.
    pub home_path: String,
    /// `config/terminal`.
    pub terminal: String,
    /// `config/archiver`.
    pub archiver: String,
    /// `config/bm_open_method`.
    pub bm_open_method: BookmarkOpen,
    /// `config/drop_default_action`.
    pub drop_default_action: DropAction,
    /// `ui/side_pane_mode`.
    pub side_pane_mode: SidePane,
    /// `ui/view_mode`.
    pub view_mode: ViewMode,
    /// `ui/sort`.
    pub sort: SortSpec,
    /// `ui/columns`.
    pub columns: Columns,
    /// `ui/toolbar`.
    pub toolbar: Toolbar,
    /// Media content-type choices.
    pub autorun_choices: BTreeMap<String, AutorunChoice>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            single_click: false,
            use_trash: true,
            confirm_del: true,
            confirm_trash: true,
            no_usb_trash: true,
            quick_exec: false,
            si_unit: false,
            backup_as_hidden: true,
            show_full_names: false,
            advanced_mode: false,
            only_user_templates: false,
            template_type_once: false,
            template_run_app: false,
            defer_content_test: false,
            no_child_non_expandable: false,
            show_internal_volumes: false,
            thumbnail_local: true,
            mount_on_startup: true,
            mount_removable: true,
            autorun: true,
            always_show_tabs: false,
            hide_close_btn: false,
            maximized: false,
            media_in_new_tab: false,
            desktop_folder_new_win: false,
            change_tab_on_drop: true,
            close_on_unmount: true,
            focus_previous: false,
            show_hidden: false,
            show_statusbar: true,
            pathbar_mode_buttons: false,
            dual_pane: false,
            show_thumbnail: true,
            shadow_hidden: false,
            places_home: true,
            places_desktop: true,
            places_root: false,
            places_computer: true,
            places_trash: true,
            places_applications: true,
            places_network: false,
            places_unmounted: true,
            auto_selection_delay: 600,
            thumbnail_max: 2048,
            max_tab_chars: 32,
            win_width: 640,
            win_height: 480,
            splitter_pos: 150,
            big_icon_size: 48,
            small_icon_size: 24,
            pane_icon_size: 24,
            thumbnail_size: 128,
            home_path: String::new(),
            terminal: String::new(),
            archiver: String::new(),
            bm_open_method: BookmarkOpen::Current,
            drop_default_action: DropAction::Auto,
            side_pane_mode: SidePane {
                hidden: false,
                mode: PaneMode::Places,
            },
            view_mode: ViewMode::Icon,
            sort: SortSpec::default(),
            columns: Columns::default(),
            toolbar: Toolbar(vec!["newtab".into(), "navigation".into(), "home".into()]),
            autorun_choices: BTreeMap::new(),
        }
    }
}
