//! Where photo files live. A photo in at least one album is stored in its "home" album's
//! folder (`albums/<dir>/`); every other photo lives in `originals/YYYY/MM/DD/`.
//! Trashed photos keep the same relative path under the trash dir.

use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension};

use crate::error::{AppError, Result};
use crate::import::reserve_dest;
use crate::library::Library;

struct Placement {
    id: i64,
    rel_path: String,
    orig_name: String,
    taken_at: String,
    trashed: bool,
    home: Option<i64>,
    albums: Vec<i64>,
}

/// Turns an album name into a safe folder name.
pub fn sanitize_dir_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let short: String = trimmed.chars().take(100).collect();
    if short.is_empty() { "Album".into() } else { short }
}

/// Picks a folder name for `name` not used by another album nor present on disk.
fn free_dir_name(lib: &Library, album_id: i64, name: &str) -> Result<String> {
    let base = sanitize_dir_name(name);
    for n in 1..10_000 {
        let candidate = if n == 1 { base.clone() } else { format!("{base} ({n})") };
        let taken = lib
            .conn()
            .query_row(
                "SELECT 1 FROM albums WHERE dir = ?1 COLLATE NOCASE AND id != ?2",
                params![candidate, album_id],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !taken && !lib.albums_dir().join(&candidate).exists() {
            return Ok(candidate);
        }
    }
    Err(AppError::msg(format!("could not find a free folder name for {name}")))
}

/// The album's folder name, assigning one on first use.
fn album_dir(lib: &Library, album_id: i64) -> Result<String> {
    let (dir, name): (Option<String>, String) =
        lib.conn()
            .query_row("SELECT dir, name FROM albums WHERE id = ?1", [album_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if let Some(dir) = dir {
        return Ok(dir);
    }
    let dir = free_dir_name(lib, album_id, &name)?;
    lib.conn().execute("UPDATE albums SET dir = ?1 WHERE id = ?2", params![dir, album_id])?;
    Ok(dir)
}

fn load(lib: &Library, ids: &[i64]) -> Result<Vec<Placement>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT id, rel_path, orig_name, taken_at, trashed_at IS NOT NULL, home_album_id,
            (SELECT group_concat(album_id) FROM
               (SELECT album_id FROM album_photos WHERE photo_id = p.id ORDER BY album_id))
         FROM photos p WHERE id = ?1",
    )?;
    let mut out = Vec::new();
    for id in ids {
        let row = stmt
            .query_row([id], |r| {
                let albums: Option<String> = r.get(6)?;
                Ok(Placement {
                    id: r.get(0)?,
                    rel_path: r.get(1)?,
                    orig_name: r.get(2)?,
                    taken_at: r.get(3)?,
                    trashed: r.get(4)?,
                    home: r.get(5)?,
                    albums: albums.iter().flat_map(|s| s.split(',')).filter_map(|s| s.parse().ok()).collect(),
                })
            })
            .optional()?;
        out.extend(row);
    }
    Ok(out)
}

fn parent_rel(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("")
}

/// Moves each photo to where it belongs given its current album memberships.
/// Keeps going on errors and reports them together at the end.
pub fn rehome(lib: &Library, ids: &[i64]) -> Result<()> {
    let mut errors = Vec::new();
    for p in load(lib, ids)? {
        let home = match p.home {
            Some(h) if p.albums.contains(&h) => Some(h),
            _ => p.albums.first().copied(),
        };
        let dir = match home {
            Some(album) => format!("albums/{}", album_dir(lib, album)?),
            None => format!("originals/{}", p.taken_at.get(..10).unwrap_or("unknown").replace('-', "/")),
        };
        let result = if parent_rel(&p.rel_path) == dir {
            if home != p.home {
                lib.conn().execute("UPDATE photos SET home_album_id = ?1 WHERE id = ?2", params![home, p.id])?;
            }
            Ok(())
        } else {
            relocate(lib, &p, &dir, home)
        };
        if let Err(e) = result {
            errors.push(format!("{}: {e}", p.orig_name));
        }
    }
    match errors.len() {
        0 => Ok(()),
        n => Err(AppError::msg(format!("{n} photo(s) could not be moved: {}", errors.join("; ")))),
    }
}

fn relocate(lib: &Library, p: &Placement, dir: &str, home: Option<i64>) -> Result<()> {
    let base = if p.trashed { lib.trash_dir() } else { lib.root.clone() };
    let from = base.join(&p.rel_path);
    if !from.is_file() {
        return Err(AppError::msg(format!("{} is missing", from.display())));
    }
    let dest_dir = base.join(dir);
    std::fs::create_dir_all(&dest_dir)?;
    // Reserve a free name, then move the photo over the empty placeholder.
    let name = if p.orig_name.is_empty() { p.rel_path.rsplit('/').next().unwrap_or("photo") } else { &p.orig_name };
    let (dest, _) = reserve_dest(&dest_dir, name)?;
    if let Err(e) = std::fs::rename(&from, &dest) {
        let _ = std::fs::remove_file(&dest);
        return Err(e.into());
    }
    let name = dest.file_name().unwrap().to_string_lossy();
    let rel = format!("{dir}/{name}");
    if let Err(e) = lib.conn().execute(
        "UPDATE photos SET rel_path = ?1, home_album_id = ?2 WHERE id = ?3",
        params![rel, home, p.id],
    ) {
        let _ = std::fs::rename(&dest, &from);
        return Err(e.into());
    }
    prune_empty_dirs(&base, from.parent());
    Ok(())
}

/// Removes now-empty folders above a moved file, stopping at the top-level folders.
fn prune_empty_dirs(base: &Path, mut dir: Option<&Path>) {
    let stops = [base.to_path_buf(), base.join("originals"), base.join("albums")];
    while let Some(d) = dir {
        if stops.iter().any(|s| s == d) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// Renames an album's folder (in the library and in the trash) to match its new name.
pub fn rename_album_dir(lib: &Library, album_id: i64, new_name: &str) -> Result<()> {
    let old: Option<String> = lib.conn().query_row("SELECT dir FROM albums WHERE id = ?1", [album_id], |r| r.get(0))?;
    let Some(old) = old else { return Ok(()) };
    if sanitize_dir_name(new_name) == old {
        return Ok(());
    }
    let new = free_dir_name(lib, album_id, new_name)?;
    let dirs = |base: PathBuf| (base.join("albums").join(&old), base.join("albums").join(&new));
    let moves: Vec<(PathBuf, PathBuf)> =
        [dirs(lib.root.clone()), dirs(lib.trash_dir())].into_iter().filter(|(from, _)| from.exists()).collect();
    for (i, (from, to)) in moves.iter().enumerate() {
        if let Err(e) = std::fs::create_dir_all(to.parent().unwrap()).and_then(|_| std::fs::rename(from, to)) {
            for (from, to) in &moves[..i] {
                let _ = std::fs::rename(to, from);
            }
            return Err(e.into());
        }
    }
    let (old_prefix, new_prefix) = (format!("albums/{old}/"), format!("albums/{new}/"));
    let mut conn = lib.conn();
    let tx = conn.transaction()?;
    tx.execute("UPDATE albums SET dir = ?1 WHERE id = ?2", params![new, album_id])?;
    tx.execute(
        "UPDATE photos SET rel_path = ?1 || substr(rel_path, ?2)
         WHERE home_album_id = ?3 AND substr(rel_path, 1, ?4) = ?5",
        params![
            new_prefix,
            old_prefix.chars().count() as i64 + 1,
            album_id,
            old_prefix.chars().count() as i64,
            old_prefix
        ],
    )?;
    tx.commit()?;
    Ok(())
}

/// Removes an album's (empty) folders after it was deleted.
pub fn remove_album_dir(lib: &Library, dir: &str) {
    for base in [lib.root.clone(), lib.trash_dir()] {
        let _ = std::fs::remove_dir(base.join("albums").join(dir));
    }
}

/// Repairs placement for photos whose files aren't where their albums say, e.g. albums
/// created before album folders existed, or an interrupted album delete.
pub fn rehome_all(lib: &Library) -> Result<usize> {
    let ids: Vec<i64> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id FROM photos p WHERE
               (home_album_id IS NULL AND (rel_path LIKE 'albums/%'
                  OR EXISTS (SELECT 1 FROM album_photos WHERE photo_id = p.id)))
               OR (home_album_id IS NOT NULL AND NOT EXISTS
                  (SELECT 1 FROM album_photos WHERE photo_id = p.id AND album_id = p.home_album_id))",
        )?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    rehome(lib, &ids)?;
    Ok(ids.len())
}
