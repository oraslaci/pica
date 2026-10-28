use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use crate::db::{DbManager, FaceInsertData};
use crate::models::FaceScanProgress;
use super::timings::{FaceScanTimingRecord, FaceTimingRecorder};
use super::traits::{FaceClusterer, FaceDetector, FaceEmbedder};

struct ActivePhotoGuard {
    photo_id: i64,
    active_set: Arc<Mutex<std::collections::HashSet<i64>>>,
}

impl Drop for ActivePhotoGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = self.active_set.lock() {
            set.remove(&self.photo_id);
        }
    }
}

pub struct FaceWorker {
    is_running: Arc<AtomicBool>,
    is_paused: Arc<AtomicBool>,
    wake_cond: Arc<(Mutex<bool>, Condvar)>,
}

impl FaceWorker {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(false)),
            is_paused: Arc::new(AtomicBool::new(false)),
            wake_cond: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    pub fn pause(&self) {
        self.is_paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.is_paused.store(false, Ordering::SeqCst);
        self.wake();
    }

    pub fn wake(&self) {
        let (lock, cvar) = &*self.wake_cond;
        if let Ok(mut triggered) = lock.lock() {
            *triggered = true;
            cvar.notify_all();
        }
    }

    pub fn start<D, E, C, P, U>(
        &self,
        db: DbManager,
        detector: Arc<D>,
        embedder: Arc<E>,
        clusterer: Arc<C>,
        is_library_scanning: Arc<AtomicBool>,
        timing_recorder: Arc<FaceTimingRecorder>,
        progress_cb: P,
        updated_cb: U,
    ) where
        D: FaceDetector + 'static,
        E: FaceEmbedder + 'static,
        C: FaceClusterer + 'static,
        P: Fn(FaceScanProgress) + Send + Sync + 'static,
        U: Fn() + Send + Sync + 'static,
    {
        if self.is_running.swap(true, Ordering::SeqCst) {
            // Already running
            return;
        }

        let is_running_flag = self.is_running.clone();
        let is_paused_flag = self.is_paused.clone();
        let wake_cond_flag = self.wake_cond.clone();

        let progress_cb = Arc::new(progress_cb);
        let updated_cb = Arc::new(updated_cb);
        let active_photo_ids = Arc::new(Mutex::new(std::collections::HashSet::<i64>::new()));
        let shared_stats = Arc::new(Mutex::new((0usize, 0usize))); // (processed_in_run, faces_found_in_run)

        let num_workers = 2;
        let active_threads = Arc::new(std::sync::atomic::AtomicUsize::new(num_workers));

        for worker_idx in 0..num_workers {
            let is_running_clone = is_running_flag.clone();
            let is_paused_clone = is_paused_flag.clone();
            let wake_cond_clone = wake_cond_flag.clone();
            let db_clone = db.clone();
            let detector_clone = detector.clone();
            let embedder_clone = embedder.clone();
            let clusterer_clone = clusterer.clone();
            let is_library_scanning_clone = is_library_scanning.clone();
            let progress_cb_clone = progress_cb.clone();
            let updated_cb_clone = updated_cb.clone();
            let active_photo_ids_clone = active_photo_ids.clone();
            let shared_stats_clone = shared_stats.clone();
            let active_threads_clone = active_threads.clone();
            let timing_recorder_clone = timing_recorder.clone();

            thread::Builder::new()
                .name(format!("pica-face-worker-{}", worker_idx))
                .spawn(move || {
                    while is_running_clone.load(Ordering::Relaxed) {
                        // Check if paused or if higher-priority library scan is running
                        if is_paused_clone.load(Ordering::Relaxed) || is_library_scanning_clone.load(Ordering::Relaxed) {
                            let (lock, cvar) = &*wake_cond_clone;
                            if let Ok(guard) = lock.lock() {
                                let _ = cvar.wait_timeout(guard, Duration::from_millis(300));
                            }
                            continue;
                        }

                        // Claim a pending photo not currently claimed by any other worker
                        let candidate_photos = match db_clone.get_pending_face_photos(8) {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("[Pica Faces] Error fetching pending photos: {}", e);
                                thread::sleep(Duration::from_millis(1000));
                                continue;
                            }
                        };

                        let claimed_photo = {
                            let mut active = active_photo_ids_clone.lock().unwrap();
                            let mut chosen = None;
                            for p in candidate_photos {
                                if !active.contains(&p.id) {
                                    active.insert(p.id);
                                    chosen = Some(p);
                                    break;
                                }
                            }
                            chosen
                        };

                        let photo = match claimed_photo {
                            Some(p) => p,
                            None => {
                                let (processed, faces_found) = {
                                    let mut st = shared_stats_clone.lock().unwrap();
                                    let cur = *st;
                                    if cur.0 > 0 {
                                        *st = (0, 0);
                                    }
                                    cur
                                };

                                if processed > 0 {
                                    progress_cb_clone(FaceScanProgress {
                                        current_photo: None,
                                        processed,
                                        pending: 0,
                                        faces_found,
                                        is_complete: true,
                                    });
                                }

                                // Wait for wake notification or periodic poll
                                let (lock, cvar) = &*wake_cond_clone;
                                if let Ok(mut guard) = lock.lock() {
                                    *guard = false;
                                    let _ = cvar.wait_timeout(guard, Duration::from_millis(1500));
                                }
                                continue;
                            }
                        };

                        let _guard = ActivePhotoGuard {
                            photo_id: photo.id,
                            active_set: active_photo_ids_clone.clone(),
                        };

                        if !is_running_clone.load(Ordering::Relaxed) || is_library_scanning_clone.load(Ordering::Relaxed) {
                            continue;
                        }

                        // Yield CPU quantum cooperatively between photos
                        thread::sleep(Duration::from_millis(5));

                        let (_, _, total_pending) = db_clone.get_face_counts().unwrap_or((0, 0, 0));

                            let start_total = std::time::Instant::now();

                            // 1. Check if cached thumbnail exists on disk (loads in <1ms)
                            let start_decode = std::time::Instant::now();
                            let thumb_path = dirs::cache_dir()
                                .unwrap_or_else(|| std::path::PathBuf::from("."))
                                .join("pica")
                                .join("thumbnails")
                                .join(format!("{}.jpg", photo.file_hash));

                            let (working_img, is_thumb) = if thumb_path.exists() {
                                if let Ok(t_img) = image::open(&thumb_path) {
                                    if t_img.width() >= 480 || t_img.height() >= 480 {
                                        (Some(t_img), true)
                                    } else {
                                        // 160x120 thumbnail is too small to resolve small faces; load original photo
                                        (image::open(&photo.file_path).ok(), false)
                                    }
                                } else {
                                    (image::open(&photo.file_path).ok(), false)
                                }
                            } else {
                                (image::open(&photo.file_path).ok(), false)
                            };
                            let decode_ms = start_decode.elapsed().as_secs_f64() * 1000.0;

                            if let Some(working_img) = working_img {
                                let (img_w, img_h) = (working_img.width(), working_img.height());

                                let start_detect = std::time::Instant::now();
                                let detected = match detector_clone.detect_faces(&working_img) {
                                    Ok(faces) => faces,
                                    Err(e) => {
                                        eprintln!("[Pica Faces] Detection error on {}: {}", photo.file_name, e);
                                        Vec::new()
                                    }
                                };
                                let detect_ms = start_detect.elapsed().as_secs_f64() * 1000.0;

                                let mut face_inserts = Vec::new();
                                let img_wf = img_w as f32;
                                let img_hf = img_h as f32;

                                let mut existing_clusters = db_clone.get_person_centroids().unwrap_or_default();
                                let mut full_img_cache: Option<image::DynamicImage> = None;

                                let start_embed = std::time::Instant::now();
                                let mut cluster_time_accum = 0.0;

                                let mut used_person_ids_in_photo = std::collections::HashSet::new();

                                for face in detected {
                                    // 1:1 Aspect-ratio preserving square crop with 10% margin centered on face
                                    let cx = face.box_x + face.box_w * 0.5;
                                    let cy = face.box_y + face.box_h * 0.5;
                                    let max_dim = face.box_w.max(face.box_h) * 1.10;
                                    let half = max_dim * 0.5;

                                    let chip_x = ((cx - half).max(0.0) * img_wf) as u32;
                                    let chip_y = ((cy - half).max(0.0) * img_hf) as u32;
                                    let chip_w = (((cx + half).min(1.0) * img_wf) as u32)
                                        .saturating_sub(chip_x)
                                        .min(img_w.saturating_sub(chip_x));
                                    let chip_h = (((cy + half).min(1.0) * img_hf) as u32)
                                        .saturating_sub(chip_y)
                                        .min(img_h.saturating_sub(chip_y));

                                    let embedding = if let Some(ref lms) = face.landmarks {
                                        if is_thumb && (chip_w < 80 || chip_h < 80) {
                                            if full_img_cache.is_none() {
                                                full_img_cache = image::open(&photo.file_path).ok();
                                            }
                                            if let Some(ref full_img) = full_img_cache {
                                                let chip = crate::faces::embedder::align_face_chip(full_img, lms);
                                                embedder_clone.compute_embedding(&chip).ok()
                                            } else {
                                                let chip = crate::faces::embedder::align_face_chip(&working_img, lms);
                                                embedder_clone.compute_embedding(&chip).ok()
                                            }
                                        } else {
                                            let chip = crate::faces::embedder::align_face_chip(&working_img, lms);
                                            embedder_clone.compute_embedding(&chip).ok()
                                        }
                                    } else if chip_w >= 80 && chip_h >= 80 {
                                        let chip = working_img.crop_imm(chip_x, chip_y, chip_w, chip_h);
                                        embedder_clone.compute_embedding(&chip).ok()
                                    } else if chip_w > 8 && chip_h > 8 {
                                        if is_thumb {
                                            if full_img_cache.is_none() {
                                                full_img_cache = image::open(&photo.file_path).ok();
                                            }
                                            if let Some(ref full_img) = full_img_cache {
                                                let fw = full_img.width() as f32;
                                                let fh = full_img.height() as f32;
                                                let fx = ((cx - half).max(0.0) * fw) as u32;
                                                let fy = ((cy - half).max(0.0) * fh) as u32;
                                                let f_w = (((cx + half).min(1.0) * fw) as u32)
                                                    .saturating_sub(fx)
                                                    .min(full_img.width().saturating_sub(fx));
                                                let f_h = (((cy + half).min(1.0) * fh) as u32)
                                                    .saturating_sub(fy)
                                                    .min(full_img.height().saturating_sub(fy));
                                                if f_w > 8 && f_h > 8 {
                                                    let chip = full_img.crop_imm(fx, fy, f_w, f_h);
                                                    embedder_clone.compute_embedding(&chip).ok()
                                                } else {
                                                    let chip = working_img.crop_imm(chip_x, chip_y, chip_w, chip_h);
                                                    embedder_clone.compute_embedding(&chip).ok()
                                                }
                                            } else {
                                                let chip = working_img.crop_imm(chip_x, chip_y, chip_w, chip_h);
                                                embedder_clone.compute_embedding(&chip).ok()
                                            }
                                        } else {
                                            let chip = working_img.crop_imm(chip_x, chip_y, chip_w, chip_h);
                                            embedder_clone.compute_embedding(&chip).ok()
                                        }
                                    } else {
                                        None
                                    };

                                    let start_cluster = std::time::Instant::now();
                                    let mut assigned_person_id = None;
                                    if let Some(ref emb) = embedding {
                                        // Enforce cannot-link constraint: do not match a person already present in this photo
                                        let available_clusters: Vec<_> = existing_clusters
                                            .iter()
                                            .filter(|(pid, _)| !used_person_ids_in_photo.contains(pid))
                                            .cloned()
                                            .collect();

                                        let cluster_threshold: f32 = db_clone
                                            .get_setting("face_cluster_threshold")
                                            .ok()
                                            .flatten()
                                            .and_then(|s| s.parse::<f32>().ok())
                                            .unwrap_or(0.50);

                                        if let Some(pid) = clusterer_clone.find_matching_cluster(emb, &available_clusters, cluster_threshold) {
                                            assigned_person_id = Some(pid);
                                            used_person_ids_in_photo.insert(pid);
                                        } else {
                                            let next_id = db_clone.get_next_person_number().unwrap_or(1);
                                            let person_name = format!("Person {}", next_id);
                                            if let Ok(new_pid) = db_clone.create_face_person(&person_name, None) {
                                                assigned_person_id = Some(new_pid);
                                                used_person_ids_in_photo.insert(new_pid);
                                                existing_clusters.push((new_pid, emb.clone()));
                                            }
                                        }
                                    }
                                    cluster_time_accum += start_cluster.elapsed().as_secs_f64() * 1000.0;

                                    face_inserts.push(FaceInsertData {
                                        box_x: face.box_x,
                                        box_y: face.box_y,
                                        box_w: face.box_w,
                                        box_h: face.box_h,
                                        confidence: face.confidence,
                                        embedding,
                                        person_id: assigned_person_id,
                                    });
                                }

                                let total_embed_duration_ms = start_embed.elapsed().as_secs_f64() * 1000.0;
                                let embed_ms = (total_embed_duration_ms - cluster_time_accum).max(0.0);
                                let cluster_ms = cluster_time_accum;

                                let faces_in_photo = face_inserts.len();
                                let start_db = std::time::Instant::now();
                                let _ = db_clone.save_detected_faces(photo.id, &face_inserts);
                                let db_save_ms = start_db.elapsed().as_secs_f64() * 1000.0;
                                let total_ms = start_total.elapsed().as_secs_f64() * 1000.0;

                                if timing_recorder_clone.is_recording() {
                                    let timestamp_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);

                                    timing_recorder_clone.record(FaceScanTimingRecord {
                                        photo_id: photo.id,
                                        file_name: photo.file_name.clone(),
                                        file_hash: photo.file_hash.clone(),
                                        worker_id: worker_idx,
                                        is_thumbnail: is_thumb,
                                        width: img_w,
                                        height: img_h,
                                        decode_ms: (decode_ms * 100.0).round() / 100.0,
                                        detect_ms: (detect_ms * 100.0).round() / 100.0,
                                        faces_detected: faces_in_photo,
                                        embed_ms: (embed_ms * 100.0).round() / 100.0,
                                        cluster_ms: (cluster_ms * 100.0).round() / 100.0,
                                        db_save_ms: (db_save_ms * 100.0).round() / 100.0,
                                        total_ms: (total_ms * 100.0).round() / 100.0,
                                        timestamp_ms,
                                    });
                                }

                                let (p_count, f_count) = {
                                    let mut st = shared_stats_clone.lock().unwrap();
                                    st.0 += 1;
                                    st.1 += faces_in_photo;
                                    *st
                                };

                                let remaining = (total_pending as usize).saturating_sub(1);
                                progress_cb_clone(FaceScanProgress {
                                    current_photo: Some(photo.file_name.clone()),
                                    processed: p_count,
                                    pending: remaining,
                                    faces_found: f_count,
                                    is_complete: false,
                                });

                                if faces_in_photo > 0 {
                                    updated_cb_clone();
                                }
                            } else {
                                let start_db = std::time::Instant::now();
                                let _ = db_clone.save_detected_faces(photo.id, &[]);
                                let db_save_ms = start_db.elapsed().as_secs_f64() * 1000.0;
                                let total_ms = start_total.elapsed().as_secs_f64() * 1000.0;

                                if timing_recorder_clone.is_recording() {
                                    let timestamp_ms = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);

                                    timing_recorder_clone.record(FaceScanTimingRecord {
                                        photo_id: photo.id,
                                        file_name: photo.file_name.clone(),
                                        file_hash: photo.file_hash.clone(),
                                        worker_id: worker_idx,
                                        is_thumbnail: false,
                                        width: 0,
                                        height: 0,
                                        decode_ms: (decode_ms * 100.0).round() / 100.0,
                                        detect_ms: 0.0,
                                        faces_detected: 0,
                                        embed_ms: 0.0,
                                        cluster_ms: 0.0,
                                        db_save_ms: (db_save_ms * 100.0).round() / 100.0,
                                        total_ms: (total_ms * 100.0).round() / 100.0,
                                        timestamp_ms,
                                    });
                                }

                                let mut st = shared_stats_clone.lock().unwrap();
                                st.0 += 1;
                            }
                        }

                    if active_threads_clone.fetch_sub(1, Ordering::SeqCst) == 1 {
                        is_running_clone.store(false, Ordering::SeqCst);
                    }
                })
                .expect("Failed to spawn face worker thread");
        }
    }
}
