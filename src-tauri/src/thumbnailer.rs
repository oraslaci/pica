use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::codecs::jpeg::JpegEncoder;
use image::{image_dimensions, ExtendedColorType};
use rayon::prelude::*;

use crate::db::DbManager;

pub const DEFAULT_THUMBNAIL_SIZE: u32 = 256;

fn compute_thumb_dimensions(w: u32, h: u32, target_size: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (1, 1);
    }
    if w <= target_size && h <= target_size {
        return (w, h);
    }
    if w >= h {
        let nh = ((h as f64 * target_size as f64) / w as f64).round() as u32;
        (target_size, nh.max(1))
    } else {
        let nw = ((w as f64 * target_size as f64) / h as f64).round() as u32;
        (nw.max(1), target_size)
    }
}

pub struct ThumbnailManager {
    pub cache_dir: PathBuf,
}

impl ThumbnailManager {
    pub fn new() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica")
            .join("thumbnails");

        Self::with_cache_dir(cache_dir)
    }

    pub fn with_cache_dir(cache_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&cache_dir);
        Self { cache_dir }
    }

    pub fn get_thumbnail_path(&self, file_hash: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.jpg", file_hash))
    }

    pub fn has_thumbnail(&self, file_hash: &str) -> bool {
        self.get_thumbnail_path(file_hash).exists()
    }

    pub fn try_extract_exif_thumbnail(source_path: &Path) -> Option<Vec<u8>> {
        let ext = source_path.extension()?.to_str()?.to_lowercase();
        if ext != "jpg" && ext != "jpeg" {
            return None;
        }

        let file = File::open(source_path).ok()?;
        let mut bufreader = std::io::BufReader::with_capacity(64 * 1024, file);
        let raw_exif = exif::get_exif_attr_from_jpeg(&mut bufreader).ok()?;
        let parsed_exif = exif::Reader::new().read_raw(raw_exif.clone()).ok()?;

        let offset_field = parsed_exif.get_field(exif::Tag::JPEGInterchangeFormat, exif::In::THUMBNAIL)?;
        let length_field = parsed_exif.get_field(exif::Tag::JPEGInterchangeFormatLength, exif::In::THUMBNAIL)?;

        let offset = match offset_field.value {
            exif::Value::Long(ref v) => *v.first()? as usize,
            exif::Value::Short(ref v) => *v.first()? as usize,
            _ => return None,
        };
        let length = match length_field.value {
            exif::Value::Long(ref v) => *v.first()? as usize,
            exif::Value::Short(ref v) => *v.first()? as usize,
            _ => return None,
        };

        if length > 1000 && offset + length <= raw_exif.len() {
            let slice = &raw_exif[offset..offset + length];
            if slice.len() >= 4 && slice[0] == 0xFF && slice[1] == 0xD8 && slice[2] == 0xFF {
                return Some(slice.to_vec());
            }
        }

        None
    }

    pub fn generate_thumbnail(
        &self,
        source_path: &Path,
        file_hash: &str,
        target_size: u32,
    ) -> Result<PathBuf, String> {
        let target_path = self.get_thumbnail_path(file_hash);
        if target_path.exists() {
            return Ok(target_path);
        }

        // Fast-path: extract embedded EXIF thumbnail in <1ms without full image decoding
        if let Some(thumb_data) = Self::try_extract_exif_thumbnail(source_path) {
            let now_nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let temp_path = self.cache_dir.join(format!("{}.tmp.{}.{}", file_hash, std::process::id(), now_nanos));
            if fs::write(&temp_path, &thumb_data).is_ok() {
                if fs::rename(&temp_path, &target_path).is_ok() {
                    return Ok(target_path);
                }
            }
        }

        let img = image::open(source_path)
            .map_err(|e| format!("Failed to open image {}: {}", source_path.display(), e))?;

        let rgb_img = img.to_rgb8();
        let (src_w, src_h) = (rgb_img.width(), rgb_img.height());
        let (dst_w, dst_h) = compute_thumb_dimensions(src_w, src_h, target_size);

        let mut dst_image = Image::new(dst_w, dst_h, PixelType::U8x3);
        let mut resizer = Resizer::new();
        let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::CatmullRom));
        resizer
            .resize(&rgb_img, &mut dst_image, &options)
            .map_err(|e| format!("Resize error: {}", e))?;

        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = self.cache_dir.join(format!("{}.tmp.{}.{}", file_hash, std::process::id(), now_nanos));
        let file = File::create(&temp_path)
            .map_err(|e| format!("Failed to create temp thumbnail file: {}", e))?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);

        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 75);
        encoder
            .encode(dst_image.buffer(), dst_w, dst_h, ExtendedColorType::Rgb8)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;
        drop(writer);

        fs::rename(&temp_path, &target_path)
            .map_err(|e| format!("Failed to rename temp thumbnail to final path: {}", e))?;

        Ok(target_path)
    }

    pub fn batch_generate(
        &self,
        items: &[(PathBuf, String)],
        target_size: u32,
    ) -> Vec<Result<PathBuf, String>> {
        items
            .par_iter()
            .map(|(path, hash)| self.generate_thumbnail(path, hash, target_size))
            .collect()
    }

    pub fn get_dimensions(source_path: &Path) -> Option<(u32, u32)> {
        image_dimensions(source_path).ok()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThumbnailJob {
    pub source_path: PathBuf,
    pub file_hash: String,
    pub target_size: u32,
    pub epoch: u64,
    pub is_priority: bool,
}

struct QueueState {
    /// All pending thumbnail jobs that are not yet active or completed.
    /// Keyed by file_hash for O(1) lookups and removals.
    pending_jobs: HashMap<String, ThumbnailJob>,
    /// Chronological list of pending hashes (for "the rest" / background processing)
    pending_order: VecDeque<String>,
    /// Hashes currently visible on screen (Tier 1: top priority)
    screen_hashes: Vec<String>,
    /// Hashes in previous and next 0.x screen (Tier 2: lookaround priority)
    near_hashes: Vec<String>,
    /// Hashes currently being actively processed by worker threads (no duplicates)
    active_hashes: HashSet<String>,
    /// Viewport epoch to preempt stale off-screen priority jobs
    epoch: u64,
}

fn pop_next_job(
    q: &mut QueueState,
    manager: &ThumbnailManager,
    db: &Option<DbManager>,
) -> Option<ThumbnailJob> {
    // 1. Tier 1: First item with no thumb and no thumb job from the screen
    for hash in &q.screen_hashes {
        if q.active_hashes.contains(hash) || manager.has_thumbnail(hash) {
            continue;
        }
        if let Some(mut job) = q.pending_jobs.remove(hash) {
            job.epoch = q.epoch;
            job.is_priority = true;
            q.active_hashes.insert(hash.clone());
            return Some(job);
        }
        if let Some(ref db_mgr) = db {
            if let Ok(Some(source_path_str)) = db_mgr.get_photo_path_by_hash(hash) {
                let source_path = PathBuf::from(source_path_str);
                if source_path.exists() {
                    q.active_hashes.insert(hash.clone());
                    return Some(ThumbnailJob {
                        source_path,
                        file_hash: hash.clone(),
                        target_size: DEFAULT_THUMBNAIL_SIZE,
                        epoch: q.epoch,
                        is_priority: true,
                    });
                }
            }
        }
    }

    // 2. Tier 2: Next, items with no thumb and no thumb job from previous and next 0.x part of screen
    for hash in &q.near_hashes {
        if q.active_hashes.contains(hash) || manager.has_thumbnail(hash) {
            continue;
        }
        if let Some(mut job) = q.pending_jobs.remove(hash) {
            job.epoch = q.epoch;
            job.is_priority = true;
            q.active_hashes.insert(hash.clone());
            return Some(job);
        }
        if let Some(ref db_mgr) = db {
            if let Ok(Some(source_path_str)) = db_mgr.get_photo_path_by_hash(hash) {
                let source_path = PathBuf::from(source_path_str);
                if source_path.exists() {
                    q.active_hashes.insert(hash.clone());
                    return Some(ThumbnailJob {
                        source_path,
                        file_hash: hash.clone(),
                        target_size: DEFAULT_THUMBNAIL_SIZE,
                        epoch: q.epoch,
                        is_priority: true,
                    });
                }
            }
        }
    }

    // 3. Tier 3: The rest (background pool)
    while let Some(hash) = q.pending_order.pop_front() {
        if q.active_hashes.contains(&hash) {
            continue;
        }
        if manager.has_thumbnail(&hash) {
            q.pending_jobs.remove(&hash);
            continue;
        }
        if let Some(job) = q.pending_jobs.remove(&hash) {
            q.active_hashes.insert(hash);
            return Some(job);
        }
    }

    // 4. Any leftover jobs in pending_jobs not in pending_order
    let leftover = q.pending_jobs.keys().find(|h| {
        !q.active_hashes.contains(*h) && !manager.has_thumbnail(*h)
    }).cloned();

    if let Some(hash) = leftover {
        if let Some(job) = q.pending_jobs.remove(&hash) {
            q.active_hashes.insert(hash);
            return Some(job);
        }
    }

    None
}

pub type ReadyCallback = Arc<dyn Fn(&str) + Send + Sync>;

#[derive(Clone)]
pub struct ThumbnailScheduler {
    pub manager: Arc<ThumbnailManager>,
    state: Arc<(Mutex<QueueState>, Condvar)>,
    pub db: Option<DbManager>,
    shutdown: Arc<AtomicBool>,
    ready_callbacks: Arc<Mutex<Vec<ReadyCallback>>>,
}

impl ThumbnailScheduler {
    pub fn new(
        manager: Arc<ThumbnailManager>,
        db: Option<DbManager>,
        num_workers: usize,
    ) -> Self {
        let state = Arc::new((
            Mutex::new(QueueState {
                pending_jobs: HashMap::new(),
                pending_order: VecDeque::new(),
                screen_hashes: Vec::new(),
                near_hashes: Vec::new(),
                active_hashes: HashSet::new(),
                epoch: 0,
            }),
            Condvar::new(),
        ));
        let shutdown = Arc::new(AtomicBool::new(false));
        let ready_callbacks: Arc<Mutex<Vec<ReadyCallback>>> = Arc::new(Mutex::new(Vec::new()));

        for _ in 0..num_workers {
            let state_clone = state.clone();
            let manager_clone = manager.clone();
            let db_clone = db.clone();
            let shutdown_clone = shutdown.clone();
            let ready_callbacks_clone = ready_callbacks.clone();

            thread::spawn(move || {
                let (lock, cvar) = &*state_clone;
                while !shutdown_clone.load(Ordering::Relaxed) {
                    let job = {
                        let mut q = lock.lock().unwrap();
                        loop {
                            if shutdown_clone.load(Ordering::Relaxed) {
                                return;
                            }
                            if let Some(job) = pop_next_job(&mut q, &manager_clone, &db_clone) {
                                break Some(job);
                            }
                            // Both screen, near, and background empty -> sleep
                            q = cvar.wait(q).unwrap();
                        }
                    };

                    if let Some(job) = job {
                        // Cooperative cancellation: if this priority job belongs to an older epoch
                        // and is no longer on screen or in near lookaround, discard it immediately.
                        if job.is_priority {
                            let (current_epoch, is_still_relevant) = {
                                let q = lock.lock().unwrap();
                                (
                                    q.epoch,
                                    q.screen_hashes.contains(&job.file_hash)
                                        || q.near_hashes.contains(&job.file_hash),
                                )
                            };
                            if job.epoch < current_epoch && !is_still_relevant {
                                let mut q = lock.lock().unwrap();
                                q.active_hashes.remove(&job.file_hash);
                                cvar.notify_all();
                                continue;
                            }
                        }

                        let res = if !manager_clone.has_thumbnail(&job.file_hash) {
                            manager_clone.generate_thumbnail(
                                &job.source_path,
                                &job.file_hash,
                                job.target_size,
                            )
                        } else {
                            Ok(manager_clone.get_thumbnail_path(&job.file_hash))
                        };

                        if res.is_ok() {
                            let cbs = ready_callbacks_clone.lock().unwrap().clone();
                            for cb in cbs {
                                cb(&job.file_hash);
                            }
                        }

                        let mut q = lock.lock().unwrap();
                        q.active_hashes.remove(&job.file_hash);
                        // Wake up waiting worker if there are more jobs
                        cvar.notify_all();
                    }
                }
            });
        }

        Self {
            manager,
            state,
            db,
            shutdown,
            ready_callbacks,
        }
    }

    /// Set prioritized view: on-screen photos (Tier 1) and near-screen lookaround photos (Tier 2).
    pub fn set_view_priority(&self, screen: Vec<String>, near: Vec<String>) {
        let (lock, cvar) = &*self.state;
        let mut q = lock.lock().unwrap();
        q.epoch = q.epoch.wrapping_add(1);
        q.screen_hashes = screen;
        q.near_hashes = near;
        cvar.notify_all();
    }

    /// Register a callback to be called when any thumbnail finishes generation.
    pub fn add_ready_callback<F>(&self, cb: F)
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let mut lock = self.ready_callbacks.lock().unwrap();
        lock.push(Arc::new(cb));
    }

    /// Enqueue discovered photos into the background queue (Tier 3: "the rest").
    pub fn enqueue_low_priority(&self, items: Vec<(PathBuf, String)>) {
        let (lock, cvar) = &*self.state;
        let mut q = lock.lock().unwrap();

        for (source_path, file_hash) in items {
            if self.manager.has_thumbnail(&file_hash) || q.pending_jobs.contains_key(&file_hash) {
                continue;
            }

            q.pending_order.push_back(file_hash.clone());
            q.pending_jobs.insert(
                file_hash.clone(),
                ThumbnailJob {
                    source_path,
                    file_hash,
                    target_size: DEFAULT_THUMBNAIL_SIZE,
                    epoch: 0,
                    is_priority: false,
                },
            );
        }

        cvar.notify_all();
    }

    /// Reset all queue state (e.g. on debug cache clear)
    pub fn clear_all_jobs(&self) {
        let (lock, cvar) = &*self.state;
        let mut q = lock.lock().unwrap();
        q.pending_jobs.clear();
        q.pending_order.clear();
        q.screen_hashes.clear();
        q.near_hashes.clear();
        cvar.notify_all();
    }
}

impl Drop for ThumbnailScheduler {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        let (_, cvar) = &*self.state;
        cvar.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn test_thumbnail_generation_and_caching() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let thumb_manager = ThumbnailManager::with_cache_dir(temp_dir.path().to_path_buf());

        // Create a 800x600 test image
        let mut img = RgbImage::new(800, 600);
        for pixel in img.pixels_mut() {
            *pixel = Rgb([200, 100, 50]);
        }
        let test_img_path = temp_dir.path().join("test_orig.png");
        img.save(&test_img_path).expect("save test image");

        let hash = "abc12345test";
        let thumb_path = thumb_manager
            .generate_thumbnail(&test_img_path, hash, 200)
            .expect("generate thumbnail");

        assert!(thumb_path.exists());
        assert!(thumb_manager.has_thumbnail(hash));

        // Verify dimensions <= 200
        let (w, h) = ThumbnailManager::get_dimensions(&thumb_path).expect("dimensions");
        assert!(w <= 200 && h <= 200);

        // Cached retrieval
        let cached_path = thumb_manager
            .generate_thumbnail(&test_img_path, hash, 200)
            .expect("retrieve cached thumbnail");
        assert_eq!(thumb_path, cached_path);
    }

    #[test]
    fn test_exif_fast_path_graceful_fallback_without_exif() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let test_img_path = temp_dir.path().join("plain_no_exif.jpg");
        let img = image::DynamicImage::new_rgb8(320, 240);
        img.save(&test_img_path).expect("save plain jpeg");

        // Plain synthetic JPEG has no embedded EXIF IFD1 thumbnail
        let thumb_bytes = ThumbnailManager::try_extract_exif_thumbnail(&test_img_path);
        assert!(thumb_bytes.is_none());
    }

    #[test]
    fn test_thumbnail_scheduler_dual_priority_and_preemption() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let thumb_manager = Arc::new(ThumbnailManager::with_cache_dir(temp_dir.path().to_path_buf()));

        // Create 3 test images
        let mut paths = Vec::new();
        for i in 0..3 {
            let mut img = RgbImage::new(100, 100);
            for pixel in img.pixels_mut() {
                *pixel = Rgb([i as u8 * 50, 100, 200]);
            }
            let p = temp_dir.path().join(format!("img_{}.png", i));
            img.save(&p).expect("save");
            paths.push((p, format!("hash_{}", i)));
        }

        let scheduler = ThumbnailScheduler::new(thumb_manager.clone(), None, 2);

        // Enqueue to low priority
        scheduler.enqueue_low_priority(paths.clone());

        // Wait until all are processed
        for (_, hash) in &paths {
            for _ in 0..100 {
                if thumb_manager.has_thumbnail(hash) {
                    break;
                }
                thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(thumb_manager.has_thumbnail(hash));
        }
    }
}
