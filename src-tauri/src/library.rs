use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

use crate::db;
use crate::error::{AppError, Result};

const META_DIR: &str = ".frutyfoto";
const LEGACY_META_DIR: &str = ".fotolake";

/// A library root on disk plus its open database.
pub struct Library {
    pub root: PathBuf,
    conn: Mutex<Connection>,
}

impl Library {
    /// Opens an existing library, or initialises one when `create` is set.
    pub fn open(root: &Path, create: bool) -> Result<Self> {
        // Libraries created before the rename to frutyfoto keep their metadata in `.fotolake`.
        let legacy = root.join(LEGACY_META_DIR);
        if legacy.is_dir() && !root.join(META_DIR).exists() {
            std::fs::rename(&legacy, root.join(META_DIR))?;
        }
        let db_path = root.join(META_DIR).join("library.db");
        if !db_path.exists() {
            if !create {
                return Err(AppError::msg(format!(
                    "{} is not a frutyfoto library",
                    root.display()
                )));
            }
            if root.join("originals").exists() {
                return Err(AppError::msg("folder already contains an 'originals' directory"));
            }
        }
        let root = root.to_path_buf();
        for dir in ["originals", "albums", ".frutyfoto/thumbs", ".frutyfoto/previews", ".frutyfoto/trash"] {
            std::fs::create_dir_all(root.join(dir))?;
        }
        let conn = db::open(&db_path)?;
        Ok(Library { root, conn: Mutex::new(conn) })
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn originals_dir(&self) -> PathBuf {
        self.root.join("originals")
    }

    /// Photos that belong to an album are stored in `albums/<album folder>/`.
    pub fn albums_dir(&self) -> PathBuf {
        self.root.join("albums")
    }

    pub fn trash_dir(&self) -> PathBuf {
        self.root.join(META_DIR).join("trash")
    }

    pub fn thumbs_dir(&self) -> PathBuf {
        self.root.join(META_DIR).join("thumbs")
    }

    pub fn previews_dir(&self) -> PathBuf {
        self.root.join(META_DIR).join("previews")
    }

    /// Folders the webview loads images from. Those inside the hidden `.frutyfoto` dir must
    /// be listed explicitly: Tauri's scope doesn't match dot-directories via `**` on Unix.
    pub fn served_dirs(&self) -> [PathBuf; 5] {
        [self.originals_dir(), self.albums_dir(), self.thumbs_dir(), self.previews_dir(), self.trash_dir()]
    }

    pub fn thumb_path(&self, hash: &str) -> PathBuf {
        self.thumbs_dir().join(&hash[..2]).join(format!("{hash}.jpg"))
    }

    /// Full-size JPEG rendition for formats the webview can't display (e.g. HEIC).
    pub fn preview_path(&self, hash: &str) -> PathBuf {
        self.previews_dir().join(&hash[..2]).join(format!("{hash}.jpg"))
    }

    /// Absolute path of a photo's file; trashed files live under the trash dir.
    pub fn photo_path(&self, rel_path: &str, trashed: bool) -> PathBuf {
        let base = if trashed { self.trash_dir() } else { self.root.clone() };
        base.join(rel_path)
    }
}
