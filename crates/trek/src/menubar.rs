//! The menu bar model: the PROMPT §8.2 tree, built in code as `gio::Menu`.

use gtk::{gio, prelude::*};

fn section(items: &[(&str, &str)]) -> gio::Menu {
    let menu = gio::Menu::new();
    for (label, action) in items {
        let item = gio::MenuItem::new(Some(label), Some(action));
        if *action == "win.trash" {
            // The window removes this action inside trash:// (§8.16).
            item.set_attribute_value("hidden-when", Some(&"action-missing".to_variant()));
        }
        menu.append_item(&item);
    }
    menu
}

fn sections(parts: &[&gio::Menu]) -> gio::Menu {
    let menu = gio::Menu::new();
    for part in parts {
        menu.append_section(None, *part);
    }
    menu
}

fn submenu_section(label: &str, submenu: &gio::Menu) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append_submenu(Some(label), submenu);
    menu
}

/// The Sort Files submenu, shared with the folder context menu.
pub fn sort_menu() -> gio::Menu {
    sections(&[
        &section(&[
            ("_Ascending", "win.sort-order::ascending"),
            ("_Descending", "win.sort-order::descending"),
        ]),
        &section(&[
            ("By _Name", "win.sort-by::name"),
            ("By _Modification Time", "win.sort-by::mtime"),
            ("By _Size", "win.sort-by::size"),
            ("By File _Type", "win.sort-by::desc"),
            ("By _Extension", "win.sort-by::ext"),
        ]),
        &section(&[
            ("Mingle _Files and Folders", "win.sort-mingle"),
            ("_Ignore Name Case", "win.sort-ignore-case"),
        ]),
    ])
}

/// The Create New submenu, shared with the folder context menu. Templates
/// are deferred (DISCOVERY D4), so only Folder and Empty File are listed.
pub fn create_menu() -> gio::Menu {
    section(&[
        ("Folder", "win.create-folder"),
        ("Empty File", "win.create-file"),
    ])
}

fn file_menu() -> gio::Menu {
    sections(&[
        &section(&[
            ("_New Window", "win.new-window"),
            ("New T_ab", "win.new-tab"),
        ]),
        &submenu_section("C_reate New...", &create_menu()),
        &section(&[("Folder Propertie_s", "win.folder-properties")]),
        &section(&[
            ("_Close Tab", "win.close-tab"),
            ("Close _Window", "win.close-window"),
        ]),
    ])
}

fn edit_menu() -> gio::Menu {
    sections(&[
        &section(&[("_Open", "win.open")]),
        &section(&[
            ("C_ut", "win.cut"),
            ("_Copy", "win.copy"),
            ("_Paste", "win.paste"),
            ("Move to _Trash", "win.trash"),
            ("_Remove", "win.delete"),
            ("Copy Pat_h(s)", "win.copy-path"),
        ]),
        &section(&[("Prop_erties", "win.properties")]),
        &section(&[
            ("Re_name...", "win.rename"),
            ("Create Lin_k...", "win.create-link"),
            ("_Move to...", "win.move-to"),
            ("Copy to_...", "win.copy-to"),
        ]),
        &section(&[
            ("Select _All", "win.select-all"),
            ("_Invert Selection", "win.invert-selection"),
        ]),
        &section(&[("Prefere_nces", "win.preferences")]),
    ])
}

fn view_menu() -> gio::Menu {
    let mode = section(&[
        ("Li_ne View", "win.view-mode::lines"),
        ("_Icon View", "win.view-mode::icon"),
        ("_Compact View", "win.view-mode::compact"),
        ("_Thumbnail View", "win.view-mode::thumbnail"),
        ("Detailed _List View", "win.view-mode::list"),
    ]);
    let toolbar = sections(&[
        &section(&[("Show _Toolbar", "win.toolbar-show")]),
        &section(&[
            ("Show 'New _Window' Button", "win.toolbar-newwin"),
            ("Show 'New _Tab' Button", "win.toolbar-newtab"),
            ("Show _Navigation Buttons", "win.toolbar-nav"),
            ("Show '_Home' Button", "win.toolbar-home"),
        ]),
    ]);
    let pathbar = section(&[
        ("_Location", "win.pathbar-mode::location"),
        ("_Buttons", "win.pathbar-mode::buttons"),
    ]);
    let side = sections(&[
        &section(&[("Sho_w Side Pane", "win.sidepane-show")]),
        &section(&[
            ("Places", "win.sidepane-mode::places"),
            ("Directory Tree", "win.sidepane-mode::dirtree"),
        ]),
    ]);
    let first = gio::Menu::new();
    first.append(Some("Show Hidde_n"), Some("win.show-hidden"));
    first.append_submenu(Some("_Sort Files"), &sort_menu());
    first.append_submenu(Some("Fo_lder View Mode"), &mode);
    first.append(
        Some("_Preserve This Folder's Settings"),
        Some("win.preserve-settings"),
    );
    let chrome = gio::Menu::new();
    chrome.append_submenu(Some("_Toolbar"), &toolbar);
    chrome.append_submenu(Some("Pat_h Bar"), &pathbar);
    chrome.append_submenu(Some("Side _Pane"), &side);
    chrome.append(Some("Parent _Column"), Some("win.miller-parent"));
    chrome.append(Some("Pre_view Column"), Some("win.miller-preview"));
    chrome.append(Some("Show Status B_ar"), Some("win.show-statusbar"));
    chrome.append(Some("Show _Menu Bar"), Some("win.show-menubar"));
    sections(&[
        &section(&[("_Reload Folder", "win.reload")]),
        &first,
        &chrome,
        &section(&[
            ("_Dual Pane Mode", "win.dual-pane"),
            ("Fullscreen _Mode", "win.fullscreen"),
        ]),
        &section(&[
            ("Zoom _In", "win.zoom-in"),
            ("Zoom _Out", "win.zoom-out"),
            ("_Normal Size", "win.zoom-normal"),
        ]),
        &section(&[("Fil_ter...", "win.filter")]),
    ])
}

fn go_menu() -> gio::Menu {
    sections(&[
        &section(&[
            ("_Previous Folder", "win.back"),
            ("_Next Folder", "win.forward"),
            ("Pa_rent Folder", "win.up"),
        ]),
        &section(&[
            ("H_ome Folder", "win.home"),
            ("_Desktop", "win.go-desktop"),
            ("_Trash Can", "win.go-trash"),
            ("_Applications", "win.go-applications"),
            ("Dev_ices", "win.go-devices"),
            ("Net_work", "win.go-network"),
        ]),
        &section(&[
            ("_Go to Location...", "win.go-location"),
            ("_Connect to Server...", "win.connect-server"),
        ]),
    ])
}

/// Builds the whole menu bar. `bookmarks` is the live section that the
/// bookmark store refills; it is shared by every window.
pub fn build(bookmarks: &gio::Menu) -> gio::Menu {
    let bookmarks_menu = sections(&[
        &section(&[
            ("_Add to Bookmarks...", "win.add-bookmark"),
            ("_Edit Bookmarks...", "win.edit-bookmarks"),
        ]),
        bookmarks,
    ]);
    let tools = section(&[
        ("Open Current Folder in _Terminal", "win.terminal"),
        ("Fin_d Files...", "win.find-files"),
        ("_Run a Command in Current Folder...", "win.run-command"),
    ]);
    let help = section(&[
        ("_About", "win.about"),
        ("_Keys", "win.keyboard-navigation"),
    ]);
    let bar = gio::Menu::new();
    bar.append_submenu(Some("_File"), &file_menu());
    bar.append_submenu(Some("_Edit"), &edit_menu());
    bar.append_submenu(Some("_View"), &view_menu());
    bar.append_submenu(Some("_Bookmarks"), &bookmarks_menu);
    bar.append_submenu(Some("_Go"), &go_menu());
    bar.append_submenu(Some("Too_ls"), &tools);
    bar.append_submenu(Some("_Help"), &help);
    bar
}
