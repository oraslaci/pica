use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use image::codecs::jpeg::JpegEncoder;
use image::image_dimensions;
use serde_json::json;

use crate::db::DbManager;
use crate::models::{
    AppSettings, EditResult, EditSessionManifest, EditSessionState, EditStep, Photo,
};
use crate::scanner::Scanner;
use crate::thumbnailer::{ThumbnailManager, DEFAULT_THUMBNAIL_SIZE};

pub struct EditorManager {
    pub edits_dir: PathBuf,
}

impl EditorManager {
    pub fn new() -> Self {
        let edits_dir = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("pica")
            .join("edits");
        let _ = fs::create_dir_all(&edits_dir);
        Self { edits_dir }
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

    pub fn save_manifest(&self, manifest: &EditSessionManifest) -> Result<(), String> {
        let session_dir = self.get_session_dir(&manifest.base_hash);
        fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;
        let path = session_dir.join("session.json");
        let content = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())?;
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
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let backup_path = Self::get_backup_path(&target_path);
        let has_backup = backup_path.exists();

        // Check if there is a session for this photo
        let manifest = self.find_session_for_photo(&photo);

        if let Some(m) = manifest {
            let has_conflict = Self::check_external_conflict(&m, &target_path);
            let can_undo = m.current_step > 0;
            let can_redo = m.current_step < m.steps.len() - 1;
            let cumulative_rotation = Self::calculate_cumulative_rotation(&m.steps, m.current_step);

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
                has_conflict,
                is_committed: m.is_committed,
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
                has_conflict: false,
                is_committed: false,
            })
        }
    }

    fn calculate_cumulative_rotation(steps: &[EditStep], current_step: usize) -> i32 {
        let mut rot = 0;
        for step in steps.iter().take(current_step + 1) {
            if step.op == "rotate" {
                if let Some(deg) = step.params.get("degrees").and_then(|v| v.as_i64()) {
                    rot += deg as i32;
                }
            }
        }
        let normalized = ((rot % 360) + 360) % 360;
        normalized
    }

    pub fn apply_rotation(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        degrees: i32,
    ) -> Result<EditResult, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        if !target_path.exists() {
            return Err("Target file does not exist on disk".to_string());
        }

        let settings = db.get_app_settings().unwrap_or_default();

        // 1. Get or create session manifest
        let mut manifest = match self.find_session_for_photo(&photo) {
            Some(m) => m,
            None => {
                let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
                let mtime = meta
                    .modified()
                    .unwrap_or(SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                let session_dir = self.get_session_dir(&photo.file_hash);
                fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

                let backup_path = self.ensure_backup(&target_path, &settings, &session_dir)?;

                let initial_step = EditStep {
                    step_id: 0,
                    op: "original".to_string(),
                    params: json!({}),
                    timestamp: mtime as i64,
                };

                EditSessionManifest {
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
                }
            }
        };

        // Check external conflict
        let has_conflict = Self::check_external_conflict(&manifest, &target_path);
        if has_conflict {
            let session_state = self.get_session_state(db, photo_id)?;
            return Ok(EditResult {
                success: false,
                photo: Some(photo),
                session_state,
                message: Some("External changes detected on image file".to_string()),
            });
        }

        // 2. Append new edit step (truncate any redo steps beyond current_step)
        manifest.steps.truncate(manifest.current_step + 1);

        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let next_step_id = manifest.steps.len();
        manifest.steps.push(EditStep {
            step_id: next_step_id,
            op: "rotate".to_string(),
            params: json!({ "degrees": degrees }),
            timestamp: now_sec,
        });
        manifest.current_step = next_step_id;
        manifest.is_committed = false;

        // 3. Render image to disk based on cumulative operations from pristine base
        let updated_photo = self.render_and_update(
            db,
            thumbnailer,
            &photo,
            &mut manifest,
        )?;

        let session_state = self.get_session_state(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(updated_photo),
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

        let updated_photo = self.render_and_update(
            db,
            thumbnailer,
            &photo,
            &mut manifest,
        )?;

        let session_state = self.get_session_state(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(updated_photo),
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
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        if manifest.current_step >= manifest.steps.len() - 1 {
            return Err("Nothing to redo".to_string());
        }

        manifest.current_step += 1;
        manifest.is_committed = false;

        let updated_photo = self.render_and_update(
            db,
            thumbnailer,
            &photo,
            &mut manifest,
        )?;

        let session_state = self.get_session_state(db, photo_id)?;

        Ok(EditResult {
            success: true,
            photo: Some(updated_photo),
            session_state,
            message: None,
        })
    }

    pub fn save_edit_session(
        &self,
        db: &DbManager,
        photo_id: i64,
    ) -> Result<EditSessionState, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let mut manifest = self
            .find_session_for_photo(&photo)
            .ok_or_else(|| "No active edit session found".to_string())?;

        let settings = db.get_app_settings().unwrap_or_default();
        if settings.original_handling == "overwrite" {
            // Delete backup copy if policy is overwrite
            let target_path = PathBuf::from(&photo.file_path);
            let backup_path = Self::get_backup_path(&target_path);
            if backup_path.exists() {
                let _ = fs::remove_file(backup_path);
            }
        }

        manifest.is_committed = true;
        self.save_manifest(&manifest)?;

        self.get_session_state(db, photo_id)
    }

    pub fn revert_edit_session(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
    ) -> Result<EditResult, String> {
        let photo = db
            .get_photo_by_id(photo_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Photo not found".to_string())?;

        let target_path = PathBuf::from(&photo.file_path);
        let backup_path = Self::get_backup_path(&target_path);
        let session_dir = self
            .find_session_for_photo(&photo)
            .map(|m| self.get_session_dir(&m.base_hash))
            .unwrap_or_else(|| self.get_session_dir(&photo.file_hash));
        let internal_pristine = Self::get_pristine_path(&session_dir, &target_path);

        let source_restore = if backup_path.exists() {
            Some(backup_path.clone())
        } else if internal_pristine.exists() {
            Some(internal_pristine)
        } else {
            None
        };

        if let Some(src) = source_restore {
            fs::copy(&src, &target_path).map_err(|e| e.to_string())?;

            // Remove backup if reverting
            if backup_path.exists() {
                let _ = fs::remove_file(&backup_path);
            }
            // Remove session
            let _ = fs::remove_dir_all(&session_dir);

            // Update photo in db
            let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
            let mtime = meta
                .modified()
                .unwrap_or(SystemTime::now())
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let new_hash = Scanner::compute_file_hash(&target_path, meta.len(), mtime);
            let dims = image_dimensions(&target_path).ok();

            // Refresh thumbnail
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
            let session_state = self.get_session_state(db, photo_id)?;

            Ok(EditResult {
                success: true,
                photo: updated_photo,
                session_state,
                message: Some("Reverted to original photo".to_string()),
            })
        } else {
            Err("No backup or pristine copy found to revert to".to_string())
        }
    }

    pub fn resolve_conflict(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        photo_id: i64,
        choice: &str, // "keep_external", "apply_pica", "save_as_copy"
    ) -> Result<EditResult, String> {
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
                let session_state = self.get_session_state(db, photo_id)?;
                Ok(EditResult {
                    success: true,
                    photo: updated_photo,
                    session_state,
                    message: Some("Adopted external version".to_string()),
                })
            }
            "apply_pica" => {
                // Overwrite external file with Pica's cumulative state
                if let Some(mut m) = self.load_manifest(&photo.file_hash) {
                    let updated_photo = self.render_and_update(db, thumbnailer, &photo, &mut m)?;
                    let session_state = self.get_session_state(db, photo_id)?;
                    Ok(EditResult {
                        success: true,
                        photo: Some(updated_photo),
                        session_state,
                        message: Some("Applied Pica draft over external changes".to_string()),
                    })
                } else {
                    Err("No manifest found".to_string())
                }
            }
            "save_as_copy" => {
                // Save Pica draft to a new file, leave external untouched
                if let Some(m) = self.load_manifest(&photo.file_hash) {
                    let stem = target_path.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = target_path.extension().unwrap_or_default().to_string_lossy();
                    let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
                    let new_file_name = format!("{}_pica_draft.{}", stem, ext);
                    let new_copy_path = parent.join(new_file_name);

                    // Render to new copy path
                    let internal_pristine = Self::get_pristine_path(&session_dir, &target_path);
                    let cumulative_deg = Self::calculate_cumulative_rotation(&m.steps, m.current_step);
                    Self::render_rotation_to_file(&internal_pristine, &new_copy_path, cumulative_deg)?;

                    let _ = fs::remove_dir_all(&session_dir);
                    let session_state = self.get_session_state(db, photo_id)?;
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

    fn render_and_update(
        &self,
        db: &DbManager,
        thumbnailer: &Arc<ThumbnailManager>,
        current_photo: &Photo,
        manifest: &mut EditSessionManifest,
    ) -> Result<Photo, String> {
        let target_path = PathBuf::from(&manifest.original_path);
        let session_dir = self.get_session_dir(&manifest.base_hash);
        let internal_pristine = Self::get_pristine_path(&session_dir, &target_path);

        let base_source = if internal_pristine.exists() {
            internal_pristine
        } else if let Some(ref bp) = manifest.backup_path {
            PathBuf::from(bp)
        } else {
            target_path.clone()
        };

        let cumulative_degrees = Self::calculate_cumulative_rotation(&manifest.steps, manifest.current_step);

        // Render to target file
        Self::render_rotation_to_file(&base_source, &target_path, cumulative_degrees)?;

        // Update file metadata & hash
        let meta = fs::metadata(&target_path).map_err(|e| e.to_string())?;
        let mtime = meta
            .modified()
            .unwrap_or(SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        manifest.last_saved_mtime = mtime;
        manifest.last_saved_size = meta.len();
        self.save_manifest(manifest)?;

        let new_hash = Scanner::compute_file_hash(&target_path, meta.len(), mtime);
        let dims = image_dimensions(&target_path).ok();

        // Regenerate thumbnail for new hash
        let old_thumb = thumbnailer.get_thumbnail_path(&current_photo.file_hash);
        let _ = fs::remove_file(old_thumb);
        let _ = thumbnailer.generate_thumbnail(&target_path, &new_hash, DEFAULT_THUMBNAIL_SIZE);

        db.update_photo_after_edit(
            current_photo.id,
            &new_hash,
            meta.len() as i64,
            dims.map(|d| d.0),
            dims.map(|d| d.1),
            Some(1),
        )
        .map_err(|e| e.to_string())?;

        db.get_photo_by_id(current_photo.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to reload photo after edit".to_string())
    }

    fn render_rotation_to_file(
        source_path: &Path,
        dest_path: &Path,
        degrees: i32,
    ) -> Result<(), String> {
        let normalized = ((degrees % 360) + 360) % 360;

        if normalized == 0 {
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
        let img = image::open(source_path)
            .map_err(|e| format!("Failed to open image {}: {}", source_path.display(), e))?;

        let rotated = match normalized {
            90 => img.rotate90(),
            180 => img.rotate180(),
            270 => img.rotate270(),
            _ => img,
        };

        let stem = dest_path.file_stem().unwrap_or_default().to_string_lossy();
        let ext_str = dest_path.extension().unwrap_or_default().to_string_lossy();
        let temp_dest = dest_path.with_file_name(format!("{}.tmp.{}.{}", stem, std::process::id(), ext_str));

        if is_jpeg {
            let rgb = rotated.to_rgb8();
            let file = File::create(&temp_dest)
                .map_err(|e| format!("Failed to create output file: {}", e))?;
            let mut writer = BufWriter::new(file);
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, 98);
            encoder
                .encode(
                    rgb.as_raw(),
                    rotated.width(),
                    rotated.height(),
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
            rotated
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
        let editor = EditorManager { edits_dir };

        // 1. Apply 90° rotation
        let res1 = editor.apply_rotation(&db, &thumbnailer, photo.id, 90).unwrap();
        assert!(res1.success);
        let s1 = res1.session_state;
        assert_eq!(s1.cumulative_rotation, 90);
        assert!(s1.can_undo);
        assert!(!s1.can_redo);

        // Check image dimensions on disk: was 100x50, should now be 50x100!
        let (w, h) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w, h), (50, 100));

        // Check backup was created in .pica_originals
        let backup_path = EditorManager::get_backup_path(&test_img_path);
        assert!(backup_path.exists());

        // 2. Undo
        let res_undo = editor.undo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_undo.success);
        let s_undo = res_undo.session_state;
        assert_eq!(s_undo.cumulative_rotation, 0);
        assert!(!s_undo.can_undo);
        assert!(s_undo.can_redo);

        // On disk: should be restored to 100x50
        let (w_orig, h_orig) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w_orig, h_orig), (100, 50));

        // 3. Redo
        let res_redo = editor.redo_edit(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_redo.success);
        let s_redo = res_redo.session_state;
        assert_eq!(s_redo.cumulative_rotation, 90);

        // 4. Save (commit)
        let s_saved = editor.save_edit_session(&db, photo.id).unwrap();
        assert!(s_saved.is_committed);

        // 5. Revert
        let res_revert = editor.revert_edit_session(&db, &thumbnailer, photo.id).unwrap();
        assert!(res_revert.success);
        let (w_rev, h_rev) = image_dimensions(&test_img_path).unwrap();
        assert_eq!((w_rev, h_rev), (100, 50));
    }
}

