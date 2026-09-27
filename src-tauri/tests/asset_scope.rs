//! The webview loads thumbnails/previews through Tauri's asset protocol. On Unix its scope
//! won't match dot-directories via `**`, so `.fotolake/...` must be allowed explicitly.

use foto_lake_lib::library::Library;
use tauri::Manager;

#[test]
fn library_images_are_served_but_database_is_not() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::open(tmp.path(), true).unwrap();
    let app = tauri::test::mock_app();
    let scope = app.asset_protocol_scope();
    for dir in lib.served_dirs() {
        scope.allow_directory(&dir, true).unwrap();
    }

    let hash = "ab".repeat(32);
    for path in [
        lib.thumb_path(&hash),
        lib.preview_path(&hash),
        lib.photo_path("originals/2024/01/02/a.jpg", false),
        lib.photo_path("originals/2024/01/02/a.jpg", true),
    ] {
        assert!(scope.is_allowed(&path), "should be served: {}", path.display());
    }
    assert!(!scope.is_allowed(tmp.path().join(".fotolake/library.db")));
}
