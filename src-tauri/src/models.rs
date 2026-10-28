use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub display_name: String,
    pub scan_mode: String, // "always", "once", "exclude"
    pub last_scanned: Option<i64>,
    pub photo_count: i64,
    #[serde(default = "default_true")]
    pub face_detect_enabled: bool,
}

fn default_true() -> bool {
    true
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
    #[serde(default)]
    pub edit_rotation: Option<i32>,
    #[serde(default)]
    pub has_unsaved_edits: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderWithPhotos {
    pub folder: Folder,
    pub photos: Vec<Photo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub current_folder: String,
    #[serde(default)]
    pub current_file: Option<String>,
    pub scanned_photos: usize,
    pub total_found: usize,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceScanProgress {
    #[serde(default)]
    pub current_photo: Option<String>,
    pub processed: usize,
    pub pending: usize,
    pub faces_found: usize,
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
    #[serde(default)]
    pub render_file: Option<String>,
    #[serde(default)]
    pub cumulative_rotation: i32,
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
    pub preview_path: Option<String>,
    pub has_conflict: bool,
    pub is_committed: bool,
    #[serde(default)]
    pub session_dir: Option<String>,
    #[serde(default)]
    pub session_manifest_path: Option<String>,
    #[serde(default)]
    pub steps: Vec<EditStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditResult {
    pub success: bool,
    pub photo: Option<Photo>,
    pub session_state: EditSessionState,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CropSuggestion {
    pub id: usize,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StraightenSuggestion {
    pub id: usize,
    pub label: String,
    pub angle: f64,
    pub confidence: f64,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedEyeBox {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetouchSpot {
    pub id: String,
    pub target_x: f64,
    pub target_y: f64,
    pub radius: f64,
    pub source_x: f64,
    pub source_y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceBox {
    pub id: i64,
    pub photo_id: i64,
    pub person_id: Option<i64>,
    pub person_name: Option<String>,
    pub box_x: f32,
    pub box_y: f32,
    pub box_w: f32,
    pub box_h: f32,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceCollection {
    pub id: i64,
    pub name: String,
    pub cover_face_id: Option<i64>,
    pub cover_photo_hash: Option<String>,
    pub photo_count: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaceSettings {
    pub enabled: bool,
    pub models_installed: bool,
    pub is_downloading: bool,
    pub download_progress: f32,
    pub model_size_mb: f64,
    pub pending_photos: i64,
    pub total_faces: i64,
    pub total_people: i64,
    #[serde(default = "default_cluster_threshold")]
    pub cluster_threshold: f32,
}

fn default_cluster_threshold() -> f32 {
    0.50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextItem {
    pub id: String,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub scale: f64,
    #[serde(alias = "fontSize")]
    pub font_size: f64,
    #[serde(alias = "fontFamily")]
    pub font_family: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub alignment: String,
    pub color: String,
    #[serde(alias = "hasFill")]
    pub has_fill: bool,
    #[serde(alias = "hasOutline")]
    pub has_outline: bool,
    #[serde(alias = "outlineColor")]
    pub outline_color: String,
    #[serde(alias = "outlineWidth")]
    pub outline_width: f64,
    pub transparency: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExifTagEntry {
    pub tag: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoMetadata {
    pub photo_id: i64,
    pub file_path: String,
    pub file_name: String,
    pub location: String,
    pub file_size: i64,
    pub file_size_formatted: String,
    pub dimensions: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub megapixels: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub camera_date: Option<String>,
    pub digitized_date: Option<String>,
    pub modified_date: Option<String>,
    pub file_date: Option<String>,
    pub orientation: Option<String>,
    pub flash: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<String>,
    pub focal_length_35mm: Option<String>,
    pub ccd_width: Option<String>,
    pub exposure_time: Option<String>,
    pub aperture: Option<String>,
    pub f_number: Option<String>,
    pub iso: Option<String>,
    pub white_balance: Option<String>,
    pub metering_mode: Option<String>,
    pub exposure_program: Option<String>,
    pub color_space: Option<String>,
    pub jpeg_quality: Option<String>,
    pub people_tags: Vec<String>,
    pub unique_id: String,
    pub x_resolution: Option<String>,
    pub y_resolution: Option<String>,
    pub resolution_unit: Option<String>,
    pub all_tags: Vec<ExifTagEntry>,
}

