use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::image_dimensions;
use rayon::prelude::*;
use serde_json::json;

use crate::db::DbManager;
use crate::models::{
    AppSettings, CropSuggestion, EditResult, EditSessionManifest, EditSessionState, EditStep,
    Photo, StraightenSuggestion,
};
use crate::scanner::Scanner;
use crate::thumbnailer::{ThumbnailManager, DEFAULT_THUMBNAIL_SIZE};

pub struct EditorManager {
    pub edits_dir: PathBuf,
    pub session_lock: std::sync::Mutex<()>,
}

impl EditorManager {
    pub fn new() -> Self {
        let edits_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica")
            .join("edits");
        let _ = fs::create_dir_all(&edits_dir);
        Self {
            edits_dir,
            session_lock: std::sync::Mutex::new(()),
        }
    }

    pub fn with_edits_dir(edits_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&edits_dir);
        Self {
            edits_dir,
            session_lock: std::sync::Mutex::new(()),
        }
    }

    pub fn get_session_dir(&self, base_hash: &str) -> PathBuf {
        self.edits_dir.join(base_hash)
    }

    pub fn get_session_manifest_path(&self, base_hash: &str) -> PathBuf {
        self.get_session_dir(base_hash).join("session.json")
    }

    pub fn load_manifest(&self, base_hash: &str) -> Option<EditSessionManifest> {
        let path = self.get_session_manifest_path(base_hash);
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn find_session_for_photo(&self, photo: &Photo) -> Option<EditSessionManifest> {
        if let Some(m) = self.load_manifest(&photo.file_hash) {
            return Some(m);
        }
        if let Ok(entries) = fs::read_dir(&self.edits_dir) {
            for entry in entries.flatten() {
                let sess_file = entry.path().join("session.json");
                if sess_file.exists() {
                    if let Ok(content) = fs::read_to_string(&sess_file) {
                        if let Ok(m) = serde_json::from_str::<EditSessionManifest>(&content) {
                            if m.photo_id == photo.id || m.original_path == photo.file_path {
                                return Some(m);
                            }
                        }
                    }
                }
            }
        }
        None
    }

    pub fn sync_sessions_to_db(&self, db: &DbManager) {
        if let Ok(entries) = fs::read_dir(&self.edits_dir) {
            for entry in entries.flatten() {
                let sess_file = entry.path().join("session.json");
                if sess_file.exists() {
                    if let Ok(content) = fs::read_to_string(&sess_file) {
                        if let Ok(m) = serde_json::from_str::<EditSessionManifest>(&content) {
                            let rot = if !m.is_committed {
                                Self::calculate_cumulative_rotation(&m.steps, m.current_step)
                            } else {
                                0
                            };
                            let _ = db.update_photo_edit_rotation(m.photo_id, rot);
                        }
                    }
                }
            }
        }
    }

    pub fn save_manifest(&self, manifest: &EditSessionManifest) -> Result<(), String> {
        let session_dir = self.get_session_dir(&manifest.base_hash);
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
        let path = session_dir.join("session.json");
        let tmp_path = session_dir.join(format!("session.json.tmp.{}", std::process::id()));
        let content = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
        fs::write(&tmp_path, content).map_err(|e| e.to_string())?;
        fs::rename(&tmp_path, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get_backup_path(target_path: &Path) -> PathBuf {
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = target_path.file_name().unwrap_or_default();
        parent.join(".pica_originals").join(file_name)
    }

    pub fn get_pristine_path(manifest_dir: &Path, target_path: &Path) -> PathBuf {
        let ext = target_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg");
        manifest_dir.join(format!("pristine.{}", ext))
    }

    pub fn ensure_backup(
        &self,
        target_path: &Path,
        settings: &AppSettings,
        manifest_dir: &Path,
    ) -> Result<Option<PathBuf>, String> {
        let internal_pristine = Self::get_pristine_path(manifest_dir, target_path);
        if !internal_pristine.exists() {
            fs::copy(target_path, &internal_pristine)
                .map_err(|e| format!("Failed to create internal pristine copy: {}", e))?;
        }

        if settings.original_handling == "backup" {
            let backup_path = Self::get_backup_path(target_path);
            if let Some(parent) = backup_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if !backup_path.exists() {
                fs::copy(target_path, &backup_path)
                    .map_err(|e| format!("Failed to create backup copy: {}", e))?;
            }
            Ok(Some(backup_path))
        } else {
            Ok(None)
        }
    }

    pub fn ensure_base_proxy(
        &self,
        session_dir: &Path,
        target_path: &Path,
        _photo_hash: &str,
        _thumbnailer: &ThumbnailManager,
    ) -> Result<PathBuf, String> {
        let step0_path = session_dir.join("step_0.jpg");

        // If step_0.jpg already exists, check if it's an old 512px thumbnail (< 250KB while target is large)
        if step0_path.exists() {
            let meta_s0 = fs::metadata(&step0_path).ok();
            let meta_target = fs::metadata(target_path).ok();
            if let (Some(s0), Some(tg)) = (meta_s0, meta_target) {
                if s0.len() >= 250_000 || s0.len() >= tg.len() * 8 / 10 {
                    return Ok(step0_path);
                }
            } else {
                return Ok(step0_path);
            }
        }

        // Direct copy for small JPEG files preserves full resolution in milliseconds
        let ext = target_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let tmp_step0 = session_dir.join(format!("step_0.jpg.tmp.{}", std::process::id()));
        if (ext == "jpg" || ext == "jpeg") && target_path.exists() {
            if let Ok(meta) = fs::metadata(target_path) {
                // If under 2MB, direct copy is instantaneous
                if meta.len() <= 2_000_000 {
                    if fs::copy(target_path, &tmp_step0).is_ok() {
                        let _ = fs::rename(&tmp_step0, &step0_path);
                        return Ok(step0_path);
                    }
                }
            }
        }

        // For other formats or large JPEGs: decode and save high-quality proxy (up to 1920px)
        if let Ok(img) = image::open(target_path) {
            let (w, h) = (img.width(), img.height());
            let max_dim = 1920u32;
            let proxy_img = if w > max_dim || h > max_dim {
                img.thumbnail(max_dim, max_dim)
            } else {
                img
            };
            if let Ok(file) = File::create(&tmp_step0) {
                let mut writer = BufWriter::new(file);
                let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
                let rgb = proxy_img.to_rgb8();
                if encoder.encode(rgb.as_raw(), proxy_img.width(), proxy_img.height(), image::ExtendedColorType::Rgb8).is_ok() {
                    drop(writer);
                    let _ = fs::rename(&tmp_step0, &step0_path);
                    if step0_path.exists() {
                        return Ok(step0_path);
                    }
                }
            }
        }

        let _ = fs::copy(target_path, &tmp_step0);
        let _ = fs::rename(&tmp_step0, &step0_path);
        Ok(step0_path)
    }

    pub fn render_step_proxy(
        step0_path: &Path,
        dest_step_path: &Path,
        cumulative_rotation: i32,
    ) -> Result<(), String> {
        let normalized = ((cumulative_rotation % 360) + 360) % 360;
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));

        if normalized == 0 {
            fs::copy(step0_path, &tmp_dest).map_err(|e| e.to_string())?;
            fs::rename(&tmp_dest, dest_step_path).map_err(|e| e.to_string())?;
            return Ok(());
        }

        let img = image::open(step0_path)
            .map_err(|e| format!("Failed to open step_0: {}", e))?;
        let rotated = match normalized {
            90 => img.rotate90(),
            180 => img.rotate180(),
            270 => img.rotate270(),
            _ => img,
        };

        let max_dim = 1920u32;
        let final_img = if rotated.width() > max_dim || rotated.height() > max_dim {
            rotated.thumbnail(max_dim, max_dim)
        } else {
            rotated
        };

        let rgb = final_img.to_rgb8();
        let file = File::create(&tmp_dest)
            .map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                final_img.width(),
                final_img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| format!("JPEG encode error: {}", e))?;
        drop(writer);
        fs::rename(&tmp_dest, dest_step_path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn rotate_and_crop_image(img: &image::DynamicImage, angle_degrees: f64) -> image::RgbImage {
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        if angle_degrees.abs() < 1e-4 || w == 0 || h == 0 {
            return rgb;
        }

        let angle_rad = angle_degrees.to_radians();
        let cos_a = angle_rad.cos();
        let sin_a = angle_rad.sin();
        let abs_angle_rad = angle_rad.abs();

        // Calculate required zoom factor s(theta) to fill the unrotated w x h frame completely
        let aspect = w as f64 / h as f64;
        let inv_aspect = h as f64 / w as f64;
        let max_ratio = aspect.max(inv_aspect);
        let zoom = abs_angle_rad.cos() + abs_angle_rad.sin() * max_ratio;

        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;

        let mut out_img = image::RgbImage::new(w, h);
        let raw_src = rgb.as_raw();
        let stride = (w * 3) as usize;

        out_img
            .as_mut()
            .par_chunks_exact_mut(stride)
            .enumerate()
            .for_each(|(y, row)| {
                let y_out = y as f64 + 0.5;
                let v = (y_out - cy) / zoom;

                for x in 0..w as usize {
                    let x_out = x as f64 + 0.5;
                    let u = (x_out - cx) / zoom;

                    // Inverse rotation: mapping destination pixel (rotated clockwise by angle_degrees)
                    // back to source image (rotated counter-clockwise by -angle_degrees)
                    let u_src = u * cos_a + v * sin_a;
                    let v_src = -u * sin_a + v * cos_a;

                    let src_x = (u_src + cx - 0.5).clamp(0.0, (w - 1) as f64);
                    let src_y = (v_src + cy - 0.5).clamp(0.0, (h - 1) as f64);

                    let x0 = src_x.floor() as usize;
                    let y0 = src_y.floor() as usize;
                    let x1 = (x0 + 1).min((w - 1) as usize);
                    let y1 = (y0 + 1).min((h - 1) as usize);

                    let fx = src_x - x0 as f64;
                    let fy = src_y - y0 as f64;

                    let idx00 = y0 * stride + x0 * 3;
                    let idx10 = y0 * stride + x1 * 3;
                    let idx01 = y1 * stride + x0 * 3;
                    let idx11 = y1 * stride + x1 * 3;

                    let w00 = (1.0 - fx) * (1.0 - fy);
                    let w10 = fx * (1.0 - fy);
                    let w01 = (1.0 - fx) * fy;
                    let w11 = fx * fy;

                    for c in 0..3 {
                        let val = w00 * (raw_src[idx00 + c] as f64)
                            + w10 * (raw_src[idx10 + c] as f64)
                            + w01 * (raw_src[idx01 + c] as f64)
                            + w11 * (raw_src[idx11 + c] as f64);
                        row[x * 3 + c] = val.round().clamp(0.0, 255.0) as u8;
                    }
                }
            });

        out_img
    }

    pub fn render_straighten_step_proxy(
        src_path: &Path,
        dest_path: &Path,
        angle_deg: f64,
    ) -> Result<(), String> {
        let img = image::open(src_path)
            .map_err(|e| format!("Failed to open image for straighten: {}", e))?;
        let max_dim = 1920u32;
        let base_img = if img.width() > max_dim || img.height() > max_dim {
            img.thumbnail(max_dim, max_dim)
        } else {
            img
        };

        let rotated_rgb = Self::rotate_and_crop_image(&base_img, angle_deg);
        let tmp_dest = dest_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest)
            .map_err(|e| format!("Failed to create straighten step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rotated_rgb.as_raw(),
                rotated_rgb.width(),
                rotated_rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| format!("JPEG encode error: {}", e))?;
        drop(writer);
        fs::rename(&tmp_dest, dest_path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn check_external_conflict(
        manifest: &EditSessionManifest,
        target_path: &Path,
    ) -> bool {
        if !target_path.exists() {
            return true;
        }
        if let Ok(meta) = fs::metadata(target_path) {
            let current_size = meta.len();
            let current_mtime = meta
                .modified()
                .unwrap_or(SystemTime::now())
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // If file size or mtime differs from what Pica last wrote
            if current_mtime != manifest.last_saved_mtime && current_size != manifest.last_saved_size {
                // If it also differs from base original, external program touched it
                if current_mtime != manifest.base_mtime || current_size != manifest.base_size {
                    return true;
                }
            }
        }
        false
    }

    pub fn get_session_state(
        &self,
        db: &DbManager,
        photo_id: i64,
    ) -> Result<EditSessionState, String> {
        let _lock = self.session_lock.lock().unwrap();
        self.get_session_state_internal(db, photo_id)
    }

    pub fn get_session_state_internal(
        &self,
        db: &DbManager,
        photo_id: i64,
    ) -> Result<EditSessionState, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let backup_path = Self::get_backup_path(&target_path);
        let has_backup = backup_path.exists();

        let backup_path_str = if has_backup {
            Some(backup_path.to_string_lossy().to_string())
        } else {
            None
        };

        // Check if there is a session for this photo
        let manifest = self.find_session_for_photo(&photo);

        if let Some(m) = manifest {
            let has_conflict = Self::check_external_conflict(&m, &target_path);
            let can_undo = m.current_step > 0;
            let can_redo = m.current_step < m.steps.len().saturating_sub(1);
            let cumulative_rotation = Self::calculate_cumulative_rotation(&m.steps, m.current_step);

            let preview_path = m.steps.get(m.current_step).and_then(|step| {
                step.render_file.as_ref().map(|rf| {
                    self.get_session_dir(&m.base_hash).join(rf).to_string_lossy().to_string()
                })
            });

            let session_dir = self.get_session_dir(&m.base_hash);
            let session_manifest_path = self.get_session_manifest_path(&m.base_hash);

            Ok(EditSessionState {
                photo_id,
                file_path: photo.file_path,
                has_session: true,
                can_undo,
                can_redo,
                current_step: m.current_step,
                total_steps: m.steps.len(),
                cumulative_rotation,
                has_backup,
                backup_path: backup_path_str,
                preview_path,
                has_conflict,
                is_committed: m.is_committed,
                session_dir: Some(session_dir.to_string_lossy().to_string()),
                session_manifest_path: Some(session_manifest_path.to_string_lossy().to_string()),
                steps: m.steps.clone(),
            })
        } else {
            Ok(EditSessionState {
                photo_id,
                file_path: photo.file_path,
                has_session: false,
                can_undo: false,
                can_redo: false,
                current_step: 0,
                total_steps: 0,
                cumulative_rotation: 0,
                has_backup,
                backup_path: backup_path_str,
                preview_path: None,
                has_conflict: false,
                is_committed: false,
                session_dir: None,
                session_manifest_path: None,
                steps: Vec::new(),
            })
        }
    }

    pub fn calculate_cumulative_rotation(steps: &[EditStep], current_step: usize) -> i32 {
        let mut rot = 0;
        for step in steps.iter().take(current_step + 1) {
            if step.op == "rotate" {
                if let Some(deg) = step.params.get("degrees").and_then(|v| v.as_i64()) {
                    rot += deg as i32;
                }
            }
        }
        ((rot % 360) + 360) % 360
    }

    pub fn apply_rotation(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        degrees: i32,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        if !target_path.exists() {
            return Err("Target file does not exist on disk".to_string());
        }

        let settings = db.get_app_settings().unwrap_or_default();

        let mut manifest = match self.find_session_for_photo(&photo) {
            Some(m) => m,
            None => {
                let session_dir = self.get_session_dir(&photo.file_hash);
                fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

                let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let backup_path = self.ensure_backup(&target_path, &settings, &session_dir)?;
                let _ = self.ensure_base_proxy(&session_dir, &target_path, &photo.file_hash, thumbnailer)?;

                let initial_step = EditStep {
                    step_id: 0,
                    op: "original".to_string(),
                    params: json!({}),
                    timestamp: mtime as i64,
                    render_file: Some("step_0.jpg".to_string()),
                    cumulative_rotation: 0,
                };

                let m = EditSessionManifest {
                    photo_id,
                    original_path: photo.file_path.clone(),
                    backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
                    base_hash: photo.file_hash.clone(),
                    base_mtime: mtime,
                    base_size: meta.len(),
                    last_saved_mtime: mtime,
                    last_saved_size: meta.len(),
                    current_step: 0,
                    steps: vec![initial_step],
                    is_committed: false,
                };
                self.save_manifest(&m)?;
                m
            }
        };

        let session_dir = self.get_session_dir(&manifest.base_hash);
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

        // Check external conflict
        let has_conflict = Self::check_external_conflict(&manifest, &target_path);
        if has_conflict {
            let session_state = self.get_session_state_internal(db, photo_id)?;
            return Ok(EditResult {
                success: false,
                photo: Some(photo),
                session_state,
                message: Some("External changes detected on image file".to_string()),
            });
        }

        // Ensure step_0.jpg exists
        let step0_path = self.ensure_base_proxy(&session_dir, &target_path, &manifest.base_hash, thumbnailer)?;

        // Truncate any redo steps beyond current_step
        manifest.steps.truncate(manifest.current_step + 1);

        // Remove any orphaned forward step files
        let mut orphan_id = manifest.current_step + 1;
        loop {
            let orphan_file = session_dir.join(format!("step_{}.jpg", orphan_id));
            if orphan_file.exists() {
                let _ = fs::remove_file(orphan_file);
                orphan_id += 1;
            } else {
                break;
            }
        }

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let next_step_id = manifest.steps.len();
        let step_file_name = format!("step_{}.jpg", next_step_id);
        let dest_step_path = session_dir.join(&step_file_name);

        // Compute cumulative rotation for the new step
        let prev_cum_rot = manifest
            .steps
            .get(manifest.current_step)
            .map(|s| s.cumulative_rotation)
            .unwrap_or(0);
        let new_cum_rot = ((prev_cum_rot + degrees) % 360 + 360) % 360;

        // If the active chain contains non-rotation steps (e.g. crop), rotate from the previous step render proxy
        let has_non_rotation = manifest.steps[..=manifest.current_step]
            .iter()
            .any(|s| s.op != "original" && s.op != "rotate");

        if has_non_rotation {
            let prev_render_name = manifest
                .steps
                .get(manifest.current_step)
                .and_then(|s| s.render_file.clone())
                .unwrap_or_else(|| "step_0.jpg".to_string());
            let prev_path = session_dir.join(&prev_render_name);
            let src = if prev_path.exists() { prev_path } else { step0_path.clone() };
            let norm_deg = ((degrees % 360) + 360) % 360;
            Self::render_step_proxy(&src, &dest_step_path, norm_deg)?;
        } else {
            Self::render_step_proxy(&step0_path, &dest_step_path, new_cum_rot)?;
        }

        manifest.steps.push(EditStep {
            step_id: next_step_id,
            op: "rotate".to_string(),
            params: json!({ "degrees": degrees }),
            timestamp: now_sec,
            render_file: Some(step_file_name.clone()),
            cumulative_rotation: new_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        let _ = db.update_photo_edit_rotation(photo_id, new_cum_rot);

        // Fast update thumbnail cache for gallery view
        let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
        let _ = fs::copy(&dest_step_path, &thumb_path);

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn get_crop_suggestions(
        &self,
        db: &DbManager,
        thumbnailer: &ThumbnailManager,
        photo_id: i64,
        aspect_ratio: Option<f64>,
    ) -> Result<Vec<CropSuggestion>, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let manifest = self.find_session_for_photo(&photo);
        let src_path = if let Some(ref m) = manifest {
            let session_dir = self.get_session_dir(&m.base_hash);
            if let Some(step) = m.steps.get(m.current_step) {
                if let Some(ref rf) = step.render_file {
                    let p = session_dir.join(rf);
                    if p.exists() {
                        Some(p)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let eval_path = match src_path {
            Some(p) => p,
            None => {
                let thumb = thumbnailer.get_thumbnail_path(&photo.file_hash);
                if thumb.exists() {
                    thumb
                } else {
                    target_path
                }
            }
        };

        Self::compute_crop_suggestions(&eval_path, aspect_ratio.unwrap_or(0.0))
    }

    pub fn compute_crop_suggestions(
        img_path: &Path,
        target_aspect: f64,
    ) -> Result<Vec<CropSuggestion>, String> {
        let img = image::open(img_path).map_err(|e| format!("Failed to open image {}: {}", img_path.display(), e))?;
        let orig_w = img.width();
        let orig_h = img.height();
        if orig_w == 0 || orig_h == 0 {
            return Err("Empty image".to_string());
        }

        let aspect = if target_aspect <= 0.05 || target_aspect >= 20.0 {
            orig_w as f64 / orig_h as f64
        } else {
            target_aspect
        };

        // Scale to proxy size for sub-millisecond Sobel & SAT computation
        let max_dim = 256u32;
        let (proxy_w, proxy_h) = if orig_w > max_dim || orig_h > max_dim {
            if orig_w >= orig_h {
                (max_dim, ((orig_h as f64 * max_dim as f64 / orig_w as f64).round() as u32).max(16))
            } else {
                (((orig_w as f64 * max_dim as f64 / orig_h as f64).round() as u32).max(16), max_dim)
            }
        } else {
            (orig_w.max(16), orig_h.max(16))
        };

        let proxy = img.resize_exact(proxy_w, proxy_h, image::imageops::FilterType::Nearest);
        let luma = proxy.to_luma8();
        let (w, h) = (proxy_w as usize, proxy_h as usize);

        // Compute Sobel gradient energy with border attenuation & center weighting
        let mut energy = vec![0f64; w * h];
        for y in 1..(h - 1) {
            for x in 1..(w - 1) {
                let p = |dx: isize, dy: isize| -> f64 {
                    luma.get_pixel((x as isize + dx) as u32, (y as isize + dy) as u32)[0] as f64
                };
                let gx = (p(1, -1) + 2.0 * p(1, 0) + p(1, 1)) - (p(-1, -1) + 2.0 * p(-1, 0) + p(-1, 1));
                let gy = (p(-1, 1) + 2.0 * p(0, 1) + p(1, 1)) - (p(-1, -1) + 2.0 * p(0, -1) + p(1, -1));
                let grad = (gx * gx + gy * gy).sqrt();

                // Border margin attenuation (damp extreme 6% border)
                let border_x = (x as f64 / w as f64).min(1.0 - x as f64 / w as f64);
                let border_y = (y as f64 / h as f64).min(1.0 - y as f64 / h as f64);
                let border_dist = border_x.min(border_y);
                let border_weight = (border_dist / 0.08).min(1.0);

                // Center weight
                let cx = (x as f64 / w as f64) - 0.5;
                let cy = (y as f64 / h as f64) - 0.5;
                let center_weight = 1.0 - 0.25 * (cx * cx + cy * cy) * 4.0;

                energy[y * w + x] = grad * border_weight * center_weight.max(0.2);
            }
        }

        // Build Summed Area Table (SAT / Integral Image)
        let sat_stride = w + 1;
        let mut sat = vec![0f64; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut row_sum = 0f64;
            for x in 0..w {
                row_sum += energy[y * w + x];
                sat[(y + 1) * sat_stride + (x + 1)] = sat[y * sat_stride + (x + 1)] + row_sum;
            }
        }

        let rect_energy = |rx: usize, ry: usize, rw: usize, rh: usize| -> f64 {
            let x2 = (rx + rw).min(w);
            let y2 = (ry + rh).min(h);
            sat[y2 * sat_stride + x2]
                - sat[ry * sat_stride + x2]
                - sat[y2 * sat_stride + rx]
                + sat[ry * sat_stride + rx]
        };

        let total_energy = sat[h * sat_stride + w].max(1.0);

        // Center of mass of gradient energy
        let mut com_x = 0.5;
        let mut com_y = 0.5;
        let mut sum_e = 0.0;
        for y in 0..h {
            for x in 0..w {
                let e = energy[y * w + x];
                com_x += (x as f64 / w as f64) * e;
                com_y += (y as f64 / h as f64) * e;
                sum_e += e;
            }
        }
        if sum_e > 0.0 {
            com_x /= sum_e;
            com_y /= sum_e;
        }

        let get_window_dims = |scale: f64| -> (usize, usize) {
            let mut target_w = (w as f64 * scale).round();
            let mut target_h = (target_w / aspect).round();
            if target_h > h as f64 {
                target_h = (h as f64 * scale).round();
                target_w = (target_h * aspect).round();
            }
            if target_w > w as f64 {
                target_w = w as f64;
                target_h = (target_w / aspect).round();
            }
            let rw = (target_w as usize).max(8).min(w);
            let rh = (target_h as usize).max(8).min(h);
            (rw, rh)
        };

        // 1. Tight Focus (~50% scale, maximizes energy density)
        let (s1_w, s1_h) = get_window_dims(0.50);
        let mut best_s1 = (0, 0, s1_w, s1_h);
        let mut max_s1_score = -1.0;
        let step_x1 = ((w.saturating_sub(s1_w)) / 20).max(1);
        let step_y1 = ((h.saturating_sub(s1_h)) / 20).max(1);
        for ry in (0..=(h.saturating_sub(s1_h))).step_by(step_y1) {
            for rx in (0..=(w.saturating_sub(s1_w))).step_by(step_x1) {
                let e = rect_energy(rx, ry, s1_w, s1_h);
                let score = e / ((s1_w * s1_h) as f64).powf(0.85);
                if score > max_s1_score {
                    max_s1_score = score;
                    best_s1 = (rx, ry, s1_w, s1_h);
                }
            }
        }

        // 2. Rule of Thirds (~72% scale, balances subject alignment with power points)
        let (s2_w, s2_h) = get_window_dims(0.72);
        let mut best_s2 = (0, 0, s2_w, s2_h);
        let mut max_s2_score = -1.0;
        let step_x2 = ((w.saturating_sub(s2_w)) / 20).max(1);
        let step_y2 = ((h.saturating_sub(s2_h)) / 20).max(1);
        for ry in (0..=(h.saturating_sub(s2_h))).step_by(step_y2) {
            for rx in (0..=(w.saturating_sub(s2_w))).step_by(step_x2) {
                let e = rect_energy(rx, ry, s2_w, s2_h);
                let rel_x = (com_x * w as f64 - rx as f64) / s2_w as f64;
                let rel_y = (com_y * h as f64 - ry as f64) / s2_h as f64;

                let dist_x = ((rel_x - 1.0 / 3.0).abs()).min((rel_x - 2.0 / 3.0).abs()).min((rel_x - 0.5).abs() * 1.2);
                let dist_y = ((rel_y - 1.0 / 3.0).abs()).min((rel_y - 2.0 / 3.0).abs()).min((rel_y - 0.5).abs() * 1.2);
                let composition_penalty = 1.0 - 0.4 * (dist_x * dist_x + dist_y * dist_y).sqrt();

                let score = (e / total_energy) * composition_penalty.max(0.1);
                if score > max_s2_score {
                    max_s2_score = score;
                    best_s2 = (rx, ry, s2_w, s2_h);
                }
            }
        }

        // 3. Wide Context (~88% scale, captures complete scene cleanly)
        let (s3_w, s3_h) = get_window_dims(0.88);
        let mut best_s3 = (0, 0, s3_w, s3_h);
        let mut max_s3_score = -1.0;
        let step_x3 = ((w.saturating_sub(s3_w)) / 16).max(1);
        let step_y3 = ((h.saturating_sub(s3_h)) / 16).max(1);
        for ry in (0..=(h.saturating_sub(s3_h))).step_by(step_y3) {
            for rx in (0..=(w.saturating_sub(s3_w))).step_by(step_x3) {
                let e = rect_energy(rx, ry, s3_w, s3_h);
                let win_center_x = (rx as f64 + s3_w as f64 / 2.0) / w as f64;
                let win_center_y = (ry as f64 + s3_h as f64 / 2.0) / h as f64;
                let center_dist = ((win_center_x - com_x).powi(2) + (win_center_y - com_y).powi(2)).sqrt();
                let score = (e / total_energy) * (1.0 - 0.2 * center_dist);
                if score > max_s3_score {
                    max_s3_score = score;
                    best_s3 = (rx, ry, s3_w, s3_h);
                }
            }
        }

        let to_suggestion = |id: usize, label: &str, (rx, ry, rw, rh): (usize, usize, usize, usize)| -> CropSuggestion {
            CropSuggestion {
                id,
                label: label.to_string(),
                x: (rx as f64 / w as f64).clamp(0.0, 1.0),
                y: (ry as f64 / h as f64).clamp(0.0, 1.0),
                width: (rw as f64 / w as f64).clamp(0.01, 1.0),
                height: (rh as f64 / h as f64).clamp(0.01, 1.0),
            }
        };

        Ok(vec![
            to_suggestion(1, "Tight Focus", best_s1),
            to_suggestion(2, "Rule of Thirds", best_s2),
            to_suggestion(3, "Wide Context", best_s3),
        ])
    }

    pub fn get_straighten_suggestions(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<Vec<StraightenSuggestion>, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let manifest = self.find_session_for_photo(&photo);
        let src_path = if let Some(ref m) = manifest {
            let session_dir = self.get_session_dir(&m.base_hash);
            if let Some(step) = m.steps.get(m.current_step) {
                if let Some(ref rf) = step.render_file {
                    let p = session_dir.join(rf);
                    if p.exists() {
                        Some(p)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let eval_path = match src_path {
            Some(p) => p,
            None => {
                let thumb = thumbnailer.get_thumbnail_path(&photo.file_hash);
                if thumb.exists() {
                    thumb
                } else {
                    target_path
                }
            }
        };

        Self::compute_straighten_suggestions(&eval_path)
    }

    pub fn compute_straighten_suggestions(
        img_path: &Path,
    ) -> Result<Vec<StraightenSuggestion>, String> {
        let img = image::open(img_path)
            .map_err(|e| format!("Failed to open image {}: {}", img_path.display(), e))?;
        let orig_w = img.width();
        let orig_h = img.height();
        if orig_w == 0 || orig_h == 0 {
            return Err("Empty image".to_string());
        }

        // Downscale to fast working proxy (max 480px)
        let max_dim = 480u32;
        let proxy = if orig_w > max_dim || orig_h > max_dim {
            img.thumbnail(max_dim, max_dim)
        } else {
            img
        };

        let gray = proxy.to_luma8();
        let (w, h) = (gray.width() as usize, gray.height() as usize);
        if w < 20 || h < 20 {
            return Ok(Self::fallback_straighten_suggestions());
        }

        let raw = gray.as_raw();

        // Step 1: 3x3 Gaussian Blur to suppress sensor noise & compression artifacts
        let mut blurred = vec![0u8; w * h];
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let sum = (raw[(y - 1) * w + (x - 1)] as u32) + 2 * (raw[(y - 1) * w + x] as u32) + (raw[(y - 1) * w + (x + 1)] as u32)
                        + 2 * (raw[y * w + (x - 1)] as u32)   + 4 * (raw[y * w + x] as u32)       + 2 * (raw[y * w + (x + 1)] as u32)
                        + (raw[(y + 1) * w + (x - 1)] as u32) + 2 * (raw[(y + 1) * w + x] as u32) + (raw[(y + 1) * w + (x + 1)] as u32);
                blurred[y * w + x] = (sum / 16) as u8;
            }
        }

        // Step 2: Sobel Gradients
        let mut gx_buf = vec![0.0f32; w * h];
        let mut gy_buf = vec![0.0f32; w * h];
        let mut mag_buf = vec![0.0f32; w * h];

        let mut mag_vals = Vec::with_capacity(w * h / 4);

        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let p00 = blurred[(y - 1) * w + (x - 1)] as f32;
                let p01 = blurred[(y - 1) * w + x] as f32;
                let p02 = blurred[(y - 1) * w + (x + 1)] as f32;
                let p10 = blurred[y * w + (x - 1)] as f32;
                let p12 = blurred[y * w + (x + 1)] as f32;
                let p20 = blurred[(y + 1) * w + (x - 1)] as f32;
                let p21 = blurred[(y + 1) * w + x] as f32;
                let p22 = blurred[(y + 1) * w + (x + 1)] as f32;

                let gx = (p02 + 2.0 * p12 + p22) - (p00 + 2.0 * p10 + p20);
                let gy = (p20 + 2.0 * p21 + p22) - (p00 + 2.0 * p01 + p02);
                let mag = (gx * gx + gy * gy).sqrt();

                let idx = y * w + x;
                gx_buf[idx] = gx;
                gy_buf[idx] = gy;
                mag_buf[idx] = mag;

                if mag > 20.0 {
                    mag_vals.push(mag);
                }
            }
        }

        if mag_vals.len() < 50 {
            return Ok(Self::fallback_straighten_suggestions());
        }

        mag_vals.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // Dynamic thresholding: low = 50th percentile, high = 80th percentile
        let high_idx = (mag_vals.len() as f64 * 0.80) as usize;
        let low_idx = (mag_vals.len() as f64 * 0.50) as usize;
        let t_high = mag_vals[high_idx.min(mag_vals.len() - 1)].max(35.0);
        let t_low = mag_vals[low_idx].max(15.0);

        // Step 3: Non-Maximum Suppression (NMS)
        let mut nms_edges = vec![false; w * h];
        for y in 2..h - 2 {
            for x in 2..w - 2 {
                let idx = y * w + x;
                let m = mag_buf[idx];
                if m < t_low {
                    continue;
                }

                let gx = gx_buf[idx];
                let gy = gy_buf[idx];
                let angle = gy.atan2(gx).to_degrees();
                let norm_angle = if angle < 0.0 { angle + 180.0 } else { angle };

                // 4 sectors
                let is_max = if (norm_angle >= 0.0 && norm_angle < 22.5) || (norm_angle >= 157.5 && norm_angle <= 180.0) {
                    // Horizontal gradient -> compare Left / Right
                    m >= mag_buf[idx - 1] && m >= mag_buf[idx + 1]
                } else if norm_angle >= 22.5 && norm_angle < 67.5 {
                    // Diagonal 45 deg -> compare Top-Right / Bottom-Left
                    m >= mag_buf[(y - 1) * w + (x + 1)] && m >= mag_buf[(y + 1) * w + (x - 1)]
                } else if norm_angle >= 67.5 && norm_angle < 112.5 {
                    // Vertical gradient -> compare Top / Bottom
                    m >= mag_buf[(y - 1) * w + x] && m >= mag_buf[(y + 1) * w + x]
                } else {
                    // Diagonal 135 deg -> compare Top-Left / Bottom-Right
                    m >= mag_buf[(y - 1) * w + (x - 1)] && m >= mag_buf[(y + 1) * w + (x + 1)]
                };

                if is_max && m >= t_low {
                    nms_edges[idx] = true;
                }
            }
        }

        // Step 4: Contour / Chain Tracing
        let mut visited = vec![false; w * h];
        let mut chains: Vec<Vec<(f64, f64)>> = Vec::new();

        let dx8 = [-1, 0, 1, -1, 1, -1, 0, 1];
        let dy8 = [-1, -1, -1, 0, 0, 1, 1, 1];

        for y in 4..h - 4 {
            for x in 4..w - 4 {
                let start_idx = y * w + x;
                if !nms_edges[start_idx] || visited[start_idx] || mag_buf[start_idx] < t_high {
                    continue;
                }

                // Start tracing a chain
                let mut chain: Vec<(f64, f64)> = Vec::new();
                let mut cur_x = x;
                let mut cur_y = y;
                visited[start_idx] = true;
                chain.push((cur_x as f64, cur_y as f64));

                loop {
                    let mut found_next = false;
                    let mut best_m = 0.0f32;
                    let mut next_x = cur_x;
                    let mut next_y = cur_y;

                    for k in 0..8 {
                        let nx = (cur_x as isize + dx8[k]) as usize;
                        let ny = (cur_y as isize + dy8[k]) as usize;
                        if nx >= 3 && nx < w - 3 && ny >= 3 && ny < h - 3 {
                            let nidx = ny * w + nx;
                            if nms_edges[nidx] && !visited[nidx] && mag_buf[nidx] > best_m {
                                best_m = mag_buf[nidx];
                                next_x = nx;
                                next_y = ny;
                                found_next = true;
                            }
                        }
                    }

                    if found_next {
                        visited[next_y * w + next_x] = true;
                        chain.push((next_x as f64, next_y as f64));
                        cur_x = next_x;
                        cur_y = next_y;
                    } else {
                        break;
                    }
                }

                if chain.len() >= 12 {
                    chains.push(chain);
                }
            }
        }

        // Step 5: Ramer-Douglas-Peucker (RDP) Line Segment Approximation
        struct Segment {
            p1: (f64, f64),
            p2: (f64, f64),
            length: f64,
            angle_deg: f64, // Tangent orientation in [-90, +90)
            avg_mag: f64,
        }

        fn rdp(
            pts: &[(f64, f64)],
            epsilon: f64,
            mag_buf: &[f32],
            w: usize,
            out: &mut Vec<Segment>,
        ) {
            if pts.len() < 2 {
                return;
            }
            let p1 = pts[0];
            let p2 = pts[pts.len() - 1];
            let dx = p2.0 - p1.0;
            let dy = p2.1 - p1.1;
            let len_sq = dx * dx + dy * dy;
            let mut dmax = 0.0;
            let mut index = 0;

            if len_sq > 1e-4 {
                let len = len_sq.sqrt();
                for i in 1..pts.len() - 1 {
                    let p = pts[i];
                    let dist = ((p.0 - p1.0) * dy - (p.1 - p1.1) * dx).abs() / len;
                    if dist > dmax {
                        dmax = dist;
                        index = i;
                    }
                }
            }

            if dmax > epsilon {
                rdp(&pts[..=index], epsilon, mag_buf, w, out);
                rdp(&pts[index..], epsilon, mag_buf, w, out);
            } else {
                let len = len_sq.sqrt();
                if len >= 18.0 {
                    // Compute tangent angle in [-90, +90)
                    let mut ang = dy.atan2(dx).to_degrees();
                    if ang < -90.0 {
                        ang += 180.0;
                    } else if ang >= 90.0 {
                        ang -= 180.0;
                    }

                    // Compute average gradient along points
                    let mut sum_m = 0.0;
                    for p in pts {
                        let xi = (p.0.round() as usize).min(w - 1);
                        let yi = p.1.round() as usize;
                        sum_m += mag_buf[yi * w + xi] as f64;
                    }
                    let avg_m = sum_m / pts.len() as f64;

                    out.push(Segment {
                        p1,
                        p2,
                        length: len,
                        angle_deg: ang,
                        avg_mag: avg_m,
                    });
                }
            }
        }

        let mut segments: Vec<Segment> = Vec::new();
        for chain in &chains {
            rdp(chain, 1.25, &mag_buf, w, &mut segments);
        }

        if segments.is_empty() {
            return Ok(Self::fallback_straighten_suggestions());
        }

        // Step 6: Classify Segments & Continuous Kernel Density Estimation (KDE)
        // Horizontal: |angle| <= 35.0 deg. Correction = -angle.
        // Vertical: |angle - 90| <= 35.0 deg or |angle + 90| <= 35.0 deg.
        // Deviation delta_V: if angle > 0 { angle - 90 } else { angle + 90 }. Correction = -delta_V.

        struct ClassifiedLine {
            correction_deg: f64,
            weight: f64,
            is_horizontal: bool,
        }

        let mut classified: Vec<ClassifiedLine> = Vec::with_capacity(segments.len());
        let fw = w as f64;
        let fh = h as f64;

        for seg in &segments {
            let ang = seg.angle_deg;
            let mid_x = (seg.p1.0 + seg.p2.0) / 2.0;
            let mid_y = (seg.p1.1 + seg.p2.1) / 2.0;
            let span_x = (seg.p2.0 - seg.p1.0).abs();

            if ang.abs() <= 35.0 {
                // Horizontal line
                let correction = -ang;
                let mut weight = seg.length.powf(2.1) * (seg.avg_mag / 50.0).max(0.5);

                // Horizon prior: line spans across horizontal width and is in middle 15%-85% height
                if mid_y >= fh * 0.15 && mid_y <= fh * 0.85 && span_x >= fw * 0.20 {
                    weight *= 2.0;
                }

                classified.push(ClassifiedLine {
                    correction_deg: correction,
                    weight,
                    is_horizontal: true,
                });
            } else {
                let delta_v = if ang > 0.0 { ang - 90.0 } else { ang + 90.0 };
                if delta_v.abs() <= 35.0 {
                    // Vertical line
                    let correction = -delta_v;
                    let mut weight = seg.length.powf(2.0) * (seg.avg_mag / 50.0).max(0.5);

                    // Central vertical line prior: lines near horizontal center suffer less keystoning
                    let dist_from_center = (mid_x - fw / 2.0).abs() / (fw / 2.0);
                    let keystone_factor = 1.0 - 0.45 * dist_from_center;
                    weight *= keystone_factor.max(0.2);

                    classified.push(ClassifiedLine {
                        correction_deg: correction,
                        weight,
                        is_horizontal: false,
                    });
                }
            }
        }

        if classified.is_empty() {
            return Ok(Self::fallback_straighten_suggestions());
        }

        // KDE across angles -35.0 to +35.0 with step 0.1 deg (701 points)
        let n_kde = 701;
        let sigma = 0.65f64; // Gaussian standard deviation in degrees
        let two_sigma_sq = 2.0 * sigma * sigma;

        let mut kde_h = vec![0.0f64; n_kde];
        let mut kde_v = vec![0.0f64; n_kde];

        for line in &classified {
            let center_idx = (((line.correction_deg + 35.0) / 0.1).round() as isize).clamp(0, n_kde as isize - 1);
            let radius = ((3.0 * sigma / 0.1).ceil() as isize).min(30);

            for offset in -radius..=radius {
                let idx = center_idx + offset;
                if idx >= 0 && idx < n_kde as isize {
                    let a = -35.0 + (idx as f64) * 0.1;
                    let diff = a - line.correction_deg;
                    let g = (-diff * diff / two_sigma_sq).exp();
                    if line.is_horizontal {
                        kde_h[idx as usize] += line.weight * g;
                    } else {
                        kde_v[idx as usize] += line.weight * g;
                    }
                }
            }
        }

        let mut kde_total = vec![0.0f64; n_kde];
        for i in 0..n_kde {
            kde_total[i] = kde_h[i] * 1.35 + kde_v[i];
        }

        // Find local peaks in kde_total, kde_h, kde_v
        #[derive(Clone, Debug)]
        struct Peak {
            angle: f64,
            score: f64,
            is_horizon: bool,
        }

        let find_peaks = |arr: &[f64], is_h_dominant: bool| -> Vec<Peak> {
            let mut res = Vec::new();
            for i in 2..arr.len() - 2 {
                if arr[i] > arr[i - 1] && arr[i] > arr[i + 1] && arr[i] >= arr[i - 2] && arr[i] >= arr[i + 2] && arr[i] > 1e-3 {
                    let angle = -35.0 + (i as f64) * 0.1;
                    res.push(Peak {
                        angle: (angle * 10.0).round() / 10.0,
                        score: arr[i],
                        is_horizon: is_h_dominant,
                    });
                }
            }
            res.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
            res
        };

        let mut total_peaks = Vec::new();
        for i in 2..n_kde - 2 {
            if kde_total[i] > kde_total[i - 1] && kde_total[i] > kde_total[i + 1] && kde_total[i] >= kde_total[i - 2] && kde_total[i] >= kde_total[i + 2] && kde_total[i] > 1e-3 {
                let angle = -35.0 + (i as f64) * 0.1;
                let is_h = kde_h[i] * 1.35 >= kde_v[i];
                total_peaks.push(Peak {
                    angle: (angle * 10.0).round() / 10.0,
                    score: kde_total[i],
                    is_horizon: is_h,
                });
            }
        }
        total_peaks.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let h_peaks = find_peaks(&kde_h, true);
        let v_peaks = find_peaks(&kde_v, false);

        let mut suggestions: Vec<StraightenSuggestion> = Vec::new();

        // 1. Primary Suggestion: Top peak from total line orientation
        if let Some(p1) = total_peaks.get(0) {
            let clean_ang = if p1.angle.abs() < 0.2 { 0.0 } else { p1.angle };
            let label = if clean_ang == 0.0 {
                "Level"
            } else if p1.is_horizon {
                "Horizon"
            } else {
                "Verticals"
            };
            let desc = if clean_ang == 0.0 {
                "Image appears already level"
            } else if p1.is_horizon {
                "Aligned to dominant horizon line"
            } else {
                "Aligned to prominent vertical architecture"
            };
            suggestions.push(StraightenSuggestion {
                id: 1,
                label: label.to_string(),
                angle: clean_ang,
                confidence: 0.95,
                description: desc.to_string(),
            });
        } else {
            suggestions.push(StraightenSuggestion {
                id: 1,
                label: "Level".to_string(),
                angle: 0.0,
                confidence: 0.5,
                description: "Neutral horizon alignment".to_string(),
            });
        }

        let p1_ang = suggestions[0].angle;
        let p1_is_h = suggestions[0].label == "Horizon" || (suggestions[0].label == "Level" && !h_peaks.is_empty());

        // 2. Secondary Suggestion: Complementary mode (Verticals if p1 was Horizon, Horizon if p1 was Verticals)
        let complementary = if p1_is_h {
            v_peaks.iter().find(|p| (p.angle - p1_ang).abs() >= 0.8)
        } else {
            h_peaks.iter().find(|p| (p.angle - p1_ang).abs() >= 0.8)
        };

        if let Some(p2) = complementary {
            let label = if p2.is_horizon { "Horizon" } else { "Verticals" };
            let desc = if p2.is_horizon {
                "Aligned to horizontal horizon lines"
            } else {
                "Aligned to vertical architectural pillars"
            };
            suggestions.push(StraightenSuggestion {
                id: 2,
                label: label.to_string(),
                angle: p2.angle,
                confidence: 0.80,
                description: desc.to_string(),
            });
        } else if let Some(p2_alt) = total_peaks.iter().find(|p| (p.angle - p1_ang).abs() >= 1.5) {
            let label = if p2_alt.is_horizon { "Horizon 2" } else { "Verticals" };
            suggestions.push(StraightenSuggestion {
                id: 2,
                label: label.to_string(),
                angle: p2_alt.angle,
                confidence: 0.75,
                description: "Secondary prominent alignment".to_string(),
            });
        } else {
            // Intelligent subtle option
            let sub_ang = if p1_ang.abs() >= 1.0 {
                (p1_ang * 0.5 * 10.0).round() / 10.0
            } else if p1_ang != 0.0 {
                0.0
            } else {
                1.0
            };
            suggestions.push(StraightenSuggestion {
                id: 2,
                label: if sub_ang == 0.0 { "Original" } else { "Subtle" }.to_string(),
                angle: sub_ang,
                confidence: 0.70,
                description: "Gentle fine-tuning adjustment".to_string(),
            });
        }

        // 3. Third Suggestion: Distinct 3rd peak, or Original (0.0°), or subtle half
        let p2_ang = suggestions[1].angle;
        let third_peak = total_peaks
            .iter()
            .find(|p| (p.angle - p1_ang).abs() >= 1.5 && (p.angle - p2_ang).abs() >= 1.5);

        if let Some(p3) = third_peak {
            suggestions.push(StraightenSuggestion {
                id: 3,
                label: "Alternative".to_string(),
                angle: p3.angle,
                confidence: 0.65,
                description: "Alternative line alignment candidate".to_string(),
            });
        } else if p1_ang != 0.0 && p2_ang != 0.0 {
            suggestions.push(StraightenSuggestion {
                id: 3,
                label: "Original".to_string(),
                angle: 0.0,
                confidence: 0.60,
                description: "Neutral unrotated baseline".to_string(),
            });
        } else {
            let fine_ang = if p1_ang != 0.0 {
                (-p1_ang * 0.5 * 10.0).round() / 10.0
            } else {
                -1.0
            };
            suggestions.push(StraightenSuggestion {
                id: 3,
                label: "Fine Tune".to_string(),
                angle: fine_ang,
                confidence: 0.60,
                description: "Alternative fine leveling".to_string(),
            });
        }

        for sug in &mut suggestions {
            if sug.angle.abs() < 0.05 {
                sug.angle = 0.0;
            }
        }

        Ok(suggestions)
    }

    fn fallback_straighten_suggestions() -> Vec<StraightenSuggestion> {
        vec![
            StraightenSuggestion {
                id: 1,
                label: "Horizon".to_string(),
                angle: 0.0,
                confidence: 0.5,
                description: "Neutral horizon alignment".to_string(),
            },
            StraightenSuggestion {
                id: 2,
                label: "Subtle".to_string(),
                angle: 1.0,
                confidence: 0.4,
                description: "Minor tilt adjustment".to_string(),
            },
            StraightenSuggestion {
                id: 3,
                label: "Alternative".to_string(),
                angle: -1.0,
                confidence: 0.3,
                description: "Counter-tilt adjustment".to_string(),
            },
        ]
    }

    pub fn apply_crop(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        if !target_path.exists() {
            return Err("Target file does not exist on disk".to_string());
        }

        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let width = width.clamp(0.01, 1.0 - x);
        let height = height.clamp(0.01, 1.0 - y);

        let settings = db.get_app_settings().unwrap_or_default();

        let mut manifest = match self.find_session_for_photo(&photo) {
            Some(m) => m,
            None => {
                let session_dir = self.get_session_dir(&photo.file_hash);
                fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

                let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let backup_path = self.ensure_backup(&target_path, &settings, &session_dir)?;
                let _ = self.ensure_base_proxy(&session_dir, &target_path, &photo.file_hash, thumbnailer)?;

                let initial_step = EditStep {
                    step_id: 0,
                    op: "original".to_string(),
                    params: json!({}),
                    timestamp: mtime as i64,
                    render_file: Some("step_0.jpg".to_string()),
                    cumulative_rotation: 0,
                };

                let m = EditSessionManifest {
                    photo_id,
                    original_path: photo.file_path.clone(),
                    backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
                    base_hash: photo.file_hash.clone(),
                    base_mtime: mtime,
                    base_size: meta.len(),
                    last_saved_mtime: mtime,
                    last_saved_size: meta.len(),
                    current_step: 0,
                    steps: vec![initial_step],
                    is_committed: false,
                };
                self.save_manifest(&m)?;
                m
            }
        };

        let session_dir = self.get_session_dir(&manifest.base_hash);
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

        // Check external conflict
        let has_conflict = Self::check_external_conflict(&manifest, &target_path);
        if has_conflict {
            let session_state = self.get_session_state_internal(db, photo_id)?;
            return Ok(EditResult {
                success: false,
                photo: Some(photo),
                session_state,
                message: Some("External changes detected on image file".to_string()),
            });
        }

        // Truncate any redo steps beyond current_step
        manifest.steps.truncate(manifest.current_step + 1);

        // Remove orphaned forward step files
        let mut orphan_id = manifest.current_step + 1;
        loop {
            let orphan_file = session_dir.join(format!("step_{}.jpg", orphan_id));
            if orphan_file.exists() {
                let _ = fs::remove_file(orphan_file);
                orphan_id += 1;
            } else {
                break;
            }
        }

        // Previous step render file
        let prev_render_name = manifest
            .steps
            .get(manifest.current_step)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_crop = if prev_path.exists() {
            prev_path
        } else {
            self.ensure_base_proxy(&session_dir, &target_path, &manifest.base_hash, thumbnailer)?
        };

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let next_step_id = manifest.steps.len();
        let step_file_name = format!("step_{}.jpg", next_step_id);
        let dest_step_path = session_dir.join(&step_file_name);

        // Crop the previous step proxy image
        let img = image::open(&src_to_crop).map_err(|e| format!("Failed to open image: {}", e))?;
        let px_x = ((x * img.width() as f64).round() as u32).min(img.width().saturating_sub(1));
        let px_y = ((y * img.height() as f64).round() as u32).min(img.height().saturating_sub(1));
        let px_w = ((width * img.width() as f64).round() as u32).max(1).min(img.width() - px_x);
        let px_h = ((height * img.height() as f64).round() as u32).max(1).min(img.height() - px_y);
        let cropped = img.crop_imm(px_x, px_y, px_w, px_h);

        let rgb = cropped.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                cropped.width(),
                cropped.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| format!("JPEG encode error: {}", e))?;
        drop(writer);
        let _ = fs::rename(&tmp_dest, &dest_step_path);

        let prev_cum_rot = manifest
            .steps
            .get(manifest.current_step)
            .map(|s| s.cumulative_rotation)
            .unwrap_or(0);

        manifest.steps.push(EditStep {
            step_id: next_step_id,
            op: "crop".to_string(),
            params: json!({ "x": x, "y": y, "width": width, "height": height }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        // Fast update thumbnail cache for gallery view
        let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
        let _ = fs::copy(&dest_step_path, &thumb_path);

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn apply_straighten(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        angle: f64,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        if !target_path.exists() {
            return Err("Target file does not exist on disk".to_string());
        }

        let angle = angle.clamp(-45.0, 45.0);
        let settings = db.get_app_settings().unwrap_or_default();

        let mut manifest = match self.find_session_for_photo(&photo) {
            Some(m) => m,
            None => {
                let session_dir = self.get_session_dir(&photo.file_hash);
                fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

                let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let backup_path = self.ensure_backup(&target_path, &settings, &session_dir)?;
                let _ = self.ensure_base_proxy(&session_dir, &target_path, &photo.file_hash, thumbnailer)?;

                let initial_step = EditStep {
                    step_id: 0,
                    op: "original".to_string(),
                    params: json!({}),
                    timestamp: mtime as i64,
                    render_file: Some("step_0.jpg".to_string()),
                    cumulative_rotation: 0,
                };

                let m = EditSessionManifest {
                    photo_id,
                    original_path: photo.file_path.clone(),
                    backup_path: backup_path.map(|p| p.to_string_lossy().to_string()),
                    base_hash: photo.file_hash.clone(),
                    base_mtime: mtime,
                    base_size: meta.len(),
                    last_saved_mtime: mtime,
                    last_saved_size: meta.len(),
                    current_step: 0,
                    steps: vec![initial_step],
                    is_committed: false,
                };
                self.save_manifest(&m)?;
                m
            }
        };

        let session_dir = self.get_session_dir(&manifest.base_hash);
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

        // Check external conflict
        let has_conflict = Self::check_external_conflict(&manifest, &target_path);
        if has_conflict {
            let session_state = self.get_session_state_internal(db, photo_id)?;
            return Ok(EditResult {
                success: false,
                photo: Some(photo),
                session_state,
                message: Some("External changes detected on image file".to_string()),
            });
        }

        // Truncate any redo steps beyond current_step
        manifest.steps.truncate(manifest.current_step + 1);

        // Remove orphaned forward step files
        let mut orphan_id = manifest.current_step + 1;
        loop {
            let orphan_file = session_dir.join(format!("step_{}.jpg", orphan_id));
            if orphan_file.exists() {
                let _ = fs::remove_file(orphan_file);
                orphan_id += 1;
            } else {
                break;
            }
        }

        // Previous step render file
        let prev_render_name = manifest
            .steps
            .get(manifest.current_step)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_straighten = if prev_path.exists() {
            prev_path
        } else {
            self.ensure_base_proxy(&session_dir, &target_path, &manifest.base_hash, thumbnailer)?
        };

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let next_step_id = manifest.steps.len();
        let step_file_name = format!("step_{}.jpg", next_step_id);
        let dest_step_path = session_dir.join(&step_file_name);

        Self::render_straighten_step_proxy(&src_to_straighten, &dest_step_path, angle)?;

        let prev_cum_rot = manifest
            .steps
            .get(manifest.current_step)
            .map(|s| s.cumulative_rotation)
            .unwrap_or(0);

        manifest.steps.push(EditStep {
            step_id: next_step_id,
            op: "straighten".to_string(),
            params: json!({ "angle": angle }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        // Fast update thumbnail cache for gallery view
        let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
        let _ = fs::copy(&dest_step_path, &thumb_path);

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn undo_edit(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        if manifest.current_step == 0 {
            return Err("Nothing to undo".to_string());
        }

        manifest.current_step -= 1;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        let cum_rot = Self::calculate_cumulative_rotation(&manifest.steps, manifest.current_step);
        let _ = db.update_photo_edit_rotation(photo_id, cum_rot);

        // Update gallery thumbnail from the step render file
        let session_dir = self.get_session_dir(&manifest.base_hash);
        if let Some(step) = manifest.steps.get(manifest.current_step) {
            if let Some(ref rf) = step.render_file {
                let step_path = session_dir.join(rf);
                if step_path.exists() {
                    let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
                    let _ = fs::copy(&step_path, &thumb_path);
                }
            }
        }

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn redo_edit(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        if manifest.current_step >= manifest.steps.len().saturating_sub(1) {
            return Err("Nothing to redo".to_string());
        }

        manifest.current_step += 1;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        let cum_rot = Self::calculate_cumulative_rotation(&manifest.steps, manifest.current_step);
        let _ = db.update_photo_edit_rotation(photo_id, cum_rot);

        // Update gallery thumbnail from the step render file
        let session_dir = self.get_session_dir(&manifest.base_hash);
        if let Some(step) = manifest.steps.get(manifest.current_step) {
            if let Some(ref rf) = step.render_file {
                let step_path = session_dir.join(rf);
                if step_path.exists() {
                    let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
                    let _ = fs::copy(&step_path, &thumb_path);
                }
            }
        }

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn jump_to_step(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        target_step: usize,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        if target_step >= manifest.steps.len() {
            return Err("Invalid step index".to_string());
        }

        manifest.current_step = target_step;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;

        let cum_rot = Self::calculate_cumulative_rotation(&manifest.steps, manifest.current_step);
        let _ = db.update_photo_edit_rotation(photo_id, cum_rot);

        let session_dir = self.get_session_dir(&manifest.base_hash);
        if let Some(step) = manifest.steps.get(manifest.current_step) {
            if let Some(ref rf) = step.render_file {
                let step_path = session_dir.join(rf);
                if step_path.exists() {
                    let thumb_path = thumbnailer.get_thumbnail_path(&photo.file_hash);
                    let _ = fs::copy(&step_path, &thumb_path);
                }
            }
        }

        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(photo),
            session_state,
            message: None,
        })
    }

    pub fn save_edit_session(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditSessionState, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let session_dir = self.get_session_dir(&manifest.base_hash);
        let internal_pristine = Self::get_pristine_path(&session_dir, &target_path);

        let base_source = if internal_pristine.exists() {
            internal_pristine
        } else if let Some(ref bp) = manifest.backup_path {
            PathBuf::from(bp)
        } else {
            target_path.clone()
        };

        let settings = db.get_app_settings().unwrap_or_default();
        let _ = self.ensure_backup(&target_path, &settings, &session_dir)?;

        // High quality render from pristine original to target file
        Self::render_pipeline_to_file(&base_source, &target_path, &manifest.steps, manifest.current_step)?;

        if settings.original_handling == "overwrite" {
            let backup_path = Self::get_backup_path(&target_path);
            if backup_path.exists() {
                let _ = fs::remove_file(backup_path);
            }
        }

        let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
        let mtime = meta
            .modified()
            .unwrap_or(SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        manifest.last_saved_mtime = mtime;
        manifest.last_saved_size = meta.len();
        manifest.is_committed = true;
        self.save_manifest(&manifest)?;

        let new_hash = Scanner::compute_file_hash(&target_path, meta.len(), mtime);
        let dims = image_dimensions(&target_path).ok();

        // Regenerate thumbnail for new hash
        if new_hash != photo.file_hash {
            let old_thumb = thumbnailer.get_thumbnail_path(&photo.file_hash);
            let _ = fs::remove_file(old_thumb);
        }
        let _ = thumbnailer.generate_thumbnail(&target_path, &new_hash, DEFAULT_THUMBNAIL_SIZE);

        db.update_photo_after_edit(
            photo.id,
            &new_hash,
            meta.len() as i64,
            dims.map(|d| d.0),
            dims.map(|d| d.1),
            Some(1),
        )
        .map_err(|e| e.to_string())?;

        self.get_session_state_internal(db, photo_id)
    }

    pub fn revert_edit_session(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let backup_path = Self::get_backup_path(&target_path);

        // Find existing session manifest to locate internal pristine copy if backup doesn't exist
        let manifest = self.find_session_for_photo(&photo);
        let internal_pristine = if let Some(ref m) = manifest {
            let sdir = self.get_session_dir(&m.base_hash);
            let p = Self::get_pristine_path(&sdir, &target_path);
            if p.exists() { Some(p) } else { None }
        } else {
            let sdir = self.get_session_dir(&photo.file_hash);
            let p = Self::get_pristine_path(&sdir, &target_path);
            if p.exists() { Some(p) } else { None }
        };

        let source_restore = if backup_path.exists() {
            Some(backup_path.clone())
        } else if let Some(p) = internal_pristine {
            Some(p)
        } else {
            None
        };

        if let Some(src) = source_restore {
            if src != target_path {
                let _ = fs::copy(&src, &target_path);
            }
            if backup_path.exists() {
                let _ = fs::remove_file(&backup_path);
            }
        }

        // Thoroughly purge ALL session directories associated with this photo across edits_dir
        if let Ok(entries) = fs::read_dir(&self.edits_dir) {
            for entry in entries.flatten() {
                let sess_file = entry.path().join("session.json");
                if sess_file.exists() {
                    if let Ok(content) = fs::read_to_string(&sess_file) {
                        if let Ok(m) = serde_json::from_str::<EditSessionManifest>(&content) {
                            if m.photo_id == photo_id || m.original_path == photo.file_path {
                                let _ = fs::remove_dir_all(entry.path());
                            }
                        }
                    }
                }
            }
        }
        let hash_dir = self.get_session_dir(&photo.file_hash);
        if hash_dir.exists() {
            let _ = fs::remove_dir_all(&hash_dir);
        }

        // Reset edit rotation in DB
        let _ = db.update_photo_edit_rotation(photo_id, 0);

        // Update photo metadata in DB with the restored original file
        if let Ok(meta) = fs::metadata(&target_path) {
            let mtime = meta
                .modified()
                .unwrap_or(SystemTime::now())
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let new_hash = Scanner::compute_file_hash(&target_path, meta.len(), mtime);
            let dims = image_dimensions(&target_path).ok();

            if new_hash != photo.file_hash {
                let old_thumb = thumbnailer.get_thumbnail_path(&photo.file_hash);
                let _ = fs::remove_file(old_thumb);
            }
            let _ = thumbnailer.generate_thumbnail(&target_path, &new_hash, DEFAULT_THUMBNAIL_SIZE);

            let _ = db.update_photo_after_edit(
                photo.id,
                &new_hash,
                meta.len() as i64,
                dims.map(|d| d.0),
                dims.map(|d| d.1),
                Some(1),
            );
        }

        let updated_photo = db.get_photo_by_id(photo_id).ok().flatten();
        let session_state = self.get_session_state_internal(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: updated_photo.or(Some(photo)),
            session_state,
            message: Some("Reverted to original photo".to_string()),
        })
    }

    pub fn resolve_conflict(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        choice: &str, // "keep_external", "apply_pica", "save_as_copy"
    ) -> Result<EditResult, String> {
        let _lock = self.session_lock.lock().unwrap();
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let session_dir = self.get_session_dir(&photo.file_hash);

        match choice {
            "keep_external" => {
                // Discard Pica's session and adopt external file
                let _ = fs::remove_dir_all(&session_dir);
                let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let new_hash = Scanner::compute_file_hash(&target_path, meta.len(), mtime);
                let dims = image_dimensions(&target_path).ok();

                let old_thumb = thumbnailer.get_thumbnail_path(&photo.file_hash);
                let _ = fs::remove_file(old_thumb);
                let _ = thumbnailer.generate_thumbnail(&target_path, &new_hash, DEFAULT_THUMBNAIL_SIZE);

                db.update_photo_after_edit(
                    photo.id,
                    &new_hash,
                    meta.len() as i64,
                    dims.map(|d| d.0),
                    dims.map(|d| d.1),
                    Some(1),
                )
                .map_err(|e| e.to_string())?;

                let updated_photo = db.get_photo_by_id(photo_id).ok().flatten();
                let session_state = self.get_session_state_internal(db, photo_id)?;
                Ok(EditResult {
                    success: true,
                    photo: updated_photo,
                    session_state,
                    message: Some("Adopted external version".to_string()),
                })
            }
            "apply_pica" => {
                // Commit Pica draft over external changes
                let session_state = self.save_edit_session(db, thumbnailer, photo_id)?;
                let updated_photo = db.get_photo_by_id(photo_id).ok().flatten();
                Ok(EditResult {
                    success: true,
                    photo: updated_photo,
                    session_state,
                    message: Some("Applied Pica draft over external changes".to_string()),
                })
            }
            "save_as_copy" => {
                // Save Pica draft to a new file, leave external untouched
                if let Some(m) = self.load_manifest(&photo.file_hash) {
                    let stem = target_path.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = target_path.extension().unwrap_or_default().to_string_lossy();
                    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
                    let new_file_name = format!("{}_pica_draft.{}", stem, ext);
                    let new_copy_path = parent.join(new_file_name);

                    let internal_pristine = Self::get_pristine_path(&session_dir, &target_path);
                    Self::render_pipeline_to_file(&internal_pristine, &new_copy_path, &m.steps, m.current_step)?;

                    let _ = fs::remove_dir_all(&session_dir);
                    let session_state = self.get_session_state_internal(db, photo_id)?;
                    Ok(EditResult {
                        success: true,
                        photo: Some(photo),
                        session_state,
                        message: Some(format!("Saved Pica draft to {}", new_copy_path.display())),
                    })
                } else {
                    Err("No manifest found".to_string())
                }
            }
            _ => Err("Invalid conflict resolution choice".to_string()),
        }
    }

    fn render_pipeline_to_file(
        source_path: &Path,
        dest_path: &Path,
        steps: &[EditStep],
        current_step: usize,
    ) -> Result<(), String> {
        let active_steps = &steps[..=current_step.min(steps.len().saturating_sub(1))];
        let has_ops = active_steps.iter().any(|s| s.op != "original");

        if !has_ops {
            // Pristine state: directly copy source to destination
            if source_path != dest_path {
                fs::copy(source_path, dest_path).map_err(|e| e.to_string())?;
            }
            return Ok(());
        }

        let ext = dest_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let is_jpeg = ext == "jpg" || ext == "jpeg";

        // Read source image
        let mut img = image::open(source_path)
            .map_err(|e| format!("Failed to open image {}: {}", source_path.display(), e))?;

        for step in active_steps {
            match step.op.as_str() {
                "rotate" => {
                    let deg = step.params.get("degrees").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                    let norm = ((deg % 360) + 360) % 360;
                    img = match norm {
                        90 => img.rotate90(),
                        180 => img.rotate180(),
                        270 => img.rotate270(),
                        _ => img,
                    };
                }
                "crop" => {
                    let x = step.params.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = step.params.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let w = step.params.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0);
                    let h = step.params.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0);

                    let cur_w = img.width();
                    let cur_h = img.height();
                    let px_x = ((x.clamp(0.0, 1.0) * cur_w as f64).round() as u32).min(cur_w.saturating_sub(1));
                    let px_y = ((y.clamp(0.0, 1.0) * cur_h as f64).round() as u32).min(cur_h.saturating_sub(1));
                    let px_w = ((w.clamp(0.0, 1.0) * cur_w as f64).round() as u32).max(1).min(cur_w - px_x);
                    let px_h = ((h.clamp(0.0, 1.0) * cur_h as f64).round() as u32).max(1).min(cur_h - px_y);
                    img = img.crop_imm(px_x, px_y, px_w, px_h);
                }
                "straighten" => {
                    let angle = step.params.get("angle").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    if angle.abs() > 1e-4 {
                        let rotated_rgb = Self::rotate_and_crop_image(&img, angle);
                        img = image::DynamicImage::ImageRgb8(rotated_rgb);
                    }
                }
                _ => {}
            }
        }

        let stem = dest_path.file_stem().unwrap_or_default().to_string_lossy();
        let ext_str = dest_path.extension().unwrap_or_default().to_string_lossy();
        let temp_dest = dest_path.with_file_name(format!("{}.tmp.{}.{}", stem, std::process::id(), ext_str));

        if is_jpeg {
            let rgb = img.to_rgb8();
            let file = File::create(&temp_dest)
                .map_err(|e| format!("Failed to create output file: {}", e))?;
            let mut writer = BufWriter::new(file);
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, 98);
            encoder
                .encode(
                    rgb.as_raw(),
                    img.width(),
                    img.height(),
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| format!("JPEG encode error: {}", e))?;
            drop(writer);

            // If exiftool is available, copy all EXIF tags from original pristine file!
            let _ = Command::new("exiftool")
                .arg("-tagsFromFile")
                .arg(source_path.to_string_lossy().to_string())
                .arg("-all:all")
                .arg("-overwrite_original")
                .arg(temp_dest.to_string_lossy().to_string())
                .output();
        } else {
            img
                .save(&temp_dest)
                .map_err(|e| format!("Failed to save image: {}", e))?;
        }

        fs::rename(&temp_dest, dest_path).map_err(|e| format!("Failed to finalize file: {}", e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PhotoInsert;
    use tempfile::tempdir;

    #[test]
    fn test_editor_rotation_and_undo_redo() {
        let temp_dir = tempdir().unwrap();
        let photos_dir = temp_dir.path().join("Photos");
        fs::create_dir_all(&photos_dir).unwrap();

        // Create a 100x50 test image
        let test_img_path = photos_dir.join("test.png");
        let img = image::RgbImage::new(100, 50);
        img.save(&test_img_path).unwrap();

        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(photos_dir.to_str().unwrap(), "Photos", "always").unwrap();

        let initial_hash = Scanner::compute_file_hash(&test_img_path, 100, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash.clone(),
            file_name: "test.png".to_string(),
            file_size: 100,
            width: Some(100),
            height: Some(50),
            date_taken: Some(1000),
            camera_make: None,
            camera_model: None,
            orientation: Some(1),
        }]).unwrap();

        let photo = db.get_photo_by_path(test_img_path.to_str().unwrap()).unwrap().unwrap();

        let thumb_dir = temp_dir.path().join("thumbs");
        let thumbnailer = Arc::new(ThumbnailManager::with_cache_dir(thumb_dir));

        let edits_dir = temp_dir.path().join("edits");
        let editor = EditorManager::with_edits_dir(edits_dir);

        // 1. Apply 90° rotation
        let res1 = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert!(res1.success);
        let s1 = res1.session_state;
        assert_eq!(s1.cumulative_rotation, 90);
        assert_eq!(s1.current_step, 1);
        assert_eq!(s1.total_steps, 2);
        assert!(s1.can_undo);
        assert!(!s1.can_redo);
        assert!(!s1.is_committed);

        // Check proxy files in session directory
        let session_dir = editor.get_session_dir(&photo.file_hash);
        assert!(session_dir.join("step_0.jpg").exists());
        assert!(session_dir.join("step_1.jpg").exists());
        let manifest = editor.load_manifest(&photo.file_hash).unwrap();
        assert_eq!(manifest.steps[1].render_file, Some("step_1.jpg".to_string()));
        assert_eq!(manifest.steps[1].cumulative_rotation, 90);

        // On disk, target file has NOT been modified yet (fast draft mode!)
        let (w_draft, h_draft) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w_draft, h_draft), (100, 50));

        // Check backup was created in .pica_originals
        let backup_path = EditorManager::get_backup_path(&test_img_path);
        assert!(backup_path.exists());

        // 2. Undo
        let res_undo = editor.undo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_undo.success);
        let s_undo = res_undo.session_state;
        assert_eq!(s_undo.cumulative_rotation, 0);
        assert_eq!(s_undo.current_step, 0);
        assert!(!s_undo.can_undo);
        assert!(s_undo.can_redo);

        // 3. Redo
        let res_redo = editor.redo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_redo.success);
        let s_redo = res_redo.session_state;
        assert_eq!(s_redo.cumulative_rotation, 90);
        assert_eq!(s_redo.current_step, 1);

        // 4. Save (commit) -> high quality render written to disk
        let s_saved = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(s_saved.is_committed);

        // Check image dimensions on disk: now committed as 50x100!
        let (w, h) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w, h), (50, 100));

        // 5. Revert
        let res_revert = editor.revert_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_revert.success);
        let (w_rev, h_rev) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w_rev, h_rev), (100, 50));
        assert!(!res_revert.session_state.has_session);
        assert_eq!(res_revert.session_state.steps.len(), 0);

        // 6. Test post-revert fresh edit: should start from step 1 cleanly with no conflict!
        let res_post_revert = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert!(res_post_revert.success);
        assert_eq!(res_post_revert.session_state.current_step, 1);
        assert_eq!(res_post_revert.session_state.total_steps, 2);
        assert_eq!(res_post_revert.session_state.cumulative_rotation, 90);

        // 7. Add step 2
        let res_step2 = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert!(res_step2.success);
        assert_eq!(res_step2.session_state.current_step, 2);
        assert_eq!(res_step2.session_state.total_steps, 3);
        assert_eq!(res_step2.session_state.cumulative_rotation, 180);

        // 8. Undo back to step 1
        let res_undo_step1 = editor.undo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_undo_step1.success);
        assert_eq!(res_undo_step1.session_state.current_step, 1);

        // 9. Branching edit after undo: rotate -90° should truncate forward step 2 and replace it
        let res_branch = editor.apply_rotation(&db, &thumbnailer, photo.id, -90).unwrap();
        assert!(res_branch.success);
        assert_eq!(res_branch.session_state.current_step, 2);
        assert_eq!(res_branch.session_state.total_steps, 3);
        assert_eq!(res_branch.session_state.cumulative_rotation, 0);
    }

    #[test]
    fn test_editor_crop_suggestions_and_apply() {
        let temp_dir = tempdir().unwrap();
        let photos_dir = temp_dir.path().join("Photos");
        fs::create_dir_all(&photos_dir).unwrap();

        // Create a 200x100 test image with some high contrast lines
        let test_img_path = photos_dir.join("crop_test.jpg");
        let mut img = image::RgbImage::new(200, 100);
        // Draw a bright square in the center
        for y in 40..60 {
            for x in 80..120 {
                img.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        img.save(&test_img_path).unwrap();

        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(photos_dir.to_str().unwrap(), "Photos", "always").unwrap();

        let initial_hash = Scanner::compute_file_hash(&test_img_path, 200, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash.clone(),
            file_name: "crop_test.jpg".to_string(),
            file_size: 200,
            width: Some(200),
            height: Some(100),
            date_taken: Some(1000),
            camera_make: None,
            camera_model: None,
            orientation: Some(1),
        }]).unwrap();

        let photo = db.get_photo_by_path(test_img_path.to_str().unwrap()).unwrap().unwrap();
        let thumb_dir = temp_dir.path().join("thumbs");
        let thumbnailer = Arc::new(ThumbnailManager::with_cache_dir(thumb_dir));
        let edits_dir = temp_dir.path().join("edits");
        let editor = EditorManager::with_edits_dir(edits_dir);

        // Test crop suggestions for 1:1 aspect ratio
        let suggestions = editor.get_crop_suggestions(&db, &thumbnailer, photo.id, Some(1.0)).unwrap();
        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].label, "Tight Focus");
        assert_eq!(suggestions[1].label, "Rule of Thirds");
        assert_eq!(suggestions[2].label, "Wide Context");
        for sug in &suggestions {
            assert!(sug.x >= 0.0 && sug.x <= 1.0);
            assert!(sug.y >= 0.0 && sug.y <= 1.0);
            assert!(sug.width > 0.0 && sug.x + sug.width <= 1.0001);
            assert!(sug.height > 0.0 && sug.y + sug.height <= 1.0001);
        }

        // Apply crop: center 50%
        let res_crop = editor.apply_crop(&db, &thumbnailer, photo.id, 0.25, 0.25, 0.5, 0.5).unwrap();
        assert!(res_crop.success);
        let s_crop = res_crop.session_state;
        assert_eq!(s_crop.current_step, 1);
        assert!(s_crop.can_undo);

        // Commit (save)
        let s_saved = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(s_saved.is_committed);

        // Verify dimensions on disk
        let (w, h) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w, h), (100, 50));
    }

    #[test]
    fn test_editor_rotation_then_crop_composability() {
        let temp_dir = tempdir().unwrap();
        let photos_dir = temp_dir.path().join("photos");
        fs::create_dir_all(&photos_dir).unwrap();

        // Create an asymmetric 200x100 image: left half RED, right half BLUE
        let test_img_path = photos_dir.join("rot_crop_test.jpg");
        let mut img = image::RgbImage::new(200, 100);
        for y in 0..100 {
            for x in 0..200 {
                if x < 100 {
                    img.put_pixel(x, y, image::Rgb([255, 0, 0])); // Red on left
                } else {
                    img.put_pixel(x, y, image::Rgb([0, 0, 255])); // Blue on right
                }
            }
        }
        img.save(&test_img_path).unwrap();

        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(photos_dir.to_str().unwrap(), "Photos", "always").unwrap();

        let initial_hash = Scanner::compute_file_hash(&test_img_path, 200, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash.clone(),
            file_name: "rot_crop_test.jpg".to_string(),
            file_size: 200,
            width: Some(200),
            height: Some(100),
            date_taken: Some(1000),
            camera_make: None,
            camera_model: None,
            orientation: Some(1),
        }]).unwrap();

        let photo = db.get_photo_by_path(test_img_path.to_str().unwrap()).unwrap().unwrap();
        let thumb_dir = temp_dir.path().join("thumbs");
        let thumbnailer = Arc::new(ThumbnailManager::with_cache_dir(thumb_dir));
        let edits_dir = temp_dir.path().join("edits");
        let editor = EditorManager::with_edits_dir(edits_dir);

        // 1. Rotate 90°
        let r1 = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert_eq!(r1.session_state.cumulative_rotation, 90);

        // 2. Rotate 90° again (total 180° - upside down)
        // Red is now on the RIGHT half (x: 100..200), Blue is on the LEFT half (x: 0..100)
        let r2 = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert_eq!(r2.session_state.cumulative_rotation, 180);

        // 3. Crop left half of the 180°-rotated image (x: 0.0..0.5, y: 0.0..1.0)
        // This should crop the BLUE region!
        let r3 = editor.apply_crop(&db, &thumbnailer, photo.id, 0.0, 0.0, 0.5, 1.0).unwrap();
        assert!(r3.success);
        assert_eq!(r3.session_state.cumulative_rotation, 180);
        assert_eq!(r3.session_state.current_step, 3);

        // 4. Save and commit
        let s_saved = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(s_saved.is_committed);

        // Verify dimensions and pixel color on disk:
        let saved_img = image::open(&test_img_path).unwrap().to_rgb8();
        assert_eq!(saved_img.width(), 100);
        assert_eq!(saved_img.height(), 100);

        // The cropped region is blue (high B, low R)
        let center_pixel = saved_img.get_pixel(50, 50);
        assert!(center_pixel[2] > 200, "Expected blue pixel, got {:?}", center_pixel);
        assert!(center_pixel[0] < 50, "Expected low red, got {:?}", center_pixel);
    }

    #[test]
    fn test_straighten_edit_and_save() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db = DbManager::new_in_memory().unwrap();
        let test_img_path = temp_dir.path().join("test_straighten.jpg");

        let img = image::RgbImage::from_fn(200, 150, |x, _y| {
            if x < 100 {
                image::Rgb([255, 0, 0])
            } else {
                image::Rgb([0, 0, 255])
            }
        });
        img.save(&test_img_path).unwrap();

        let folder_id = db.add_or_update_folder(temp_dir.path().to_str().unwrap(), "Test", "always").unwrap();
        let initial_hash = Scanner::compute_file_hash(&test_img_path, 1000, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash,
            file_name: "test_straighten.jpg".to_string(),
            file_size: 1000,
            width: Some(200),
            height: Some(150),
            date_taken: Some(1000),
            camera_make: None,
            camera_model: None,
            orientation: Some(1),
        }]).unwrap();

        let photo = db.get_photo_by_path(test_img_path.to_str().unwrap()).unwrap().unwrap();
        let thumb_dir = temp_dir.path().join("thumbs");
        let thumbnailer = Arc::new(ThumbnailManager::with_cache_dir(thumb_dir));
        let edits_dir = temp_dir.path().join("edits");
        let editor = EditorManager::with_edits_dir(edits_dir);

        // 1. Apply straighten angle 5.0 deg
        let res = editor.apply_straighten(&db, &thumbnailer, photo.id, 5.0).unwrap();
        assert!(res.success);
        assert_eq!(res.session_state.current_step, 1);
        assert_eq!(res.session_state.steps.len(), 2);
        assert_eq!(res.session_state.steps[1].op, "straighten");

        // 2. Save session
        let saved_session = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(saved_session.is_committed);

        let saved_img = image::open(&test_img_path).unwrap();
        assert_eq!(saved_img.width(), 200);
        assert_eq!(saved_img.height(), 150);
    }

    #[test]
    fn test_straighten_suggestions_detection() {
        let temp_dir = tempdir().unwrap();
        let test_img_path = temp_dir.path().join("test_horizon.jpg");

        // Create an image with a distinct horizontal line (e.g. top half black, bottom half white)
        let img = image::RgbImage::from_fn(200, 150, |_x, y| {
            if y < 75 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([255, 255, 255])
            }
        });
        img.save(&test_img_path).unwrap();

        let suggestions = EditorManager::compute_straighten_suggestions(&test_img_path).unwrap();
        assert_eq!(suggestions.len(), 3);
        assert_eq!(suggestions[0].id, 1);
        assert_eq!(suggestions[1].id, 2);
        assert_eq!(suggestions[2].id, 3);
        // For a perfectly horizontal line, top suggestion should be near 0 deg
        assert!(suggestions[0].angle.abs() <= 1.0);
    }
}


