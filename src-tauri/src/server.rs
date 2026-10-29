use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread;

use serde::Deserialize;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::commands::AppState;
use crate::models::{Folder, FolderWithPhotos};
use crate::scanner::Scanner;

#[derive(Deserialize)]
struct AddFolderReq {
    path: String,
    scan_mode: String,
}

#[derive(Deserialize)]
struct UpdateModeReq {
    folder_id: i64,
    scan_mode: String,
}

#[derive(Deserialize)]
struct ToggleFavReq {
    photo_id: i64,
}

#[derive(Deserialize)]
struct PriorityThumbsReq {
    hashes: Option<Vec<String>>,
    screen: Option<Vec<String>>,
    near: Option<Vec<String>>,
    _replace: Option<bool>,
}

#[derive(Deserialize)]
struct RotateReq {
    photo_id: i64,
    degrees: i32,
}

#[derive(Deserialize)]
struct PhotoActionReq {
    photo_id: i64,
}

#[derive(Deserialize)]
struct ResolveConflictReq {
    photo_id: i64,
    choice: String,
}

#[derive(Deserialize)]
struct JumpStepReq {
    photo_id: i64,
    step_id: usize,
}

#[derive(Deserialize)]
struct CropReq {
    photo_id: i64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Deserialize)]
struct CropSuggestionsReq {
    photo_id: i64,
    aspect_ratio: Option<f64>,
}

#[derive(Deserialize)]
struct StraightenReq {
    photo_id: i64,
    angle: f64,
}

#[derive(Deserialize)]
struct StraightenSuggestionsReq {
    photo_id: i64,
}

#[derive(Deserialize)]
struct DetectRedEyesReq {
    photo_id: i64,
}

#[derive(Deserialize)]
struct RedEyeReq {
    photo_id: i64,
    boxes: Vec<crate::models::RedEyeBox>,
}

#[derive(Deserialize)]
struct RetouchReq {
    photo_id: i64,
    spots: Vec<crate::models::RetouchSpot>,
}

#[derive(Deserialize)]
struct TextReq {
    photo_id: i64,
    items: Vec<crate::models::TextItem>,
    overlay_base64: Option<String>,
}

#[derive(Deserialize)]
struct FillLightReq {
    photo_id: i64,
    amount: f32,
}

#[derive(Deserialize)]
struct FaceSettingsReq {
    enabled: Option<bool>,
}

#[derive(Deserialize)]
struct FaceTimingToggleReq {
    enabled: bool,
}

#[derive(Deserialize)]
struct FaceFolderDetectReq {
    folder_id: i64,
    enabled: bool,
}

#[derive(Deserialize)]
struct RenamePersonReq {
    person_id: i64,
    name: String,
}

pub fn start_http_server(state: AppState, port: u16) {
    let addr = format!("127.0.0.1:{}", port);
    let server = match Server::http(&addr) {
        Ok(s) => {
            println!("[Pica Server] Listening on http://{}", addr);
            Arc::new(s)
        }
        Err(e) => {
            eprintln!("[Pica Server] Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    let ready_history = Arc::new(std::sync::Mutex::new((
        0u64,
        std::collections::VecDeque::<(u64, String)>::new(),
    )));
    let ready_history_cb = ready_history.clone();
    state.scheduler.add_ready_callback(move |hash| {
        let mut lock = ready_history_cb.lock().unwrap();
        lock.0 += 1;
        let seq = lock.0;
        lock.1.push_back((seq, hash.to_string()));
        if lock.1.len() > 1000 {
            lock.1.pop_front();
        }
    });

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(4)
        .min(16);
    for _ in 0..worker_count {
        let server = server.clone();
        let state = state.clone();
        let ready_history = ready_history.clone();
        thread::spawn(move || {
            while let Ok(mut request) = server.recv() {
                let url = request.url().to_string();
                let method = request.method().clone();

                // Handle CORS preflight
                if method == Method::Options {
                    let response = Response::empty(204)
                        .with_header(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap())
                        .with_header(Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"GET, POST, DELETE, OPTIONS"[..]).unwrap())
                        .with_header(Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap());
                    let _ = request.respond(response);
                    continue;
                }

                let response = handle_request(&mut request, &state, &url, &method, &ready_history);
                let _ = request.respond(response);
            }
        });
    }
}

fn handle_request(
    request: &mut tiny_http::Request,
    state: &AppState,
    url: &str,
    method: &Method,
    ready_history: &Arc<std::sync::Mutex<(u64, std::collections::VecDeque<(u64, String)>)>>,
) -> Response<Cursor<Vec<u8>>> {
    let cors_header = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    let json_header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();

    // GET /api/thumbs/ready?since=<seq>
    if url.starts_with("/api/thumbs/ready") && method == &Method::Get {
        let mut since_seq: u64 = 0;
        if let Some(pos) = url.find("since=") {
            let s = &url[pos + 6..];
            let s = s.split('&').next().unwrap_or(s);
            since_seq = s.parse().unwrap_or(0);
        }

        let lock = ready_history.lock().unwrap();
        let cur_seq = lock.0;
        let mut ready = Vec::new();
        for (seq, hash) in &lock.1 {
            if *seq > since_seq {
                ready.push(hash.clone());
            }
        }
        let json = serde_json::json!({
            "seq": cur_seq,
            "hashes": ready,
        });
        return Response::from_string(json.to_string())
            .with_header(cors_header)
            .with_header(json_header);
    }

    // Health check
    if url == "/api/health" && method == &Method::Get {
        return Response::from_string(r#"{"status":"ok"}"#)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/folders
    if url == "/api/folders" && method == &Method::Get {
        let folders = state.db.get_folders().unwrap_or_default();
        let json = serde_json::to_string(&folders).unwrap_or_else(|_| "[]".into());
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/folders
    if url == "/api/folders" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<AddFolderReq>(&body) {
            let path_buf = PathBuf::from(&req.path);
            if path_buf.exists() && path_buf.is_dir() {
                let display_name = path_buf
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| req.path.clone());

                if let Ok(folder_id) = state.db.add_or_update_folder(&req.path, &display_name, &req.scan_mode) {
                    let folder = Folder {
                        id: folder_id,
                        path: req.path.clone(),
                        display_name,
                        scan_mode: req.scan_mode.clone(),
                        last_scanned: None,
                        photo_count: 0,
                        face_detect_enabled: true,
                    };

                    // Trigger scan
                    if req.scan_mode != "exclude" {
                        let db = state.db.clone();
                        let thumb = state.thumbnailer.clone();
                        let scheduler = state.scheduler.clone();
                        let is_scanning = state.is_scanning.clone();
                        let f_clone = folder.clone();

                        thread::spawn(move || {
                            is_scanning.store(true, Ordering::SeqCst);
                            let scanner = Scanner::with_scheduler(db, thumb, scheduler);
                            let _ = scanner.scan_folder(&f_clone, |_| {});
                            is_scanning.store(false, Ordering::SeqCst);
                        });
                    }

                    let json = serde_json::to_string(&folder).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid directory path"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/folders/mode
    if url == "/api/folders/mode" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<UpdateModeReq>(&body) {
            let _ = state.db.update_folder_mode(req.folder_id, &req.scan_mode);
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // DELETE /api/folders?id=X
    if url.starts_with("/api/folders?") && method == &Method::Delete {
        if let Some(id_str) = url.split("id=").nth(1) {
            if let Ok(id) = id_str.parse::<i64>() {
                let _ = state.db.remove_folder(id);
                return Response::from_string(r#"{"status":"ok"}"#)
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
    }

    // POST /api/scan
    if url == "/api/scan" && method == &Method::Post {
        if !state.is_scanning.load(Ordering::SeqCst) {
            let db = state.db.clone();
            let thumb = state.thumbnailer.clone();
            let scheduler = state.scheduler.clone();
            let is_scanning = state.is_scanning.clone();
            if let Ok(folders) = state.db.get_folders() {
                thread::spawn(move || {
                    is_scanning.store(true, Ordering::SeqCst);
                    let scanner = Scanner::with_scheduler(db, thumb, scheduler);
                    for f in folders {
                        if f.scan_mode != "exclude" {
                            let _ = scanner.scan_folder(&f, |_| {});
                        }
                    }
                    is_scanning.store(false, Ordering::SeqCst);
                });
            }
        }
        return Response::from_string(r#"{"status":"scanning_started"}"#)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/thumbs/priority
    if url == "/api/thumbs/priority" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PriorityThumbsReq>(&body) {
            let screen = req.screen.unwrap_or_else(|| req.hashes.unwrap_or_default());
            let near = req.near.unwrap_or_default();
            state.scheduler.set_view_priority(screen, near);
            return Response::from_string(r#"{"status":"priority_updated"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/debug/thumb-stats — count cached thumbnails and return cache size
    if url == "/api/debug/thumb-stats" && method == &Method::Get {
        let cache_dir = state.thumbnailer.cache_dir.clone();
        let (count, total_bytes) = if let Ok(entries) = fs::read_dir(&cache_dir) {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().map(|x| x == "jpg").unwrap_or(false)
                })
                .fold((0u64, 0u64), |(c, b), e| {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    (c + 1, b + size)
                })
        } else {
            (0, 0)
        };
        let json = format!(r#"{{"cached_thumbs":{},"total_bytes":{}}}"#, count, total_bytes);
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/debug/clear-thumbs — delete all cached thumbnail files
    if url == "/api/debug/clear-thumbs" && method == &Method::Post {
        state.scheduler.clear_all_jobs();
        let cache_dir = state.thumbnailer.cache_dir.clone();
        let mut deleted = 0u64;
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.extension().map(|x| x == "jpg").unwrap_or(false) {
                    if fs::remove_file(&p).is_ok() {
                        deleted += 1;
                    }
                }
            }
        }
        let json = format!(r#"{{"status":"ok","deleted":{}}}"#, deleted);
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/debug/face-timings — get timing stats and recent records
    if url.starts_with("/api/debug/face-timings") && !url.contains("export") && method == &Method::Get {
        let stats = state.faces.get_timing_stats();
        let records = state.faces.get_timing_records(Some(100));
        let response_data = serde_json::json!({
            "stats": stats,
            "records": records,
        });
        return Response::from_string(serde_json::to_string(&response_data).unwrap_or_default())
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/debug/face-timings/toggle — enable or disable timing recordings
    if url == "/api/debug/face-timings/toggle" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<FaceTimingToggleReq>(&body) {
            let _ = state.faces.set_timing_recording(req.enabled);
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/debug/face-timings/clear — clear in-memory records and disk CSV
    if url == "/api/debug/face-timings/clear" && method == &Method::Post {
        state.faces.clear_timing_records();
        return Response::from_string(r#"{"status":"ok"}"#)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/debug/face-timings/export.csv — export raw CSV
    if (url == "/api/debug/face-timings/export.csv" || url == "/api/debug/face-timings/export") && method == &Method::Get {
        let csv = state.faces.export_timing_csv();
        let csv_header = tiny_http::Header::from_bytes(
            &b"Content-Type"[..],
            &b"text/csv; charset=utf-8"[..],
        ).unwrap();
        let disposition_header = tiny_http::Header::from_bytes(
            &b"Content-Disposition"[..],
            &b"attachment; filename=\"face_scan_timings.csv\""[..],
        ).unwrap();
        return Response::from_string(csv)
            .with_header(cors_header)
            .with_header(csv_header)
            .with_header(disposition_header);
    }

    // GET /api/library (folders with photos)
    if url == "/api/library" && method == &Method::Get {
        let folders = state.db.get_folders().unwrap_or_default();
        let mut result = Vec::new();

        for folder in folders {
            if folder.scan_mode == "exclude" {
                continue;
            }
            if let Ok(mut photos) = state.db.get_photos_for_folder(folder.id) {
                for p in &mut photos {
                    p.thumbnail_path = Some(format!("http://127.0.0.1:3456/thumb/{}", p.file_hash));
                }
                if !photos.is_empty() {
                    result.push(FolderWithPhotos { folder, photos });
                }
            }
        }

        let json = serde_json::to_string(&result).unwrap_or_else(|_| "[]".into());
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/stats
    if url == "/api/stats" && method == &Method::Get {
        let is_scanning = state.is_scanning.load(Ordering::Relaxed);
        let stats = state.db.get_stats(is_scanning).unwrap_or(crate::models::LibraryStats {
            total_photos: 0,
            total_folders: 0,
            total_favorites: 0,
            is_scanning: false,
        });
        let json = serde_json::to_string(&stats).unwrap_or_default();
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/favorite
    if url == "/api/favorite" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<ToggleFavReq>(&body) {
            if let Ok(fav) = state.db.toggle_favorite(req.photo_id) {
                return Response::from_string(format!(r#"{{"is_favorite":{}}}"#, fav))
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
    }

    // GET /api/settings
    if url == "/api/settings" && method == &Method::Get {
        if let Ok(settings) = state.db.get_app_settings() {
            let json = serde_json::to_string(&settings).unwrap_or_default();
            return Response::from_string(json)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // POST /api/settings
    if url == "/api/settings" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(settings) = serde_json::from_str::<crate::models::AppSettings>(&body) {
            let _ = state.db.update_app_settings(&settings);
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // GET /api/photo/metadata?id=X or ?photo_id=X
    if (url.starts_with("/api/photo/metadata?") || url.starts_with("/api/metadata?")) && method == &Method::Get {
        let id_opt = url
            .split("photo_id=")
            .nth(1)
            .or_else(|| url.split("id=").nth(1))
            .and_then(|s| s.split('&').next())
            .and_then(|s| s.parse::<i64>().ok());

        if let Some(photo_id) = id_opt {
            if let Ok(Some(photo)) = state.db.get_photo_by_id(photo_id) {
                if let Ok(meta) = Scanner::extract_full_metadata(&photo, &state.db) {
                    let json = serde_json::to_string(&meta).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Photo or metadata not found"}"#)
            .with_status_code(StatusCode(404))
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/edit/session?photo_id=X
    if url.starts_with("/api/edit/session?") && method == &Method::Get {
        if let Some(id_str) = url.split("photo_id=").nth(1) {
            if let Ok(photo_id) = id_str.split('&').next().unwrap_or(id_str).parse::<i64>() {
                if let Ok(session) = state.editor.get_session_state(&state.db, photo_id) {
                    let json = serde_json::to_string(&session).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/rotate
    if url == "/api/edit/rotate" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<RotateReq>(&body) {
            match state.editor.apply_rotation(&state.db, &state.thumbnailer, req.photo_id, req.degrees) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/undo
    if url == "/api/edit/undo" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.undo_edit(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/redo
    if url == "/api/edit/redo" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.redo_edit(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/step
    if url == "/api/edit/step" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<JumpStepReq>(&body) {
            match state.editor.jump_to_step(&state.db, &state.thumbnailer, req.photo_id, req.step_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/save
    if url == "/api/edit/save" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.save_edit_session(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(sess) => {
                    let json = serde_json::to_string(&sess).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/revert
    if url == "/api/edit/revert" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.revert_edit_session(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/resolve_conflict
    if url == "/api/edit/resolve_conflict" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<ResolveConflictReq>(&body) {
            match state.editor.resolve_conflict(&state.db, &state.thumbnailer, req.photo_id, &req.choice) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/crop_suggestions
    if url == "/api/edit/crop_suggestions" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<CropSuggestionsReq>(&body) {
            match state.editor.get_crop_suggestions(&state.db, &state.thumbnailer, req.photo_id, req.aspect_ratio) {
                Ok(suggestions) => {
                    let json = serde_json::to_string(&suggestions).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/crop
    if url == "/api/edit/crop" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<CropReq>(&body) {
            match state.editor.apply_crop(&state.db, &state.thumbnailer, req.photo_id, req.x, req.y, req.width, req.height) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
    }

    // POST /api/edit/straighten
    if url == "/api/edit/straighten" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<StraightenReq>(&body) {
            match state.editor.apply_straighten(&state.db, &state.thumbnailer, req.photo_id, req.angle) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/straighten_suggestions
    if url == "/api/edit/straighten_suggestions" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<StraightenSuggestionsReq>(&body) {
            match state.editor.get_straighten_suggestions(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(suggestions) => {
                    let json = serde_json::to_string(&suggestions).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/detect_red_eyes
    if url == "/api/edit/detect_red_eyes" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<DetectRedEyesReq>(&body) {
            let photo_res = state.db.get_photo_by_id(req.photo_id).map_err(|e| e.to_string());
            match photo_res {
                Ok(Some(photo)) => {
                    match state.editor.auto_detect_red_eyes(&photo, state.faces.detector.as_ref()) {
                        Ok(boxes) => {
                            let json = serde_json::to_string(&boxes).unwrap_or_default();
                            return Response::from_string(json)
                                .with_header(cors_header)
                                .with_header(json_header);
                        }
                        Err(err) => {
                            return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                                .with_status_code(StatusCode(400))
                                .with_header(cors_header)
                                .with_header(json_header);
                        }
                    }
                }
                _ => {
                    return Response::from_string(r#"{"error":"Photo not found"}"#)
                        .with_status_code(StatusCode(404))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/redeye
    if url == "/api/edit/redeye" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<RedEyeReq>(&body) {
            match state.editor.apply_redeye(&state.db, &state.thumbnailer, req.photo_id, req.boxes) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/retouch
    if url == "/api/edit/retouch" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<RetouchReq>(&body) {
            match state.editor.apply_retouch(&state.db, &state.thumbnailer, req.photo_id, req.spots) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/text
    if url == "/api/edit/text" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<TextReq>(&body) {
            match state.editor.apply_text(&state.db, &state.thumbnailer, req.photo_id, req.items, req.overlay_base64) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/lucky (or /api/edit/enhance)
    if (url == "/api/edit/lucky" || url == "/api/edit/enhance") && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.apply_feeling_lucky(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/autocontrast
    if url == "/api/edit/autocontrast" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.apply_auto_contrast(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/autocolor
    if url == "/api/edit/autocolor" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<PhotoActionReq>(&body) {
            match state.editor.apply_auto_color(&state.db, &state.thumbnailer, req.photo_id) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // POST /api/edit/filllight
    if (url == "/api/edit/filllight" || url == "/api/edit/fill") && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<FillLightReq>(&body) {
            match state.editor.apply_fill_light(&state.db, &state.thumbnailer, req.photo_id, req.amount) {
                Ok(res) => {
                    let json = serde_json::to_string(&res).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
                Err(err) => {
                    return Response::from_string(format!(r#"{{"error":"{}"}}"#, err))
                        .with_status_code(StatusCode(400))
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string(r#"{"error":"Invalid request"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header);
    }

    // GET /api/faces/settings
    if url == "/api/faces/settings" && method == &Method::Get {
        let settings = state.faces.get_settings();
        let json = serde_json::to_string(&settings).unwrap_or_default();
        return Response::from_string(json)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/faces/settings
    if url == "/api/faces/settings" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<FaceSettingsReq>(&body) {
            if let Some(enabled) = req.enabled {
                let _ = state.faces.set_enabled(enabled, state.is_scanning.clone());
            }
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // POST /api/faces/folder/detect
    if url == "/api/faces/folder/detect" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<FaceFolderDetectReq>(&body) {
            let _ = state.faces.update_folder_face_detect(req.folder_id, req.enabled, state.is_scanning.clone());
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // GET /api/faces/collections
    if url == "/api/faces/collections" && method == &Method::Get {
        if let Ok(cols) = state.faces.get_collections() {
            let json = serde_json::to_string(&cols).unwrap_or_default();
            return Response::from_string(json)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // GET /api/faces/photo?id={photo_id}
    if url.starts_with("/api/faces/photo") && method == &Method::Get {
        if let Some(pos) = url.find("id=") {
            let s = &url[pos + 3..];
            let id_str = s.split('&').next().unwrap_or(s);
            if let Ok(photo_id) = id_str.parse::<i64>() {
                if let Ok(faces) = state.faces.get_faces_for_photo(photo_id) {
                    let json = serde_json::to_string(&faces).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string("[]")
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/faces/person?id={person_id}
    if url.starts_with("/api/faces/person") && method == &Method::Get {
        if let Some(pos) = url.find("id=") {
            let s = &url[pos + 3..];
            let id_str = s.split('&').next().unwrap_or(s);
            if let Ok(person_id) = id_str.parse::<i64>() {
                if let Ok(photos) = state.faces.get_photos_for_person(person_id) {
                    let json = serde_json::to_string(&photos).unwrap_or_default();
                    return Response::from_string(json)
                        .with_header(cors_header)
                        .with_header(json_header);
                }
            }
        }
        return Response::from_string("[]")
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/faces/person/rename
    if url == "/api/faces/person/rename" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        if let Ok(req) = serde_json::from_str::<RenamePersonReq>(&body) {
            let _ = state.faces.update_person_name(req.person_id, &req.name);
            return Response::from_string(r#"{"status":"ok"}"#)
                .with_header(cors_header)
                .with_header(json_header);
        }
    }

    // POST /api/faces/scan
    if url == "/api/faces/scan" && method == &Method::Post {
        state.faces.trigger_scan(state.is_scanning.clone());
        return Response::from_string(r#"{"status":"ok"}"#)
            .with_header(cors_header)
            .with_header(json_header);
    }

    // POST /api/faces/recluster
    if url == "/api/faces/recluster" && method == &Method::Post {
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        #[derive(serde::Deserialize)]
        struct ReclusterReq {
            threshold: Option<f32>,
        }
        let threshold = serde_json::from_str::<ReclusterReq>(&body)
            .ok()
            .and_then(|r| r.threshold);
        match state.faces.recluster_all_faces(threshold) {
            Ok(count) => {
                return Response::from_string(format!(r#"{{"status":"ok","people_count":{}}}"#, count))
                    .with_header(cors_header)
                    .with_header(json_header);
            }
            Err(e) => {
                return Response::from_string(format!(r#"{{"error":"{}"}}"#, e))
                    .with_status_code(StatusCode(500))
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
    }

    // POST /api/faces/realign
    if url == "/api/faces/realign" && method == &Method::Post {
        match state.faces.realign_all_embeddings() {
            Ok((updated_faces, people_count)) => {
                return Response::from_string(format!(
                    r#"{{"status":"ok","updated_faces":{},"people_count":{}}}"#,
                    updated_faces, people_count
                ))
                .with_header(cors_header)
                .with_header(json_header);
            }
            Err(e) => {
                return Response::from_string(format!(r#"{{"error":"{}"}}"#, e))
                    .with_status_code(StatusCode(500))
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
    }

    // GET /api/faces/{id}/crop
    if url.starts_with("/api/faces/") && url.ends_with("/crop") && method == &Method::Get {
        let part = url.trim_start_matches("/api/faces/").trim_end_matches("/crop");
        if let Ok(face_id) = part.parse::<i64>() {
            if let Ok(Some(bytes)) = state.faces.get_face_chip(face_id) {
                let img_header = Header::from_bytes(&b"Content-Type"[..], &b"image/jpeg"[..]).unwrap();
                let cache_header = Header::from_bytes(&b"Cache-Control"[..], &b"public, max-age=86400"[..]).unwrap();
                return Response::from_data(bytes)
                    .with_header(cors_header)
                    .with_header(img_header)
                    .with_header(cache_header);
            }
        }
        return Response::from_string("")
            .with_status_code(StatusCode(404))
            .with_header(cors_header);
    }

    // GET /thumb/{hash} (with optional ?r=1 cachebusters)
    if url.starts_with("/thumb/") {
        let path_and_query = url.trim_start_matches("/thumb/");
        let hash = path_and_query.split('?').next().unwrap_or(path_and_query);
        let thumb_path = state.thumbnailer.get_thumbnail_path(hash);

        if thumb_path.exists() {
            if let Ok(bytes) = fs::read(&thumb_path) {
                let img_header = Header::from_bytes(&b"Content-Type"[..], &b"image/jpeg"[..]).unwrap();
                let cache_header = Header::from_bytes(&b"Cache-Control"[..], &b"public, max-age=31536000, immutable"[..]).unwrap();
                return Response::from_data(bytes)
                    .with_header(cors_header)
                    .with_header(img_header)
                    .with_header(cache_header);
            }
        }

        // Return 404 immediately; view priority is governed in batches by the viewport
        let no_cache_header = Header::from_bytes(&b"Cache-Control"[..], &b"no-cache, no-store, must-revalidate"[..]).unwrap();
        return Response::from_string("")
            .with_status_code(StatusCode(404))
            .with_header(cors_header)
            .with_header(no_cache_header);
    }

    // GET /photo?path={encoded}
    if url.starts_with("/photo?") {
        if let Some(query) = url.split_once('?') {
            for param in query.1.split('&') {
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
                            let mime_header = Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
                            return Response::from_data(bytes)
                                .with_header(cors_header)
                                .with_header(mime_header);
                        }
                    }
                }
            }
        }
        return Response::from_string("")
            .with_status_code(StatusCode(404))
            .with_header(cors_header);
    }

    // GET /api/viewer/folder?path={encoded}
    if url.starts_with("/api/viewer/folder") && method == &Method::Get {
        if let Some(pos) = url.find("path=") {
            let s = &url[pos + 5..];
            let path_part = s.split('&').next().unwrap_or(s);
            let decoded = percent_encoding::percent_decode_str(path_part)
                .decode_utf8_lossy()
                .to_string();
            if let Ok(ctx) = crate::commands::get_viewer_folder_context_inner(&decoded, &state) {
                let json = serde_json::to_string(&ctx).unwrap_or_default();
                return Response::from_string(json)
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
        return Response::from_string(r#"{"error":"Failed to get viewer folder context"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header)
            .with_header(json_header);
    }

    // GET /api/viewer/metadata?path={encoded}
    if url.starts_with("/api/viewer/metadata") && method == &Method::Get {
        if let Some(pos) = url.find("path=") {
            let s = &url[pos + 5..];
            let path_part = s.split('&').next().unwrap_or(s);
            let decoded = percent_encoding::percent_decode_str(path_part)
                .decode_utf8_lossy()
                .to_string();
            if let Ok(meta) = crate::commands::get_viewer_photo_metadata_inner(&decoded, &state) {
                let json = serde_json::to_string(&meta).unwrap_or_default();
                return Response::from_string(json)
                    .with_header(cors_header)
                    .with_header(json_header);
            }
        }
        return Response::from_string(r#"{"error":"Failed to get metadata"}"#)
            .with_status_code(StatusCode(400))
            .with_header(cors_header)
            .with_header(json_header);
    }

    Response::from_string("")
        .with_status_code(StatusCode(404))
        .with_header(cors_header)
}
