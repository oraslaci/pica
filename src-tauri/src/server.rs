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

    let worker_count = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(4)
        .min(16);
    for _ in 0..worker_count {
        let server = server.clone();
        let state = state.clone();
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

                let response = handle_request(&mut request, &state, &url, &method);
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
) -> Response<Cursor<Vec<u8>>> {
    let cors_header = Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    let json_header = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap();

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

        // If thumbnail does not exist yet, ensure it is queued in high-priority without wiping queue
        state.scheduler.set_high_priority_hashes(vec![hash.to_string()], false);

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

    Response::from_string("")
        .with_status_code(StatusCode(404))
        .with_header(cors_header)
}
