pub mod clustering;
pub mod detector;
pub mod embedder;
pub mod model_manager;
pub mod timings;
pub mod traits;
pub mod worker;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::db::DbManager;
use crate::models::{FaceBox, FaceCollection, FaceScanProgress, FaceSettings, Photo};
use clustering::SowGrowFaceClusterer;
use detector::YuNetFaceDetector;
use embedder::CompositeFaceEmbedder;
use model_manager::{ModelManager, TOTAL_MODEL_SIZE_MB};
pub use timings::{FaceScanTimingRecord, FaceTimingRecorder, FaceTimingStats};
use worker::FaceWorker;

type FaceProgressCb = Arc<Mutex<Option<Box<dyn Fn(FaceScanProgress) + Send + Sync + 'static>>>>;

#[derive(Clone)]
pub struct FaceManager {
    db: DbManager,
    pub detector: Arc<YuNetFaceDetector>,
    embedder: Arc<CompositeFaceEmbedder>,
    pub clusterer: Arc<SowGrowFaceClusterer>,
    model_manager: Arc<ModelManager>,
    worker: Arc<FaceWorker>,
    pub timings: Arc<FaceTimingRecorder>,
    is_enabled: Arc<AtomicBool>,
    progress_callback: FaceProgressCb,
}

impl FaceManager {
    pub fn new(db: DbManager) -> Self {
        let model_mgr = Arc::new(ModelManager::new());
        let emb_path = model_mgr.get_embedder_path();

        let detector = Arc::new(YuNetFaceDetector::new());
        let embedder = Arc::new(CompositeFaceEmbedder::new(emb_path));
        embedder.set_available(true);
        let clusterer = Arc::new(SowGrowFaceClusterer::new());
        let worker = Arc::new(FaceWorker::new());

        let timing_setting = db
            .get_setting("debug_record_face_timings")
            .unwrap_or(None)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        let timings = Arc::new(FaceTimingRecorder::new(timing_setting));

        let enabled_setting = db
            .get_setting("faces_enabled")
            .unwrap_or(None)
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        Self {
            db,
            detector,
            embedder,
            clusterer,
            model_manager: model_mgr,
            worker,
            timings,
            is_enabled: Arc::new(AtomicBool::new(enabled_setting)),
            progress_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_progress_callback<F>(&self, cb: F)
    where
        F: Fn(FaceScanProgress) + Send + Sync + 'static,
    {
        if let Ok(mut lock) = self.progress_callback.lock() {
            *lock = Some(Box::new(cb));
        }
    }

    pub fn start_background_service(&self, is_library_scanning: Arc<AtomicBool>) {
        if self.is_enabled.load(Ordering::Relaxed) {
            let cb_ref = self.progress_callback.clone();
            self.worker.start(
                self.db.clone(),
                self.detector.clone(),
                self.embedder.clone(),
                self.clusterer.clone(),
                is_library_scanning,
                self.timings.clone(),
                move |prog| {
                    if let Ok(lock) = cb_ref.lock() {
                        if let Some(ref cb) = *lock {
                            cb(prog);
                        }
                    }
                },
                || {},
            );
        }
    }

    pub fn get_settings(&self) -> FaceSettings {
        let (total_faces, total_people, pending_photos) = self.db.get_face_counts().unwrap_or((0, 0, 0));
        let models_installed = self.model_manager.are_models_installed();
        let is_downloading = self.model_manager.is_downloading();
        let download_progress = self.model_manager.get_download_progress();
        let enabled = self.is_enabled.load(Ordering::Relaxed);
        let cluster_threshold = self.db.get_setting("face_cluster_threshold")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.50);

        FaceSettings {
            enabled,
            models_installed,
            is_downloading,
            download_progress,
            model_size_mb: TOTAL_MODEL_SIZE_MB,
            pending_photos,
            total_faces,
            total_people,
            cluster_threshold,
        }
    }

    pub fn set_enabled(&self, enabled: bool, is_library_scanning: Arc<AtomicBool>) -> Result<(), String> {
        self.is_enabled.store(enabled, Ordering::SeqCst);
        let _ = self.db.set_setting("faces_enabled", if enabled { "true" } else { "false" });

        if enabled {
            self.worker.resume();
            self.start_background_service(is_library_scanning);
        } else {
            self.worker.pause();
        }

        Ok(())
    }

    pub fn get_collections(&self) -> Result<Vec<FaceCollection>, String> {
        self.db.get_face_collections().map_err(|e| e.to_string())
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<FaceBox>, String> {
        self.db.get_faces_for_photo(photo_id).map_err(|e| e.to_string())
    }

    pub fn get_photos_for_person(&self, person_id: i64) -> Result<Vec<Photo>, String> {
        self.db.get_photos_for_person(person_id).map_err(|e| e.to_string())
    }

    pub fn update_person_name(&self, person_id: i64, name: &str) -> Result<(), String> {
        self.db.update_person_name(person_id, name).map_err(|e| e.to_string())
    }

    pub fn update_folder_face_detect(
        &self,
        folder_id: i64,
        enabled: bool,
        is_library_scanning: Arc<AtomicBool>,
    ) -> Result<(), String> {
        self.db
            .update_folder_face_detect(folder_id, enabled)
            .map_err(|e| e.to_string())?;

        if enabled {
            self.is_enabled.store(true, Ordering::SeqCst);
            let _ = self.db.set_setting("faces_enabled", "true");
            self.worker.resume();
            self.start_background_service(is_library_scanning);
            self.worker.wake();
        } else {
            self.worker.wake();
        }

        Ok(())
    }

    pub fn get_face_chip(&self, face_id: i64) -> Result<Option<Vec<u8>>, String> {
        self.db.get_face_chip_bytes(face_id).map_err(|e| e.to_string())
    }

    pub fn trigger_scan(&self, is_library_scanning: Arc<AtomicBool>) {
        self.is_enabled.store(true, Ordering::SeqCst);
        let _ = self.db.set_setting("faces_enabled", "true");
        self.worker.resume();
        self.start_background_service(is_library_scanning);
        self.worker.wake();
    }

    pub fn wake(&self) {
        self.worker.wake();
    }

    pub fn set_timing_recording(&self, enabled: bool) -> Result<(), String> {
        self.timings.set_recording(enabled);
        let _ = self.db.set_setting("debug_record_face_timings", if enabled { "true" } else { "false" });
        Ok(())
    }

    pub fn get_timing_stats(&self) -> FaceTimingStats {
        self.timings.get_stats()
    }

    pub fn get_timing_records(&self, limit: Option<usize>) -> Vec<FaceScanTimingRecord> {
        self.timings.get_records(limit)
    }

    pub fn clear_timing_records(&self) {
        self.timings.clear();
    }

    pub fn export_timing_csv(&self) -> String {
        self.timings.export_csv()
    }

    pub fn recluster_all_faces(&self, threshold: Option<f32>) -> Result<usize, String> {
        let thresh = threshold.or_else(|| {
            self.db.get_setting("face_cluster_threshold")
                .ok()
                .flatten()
                .and_then(|s| s.parse::<f32>().ok())
            }).unwrap_or(0.50);
        self.db.recluster_all_faces(thresh).map_err(|e| e.to_string())
    }

    pub fn realign_all_embeddings(&self) -> Result<(usize, usize), String> {
        use traits::{FaceDetector, FaceEmbedder};

        let faces_with_photos = self.db.get_faces_for_realignment().map_err(|e| e.to_string())?;

        if faces_with_photos.is_empty() {
            return Ok((0, 0));
        }

        let mut photos_map: std::collections::HashMap<i64, (String, String, Vec<(i64, f32, f32, f32, f32)>)> =
            std::collections::HashMap::new();
        for (fid, pid, path, hash, bx, by, bw, bh) in faces_with_photos {
            photos_map.entry(pid).or_insert_with(|| (path, hash, Vec::new())).2.push((fid, bx, by, bw, bh));
        }

        let thumb_cache_dir = dirs::cache_dir().map(|d| d.join("pica").join("thumbnails"));
        let mut updates: Vec<(i64, Vec<f32>)> = Vec::new();

        for (_pid, (file_path, file_hash, db_faces)) in photos_map {
            let mut working_img = None;
            let mut is_thumb = false;
            if let Some(ref thumb_dir) = thumb_cache_dir {
                let p_webp = thumb_dir.join(format!("{}.webp", file_hash));
                let p_jpg = thumb_dir.join(format!("{}.jpg", file_hash));
                if p_webp.exists() {
                    working_img = image::open(&p_webp).ok();
                    is_thumb = true;
                } else if p_jpg.exists() {
                    working_img = image::open(&p_jpg).ok();
                    is_thumb = true;
                }
            }
            if working_img.is_none() {
                working_img = image::open(&file_path).ok();
                is_thumb = false;
            }

            let Some(img) = working_img else {
                continue;
            };

            let detected = match self.detector.detect_faces(&img) {
                Ok(d) => d,
                Err(_) => continue,
            };

            if detected.is_empty() {
                continue;
            }

            let mut full_img_cache: Option<image::DynamicImage> = None;

            for (fid, bx, by, bw, bh) in db_faces {
                let db_cx = bx + bw * 0.5;
                let db_cy = by + bh * 0.5;

                let mut best_det: Option<&crate::faces::traits::DetectedFace> = None;
                let mut best_dist = f32::MAX;

                for det in &detected {
                    let det_cx = det.box_x + det.box_w * 0.5;
                    let det_cy = det.box_y + det.box_h * 0.5;
                    let dist = (db_cx - det_cx).hypot(db_cy - det_cy);
                    if dist < best_dist && dist < 0.25 {
                        best_dist = dist;
                        best_det = Some(det);
                    }
                }

                if let Some(det) = best_det {
                    if let Some(ref lms) = det.landmarks {
                        let aligned = if is_thumb && (det.box_w * (img.width() as f32) < 80.0) {
                            if full_img_cache.is_none() {
                                full_img_cache = image::open(&file_path).ok();
                            }
                            if let Some(ref full_img) = full_img_cache {
                                crate::faces::embedder::align_face_chip(full_img, lms)
                            } else {
                                crate::faces::embedder::align_face_chip(&img, lms)
                            }
                        } else {
                            crate::faces::embedder::align_face_chip(&img, lms)
                        };

                        if let Ok(emb) = self.embedder.compute_embedding(&aligned) {
                            updates.push((fid, emb));
                        }
                    }
                }
            }
        }

        let updated_count = updates.len();
        self.db.update_face_embeddings(&updates).map_err(|e| e.to_string())?;

        let thresh = self.db.get_setting("face_cluster_threshold")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(0.50);
        let people_count = self.db.recluster_all_faces(thresh).map_err(|e| e.to_string())?;

        Ok((updated_count, people_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{FaceDetector, FaceEmbedder};
    use image::{Rgba, RgbaImage};

    #[test]
    fn test_faces_module_pipeline() {
        let db = DbManager::new_in_memory().expect("in memory db");
        let face_mgr = FaceManager::new(db);

        // 1. Initial settings (defaults to true)
        let settings = face_mgr.get_settings();
        assert_eq!(settings.enabled, true);
        assert_eq!(settings.model_size_mb, 13.8);

        // 2. Disable then re-enable
        face_mgr.set_enabled(false, Arc::new(AtomicBool::new(false))).expect("set disabled");
        assert_eq!(face_mgr.get_settings().enabled, false);

        face_mgr.set_enabled(true, Arc::new(AtomicBool::new(false))).expect("set enabled");
        assert_eq!(face_mgr.get_settings().enabled, true);

        // 3. Test detector and embedder with synthetic image
        let mut img = RgbaImage::new(128, 128);
        for y in 0..128 {
            for x in 0..128 {
                // Draw a synthetic face-like skin patch in the center
                if x >= 32 && x <= 96 && y >= 32 && y <= 96 {
                    img.put_pixel(x, y, Rgba([210, 160, 130, 255]));
                } else {
                    img.put_pixel(x, y, Rgba([30, 30, 30, 255]));
                }
            }
        }
        let dyn_img = image::DynamicImage::ImageRgba8(img);

        let detector = detector::YuNetFaceDetector::new();
        let faces = detector.detect_faces(&dyn_img).expect("detect faces");
        // Synthetic skin patch has no facial geometry, so YuNet properly returns 0 false faces
        assert_eq!(faces.len(), 0);

        let embedder = embedder::HighAccuracyFaceEmbedder::new(std::path::PathBuf::from("models/arcface_mbf.onnx"));
        let emb = embedder.compute_embedding(&dyn_img).expect("compute embedding");
        assert_eq!(emb.len(), 512);

        // Verify L2 normalization: sum(emb_i^2) ≈ 1.0
        let norm_sq: f32 = emb.iter().map(|v| v * v).sum();
        assert!((norm_sq - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_face_progress_callback() {
        let db = DbManager::new_in_memory().expect("in memory db");
        let face_mgr = FaceManager::new(db);
        let progress_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = progress_received.clone();
        face_mgr.set_progress_callback(move |prog| {
            if prog.is_complete {
                flag.store(true, Ordering::SeqCst);
            }
        });

        // Trigger callback via registered closure
        if let Ok(lock) = face_mgr.progress_callback.lock() {
            if let Some(ref cb) = *lock {
                cb(FaceScanProgress {
                    current_photo: Some("test.jpg".to_string()),
                    processed: 1,
                    pending: 0,
                    faces_found: 1,
                    is_complete: true,
                });
            }
        }
        assert!(progress_received.load(Ordering::SeqCst));
    }

    #[test]
    fn test_face_timing_controls() {
        let db = DbManager::new_in_memory().expect("in memory db");
        let face_mgr = FaceManager::new(db);

        assert_eq!(face_mgr.get_timing_stats().is_recording, false);
        face_mgr.set_timing_recording(true).expect("set timing true");
        assert_eq!(face_mgr.get_timing_stats().is_recording, true);

        // Record a mock timing
        face_mgr.timings.record(timings::FaceScanTimingRecord {
            photo_id: 42,
            file_name: "test.jpg".to_string(),
            file_hash: "hash42".to_string(),
            worker_id: 0,
            is_thumbnail: true,
            width: 256,
            height: 256,
            decode_ms: 1.2,
            detect_ms: 14.5,
            faces_detected: 1,
            embed_ms: 22.0,
            cluster_ms: 0.3,
            db_save_ms: 1.1,
            total_ms: 39.1,
            timestamp_ms: 1000,
        });

        let stats = face_mgr.get_timing_stats();
        assert_eq!(stats.record_count, 1);
        assert_eq!(stats.avg_detect_ms, 14.5);

        let records = face_mgr.get_timing_records(Some(10));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].photo_id, 42);

        let csv = face_mgr.export_timing_csv();
        assert!(csv.contains("hash42"));

        face_mgr.clear_timing_records();
        assert_eq!(face_mgr.get_timing_stats().record_count, 0);
    }

    #[test]
    fn test_average_linkage_reclustering() {
        let db = DbManager::new_in_memory().expect("in memory db");
        // Test reclustering on empty DB
        let res = db.recluster_all_faces(0.50).expect("recluster empty");
        assert_eq!(res, 0);
    }
}

