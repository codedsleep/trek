//! Freedesktop PNG metadata, validity, paths and failure markers.
use super::Error;
use std::path::{Path, PathBuf};
use trek_core::{config::Config, location::Location};
/// Freedesktop thumbnail directory sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Size {
    /// 128px.
    Normal,
    /// 256px.
    Large,
    /// 512px.
    XLarge,
    /// 1024px; requests larger than this are capped.
    XXLarge,
}
impl Size {
    /// Smallest supported size at least pixels × scale, including fractional display scales.
    /// Invalid scales fall back to 1; sizes above 1024 are capped.
    pub fn for_pixels(pixels: u32, scale: f64) -> Self {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        match (f64::from(pixels) * scale).ceil() as u64 {
            0..=128 => Self::Normal,
            129..=256 => Self::Large,
            257..=512 => Self::XLarge,
            _ => Self::XXLarge,
        }
    }
    /// Cache subdirectory.
    pub fn directory(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Large => "large",
            Self::XLarge => "x-large",
            Self::XXLarge => "xx-large",
        }
    }
    /// Maximum generated side length.
    pub fn pixels(self) -> i32 {
        match self {
            Self::Normal => 128,
            Self::Large => 256,
            Self::XLarge => 512,
            Self::XXLarge => 1024,
        }
    }
}
/// Identity metadata for a source. URIs are already escaped, not raw filenames.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Spec {
    /// Source URI.
    pub uri: String,
    /// Source mtime in seconds.
    pub mtime: i64,
    /// Source bytes.
    pub bytes: u64,
    /// Image MIME type.
    pub mimetype: String,
}
impl From<&crate::meta::FileMeta> for Spec {
    fn from(m: &crate::meta::FileMeta) -> Self {
        Self {
            uri: m.uri.clone(),
            mtime: m.mtime,
            bytes: m.size,
            mimetype: m.content_type.to_string(),
        }
    }
}
/// Cache root under an explicitly supplied XDG_CACHE_HOME.
#[derive(Debug, Clone)]
pub struct Cache {
    root: PathBuf,
}
impl Cache {
    /// Select an isolated/shared cache directory. Own cache files do not use the job guard.
    pub fn new(cache_home: &Path) -> Self {
        Self {
            root: cache_home.join("thumbnails"),
        }
    }
    /// The shared thumbnail root.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// MD5 filename, matching the freedesktop test vector.
    pub fn filename(uri: &str) -> Result<String, Error> {
        Ok(format!(
            "{}.png",
            glib::compute_checksum_for_string(glib::ChecksumType::Md5, uri)
                .ok_or(Error::Allocation)?
        ))
    }
    /// Normal cached-image path.
    pub fn path(&self, spec: &Spec, size: Size) -> Result<PathBuf, Error> {
        Ok(self
            .root
            .join(size.directory())
            .join(Self::filename(&spec.uri)?))
    }
    /// Failure marker path, namespaced by generator version.
    pub fn fail_path(&self, spec: &Spec) -> Result<PathBuf, Error> {
        Ok(self
            .root
            .join("fail")
            .join(concat!("trek-", env!("CARGO_PKG_VERSION")))
            .join(Self::filename(&spec.uri)?))
    }
    fn read_valid(&self, path: &Path, spec: &Spec) -> Option<gdk_pixbuf::Pixbuf> {
        if !path.exists() {
            return None;
        }
        match gdk_pixbuf::Pixbuf::from_file(path) {
            Ok(pixbuf) => {
                let valid = pixbuf
                    .option("tEXt::Thumb::URI")
                    .is_some_and(|s| s == spec.uri)
                    && pixbuf
                        .option("tEXt::Thumb::MTime")
                        .is_some_and(|s| s.as_str() == spec.mtime.to_string());
                valid.then_some(pixbuf)
            }
            Err(e) => {
                log::warn!("Invalid thumbnail cache {}: {e}", path.display());
                None
            }
        }
    }
    /// Valid cached pixbuf; stale or malformed files are misses.
    pub fn load(&self, spec: &Spec, size: Size) -> Result<Option<gdk_pixbuf::Pixbuf>, Error> {
        Ok(self.read_valid(&self.path(spec, size)?, spec))
    }
    /// Whether this exact URI/mtime has a failure marker.
    pub fn failed(&self, spec: &Spec) -> Result<bool, Error> {
        Ok(self.read_valid(&self.fail_path(spec)?, spec).is_some())
    }
    fn write_at(&self, path: &Path, spec: &Spec, pixbuf: &gdk_pixbuf::Pixbuf) -> Result<(), Error> {
        let mtime = spec.mtime.to_string();
        let bytes = spec.bytes.to_string();
        let png = pixbuf.save_to_bufferv(
            "png",
            &[
                ("tEXt::Thumb::URI", &spec.uri),
                ("tEXt::Thumb::MTime", &mtime),
                ("tEXt::Thumb::Size", &bytes),
                ("tEXt::Thumb::Mimetype", &spec.mimetype),
            ],
        )?;
        crate::atomic::write(path, &png)?;
        Ok(())
    }
    /// Write an interoperable PNG, private mode 0600, with all four Thumb:: chunks.
    pub fn save(&self, spec: &Spec, size: Size, pixbuf: &gdk_pixbuf::Pixbuf) -> Result<(), Error> {
        self.write_at(&self.path(spec, size)?, spec, pixbuf)
    }
    /// Record generation failure. A changed mtime invalidates the marker automatically.
    pub fn record_failure(&self, spec: &Spec) -> Result<(), Error> {
        let pixbuf = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 1, 1)
            .ok_or(Error::Allocation)?;
        pixbuf.fill(0);
        self.write_at(&self.fail_path(spec)?, spec, &pixbuf)
    }
}
/// Skip remote sources when local-only, oversize images, cache contents and non-images.
pub fn skip(cache: &Cache, spec: &Spec, config: &Config) -> bool {
    let location = Location::parse(&spec.uri, Path::new("/"), Path::new("/"));
    let Ok(location) = location else {
        return true;
    };
    !spec.mimetype.starts_with("image/")
        || (config.thumbnail_local && location.path().is_none())
        || (config.thumbnail_max > 0 && spec.bytes > u64::from(config.thumbnail_max) * 1024)
        || location.path().is_some_and(|p| p.starts_with(cache.root()))
}
