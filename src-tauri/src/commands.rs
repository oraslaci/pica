use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

use crate::db::DbManager;
use crate::editor::EditorManager;
use crate::faces::timings::{FaceScanTimingRecord, FaceTimingStats};
use crate::models::{
    AppSettings, CropSuggestion, EditResult, EditSessionState, FaceBox, FaceCollection,
    FaceSettings, Folder, FolderWithPhotos, LibraryStats, Photo, PhotoMetadata, RedEyeBox, RetouchSpot, StraightenSuggestion, TextItem,
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
    pub faces: Arc<crate::faces::FaceManager>,
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
        face_detect_enabled: true,
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
    let faces = state.faces.clone();

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
        faces.wake();
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

#[tauri::command]
pub fn detect_red_eyes(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<RedEyeBox>, String> {
    let photo = state
        .db
        .get_photo_by_id(photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Photo not found".to_string())?;

    state.editor.auto_detect_red_eyes(&photo, state.faces.detector.as_ref())
}

#[tauri::command]
pub fn apply_redeye_step(
    photo_id: i64,
    boxes: Vec<RedEyeBox>,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_redeye(&state.db, &state.thumbnailer, photo_id, boxes)
}

#[tauri::command]
pub fn apply_retouch_step(
    photo_id: i64,
    spots: Vec<RetouchSpot>,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_retouch(&state.db, &state.thumbnailer, photo_id, spots)
}

#[tauri::command]
pub fn apply_text_step(
    photo_id: i64,
    items: Vec<TextItem>,
    overlay_base64: Option<String>,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_text(&state.db, &state.thumbnailer, photo_id, items, overlay_base64)
}

#[tauri::command]
pub fn apply_feeling_lucky(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_feeling_lucky(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn apply_auto_contrast(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_auto_contrast(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn apply_auto_color(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_auto_color(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub fn apply_fill_light(
    photo_id: i64,
    amount: f32,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_fill_light(&state.db, &state.thumbnailer, photo_id, amount)
}

#[tauri::command]
pub fn get_face_settings(state: State<'_, AppState>) -> Result<FaceSettings, String> {
    Ok(state.faces.get_settings())
}

#[tauri::command]
pub fn update_face_settings(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.faces.set_enabled(enabled, state.is_scanning.clone())
}

#[tauri::command]
pub fn update_folder_face_detect(
    folder_id: i64,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.faces.update_folder_face_detect(folder_id, enabled, state.is_scanning.clone())
}

#[tauri::command]
pub fn get_face_collections(state: State<'_, AppState>) -> Result<Vec<FaceCollection>, String> {
    state.faces.get_collections()
}

#[tauri::command]
pub fn get_faces_for_photo(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<FaceBox>, String> {
    state.faces.get_faces_for_photo(photo_id)
}

#[tauri::command]
pub fn get_photos_for_person(
    person_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Photo>, String> {
    state.faces.get_photos_for_person(person_id)
}

#[tauri::command]
pub fn update_person_name(
    person_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.faces.update_person_name(person_id, &name)
}

#[tauri::command]
pub fn trigger_face_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.faces.trigger_scan(state.is_scanning.clone());
    Ok(())
}

#[tauri::command]
pub fn get_face_timing_stats(state: State<'_, AppState>) -> Result<FaceTimingStats, String> {
    Ok(state.faces.get_timing_stats())
}

#[tauri::command]
pub fn set_face_timing_recording(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.faces.set_timing_recording(enabled)
}

#[tauri::command]
pub fn get_face_timing_records(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FaceScanTimingRecord>, String> {
    Ok(state.faces.get_timing_records(limit))
}

#[tauri::command]
pub fn clear_face_timing_records(state: State<'_, AppState>) -> Result<(), String> {
    state.faces.clear_timing_records();
    Ok(())
}

#[tauri::command]
pub fn export_face_timing_csv(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.faces.export_timing_csv())
}

#[tauri::command]
pub fn recluster_faces(threshold: Option<f32>, state: State<'_, AppState>) -> Result<usize, String> {
    state.faces.recluster_all_faces(threshold)
}

#[derive(serde::Serialize)]
pub struct RealignResult {
    pub updated_faces: usize,
    pub people_count: usize,
}

#[tauri::command]
pub fn realign_face_embeddings(state: State<'_, AppState>) -> Result<RealignResult, String> {
    let (updated_faces, people_count) = state.faces.realign_all_embeddings()?;
    Ok(RealignResult {
        updated_faces,
        people_count,
    })
}

#[tauri::command]
pub fn get_photo_metadata(photo_id: i64, state: State<'_, AppState>) -> Result<PhotoMetadata, String> {
    let photo = state
        .db
        .get_photo_by_id(photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Photo not found: {}", photo_id))?;

    Scanner::extract_full_metadata(&photo, &state.db)
}


