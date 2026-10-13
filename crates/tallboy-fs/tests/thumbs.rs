mod common;
use common::*;
use std::{fs, os::unix::fs::PermissionsExt};
use tallboy_core::config::Config;
use tallboy_fs::thumbs::*;
fn spec() -> Spec {
    Spec {
        uri: "file:///home/jens/photos/me.png".into(),
        mtime: 123,
        bytes: 456,
        mimetype: "image/png".into(),
    }
}
#[test]
fn cache_vector_sizes_chunks_validity_fail_and_skip() {
    let t = temp();
    let cache = Cache::new(t.path());
    let s = spec();
    assert_eq!(
        Cache::filename(&s.uri).unwrap(),
        "c6ee772d9e49320e97ec29a7eb5b1697.png"
    );
    for (pixels, scale, size) in [
        (48, 1.0, Size::Normal),
        (128, 2.0, Size::Large),
        (128, 1.5, Size::Large),
        (256, 1.5, Size::XLarge),
        (256, 2.0, Size::XLarge),
        (512, 2.0, Size::XXLarge),
        (2048, 2.0, Size::XXLarge),
    ] {
        assert_eq!(Size::for_pixels(pixels, scale), size);
    }
    let p = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, true, 8, 16, 12).unwrap();
    p.fill(0xff0000ff);
    cache.save(&s, Size::Normal, &p).unwrap();
    let path = cache.path(&s, Size::Normal).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let p = gdk_pixbuf::Pixbuf::from_file(path).unwrap();
    for (key, value) in [
        ("URI", s.uri.clone()),
        ("MTime", "123".into()),
        ("Size", "456".into()),
        ("Mimetype", "image/png".into()),
    ] {
        assert_eq!(
            p.option(&format!("tEXt::Thumb::{key}")).unwrap().as_str(),
            value
        );
    }
    assert!(cache.load(&s, Size::Normal).unwrap().is_some());
    let mut changed = s.clone();
    changed.mtime += 1;
    assert!(cache.load(&changed, Size::Normal).unwrap().is_none());
    cache.record_failure(&s).unwrap();
    assert!(cache.failed(&s).unwrap());
    assert!(!cache.failed(&changed).unwrap());
    let mut c = Config::default();
    assert!(!skip(&cache, &s, &c));
    let mut remote = s.clone();
    remote.uri = "sftp://host/image.png".into();
    assert!(skip(&cache, &remote, &c));
    c.thumbnail_local = false;
    assert!(!skip(&cache, &remote, &c));
    let mut large = s.clone();
    large.bytes = 3 * 1024 * 1024;
    assert!(skip(&cache, &large, &c));
    c.thumbnail_max = 0;
    assert!(!skip(&cache, &large, &c));
    let mut cached = s.clone();
    cached.uri = uri(cache.path(&s, Size::Normal).unwrap());
    assert!(skip(&cache, &cached, &c));
}
#[test]
fn five_hundred_fixture_images_complete_and_reuse_cache() {
    let t = temp();
    let cache = Cache::new(t.path());
    let pool = Pool::new(cache.clone()).unwrap();
    assert!((1..=4).contains(&pool.worker_count()));
    let fixture = glib::user_cache_dir().join("tallboy-fixture/images");
    let mut requests = vec![];
    for n in 0..500 {
        let image = fixture.join(format!("image-{n:03}.png"));
        assert!(image.exists(), "run scripts/mkfixture.sh first");
        let spec = Spec {
            uri: uri(&image),
            mtime: 123,
            bytes: fs::metadata(image).unwrap().len(),
            mimetype: "image/png".into(),
        };
        requests.push(pool.request(spec, 128, 1.0, &Config::default()));
    }
    for request in requests {
        let decoded = request.result.recv_blocking().unwrap().unwrap();
        assert!(decoded.width <= 128 && decoded.height <= 128);
        assert!(decoded.width > 0);
        assert!(!decoded.pixels.is_empty());
    }
    assert_eq!(
        fs::read_dir(cache.root().join("normal")).unwrap().count(),
        500
    );
    let spec = Spec {
        uri: uri(fixture.join("image-000.png")),
        mtime: 123,
        bytes: 456,
        mimetype: "image/png".into(),
    };
    let first = pool
        .request(spec.clone(), 128, 1.0, &Config::default())
        .result
        .recv_blocking()
        .unwrap()
        .unwrap();
    let next = pool
        .request(spec, 128, 1.0, &Config::default())
        .result
        .recv_blocking()
        .unwrap()
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &next));
}
