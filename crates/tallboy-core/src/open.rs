//! Activation decisions shared by every application entry point.
/// Kind of item being activated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Folder.
    Directory,
    /// Shortcut.
    Shortcut,
    /// Mountable.
    Mountable,
    /// Desktop launcher.
    Launcher,
    /// Text file.
    Text,
    /// Binary/other file.
    Binary,
}
/// Activation metadata, without GTK or GIO objects.
#[derive(Debug, Clone)]
pub struct Item {
    /// Item kind.
    pub kind: Kind,
    /// Executable access bit.
    pub executable: bool,
    /// Known native filesystem type.
    pub filesystem: Option<String>,
    /// Native path exists.
    pub native: bool,
    /// Under an XDG applications directory.
    pub in_applications: bool,
    /// Default application is available.
    pub has_default: bool,
}
/// Result of an activation policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Navigate.
    Navigate,
    /// Follow target URI.
    FollowTarget,
    /// Mount then navigate.
    Mount,
    /// Default application.
    LaunchDefault,
    /// Application chooser.
    OpenWith,
    /// Execute without prompting.
    Execute,
    /// Text editor.
    Edit,
    /// Binary prompt.
    AskBinary,
    /// Trusted launcher.
    LaunchLauncher,
    /// Confirm launcher trust.
    ConfirmLauncher,
}
/// Filesystems with synthetic executable bits are never trusted for launchers.
pub fn synthetic_execution(native: bool, filesystem: Option<&str>) -> bool {
    !native
        || filesystem.is_some_and(|f| {
            matches!(
                f.to_ascii_lowercase().as_str(),
                "vfat" | "exfat" | "ntfs" | "ntfs3" | "fuseblk"
            )
        })
}
/// Apply executable and removable-media policy, then send text to the editor,
/// before choosing a default app.
pub fn classify(item: &Item, quick_exec: bool) -> Activation {
    match item.kind {
        Kind::Directory => Activation::Navigate,
        Kind::Shortcut => Activation::FollowTarget,
        Kind::Mountable => Activation::Mount,
        Kind::Launcher => {
            if !synthetic_execution(item.native, item.filesystem.as_deref())
                && (item.executable || item.in_applications)
            {
                Activation::LaunchLauncher
            } else {
                Activation::ConfirmLauncher
            }
        }
        Kind::Text | Kind::Binary => {
            let text = item.kind == Kind::Text;
            let runnable = item.executable
                && !(text && synthetic_execution(item.native, item.filesystem.as_deref()));
            if runnable && quick_exec {
                Activation::Execute
            } else if runnable && !text {
                Activation::AskBinary
            } else if text && item.native {
                // Text is edited, scripts included: the editor needs a path.
                Activation::Edit
            } else if item.has_default {
                Activation::LaunchDefault
            } else {
                Activation::OpenWith
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decision_table() {
        let mut i = Item {
            kind: Kind::Directory,
            executable: true,
            filesystem: Some("btrfs".into()),
            native: true,
            in_applications: false,
            has_default: true,
        };
        for (kind, expected) in [
            (Kind::Directory, Activation::Navigate),
            (Kind::Shortcut, Activation::FollowTarget),
            (Kind::Mountable, Activation::Mount),
            (Kind::Launcher, Activation::LaunchLauncher),
            (Kind::Text, Activation::Edit),
            (Kind::Binary, Activation::AskBinary),
        ] {
            i.kind = kind;
            assert_eq!(classify(&i, false), expected);
        }
        i.kind = Kind::Text;
        assert_eq!(classify(&i, true), Activation::Execute);
        i.filesystem = Some("vfat".into());
        assert_eq!(classify(&i, true), Activation::Edit);
        i.native = false;
        assert_eq!(classify(&i, false), Activation::LaunchDefault);
        i.native = true;
        i.kind = Kind::Launcher;
        i.in_applications = true;
        assert_eq!(classify(&i, true), Activation::ConfirmLauncher);
        i.kind = Kind::Text;
        i.has_default = false;
        assert_eq!(classify(&i, false), Activation::Edit);
        i.native = false;
        assert_eq!(classify(&i, false), Activation::OpenWith);
        i.native = true;
        i.kind = Kind::Binary;
        i.executable = false;
        assert_eq!(classify(&i, true), Activation::OpenWith);
        i.executable = true;
        i.filesystem = Some("btrfs".into());
        i.kind = Kind::Launcher;
        i.executable = false;
        i.in_applications = false;
        assert_eq!(classify(&i, false), Activation::ConfirmLauncher);
    }
}
