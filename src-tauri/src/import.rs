use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDateTime};
use rayon::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use walkdir::WalkDir;

use crate::error::{AppError, Result};
use crate::library::Library;
use crate::{exif, thumbs};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff", "gif", "heic", "heif", "hif"];

/// Formats the webview (WebKitGTK) can't display; these get a full-size JPEG preview for the viewer.
const PREVIEW_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "tif", "tiff"];

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub import_id: i64,
    pub total: usize,
    pub done: usize,
    pub added: usize,
    pub skipped: usize,
    pub failed: usize,
    pub current: String,
    pub finished: bool,
    pub cancelled: bool,
    pub errors: Vec<ImportFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportFailure {
    pub path: String,
    pub error: String,
}

enum Outcome {
    Added,
    Duplicate,
}

fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
}

pub fn is_supported(path: &Path) -> bool {
    extension(path).is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.as_str()))
}

pub fn needs_preview(path: &Path) -> bool {
    extension(path).is_some_and(|e| PREVIEW_EXTENSIONS.contains(&e.as_str()))
}

/// Copies every supported image under `src` into the library. Source files are only read.
/// `on_progress` is called (throttled) while running and once more when finished.
pub fn run(
    lib: &Library,
    src: &Path,
    cancel: &AtomicBool,
    on_progress: impl Fn(&ImportProgress) + Sync,
) -> Result<ImportProgress> {
    if !src.is_dir() {
        return Err(AppError::msg(format!("{} is not a folder", src.display())));
    }
    if src.starts_with(&lib.root) || lib.root.starts_with(src) {
        return Err(AppError::msg("cannot import from inside the library (or a parent of it)"));
    }

    let files: Vec<PathBuf> = WalkDir::new(src)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_supported(e.path()))
        .map(|e| e.into_path())
        .collect();

    let import_id = {
        let conn = lib.conn();
        conn.execute(
            "INSERT INTO imports (source_dir, started_at) VALUES (?1, ?2)",
            params![src.to_string_lossy(), now()],
        )?;
        conn.last_insert_rowid()
    };

    let progress = Mutex::new(ImportProgress { import_id, total: files.len(), ..Default::default() });
    let last_emit = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
    let claimed = Mutex::new(HashSet::<String>::new());

    on_progress(&progress.lock().unwrap());

    files.par_iter().for_each(|path| {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let result = import_one(lib, import_id, path, &claimed);
        let mut p = progress.lock().unwrap();
        p.done += 1;
        p.current = path.to_string_lossy().into_owned();
        match result {
            Ok(Outcome::Added) => p.added += 1,
            Ok(Outcome::Duplicate) => p.skipped += 1,
            Err(e) => {
                p.failed += 1;
                let path = p.current.clone();
                p.errors.push(ImportFailure { path, error: e.to_string() });
            }
        }
        let mut last = last_emit.lock().unwrap();
        if last.elapsed() >= PROGRESS_INTERVAL {
            *last = Instant::now();
            on_progress(&p);
        }
    });

    let mut p = progress.into_inner().unwrap();
    p.finished = true;
    p.cancelled = cancel.load(Ordering::Relaxed);
    {
        let mut conn = lib.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE imports SET finished_at = ?1, added = ?2, skipped_dupes = ?3, failed = ?4, cancelled = ?5
             WHERE id = ?6",
            params![now(), p.added, p.skipped, p.failed, p.cancelled, import_id],
        )?;
        for e in &p.errors {
            tx.execute(
                "INSERT INTO import_errors (import_id, path, error) VALUES (?1, ?2, ?3)",
                params![import_id, e.path, e.error],
            )?;
        }
        tx.commit()?;
    }
    on_progress(&p);
    Ok(p)
}

fn import_one(lib: &Library, import_id: i64, src: &Path, claimed: &Mutex<HashSet<String>>) -> Result<Outcome> {
    let hash = hash_file(src)?;

    // Claim the hash so identical files within one import aren't copied twice.
    {
        let mut claimed = claimed.lock().unwrap();
        if claimed.contains(&hash) {
            return Ok(Outcome::Duplicate);
        }
        let exists = lib
            .conn()
            .query_row("SELECT 1 FROM photos WHERE hash = ?1", [&hash], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            return Ok(Outcome::Duplicate);
        }
        claimed.insert(hash.clone());
    }

    let result = copy_and_index(lib, import_id, src, &hash);
    if result.is_err() {
        claimed.lock().unwrap().remove(&hash);
    }
    result.map(|_| Outcome::Added)
}

fn copy_and_index(lib: &Library, import_id: i64, src: &Path, hash: &str) -> Result<()> {
    let meta = std::fs::metadata(src)?;
    let exif = exif::read(src);
    let taken_at = exif.taken_at.unwrap_or_else(|| mtime(&meta));

    let dir = lib.originals_dir().join(taken_at.format("%Y/%m/%d").to_string());
    std::fs::create_dir_all(&dir)?;
    let orig_name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (dest, file) = reserve_dest(&dir, &orig_name)?;
    let thumb = lib.thumb_path(hash);
    let preview = needs_preview(src).then(|| lib.preview_path(hash));

    let cleanup = |e: AppError| {
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(&thumb);
        if let Some(p) = &preview {
            let _ = std::fs::remove_file(p);
        }
        e
    };

    copy_verified(src, file, hash).map_err(cleanup)?;
    let processed = thumbs::process(&dest, &thumb, preview.as_deref()).map_err(cleanup)?;

    let rel_path = dest
        .strip_prefix(&lib.root)
        .expect("dest is inside library")
        .to_string_lossy()
        .replace('\\', "/");

    lib.conn()
        .execute(
            "INSERT INTO photos (hash, phash, rel_path, orig_name, source_path, taken_at, imported_at,
                width, height, file_size, mime, camera_make, camera_model, lens, iso, f_number,
                exposure, focal_len, gps_lat, gps_lon, orientation, import_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                hash,
                processed.phash as i64,
                rel_path,
                orig_name,
                src.to_string_lossy(),
                fmt_dt(taken_at),
                now(),
                processed.width,
                processed.height,
                meta.len() as i64,
                mime_for(src),
                exif.camera_make,
                exif.camera_model,
                exif.lens,
                exif.iso,
                exif.f_number,
                exif.exposure,
                exif.focal_len,
                exif.gps_lat,
                exif.gps_lon,
                exif.orientation,
                import_id,
            ],
        )
        .map_err(|e| cleanup(e.into()))?;
    Ok(())
}

/// Atomically creates `dir/name`, or `dir/stem-N.ext` if taken.
fn reserve_dest(dir: &Path, name: &str) -> Result<(PathBuf, File)> {
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 0..10_000 {
        let candidate = if n == 0 { dir.join(name) } else { dir.join(format!("{stem}-{n}{ext}")) };
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(f) => return Ok((candidate, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::msg(format!("could not find a free file name for {name}")))
}

/// Copies `src` into `dest`, verifying the bytes written match the expected hash.
fn copy_verified(src: &Path, mut dest: File, expected_hash: &str) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(src)?);
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dest.write_all(&buf[..n])?;
    }
    dest.sync_all()?;
    if hasher.finalize().to_hex().as_str() != expected_hash {
        return Err(AppError::msg("source file changed during import"));
    }
    Ok(())
}

/// Creates any missing previews, e.g. for photos imported before their format got one.
/// Per-photo failures are logged and skipped. Returns how many previews were made.
pub fn backfill_previews(lib: &Library) -> Result<usize> {
    let rows: Vec<(String, String, bool)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT hash, rel_path, trashed_at IS NOT NULL FROM photos")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let made = rows
        .par_iter()
        .filter(|(hash, rel, trashed)| {
            needs_preview(Path::new(rel)) && !lib.preview_path(hash).exists() && lib.photo_path(rel, *trashed).is_file()
        })
        .filter(|(hash, rel, trashed)| {
            let src = lib.photo_path(rel, *trashed);
            match thumbs::decode_upright(&src).and_then(|img| thumbs::write_preview(&img, &lib.preview_path(hash))) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("preview for {} failed: {e}", src.display());
                    false
                }
            }
        })
        .count();
    Ok(made)
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn mtime(meta: &std::fs::Metadata) -> NaiveDateTime {
    meta.modified()
        .map(|t| DateTime::<Local>::from(t).naive_local())
        .unwrap_or_else(|_| Local::now().naive_local())
}

fn mime_for(path: &Path) -> &'static str {
    match extension(path).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("tif" | "tiff") => "image/tiff",
        Some("gif") => "image/gif",
        Some("heic" | "hif") => "image/heic",
        Some("heif") => "image/heif",
        _ => "application/octet-stream",
    }
}

pub fn fmt_dt(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn now() -> String {
    fmt_dt(Local::now().naive_local())
}
