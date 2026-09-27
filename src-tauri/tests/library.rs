use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{Local, NaiveDate, TimeZone};
use exif::experimental::Writer;
use exif::{Field, In, Tag, Value};
use foto_lake_lib::import::{self, hash_file, ImportProgress};
use foto_lake_lib::library::Library;
use foto_lake_lib::queries::{self, Filter, Sort};
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
    import::run(lib, src, &AtomicBool::new(false), |_| {}).unwrap()
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

// ---- tests ----

#[test]
fn import_organises_by_date_and_skips_duplicates() {
    let Env { src, lib, .. } = env();

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
    let Env { lib, .. } = env();
    let err = import::run(&lib, &lib.root.join("originals"), &AtomicBool::new(false), |_| {});
    assert!(err.is_err());
}

#[test]
fn near_duplicates_are_grouped() {
    let Env { src, lib, .. } = env();
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
    let Env { src, lib, .. } = env();
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
    let Env { src, lib, .. } = env();
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
    assert_eq!(queries::empty_trash(&lib).unwrap(), 1);
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
