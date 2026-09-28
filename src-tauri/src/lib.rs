pub mod commands;
pub mod db;
pub mod dupes;
pub mod error;
pub mod exif;
pub mod import;
pub mod library;
pub mod marks;
pub mod queries;
pub mod storage;
pub mod thumbs;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            restore_last_library(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_library,
            current_library,
            start_import,
            cancel_import,
            list_imports,
            import_errors,
            list_photos,
            photo_date_buckets,
            get_photo,
            counts,
            set_rating,
            set_favorite,
            list_tags,
            add_tags,
            remove_tag,
            list_cameras,
            list_albums,
            create_album,
            rename_album,
            delete_album,
            add_to_album,
            remove_from_album,
            set_album_cover,
            find_duplicates,
            trash_photos,
            restore_photos,
            empty_trash,
            set_mark,
            list_marks,
            set_mark_action,
            run_mark,
            list_sources,
            set_source,
        ])
        .run(tauri::generate_context!())
        .expect("error while running foto-lake");
}
