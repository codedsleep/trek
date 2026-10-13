//! Action enablement based on capabilities, location and selection.
/// Selected item capabilities, aggregated with logical AND by the caller.
#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    /// Count.
    pub count: usize,
    /// All deletable.
    pub deletable: bool,
    /// All trashable.
    pub trashable: bool,
    /// All renameable.
    pub renameable: bool,
    /// All readable.
    pub readable: bool,
}
/// Enabled/visible file actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actions {
    /// Open selection.
    pub open: bool,
    /// Copy selection.
    pub copy: bool,
    /// Cut selection.
    pub cut: bool,
    /// Paste files.
    pub paste: bool,
    /// Rename single item.
    pub rename: bool,
    /// Delete selection.
    pub delete: bool,
    /// Trash visible outside trash only.
    pub trash_visible: bool,
    /// Trash enabled.
    pub trash: bool,
    /// Restore selection inside trash.
    pub restore: bool,
    /// Create in writable location.
    pub create: bool,
}
/// Resolve all action flags in one place.
pub fn actions(s: Selection, writable: bool, in_trash: bool, clipboard_has_files: bool) -> Actions {
    let selected = s.count > 0;
    Actions {
        open: selected,
        copy: selected && s.readable,
        cut: selected && s.deletable && !in_trash,
        paste: writable && clipboard_has_files && !in_trash,
        rename: s.count == 1 && s.renameable && !in_trash,
        delete: selected && s.deletable,
        trash_visible: !in_trash,
        trash: selected && s.trashable && !in_trash,
        restore: selected && in_trash,
        create: writable && !in_trash,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn table() {
        for writable in [false, true] {
            for clipboard in [false, true] {
                for trash in [false, true] {
                    for count in [0, 1, 2] {
                        let a = actions(
                            Selection {
                                count,
                                deletable: true,
                                trashable: true,
                                renameable: true,
                                readable: true,
                            },
                            writable,
                            trash,
                            clipboard,
                        );
                        assert_eq!(a.paste, writable && clipboard && !trash);
                        assert_eq!(a.rename, count == 1 && !trash);
                        assert_eq!(a.trash_visible, !trash);
                    }
                }
            }
        }
    }
}
