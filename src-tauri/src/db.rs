use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::Connection;

use crate::error::Result;

/// Each entry upgrades the schema by one version (tracked in PRAGMA user_version).
pub const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE photos (
    id           INTEGER PRIMARY KEY,
    hash         TEXT NOT NULL UNIQUE,
    phash        INTEGER,
    rel_path     TEXT NOT NULL,
    orig_name    TEXT NOT NULL,
    source_path  TEXT,
    taken_at     TEXT NOT NULL,
    imported_at  TEXT NOT NULL,
    width        INTEGER,
    height       INTEGER,
    file_size    INTEGER NOT NULL,
    mime         TEXT,
    camera_make  TEXT,
    camera_model TEXT,
    lens         TEXT,
    iso          INTEGER,
    f_number     REAL,
    exposure     TEXT,
    focal_len    REAL,
    gps_lat      REAL,
    gps_lon      REAL,
    orientation  INTEGER,
    rating       INTEGER NOT NULL DEFAULT 0,
    favorite     INTEGER NOT NULL DEFAULT 0,
    trashed_at   TEXT
);
CREATE INDEX idx_photos_taken ON photos(taken_at);
CREATE INDEX idx_photos_rating ON photos(rating);
CREATE INDEX idx_photos_phash ON photos(phash);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE photo_tags (
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (photo_id, tag_id)
);
CREATE INDEX idx_photo_tags_tag ON photo_tags(tag_id);

CREATE TABLE albums (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    cover_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL
);
CREATE TABLE album_photos (
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (album_id, photo_id)
);
CREATE INDEX idx_album_photos_photo ON album_photos(photo_id);

CREATE TABLE imports (
    id            INTEGER PRIMARY KEY,
    source_dir    TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    added         INTEGER NOT NULL DEFAULT 0,
    skipped_dupes INTEGER NOT NULL DEFAULT 0,
    failed        INTEGER NOT NULL DEFAULT 0
);
"#,
    // v2: link photos to the import that added them; keep per-file import errors.
    r#"
ALTER TABLE photos ADD COLUMN import_id INTEGER REFERENCES imports(id);
ALTER TABLE imports ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_photos_import ON photos(import_id);

CREATE TABLE import_errors (
    import_id INTEGER NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    path      TEXT NOT NULL,
    error     TEXT NOT NULL
);
CREATE INDEX idx_import_errors_import ON import_errors(import_id);

-- Existing photos: attribute each to the import whose time window contains it.
UPDATE photos SET import_id = (
    SELECT i.id FROM imports i
    WHERE photos.imported_at >= i.started_at
      AND photos.imported_at <= COALESCE(i.finished_at, '9999')
    ORDER BY i.id DESC LIMIT 1
);
"#,
    // v3: culling marks, photo source annotation, album folders.
    r#"
ALTER TABLE photos ADD COLUMN mark TEXT;
ALTER TABLE photos ADD COLUMN source TEXT;
ALTER TABLE photos ADD COLUMN home_album_id INTEGER REFERENCES albums(id) ON DELETE SET NULL;
CREATE INDEX idx_photos_mark ON photos(mark);
CREATE INDEX idx_photos_home ON photos(home_album_id);

-- Folder name under albums/; assigned when the album first holds a photo.
ALTER TABLE albums ADD COLUMN dir TEXT;
CREATE UNIQUE INDEX idx_albums_dir ON albums(dir COLLATE NOCASE);

-- What marks 1-4 do when run ('x' always moves to trash).
CREATE TABLE mark_actions (
    slot   INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 4),
    label  TEXT NOT NULL DEFAULT '',
    kind   TEXT NOT NULL CHECK (kind IN ('copy', 'album', 'tag')),
    target TEXT NOT NULL DEFAULT ''
);
"#,
];

pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )?;
    register_functions(&conn)?;
    migrate(&mut conn)?;
    Ok(conn)
}

/// `device_kind(make, model)` -> 'phone' | 'camera' | 'unknown', usable in queries.
fn register_functions(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "device_kind",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let make: Option<String> = ctx.get(0)?;
            let model: Option<String> = ctx.get(1)?;
            Ok(device_kind(make.as_deref(), model.as_deref()))
        },
    )?;
    Ok(())
}

const PHONE_MAKES: &[&str] = &[
    "apple", "google", "samsung", "xiaomi", "redmi", "huawei", "honor", "oneplus", "oppo", "vivo", "realme",
    "motorola", "nokia", "htc", "asus", "zte", "meizu", "lge", "nothing",
];
const PHONE_MODELS: &[&str] = &["iphone", "ipad", "pixel", "galaxy", "xperia", "sm-"];

/// Guesses whether a photo came from a phone or a dedicated camera from its EXIF make/model.
pub fn device_kind(make: Option<&str>, model: Option<&str>) -> &'static str {
    let make = make.unwrap_or("").trim().to_ascii_lowercase();
    let model = model.unwrap_or("").trim().to_ascii_lowercase();
    if make.is_empty() && model.is_empty() {
        return "unknown";
    }
    let phone = PHONE_MAKES.iter().any(|m| make.starts_with(m)) || PHONE_MODELS.iter().any(|m| model.contains(m));
    if phone { "phone" } else { "camera" }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}
