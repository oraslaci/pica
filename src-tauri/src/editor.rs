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
use crate::faces::traits::FaceDetector;
use crate::models::{
    AppSettings, CropSuggestion, EditResult, EditSessionManifest, EditSessionState, EditStep,
    Photo, RedEyeBox, RetouchSpot, StraightenSuggestion, TextItem,
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
                            let is_unsaved = !m.is_committed && m.current_step > 0;
                            let rot = if is_unsaved {
                                Self::calculate_cumulative_rotation(&m.steps, m.current_step)
                            } else {
                                0
                            };
                            let _ = db.update_photo_edit_rotation(m.photo_id, rot);
                            let _ = db.update_photo_unsaved_edits(m.photo_id, is_unsaved);
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
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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

    pub fn apply_redeye_correction_to_image(img: &mut image::DynamicImage, params: &serde_json::Value) {
        let boxes_val = match params.get("boxes").and_then(|v| v.as_array()) {
            Some(b) => b,
            None => return,
        };

        if boxes_val.is_empty() {
            return;
        }

        let mut rgba = img.to_rgba8();
        let (img_w, img_h) = (rgba.width(), rgba.height());
        if img_w == 0 || img_h == 0 {
            return;
        }

        for b in boxes_val {
            let bx = b.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0);
            let by = b.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.0, 1.0);
            let bw = b.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.001, 1.0 - bx);
            let bh = b.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0).clamp(0.001, 1.0 - by);

            let px_x = ((bx * img_w as f64).round() as u32).min(img_w.saturating_sub(1));
            let px_y = ((by * img_h as f64).round() as u32).min(img_h.saturating_sub(1));
            let px_w = ((bw * img_w as f64).round() as u32).max(1).min(img_w - px_x);
            let px_h = ((bh * img_h as f64).round() as u32).max(1).min(img_h - px_y);

            let cx = px_x as f32 + (px_w as f32) * 0.5;
            let cy = px_y as f32 + (px_h as f32) * 0.5;
            let rx = (px_w as f32) * 0.5;
            let ry = (px_h as f32) * 0.5;

            for y in px_y..(px_y + px_h) {
                for x in px_x..(px_x + px_w) {
                    let pixel = rgba.get_pixel_mut(x, y);
                    let r = pixel[0] as f32;
                    let g = pixel[1] as f32;
                    let b = pixel[2] as f32;

                    // 1. Catchlight test: preserve natural corneal specular reflection
                    if r > 185.0 && g > 185.0 && b > 185.0
                        && (r - g).abs() < 35.0
                        && (r - b).abs() < 35.0
                    {
                        continue;
                    }

                    // Normalized radial distance from center of bounding box
                    let dx = (x as f32 - cx) / rx.max(1.0);
                    let dy = (y as f32 - cy) / ry.max(1.0);
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq > 1.0 {
                        continue;
                    }
                    let falloff = (1.0 - dist_sq.sqrt()).clamp(0.0, 1.0);

                    // 2. Redness metrics
                    let redness_diff = r - (g + b) * 0.5;
                    let max_gb = g.max(b);
                    let redness_ratio = r / (max_gb + 1.0);

                    if redness_diff > 12.0 && redness_ratio > 1.25 && r > 40.0 {
                        let target_lum = (g + b) * 0.5;
                        let dark_pupil = target_lum * 0.45;
                        let ratio_weight = ((redness_ratio - 1.25) / 0.40).clamp(0.0, 1.0);
                        let total_weight = ratio_weight * (0.6 + 0.4 * falloff);

                        pixel[0] = ((r * (1.0 - total_weight) + dark_pupil * total_weight).clamp(0.0, 255.0)) as u8;
                        pixel[1] = ((g * (1.0 - total_weight) + (target_lum * 0.65) * total_weight).clamp(0.0, 255.0)) as u8;
                        pixel[2] = ((b * (1.0 - total_weight) + (target_lum * 0.65) * total_weight).clamp(0.0, 255.0)) as u8;
                    } else if dist_sq < 0.40 && (g > 100.0 && g > 1.35 * b && r > 80.0) {
                        // Pet eye reflection (tapetum lucidum glow)
                        let target_lum = (r + b) * 0.4;
                        let glow_weight = 0.7 * (1.0 - dist_sq / 0.40);
                        pixel[0] = (r * (1.0 - glow_weight) + target_lum * 0.5 * glow_weight) as u8;
                        pixel[1] = (g * (1.0 - glow_weight) + target_lum * 0.5 * glow_weight) as u8;
                        pixel[2] = (b * (1.0 - glow_weight) + target_lum * 0.5 * glow_weight) as u8;
                    }
                }
            }
        }

        *img = image::DynamicImage::ImageRgba8(rgba);
    }

    pub fn auto_detect_red_eyes(
        &self,
        photo: &Photo,
        detector: &dyn FaceDetector,
    ) -> Result<Vec<RedEyeBox>, String> {
        let path = PathBuf::from(&photo.file_path);
        if !path.exists() {
            return Err("Photo file not found".to_string());
        }

        let img = image::open(&path).map_err(|e| format!("Failed to open photo: {}", e))?;
        let (orig_w, orig_h) = (img.width(), img.height());
        if orig_w == 0 || orig_h == 0 {
            return Ok(Vec::new());
        }

        // 1. Scan for compact, circular red pupil candidates in the image
        let max_scan_dim = 1024u32;
        let scan_scale = (max_scan_dim as f64) / (orig_w.max(orig_h) as f64).max(1.0);
        let scan_w = if scan_scale < 1.0 { ((orig_w as f64 * scan_scale).round() as u32).max(1) } else { orig_w };
        let scan_h = if scan_scale < 1.0 { ((orig_h as f64 * scan_scale).round() as u32).max(1) } else { orig_h };
        let scan_img = if scan_scale < 1.0 { img.thumbnail(scan_w, scan_h) } else { img.clone() };
        let rgb_scan = scan_img.to_rgb8();

        let mut red_mask = vec![false; (scan_w * scan_h) as usize];
        for sy in 0..scan_h {
            for sx in 0..scan_w {
                let pix = rgb_scan.get_pixel(sx, sy);
                let (r, g, b) = (pix[0] as f32, pix[1] as f32, pix[2] as f32);
                let sum = (r + g + b).max(1.0);
                let chroma = r / sum;
                // High red chromaticity (>=50%) and clear dominance over G and B.
                // This reliably rejects skin (chroma ~0.40..0.45) and dark hair shadows.
                if r >= 85.0 && chroma >= 0.50 && r >= 1.55 * g && r >= 1.55 * b && (r - g.max(b)) >= 22.0 {
                    red_mask[(sy * scan_w + sx) as usize] = true;
                }
            }
        }

        // BFS connected components to discover distinct red pupil blobs
        let mut visited = vec![false; (scan_w * scan_h) as usize];
        struct PupilCandidate {
            cx: f64,
            cy: f64,
            size_norm: f64,
        }

        let min_pupil_count = ((scan_w * scan_h) as f64 * 0.00015).round() as usize;
        let min_count = min_pupil_count.clamp(25, 200);

        let mut pupils: Vec<PupilCandidate> = Vec::new();
        for sy in 0..scan_h {
            for sx in 0..scan_w {
                let idx = (sy * scan_w + sx) as usize;
                if red_mask[idx] && !visited[idx] {
                    visited[idx] = true;
                    let mut queue = vec![(sx, sy)];
                    let mut pts = Vec::new();
                    let mut q_head = 0;
                    while q_head < queue.len() {
                        let (qx, qy) = queue[q_head];
                        q_head += 1;
                        pts.push((qx, qy));

                        for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                            let nx = qx as i32 + dx;
                            let ny = qy as i32 + dy;
                            if nx >= 0 && nx < scan_w as i32 && ny >= 0 && ny < scan_h as i32 {
                                let nidx = (ny as u32 * scan_w + nx as u32) as usize;
                                if red_mask[nidx] && !visited[nidx] {
                                    visited[nidx] = true;
                                    queue.push((nx as u32, ny as u32));
                                }
                            }
                        }
                    }

                    if pts.len() >= min_count {
                        let min_x = pts.iter().map(|p| p.0).min().unwrap();
                        let max_x = pts.iter().map(|p| p.0).max().unwrap();
                        let min_y = pts.iter().map(|p| p.1).min().unwrap();
                        let max_y = pts.iter().map(|p| p.1).max().unwrap();
                        let sum_x: u64 = pts.iter().map(|p| p.0 as u64).sum();
                        let sum_y: u64 = pts.iter().map(|p| p.1 as u64).sum();
                        let cx = (sum_x as f64 / pts.len() as f64) / scan_w as f64;
                        let cy = (sum_y as f64 / pts.len() as f64) / scan_h as f64;

                        // Keep away from outermost image borders
                        if cx >= 0.03 && cx <= 0.97 && cy >= 0.03 && cy <= 0.97 {
                            let cw = (max_x - min_x + 1) as f64 / scan_w as f64;
                            let ch = (max_y - min_y + 1) as f64 / scan_h as f64;
                            let aspect = (max_x - min_x + 1) as f64 / (max_y - min_y + 1).max(1) as f64;
                            let max_dim = cw.max(ch);

                            // Pupils are compact and roughly circular (aspect ratio 0.65..1.45),
                            // rejecting wide shapes like lips or long thin lines like shadows.
                            if aspect >= 0.65 && aspect <= 1.45 && max_dim >= 0.012 && max_dim <= 0.16 {
                                pupils.push(PupilCandidate {
                                    cx,
                                    cy,
                                    size_norm: max_dim,
                                });
                            }
                        }
                    }
                }
            }
        }

        let faces = detector.detect_faces(&img).unwrap_or_default();
        let mut final_boxes: Vec<RedEyeBox> = Vec::new();
        let mut claimed_pupils = vec![false; pupils.len()];

        // 2. Face Landmark Snapping
        for (f_idx, face) in faces.iter().enumerate() {
            if let Some(ref lms) = face.landmarks {
                let rx = lms[0].0 as f64;
                let ry = lms[0].1 as f64;
                let lx = lms[1].0 as f64;
                let ly = lms[1].1 as f64;
                let iod = ((lx - rx).powi(2) + (ly - ry).powi(2)).sqrt();
                let default_box_size = (iod * 0.40).clamp(0.04, 0.18);
                let search_radius = (iod * 0.65).clamp(0.06, 0.22);

                // Right eye
                let mut r_pupil = None;
                let mut r_min_d = search_radius;
                for (p_idx, p) in pupils.iter().enumerate() {
                    let d = ((p.cx - rx).powi(2) + (p.cy - ry).powi(2)).sqrt();
                    let in_eye_zone = face.box_h <= 0.01 || p.cy <= (face.box_y + face.box_h * 0.55) as f64;
                    if in_eye_zone && d < r_min_d {
                        r_min_d = d;
                        r_pupil = Some(p_idx);
                    }
                }

                // Left eye
                let mut l_pupil = None;
                let mut l_min_d = search_radius;
                for (p_idx, p) in pupils.iter().enumerate() {
                    if Some(p_idx) == r_pupil { continue; }
                    let d = ((p.cx - lx).powi(2) + (p.cy - ly).powi(2)).sqrt();
                    let in_eye_zone = face.box_h <= 0.01 || p.cy <= (face.box_y + face.box_h * 0.55) as f64;
                    if in_eye_zone && d < l_min_d {
                        l_min_d = d;
                        l_pupil = Some(p_idx);
                    }
                }

                if let Some(p_idx) = r_pupil {
                    claimed_pupils[p_idx] = true;
                    let p = &pupils[p_idx];
                    let b_size = (p.size_norm * 1.5).max(default_box_size).clamp(0.04, 0.20);
                    final_boxes.push(RedEyeBox {
                        id: format!("auto_f{}_reye", f_idx),
                        x: (p.cx - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                        y: (p.cy - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                        width: b_size,
                        height: b_size,
                    });
                }

                if let Some(p_idx) = l_pupil {
                    claimed_pupils[p_idx] = true;
                    let p = &pupils[p_idx];
                    let b_size = (p.size_norm * 1.5).max(default_box_size).clamp(0.04, 0.20);
                    final_boxes.push(RedEyeBox {
                        id: format!("auto_f{}_leye", f_idx),
                        x: (p.cx - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                        y: (p.cy - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                        width: b_size,
                        height: b_size,
                    });
                }
            }
        }

        // 3. Eye-Pair Discovery for Macro / Cropped Shots (where face landmark didn't reach)
        let unclaimed_indices: Vec<usize> = (0..pupils.len()).filter(|&i| !claimed_pupils[i]).collect();
        for i in 0..unclaimed_indices.len() {
            let idx1 = unclaimed_indices[i];
            if claimed_pupils[idx1] { continue; }
            let p1 = &pupils[idx1];

            for j in (i + 1)..unclaimed_indices.len() {
                let idx2 = unclaimed_indices[j];
                if claimed_pupils[idx2] { continue; }
                let p2 = &pupils[idx2];

                let dx = (p1.cx - p2.cx).abs();
                let dy = (p1.cy - p2.cy).abs();
                let size_ratio = p1.size_norm / p2.size_norm.max(0.001);

                // Symmetrical eye pair geometry: roughly level, realistic eye spacing
                if dx >= 0.12 && dx <= 0.65 && dy <= 0.12 && size_ratio >= 0.65 && size_ratio <= 1.55 {
                    let in_mouth1 = faces.iter().any(|f| f.box_h > 0.01 && p1.cy > (f.box_y + f.box_h * 0.55) as f64 && p1.cx >= f.box_x as f64 && p1.cx <= (f.box_x + f.box_w) as f64);
                    let in_mouth2 = faces.iter().any(|f| f.box_h > 0.01 && p2.cy > (f.box_y + f.box_h * 0.55) as f64 && p2.cx >= f.box_x as f64 && p2.cx <= (f.box_x + f.box_w) as f64);
                    if in_mouth1 || in_mouth2 { continue; }

                    claimed_pupils[idx1] = true;
                    claimed_pupils[idx2] = true;

                    for (pid, p) in [(idx1, p1), (idx2, p2)] {
                        let b_size = (p.size_norm * 1.6).clamp(0.06, 0.18);
                        final_boxes.push(RedEyeBox {
                            id: format!("auto_pair_pupil_{}", pid),
                            x: (p.cx - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                            y: (p.cy - b_size * 0.5).clamp(0.0, 1.0 - b_size),
                            width: b_size,
                            height: b_size,
                        });
                    }
                    break;
                }
            }
        }

        // 4. Deduplicate / Merge overlapping boxes
        let mut merged_boxes: Vec<RedEyeBox> = Vec::new();
        for b in final_boxes {
            let mut merged = false;
            for mb in &mut merged_boxes {
                let d = ((mb.x + mb.width * 0.5) - (b.x + b.width * 0.5)).abs().max(
                    ((mb.y + mb.height * 0.5) - (b.y + b.height * 0.5)).abs()
                );
                if d < 0.08 {
                    let min_x = mb.x.min(b.x);
                    let min_y = mb.y.min(b.y);
                    let max_x = (mb.x + mb.width).max(b.x + b.width);
                    let max_y = (mb.y + mb.height).max(b.y + b.height);
                    mb.x = min_x;
                    mb.y = min_y;
                    mb.width = max_x - min_x;
                    mb.height = max_y - min_y;
                    merged = true;
                    break;
                }
            }
            if !merged {
                merged_boxes.push(b);
            }
        }

        Ok(merged_boxes)
    }

    pub fn apply_redeye(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        boxes: Vec<RedEyeBox>,
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

        manifest.steps.truncate(manifest.current_step + 1);

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

        let prev_render_name = manifest
            .steps
            .get(manifest.current_step)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_edit = if prev_path.exists() {
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

        let mut img = image::open(&src_to_edit).map_err(|e| format!("Failed to open image: {}", e))?;
        let boxes_json = json!({ "boxes": boxes });
        Self::apply_redeye_correction_to_image(&mut img, &boxes_json);

        let rgb = img.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                img.width(),
                img.height(),
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
            op: "redeye".to_string(),
            params: boxes_json,
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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

    /// Authentic Google Picasa SpotBlend / CRetouchFilter algorithm
    /// Seamlessly blends donor source patch into target blemish using
    /// Poisson gradient domain reconstruction with smooth cubic hermite boundary falloff.
    pub fn apply_spot_blend_algorithm(img: &mut image::DynamicImage, spots: &[RetouchSpot]) {
        if spots.is_empty() {
            return;
        }

        let mut rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        if width == 0 || height == 0 {
            return;
        }

        let w_f = width as f64;
        let h_f = height as f64;

        for spot in spots {
            let tx = spot.target_x * w_f;
            let ty = spot.target_y * h_f;
            let sx = spot.source_x * w_f;
            let sy = spot.source_y * h_f;
            let r = spot.radius * w_f.min(h_f);

            if r <= 0.5 {
                continue;
            }

            let r_sq = r * r;
            // Picasa boundary falloff zone: wide smooth margin falloff across outer 75% of circle
            let r_inner = 0.25 * r;
            let r_band = (r - r_inner).max(1e-4);

            // Snapshot current image state for this spot so donor sampling within the spot
            // handles any self-overlap cleanly, while subsequent spots will read from the newly updated rgb.
            let source_snapshot = rgb.clone();

            // Sample Dirichlet boundary discrepancy: boundary_diff(x, y) = target(x, y) - source(x, y)
            // We accumulate boundary samples around the circle circumference to build the harmonic membrane.
            let mut boundary_samples: Vec<(f64, f64, [f64; 3])> = Vec::new();
            let num_samples = ((2.0 * std::f64::consts::PI * r).ceil() as usize).clamp(32, 256);

            for i in 0..num_samples {
                let theta = (i as f64) * 2.0 * std::f64::consts::PI / (num_samples as f64);
                let cos_t = theta.cos();
                let sin_t = theta.sin();

                let b_tx = (tx + r * cos_t).round() as i32;
                let b_ty = (ty + r * sin_t).round() as i32;
                let b_sx = (sx + r * cos_t).round() as i32;
                let b_sy = (sy + r * sin_t).round() as i32;

                if b_tx >= 0 && b_tx < width as i32 && b_ty >= 0 && b_ty < height as i32
                    && b_sx >= 0 && b_sx < width as i32 && b_sy >= 0 && b_sy < height as i32
                {
                    let t_px = source_snapshot.get_pixel(b_tx as u32, b_ty as u32);
                    let s_px = source_snapshot.get_pixel(b_sx as u32, b_sy as u32);
                    let diff = [
                        (t_px[0] as f64) - (s_px[0] as f64),
                        (t_px[1] as f64) - (s_px[1] as f64),
                        (t_px[2] as f64) - (s_px[2] as f64),
                    ];
                    boundary_samples.push((cos_t, sin_t, diff));
                }
            }

            if boundary_samples.is_empty() {
                continue;
            }

            // Precompute average boundary difference and linear gradients for harmonic membrane interpolation
            let mut avg_diff = [0.0; 3];
            let mut grad_x = [0.0; 3];
            let mut grad_y = [0.0; 3];

            for (cos_t, sin_t, diff) in &boundary_samples {
                for c in 0..3 {
                    avg_diff[c] += diff[c];
                    grad_x[c] += diff[c] * cos_t;
                    grad_y[c] += diff[c] * sin_t;
                }
            }

            let n_s = boundary_samples.len() as f64;
            for c in 0..3 {
                avg_diff[c] /= n_s;
                grad_x[c] = (grad_x[c] / n_s) * (2.0 / r);
                grad_y[c] = (grad_y[c] / n_s) * (2.0 / r);
            }

            // Apply Poisson reconstruction with wide smootherstep boundary transition:
            // For each pixel (px, py) in target circle:
            // 1. Donor sample coordinate: (donor_x, donor_y) = (px - tx + sx, py - ty + sy)
            // 2. Harmonic membrane correction: corr = avg_diff + grad_x * (px - tx) + grad_y * (py - ty)
            // 3. Blended value: f = donor + corr
            // 4. Alpha transition across [r_inner, r]: quintic smootherstep (6t^5 - 15t^4 + 10t^3)
            //    final = (1 - alpha) * f + alpha * target
            for py in (ty - r).floor() as i32..=(ty + r).ceil() as i32 {
                if py < 0 || py >= height as i32 {
                    continue;
                }
                let dy = (py as f64) - ty;
                let donor_y = (sy + dy).round() as i32;
                if donor_y < 0 || donor_y >= height as i32 {
                    continue;
                }

                for px in (tx - r).floor() as i32..=(tx + r).ceil() as i32 {
                    if px < 0 || px >= width as i32 {
                        continue;
                    }
                    let dx = (px as f64) - tx;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq >= r_sq {
                        continue;
                    }

                    let donor_x = (sx + dx).round() as i32;
                    if donor_x < 0 || donor_x >= width as i32 {
                        continue;
                    }

                    let dist = dist_sq.sqrt();
                    let t_px = source_snapshot.get_pixel(px as u32, py as u32);
                    let s_px = source_snapshot.get_pixel(donor_x as u32, donor_y as u32);

                    // Compute smooth alpha feathering: inside r_inner alpha = 0.0, outside r alpha = 1.0
                    let alpha = if dist <= r_inner {
                        0.0
                    } else {
                        let t = ((dist - r_inner) / r_band).clamp(0.0, 1.0);
                        t * t * t * (t * (t * 6.0 - 15.0) + 10.0) // Quintic smootherstep
                    };

                    let mut out = [0u8; 3];
                    for c in 0..3 {
                        let membrane_correction = avg_diff[c] + grad_x[c] * dx + grad_y[c] * dy;
                        let healed_val = (s_px[c] as f64 + membrane_correction).clamp(0.0, 255.0);
                        let final_val = (1.0 - alpha) * healed_val + alpha * (t_px[c] as f64);
                        out[c] = final_val.round().clamp(0.0, 255.0) as u8;
                    }

                    rgb.put_pixel(px as u32, py as u32, image::Rgb(out));
                }
            }
        }

        *img = image::DynamicImage::ImageRgb8(rgb);
    }

    pub fn apply_retouch(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        spots: Vec<RetouchSpot>,
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

        manifest.steps.truncate(manifest.current_step + 1);

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

        let prev_render_name = manifest
            .steps
            .get(manifest.current_step)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_edit = if prev_path.exists() {
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

        let mut img = image::open(&src_to_edit).map_err(|e| format!("Failed to open image: {}", e))?;
        Self::apply_spot_blend_algorithm(&mut img, &spots);

        let rgb = img.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                img.width(),
                img.height(),
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
            op: "retouch".to_string(),
            params: json!({ "spots": spots }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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

    pub fn blend_overlay_onto_image(base: &mut image::DynamicImage, overlay: &image::DynamicImage) {
        let target_w = base.width();
        let target_h = base.height();
        let resized_overlay = if overlay.width() != target_w || overlay.height() != target_h {
            overlay.resize_exact(target_w, target_h, image::imageops::FilterType::Lanczos3)
        } else {
            overlay.clone()
        };
        let overlay_rgba = resized_overlay.to_rgba8();
        let mut base_rgba = base.to_rgba8();
        let w = target_w.min(overlay_rgba.width());
        let h = target_h.min(overlay_rgba.height());
        for y in 0..h {
            for x in 0..w {
                let op = overlay_rgba.get_pixel(x, y);
                let alpha = op[3] as f32 / 255.0;
                if alpha > 0.0 {
                    let bp = base_rgba.get_pixel_mut(x, y);
                    let inv = 1.0 - alpha;
                    bp[0] = (op[0] as f32 * alpha + bp[0] as f32 * inv).round() as u8;
                    bp[1] = (op[1] as f32 * alpha + bp[1] as f32 * inv).round() as u8;
                    bp[2] = (op[2] as f32 * alpha + bp[2] as f32 * inv).round() as u8;
                }
            }
        }
        *base = image::DynamicImage::ImageRgba8(base_rgba);
    }

    pub fn apply_text(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        items: Vec<TextItem>,
        overlay_base64: Option<String>,
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

        manifest.steps.truncate(manifest.current_step + 1);

        let mut orphan_id = manifest.current_step + 1;
        loop {
            let orphan_file = session_dir.join(format!("step_{}.jpg", orphan_id));
            let orphan_overlay = session_dir.join(format!("text_overlay_{}.png", orphan_id));
            let mut removed_any = false;
            if orphan_file.exists() {
                let _ = fs::remove_file(orphan_file);
                removed_any = true;
            }
            if orphan_overlay.exists() {
                let _ = fs::remove_file(orphan_overlay);
                removed_any = true;
            }
            if !removed_any {
                break;
            }
            orphan_id += 1;
        }

        let prev_render_name = manifest
            .steps
            .get(manifest.current_step)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_edit = if prev_path.exists() {
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
        let overlay_file_name = format!("text_overlay_{}.png", next_step_id);
        let overlay_path = session_dir.join(&overlay_file_name);

        let mut img = image::open(&src_to_edit).map_err(|e| format!("Failed to open image: {}", e))?;

        if let Some(ref b64) = overlay_base64 {
            let clean_b64 = if let Some(idx) = b64.find(',') {
                &b64[idx + 1..]
            } else {
                b64.as_str()
            };
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(clean_b64.trim()) {
                if let Ok(overlay_img) = image::load_from_memory(&bytes) {
                    let _ = fs::write(&overlay_path, &bytes);
                    Self::blend_overlay_onto_image(&mut img, &overlay_img);
                }
            }
        }

        let rgb = img.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                img.width(),
                img.height(),
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
            op: "text".to_string(),
            params: json!({
                "items": items,
                "overlay_file": overlay_file_name,
            }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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

    pub fn apply_feeling_lucky_algorithm(img: &mut image::DynamicImage) {
        // Picasa "I'm Feeling Lucky" (internally `autobacklight` at 0x8f7cc0 in Picasa3.exe):
        // A harmonious 4-stage pipeline that genuinely enhances real-world photos:
        // 1. Contrast expansion (0.8% percentile stretch)
        // 2. Backlight / Fill Light (lifts underexposed faces and shadows without blowing highlights)
        // 3. Gentle color cast balance (55% strength gray-world)
        // 4. Midtone vibrance / saturation boost (+12% saturation in midtones)
        let mut rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let total_pixels = (width as usize) * (height as usize);
        if total_pixels == 0 {
            return;
        }

        // --- Step 1: Luminance Contrast Expansion ---
        let mut hist_y = [0u32; 256];
        for pixel in rgb.pixels() {
            let y = ((pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8) as usize;
            hist_y[y] += 1;
        }

        let clip_low = ((total_pixels as f64) * 0.008).floor() as u32;
        let clip_high = ((total_pixels as f64) * 0.008).floor() as u32;

        let mut low_y = 0i32;
        let mut acc = 0u32;
        for (i, &count) in hist_y.iter().enumerate() {
            acc += count;
            if acc >= clip_low {
                low_y = i as i32;
                break;
            }
        }

        acc = 0;
        let mut high_y = 255i32;
        for (i, &count) in hist_y.iter().enumerate().rev() {
            acc += count;
            if acc >= clip_high {
                high_y = i as i32;
                break;
            }
        }
        if high_y <= low_y {
            high_y = (low_y + 1).min(255);
        }

        let range_y = (high_y - low_y).max(1);

        // --- Step 2 & 3: Fill Light Curve + Color Neutralization ---
        // Sample midtones to compute mild illuminant shift
        let mut sum_r = 0u64;
        let mut sum_g = 0u64;
        let mut sum_b = 0u64;
        let mut count_mid = 0u64;
        for pixel in rgb.pixels() {
            let lum = (pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8;
            if lum >= 30 && lum <= 225 {
                sum_r += pixel[0] as u64;
                sum_g += pixel[1] as u64;
                sum_b += pixel[2] as u64;
                count_mid += 1;
            }
        }

        let (gain_r, gain_g, gain_b) = if count_mid > 32 {
            let avg_r = sum_r as f64 / count_mid as f64;
            let avg_g = sum_g as f64 / count_mid as f64;
            let avg_b = sum_b as f64 / count_mid as f64;
            let avg_all = (avg_r + avg_g + avg_b) / 3.0;

            // 55% strength correction toward neutral
            let r_target = avg_r + (avg_all - avg_r) * 0.55;
            let g_target = avg_g + (avg_all - avg_g) * 0.55;
            let b_target = avg_b + (avg_all - avg_b) * 0.55;

            let gr = (r_target / avg_r.max(1.0)).clamp(0.85, 1.25);
            let gg = (g_target / avg_g.max(1.0)).clamp(0.85, 1.25);
            let gb = (b_target / avg_b.max(1.0)).clamp(0.85, 1.25);
            (gr, gg, gb)
        } else {
            (1.0, 1.0, 1.0)
        };

        // Precompute LUT for combined contrast stretch + backlight fill
        let mut lut_r = [0u8; 256];
        let mut lut_g = [0u8; 256];
        let mut lut_b = [0u8; 256];

        for i in 0..256 {
            // 1. Stretched normalized value [0.0, 1.0]
            let norm = ((i as f64 - low_y as f64) / range_y as f64).clamp(0.0, 1.0);

            // 2. Picasa Backlight / Fill Light shadow lifting formula:
            // Lifts dark areas (+15-20% in deep shadows) while tapering smoothly to 0 at black and white
            let fill = 0.22 * norm * (1.0 - norm) * (2.0 - norm);
            let lifted = (norm + fill).clamp(0.0, 1.0);

            // 3. Rescale to 0..255 with color balance gains
            lut_r[i] = ((lifted * 255.0 * gain_r).round() as i32).clamp(0, 255) as u8;
            lut_g[i] = ((lifted * 255.0 * gain_g).round() as i32).clamp(0, 255) as u8;
            lut_b[i] = ((lifted * 255.0 * gain_b).round() as i32).clamp(0, 255) as u8;
        }

        // Apply LUT + subtle midtone vibrance (+12% saturation)
        let raw_pixels = rgb.as_mut();
        raw_pixels.par_chunks_exact_mut(3).for_each(|pixel| {
            let r = lut_r[pixel[0] as usize] as f32;
            let g = lut_g[pixel[1] as usize] as f32;
            let b = lut_b[pixel[2] as usize] as f32;

            let y = 0.299 * r + 0.587 * g + 0.114 * b;
            // Vibrance: boost saturation by 12% in midtones
            let sat_boost = 1.12;
            let new_r = (y + (r - y) * sat_boost).round().clamp(0.0, 255.0) as u8;
            let new_g = (y + (g - y) * sat_boost).round().clamp(0.0, 255.0) as u8;
            let new_b = (y + (b - y) * sat_boost).round().clamp(0.0, 255.0) as u8;

            pixel[0] = new_r;
            pixel[1] = new_g;
            pixel[2] = new_b;
        });

        *img = image::DynamicImage::ImageRgb8(rgb);
    }

    pub fn apply_fill_light_algorithm(img: &mut image::DynamicImage, amount: f32) {
        let amount = amount.clamp(0.0, 1.0);
        if amount <= 1e-4 {
            return;
        }

        let mut rgb = img.to_rgb8();
        let mut lut = [0u8; 256];

        for i in 0..256 {
            let norm = i as f64 / 255.0;
            // Picasa Fill Light shadow lifting formula:
            // Lifts dark areas (+amount in deep/mid shadows) while smoothly tapering to 0 at black (0.0) and white (1.0).
            // Curve: fill = amount * norm * (1.0 - norm) * (2.0 - norm)
            let fill = (amount as f64) * norm * (1.0 - norm) * (2.0 - norm);
            let lifted = (norm + fill).clamp(0.0, 1.0);
            lut[i] = ((lifted * 255.0).round() as i32).clamp(0, 255) as u8;
        }

        let raw_pixels = rgb.as_mut();
        raw_pixels.par_chunks_exact_mut(3).for_each(|pixel| {
            pixel[0] = lut[pixel[0] as usize];
            pixel[1] = lut[pixel[1] as usize];
            pixel[2] = lut[pixel[2] as usize];
        });

        *img = image::DynamicImage::ImageRgb8(rgb);
    }

    pub fn apply_auto_contrast_algorithm(img: &mut image::DynamicImage) {
        // High-impact Auto Contrast:
        // Analyzes full-image luminance histogram with 0.8% percentile clipping at both black and white.
        // Applies a uniform linear expansion across all 3 color channels (strictly 0% color tint shift)
        // plus an adaptive contrast S-curve boost so even already-spread images get a distinct, pleasing punch.
        let mut rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let total_pixels = (width as usize) * (height as usize);
        if total_pixels == 0 {
            return;
        }

        let mut hist_y = [0u32; 256];
        for pixel in rgb.pixels() {
            let y = ((pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8) as usize;
            hist_y[y] += 1;
        }

        let clip_thresh = ((total_pixels as f64) * 0.008).floor().max(1.0) as u32;

        let mut low_y = 0i32;
        let mut acc = 0u32;
        for (i, &count) in hist_y.iter().enumerate() {
            acc += count;
            if acc >= clip_thresh {
                low_y = i as i32;
                break;
            }
        }

        acc = 0;
        let mut high_y = 255i32;
        for (i, &count) in hist_y.iter().enumerate().rev() {
            acc += count;
            if acc >= clip_thresh {
                high_y = i as i32;
                break;
            }
        }
        if high_y <= low_y {
            high_y = (low_y + 1).min(255);
        }

        let range = (high_y - low_y) as f64;

        // Build uniform 256-entry LUT: linear percentile stretch + gentle S-curve contrast boost
        let mut lut = [0u8; 256];
        for i in 0..256 {
            let stretched = ((i as f64 - low_y as f64) / range).clamp(0.0, 1.0);
            // S-curve contrast adjustment: f(x) = x + 0.15 * sin(2*PI * x) (steepens midtones)
            // or classic cubic S-curve: x' = 0.5 + 0.5 * sin(pi * (x - 0.5))
            // Here: 15% blend toward smoothstep
            let smooth = stretched * stretched * (3.0 - 2.0 * stretched);
            let boosted = stretched * 0.70 + smooth * 0.30;
            lut[i] = ((boosted * 255.0).round() as i32).clamp(0, 255) as u8;
        }

        let raw_pixels = rgb.as_mut();
        raw_pixels.par_chunks_exact_mut(3).for_each(|pixel| {
            pixel[0] = lut[pixel[0] as usize];
            pixel[1] = lut[pixel[1] as usize];
            pixel[2] = lut[pixel[2] as usize];
        });

        *img = image::DynamicImage::ImageRgb8(rgb);
    }

    pub fn apply_auto_color_algorithm(img: &mut image::DynamicImage) {
        // High-impact Auto Color:
        // Analyzes midtone illuminant cast using gray-world balance with 85% correction strength.
        // Effectively removes warm indoor tungsten, blue shadows, or greenish fluorescent tints.
        let mut rgb = img.to_rgb8();
        let (width, height) = rgb.dimensions();
        let total_pixels = (width as usize) * (height as usize);
        if total_pixels == 0 {
            return;
        }

        let mut sum_r = 0u64;
        let mut sum_g = 0u64;
        let mut sum_b = 0u64;
        let mut count = 0u64;

        for pixel in rgb.pixels() {
            let lum = (pixel[0] as u32 * 77 + pixel[1] as u32 * 150 + pixel[2] as u32 * 29) >> 8;
            // Restrict to midtones [25, 230] to ignore deep shadow clipping and specular light sources
            if lum >= 25 && lum <= 230 {
                sum_r += pixel[0] as u64;
                sum_g += pixel[1] as u64;
                sum_b += pixel[2] as u64;
                count += 1;
            }
        }

        if count < 16 {
            return;
        }

        let avg_r = sum_r as f64 / count as f64;
        let avg_g = sum_g as f64 / count as f64;
        let avg_b = sum_b as f64 / count as f64;
        let avg_gray = (avg_r + avg_g + avg_b) / 3.0;

        // Apply 85% correction strength toward gray balance
        let target_r = avg_r + (avg_gray - avg_r) * 0.85;
        let target_g = avg_g + (avg_gray - avg_g) * 0.85;
        let target_b = avg_b + (avg_gray - avg_b) * 0.85;

        let gain_r = (target_r / avg_r.max(1.0)).clamp(0.70, 1.45);
        let gain_g = (target_g / avg_g.max(1.0)).clamp(0.70, 1.45);
        let gain_b = (target_b / avg_b.max(1.0)).clamp(0.70, 1.45);

        // Normalize gains to preserve overall perceptual luminance
        let lum_weight = 0.299 * gain_r + 0.587 * gain_g + 0.114 * gain_b;
        let norm_r = gain_r / lum_weight;
        let norm_g = gain_g / lum_weight;
        let norm_b = gain_b / lum_weight;

        let mut lut_r = [0u8; 256];
        let mut lut_g = [0u8; 256];
        let mut lut_b = [0u8; 256];

        for i in 0..256 {
            lut_r[i] = ((i as f64 * norm_r).round() as i32).clamp(0, 255) as u8;
            lut_g[i] = ((i as f64 * norm_g).round() as i32).clamp(0, 255) as u8;
            lut_b[i] = ((i as f64 * norm_b).round() as i32).clamp(0, 255) as u8;
        }

        let raw_pixels = rgb.as_mut();
        raw_pixels.par_chunks_exact_mut(3).for_each(|pixel| {
            pixel[0] = lut_r[pixel[0] as usize];
            pixel[1] = lut_g[pixel[1] as usize];
            pixel[2] = lut_b[pixel[2] as usize];
        });

        *img = image::DynamicImage::ImageRgb8(rgb);
    }

    fn apply_one_click_filter<F>(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        op_name: &str,
        filter_type: &str,
        apply_fn: F,
    ) -> Result<EditResult, String>
    where
        F: FnOnce(&mut image::DynamicImage),
    {
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
        let src_to_edit = if prev_path.exists() {
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

        let mut img = image::open(&src_to_edit).map_err(|e| format!("Failed to open image: {}", e))?;
        apply_fn(&mut img);

        let rgb = img.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                img.width(),
                img.height(),
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
            op: op_name.to_string(),
            params: json!({ "type": filter_type }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;
        self.save_manifest(&manifest)?;
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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

    pub fn apply_feeling_lucky(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        self.apply_one_click_filter(
            db,
            thumbnailer,
            photo_id,
            "enhance",
            "lucky",
            Self::apply_feeling_lucky_algorithm,
        )
    }

    pub fn apply_auto_contrast(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        self.apply_one_click_filter(
            db,
            thumbnailer,
            photo_id,
            "autocontrast",
            "autocontrast",
            Self::apply_auto_contrast_algorithm,
        )
    }

    pub fn apply_auto_color(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        self.apply_one_click_filter(
            db,
            thumbnailer,
            photo_id,
            "autocolor",
            "autocolor",
            Self::apply_auto_color_algorithm,
        )
    }

    pub fn apply_fill_light(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        amount: f32,
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

        let amount = amount.clamp(0.0, 1.0);
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

        let is_current_fill = manifest
            .steps
            .get(manifest.current_step)
            .map(|s| s.op == "filllight" || s.op == "fill")
            .unwrap_or(false);

        // If setting to 0% and current step is already a filllight step, revert that step
        if amount <= 1e-4 && is_current_fill {
            if manifest.current_step > 0 {
                manifest.current_step -= 1;
                manifest.is_committed = false;
                self.save_manifest(&manifest)?;
                let _ = db.update_photo_unsaved_edits(photo_id, manifest.current_step > 0);

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
                return Ok(EditResult {
                    success: true,
                    photo: Some(photo),
                    session_state,
                    message: None,
                });
            }
        }

        // Determine source step image and target step id
        let (src_step_index, target_step_id) = if is_current_fill {
            // Overwrite current step, source is previous step
            (manifest.current_step.saturating_sub(1), manifest.current_step)
        } else {
            // Append new step: truncate forward redo steps
            manifest.steps.truncate(manifest.current_step + 1);
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
            (manifest.current_step, manifest.steps.len())
        };

        let prev_render_name = manifest
            .steps
            .get(src_step_index)
            .and_then(|s| s.render_file.clone())
            .unwrap_or_else(|| "step_0.jpg".to_string());
        let prev_path = session_dir.join(&prev_render_name);
        let src_to_edit = if prev_path.exists() {
            prev_path
        } else {
            self.ensure_base_proxy(&session_dir, &target_path, &manifest.base_hash, thumbnailer)?
        };

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let step_file_name = format!("step_{}.jpg", target_step_id);
        let dest_step_path = session_dir.join(&step_file_name);

        let mut img = image::open(&src_to_edit).map_err(|e| format!("Failed to open image: {}", e))?;
        Self::apply_fill_light_algorithm(&mut img, amount);

        let rgb = img.to_rgb8();
        let tmp_dest = dest_step_path.with_extension(format!("tmp.{}", std::process::id()));
        let file = File::create(&tmp_dest).map_err(|e| format!("Failed to create step file: {}", e))?;
        let mut writer = BufWriter::new(file);
        let mut encoder = JpegEncoder::new_with_quality(&mut writer, 90);
        encoder
            .encode(
                rgb.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgb8,
            )
            .map_err(|e| format!("JPEG encode error: {}", e))?;
        drop(writer);
        let _ = fs::rename(&tmp_dest, &dest_step_path);

        let prev_cum_rot = manifest
            .steps
            .get(src_step_index)
            .map(|s| s.cumulative_rotation)
            .unwrap_or(0);

        let new_step = EditStep {
            step_id: target_step_id,
            op: "filllight".to_string(),
            params: json!({ "amount": amount }),
            timestamp: now_sec,
            render_file: Some(step_file_name),
            cumulative_rotation: prev_cum_rot,
        };

        if is_current_fill {
            manifest.steps[target_step_id] = new_step;
        } else {
            manifest.steps.push(new_step);
            manifest.current_step = target_step_id;
        }

        manifest.is_committed = false;
        self.save_manifest(&manifest)?;
        let _ = db.update_photo_unsaved_edits(photo_id, true);

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
        let _ = db.update_photo_unsaved_edits(photo_id, manifest.current_step > 0);

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
        let _ = db.update_photo_unsaved_edits(photo_id, manifest.current_step > 0);

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
        let _ = db.update_photo_unsaved_edits(photo_id, manifest.current_step > 0);

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
        Self::render_pipeline_to_file(&base_source, &target_path, &manifest.steps, manifest.current_step, Some(&session_dir))?;

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

        // Reset edit rotation and unsaved edits in DB
        let _ = db.update_photo_edit_rotation(photo_id, 0);
        let _ = db.update_photo_unsaved_edits(photo_id, false);

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
                    Self::render_pipeline_to_file(&internal_pristine, &new_copy_path, &m.steps, m.current_step, Some(&session_dir))?;

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
        session_dir: Option<&Path>,
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
                "redeye" => {
                    Self::apply_redeye_correction_to_image(&mut img, &step.params);
                }
                "enhance" | "lucky" => {
                    Self::apply_feeling_lucky_algorithm(&mut img);
                }
                "autocontrast" => {
                    Self::apply_auto_contrast_algorithm(&mut img);
                }
                "autocolor" => {
                    Self::apply_auto_color_algorithm(&mut img);
                }
                "fill" | "filllight" => {
                    let amount = step.params.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
                    Self::apply_fill_light_algorithm(&mut img, amount);
                }
                "retouch" | "spotblend" => {
                    if let Some(spots_val) = step.params.get("spots") {
                        if let Ok(spots) = serde_json::from_value::<Vec<RetouchSpot>>(spots_val.clone()) {
                            Self::apply_spot_blend_algorithm(&mut img, &spots);
                        }
                    }
                }
                "text" => {
                    if let Some(s_dir) = session_dir {
                        if let Some(overlay_name) = step.params.get("overlay_file").and_then(|v| v.as_str()) {
                            let overlay_path = s_dir.join(overlay_name);
                            if overlay_path.exists() {
                                if let Ok(overlay_img) = image::open(&overlay_path) {
                                    Self::blend_overlay_onto_image(&mut img, &overlay_img);
                                }
                            }
                        }
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

    #[test]
    fn test_redeye_correction_and_session() {
        use crate::models::RedEyeBox;

        let temp_dir = tempdir().unwrap();
        let test_img_path = temp_dir.path().join("test_redeye.jpg");

        // Create 200x200 test image:
        // - Base skin tone: (210, 160, 140)
        // - Red pupil centered at (100, 100) with radius 8: (220, 30, 30)
        // - Catchlight at (100, 98): (250, 250, 250)
        let img = image::RgbImage::from_fn(200, 200, |x, y| {
            let dx = (x as i32) - 100;
            let dy = (y as i32) - 100;
            let dist_sq = dx * dx + dy * dy;
            if x == 100 && y == 98 {
                image::Rgb([250, 250, 250]) // Catchlight specular reflection
            } else if dist_sq <= 64 {
                image::Rgb([220, 30, 30]) // Bright flash red eye
            } else {
                image::Rgb([210, 160, 140]) // Normal skin
            }
        });

        // 1. Direct pixel-level correction test
        let mut test_dyn = image::DynamicImage::ImageRgb8(img.clone());
        let boxes = vec![RedEyeBox {
            id: "test_eye_1".to_string(),
            x: 0.40, // 80px to 120px
            y: 0.40,
            width: 0.20,
            height: 0.20,
        }];

        let params = serde_json::json!({ "boxes": boxes });
        EditorManager::apply_redeye_correction_to_image(&mut test_dyn, &params);
        let corrected_rgb = test_dyn.to_rgb8();

        // Check red pupil: before was (220, 30, 30), should now be darkened/desaturated
        let pupil_pixel = corrected_rgb.get_pixel(100, 102);
        assert!(
            pupil_pixel[0] < 80,
            "Red channel should be darkened, was {}",
            pupil_pixel[0]
        );
        assert!(
            (pupil_pixel[0] as i32 - pupil_pixel[1] as i32).abs() < 25,
            "Pupil should be neutral/desaturated, was {:?}",
            pupil_pixel
        );

        // Check catchlight reflection: should be preserved as bright white (>200)
        let catchlight_pixel = corrected_rgb.get_pixel(100, 98);
        assert!(
            catchlight_pixel[0] >= 240 && catchlight_pixel[1] >= 240 && catchlight_pixel[2] >= 240,
            "Catchlight should be preserved untouched, was {:?}",
            catchlight_pixel
        );

        // Check surrounding skin tone: should be untouched
        let skin_pixel = corrected_rgb.get_pixel(50, 50);
        assert_eq!(skin_pixel[0], 210);
        assert_eq!(skin_pixel[1], 160);
        assert_eq!(skin_pixel[2], 140);

        // 2. Editor session pipeline test
        img.save(&test_img_path).unwrap();
        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(temp_dir.path().to_str().unwrap(), "Test", "always").unwrap();
        let initial_hash = Scanner::compute_file_hash(&test_img_path, 1000, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash,
            file_name: "test_redeye.jpg".to_string(),
            file_size: 1000,
            width: Some(200),
            height: Some(200),
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

        let res = editor
            .apply_redeye(&db, &thumbnailer, photo.id, boxes)
            .unwrap();
        assert!(res.success);
        assert_eq!(res.session_state.current_step, 1);
        assert_eq!(res.session_state.steps.len(), 2);
        assert_eq!(res.session_state.steps[1].op, "redeye");

        // 3. Save session
        let saved_session = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(saved_session.is_committed);

        let saved_img = image::open(&test_img_path).unwrap().to_rgb8();
        let saved_pupil = saved_img.get_pixel(100, 102);
        assert!(
            saved_pupil[0] < 80,
            "Saved file should contain red eye repair, was {:?}",
            saved_pupil
        );
    }

    #[test]
    fn test_feeling_lucky_enhancement() {
        use image::{Rgb, RgbImage};

        // Create a test image with low contrast and slight blue cast
        let mut test_img = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                // Dull, dark, bluish pixel
                let val = (40 + (x + y) / 4) as u8;
                test_img.put_pixel(x, y, Rgb([val, val, val + 15]));
            }
        }

        let mut dyn_img = image::DynamicImage::ImageRgb8(test_img);
        EditorManager::apply_feeling_lucky_algorithm(&mut dyn_img);

        let rgb = dyn_img.to_rgb8();
        let (w, h) = rgb.dimensions();
        assert_eq!(w, 100);
        assert_eq!(h, 100);

        // Check that dynamic range and brightness were enhanced
        let mut max_lum = 0u8;
        let mut min_lum = 255u8;
        for p in rgb.pixels() {
            let lum = (0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64) as u8;
            max_lum = max_lum.max(lum);
            min_lum = min_lum.min(lum);
        }

        assert!(max_lum > 80, "Expected max luminance to expand, got {}", max_lum);
        assert!(min_lum < 40, "Expected shadow definition, got {}", min_lum);
    }

    #[test]
    fn test_auto_contrast_enhancement() {
        use image::{Rgb, RgbImage};

        // Create a flat low-contrast image (values between 80 and 160)
        let mut test_img = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let val = (80 + (x * 80) / 100) as u8;
                test_img.put_pixel(x, y, Rgb([val, val, val]));
            }
        }

        let mut dyn_img = image::DynamicImage::ImageRgb8(test_img);
        EditorManager::apply_auto_contrast_algorithm(&mut dyn_img);

        let rgb = dyn_img.to_rgb8();
        let mut min_val = 255u8;
        let mut max_val = 0u8;
        for p in rgb.pixels() {
            min_val = min_val.min(p[0]);
            max_val = max_val.max(p[0]);
            // Neutral image remains strictly neutral
            assert_eq!(p[0], p[1]);
            assert_eq!(p[1], p[2]);
        }

        // Contrast should be significantly stretched toward 0 and 255
        assert!(min_val < 30, "Expected low end to stretch toward 0, got {}", min_val);
        assert!(max_val > 225, "Expected high end to stretch toward 255, got {}", max_val);
    }

    #[test]
    fn test_auto_color_enhancement() {
        use image::{Rgb, RgbImage};

        // Create an image with an artificial warm/yellow cast (R elevated, B suppressed)
        let mut test_img = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                let g = 128u8;
                let r = 150u8; // +22 warm cast
                let b = 106u8; // -22 cool suppression
                test_img.put_pixel(x, y, Rgb([r, g, b]));
            }
        }

        let mut dyn_img = image::DynamicImage::ImageRgb8(test_img);
        EditorManager::apply_auto_color_algorithm(&mut dyn_img);

        let rgb = dyn_img.to_rgb8();
        let sample = rgb.get_pixel(50, 50);
        // Cast should be noticeably neutralized: difference between R and B should shrink significantly
        let orig_diff = (150i32 - 106i32).abs();
        let new_diff = (sample[0] as i32 - sample[2] as i32).abs();
        assert!(
            new_diff < orig_diff,
            "Auto Color should reduce channel difference: original diff {}, new diff {}",
            orig_diff,
            new_diff
        );
    }

    #[test]
    fn test_retouch_spot_blend_algorithm() {
        use image::{Rgb, RgbImage};
        use crate::models::RetouchSpot;

        // Create a 100x100 uniform background (light gray [200, 200, 200])
        // with a dark blemish at (30, 30) of radius 5 (values [20, 20, 20])
        let mut test_img = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                test_img.put_pixel(x, y, Rgb([200, 200, 200]));
            }
        }

        // Draw dark blemish
        for y in 26..=34 {
            for x in 26..=34 {
                let dx = (x as f64) - 30.0;
                let dy = (y as f64) - 30.0;
                if dx * dx + dy * dy <= 16.0 {
                    test_img.put_pixel(x, y, Rgb([20, 20, 20]));
                }
            }
        }

        let mut dyn_img = image::DynamicImage::ImageRgb8(test_img);

        // Apply SpotBlend retouch healing from clean source patch at (70, 70)
        let spot = RetouchSpot {
            id: "test-spot-1".to_string(),
            target_x: 0.30,
            target_y: 0.30,
            radius: 0.08, // 8px radius
            source_x: 0.70,
            source_y: 0.70,
        };

        EditorManager::apply_spot_blend_algorithm(&mut dyn_img, &[spot]);

        let rgb = dyn_img.to_rgb8();
        let healed_center = rgb.get_pixel(30, 30);

        // The blemish should be healed: center should be close to 200, not 20
        assert!(
            healed_center[0] > 180,
            "Blemish center was not healed properly: expected >180, got {:?}",
            healed_center
        );
        assert!(
            healed_center[1] > 180,
            "Blemish center was not healed properly: expected >180, got {:?}",
            healed_center
        );
        assert!(
            healed_center[2] > 180,
            "Blemish center was not healed properly: expected >180, got {:?}",
            healed_center
        );

        // Test sequential spots: Add a 2nd blemish at (50, 50), and use the newly HEALED (30, 30) as donor source!
        let mut img2 = dyn_img.to_rgb8();
        for dy in -3..=3 {
            for dx in -3..=3 {
                if dx * dx + dy * dy <= 9 {
                    img2.put_pixel((50 + dx) as u32, (50 + dy) as u32, Rgb([10, 10, 10]));
                }
            }
        }
        let mut dyn_img2 = image::DynamicImage::ImageRgb8(img2);

        let spot2 = RetouchSpot {
            id: "test-spot-2".to_string(),
            target_x: 0.50,
            target_y: 0.50,
            radius: 0.08,
            source_x: 0.30, // Using (30, 30) which was previously blemish, but is now healed!
            source_y: 0.30,
        };

        EditorManager::apply_spot_blend_algorithm(&mut dyn_img2, &[spot2]);
        let rgb2 = dyn_img2.to_rgb8();
        let spot2_center = rgb2.get_pixel(50, 50);

        // Because spot 2 sampled from the previously healed spot 1, it should receive clean bright pixels (> 170)
        assert!(
            spot2_center[0] > 170,
            "Spot 2 did not sample from the healed spot 1: expected >170, got {:?}",
            spot2_center
        );
    }

    #[test]
    fn test_editor_text_and_undo_redo() {
        use base64::engine::general_purpose::STANDARD as BASE64;
        use base64::Engine;

        let temp_dir = tempdir().unwrap();
        let photos_dir = temp_dir.path().join("Photos");
        fs::create_dir_all(&photos_dir).unwrap();

        // 100x100 white image
        let test_img_path = photos_dir.join("text_test.png");
        let mut img = image::RgbImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgb([255, 255, 255]);
        }
        img.save(&test_img_path).unwrap();

        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(photos_dir.to_str().unwrap(), "Photos", "always").unwrap();

        let initial_hash = Scanner::compute_file_hash(&test_img_path, 1000, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash.clone(),
            file_name: "text_test.png".to_string(),
            file_size: 1000,
            width: Some(100),
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

        // Create overlay with red box in center
        let mut overlay = image::RgbaImage::new(100, 100);
        for y in 40..60 {
            for x in 40..60 {
                overlay.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let mut overlay_bytes = Vec::new();
        overlay.write_to(&mut std::io::Cursor::new(&mut overlay_bytes), image::ImageFormat::Png).unwrap();
        let overlay_b64 = format!("data:image/png;base64,{}", BASE64.encode(&overlay_bytes));

        let text_item = TextItem {
            id: "t1".to_string(),
            text: "Hello Picasa".to_string(),
            x: 0.5,
            y: 0.5,
            angle: 0.0,
            scale: 1.0,
            font_family: "Arial".to_string(),
            font_size: 20.0,
            bold: false,
            italic: false,
            underline: false,
            alignment: "center".to_string(),
            color: "#ff0000".to_string(),
            has_fill: true,
            has_outline: false,
            outline_color: "#000000".to_string(),
            outline_width: 0.0,
            transparency: 0.0,
        };

        // 1. Apply text
        let res = editor.apply_text(&db, &thumbnailer, photo.id, vec![text_item], Some(overlay_b64)).unwrap();
        assert!(res.success);
        let s = res.session_state;
        assert_eq!(s.current_step, 1);
        assert!(s.can_undo);
        assert!(!s.can_redo);

        // Check proxy files
        let session_dir = editor.get_session_dir(&photo.file_hash);
        assert!(session_dir.join("step_1.jpg").exists());
        assert!(session_dir.join("text_overlay_1.png").exists());

        // 2. Undo text
        let res_undo = editor.undo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_undo.success);
        assert_eq!(res_undo.session_state.current_step, 0);
        assert!(!res_undo.session_state.can_undo);
        assert!(res_undo.session_state.can_redo);

        // 3. Redo text
        let res_redo = editor.redo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_redo.success);
        assert_eq!(res_redo.session_state.current_step, 1);
        assert!(res_redo.session_state.can_undo);
        assert!(!res_redo.session_state.can_redo);

        // 4. Save (commit)
        let saved = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(saved.is_committed);

        // Verify committed file on disk has red center
        let disk_img = image::open(&test_img_path).unwrap().to_rgb8();
        let center_px = disk_img.get_pixel(50, 50);
        assert_eq!(center_px[0], 255);
        assert!(center_px[1] < 50);
        assert!(center_px[2] < 50);
    }

    #[test]
    fn test_editor_fill_light_algorithm() {
        // Create 4x1 image: black, shadow (30), midtone (100), white (255)
        let mut img = image::RgbImage::new(4, 1);
        img.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        img.put_pixel(1, 0, image::Rgb([30, 30, 30]));
        img.put_pixel(2, 0, image::Rgb([100, 100, 100]));
        img.put_pixel(3, 0, image::Rgb([255, 255, 255]));

        let mut dyn_img = image::DynamicImage::ImageRgb8(img.clone());

        // Amount 0.0 should leave image untouched
        EditorManager::apply_fill_light_algorithm(&mut dyn_img, 0.0);
        assert_eq!(dyn_img.to_rgb8().get_pixel(0, 0), &image::Rgb([0, 0, 0]));
        assert_eq!(dyn_img.to_rgb8().get_pixel(1, 0), &image::Rgb([30, 30, 30]));
        assert_eq!(dyn_img.to_rgb8().get_pixel(2, 0), &image::Rgb([100, 100, 100]));
        assert_eq!(dyn_img.to_rgb8().get_pixel(3, 0), &image::Rgb([255, 255, 255]));

        // Amount 0.5 should lift shadows and midtones while preserving black and white
        EditorManager::apply_fill_light_algorithm(&mut dyn_img, 0.5);
        let rgb = dyn_img.to_rgb8();
        assert_eq!(rgb.get_pixel(0, 0), &image::Rgb([0, 0, 0]), "Black point must stay 0");
        assert_eq!(rgb.get_pixel(3, 0), &image::Rgb([255, 255, 255]), "White point must stay 255");
        assert!(rgb.get_pixel(1, 0)[0] > 30, "Shadows must be lifted");
        assert!(rgb.get_pixel(2, 0)[0] > 100, "Midtones must be lifted");
        assert!(rgb.get_pixel(1, 0)[0] < rgb.get_pixel(2, 0)[0], "Monotonicity must be preserved");
    }

    #[test]
    fn test_editor_fill_light_session_and_commit() {
        let temp_dir = tempdir().unwrap();
        let photos_dir = temp_dir.path().join("Photos");
        fs::create_dir_all(&photos_dir).unwrap();

        // 100x100 dark shadow image (all pixels at 40, 40, 40)
        let test_img_path = photos_dir.join("fill_test.png");
        let mut img = image::RgbImage::new(100, 100);
        for pixel in img.pixels_mut() {
            *pixel = image::Rgb([40, 40, 40]);
        }
        img.save(&test_img_path).unwrap();

        let db = DbManager::new_in_memory().unwrap();
        let folder_id = db.add_or_update_folder(photos_dir.to_str().unwrap(), "Photos", "always").unwrap();

        let initial_hash = Scanner::compute_file_hash(&test_img_path, 1000, 1000);
        db.batch_upsert_photos(&[PhotoInsert {
            folder_id,
            file_path: test_img_path.to_str().unwrap().to_string(),
            file_hash: initial_hash.clone(),
            file_name: "fill_test.png".to_string(),
            file_size: 1000,
            width: Some(100),
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

        // 1. Apply fill light 30%
        let res = editor.apply_fill_light(&db, &thumbnailer, photo.id, 0.3).unwrap();
        assert!(res.success);
        assert_eq!(res.session_state.current_step, 1);
        assert!(res.session_state.can_undo);

        // 2. Adjust fill light slider to 50% (should update step 1 in-place rather than push step 2)
        let res2 = editor.apply_fill_light(&db, &thumbnailer, photo.id, 0.5).unwrap();
        assert!(res2.success);
        assert_eq!(res2.session_state.current_step, 1);
        assert_eq!(res2.session_state.steps.len(), 2); // step 0 and step 1

        // 3. Undo fill light
        let res_undo = editor.undo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_undo.success);
        assert_eq!(res_undo.session_state.current_step, 0);

        // 4. Redo fill light
        let res_redo = editor.redo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_redo.success);
        assert_eq!(res_redo.session_state.current_step, 1);

        // 5. Commit to disk
        let saved = editor.save_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(saved.is_committed);

        // Verify disk file pixels were lifted significantly above 40
        let disk_img = image::open(&test_img_path).unwrap().to_rgb8();
        let px = disk_img.get_pixel(50, 50);
        assert!(px[0] > 60, "Committed file should have lifted shadows: expected >60, got {}", px[0]);
    }
}



