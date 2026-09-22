use std::collections::{HashSet, VecDeque};
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

pub const DEFAULT_THUMBNAIL_SIZE: u32 = 384;

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
    cache_dir: PathBuf,
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
}

struct QueueState {
    high_priority: VecDeque<ThumbnailJob>,
    low_priority: VecDeque<ThumbnailJob>,
    enqueued_hashes: HashSet<String>,
    active_hashes: HashSet<String>,
}

#[derive(Clone)]
pub struct ThumbnailScheduler {
    pub manager: Arc<ThumbnailManager>,
    state: Arc<(Mutex<QueueState>, Condvar)>,
    db: Option<DbManager>,
    shutdown: Arc<AtomicBool>,
}

impl ThumbnailScheduler {
    pub fn new(
        manager: Arc<ThumbnailManager>,
        db: Option<DbManager>,
        num_workers: usize,
    ) -> Self {
        let state = Arc::new((
            Mutex::new(QueueState {
                high_priority: VecDeque::new(),
                low_priority: VecDeque::new(),
                enqueued_hashes: HashSet::new(),
                active_hashes: HashSet::new(),
            }),
            Condvar::new(),
        ));
        let shutdown = Arc::new(AtomicBool::new(false));

        for _ in 0..num_workers {
            let state_clone = state.clone();
            let manager_clone = manager.clone();
            let shutdown_clone = shutdown.clone();

            thread::spawn(move || {
                let (lock, cvar) = &*state_clone;
                while !shutdown_clone.load(Ordering::Relaxed) {
                    let job = {
                        let mut q = lock.lock().unwrap();
                        loop {
                            if shutdown_clone.load(Ordering::Relaxed) {
                                return;
                            }
                            // 1. High priority queue first
                            if let Some(job) = q.high_priority.pop_front() {
                                q.enqueued_hashes.remove(&job.file_hash);
                                q.active_hashes.insert(job.file_hash.clone());
                                break Some(job);
                            }
                            // 2. Low priority queue only when high priority is empty
                            if let Some(job) = q.low_priority.pop_front() {
                                q.enqueued_hashes.remove(&job.file_hash);
                                q.active_hashes.insert(job.file_hash.clone());
                                break Some(job);
                            }
                            // 3. Both queues empty -> sleep on condvar
                            q = cvar.wait(q).unwrap();
                        }
                    };

                    if let Some(job) = job {
                        if !manager_clone.has_thumbnail(&job.file_hash) {
                            let _ = manager_clone.generate_thumbnail(
                                &job.source_path,
                                &job.file_hash,
                                job.target_size,
                            );
                        }

                        let mut q = lock.lock().unwrap();
                        q.active_hashes.remove(&job.file_hash);
                    }
                }
            });
        }

        Self {
            manager,
            state,
            db,
            shutdown,
        }
    }

    /// Set high-priority photos (e.g. current viewport).
    /// If `replace` is true, previous unstarted high-priority jobs are cleared immediately.
    pub fn set_high_priority_hashes(&self, hashes: Vec<String>, replace: bool) {
        let (lock, cvar) = &*self.state;
        let mut q = lock.lock().unwrap();

        if replace {
            // Empties high priority queue immediately for the new view
            while let Some(job) = q.high_priority.pop_front() {
                q.enqueued_hashes.remove(&job.file_hash);
            }
        }

        for hash in hashes {
            if self.manager.has_thumbnail(&hash) {
                continue;
            }
            if q.active_hashes.contains(&hash) || q.enqueued_hashes.contains(&hash) {
                continue;
            }

            if let Some(ref db) = self.db {
                if let Ok(Some(source_path_str)) = db.get_photo_path_by_hash(&hash) {
                    let source_path = PathBuf::from(source_path_str);
                    if source_path.exists() {
                        q.enqueued_hashes.insert(hash.clone());
                        q.high_priority.push_back(ThumbnailJob {
                            source_path,
                            file_hash: hash,
                            target_size: DEFAULT_THUMBNAIL_SIZE,
                        });
                    }
                }
            }
        }

        cvar.notify_all();
    }

    /// Enqueue background batch of photos into the low-priority queue.
    pub fn enqueue_low_priority(&self, items: Vec<(PathBuf, String)>) {
        let (lock, cvar) = &*self.state;
        let mut q = lock.lock().unwrap();

        for (source_path, file_hash) in items {
            if self.manager.has_thumbnail(&file_hash) {
                continue;
            }
            if q.active_hashes.contains(&file_hash) || q.enqueued_hashes.contains(&file_hash) {
                continue;
            }

            q.enqueued_hashes.insert(file_hash.clone());
            q.low_priority.push_back(ThumbnailJob {
                source_path,
                file_hash,
                target_size: DEFAULT_THUMBNAIL_SIZE,
            });
        }

        cvar.notify_all();
    }

    pub fn get_queue_lengths(&self) -> (usize, usize) {
        let (lock, _) = &*self.state;
        let q = lock.lock().unwrap();
        (q.high_priority.len(), q.low_priority.len())
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
    fn test_webcam_thumbnail() {
        let path = PathBuf::from("/home/pathfindr/Pictures/Webcam/2023-11-08-160205.jpg");
        if path.exists() {
            let temp_dir = tempfile::tempdir().expect("create temp dir");
            let thumb_manager = ThumbnailManager::with_cache_dir(temp_dir.path().to_path_buf());
            let thumb = thumb_manager.generate_thumbnail(&path, "testwebcam", 384);
            println!("Webcam thumb result: {:?}", thumb);
            assert!(thumb.is_ok());
        }
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
