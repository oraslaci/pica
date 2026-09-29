use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::db::DbManager;
use crate::editor::EditorManager;
use crate::models::{
    AppSettings, CropSuggestion, EditResult, EditSessionState, Folder, FolderWithPhotos,
    LibraryStats, Photo, StraightenSuggestion,
};
use crate::scanner::Scanner;
use crate::thumbnailer::{ThumbnailManager, ThumbnailScheduler};

#[derive(Clone)]
pub struct AppState {
    pub db: DbManager,
    pub thumbnailer: Arc<ThumbnailManager>,
    pub scheduler: Arc<ThumbnailScheduler>,
    pub is_scanning: Arc<AtomicBool>,
    pub editor: Arc<EditorManager>,
}

#[tauri::command]
pub fn get_watch_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, String> {
    state.db.get_folders().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_watch_folder(
    path: String,
    scan_mode: String,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Folder, String> {
    let folder_path = PathBuf::from(&path);
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err(format!("Invalid directory: {}", path));
    }

    let display_name = folder_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    let folder_id = state
        .db
        .add_or_update_folder(&path, &display_name, &scan_mode)
        .map_err(|e| e.to_string())?;

    let folder = Folder {
        id: folder_id,
        path: path.clone(),
        display_name,
        scan_mode: scan_mode.clone(),
        last_scanned: None,
        photo_count: 0,
    };

    if scan_mode != "exclude" {
        let db = state.db.clone();
        let thumbnailer = state.thumbnailer.clone();
        let scheduler = state.scheduler.clone();
        let is_scanning = state.is_scanning.clone();
        let handle = app_handle.clone();
        let f_clone = folder.clone();

        tauri::async_runtime::spawn(async move {
            is_scanning.store(true, Ordering::SeqCst);
            let scanner = Scanner::with_scheduler(db, thumbnailer, scheduler);
            let handle_clone = handle.clone();

            let _ = scanner.scan_folder(&f_clone, move |prog| {
                let _ = handle_clone.emit("scan-progress", prog);
            });

            is_scanning.store(false, Ordering::SeqCst);
            let _ = handle.emit("scan-complete", ());
        });
    }

    Ok(folder)
}

#[tauri::command]
pub fn update_watch_folder_mode(
    folder_id: i64,
    scan_mode: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .db
        .update_folder_mode(folder_id, &scan_mode)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_watch_folder(folder_id: i64, state: State<'_, AppState>) -> Result<(), String> {
    state.db.remove_folder(folder_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn trigger_scan(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if state.is_scanning.load(Ordering::SeqCst) {
        return Ok(());
    }

    let folders = state.db.get_folders().map_err(|e| e.to_string())?;
    let db = state.db.clone();
    let thumbnailer = state.thumbnailer.clone();
    let scheduler = state.scheduler.clone();
    let is_scanning = state.is_scanning.clone();

    tauri::async_runtime::spawn(async move {
        is_scanning.store(true, Ordering::SeqCst);
        let scanner = Scanner::with_scheduler(db, thumbnailer, scheduler);

        for folder in folders {
            if folder.scan_mode == "exclude" {
                continue;
            }

            let handle = app_handle.clone();
            let _ = scanner.scan_folder(&folder, move |prog| {
                let _ = handle.emit("scan-progress", prog);
            });
        }

        is_scanning.store(false, Ordering::SeqCst);
        let _ = app_handle.emit("scan-complete", ());
    });

    Ok(())
}

#[tauri::command]
pub fn set_thumbnail_priority(
    screen: Option<Vec<String>>,
    near: Option<Vec<String>>,
    hashes: Option<Vec<String>>,
    _replace: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let screen = screen.unwrap_or_else(|| hashes.unwrap_or_default());
    let near = near.unwrap_or_default();
    state.scheduler.set_view_priority(screen, near);
    Ok(())
}

#[tauri::command]
pub fn get_folders_with_photos(state: State<'_, AppState>) -> Result<Vec<FolderWithPhotos>, String> {
    let folders = state.db.get_folders().map_err(|e| e.to_string())?;
    let mut result = Vec::new();

    for folder in folders {
        if folder.scan_mode == "exclude" {
            continue;
        }

        let mut photos = state
            .db
            .get_photos_for_folder(folder.id)
            .map_err(|e| e.to_string())?;

        // Enrich photos with thumbnail path
        for p in &mut photos {
            let thumb = state.thumbnailer.get_thumbnail_path(&p.file_hash);
            if thumb.exists() {
                p.thumbnail_path = Some(thumb.to_string_lossy().to_string());
            }
        }

        if !photos.is_empty() {
            result.push(FolderWithPhotos { folder, photos });
        }
    }

    Ok(result)
}

#[tauri::command]
pub fn get_photos_for_folder(folder_id: i64, state: State<'_, AppState>) -> Result<Vec<Photo>, String> {
    let mut photos = state
        .db
        .get_photos_for_folder(folder_id)
        .map_err(|e| e.to_string())?;

    for p in &mut photos {
        let thumb = state.thumbnailer.get_thumbnail_path(&p.file_hash);
        if thumb.exists() {
            p.thumbnail_path = Some(thumb.to_string_lossy().to_string());
        }
    }

    Ok(photos)
}

#[tauri::command]
pub fn get_all_photos(
    limit: i64,
    offset: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Photo>, String> {
    let mut photos = state
        .db
        .get_all_photos(limit, offset)
        .map_err(|e| e.to_string())?;

    for p in &mut photos {
        let thumb = state.thumbnailer.get_thumbnail_path(&p.file_hash);
        if thumb.exists() {
            p.thumbnail_path = Some(thumb.to_string_lossy().to_string());
        }
    }

    Ok(photos)
}

#[tauri::command]
pub fn toggle_favorite(photo_id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state.db.toggle_favorite(photo_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let is_scanning = state.is_scanning.load(Ordering::Relaxed);
    state.db.get_stats(is_scanning).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.db.get_app_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.update_app_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditSessionState, String> {
    state.editor.get_session_state(&state.db, photo_id)
}

#[tauri::command]
pub fn apply_rotation(
    photo_id: i64,
    degrees: i32,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_rotation(&state.db, &state.thumbnailer, photo_id, degrees)
}

#[tauri::command]
pub fn undo_edit(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.undo_edit(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn redo_edit(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.redo_edit(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn jump_to_step(
    photo_id: i64,
    step_id: usize,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.jump_to_step(&state.db, &state.thumbnailer, photo_id, step_id)
}

#[tauri::command]
pub fn save_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditSessionState, String> {
    state.editor.save_edit_session(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn revert_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .revert_edit_session(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn resolve_edit_conflict(
    photo_id: i64,
    choice: String,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .resolve_conflict(&state.db, &state.thumbnailer, photo_id, &choice)
}

#[tauri::command]
pub fn get_crop_suggestions(
    photo_id: i64,
    aspect_ratio: Option<f64>,
    state: State<'_, AppState>,
) -> Result<Vec<CropSuggestion>, String> {
    state
        .editor
        .get_crop_suggestions(&state.db, &state.thumbnailer, photo_id, aspect_ratio)
}

#[tauri::command]
pub fn apply_crop(
    photo_id: i64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_crop(&state.db, &state.thumbnailer, photo_id, x, y, width, height)
}

#[tauri::command]
pub fn get_straighten_suggestions(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<StraightenSuggestion>, String> {
    state
        .editor
        .get_straighten_suggestions(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn apply_straighten(
    photo_id: i64,
    angle: f64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_straighten(&state.db, &state.thumbnailer, photo_id, angle)
}


