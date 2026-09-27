use std::path::Path;

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
];

pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )?;
    migrate(&mut conn)?;
    Ok(conn)
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
