use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{NaiveDateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::db::{DbManager, PhotoInsert};
use crate::models::{Folder, ScanProgress};
use crate::thumbnailer::{ThumbnailManager, ThumbnailScheduler};

pub struct Scanner {
    db: DbManager,
    thumbnailer: Arc<ThumbnailManager>,
    scheduler: Option<Arc<ThumbnailScheduler>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Scanner {
    pub fn new(db: DbManager, thumbnailer: Arc<ThumbnailManager>) -> Self {
        Self {
            db,
            thumbnailer,
            scheduler: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_scheduler(
        db: DbManager,
        thumbnailer: Arc<ThumbnailManager>,
        scheduler: Arc<ThumbnailScheduler>,
    ) -> Self {
        Self {
            db,
            thumbnailer,
            scheduler: Some(scheduler),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::SeqCst);
    }

    pub fn reset_cancel(&self) {
        self.cancel_flag.store(false, Ordering::SeqCst);
    }

    pub fn is_supported_image(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext.to_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "webp" | "gif" | "bmp" | "tiff" | "tif"
            )
        } else {
            false
        }
    }

    pub fn compute_file_hash(path: &Path, file_size: u64, modified_time: u64) -> String {
        let mut hasher = Sha256::new();
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(&file_size.to_le_bytes());
        hasher.update(&modified_time.to_le_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)[..20].to_string()
    }

    pub fn scan_folder<F>(
        &self,
        folder: &Folder,
        progress_cb: F,
    ) -> Result<usize, String>
    where
        F: Fn(ScanProgress) + Send + Sync + 'static,
    {
        let folder_path = PathBuf::from(&folder.path);
        if !folder_path.exists() {
            return Err(format!("Folder does not exist: {}", folder.path));
        }

        let mut discovered_files: Vec<PathBuf> = Vec::new();
        for entry in WalkDir::new(&folder_path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
            })
            .filter_map(|e| e.ok())
        {
            if self.cancel_flag.load(Ordering::Relaxed) {
                return Ok(0);
            }

            let path = entry.path();
            if path.is_file() && Self::is_supported_image(path) {
                discovered_files.push(path.to_path_buf());
            }
        }

        let total_found = discovered_files.len();
        let mut photo_inserts: Vec<PhotoInsert> = Vec::with_capacity(total_found);
        let mut thumbs_to_generate: Vec<(PathBuf, String)> = Vec::new();

        let mut scanned_count = 0;
        let batch_size = 50;

        for path in discovered_files {
            if self.cancel_flag.load(Ordering::Relaxed) {
                break;
            }

            if let Ok(metadata) = fs::metadata(&path) {
                let file_size = metadata.len() as i64;
                let mtime = metadata
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let file_hash = Self::compute_file_hash(&path, file_size as u64, mtime);
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());

                let (date_taken, camera_make, camera_model, orientation, exif_dims) =
                    Self::read_exif(&path, mtime);

                let (width, height) = match exif_dims {
                    Some(dims) => (Some(dims.0), Some(dims.1)),
                    None => {
                        ThumbnailManager::get_dimensions(&path).map(|(w, h)| (Some(w), Some(h))).unwrap_or((None, None))
                    }
                };

                photo_inserts.push(PhotoInsert {
                    folder_id: folder.id,
                    file_path: path.to_string_lossy().to_string(),
                    file_hash: file_hash.clone(),
                    file_name,
                    file_size,
                    width,
                    height,
                    date_taken,
                    camera_make,
                    camera_model,
                    orientation,
                });

                if !self.thumbnailer.has_thumbnail(&file_hash) {
                    thumbs_to_generate.push((path.clone(), file_hash));
                }

                scanned_count += 1;

                if photo_inserts.len() >= batch_size {
                    let _ = self.db.batch_upsert_photos(&photo_inserts);
                    photo_inserts.clear();

                    progress_cb(ScanProgress {
                        current_folder: folder.display_name.clone(),
                        scanned_photos: scanned_count,
                        total_found,
                        is_complete: false,
                    });
                }
            }
        }

        // Flush remaining inserts
        if !photo_inserts.is_empty() {
            let _ = self.db.batch_upsert_photos(&photo_inserts);
        }

        // Background thumbnail generation: enqueue into low priority scheduler
        if !thumbs_to_generate.is_empty() {
            if let Some(ref scheduler) = self.scheduler {
                scheduler.enqueue_low_priority(thumbs_to_generate);
            } else {
                let thumb_ref = self.thumbnailer.clone();
                std::thread::spawn(move || {
                    let _ = thumb_ref.batch_generate(&thumbs_to_generate, 384);
                });
            }
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let _ = self.db.update_folder_scanned(folder.id, scanned_count as i64, now);

        progress_cb(ScanProgress {
            current_folder: folder.display_name.clone(),
            scanned_photos: scanned_count,
            total_found,
            is_complete: true,
        });

        Ok(scanned_count)
    }

    fn read_exif(
        path: &Path,
        fallback_mtime: u64,
    ) -> (
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<u32>,
        Option<(u32, u32)>,
    ) {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return (Some((fallback_mtime * 1000) as i64), None, None, None, None),
        };

        let mut bufreader = BufReader::new(&file);
        let exifreader = exif::Reader::new();
        let exif = match exifreader.read_from_container(&mut bufreader) {
            Ok(exif) => exif,
            Err(_) => return (Some((fallback_mtime * 1000) as i64), None, None, None, None),
        };

        let mut date_taken: Option<i64> = None;
        if let Some(field) = exif
            .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
            .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY))
        {
            let date_str = field.display_value().to_string();
            // EXIF dates format typically: "YYYY-MM-DD HH:MM:SS" or "YYYY:MM:DD HH:MM:SS"
            let normalized = date_str.replace(':', "-");
            if normalized.len() >= 19 {
                let date_part = &normalized[0..10];
                let time_part = &date_str[11..19];
                let combined = format!("{} {}", date_part, time_part);
                if let Ok(dt) = NaiveDateTime::parse_from_str(&combined, "%Y-%m-%d %H:%M:%S") {
                    date_taken = Some(Utc.from_utc_datetime(&dt).timestamp_millis());
                }
            }
        }
        if date_taken.is_none() {
            date_taken = Some((fallback_mtime * 1000) as i64);
        }

        let make = exif
            .get_field(exif::Tag::Make, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string().trim_matches('"').trim().to_string());

        let model = exif
            .get_field(exif::Tag::Model, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string().trim_matches('"').trim().to_string());

        let orientation = exif
            .get_field(exif::Tag::Orientation, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));

        let width = exif
            .get_field(exif::Tag::PixelXDimension, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));
        let height = exif
            .get_field(exif::Tag::PixelYDimension, exif::In::PRIMARY)
            .and_then(|f| f.value.get_uint(0));

        let dims = match (width, height) {
            (Some(w), Some(h)) => Some((w, h)),
            _ => None,
        };

        (date_taken, make, model, orientation, dims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_scanner_folder_traversal_and_indexing() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let photos_dir = temp_dir.path().join("Photos");
        fs::create_dir_all(&photos_dir).expect("create photos dir");

        // Create 2 test image files
        let img1_path = photos_dir.join("photo1.png");
        let mut img1 = RgbImage::new(400, 300);
        for pixel in img1.pixels_mut() {
            *pixel = Rgb([10, 20, 30]);
        }
        img1.save(&img1_path).expect("save img1");

        let img2_path = photos_dir.join("photo2.png");
        let mut img2 = RgbImage::new(600, 400);
        for pixel in img2.pixels_mut() {
            *pixel = Rgb([50, 60, 70]);
        }
        img2.save(&img2_path).expect("save img2");

        // Non-image file to be ignored
        let text_path = photos_dir.join("ignore.txt");
        fs::write(&text_path, "not a photo").expect("write text");

        let db = DbManager::new_in_memory().expect("in-memory db");
        let folder_id = db
            .add_or_update_folder(
                &photos_dir.to_string_lossy(),
                "Photos",
                "always",
            )
            .expect("add folder");

        let folders = db.get_folders().expect("get folders");
        let folder = &folders[0];

        let thumb_cache_dir = temp_dir.path().join("thumbs");
        let thumbnailer = Arc::new(ThumbnailManager::with_cache_dir(thumb_cache_dir));

        let scanner = Scanner::new(db.clone(), thumbnailer.clone());
        let scanned = scanner
            .scan_folder(folder, |_prog| {})
            .expect("scan folder failed");

        assert_eq!(scanned, 2);

        let photos = db.get_photos_for_folder(folder_id).expect("get photos");
        assert_eq!(photos.len(), 2);

        // Verify thumbnails were created (async background thread)
        for p in &photos {
            for _ in 0..100 {
                if thumbnailer.has_thumbnail(&p.file_hash) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            assert!(thumbnailer.has_thumbnail(&p.file_hash));
        }
    }
}
