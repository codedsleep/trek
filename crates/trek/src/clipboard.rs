//! Cut, copy and paste of files, in the formats other file managers use
//! (PROMPT §8.11). The codecs are `core::transfer`; this module only moves
//! their bytes on and off the GDK clipboard.

use gtk::{gdk, gio, glib, prelude::*};
use trek_core::{
    location::Location,
    transfer::{Intent, Payload},
};

/// Nautilus, Thunar, Nemo, PCManFM: `cut` or `copy`, then one URI per line.
pub const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";
pub const URI_LIST: &str = "text/uri-list";
/// Dolphin: `1` when the URI list was cut.
pub const KDE_CUT_SELECTION: &str = "application/x-kde-cutselection";

fn clipboard() -> Option<gdk::Clipboard> {
    gdk::Display::default().map(|display| display.clipboard())
}

fn bytes(text: String) -> glib::Bytes {
    glib::Bytes::from_owned(text.into_bytes())
}

/// Puts files on the clipboard as one union provider offering every format.
pub fn set_files(files: Vec<Location>, cut: bool) {
    let Some(clipboard) = clipboard() else { return };
    let list: Vec<gio::File> = files
        .iter()
        .map(|location| gio::File::for_uri(location.to_uri()))
        .collect();
    crate::view::item::set_yanked(Some(crate::view::item::Yanked {
        uris: files.iter().map(|file| file.to_uri().to_owned()).collect(),
        cut,
    }));
    let payload = Payload {
        intent: if cut { Intent::Move } else { Intent::Copy },
        files,
    };
    let mut providers = vec![
        gdk::ContentProvider::for_bytes(GNOME_COPIED_FILES, &bytes(payload.gnome())),
        gdk::ContentProvider::for_bytes(URI_LIST, &bytes(payload.uri_list())),
        gdk::ContentProvider::for_value(&gdk::FileList::from_array(&list).to_value()),
        gdk::ContentProvider::for_value(&payload.plain().to_value()),
    ];
    if cut {
        providers.push(gdk::ContentProvider::for_bytes(
            KDE_CUT_SELECTION,
            &bytes(payload.kde_cut().to_owned()),
        ));
    }
    if let Err(e) = clipboard.set_content(Some(&gdk::ContentProvider::new_union(&providers))) {
        log::warn!("Clipboard: {e}");
    }
}

/// Copy Path(s): plain text only.
pub fn set_text(text: &str) {
    if let Some(clipboard) = clipboard() {
        crate::view::item::set_yanked(None);
        clipboard.set_text(text);
    }
}

/// Whether the clipboard offers files in a format `read` understands.
pub fn has_files() -> bool {
    clipboard().is_some_and(|clipboard| {
        let formats = clipboard.formats();
        formats.contain_mime_type(GNOME_COPIED_FILES)
            || formats.contain_mime_type(URI_LIST)
            || formats.contains_type(gdk::FileList::static_type())
    })
}

/// The MIME types currently offered, for agent checks.
pub fn offered_mime_types() -> Vec<String> {
    clipboard()
        .map(|clipboard| {
            clipboard
                .formats()
                .mime_types()
                .iter()
                .map(|mime| mime.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Empties the clipboard after a cut was pasted, when the cut was ours.
pub fn clear_own() {
    crate::view::item::set_yanked(None);
    if let Some(clipboard) = clipboard()
        && clipboard.is_local()
        && let Err(e) = clipboard.set_content(gdk::ContentProvider::NONE)
    {
        log::warn!("Clipboard: {e}");
    }
}

async fn read_text(clipboard: &gdk::Clipboard, mime: &str) -> Option<String> {
    let (stream, _) = clipboard
        .read_future(&[mime], glib::Priority::DEFAULT)
        .await
        .ok()?;
    let buffer = gio::MemoryOutputStream::new_resizable();
    buffer
        .splice_future(
            &stream,
            gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
            glib::Priority::DEFAULT,
        )
        .await
        .ok()?;
    let text = String::from_utf8(buffer.steal_as_bytes().to_vec()).ok()?;
    Some(text.trim_end_matches('\0').to_owned())
}

async fn read_payload(clipboard: &gdk::Clipboard) -> Option<Payload> {
    let formats = clipboard.formats();
    // The GNOME format says cut or copy, so it is preferred.
    if formats.contain_mime_type(GNOME_COPIED_FILES) {
        let text = read_text(clipboard, GNOME_COPIED_FILES).await?;
        return Payload::from_gnome(&text).ok();
    }
    if formats.contain_mime_type(URI_LIST) {
        let text = read_text(clipboard, URI_LIST).await?;
        let payload = Payload::from_uri_list(&text).ok()?;
        if formats.contain_mime_type(KDE_CUT_SELECTION)
            && let Some(marker) = read_text(clipboard, KDE_CUT_SELECTION).await
        {
            return payload.with_kde_cut(&marker).ok();
        }
        return Some(payload);
    }
    if formats.contains_type(gdk::FileList::static_type()) {
        let value = clipboard
            .read_value_future(gdk::FileList::static_type(), glib::Priority::DEFAULT)
            .await
            .ok()?;
        let list = value.get::<gdk::FileList>().ok()?;
        let files = list
            .files()
            .iter()
            .filter_map(|file| crate::tab::location_from_uri(&file.uri()))
            .collect();
        return Some(Payload {
            intent: Intent::Copy,
            files,
        });
    }
    None
}

/// Reads the files on the clipboard; `done` gets `None` when there are none.
pub fn read(done: impl FnOnce(Option<Payload>) + 'static) {
    let Some(clipboard) = clipboard() else {
        done(None);
        return;
    };
    glib::spawn_future_local(async move {
        let payload = read_payload(&clipboard).await;
        done(payload.filter(|payload| !payload.files.is_empty()));
    });
}

/// Calls `changed` whenever the clipboard's contents change.
pub fn connect_changed(
    changed: impl Fn() + 'static,
) -> Option<(gdk::Clipboard, glib::SignalHandlerId)> {
    let clipboard = clipboard()?;
    let handler = clipboard.connect_changed(move |clipboard| {
        // Another program's clipboard replaced what was yanked here.
        if !clipboard.is_local() {
            crate::view::item::set_yanked(None);
        }
        changed();
    });
    Some((clipboard, handler))
}
