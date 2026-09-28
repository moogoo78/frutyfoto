//! Culling marks: while browsing, photos are marked "x" (delete) or "1".."4" (custom
//! actions), then each mark is processed in one batch.

use std::path::Path;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::import::{copy_verified, hash_file, reserve_dest};
use crate::library::Library;
use crate::queries;

pub const MARKS: &[&str] = &["x", "1", "2", "3", "4"];

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    /// Move to the library trash (mark "x" only).
    Trash,
    /// Copy the files to a folder, e.g. on another disk.
    Copy,
    /// Add to an album (`target` is the album id).
    Album,
    /// Add a tag (`target` is the tag name).
    Tag,
}

impl ActionKind {
    fn as_str(self) -> &'static str {
        match self {
            ActionKind::Trash => "trash",
            ActionKind::Copy => "copy",
            ActionKind::Album => "album",
            ActionKind::Tag => "tag",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "copy" => ActionKind::Copy,
            "album" => ActionKind::Album,
            "tag" => ActionKind::Tag,
            _ => return None,
        })
    }
}

/// A mark together with what running it does and how many photos carry it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkSlot {
    pub mark: String,
    pub label: String,
    /// None for a custom mark that hasn't been set up yet.
    pub kind: Option<ActionKind>,
    pub target: String,
    pub count: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub done: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

fn check_mark(mark: &str) -> Result<()> {
    if MARKS.contains(&mark) {
        Ok(())
    } else {
        Err(AppError::msg(format!("unknown mark {mark:?}")))
    }
}

fn id_list(ids: &[i64]) -> String {
    ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

/// Sets (or with `None` clears) the mark of the given photos.
pub fn set_mark(lib: &Library, ids: &[i64], mark: Option<&str>) -> Result<()> {
    if let Some(m) = mark {
        check_mark(m)?;
    }
    lib.conn().execute(&format!("UPDATE photos SET mark = ?1 WHERE id IN ({})", id_list(ids)), [mark])?;
    Ok(())
}

pub fn list_slots(lib: &Library) -> Result<Vec<MarkSlot>> {
    let conn = lib.conn();
    let mut slots: Vec<MarkSlot> = MARKS
        .iter()
        .map(|m| MarkSlot {
            mark: m.to_string(),
            label: if *m == "x" { "Delete".into() } else { String::new() },
            kind: (*m == "x").then_some(ActionKind::Trash),
            target: String::new(),
            count: 0,
        })
        .collect();
    let mut stmt = conn.prepare("SELECT slot, label, kind, target FROM mark_actions")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
    })?;
    for row in rows {
        let (slot, label, kind, target) = row?;
        if let Some(s) = slots.get_mut(slot as usize) {
            (s.label, s.kind, s.target) = (label, ActionKind::parse(&kind), target);
        }
    }
    let mut stmt =
        conn.prepare("SELECT mark, COUNT(*) FROM photos WHERE mark IS NOT NULL AND trashed_at IS NULL GROUP BY mark")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (mark, count) = row?;
        if let Some(s) = slots.iter_mut().find(|s| s.mark == mark) {
            s.count = count;
        }
    }
    Ok(slots)
}

/// Configures what custom mark `slot` (1-4) does; `kind: None` removes the setup.
pub fn set_action(lib: &Library, slot: i64, label: &str, kind: Option<ActionKind>, target: &str) -> Result<()> {
    if !(1..=4).contains(&slot) {
        return Err(AppError::msg("mark slot must be 1-4"));
    }
    let conn = lib.conn();
    match kind {
        None => {
            conn.execute("DELETE FROM mark_actions WHERE slot = ?1", [slot])?;
        }
        Some(ActionKind::Trash) => return Err(AppError::msg("only mark x moves photos to the trash")),
        Some(kind) => {
            conn.execute(
                "INSERT OR REPLACE INTO mark_actions (slot, label, kind, target) VALUES (?1, ?2, ?3, ?4)",
                params![slot, label.trim(), kind.as_str(), target.trim()],
            )?;
        }
    }
    Ok(())
}

/// Runs the action of `mark` on every (non-trashed) photo carrying it. Photos that were
/// processed lose their mark; failed ones keep it so the run can be retried.
pub fn run(lib: &Library, mark: &str, on_progress: impl Fn(usize, usize)) -> Result<RunResult> {
    check_mark(mark)?;
    let slot = list_slots(lib)?.into_iter().find(|s| s.mark == mark).unwrap();
    let kind = slot.kind.ok_or_else(|| AppError::msg(format!("mark {mark} has no action set up")))?;
    let photos: Vec<(i64, String, String, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, orig_name, hash FROM photos WHERE mark = ?1 AND trashed_at IS NULL ORDER BY taken_at",
        )?;
        let rows = stmt.query_map([mark], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let ids: Vec<i64> = photos.iter().map(|p| p.0).collect();
    let mut result = RunResult::default();
    if ids.is_empty() {
        return Ok(result);
    }

    match kind {
        ActionKind::Trash => queries::trash_photos(lib, &ids)?,
        ActionKind::Album => {
            let album: i64 = slot.target.parse().map_err(|_| AppError::msg("no album chosen for this mark"))?;
            if let Err(e) = queries::add_to_album(lib, album, &ids) {
                // Membership is recorded even if some files couldn't be moved into the folder.
                result.errors.push(e.to_string());
            }
        }
        ActionKind::Tag => {
            if slot.target.trim().is_empty() {
                return Err(AppError::msg("no tag set for this mark"));
            }
            queries::add_tags(lib, &ids, &[slot.target.clone()])?
        }
        ActionKind::Copy => {
            let target = Path::new(&slot.target);
            if slot.target.is_empty() || !target.is_dir() {
                return Err(AppError::msg(format!("target folder {:?} is not available", slot.target)));
            }
            let mut copied = Vec::new();
            for (i, (id, rel, name, hash)) in photos.iter().enumerate() {
                match copy_out(lib, rel, name, hash, target) {
                    Ok(true) => copied.push(*id),
                    Ok(false) => {
                        result.skipped += 1;
                        copied.push(*id);
                    }
                    Err(e) => result.errors.push(format!("{name}: {e}")),
                }
                on_progress(i + 1, photos.len());
            }
            set_mark(lib, &copied, None)?;
            result.done = copied.len() - result.skipped;
            return Ok(result);
        }
    }
    set_mark(lib, &ids, None)?;
    result.done = ids.len();
    Ok(result)
}

/// Copies a photo to `target`, keeping its folder below `albums/` or `originals/`
/// (e.g. `Trip/a.jpg`, `2024/07/15/a.jpg`). Returns false if an identical copy is already there.
fn copy_out(lib: &Library, rel: &str, name: &str, hash: &str, target: &Path) -> Result<bool> {
    let src = lib.photo_path(rel, false);
    let sub = rel.split_once('/').map(|(_, rest)| rest).unwrap_or(rel);
    let dir = target.join(sub).parent().map(Path::to_path_buf).unwrap_or_else(|| target.to_path_buf());
    let file_name = Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(name.to_string());
    let existing = dir.join(&file_name);
    if existing.is_file() && hash_file(&existing)? == hash {
        return Ok(false);
    }
    std::fs::create_dir_all(&dir)?;
    let (dest, file) = reserve_dest(&dir, &file_name)?;
    if let Err(e) = copy_verified(&src, file, hash) {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }
    Ok(true)
}

/// Distinct user-set sources, for suggestions and filtering.
pub fn list_sources(lib: &Library) -> Result<Vec<String>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source FROM photos WHERE source IS NOT NULL AND trashed_at IS NULL ORDER BY source COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Sets where photos came from (e.g. "LINE"); empty clears it back to the camera/phone guess.
pub fn set_source(lib: &Library, ids: &[i64], source: &str) -> Result<()> {
    let source = Some(source.trim()).filter(|s| !s.is_empty());
    lib.conn().execute(&format!("UPDATE photos SET source = ?1 WHERE id IN ({})", id_list(ids)), [source])?;
    Ok(())
}
