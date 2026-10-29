use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::db::DbManager;
use crate::editor::EditorManager;
use crate::faces::timings::{FaceScanTimingRecord, FaceTimingStats};
use crate::models::{
    AppSettings, CropSuggestion, EditResult, EditSessionState, FaceBox, FaceCollection,
    FaceSettings, Folder, FolderWithPhotos, LibraryStats, Photo, PhotoMetadata, RedEyeBox,
    RetouchSpot, StraightenSuggestion, TextItem, ViewerFolderContext,
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
pub async fn get_watch_folders(state: State<'_, AppState>) -> Result<Vec<Folder>, String> {
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
pub async fn set_thumbnail_priority(
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
pub async fn get_folders_with_photos(state: State<'_, AppState>) -> Result<Vec<FolderWithPhotos>, String> {
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
pub async fn toggle_favorite(photo_id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    state.db.toggle_favorite(photo_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_library_stats(state: State<'_, AppState>) -> Result<LibraryStats, String> {
    let is_scanning = state.is_scanning.load(Ordering::Relaxed);
    state.db.get_stats(is_scanning).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.db.get_app_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_app_settings(
    settings: AppSettings,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.db.update_app_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditSessionState, String> {
    state.editor.get_session_state(&state.db, photo_id)
}

#[tauri::command]
pub async fn apply_rotation(
    photo_id: i64,
    degrees: i32,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_rotation(&state.db, &state.thumbnailer, photo_id, degrees)
}

#[tauri::command]
pub async fn undo_edit(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.undo_edit(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn redo_edit(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.redo_edit(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn jump_to_step(
    photo_id: i64,
    step_id: usize,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state.editor.jump_to_step(&state.db, &state.thumbnailer, photo_id, step_id)
}

#[tauri::command]
pub async fn save_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditSessionState, String> {
    state.editor.save_edit_session(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn revert_edit_session(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .revert_edit_session(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn resolve_edit_conflict(
    photo_id: i64,
    choice: String,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .resolve_conflict(&state.db, &state.thumbnailer, photo_id, &choice)
}

#[tauri::command]
pub async fn get_crop_suggestions(
    photo_id: i64,
    aspect_ratio: Option<f64>,
    state: State<'_, AppState>,
) -> Result<Vec<CropSuggestion>, String> {
    state
        .editor
        .get_crop_suggestions(&state.db, &state.thumbnailer, photo_id, aspect_ratio)
}

#[tauri::command]
pub async fn apply_crop(
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
pub async fn get_straighten_suggestions(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<StraightenSuggestion>, String> {
    state
        .editor
        .get_straighten_suggestions(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn apply_straighten(
    photo_id: i64,
    angle: f64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_straighten(&state.db, &state.thumbnailer, photo_id, angle)
}

#[tauri::command]
pub async fn detect_red_eyes(
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
pub async fn apply_redeye_step(
    photo_id: i64,
    boxes: Vec<RedEyeBox>,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_redeye(&state.db, &state.thumbnailer, photo_id, boxes)
}

#[tauri::command]
pub async fn apply_retouch_step(
    photo_id: i64,
    spots: Vec<RetouchSpot>,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_retouch(&state.db, &state.thumbnailer, photo_id, spots)
}

#[tauri::command]
pub async fn apply_text_step(
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
pub async fn apply_feeling_lucky(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_feeling_lucky(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn apply_auto_contrast(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_auto_contrast(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn apply_auto_color(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_auto_color(&state.db, &state.thumbnailer, photo_id)
}

#[tauri::command]
pub async fn apply_fill_light(
    photo_id: i64,
    amount: f32,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    state
        .editor
        .apply_fill_light(&state.db, &state.thumbnailer, photo_id, amount)
}

#[tauri::command]
pub async fn get_face_settings(state: State<'_, AppState>) -> Result<FaceSettings, String> {
    Ok(state.faces.get_settings())
}

#[tauri::command]
pub async fn update_face_settings(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.faces.set_enabled(enabled, state.is_scanning.clone())
}

#[tauri::command]
pub async fn update_folder_face_detect(
    folder_id: i64,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.faces.update_folder_face_detect(folder_id, enabled, state.is_scanning.clone())
}

#[tauri::command]
pub async fn get_face_collections(state: State<'_, AppState>) -> Result<Vec<FaceCollection>, String> {
    state.faces.get_collections()
}

#[tauri::command]
pub async fn get_faces_for_photo(
    photo_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<FaceBox>, String> {
    state.faces.get_faces_for_photo(photo_id)
}

#[tauri::command]
pub async fn get_photos_for_person(
    person_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<Photo>, String> {
    state.faces.get_photos_for_person(person_id)
}

#[tauri::command]
pub async fn update_person_name(
    person_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.faces.update_person_name(person_id, &name)
}

#[tauri::command]
pub async fn trigger_face_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.faces.trigger_scan(state.is_scanning.clone());
    Ok(())
}

#[tauri::command]
pub async fn get_face_timing_stats(state: State<'_, AppState>) -> Result<FaceTimingStats, String> {
    Ok(state.faces.get_timing_stats())
}

#[tauri::command]
pub async fn set_face_timing_recording(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    state.faces.set_timing_recording(enabled)
}

#[tauri::command]
pub async fn get_face_timing_records(
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<FaceScanTimingRecord>, String> {
    Ok(state.faces.get_timing_records(limit))
}

#[tauri::command]
pub async fn clear_face_timing_records(state: State<'_, AppState>) -> Result<(), String> {
    state.faces.clear_timing_records();
    Ok(())
}

#[tauri::command]
pub async fn export_face_timing_csv(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.faces.export_timing_csv())
}

#[tauri::command]
pub async fn recluster_faces(threshold: Option<f32>, state: State<'_, AppState>) -> Result<usize, String> {
    state.faces.recluster_all_faces(threshold)
}

#[derive(serde::Serialize)]
pub struct RealignResult {
    pub updated_faces: usize,
    pub people_count: usize,
}

#[tauri::command]
pub async fn realign_face_embeddings(state: State<'_, AppState>) -> Result<RealignResult, String> {
    let (updated_faces, people_count) = state.faces.realign_all_embeddings()?;
    Ok(RealignResult {
        updated_faces,
        people_count,
    })
}

#[tauri::command]
pub async fn get_photo_metadata(photo_id: i64, state: State<'_, AppState>) -> Result<PhotoMetadata, String> {
    let photo = state
        .db
        .get_photo_by_id(photo_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Photo not found: {}", photo_id))?;

    Scanner::extract_full_metadata(&photo, &state.db)
}

pub fn get_viewer_folder_context_inner(
    file_path: &str,
    state: &AppState,
) -> Result<ViewerFolderContext, String> {
    let target_path = PathBuf::from(file_path);
    let resolved_path = if target_path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(&target_path))
            .unwrap_or(target_path)
    } else {
        target_path
    };

    let (dir_path, requested_file_name) = if resolved_path.is_dir() {
        (resolved_path.clone(), None)
    } else {
        let parent = resolved_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let file_name = resolved_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string());
        (parent, file_name)
    };

    let folder_name = dir_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Folder".to_string());
    let folder_path_str = dir_path.to_string_lossy().to_string();

    let mut discovered_files = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() && Scanner::is_supported_image(&p) {
                discovered_files.push(p);
            }
        }
    }

    discovered_files.sort_by(|a, b| {
        let a_str = a.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let b_str = b.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        a_str.cmp(&b_str)
    });

    let mut photos = Vec::with_capacity(discovered_files.len());
    let mut current_index = 0;

    for (idx, p) in discovered_files.into_iter().enumerate() {
        let path_str = p.to_string_lossy().to_string();
        let file_name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();

        if let Some(ref req_name) = requested_file_name {
            if file_name.eq_ignore_ascii_case(req_name) {
                current_index = idx;
            }
        }

        let photo_opt = state.db.get_photo_by_path(&path_str).ok().flatten();
        let photo = match photo_opt {
            Some(mut db_photo) => {
                let thumb = state.thumbnailer.get_thumbnail_path(&db_photo.file_hash);
                if thumb.exists() {
                    db_photo.thumbnail_path = Some(thumb.to_string_lossy().to_string());
                }
                db_photo
            }
            None => {
                let fs_meta = fs::metadata(&p).ok();
                let file_size = fs_meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
                let modified = fs_meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let file_hash = Scanner::compute_file_hash(&p, file_size as u64, modified);
                let thumb = state.thumbnailer.get_thumbnail_path(&file_hash);
                let thumbnail_path = if thumb.exists() {
                    Some(thumb.to_string_lossy().to_string())
                } else {
                    None
                };

                Photo {
                    id: 0,
                    folder_id: 0,
                    file_path: path_str,
                    file_hash,
                    file_name,
                    file_size,
                    width: None,
                    height: None,
                    date_taken: Some(modified as i64),
                    camera_make: None,
                    camera_model: None,
                    orientation: None,
                    is_favorite: false,
                    thumbnail_path,
                    edit_rotation: None,
                    has_unsaved_edits: false,
                }
            }
        };

        photos.push(photo);
    }

    Ok(ViewerFolderContext {
        current_index,
        folder_name,
        folder_path: folder_path_str,
        photos,
    })
}

#[tauri::command]
pub async fn get_viewer_folder_context(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<ViewerFolderContext, String> {
    get_viewer_folder_context_inner(&file_path, &state)
}

pub fn get_viewer_photo_metadata_inner(
    file_path: &str,
    state: &AppState,
) -> Result<PhotoMetadata, String> {
    let photo_opt = state.db.get_photo_by_path(file_path).ok().flatten();
    let photo = match photo_opt {
        Some(p) => p,
        None => {
            let p_buf = PathBuf::from(file_path);
            let file_name = p_buf
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let fs_meta = fs::metadata(&p_buf).ok();
            let file_size = fs_meta.as_ref().map(|m| m.len() as i64).unwrap_or(0);
            let modified = fs_meta
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let file_hash = Scanner::compute_file_hash(&p_buf, file_size as u64, modified);

            Photo {
                id: 0,
                folder_id: 0,
                file_path: file_path.to_string(),
                file_hash,
                file_name,
                file_size,
                width: None,
                height: None,
                date_taken: Some(modified as i64),
                camera_make: None,
                camera_model: None,
                orientation: None,
                is_favorite: false,
                thumbnail_path: None,
                edit_rotation: None,
                has_unsaved_edits: false,
            }
        }
    };

    Scanner::extract_full_metadata(&photo, &state.db)
}

#[tauri::command]
pub async fn get_viewer_photo_metadata(
    file_path: String,
    state: State<'_, AppState>,
) -> Result<PhotoMetadata, String> {
    get_viewer_photo_metadata_inner(&file_path, &state)
}

#[tauri::command]
pub fn open_viewer_window(
    app_handle: AppHandle,
    file_path: Option<String>,
) -> Result<(), String> {
    if let Some(viewer_win) = app_handle.get_webview_window("viewer") {
        let _ = viewer_win.set_decorations(false);
        let _ = viewer_win.show();
        let _ = viewer_win.unminimize();
        let _ = viewer_win.set_focus();
        if let Some(ref path) = file_path {
            let _ = viewer_win.emit("load-photo", path);
        }
        return Ok(());
    }

    let url_str = match file_path {
        Some(ref p) => format!(
            "index.html?mode=viewer&file={}",
            percent_encoding::percent_encode(p.as_bytes(), percent_encoding::NON_ALPHANUMERIC)
        ),
        None => "index.html?mode=viewer".to_string(),
    };

    let builder = WebviewWindowBuilder::new(
        &app_handle,
        "viewer",
        WebviewUrl::App(url_str.into()),
    )
    .title("Pica Photo Viewer")
    .inner_size(1200.0, 800.0)
    .min_inner_size(640.0, 480.0)
    .decorations(false)
    .resizable(true);

    builder.build().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn open_in_pica_editor(
    app_handle: AppHandle,
    file_path: String,
) -> Result<(), String> {
    if let Some(main_win) = app_handle.get_webview_window("main") {
        let _ = main_win.show();
        let _ = main_win.unminimize();
        let _ = main_win.set_focus();
        let _ = main_win.emit("open-in-pica-editor", &file_path);
        return Ok(());
    }

    let url_str = format!(
        "index.html?file={}",
        percent_encoding::percent_encode(file_path.as_bytes(), percent_encoding::NON_ALPHANUMERIC)
    );

    let builder = WebviewWindowBuilder::new(
        &app_handle,
        "main",
        WebviewUrl::App(url_str.into()),
    )
    .title("Pica")
    .inner_size(1200.0, 800.0)
    .min_inner_size(800.0, 550.0)
    .resizable(true);

    let win = builder.build().map_err(|e| e.to_string())?;
    let _ = win.emit("open-in-pica-editor", &file_path);
    Ok(())
}

#[tauri::command]
pub async fn rotate_viewer_photo(
    file_path: String,
    degrees: i32,
    state: State<'_, AppState>,
) -> Result<EditResult, String> {
    if let Ok(Some(photo)) = state.db.get_photo_by_path(&file_path) {
        return state
            .editor
            .apply_rotation(&state.db, &state.thumbnailer, photo.id, degrees);
    }

    // If not yet cataloged, register its folder and scan so we get full edit sessions
    let path_buf = PathBuf::from(&file_path);
    if let Some(parent) = path_buf.parent() {
        let parent_str = parent.to_string_lossy().to_string();
        let display_name = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Viewer Folder".to_string());
        if let Ok(folder_id) = state.db.add_or_update_folder(&parent_str, &display_name, "always") {
            let folder = Folder {
                id: folder_id,
                path: parent_str,
                display_name,
                scan_mode: "always".to_string(),
                last_scanned: None,
                photo_count: 0,
                face_detect_enabled: true,
            };
            let scanner = Scanner::new(state.db.clone(), state.thumbnailer.clone());
            let _ = scanner.scan_folder(&folder, |_| {});
            if let Ok(Some(new_p)) = state.db.get_photo_by_path(&file_path) {
                return state
                    .editor
                    .apply_rotation(&state.db, &state.thumbnailer, new_p.id, degrees);
            }
        }
    }

    Err("Unable to rotate photo".to_string())
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn open_external_url_inner(url: &str) -> Result<(), String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Invalid URL protocol: only http and https are permitted".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL with xdg-open: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL with open: {}", e))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("Failed to open URL with start: {}", e))?;
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        return Err("Unsupported OS platform for opening URL".to_string());
    }

    Ok(())
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    open_external_url_inner(&url)
}

#[tauri::command]
pub fn restart_app(app_handle: AppHandle) {
    app_handle.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbImage};
    use tempfile::tempdir;

    #[test]
    fn test_viewer_folder_context_and_metadata() {
        let dir = tempdir().expect("Failed to create tempdir");
        let dir_path = dir.path();

        // Create 3 synthetic test images in memory and save to tempdir
        let img = DynamicImage::ImageRgb8(RgbImage::new(100, 80));
        let path_a = dir_path.join("img_a.png");
        let path_b = dir_path.join("img_b.jpg");
        let path_c = dir_path.join("img_c.png");

        img.save(&path_a).expect("Failed to save img_a");
        img.save(&path_b).expect("Failed to save img_b");
        img.save(&path_c).expect("Failed to save img_c");

        let db = DbManager::new_in_memory().expect("Failed to create in-memory db");
        let thumbnailer = Arc::new(ThumbnailManager::new());
        let scheduler = Arc::new(ThumbnailScheduler::new(thumbnailer.clone(), Some(db.clone()), 2));
        let is_scanning = Arc::new(AtomicBool::new(false));
        let editor = Arc::new(EditorManager::new());
        let faces = Arc::new(crate::faces::FaceManager::new(db.clone()));

        let state = AppState {
            db,
            thumbnailer,
            scheduler,
            is_scanning,
            editor,
            faces,
        };

        let ctx = get_viewer_folder_context_inner(&path_b.to_string_lossy(), &state)
            .expect("Failed to get viewer folder context");

        assert_eq!(ctx.photos.len(), 3);
        assert_eq!(ctx.current_index, 1);
        assert_eq!(ctx.photos[0].file_name, "img_a.png");
        assert_eq!(ctx.photos[1].file_name, "img_b.jpg");
        assert_eq!(ctx.photos[2].file_name, "img_c.png");

        let meta = get_viewer_photo_metadata_inner(&path_b.to_string_lossy(), &state)
            .expect("Failed to get metadata");
        assert_eq!(meta.file_name, "img_b.jpg");
        assert!(meta.file_size > 0);
    }

    #[test]
    fn test_get_app_version() {
        let version = get_app_version();
        assert!(!version.is_empty());
        assert_eq!(version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_open_external_url_validation() {
        assert!(open_external_url_inner("invalid-url").is_err());
        assert!(open_external_url_inner("ftp://example.com").is_err());
        assert!(open_external_url_inner("file:///etc/passwd").is_err());
    }
}


