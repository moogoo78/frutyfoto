use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::{AppError, Result};
use crate::import::{self, ImportFailure, ImportProgress};
use crate::queries::ImportRecord;
use crate::library::Library;
use crate::marks::{self, ActionKind, MarkSlot, RunResult};
use crate::queries::{self, Album, Bucket, Counts, Filter, Photo, PhotoDetail, Sort, Tag};

pub const PROGRESS_EVENT: &str = "import://progress";
pub const MARK_PROGRESS_EVENT: &str = "marks://progress";

#[derive(Default)]
pub struct AppState {
    library: Mutex<Option<Arc<Library>>>,
    import_cancel: Arc<AtomicBool>,
    importing: Arc<AtomicBool>,
}

impl AppState {
    fn lib(&self) -> Result<Arc<Library>> {
        self.library
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::msg("no library is open"))
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    last_library: Option<PathBuf>,
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

fn save_config(app: &AppHandle, cfg: &Config) {
    if let Some(path) = config_path(app) {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, serde_json::to_vec_pretty(cfg).unwrap());
    }
}

/// Re-opens the last used library on startup, if it still exists.
pub fn restore_last_library(app: &AppHandle) {
    let Some(cfg) = config_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Config>(&b).ok())
    else {
        return;
    };
    if let Some(root) = cfg.last_library {
        if let Err(e) = activate(app, &root, false) {
            eprintln!("could not reopen library {}: {e}", root.display());
        }
    }
}

fn activate(app: &AppHandle, root: &Path, create: bool) -> Result<LibraryInfo> {
    let lib = Library::open(root, create)?;
    let scope = app.asset_protocol_scope();
    for dir in lib.served_dirs() {
        scope.allow_directory(&dir, true).map_err(|e| AppError::msg(e.to_string()))?;
    }
    // Albums made before album folders existed: move their photos into the folders.
    match crate::storage::rehome_all(&lib) {
        Ok(0) => {}
        Ok(n) => eprintln!("moved {n} photo(s) into album folders"),
        Err(e) => eprintln!("moving photos into album folders failed: {e}"),
    }
    let info = LibraryInfo { root: lib.root.to_string_lossy().into_owned() };
    let lib = Arc::new(lib);
    *app.state::<AppState>().library.lock().unwrap() = Some(lib.clone());
    std::thread::spawn(move || match import::backfill_previews(&lib) {
        Ok(0) => {}
        Ok(n) => eprintln!("created {n} missing preview(s)"),
        Err(e) => eprintln!("preview backfill failed: {e}"),
    });
    save_config(app, &Config { last_library: Some(root.to_path_buf()) });
    Ok(info)
}

#[derive(Serialize)]
pub struct LibraryInfo {
    root: String,
}

#[tauri::command]
pub async fn open_library(app: AppHandle, state: State<'_, AppState>, path: String, create: bool) -> Result<LibraryInfo> {
    if state.importing.load(Ordering::SeqCst) {
        return Err(AppError::msg("an import is running"));
    }
    activate(&app, Path::new(&path), create)
}

#[tauri::command]
pub async fn current_library(state: State<'_, AppState>) -> Result<Option<LibraryInfo>> {
    Ok(state
        .library
        .lock()
        .unwrap()
        .as_ref()
        .map(|l| LibraryInfo { root: l.root.to_string_lossy().into_owned() }))
}

#[tauri::command]
pub async fn start_import(
    app: AppHandle,
    state: State<'_, AppState>,
    src: String,
    source: Option<String>,
) -> Result<()> {
    let lib = state.lib()?;
    if state.importing.swap(true, Ordering::SeqCst) {
        return Err(AppError::msg("an import is already running"));
    }
    state.import_cancel.store(false, Ordering::SeqCst);
    let (cancel, importing) = (state.import_cancel.clone(), state.importing.clone());

    std::thread::spawn(move || {
        let emit = |p: &ImportProgress| {
            let _ = app.emit(PROGRESS_EVENT, p);
        };
        if let Err(e) = import::run(&lib, Path::new(&src), source.as_deref(), &cancel, emit) {
            emit(&ImportProgress {
                finished: true,
                failed: 1,
                errors: vec![ImportFailure { path: src.clone(), error: e.to_string() }],
                ..Default::default()
            });
        }
        importing.store(false, Ordering::SeqCst);
    });
    Ok(())
}

#[tauri::command]
pub async fn cancel_import(state: State<'_, AppState>) -> Result<()> {
    state.import_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn list_imports(state: State<'_, AppState>) -> Result<Vec<ImportRecord>> {
    queries::list_imports(&*state.lib()?)
}

#[tauri::command]
pub async fn import_errors(state: State<'_, AppState>, import_id: i64) -> Result<Vec<ImportFailure>> {
    queries::import_errors(&*state.lib()?, import_id)
}

#[tauri::command]
pub async fn list_photos(
    state: State<'_, AppState>,
    filter: Filter,
    sort: Sort,
    offset: i64,
    limit: i64,
) -> Result<Vec<Photo>> {
    queries::list_photos(&*state.lib()?, &filter, sort, offset, limit)
}

#[tauri::command]
pub async fn photo_date_buckets(state: State<'_, AppState>, filter: Filter, sort: Sort) -> Result<Vec<Bucket>> {
    queries::date_buckets(&*state.lib()?, &filter, sort)
}

#[tauri::command]
pub async fn get_photo(state: State<'_, AppState>, id: i64) -> Result<PhotoDetail> {
    queries::get_photo(&*state.lib()?, id)
}

#[tauri::command]
pub async fn counts(state: State<'_, AppState>) -> Result<Counts> {
    queries::counts(&*state.lib()?)
}

#[tauri::command]
pub async fn set_rating(state: State<'_, AppState>, ids: Vec<i64>, rating: i64) -> Result<()> {
    queries::set_rating(&*state.lib()?, &ids, rating)
}

#[tauri::command]
pub async fn set_favorite(state: State<'_, AppState>, ids: Vec<i64>, favorite: bool) -> Result<()> {
    queries::set_favorite(&*state.lib()?, &ids, favorite)
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>> {
    queries::list_tags(&*state.lib()?)
}

#[tauri::command]
pub async fn add_tags(state: State<'_, AppState>, ids: Vec<i64>, names: Vec<String>) -> Result<()> {
    queries::add_tags(&*state.lib()?, &ids, &names)
}

#[tauri::command]
pub async fn remove_tag(state: State<'_, AppState>, ids: Vec<i64>, tag_id: i64) -> Result<()> {
    queries::remove_tag(&*state.lib()?, &ids, tag_id)
}

#[tauri::command]
pub async fn list_cameras(state: State<'_, AppState>) -> Result<Vec<String>> {
    queries::list_cameras(&*state.lib()?)
}

#[tauri::command]
pub async fn list_albums(state: State<'_, AppState>) -> Result<Vec<Album>> {
    queries::list_albums(&*state.lib()?)
}

#[tauri::command]
pub async fn create_album(state: State<'_, AppState>, name: String) -> Result<i64> {
    queries::create_album(&*state.lib()?, &name)
}

#[tauri::command]
pub async fn rename_album(state: State<'_, AppState>, id: i64, name: String) -> Result<()> {
    queries::rename_album(&*state.lib()?, id, &name)
}

#[tauri::command]
pub async fn delete_album(state: State<'_, AppState>, id: i64) -> Result<()> {
    queries::delete_album(&*state.lib()?, id)
}

#[tauri::command]
pub async fn add_to_album(state: State<'_, AppState>, album_id: i64, ids: Vec<i64>) -> Result<()> {
    queries::add_to_album(&*state.lib()?, album_id, &ids)
}

#[tauri::command]
pub async fn remove_from_album(state: State<'_, AppState>, album_id: i64, ids: Vec<i64>) -> Result<()> {
    queries::remove_from_album(&*state.lib()?, album_id, &ids)
}

#[tauri::command]
pub async fn set_album_cover(state: State<'_, AppState>, album_id: i64, photo_id: i64) -> Result<()> {
    queries::set_album_cover(&*state.lib()?, album_id, photo_id)
}

#[tauri::command]
pub async fn find_duplicates(state: State<'_, AppState>, threshold: u32) -> Result<Vec<Vec<Photo>>> {
    queries::find_duplicates(&*state.lib()?, threshold.min(32))
}

#[tauri::command]
pub async fn trash_photos(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    queries::trash_photos(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn restore_photos(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    queries::restore_photos(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn empty_trash(state: State<'_, AppState>) -> Result<usize> {
    queries::empty_trash(&*state.lib()?)
}

#[tauri::command]
pub async fn set_mark(state: State<'_, AppState>, ids: Vec<i64>, mark: Option<String>) -> Result<()> {
    marks::set_mark(&*state.lib()?, &ids, mark.as_deref())
}

#[tauri::command]
pub async fn list_marks(state: State<'_, AppState>) -> Result<Vec<MarkSlot>> {
    marks::list_slots(&*state.lib()?)
}

#[tauri::command]
pub async fn set_mark_action(
    state: State<'_, AppState>,
    slot: i64,
    label: String,
    kind: Option<ActionKind>,
    target: String,
) -> Result<()> {
    marks::set_action(&*state.lib()?, slot, &label, kind, &target)
}

#[derive(Clone, Serialize)]
struct MarkProgress {
    mark: String,
    done: usize,
    total: usize,
}

#[tauri::command]
pub async fn run_mark(app: AppHandle, state: State<'_, AppState>, mark: String) -> Result<RunResult> {
    let lib = state.lib()?;
    tauri::async_runtime::spawn_blocking(move || {
        marks::run(&lib, &mark, |done, total| {
            let _ = app.emit(MARK_PROGRESS_EVENT, MarkProgress { mark: mark.clone(), done, total });
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> Result<Vec<String>> {
    marks::list_sources(&*state.lib()?)
}

#[tauri::command]
pub async fn set_source(state: State<'_, AppState>, ids: Vec<i64>, source: String) -> Result<()> {
    marks::set_source(&*state.lib()?, &ids, &source)
}
