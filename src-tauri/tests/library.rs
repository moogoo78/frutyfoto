use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{Local, NaiveDate, TimeZone};
use exif::experimental::Writer;
use exif::{Field, In, Tag, Value};
use frutyfoto_lib::import::{self, hash_file, ImportProgress};
use frutyfoto_lib::library::Library;
use frutyfoto_lib::queries::{self, Filter, Sort};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

// ---- fixtures ----

/// A deterministic test picture; `variant` changes the structure, not just colors.
fn picture(variant: u32, w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let v = match variant {
            0 => {
                let d = ((fx - 0.3).powi(2) + (fy - 0.35).powi(2)).sqrt();
                if d < 0.2 { 20.0 } else { 60.0 + 180.0 * fx }
            }
            1 => if ((fx * 6.0) as u32) % 2 == 0 { 30.0 } else { 220.0 },
            _ => 255.0 * (1.0 - fy) * if fx > 0.5 { 1.0 } else { 0.4 },
        } as u8;
        Rgb([v, v.wrapping_add(variant as u8 * 40), 255 - v])
    })
}

fn jpeg_bytes(img: &RgbImage, quality: u8) -> Vec<u8> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(img).unwrap();
    out
}

/// Inserts an EXIF APP1 segment (DateTimeOriginal + Model) right after the JPEG SOI marker.
fn with_exif(jpeg: Vec<u8>, taken: &str, model: &str) -> Vec<u8> {
    let dto = Field { tag: Tag::DateTimeOriginal, ifd_num: In::PRIMARY, value: Value::Ascii(vec![taken.into()]) };
    let cam = Field { tag: Tag::Model, ifd_num: In::PRIMARY, value: Value::Ascii(vec![model.into()]) };
    let mut writer = Writer::new();
    writer.push_field(&dto);
    writer.push_field(&cam);
    let mut tiff = Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();

    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(&tiff);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn import_all(lib: &Library, src: &Path) -> ImportProgress {
    import::run(lib, src, None, &AtomicBool::new(false), |_| {}).unwrap()
}

struct Env {
    _tmp: tempfile::TempDir,
    src: PathBuf,
    lib: Library,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    let lib = Library::open(&tmp.path().join("lib"), true).unwrap();
    Env { _tmp: tmp, src, lib }
}

/// Empties the trash without touching the real system Trash.
fn empty_trash(lib: &Library) -> frutyfoto_lib::error::Result<usize> {
    queries::empty_trash_with(lib, |p| Ok(std::fs::remove_file(p)?))
}

// ---- tests ----

#[test]
fn import_organises_by_date_and_skips_duplicates() {
    let Env { _tmp, src, lib } = env();

    let a = with_exif(jpeg_bytes(&picture(0, 320, 240), 90), "2021:07:15 10:20:30", "TestCam X1");
    write(&src.join("a.jpg"), &a);
    write(&src.join("copies/a.jpg"), &a); // identical bytes -> duplicate
    let other = with_exif(jpeg_bytes(&picture(1, 320, 240), 90), "2021:07:15 18:00:00", "TestCam X1");
    write(&src.join("other/a.jpg"), &other); // same name, same day -> a-1.jpg
    write(&src.join("notes.txt"), b"not a photo");

    // A PNG without EXIF falls back to the file's mtime.
    let png = src.join("scans/b.png");
    std::fs::create_dir_all(png.parent().unwrap()).unwrap();
    DynamicImage::ImageRgb8(picture(2, 200, 300)).save_with_format(&png, ImageFormat::Png).unwrap();
    let mtime = Local.from_local_datetime(&NaiveDate::from_ymd_opt(2019, 3, 4).unwrap().and_hms_opt(12, 0, 0).unwrap()).unwrap();
    std::fs::File::options().write(true).open(&png).unwrap().set_modified(mtime.into()).unwrap();

    let source_hash = hash_file(&src.join("a.jpg")).unwrap();

    let p = import_all(&lib, &src);
    assert_eq!((p.total, p.added, p.skipped, p.failed), (4, 3, 1, 0), "errors: {:?}", p.errors);

    let day = lib.root.join("originals/2021/07/15");
    assert!(day.join("a.jpg").is_file());
    assert!(day.join("a-1.jpg").is_file());
    assert!(lib.root.join("originals/2019/03/04/b.png").is_file());

    // Sources are untouched.
    assert_eq!(hash_file(&src.join("a.jpg")).unwrap(), source_hash);
    assert!(src.join("copies/a.jpg").is_file());

    // Metadata and thumbnails.
    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 100).unwrap();
    assert_eq!(photos.len(), 3);
    let first = &photos[0];
    assert_eq!(first.taken_at, "2021-07-15T18:00:00");
    assert_eq!(first.camera_model.as_deref(), Some("TestCam X1"));
    assert!(Path::new(&first.thumb).is_file());
    let png_row = photos.iter().find(|p| p.orig_name == "b.png").unwrap();
    assert_eq!((png_row.width, png_row.height), (Some(200), Some(300)));

    let buckets = queries::date_buckets(&lib, &Filter::default(), Sort::TakenDesc).unwrap();
    let days: Vec<_> = buckets.iter().map(|b| (b.day.as_str(), b.count)).collect();
    assert_eq!(days, vec![("2021-07-15", 2), ("2019-03-04", 1)]);

    // Re-importing the same folder adds nothing.
    let again = import_all(&lib, &src);
    assert_eq!((again.added, again.skipped, again.failed), (0, 4, 0));
}

#[test]
fn refuses_to_import_the_library_itself() {
    let Env { _tmp, lib, .. } = env();
    let err = import::run(&lib, &lib.root.join("originals"), None, &AtomicBool::new(false), |_| {});
    assert!(err.is_err());
}

#[test]
fn near_duplicates_are_grouped() {
    let Env { _tmp, src, lib } = env();
    let base = picture(0, 800, 600);
    write(&src.join("orig.jpg"), &jpeg_bytes(&base, 92));
    let small = DynamicImage::ImageRgb8(base).resize_exact(400, 300, image::imageops::FilterType::Triangle);
    write(&src.join("small.jpg"), &jpeg_bytes(&small.to_rgb8(), 60));
    write(&src.join("different.jpg"), &jpeg_bytes(&picture(1, 800, 600), 92));
    write(&src.join("different2.jpg"), &jpeg_bytes(&picture(2, 800, 600), 92));
    assert_eq!(import_all(&lib, &src).added, 4);

    let groups = queries::find_duplicates(&lib, 6).unwrap();
    assert_eq!(groups.len(), 1, "groups: {groups:?}");
    let mut names: Vec<_> = groups[0].iter().map(|p| p.orig_name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["orig.jpg", "small.jpg"]);
    // Largest image first.
    assert_eq!(groups[0][0].orig_name, "orig.jpg");
}

#[test]
fn tags_albums_ratings_and_filters() {
    let Env { _tmp, src, lib } = env();
    for (i, day) in ["2020:01:01", "2020:06:15", "2021:12:31"].iter().enumerate() {
        let bytes = with_exif(jpeg_bytes(&picture(i as u32, 64, 64), 90), &format!("{day} 08:00:00"), "Cam");
        write(&src.join(format!("p{i}.jpg")), &bytes);
    }
    import_all(&lib, &src);
    let all = queries::list_photos(&lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap();
    let ids: Vec<i64> = all.iter().map(|p| p.id).collect();

    queries::set_rating(&lib, &ids[1..], 4).unwrap();
    queries::set_favorite(&lib, &[ids[0]], true).unwrap();
    queries::add_tags(&lib, &ids[..2], &["beach".into(), " family ".into()]).unwrap();
    queries::add_tags(&lib, &[ids[2]], &["Beach".into()]).unwrap(); // case-insensitive reuse

    let tags = queries::list_tags(&lib).unwrap();
    let names: Vec<_> = tags.iter().map(|t| (t.name.as_str(), t.count)).collect();
    assert_eq!(names, vec![("beach", 3), ("family", 2)]);
    let beach = tags[0].id;
    let family = tags[1].id;

    let find = |f: Filter| -> Vec<i64> {
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
    };
    assert_eq!(find(Filter { tag_ids: vec![beach, family], ..Default::default() }), ids[..2]);
    assert_eq!(find(Filter { min_rating: Some(4), ..Default::default() }), ids[1..]);
    assert_eq!(find(Filter { favorite: Some(true), ..Default::default() }), ids[..1]);
    assert_eq!(
        find(Filter { date_from: Some("2020-06-15".into()), date_to: Some("2020-06-15".into()), ..Default::default() }),
        ids[1..2]
    );
    assert_eq!(find(Filter { text: Some("p2".into()), ..Default::default() }), ids[2..]);
    assert_eq!(find(Filter { camera: Some("Cam".into()), ..Default::default() }).len(), 3);

    let album = queries::create_album(&lib, "Trip").unwrap();
    queries::add_to_album(&lib, album, &[ids[2], ids[0]]).unwrap();
    queries::add_to_album(&lib, album, &[ids[0]]).unwrap(); // no double insert
    assert_eq!(find(Filter { album_id: Some(album), ..Default::default() }), vec![ids[0], ids[2]]);
    let albums = queries::list_albums(&lib).unwrap();
    assert_eq!((albums[0].count, albums[0].cover.is_some()), (2, true));

    let detail = queries::get_photo(&lib, ids[0]).unwrap();
    assert_eq!(detail.tags.len(), 2);
    assert_eq!(detail.albums.len(), 1);

    queries::remove_tag(&lib, &ids, family).unwrap();
    assert_eq!(queries::list_tags(&lib).unwrap().len(), 1, "unused tag is dropped");
    queries::remove_from_album(&lib, album, &[ids[2]]).unwrap();
    assert_eq!(find(Filter { album_id: Some(album), ..Default::default() }), ids[..1]);
    assert!(queries::set_rating(&lib, &ids, 6).is_err());
}

#[test]
fn trash_restore_and_empty() {
    let Env { _tmp, src, lib } = env();
    write(&src.join("x.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    write(&src.join("y.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    import_all(&lib, &src);
    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let x = photos.iter().find(|p| p.orig_name == "x.jpg").unwrap().clone();

    queries::trash_photos(&lib, &[x.id]).unwrap();
    assert!(!Path::new(&x.path).exists());
    let trashed = queries::list_photos(&lib, &Filter { trashed: true, ..Default::default() }, Sort::TakenDesc, 0, 10).unwrap();
    assert_eq!(trashed.len(), 1);
    assert!(Path::new(&trashed[0].path).is_file());
    assert_eq!(queries::counts(&lib).unwrap().trash, 1);

    queries::restore_photos(&lib, &[x.id]).unwrap();
    assert!(Path::new(&x.path).is_file());

    queries::trash_photos(&lib, &[x.id]).unwrap();
    assert_eq!(empty_trash(&lib).unwrap(), 1);
    assert!(!Path::new(&x.thumb).exists());
    let c = queries::counts(&lib).unwrap();
    assert_eq!((c.all, c.trash), (1, 0));

    // A deleted photo can be imported again.
    assert_eq!(import_all(&lib, &src).added, 1);
}

#[test]
fn reopening_requires_existing_library() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(Library::open(tmp.path(), false).is_err());
    Library::open(tmp.path(), true).unwrap();
    Library::open(tmp.path(), false).unwrap();
}

#[test]
fn opens_library_with_legacy_meta_dir() {
    let tmp = tempfile::tempdir().unwrap();
    Library::open(tmp.path(), true).unwrap();
    std::fs::rename(tmp.path().join(".frutyfoto"), tmp.path().join(".fotolake")).unwrap();
    Library::open(tmp.path(), false).unwrap();
    assert!(tmp.path().join(".frutyfoto/library.db").exists());
    assert!(!tmp.path().join(".fotolake").exists());
}

#[test]
fn imports_heic_with_exif_thumbnail_and_preview() {
    let Env { _tmp, src, lib } = env();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.heic");
    std::fs::copy(&fixture, src.join("IMG_0001.HEIC")).unwrap();
    // Same picture as a JPEG must not affect the HEIC row.
    write(&src.join("other.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));

    let p = import_all(&lib, &src);
    assert_eq!((p.added, p.failed), (2, 0), "errors: {:?}", p.errors);

    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let heic = photos.iter().find(|p| p.orig_name == "IMG_0001.HEIC").unwrap();
    assert_eq!(heic.taken_at, "2022-05-06T07:08:09", "EXIF date read from HEIC");
    assert_eq!(heic.camera_model.as_deref(), Some("HeicCam 9"));
    assert_eq!((heic.width, heic.height), (Some(320), Some(240)));
    assert_eq!(heic.mime.as_deref(), Some("image/heic"));
    assert!(lib.root.join("originals/2022/05/06/IMG_0001.HEIC").is_file());

    // Viewer gets a decodable JPEG preview; thumbnail exists too.
    assert!(heic.display.ends_with(".jpg") && heic.display != heic.path);
    let preview = image::open(&heic.display).unwrap();
    assert_eq!((preview.width(), preview.height()), (320, 240));
    assert!(Path::new(&heic.thumb).is_file());

    // Non-HEIC photos are displayed from the original file.
    let jpg = photos.iter().find(|p| p.orig_name == "other.jpg").unwrap();
    assert_eq!(jpg.display, jpg.path);

    // Emptying trash removes the preview as well.
    queries::trash_photos(&lib, &[heic.id]).unwrap();
    empty_trash(&lib).unwrap();
    assert!(!Path::new(&heic.display).exists());
}

#[test]
fn tiff_gets_preview_and_backfill_restores_missing_ones() {
    let Env { _tmp, src, lib } = env();
    let tif = src.join("scan.tif");
    DynamicImage::ImageRgb8(picture(2, 300, 200)).save_with_format(&tif, ImageFormat::Tiff).unwrap();
    write(&src.join("plain.png"), &{
        let mut v = Vec::new();
        DynamicImage::ImageRgb8(picture(0, 64, 64)).write_to(&mut Cursor::new(&mut v), ImageFormat::Png).unwrap();
        v
    });
    assert_eq!(import_all(&lib, &src).added, 2);

    let photos = queries::list_photos(&lib, &Filter::default(), Sort::TakenDesc, 0, 10).unwrap();
    let t = photos.iter().find(|p| p.orig_name == "scan.tif").unwrap();
    assert_eq!(t.mime.as_deref(), Some("image/tiff"));
    assert!(t.display.ends_with(".jpg") && t.display != t.path);
    let preview = image::open(&t.display).unwrap();
    assert_eq!((preview.width(), preview.height()), (300, 200));

    // A library imported before TIFF previews existed has none; backfill makes them.
    std::fs::remove_file(&t.display).unwrap();
    assert_eq!(import::backfill_previews(&lib).unwrap(), 1);
    assert!(Path::new(&t.display).is_file());
    assert_eq!(import::backfill_previews(&lib).unwrap(), 0, "nothing left to do");
}

#[test]
fn import_history_records_each_run() {
    let Env { _tmp, src, lib } = env();
    let (first, second) = (src.join("first"), src.join("second"));
    write(&first.join("a.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90));
    write(&first.join("b.jpg"), &jpeg_bytes(&picture(1, 64, 64), 90));
    write(&first.join("broken.jpg"), b"not really a jpeg");
    write(&second.join("c.jpg"), &jpeg_bytes(&picture(2, 64, 64), 90));
    write(&second.join("a-again.jpg"), &jpeg_bytes(&picture(0, 64, 64), 90)); // dup of a.jpg

    import_all(&lib, &first);
    import_all(&lib, &second);

    let history = queries::list_imports(&lib).unwrap();
    assert_eq!(history.len(), 2);
    let (newest, oldest) = (&history[0], &history[1]);
    assert_eq!(newest.source_dir, second.to_string_lossy());
    assert_eq!((newest.added, newest.skipped_dupes, newest.failed, newest.photo_count), (1, 1, 0, 1));
    assert_eq!((oldest.added, oldest.skipped_dupes, oldest.failed, oldest.photo_count), (2, 0, 1, 2));
    assert!(oldest.finished_at.is_some() && !oldest.cancelled);
    assert!(oldest.cover.is_some());

    // Failed files are kept with the import.
    let errors = queries::import_errors(&lib, oldest.id).unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].path.ends_with("broken.jpg"));

    // Photos can be filtered by the import that added them.
    let names = |id| -> Vec<String> {
        let f = Filter { import_id: Some(id), ..Default::default() };
        let mut v: Vec<_> = queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().into_iter().map(|p| p.orig_name).collect();
        v.sort();
        v
    };
    assert_eq!(names(oldest.id), vec!["a.jpg", "b.jpg"]);
    assert_eq!(names(newest.id), vec!["c.jpg"]);

    // Trashed photos no longer count toward an import.
    let c = queries::list_photos(&lib, &Filter { import_id: Some(newest.id), ..Default::default() }, Sort::TakenAsc, 0, 1).unwrap();
    queries::trash_photos(&lib, &[c[0].id]).unwrap();
    assert_eq!(queries::list_imports(&lib).unwrap()[0].photo_count, 0);
}

#[test]
fn migration_links_existing_photos_to_their_import() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".frutyfoto")).unwrap();
    {
        // A library created before import history existed (schema v1).
        let conn = rusqlite::Connection::open(tmp.path().join(".frutyfoto/library.db")).unwrap();
        conn.execute_batch(frutyfoto_lib::db::MIGRATIONS[0]).unwrap();
        conn.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO imports (id, source_dir, started_at, finished_at, added)
               VALUES (1, '/a', '2026-01-01T10:00:00', '2026-01-01T10:05:00', 1),
                      (2, '/b', '2026-02-01T09:00:00', '2026-02-01T09:00:30', 1);
             INSERT INTO photos (hash, rel_path, orig_name, taken_at, imported_at, file_size)
               VALUES ('h1', 'originals/x.jpg', 'x.jpg', '2020-01-01T00:00:00', '2026-01-01T10:02:00', 1),
                      ('h2', 'originals/y.jpg', 'y.jpg', '2020-01-01T00:00:00', '2026-02-01T09:00:30', 1);",
        )
        .unwrap();
    }
    let lib = Library::open(tmp.path(), false).unwrap();
    let import_of = |hash: &str| -> Option<i64> {
        lib.conn().query_row("SELECT import_id FROM photos WHERE hash = ?1", [hash], |r| r.get(0)).unwrap()
    };
    assert_eq!(import_of("h1"), Some(1));
    assert_eq!(import_of("h2"), Some(2));
}

// ---- album folders, marks, sources ----

fn rel_of(lib: &Library, id: i64) -> String {
    queries::get_photo(lib, id).unwrap().photo.rel_path
}

/// Imports three photos taken on 2021-07-15 and returns their ids (oldest first).
fn three_photos(lib: &Library, src: &Path) -> Vec<i64> {
    for i in 0..3 {
        let bytes = with_exif(jpeg_bytes(&picture(i, 64, 64), 90), &format!("2021:07:15 0{i}:00:00"), "Cam");
        write(&src.join(format!("p{i}.jpg")), &bytes);
    }
    import_all(lib, src);
    queries::list_photos(lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
}

#[test]
fn album_photos_are_stored_in_album_folders() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let trip = queries::create_album(&lib, "Trip: Kyoto/Osaka").unwrap();
    let best = queries::create_album(&lib, "Best").unwrap();

    queries::add_to_album(&lib, trip, &ids[..2]).unwrap();
    assert_eq!(rel_of(&lib, ids[0]), "albums/Trip_ Kyoto_Osaka/p0.jpg");
    assert!(lib.root.join("albums/Trip_ Kyoto_Osaka/p1.jpg").is_file());
    assert!(!lib.root.join("originals/2021/07/15/p0.jpg").exists());
    assert_eq!(rel_of(&lib, ids[2]), "originals/2021/07/15/p2.jpg", "not in an album");

    // A second album only links; the file stays in its first album's folder.
    queries::add_to_album(&lib, best, &ids[..1]).unwrap();
    assert_eq!(rel_of(&lib, ids[0]), "albums/Trip_ Kyoto_Osaka/p0.jpg");

    // Renaming the album renames its folder.
    queries::rename_album(&lib, trip, "Japan").unwrap();
    assert_eq!(rel_of(&lib, ids[0]), "albums/Japan/p0.jpg");
    assert!(lib.root.join("albums/Japan/p1.jpg").is_file());
    assert!(!lib.root.join("albums/Trip_ Kyoto_Osaka").exists());

    // Trashed album photos keep their place inside the trash and follow renames too.
    queries::trash_photos(&lib, &[ids[1]]).unwrap();
    assert!(lib.trash_dir().join("albums/Japan/p1.jpg").is_file());
    queries::rename_album(&lib, trip, "Japan 2021").unwrap();
    assert!(lib.trash_dir().join("albums/Japan 2021/p1.jpg").is_file());
    queries::restore_photos(&lib, &[ids[1]]).unwrap();
    assert!(lib.root.join("albums/Japan 2021/p1.jpg").is_file());

    // Leaving the home album moves the file to the next album it's in...
    queries::remove_from_album(&lib, trip, &[ids[0]]).unwrap();
    assert_eq!(rel_of(&lib, ids[0]), "albums/Best/p0.jpg");
    // ...or back to originals by date.
    queries::delete_album(&lib, best).unwrap();
    assert_eq!(rel_of(&lib, ids[0]), "originals/2021/07/15/p0.jpg");
    assert!(!lib.root.join("albums/Best").exists(), "empty album folder removed");
    queries::delete_album(&lib, trip).unwrap();
    assert_eq!(rel_of(&lib, ids[1]), "originals/2021/07/15/p1.jpg");

    for id in &ids {
        let p = queries::get_photo(&lib, *id).unwrap().photo;
        assert!(Path::new(&p.path).is_file(), "{} exists", p.path);
    }
}

#[test]
fn same_named_files_get_unique_names_in_an_album() {
    let Env { _tmp, src, lib } = env();
    write(&src.join("a/IMG_1.jpg"), &with_exif(jpeg_bytes(&picture(0, 64, 64), 90), "2021:01:01 00:00:00", "Cam"));
    write(&src.join("b/IMG_1.jpg"), &with_exif(jpeg_bytes(&picture(1, 64, 64), 90), "2022:01:01 00:00:00", "Cam"));
    import_all(&lib, &src);
    let ids: Vec<i64> =
        queries::list_photos(&lib, &Filter::default(), Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect();
    let album = queries::create_album(&lib, "Mix").unwrap();
    queries::add_to_album(&lib, album, &ids).unwrap();
    let mut rels: Vec<_> = ids.iter().map(|id| rel_of(&lib, *id)).collect();
    rels.sort();
    assert_eq!(rels, vec!["albums/Mix/IMG_1-1.jpg", "albums/Mix/IMG_1.jpg"]);
}

#[test]
fn existing_album_photos_are_moved_into_folders_on_open() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let album = queries::create_album(&lib, "Old").unwrap();
    // As an older version would have left it: linked, but the file still in originals.
    lib.conn().execute("INSERT INTO album_photos (album_id, photo_id) VALUES (?1, ?2)", [album, ids[0]]).unwrap();
    assert_eq!(frutyfoto_lib::storage::rehome_all(&lib).unwrap(), 1);
    assert_eq!(rel_of(&lib, ids[0]), "albums/Old/p0.jpg");
    assert!(lib.root.join("albums/Old/p0.jpg").is_file());
    assert_eq!(frutyfoto_lib::storage::rehome_all(&lib).unwrap(), 0);
}

#[test]
fn marks_are_set_filtered_and_run() {
    use frutyfoto_lib::marks::{self, ActionKind};
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let find = |f: Filter| -> Vec<i64> {
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().iter().map(|p| p.id).collect()
    };

    marks::set_mark(&lib, &[ids[0]], Some("x")).unwrap();
    marks::set_mark(&lib, &ids[1..], Some("1")).unwrap();
    assert!(marks::set_mark(&lib, &ids, Some("9")).is_err());
    assert_eq!(find(Filter { mark: Some("1".into()), ..Default::default() }), ids[1..]);
    assert_eq!(find(Filter { mark: Some("any".into()), ..Default::default() }).len(), 3);
    assert_eq!(queries::counts(&lib).unwrap().marked, 3);

    // Unconfigured custom marks can't run.
    assert!(marks::run(&lib, "1", |_, _| {}).is_err());

    // x -> library trash.
    let r = marks::run(&lib, "x", |_, _| {}).unwrap();
    assert_eq!(r.done, 1);
    assert_eq!(queries::counts(&lib).unwrap().trash, 1);

    // 1 -> copy to another disk, keeping the date folders; marks are cleared after.
    let out = _tmp.path().join("usb");
    std::fs::create_dir_all(&out).unwrap();
    marks::set_action(&lib, 1, "Backup", Some(ActionKind::Copy), &out.to_string_lossy()).unwrap();
    let slots = marks::list_slots(&lib).unwrap();
    assert_eq!((slots[1].label.as_str(), slots[1].count), ("Backup", 2));
    let r = marks::run(&lib, "1", |_, _| {}).unwrap();
    assert_eq!((r.done, r.skipped, r.errors.len()), (2, 0, 0));
    assert!(out.join("2021/07/15/p1.jpg").is_file() && out.join("2021/07/15/p2.jpg").is_file());
    assert_eq!(queries::counts(&lib).unwrap().marked, 0);
    // Running again with identical copies present skips them.
    marks::set_mark(&lib, &ids[1..2], Some("1")).unwrap();
    let r = marks::run(&lib, "1", |_, _| {}).unwrap();
    assert_eq!((r.done, r.skipped), (0, 1));
    assert!(!out.join("2021/07/15/p1-1.jpg").exists());

    // 2 -> album (moves files into its folder), 3 -> tag.
    let album = queries::create_album(&lib, "Picks").unwrap();
    marks::set_action(&lib, 2, "", Some(ActionKind::Album), &album.to_string()).unwrap();
    marks::set_action(&lib, 3, "", Some(ActionKind::Tag), "keeper").unwrap();
    marks::set_mark(&lib, &ids[1..2], Some("2")).unwrap();
    marks::set_mark(&lib, &ids[2..], Some("3")).unwrap();
    marks::run(&lib, "2", |_, _| {}).unwrap();
    marks::run(&lib, "3", |_, _| {}).unwrap();
    assert_eq!(rel_of(&lib, ids[1]), "albums/Picks/p1.jpg");
    assert_eq!(queries::get_photo(&lib, ids[2]).unwrap().tags[0].name, "keeper");

    // Removing an action setup.
    marks::set_action(&lib, 3, "", None, "").unwrap();
    assert!(marks::list_slots(&lib).unwrap()[3].kind.is_none());
    assert!(marks::set_action(&lib, 1, "", Some(ActionKind::Trash), "").is_err());
}

#[test]
fn sources_and_device_kind() {
    use frutyfoto_lib::db::device_kind;
    assert_eq!(device_kind(Some("Apple"), Some("iPhone 15 Pro")), "phone");
    assert_eq!(device_kind(Some("samsung"), Some("SM-S918B")), "phone");
    assert_eq!(device_kind(Some("SONY"), Some("ILCE-7M4")), "camera");
    assert_eq!(device_kind(Some("Sony"), Some("Xperia 1 V")), "phone");
    assert_eq!(device_kind(Some("FUJIFILM"), Some("X-T5")), "camera");
    assert_eq!(device_kind(None, None), "unknown");

    let Env { _tmp, src, lib } = env();
    write(&src.join("cam/a.jpg"), &with_exif(jpeg_bytes(&picture(0, 64, 64), 90), "2021:01:01 00:00:00", "ILCE-7M4"));
    write(&src.join("cam/b.jpg"), &with_exif(jpeg_bytes(&picture(1, 64, 64), 90), "2021:01:01 00:00:00", "iPhone 15"));
    import_all(&lib, &src.join("cam"));
    // Photos saved from a chat app have no EXIF; label the whole import.
    write(&src.join("line/c.jpg"), &jpeg_bytes(&picture(2, 64, 64), 90));
    import::run(&lib, &src.join("line"), Some(" LINE "), &AtomicBool::new(false), |_| {}).unwrap();

    let names = |origin: &str| -> Vec<String> {
        let f = Filter { origin: Some(origin.into()), ..Default::default() };
        queries::list_photos(&lib, &f, Sort::TakenAsc, 0, 10).unwrap().into_iter().map(|p| p.orig_name).collect()
    };
    assert_eq!(names("camera"), vec!["a.jpg"]);
    assert_eq!(names("phone"), vec!["b.jpg"]);
    assert_eq!(names("LINE"), vec!["c.jpg"]);
    assert_eq!(frutyfoto_lib::marks::list_sources(&lib).unwrap(), vec!["LINE"]);

    let a = queries::list_photos(&lib, &Filter { origin: Some("camera".into()), ..Default::default() }, Sort::TakenAsc, 0, 1)
        .unwrap()[0]
        .clone();
    assert_eq!((a.device, a.source.as_deref()), ("camera", None));
    frutyfoto_lib::marks::set_source(&lib, &[a.id], "Facebook").unwrap();
    assert_eq!(names("Facebook"), vec!["a.jpg"]);
    frutyfoto_lib::marks::set_source(&lib, &[a.id], "  ").unwrap();
    assert_eq!(names("camera"), vec!["a.jpg"]);
}

#[test]
fn emptying_trash_hands_files_to_the_system_trash() {
    let Env { _tmp, src, lib } = env();
    let ids = three_photos(&lib, &src);
    let album = queries::create_album(&lib, "A").unwrap();
    queries::add_to_album(&lib, album, &ids[..1]).unwrap();
    queries::trash_photos(&lib, &ids[..2]).unwrap();

    let discarded = std::sync::Mutex::new(Vec::new());
    let n = queries::empty_trash_with(&lib, |p| {
        discarded.lock().unwrap().push(p.file_name().unwrap().to_string_lossy().into_owned());
        Ok(std::fs::remove_file(p)?)
    })
    .unwrap();
    assert_eq!(n, 2);
    let mut d = discarded.into_inner().unwrap();
    d.sort();
    assert_eq!(d, vec!["p0.jpg", "p1.jpg"]);
    // Empty folders left in the trash are cleaned up.
    assert_eq!(std::fs::read_dir(lib.trash_dir().join("albums")).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(lib.trash_dir().join("originals")).unwrap().count(), 0);
}
