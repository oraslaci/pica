export interface Folder {
  id: number;
  path: string;
  display_name: string;
  scan_mode: 'always' | 'once' | 'exclude';
  last_scanned: number | null;
  photo_count: number;
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
}

export interface FolderWithPhotos {
  folder: Folder;
  photos: Photo[];
}

export interface ScanProgress {
  current_folder: string;
  scanned_photos: number;
  total_found: number;
  is_complete: boolean;
}

export interface LibraryStats {
  total_photos: number;
  total_folders: number;
  total_favorites: number;
  is_scanning: boolean;
}
