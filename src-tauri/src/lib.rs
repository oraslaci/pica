pub mod commands;
pub mod db;
pub mod editor;
pub mod models;
pub mod scanner;
pub mod server;
pub mod thumbnailer;

use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tauri::http::{header, Response, StatusCode};
use tauri::Emitter;

use commands::AppState;
use db::DbManager;
use editor::EditorManager;
use thumbnailer::ThumbnailManager;

pub fn initialize_default_folders(db: &DbManager) {
    if let Ok(folders) = db.get_folders() {
        if folders.is_empty() {
            if let Some(pic_dir) = dirs::picture_dir() {
                if pic_dir.exists() {
                    let path_str = pic_dir.to_string_lossy().to_string();
                    let _ = db.add_or_update_folder(&path_str, "Pictures", "always");
                }
            }
        }
    }
}

pub fn run_server_only() {
    let db = DbManager::new().expect("Failed to initialize database");
    initialize_default_folders(&db);
    let thumbnailer = Arc::new(ThumbnailManager::new());
    let editor = Arc::new(EditorManager::new());
    editor.sync_sessions_to_db(&db);
    let available_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // Multiple parallel workers for speed, capped between 2 and 4 to protect laptop CPU/battery
    let num_workers = (available_cpus / 2).clamp(2, 4);
    let scheduler = Arc::new(thumbnailer::ThumbnailScheduler::new(
        thumbnailer.clone(),
        Some(db.clone()),
        num_workers,
    ));
    let is_scanning = Arc::new(AtomicBool::new(false));

    let state = AppState {
        db,
        thumbnailer,
        scheduler,
        is_scanning,
        editor,
    };

    // Auto-scan initial folders on startup
    if let Ok(folders) = state.db.get_folders() {
        let db_clone = state.db.clone();
        let thumb_clone = state.thumbnailer.clone();
        let scheduler_clone = state.scheduler.clone();
        let is_scanning_clone = state.is_scanning.clone();
        std::thread::spawn(move || {
            is_scanning_clone.store(true, std::sync::atomic::Ordering::SeqCst);
            let scanner = scanner::Scanner::with_scheduler(db_clone, thumb_clone, scheduler_clone);
            for f in folders {
                if f.scan_mode != "exclude" {
                    let _ = scanner.scan_folder(&f, |prog| {
                        println!("[Pica Scanner] Scanned {} photos in {}", prog.scanned_photos, prog.current_folder);
                    });
                }
            }
            is_scanning_clone.store(false, std::sync::atomic::Ordering::SeqCst);
            println!("[Pica Scanner] Initial scan completed.");
        });
    }

    server::start_http_server(state, 3456);
    println!("[Pica Server] Running in standalone server mode at http://127.0.0.1:3456");
    loop {
        std::thread::park();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = DbManager::new().expect("Failed to initialize database");
    initialize_default_folders(&db);

    let thumbnailer = Arc::new(ThumbnailManager::new());
    let editor = Arc::new(EditorManager::new());
    editor.sync_sessions_to_db(&db);
    let available_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    // Multiple parallel workers for speed, capped between 2 and 4 to protect laptop CPU/battery
    let num_workers = (available_cpus / 2).clamp(2, 4);
    let scheduler = Arc::new(thumbnailer::ThumbnailScheduler::new(
        thumbnailer.clone(),
        Some(db.clone()),
        num_workers,
    ));
    let is_scanning = Arc::new(AtomicBool::new(false));

    let state = AppState {
        db,
        thumbnailer: thumbnailer.clone(),
        scheduler: scheduler.clone(),
        is_scanning,
        editor,
    };

    // Start local embedded HTTP server on 127.0.0.1:3456
    server::start_http_server(state.clone(), 3456);

    let thumb_manager_proto = thumbnailer.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .manage(state)
        .register_uri_scheme_protocol("pica", move |_app, request| {
            let path_part = request.uri().path();

            if path_part.starts_with("/thumb/") {
                let hash = path_part.trim_start_matches("/thumb/");
                let thumb_path = thumb_manager_proto.get_thumbnail_path(hash);
                if thumb_path.exists() {
                    if let Ok(bytes) = fs::read(&thumb_path) {
                        return Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "image/jpeg")
                            .header(header::CACHE_CONTROL, "no-cache")
                            .body(bytes)
                            .unwrap();
                    }
                }
            } else if let Some(query) = request.uri().query() {
                for param in query.split('&') {
                    if let Some((k, v)) = param.split_once('=') {
                        if k == "path" {
                            let decoded = percent_encoding::percent_decode_str(v)
                                .decode_utf8_lossy()
                                .to_string();
                            if let Ok(bytes) = fs::read(&decoded) {
                                let mime = if decoded.ends_with(".png") {
                                    "image/png"
                                } else if decoded.ends_with(".webp") {
                                    "image/webp"
                                } else if decoded.ends_with(".gif") {
                                    "image/gif"
                                } else {
                                    "image/jpeg"
                                };
                                return Response::builder()
                                    .status(StatusCode::OK)
                                    .header(header::CONTENT_TYPE, mime)
                                    .header(header::CACHE_CONTROL, "no-cache")
                                    .body(bytes)
                                    .unwrap();
                            }
                        }
                    }
                }
            }

            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Vec::new())
                .unwrap()
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_watch_folders,
            commands::add_watch_folder,
            commands::update_watch_folder_mode,
            commands::remove_watch_folder,
            commands::trigger_scan,
            commands::get_folders_with_photos,
            commands::get_photos_for_folder,
            commands::get_all_photos,
            commands::toggle_favorite,
            commands::get_library_stats,
            commands::set_thumbnail_priority,
            commands::get_app_settings,
            commands::update_app_settings,
            commands::get_edit_session,
            commands::apply_rotation,
            commands::undo_edit,
            commands::redo_edit,
            commands::jump_to_step,
            commands::save_edit_session,
            commands::revert_edit_session,
            commands::resolve_edit_conflict,
            commands::get_crop_suggestions,
            commands::apply_crop,
            commands::get_straighten_suggestions,
            commands::apply_straighten,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            scheduler.add_ready_callback(move |hash| {
                let _ = handle.emit("thumbnail-ready", hash);
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
