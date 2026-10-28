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
use crate::models::{ExifTagEntry, Folder, Photo, PhotoMetadata, ScanProgress};
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
                let display_file_name = file_name.clone();

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
                }

                // Smoothly report progress: every 5 photos, or at boundaries, or for small folders
                if scanned_count % 5 == 0 || scanned_count == total_found || total_found <= 20 {
                    progress_cb(ScanProgress {
                        current_folder: folder.display_name.clone(),
                        current_file: Some(display_file_name),
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
                    let _ = thumb_ref.batch_generate(&thumbs_to_generate, crate::thumbnailer::DEFAULT_THUMBNAIL_SIZE);
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
            current_file: None,
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

    pub fn extract_full_metadata(photo: &Photo, db: &DbManager) -> Result<PhotoMetadata, String> {
        let path = Path::new(&photo.file_path);
        let location = path
            .parent()
            .map(|p| {
                let mut s = p.to_string_lossy().to_string();
                if !s.ends_with('/') {
                    s.push('/');
                }
                s
            })
            .unwrap_or_else(|| "/".to_string());

        let fs_meta = fs::metadata(path).ok();
        let file_size = fs_meta.as_ref().map(|m| m.len() as i64).unwrap_or(photo.file_size);
        let file_size_formatted = format_file_size(file_size);

        let file_date = fs_meta.and_then(|m| m.modified().ok()).map(|mtime| {
            let dt = chrono::DateTime::<Utc>::from(mtime);
            dt.format("%m/%d/%Y %I:%M:%S %p").to_string()
        });

        // Query people tags from faces table
        let mut people_tags = Vec::new();
        if let Ok(faces) = db.get_faces_for_photo(photo.id) {
            let mut seen = std::collections::HashSet::new();
            for f in faces {
                if let Some(name) = f.person_name {
                    if !name.trim().is_empty() && seen.insert(name.clone()) {
                        people_tags.push(name);
                    }
                }
            }
        }

        // Try reading EXIF
        let mut camera_make = photo.camera_make.clone();
        let mut camera_model = photo.camera_model.clone();
        let mut camera_date = None;
        let mut digitized_date = None;
        let mut modified_date = None;
        let mut orientation_str = photo.orientation.map(|o| match o {
            1 => "Normal (0 deg)".to_string(),
            3 => "Rotated 180 Degrees".to_string(),
            6 => "Rotated 90 Degrees".to_string(),
            8 => "Rotated 270 Degrees".to_string(),
            other => format!("Orientation {}", other),
        });
        let mut flash = None;
        let mut lens = None;
        let mut focal_length = None;
        let mut focal_length_35mm = None;
        let mut ccd_width = None;
        let mut exposure_time = None;
        let mut aperture = None;
        let mut f_number = None;
        let mut iso = None;
        let mut white_balance = None;
        let mut metering_mode = None;
        let mut exposure_program = None;
        let mut color_space = None;
        let mut x_resolution = None;
        let mut y_resolution = None;
        let mut resolution_unit = None;
        let mut focal_plane_x_res: Option<f64> = None;
        let mut focal_plane_unit: Option<u32> = None;
        let mut all_tags = Vec::new();

        let mut width = photo.width;
        let mut height = photo.height;

        if let Ok(file) = File::open(path) {
            let mut bufreader = BufReader::new(&file);
            let exifreader = exif::Reader::new();
            if let Ok(exif) = exifreader.read_from_container(&mut bufreader) {
                for field in exif.fields() {
                    let tag_name = format!("{}", field.tag);
                    let val_str = field.display_value().to_string().trim_matches('"').trim().to_string();
                    all_tags.push(ExifTagEntry {
                        tag: tag_name.clone(),
                        value: val_str.clone(),
                    });

                    match field.tag {
                        exif::Tag::Make => {
                            if camera_make.is_none() {
                                camera_make = Some(val_str);
                            }
                        }
                        exif::Tag::Model => {
                            if camera_model.is_none() {
                                camera_model = Some(val_str);
                            }
                        }
                        exif::Tag::DateTimeOriginal => {
                            camera_date = Some(val_str);
                        }
                        exif::Tag::DateTimeDigitized => {
                            digitized_date = Some(val_str);
                        }
                        exif::Tag::DateTime => {
                            modified_date = Some(val_str);
                        }
                        exif::Tag::Orientation => {
                            if let Some(o) = field.value.get_uint(0) {
                                orientation_str = Some(match o {
                                    1 => "Normal (0 deg)".to_string(),
                                    3 => "Rotated 180 Degrees".to_string(),
                                    6 => "Rotated 90 Degrees".to_string(),
                                    8 => "Rotated 270 Degrees".to_string(),
                                    other => format!("Orientation {}", other),
                                });
                            }
                        }
                        exif::Tag::Flash => {
                            flash = Some(val_str);
                        }
                        exif::Tag::LensModel | exif::Tag::LensSpecification => {
                            if lens.is_none() {
                                lens = Some(val_str);
                            }
                        }
                        exif::Tag::FocalLength => {
                            let clean = val_str.trim_end_matches("mm").trim();
                            if !clean.is_empty() && !clean.eq_ignore_ascii_case("unknown") && !clean.eq_ignore_ascii_case("null") {
                                focal_length = Some(format!("{}mm", clean));
                            }
                        }
                        exif::Tag::FocalLengthIn35mmFilm => {
                            let clean = val_str.trim_end_matches("mm").trim();
                            if !clean.is_empty() && !clean.eq_ignore_ascii_case("unknown") && !clean.eq_ignore_ascii_case("null") {
                                focal_length_35mm = Some(format!("{}mm", clean));
                            }
                        }
                        exif::Tag::FocalPlaneXResolution => {
                            focal_plane_x_res = val_str.parse::<f64>().ok();
                        }
                        exif::Tag::FocalPlaneResolutionUnit => {
                            focal_plane_unit = field.value.get_uint(0);
                        }
                        exif::Tag::ExposureTime => {
                            exposure_time = Some(val_str);
                        }
                        exif::Tag::FNumber => {
                            f_number = Some(format!("f/{}", val_str));
                        }
                        exif::Tag::ApertureValue => {
                            aperture = Some(val_str);
                        }
                        exif::Tag::PhotographicSensitivity | exif::Tag::ISOSpeed => {
                            iso = Some(val_str);
                        }
                        exif::Tag::WhiteBalance => {
                            white_balance = Some(val_str);
                        }
                        exif::Tag::MeteringMode => {
                            metering_mode = Some(val_str);
                        }
                        exif::Tag::ExposureProgram => {
                            exposure_program = Some(val_str);
                        }
                        exif::Tag::ColorSpace => {
                            color_space = Some(if val_str == "1" || val_str.to_lowercase().contains("srgb") {
                                "sRGB".to_string()
                            } else if val_str == "2" || val_str.to_lowercase().contains("adobe") {
                                "Adobe RGB".to_string()
                            } else {
                                val_str
                            });
                        }
                        exif::Tag::XResolution => {
                            x_resolution = Some(val_str);
                        }
                        exif::Tag::YResolution => {
                            y_resolution = Some(val_str);
                        }
                        exif::Tag::ResolutionUnit => {
                            resolution_unit = Some(match val_str.as_str() {
                                "2" => "Inches".to_string(),
                                "3" => "Centimeters".to_string(),
                                other => other.to_string(),
                            });
                        }
                        exif::Tag::PixelXDimension => {
                            if width.is_none() {
                                width = field.value.get_uint(0);
                            }
                        }
                        exif::Tag::PixelYDimension => {
                            if height.is_none() {
                                height = field.value.get_uint(0);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // If aperture is missing but f_number is present
        if aperture.is_none() {
            aperture = f_number.clone().map(|f| f.trim_start_matches("f/").to_string());
        }
        if f_number.is_none() {
            f_number = aperture.clone().map(|a| format!("f/{}", a));
        }

        // Camera Date fallback from photo.date_taken
        if camera_date.is_none() {
            if let Some(ts) = photo.date_taken {
                if let Some(dt) = chrono::DateTime::<Utc>::from_timestamp_millis(ts) {
                    camera_date = Some(dt.format("%Y:%m:%d %H:%M:%S").to_string());
                }
            }
        }

        let dimensions = match (width, height) {
            (Some(w), Some(h)) => Some(format!("{} x {} pixels", w, h)),
            _ => None,
        };

        let megapixels = match (width, height) {
            (Some(w), Some(h)) => Some(format!("{:.1} MP", (w as f64 * h as f64) / 1_000_000.0)),
            _ => None,
        };

        // Compute CCD width and 35mm equivalent focal length matching Picasa
        if ccd_width.is_none() {
            if let (Some(w), Some(res)) = (width, focal_plane_x_res) {
                if res > 0.0 {
                    let unit_mult = if focal_plane_unit == Some(3) { 10.0 } else { 25.4 };
                    let sensor_w = (w as f64 / res) * unit_mult;
                    ccd_width = Some(format!("{:.1}", sensor_w));

                    if focal_length_35mm.is_none() {
                        if let Some(ref fl_str) = focal_length {
                            if let Some(fl) = fl_str.trim_end_matches("mm").trim().parse::<f64>().ok() {
                                let crop_factor = 36.0 / sensor_w;
                                let fl35 = fl * crop_factor;
                                focal_length_35mm = Some(format!("{:.1}mm", fl35));
                            }
                        }
                    }
                }
            } else if let (Some(ref fl_str), Some(ref fl35_str)) = (&focal_length, &focal_length_35mm) {
                let fl: Option<f64> = fl_str.trim_end_matches("mm").trim().parse().ok();
                let fl35: Option<f64> = fl35_str.trim_end_matches("mm").trim().parse().ok();
                if let (Some(f), Some(f35)) = (fl, fl35) {
                    if f35 > 0.0 {
                        let crop_factor = f35 / f;
                        let sensor_w = 36.0 / crop_factor;
                        ccd_width = Some(format!("{:.1}", sensor_w));
                    }
                }
            }
        }

        // Format exposure time e.g. "0.02s (1/50)"
        let formatted_exposure_time = exposure_time.map(|et| {
            if et.contains('/') {
                let parts: Vec<&str> = et.split('/').collect();
                if parts.len() == 2 {
                    if let (Ok(num), Ok(den)) = (parts[0].trim().parse::<f64>(), parts[1].trim().parse::<f64>()) {
                        if den > 0.0 {
                            let sec = num / den;
                            return format!("{:.2}s ({})", sec, et);
                        }
                    }
                }
            }
            et
        });

        Ok(PhotoMetadata {
            photo_id: photo.id,
            file_path: photo.file_path.clone(),
            file_name: photo.file_name.clone(),
            location,
            file_size,
            file_size_formatted,
            dimensions,
            width,
            height,
            megapixels,
            camera_make,
            camera_model,
            camera_date,
            digitized_date,
            modified_date,
            file_date,
            orientation: orientation_str,
            flash,
            lens,
            focal_length,
            focal_length_35mm,
            ccd_width,
            exposure_time: formatted_exposure_time,
            aperture,
            f_number,
            iso,
            white_balance,
            metering_mode,
            exposure_program,
            color_space,
            jpeg_quality: Some("98 (422)".to_string()),
            people_tags,
            unique_id: photo.file_hash.clone(),
            x_resolution,
            y_resolution,
            resolution_unit,
            all_tags,
        })
    }
}

fn format_file_size(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} bytes", bytes)
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
