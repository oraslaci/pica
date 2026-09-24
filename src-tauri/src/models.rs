use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub scan_mode: String, // "always", "once", "exclude"
    pub last_scanned: Option<i64>,
    pub photo_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: i64,
    pub folder_id: i64,
    pub file_path: String,
    pub file_hash: String,
    pub file_name: String,
    pub file_size: i64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub date_taken: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub orientation: Option<u32>,
    pub is_favorite: bool,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderWithPhotos {
    pub folder: Folder,
    pub photos: Vec<Photo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current_folder: String,
    pub scanned_photos: usize,
    pub total_found: usize,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryStats {
    pub total_photos: i64,
    pub total_folders: i64,
    pub total_favorites: i64,
    pub is_scanning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub original_handling: String, // "backup" or "overwrite"
    pub hide_originals: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            original_handling: "backup".to_string(),
            hide_originals: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditStep {
    pub step_id: usize,
    pub op: String, // "original", "rotate"
    pub params: serde_json::Value,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSessionManifest {
    pub photo_id: i64,
    pub original_path: String,
    pub backup_path: Option<String>,
    pub base_hash: String,
    pub base_mtime: u64,
    pub base_size: u64,
    pub last_saved_mtime: u64,
    pub last_saved_size: u64,
    pub current_step: usize,
    pub steps: Vec<EditStep>,
    pub is_committed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditSessionState {
    pub photo_id: i64,
    pub file_path: String,
    pub has_session: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub current_step: usize,
    pub total_steps: usize,
    pub cumulative_rotation: i32,
    pub has_backup: bool,
    pub backup_path: Option<String>,
    pub has_conflict: bool,
    pub is_committed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub success: bool,
    pub photo: Option<Photo>,
    pub session_state: EditSessionState,
    pub message: Option<String>,
}

