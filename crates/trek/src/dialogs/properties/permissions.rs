//! The Permissions page of Properties (PROMPT §8.18). `core::perms` maps
//! mode bits to the three dropdowns and back; this page only shows them and
//! reports what the user changed.

use super::ids;
use gtk::prelude::*;
use trek_core::perms::{self, AttrChange, Audience, Permission};
use trek_fs::meta::FileMeta;

const AUDIENCES: [(&str, Audience); 4] = [
    ("Nobody", Audience::Nobody),
    ("Only owner", Audience::Owner),
    ("Only owner and group", Audience::OwnerGroup),
    ("Anyone", Audience::Anyone),
];
/// Shown only for a selection whose files differ; choosing it changes nothing.
const MIXED: &str = "(no change)";
const SPECIAL: [(&str, u32); 3] = [
    ("Set SUID", 0o4000),
    ("Set SGID", 0o2000),
    ("Sticky", 0o1000),
];

struct Choice {
    permission: Permission,
    dropdown: gtk::DropDown,
    initial: u32,
}

/// The page and the state needed to read the user's changes back.
pub struct Permissions {
    root: gtk::Grid,
    owner: gtk::Entry,
    group: gtk::Entry,
    initial_owner: String,
    initial_group: String,
    choices: Vec<Choice>,
    special: Vec<(gtk::CheckButton, u32, Option<bool>)>,
}

fn same<T: PartialEq + Clone>(values: impl Iterator<Item = T>) -> Option<T> {
    let mut values = values;
    let first = values.next()?;
    values.all(|value| value == first).then_some(first)
}

impl Permissions {
    pub fn new(files: &[FileMeta], advanced: bool) -> Self {
        let root = gtk::Grid::new();
        root.set_row_spacing(8);
        root.set_column_spacing(12);
        root.add_css_class("dialog-body");
        let label = |text: &str| {
            let label = gtk::Label::new(Some(text));
            label.set_xalign(0.0);
            label
        };
        let mut row = 0;

        // Owner and group: one name when every file agrees.
        let initial_owner = same(files.iter().map(|f| f.owner.to_string())).unwrap_or_default();
        let initial_group = same(files.iter().map(|f| f.group.to_string())).unwrap_or_default();
        let me = ids::current_uid();
        let is_root = me == Some(0);
        let all_mine = files.iter().all(|f| Some(f.uid) == me);
        let owner = gtk::Entry::new();
        owner.set_text(&initial_owner);
        // Only root may give files away; an owner may change the group.
        owner.set_sensitive(is_root);
        owner.set_hexpand(true);
        let group = gtk::Entry::new();
        group.set_text(&initial_group);
        group.set_sensitive(is_root || all_mine);
        root.attach(&label("Owner:"), 0, row, 1, 1);
        root.attach(&owner, 1, row, 1, 1);
        row += 1;
        root.attach(&label("Group:"), 0, row, 1, 1);
        root.attach(&group, 1, row, 1, 1);
        row += 1;
        root.attach(
            &gtk::Separator::new(gtk::Orientation::Horizontal),
            0,
            row,
            2,
            1,
        );
        row += 1;

        let folders = files
            .iter()
            .filter(|f| f.file_type == gtk::gio::FileType::Directory)
            .count();
        let execute = match (folders, files.len()) {
            (0, _) => "E_xecute:",
            (n, all) if n == all => "_Access content:",
            _ => "E_xecute / access content:",
        };
        let modes: Vec<u32> = files.iter().map(|f| f.mode).collect();
        let can_change = is_root || all_mine;
        let mut choices = Vec::new();
        for (caption, permission) in [
            ("Vie_w content:", Permission::Read),
            ("C_hange content:", Permission::Write),
            (execute, Permission::Execute),
        ] {
            let audience = perms::aggregate(&modes, permission);
            let mut options: Vec<&str> = AUDIENCES.iter().map(|(text, _)| *text).collect();
            let initial = match AUDIENCES.iter().position(|(_, a)| *a == audience) {
                Some(index) => index as u32,
                None => {
                    options.push(MIXED);
                    AUDIENCES.len() as u32
                }
            };
            let dropdown = gtk::DropDown::from_strings(&options);
            dropdown.set_selected(initial);
            dropdown.set_sensitive(can_change);
            let name = gtk::Label::with_mnemonic(caption);
            name.set_xalign(0.0);
            name.set_mnemonic_widget(Some(&dropdown));
            root.attach(&name, 0, row, 1, 1);
            root.attach(&dropdown, 1, row, 1, 1);
            row += 1;
            choices.push(Choice {
                permission,
                dropdown,
                initial,
            });
        }

        // `advanced_mode`: the special bits.
        let mut special = Vec::new();
        if advanced {
            root.attach(&label("Special bits:"), 0, row, 1, 1);
            let bits = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            for (caption, mask) in SPECIAL {
                let check = gtk::CheckButton::with_label(caption);
                let state = same(modes.iter().map(|mode| mode & mask != 0));
                match state {
                    Some(on) => check.set_active(on),
                    None => check.set_inconsistent(true),
                }
                check.set_sensitive(can_change);
                // A click settles a mixed box one way or the other.
                check.connect_toggled(|check| check.set_inconsistent(false));
                bits.append(&check);
                special.push((check, mask, state));
            }
            root.attach(&bits, 1, row, 1, 1);
        }
        Self {
            root,
            owner,
            group,
            initial_owner,
            initial_group,
            choices,
            special,
        }
    }

    pub fn widget(&self) -> &gtk::Grid {
        &self.root
    }

    /// What the user changed, as a masked change: a control left alone
    /// contributes nothing. An unknown owner or group name is an error.
    pub fn change(&self) -> Result<AttrChange, String> {
        let mut change = AttrChange::default();
        for choice in &self.choices {
            let selected = choice.dropdown.selected();
            if selected != choice.initial
                && let Some((_, audience)) = AUDIENCES.get(selected as usize)
            {
                change.set(choice.permission, *audience);
            }
        }
        for (check, mask, initial) in &self.special {
            if !check.is_inconsistent() && Some(check.is_active()) != *initial {
                change.special(*mask, check.is_active());
            }
        }
        let owner = self.owner.text();
        if owner.trim() != self.initial_owner && !owner.trim().is_empty() {
            change.uid = Some(
                ids::uid(&owner).ok_or_else(|| format!("There is no user called \"{owner}\"."))?,
            );
        }
        let group = self.group.text();
        if group.trim() != self.initial_group && !group.trim().is_empty() {
            change.gid = Some(
                ids::gid(&group).ok_or_else(|| format!("There is no group called \"{group}\"."))?,
            );
        }
        Ok(change)
    }
}

/// Whether a change would do anything.
pub fn is_empty(change: &AttrChange) -> bool {
    change.mode_mask == 0 && change.uid.is_none() && change.gid.is_none()
}
