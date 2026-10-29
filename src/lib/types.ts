export interface Folder {
  id: number;
  path: string;
  display_name: string;
  scan_mode: 'always' | 'once' | 'exclude';
  last_scanned: number | null;
  photo_count: number;
  face_detect_enabled?: boolean;
}

export interface Photo {
  id: number;
  folder_id: number;
  file_path: string;
  file_hash: string;
  file_name: string;
  file_size: number;
  width: number | null;
  height: number | null;
  date_taken: number | null;
  camera_make: string | null;
  camera_model: string | null;
  orientation: number | null;
  is_favorite: boolean;
  thumbnail_path: string | null;
  edit_rotation?: number | null;
  has_unsaved_edits?: boolean;
}

export interface FolderWithPhotos {
  folder: Folder;
  photos: Photo[];
}

export interface ViewerFolderContext {
  current_index: number;
  folder_name: string;
  folder_path: string;
  photos: Photo[];
}

export interface ScanProgress {
  current_folder: string;
  current_file?: string;
  scanned_photos: number;
  total_found: number;
  is_complete: boolean;
}

export interface FaceScanProgress {
  current_photo?: string;
  processed: number;
  pending: number;
  faces_found: number;
  is_complete: boolean;
}

export interface LibraryStats {
  total_photos: number;
  total_folders: number;
  total_favorites: number;
  is_scanning: boolean;
}

export interface AppSettings {
  original_handling: 'backup' | 'overwrite';
  hide_originals: boolean;
}

export interface EditStep {
  step_id: number;
  op: string;
  params: Record<string, any>;
  timestamp: number;
  render_file?: string | null;
  cumulative_rotation?: number;
}

export interface EditSessionState {
  photo_id: number;
  file_path: string;
  has_session: boolean;
  can_undo: boolean;
  can_redo: boolean;
  current_step: number;
  total_steps: number;
  cumulative_rotation: number;
  has_backup: boolean;
  backup_path?: string | null;
  preview_path?: string | null;
  has_conflict: boolean;
  is_committed: boolean;
  session_dir?: string | null;
  session_manifest_path?: string | null;
  steps?: EditStep[];
}

export interface EditResult {
  success: boolean;
  photo: Photo | null;
  session_state: EditSessionState;
  message: string | null;
}

export interface CropSuggestion {
  id: number;
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StraightenSuggestion {
  id: number;
  label: string;
  angle: number;
  confidence: number;
  description: string;
}

export interface RedEyeBox {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RetouchSpot {
  id: string;
  target_x: number;
  target_y: number;
  radius: number;
  source_x: number;
  source_y: number;
}

export interface FaceBox {
  id: number;
  photo_id: number;
  person_id: number | null;
  person_name: string | null;
  box_x: number;
  box_y: number;
  box_w: number;
  box_h: number;
  confidence: number;
}

export interface FaceCollection {
  id: number;
  name: string;
  cover_face_id: number | null;
  cover_photo_hash: string | null;
  photo_count: number;
  updated_at: number;
}

export interface FaceSettings {
  enabled: boolean;
  models_installed: boolean;
  is_downloading: boolean;
  download_progress: number;
  model_size_mb: number;
  pending_photos: number;
  total_faces: number;
  total_people: number;
  cluster_threshold?: number;
}

export interface FaceScanTimingRecord {
  photo_id: number;
  file_name: string;
  file_hash: string;
  worker_id: number;
  is_thumbnail: boolean;
  width: number;
  height: number;
  decode_ms: number;
  detect_ms: number;
  faces_detected: number;
  embed_ms: number;
  cluster_ms: number;
  db_save_ms: number;
  total_ms: number;
  timestamp_ms: number;
}

export interface FaceTimingStats {
  is_recording: boolean;
  record_count: number;
  avg_decode_ms: number;
  avg_detect_ms: number;
  avg_embed_ms: number;
  avg_total_ms: number;
  min_total_ms: number;
  max_total_ms: number;
  photos_per_second: number;
  log_file_path: string;
}

export interface TextItem {
  id: string;
  text: string;
  x: number; // normalized center X [0..1]
  y: number; // normalized center Y [0..1]
  angle: number; // degrees [0..360]
  scale: number; // scale multiplier
  fontSize: number; // base pt size
  fontFamily: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  alignment: 'left' | 'center' | 'right';
  color: string;
  hasFill: boolean;
  hasOutline: boolean;
  outlineColor: string;
  outlineWidth: number;
  transparency: number; // 0..100 (0 = solid, 100 = transparent)
}

export interface ExifTagEntry {
  tag: string;
  value: string;
}

export interface PhotoMetadata {
  photo_id: number;
  file_path: string;
  file_name: string;
  location: string;
  file_size: number;
  file_size_formatted: string;
  dimensions: string | null;
  width: number | null;
  height: number | null;
  megapixels: string | null;
  camera_make: string | null;
  camera_model: string | null;
  camera_date: string | null;
  digitized_date: string | null;
  modified_date: string | null;
  file_date: string | null;
  orientation: string | null;
  flash: string | null;
  lens: string | null;
  focal_length: string | null;
  focal_length_35mm: string | null;
  ccd_width: string | null;
  exposure_time: string | null;
  aperture: string | null;
  f_number: string | null;
  iso: string | null;
  white_balance: string | null;
  metering_mode: string | null;
  exposure_program: string | null;
  color_space: string | null;
  jpeg_quality: string | null;
  people_tags: string[];
  unique_id: string;
  x_resolution: string | null;
  y_resolution: string | null;
  resolution_unit: string | null;
  all_tags: ExifTagEntry[];
}
