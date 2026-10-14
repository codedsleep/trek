//! Metadata boundary: one GIO query to a Send, widget-free value.
use gio::prelude::*;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex, PoisonError},
};
use tallboy_core::sort::{SortFacts, SortSpec};
/// The attributes every file is asked for, written once. The icon is kept
/// apart: asking GIO for `standard::icon` makes it read the start of every
/// file whose name does not settle its type, whichever content-type
/// attribute is asked for beside it.
macro_rules! common_attributes {
    () => {
        "standard::name,standard::display-name,standard::edit-name,standard::type,standard::is-hidden,standard::is-backup,standard::is-symlink,standard::symlink-target,standard::size,standard::allocated-size,standard::target-uri,time::modified,time::access,time::created,unix::is-mountpoint,unix::mode,unix::uid,unix::gid,owner::user,owner::group,access::*,id::filesystem,trash::*"
    };
}
/// Common metadata attributes; content testing is appended according to the preference.
pub const ATTRIBUTES: &str = concat!(common_attributes!(), ",standard::icon");
/// Content-type query variant without duplicating the attribute list.
pub fn attributes(defer_content_test: bool) -> String {
    format!(
        "{ATTRIBUTES},{}",
        if defer_content_test {
            "standard::fast-content-type"
        } else {
            "standard::content-type"
        }
    )
}
/// What a folder listing asks of each file: the type its name gives, and no
/// file is opened. A local file's icon follows from its type, see
/// [`type_facts`]; other backends know icons that no type gives (the trash
/// can, a network share), so they are still asked.
pub fn listing_attributes(native: bool) -> String {
    format!(
        "{},standard::fast-content-type",
        if native {
            common_attributes!()
        } else {
            ATTRIBUTES
        }
    )
}
/// What follows from a file's content type. Shared between files, see [`intern`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFacts {
    /// MIME type.
    pub content_type: Arc<str>,
    /// MIME description.
    pub description: Arc<str>,
    /// Serialized GIO icon of the type.
    pub icon: Option<Arc<str>>,
}
const TYPE_LIMIT: usize = 1024;
/// Description and icon of a content type, looked up once per type instead
/// of once per file.
pub fn type_facts(content_type: &str) -> TypeFacts {
    static KNOWN: LazyLock<Mutex<HashMap<Arc<str>, TypeFacts>>> = LazyLock::new(Mutex::default);
    let mut known = KNOWN.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(facts) = known.get(content_type) {
        return facts.clone();
    }
    if known.len() >= TYPE_LIMIT {
        known.clear();
    }
    let shared = intern(content_type);
    let facts = TypeFacts {
        description: intern(&gio::content_type_get_description(content_type)),
        icon: gio::content_type_get_icon(content_type)
            .to_string()
            .map(|icon| intern(&icon)),
        content_type: shared.clone(),
    };
    known.insert(shared, facts.clone());
    facts
}
/// The icons GIO gives the home folder and the XDG user folders in place of
/// the plain folder icon, as its local backend does for `standard::icon`.
fn special_folder_icon(path: &std::path::Path) -> Option<Arc<str>> {
    static SPECIAL: LazyLock<Vec<(PathBuf, Option<Arc<str>>)>> = LazyLock::new(|| {
        use glib::UserDirectory as Dir;
        let plain = |name: &str| gio::ThemedIcon::new(name);
        let with_fallbacks = |name: &str| gio::ThemedIcon::with_default_fallbacks(name);
        let mut special = vec![(glib::home_dir(), plain("user-home"))];
        for (directory, icon) in [
            (Dir::Desktop, plain("user-desktop")),
            (Dir::Documents, with_fallbacks("folder-documents")),
            (Dir::Downloads, with_fallbacks("folder-download")),
            (Dir::Music, with_fallbacks("folder-music")),
            (Dir::Pictures, with_fallbacks("folder-pictures")),
            (Dir::PublicShare, with_fallbacks("folder-publicshare")),
            (Dir::Templates, with_fallbacks("folder-templates")),
            (Dir::Videos, with_fallbacks("folder-videos")),
        ] {
            // An unset user folder is reported as the home folder, which
            // comes first, as it does in GIO.
            if let Some(path) = glib::user_special_dir(directory) {
                special.push((path, icon));
            }
        }
        special
            .into_iter()
            .map(|(path, icon)| (path, icon.to_string().map(|icon| intern(&icon))))
            .collect()
    });
    SPECIAL
        .iter()
        .find(|(special, _)| special == path)
        .and_then(|(_, icon)| icon.clone())
}
/// Reads the start of a file to find its type, as a listing without
/// `defer_content_test` used to for every file. Blocking.
pub fn sniff(file: &gio::File, cancel: Option<&gio::Cancellable>) -> Option<TypeFacts> {
    let info = file
        .query_info(
            "standard::content-type",
            gio::FileQueryInfoFlags::NONE,
            cancel,
        )
        .ok()?;
    info.attribute_string("standard::content-type")
        .map(|content_type| type_facts(&content_type))
}
/// Access capabilities reported by GIO.
#[derive(Debug, Clone, Copy, Default)]
pub struct Access {
    /// Can read.
    pub read: bool,
    /// Can write.
    pub write: bool,
    /// Can execute.
    pub execute: bool,
    /// Can delete.
    pub delete: bool,
    /// Can trash.
    pub trash: bool,
    /// Can rename.
    pub rename: bool,
}
const INTERNED_LIMIT: usize = 1024;

#[derive(Default)]
struct Strings {
    values: HashSet<Arc<str>>,
    order: VecDeque<Arc<str>>,
}

impl Strings {
    fn intern(&mut self, text: &str) -> Arc<str> {
        if let Some(shared) = self.values.get(text) {
            return shared.clone();
        }
        if self.values.len() == INTERNED_LIMIT
            && let Some(oldest) = self.order.pop_front()
        {
            self.values.remove(&oldest);
        }
        let shared: Arc<str> = Arc::from(text);
        self.values.insert(shared.clone());
        self.order.push_back(shared.clone());
        shared
    }
}

/// Returns the shared copy of a string that repeats across files.
///
/// A folder of 50,000 files has a handful of distinct content types, type
/// descriptions, icons, owners and filesystem ids. Keeping one allocation of
/// each instead of one per file is what holds listing memory inside the
/// 15 MB per 10,000 files budget. At most 1,024 recent distinct values are
/// retained: file icons may name arbitrary paths. Eviction does not affect
/// metadata that still holds a shared value.
pub fn intern(text: &str) -> Arc<str> {
    static POOL: LazyLock<Mutex<Strings>> = LazyLock::new(Mutex::default);
    POOL.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .intern(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_strings_are_bounded_and_eviction_keeps_live_metadata_valid() {
        let mut pool = Strings::default();
        let first = pool.intern("first");
        assert!(Arc::ptr_eq(&first, &pool.intern("first")));
        for n in 0..INTERNED_LIMIT * 2 {
            pool.intern(&format!("file:///unique-icon-{n}"));
        }
        assert_eq!(pool.values.len(), INTERNED_LIMIT);
        assert_eq!(pool.order.len(), INTERNED_LIMIT);
        assert_eq!(&*first, "first");
        assert_eq!(Arc::strong_count(&first), 1);
    }
}
/// Complete listing metadata; raw names are retained for operations.
#[derive(Debug, Clone)]
pub struct FileMeta {
    /// Raw basename, potentially non-UTF-8.
    pub name: PathBuf,
    /// Display text.
    pub display_name: String,
    /// Rename dialog text.
    pub edit_name: String,
    /// URI for operations.
    pub uri: String,
    /// GIO type.
    pub file_type: gio::FileType,
    /// Hidden including backup preference.
    pub hidden: bool,
    /// Symlink even if broken.
    pub symlink: bool,
    /// Raw symlink target.
    pub symlink_target: Option<PathBuf>,
    /// MIME type. Shared between files, see [`intern`].
    pub content_type: Arc<str>,
    /// The type is what the name suggests and the content has not been
    /// read yet: a listing tests it afterwards, see `FolderEvent::Typed`.
    pub type_uncertain: bool,
    /// MIME description. Shared between files, see [`intern`].
    pub description: Arc<str>,
    /// Serialized GIO icon. Shared between files, see [`intern`].
    pub icon: Option<Arc<str>>,
    /// Logical bytes.
    pub size: u64,
    /// Allocated bytes.
    pub allocated_size: u64,
    /// Modification seconds.
    pub mtime: i64,
    /// Access seconds.
    pub atime: i64,
    /// Creation ("birth") seconds; 0 when the filesystem records none.
    pub btime: i64,
    /// Unix permission bits.
    pub mode: u32,
    /// Unix owner id.
    pub uid: u32,
    /// Unix group id.
    pub gid: u32,
    /// Owner name. Shared between files, see [`intern`].
    pub owner: Arc<str>,
    /// Group name. Shared between files, see [`intern`].
    pub group: Arc<str>,
    /// Access capabilities.
    pub access: Access,
    /// Shortcut target.
    pub target_uri: Option<String>,
    /// Filesystem identity. Shared between files, see [`intern`].
    pub filesystem_id: Option<Arc<str>>,
    /// Filesystem type (queried once for folder, inherited by children). Shared.
    pub filesystem_type: Option<Arc<str>>,
    /// Trash restore path.
    pub trash_orig_path: Option<PathBuf>,
    /// Trash deletion timestamp.
    pub trash_deletion_date: Option<String>,
    /// Locale-aware filename key.
    pub collation: glib::FilenameCollationKey,
    /// Case-folded filename key.
    pub folded_collation: glib::FilenameCollationKey,
}
impl FileMeta {
    /// Convert queried GIO metadata. `file` must refer to this item, not its parent.
    pub fn from_info(
        file: &gio::File,
        info: &gio::FileInfo,
        backup_as_hidden: bool,
        filesystem_type: Option<&str>,
    ) -> Self {
        // No attribute is taken for granted: GVFS backends differ in what
        // they report, and the typed getters abort or log a critical when
        // their attribute is absent.
        let text = |attribute: &str| info.attribute_string(attribute).map(|s| s.to_string());
        let flag = |attribute: &str| info.has_attribute(attribute) && info.boolean(attribute);
        // An access right the backend does not report is assumed, so that
        // the operation is offered and the backend gives the real answer.
        let allowed = |attribute: &str| !info.has_attribute(attribute) || info.boolean(attribute);
        let name = if info.has_attribute("standard::name") {
            info.name()
        } else {
            file.basename().unwrap_or_default()
        };
        let display_name =
            text("standard::display-name").unwrap_or_else(|| name.to_string_lossy().into_owned());
        let size = if info.has_attribute("standard::size") {
            info.size().max(0) as u64
        } else {
            0
        };
        let file_type = if info.has_attribute("standard::type") {
            info.file_type()
        } else {
            gio::FileType::Unknown
        };
        let tested = info.attribute_string("standard::content-type");
        // GIO reads a file only when its name leaves the type open, and
        // never an empty one; the same rule says which files a listing by
        // name still has to test.
        let type_uncertain = tested.is_none()
            && file_type == gio::FileType::Regular
            && size > 0
            && gio::content_type_guess(Some(&name), None).1;
        let facts = tested
            .or_else(|| info.attribute_string("standard::fast-content-type"))
            .map_or_else(
                || type_facts("application/octet-stream"),
                |content_type| type_facts(&content_type),
            );
        let icon = if info.has_attribute("standard::icon") {
            info.icon()
                .and_then(|icon| icon.to_string())
                .map(|icon| intern(&icon))
        } else if file.is_native() {
            // A listing does not ask for it, see `listing_attributes`.
            file.path()
                .filter(|_| file_type == gio::FileType::Directory)
                .and_then(|path| special_folder_icon(&path))
                .or_else(|| facts.icon.clone())
        } else {
            None
        };
        let mtime = info.attribute_uint64("time::modified") as i64;
        let mode = info.attribute_uint32("unix::mode");
        let owner = intern(&info.attribute_string("owner::user").unwrap_or_default());
        let group = intern(&info.attribute_string("owner::group").unwrap_or_default());
        Self {
            name,
            edit_name: text("standard::edit-name").unwrap_or_else(|| display_name.clone()),
            uri: file.uri().to_string(),
            file_type,
            hidden: flag("standard::is-hidden")
                || (backup_as_hidden && flag("standard::is-backup")),
            symlink: flag("standard::is-symlink"),
            symlink_target: info
                .has_attribute("standard::symlink-target")
                .then(|| info.symlink_target())
                .flatten(),
            content_type: facts.content_type,
            type_uncertain,
            description: facts.description,
            icon,
            size,
            allocated_size: info.attribute_uint64("standard::allocated-size"),
            mtime,
            atime: info.attribute_uint64("time::access") as i64,
            btime: info.attribute_uint64("time::created") as i64,
            mode,
            uid: info.attribute_uint32("unix::uid"),
            gid: info.attribute_uint32("unix::gid"),
            owner,
            group,
            access: Access {
                read: allowed("access::can-read"),
                write: allowed("access::can-write"),
                // Never assumed: this one decides whether a file is run.
                execute: flag("access::can-execute"),
                delete: allowed("access::can-delete"),
                trash: allowed("access::can-trash"),
                rename: allowed("access::can-rename"),
            },
            target_uri: info
                .attribute_string("standard::target-uri")
                .map(|s| s.to_string()),
            filesystem_id: info.attribute_string("id::filesystem").map(|s| intern(&s)),
            filesystem_type: filesystem_type.map(intern),
            trash_orig_path: info.attribute_file_path("trash::orig-path"),
            trash_deletion_date: info
                .attribute_string("trash::deletion-date")
                .map(|s| s.to_string()),
            collation: glib::FilenameCollationKey::from(&display_name),
            folded_collation: glib::FilenameCollationKey::from(glib::casefold(&display_name)),
            display_name,
        }
    }
    /// Synchronous query for job workers and headless tools. UI uses async folder events.
    pub fn query(
        file: &gio::File,
        backup_as_hidden: bool,
        defer_content_test: bool,
        cancel: Option<&gio::Cancellable>,
    ) -> Result<Self, glib::Error> {
        let info = file.query_info(
            &attributes(defer_content_test),
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            cancel,
        )?;
        let filesystem = file
            .query_filesystem_info("filesystem::type", cancel)
            .ok()
            .and_then(|i| i.attribute_string("filesystem::type"))
            .map(|s| s.to_string());
        Ok(Self::from_info(
            file,
            &info,
            backup_as_hidden,
            filesystem.as_deref(),
        ))
    }
}
impl FileMeta {
    /// Takes the type found by reading the file. Returns whether it differs
    /// from the type the name suggested.
    pub fn set_type(&mut self, facts: &TypeFacts) -> bool {
        self.type_uncertain = false;
        if self.content_type == facts.content_type {
            return false;
        }
        self.content_type = facts.content_type.clone();
        self.description = facts.description.clone();
        self.icon = facts.icon.clone();
        true
    }
    /// The core comparator's inputs, borrowed from this value. A symbolic
    /// link to a folder sorts with the folders.
    pub fn sort_facts(&self) -> SortFacts<'_> {
        SortFacts {
            name: &self.display_name,
            directory: self.file_type == gio::FileType::Directory,
            description: &self.description,
            size: self.size,
            modified: self.mtime,
            created: self.btime,
            mode: self.mode,
            owner: &self.owner,
            group: &self.group,
        }
    }
}
/// Compare cached GLib filename keys through the core's column/directory policy.
pub fn compare(a: &FileMeta, b: &FileMeta, s: &SortSpec) -> Ordering {
    tallboy_core::sort::compare_facts(&a.sort_facts(), &b.sort_facts(), s, || {
        if s.case_sensitive {
            a.collation.cmp(&b.collation)
        } else {
            a.folded_collation.cmp(&b.folded_collation)
        }
    })
}
