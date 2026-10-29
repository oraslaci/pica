import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AppSettings,
  CropSuggestion,
  EditResult,
  EditSessionState,
  FaceBox,
  FaceCollection,
  FaceSettings,
  Folder,
  FolderWithPhotos,
  LibraryStats,
  Photo,
  PhotoMetadata,
  ScanProgress,
  FaceScanProgress,
  FaceScanTimingRecord,
  FaceTimingStats,
  RedEyeBox,
  RetouchSpot,
  StraightenSuggestion,
  TextItem,
  ViewerFolderContext,
} from './types';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const SERVER_URL = 'http://127.0.0.1:3456';

export async function getWatchFolders(): Promise<Folder[]> {
  if (isTauri) {
    try {
      return await invoke<Folder[]>('get_watch_folders');
    } catch {
      // Fallback to local server
    }
  }
  const res = await fetch(`${SERVER_URL}/api/folders`);
  if (!res.ok) throw new Error('Failed to load folders');
  return await res.json();
}

export async function addWatchFolder(path: string, scanMode: 'always' | 'once' | 'exclude'): Promise<Folder> {
  if (isTauri) {
    try {
      return await invoke<Folder>('add_watch_folder', { path, scanMode });
    } catch {
      // Fallback to local server
    }
  }
  const res = await fetch(`${SERVER_URL}/api/folders`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path, scan_mode: scanMode }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: 'Failed to add folder' }));
    throw new Error(err.error || 'Failed to add folder');
  }
  return await res.json();
}

export async function updateWatchFolderMode(folderId: number, scanMode: string): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('update_watch_folder_mode', { folderId, scanMode });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/folders/mode`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ folder_id: folderId, scan_mode: scanMode }),
  });
}

export async function removeWatchFolder(folderId: number): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('remove_watch_folder', { folderId });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/folders?id=${folderId}`, {
    method: 'DELETE',
  });
}

export async function triggerScan(): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('trigger_scan');
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/scan`, {
    method: 'POST',
  });
}

export async function setThumbnailPriority(
  screenHashes: string[],
  nearHashes: string[] = []
): Promise<void> {
  if (screenHashes.length === 0 && nearHashes.length === 0) return;
  if (isTauri) {
    try {
      return await invoke('set_thumbnail_priority', {
        screen: screenHashes,
        near: nearHashes,
        hashes: screenHashes,
        replace: true,
      });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/thumbs/priority`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      screen: screenHashes,
      near: nearHashes,
      hashes: screenHashes,
      replace: true,
    }),
  });
}

export async function clearThumbnailCache(): Promise<{ deleted: number }> {
  if (isTauri) {
    try {
      return await invoke<{ deleted: number }>('debug_clear_thumbs');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/clear-thumbs`, { method: 'POST' });
  return res.ok ? await res.json() : { deleted: 0 };
}

export async function getThumbStats(): Promise<{ cached_thumbs: number; total_bytes: number }> {
  if (isTauri) {
    try {
      return await invoke<{ cached_thumbs: number; total_bytes: number }>('debug_thumb_stats');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/thumb-stats`);
  return res.ok ? await res.json() : { cached_thumbs: 0, total_bytes: 0 };
}

export async function getFoldersWithPhotos(): Promise<FolderWithPhotos[]> {
  if (isTauri) {
    try {
      return await invoke<FolderWithPhotos[]>('get_folders_with_photos');
    } catch {
      // Fallback to local server
    }
  }
  const res = await fetch(`${SERVER_URL}/api/library`);
  if (!res.ok) return [];
  return await res.json();
}

export async function toggleFavorite(photoId: number): Promise<boolean> {
  if (isTauri) {
    try {
      return await invoke<boolean>('toggle_favorite', { photoId });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/favorite`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (res.ok) {
    const data = await res.json();
    return data.is_favorite;
  }
  return false;
}

export async function getLibraryStats(): Promise<LibraryStats> {
  if (isTauri) {
    try {
      return await invoke<LibraryStats>('get_library_stats');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/stats`);
  if (!res.ok) {
    return {
      total_photos: 0,
      total_folders: 0,
      total_favorites: 0,
      is_scanning: false,
    };
  }
  return await res.json();
}

export function getThumbnailUrl(photo: Photo, cacheBuster?: string | number): string {
  const cb = cacheBuster !== undefined ? `?v=${cacheBuster}` : '';
  return `${SERVER_URL}/thumb/${photo.file_hash}${cb}`;
}

export function getFullImageUrl(photo: Photo, cacheBuster?: string | number): string {
  const cb = cacheBuster !== undefined ? `&v=${cacheBuster}` : `&v=${photo.file_hash}`;
  return `${SERVER_URL}/photo?path=${encodeURIComponent(photo.file_path)}${cb}`;
}

export function getOriginalImageUrl(photo: Photo, backupPath?: string | null): string {
  const targetPath = backupPath || photo.file_path;
  return `${SERVER_URL}/photo?path=${encodeURIComponent(targetPath)}`;
}

export function getPreviewImageUrl(filePath: string, cacheBuster?: string | number): string {
  const cb = cacheBuster !== undefined ? `&v=${cacheBuster}` : '';
  return `${SERVER_URL}/photo?path=${encodeURIComponent(filePath)}${cb}`;
}

export function onScanProgress(callback: (prog: ScanProgress) => void) {
  if (!isTauri) {
    // In browser mode, poll stats periodically
    const interval = setInterval(async () => {
      try {
        const stats = await getLibraryStats();
        if (stats.is_scanning) {
          callback({
            current_folder: 'Library',
            scanned_photos: stats.total_photos,
            total_found: stats.total_photos,
            is_complete: false,
          });
        }
      } catch {}
    }, 1500);
    return () => clearInterval(interval);
  }
  return listen<ScanProgress>('scan-progress', (event) => {
    callback(event.payload);
  });
}

export function onScanComplete(callback: () => void) {
  if (!isTauri) {
    return () => {};
  }
  return listen('scan-complete', () => {
    callback();
  });
}

export function onFaceScanProgress(callback: (progress: FaceScanProgress) => void) {
  if (!isTauri) {
    return () => {};
  }
  return listen<FaceScanProgress>('face-scan-progress', (event) => {
    callback(event.payload);
  });
}

const hashListeners = new Map<string, Set<() => void>>();
let totalListeners = 0;
let listeningTauri = false;
let pollingTimer: ReturnType<typeof setInterval> | null = null;
let lastSeq = 0;

export function onThumbnailReady(hash: string, callback: () => void): () => void {
  let set = hashListeners.get(hash);
  if (!set) {
    set = new Set();
    hashListeners.set(hash, set);
  }
  set.add(callback);
  totalListeners++;

  initThumbnailNotifications();

  return () => {
    const s = hashListeners.get(hash);
    if (s) {
      s.delete(callback);
      if (s.size === 0) {
        hashListeners.delete(hash);
      }
    }
    totalListeners--;
  };
}

function notifyThumbnailReady(hash: string) {
  const set = hashListeners.get(hash);
  if (set) {
    for (const cb of set) {
      cb();
    }
  }
}

async function initThumbnailNotifications() {
  if (isTauri && !listeningTauri) {
    listeningTauri = true;
    try {
      await listen<string>('thumbnail-ready', (event) => {
        notifyThumbnailReady(event.payload);
      });
      return;
    } catch {
      // Fall back to server polling
    }
  }

  if (!pollingTimer && typeof window !== 'undefined') {
    pollingTimer = setInterval(async () => {
      if (totalListeners === 0) return;
      try {
        const res = await fetch(`${SERVER_URL}/api/thumbs/ready?since=${lastSeq}`);
        if (res.ok) {
          const data = await res.json();
          if (data.seq !== undefined) lastSeq = data.seq;
          if (Array.isArray(data.hashes)) {
            for (const h of data.hashes) {
              notifyThumbnailReady(h);
            }
          }
        }
      } catch {
        // server idle
      }
    }, 150);
  }
}

// -------------------------------------------------------------
// Options & Settings API
// -------------------------------------------------------------
export async function getAppSettings(): Promise<AppSettings> {
  if (isTauri) {
    try {
      return await invoke<AppSettings>('get_app_settings');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/settings`);
  if (!res.ok) return { original_handling: 'backup', hide_originals: true };
  return await res.json();
}

export async function updateAppSettings(settings: AppSettings): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('update_app_settings', { settings });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  });
}

// -------------------------------------------------------------
// Image Editing & Rotation API
// -------------------------------------------------------------
const sessionCache = new Map<number, EditSessionState>();

export function getCachedEditSession(photoId: number): EditSessionState | null {
  return sessionCache.get(photoId) ?? null;
}

export async function getEditSession(photoId: number): Promise<EditSessionState> {
  if (isTauri) {
    try {
      const sess = await invoke<EditSessionState>('get_edit_session', { photoId });
      sessionCache.set(photoId, sess);
      return sess;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/session?photo_id=${photoId}`);
  if (!res.ok) throw new Error('Failed to load edit session');
  const sess: EditSessionState = await res.json();
  sessionCache.set(photoId, sess);
  return sess;
}

export async function applyRotation(photoId: number, degrees: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_rotation', { photoId, degrees });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/rotate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, degrees }),
  });
  if (!res.ok) throw new Error('Failed to apply rotation');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function undoEdit(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('undo_edit', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/undo`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to undo edit');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function redoEdit(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('redo_edit', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/redo`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to redo edit');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function jumpToEditStep(photoId: number, stepId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('jump_to_step', { photoId, stepId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/step`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, step_id: stepId }),
  });
  if (!res.ok) throw new Error('Failed to jump to edit step');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function saveEditSession(photoId: number): Promise<EditSessionState> {
  let sess: EditSessionState;
  if (isTauri) {
    try {
      sess = await invoke<EditSessionState>('save_edit_session', { photoId });
      sessionCache.set(photoId, sess);
      return sess;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/save`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to save edit session');
  sess = await res.json();
  sessionCache.set(photoId, sess);
  return sess;
}

export async function revertEditSession(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('revert_edit_session', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/revert`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to revert edit session');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function resolveEditConflict(photoId: number, choice: string): Promise<EditResult> {
  if (isTauri) {
    try {
      return await invoke<EditResult>('resolve_edit_conflict', { photoId, choice });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/resolve_conflict`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, choice }),
  });
  if (!res.ok) throw new Error('Failed to resolve edit conflict');
  return await res.json();
}

export async function getCropSuggestions(
  photoId: number,
  aspectRatio?: number
): Promise<CropSuggestion[]> {
  if (isTauri) {
    try {
      return await invoke<CropSuggestion[]>('get_crop_suggestions', {
        photoId,
        aspectRatio: aspectRatio ?? null,
      });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/crop_suggestions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, aspect_ratio: aspectRatio ?? null }),
  });
  if (!res.ok) return [];
  return await res.json();
}

export async function applyCrop(
  photoId: number,
  x: number,
  y: number,
  width: number,
  height: number
): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_crop', { photoId, x, y, width, height });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/crop`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, x, y, width, height }),
  });
  if (!res.ok) throw new Error('Failed to apply crop');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyStraighten(
  photoId: number,
  angle: number
): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_straighten', { photoId, angle });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/straighten`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, angle }),
  });
  if (!res.ok) throw new Error('Failed to apply straighten');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function getStraightenSuggestions(
  photoId: number
): Promise<StraightenSuggestion[]> {
  if (isTauri) {
    try {
      return await invoke<StraightenSuggestion[]>('get_straighten_suggestions', {
        photoId,
      });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/straighten_suggestions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) return [];
  return await res.json();
}

export async function detectRedEyes(photoId: number): Promise<RedEyeBox[]> {
  if (isTauri) {
    try {
      return await invoke<RedEyeBox[]>('detect_red_eyes', { photoId });
    } catch (err) {
      console.warn('Tauri detect_red_eyes failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/detect_red_eyes`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to detect red eyes');
  return await res.json();
}

export async function applyRedEyeStep(
  photoId: number,
  boxes: RedEyeBox[]
): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_redeye_step', { photoId, boxes });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_redeye_step failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/redeye`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, boxes }),
  });
  if (!res.ok) throw new Error('Failed to apply red eye repair');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyRetouchStep(
  photoId: number,
  spots: RetouchSpot[]
): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_retouch_step', { photoId, spots });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_retouch_step failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/retouch`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, spots }),
  });
  if (!res.ok) throw new Error('Failed to apply retouch step');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyTextStep(
  photoId: number,
  items: TextItem[],
  overlayBase64?: string
): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_text_step', {
        photoId,
        items,
        overlayBase64: overlayBase64 || null,
      });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_text_step failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/text`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      photo_id: photoId,
      items,
      overlay_base64: overlayBase64 || null,
    }),
  });
  if (!res.ok) throw new Error('Failed to apply text step');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyFeelingLucky(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_feeling_lucky', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_feeling_lucky failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/lucky`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to apply I\'m Feeling Lucky');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyAutoContrast(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_auto_contrast', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_auto_contrast failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/autocontrast`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to apply Auto Contrast');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyAutoColor(photoId: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_auto_color', { photoId });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_auto_color failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/autocolor`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId }),
  });
  if (!res.ok) throw new Error('Failed to apply Auto Color');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

export async function applyFillLight(photoId: number, amount: number): Promise<EditResult> {
  let result: EditResult;
  if (isTauri) {
    try {
      result = await invoke<EditResult>('apply_fill_light', { photoId, amount });
      if (result.session_state) sessionCache.set(photoId, result.session_state);
      return result;
    } catch (err) {
      console.warn('Tauri apply_fill_light failed, falling back:', err);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/edit/filllight`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ photo_id: photoId, amount }),
  });
  if (!res.ok) throw new Error('Failed to apply Fill Light');
  result = await res.json();
  if (result.session_state) sessionCache.set(photoId, result.session_state);
  return result;
}

// ---------------------------------------------------------------------------
// Face Detection & Face Collections (People) API
// ---------------------------------------------------------------------------

export function getFaceCropUrl(faceId: number): string {
  return `${SERVER_URL}/api/faces/${faceId}/crop`;
}

export async function getFaceSettings(): Promise<FaceSettings> {
  if (isTauri) {
    try {
      return await invoke<FaceSettings>('get_face_settings');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/settings`);
  if (!res.ok) throw new Error('Failed to get face settings');
  return await res.json();
}

export async function updateFaceSettings(enabled: boolean): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('update_face_settings', { enabled });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/faces/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  });
}

export async function updateFolderFaceDetect(folderId: number, enabled: boolean): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('update_folder_face_detect', { folderId, enabled });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/faces/folder/detect`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ folder_id: folderId, enabled }),
  });
}

export async function getFaceCollections(): Promise<FaceCollection[]> {
  if (isTauri) {
    try {
      return await invoke<FaceCollection[]>('get_face_collections');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/collections`);
  if (!res.ok) throw new Error('Failed to get face collections');
  return await res.json();
}

export async function getFacesForPhoto(photoId: number): Promise<FaceBox[]> {
  if (isTauri) {
    try {
      return await invoke<FaceBox[]>('get_faces_for_photo', { photoId });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/photo?id=${photoId}`);
  if (!res.ok) return [];
  return await res.json();
}

export async function getPhotosForPerson(personId: number): Promise<Photo[]> {
  if (isTauri) {
    try {
      return await invoke<Photo[]>('get_photos_for_person', { personId });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/person?id=${personId}`);
  if (!res.ok) return [];
  return await res.json();
}

export async function updatePersonName(personId: number, name: string): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('update_person_name', { personId, name });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/faces/person/rename`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ person_id: personId, name }),
  });
}

export async function triggerFaceScan(): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('trigger_face_scan');
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/faces/scan`, {
    method: 'POST',
  });
}

export async function reclusterFaces(threshold?: number): Promise<{ people_count: number }> {
  if (isTauri) {
    try {
      const count = await invoke<number>('recluster_faces', { threshold });
      return { people_count: count };
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/recluster`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ threshold }),
  });
  if (!res.ok) throw new Error('Failed to recluster faces');
  return await res.json();
}

export async function realignFaceEmbeddings(): Promise<{ updated_faces: number; people_count: number }> {
  if (isTauri) {
    try {
      return await invoke<{ updated_faces: number; people_count: number }>('realign_face_embeddings');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/faces/realign`, {
    method: 'POST',
  });
  if (!res.ok) throw new Error('Failed to realign face embeddings');
  return await res.json();
}

export async function getFaceTimingStats(): Promise<FaceTimingStats> {
  if (isTauri) {
    try {
      return await invoke<FaceTimingStats>('get_face_timing_stats');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/face-timings`);
  if (!res.ok) {
    return {
      is_recording: false,
      record_count: 0,
      avg_decode_ms: 0,
      avg_detect_ms: 0,
      avg_embed_ms: 0,
      avg_total_ms: 0,
      min_total_ms: 0,
      max_total_ms: 0,
      photos_per_second: 0,
      log_file_path: '',
    };
  }
  const data = await res.json();
  return data.stats;
}

export async function setFaceTimingRecording(enabled: boolean): Promise<void> {
  if (isTauri) {
    try {
      return await invoke<void>('set_face_timing_recording', { enabled });
    } catch {
      // Fallback to HTTP
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/face-timings/toggle`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) {
    throw new Error(`Server returned ${res.status}: ${res.statusText}`);
  }
}

export async function getFaceTimingRecords(limit?: number): Promise<FaceScanTimingRecord[]> {
  if (isTauri) {
    try {
      return await invoke<FaceScanTimingRecord[]>('get_face_timing_records', { limit });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/face-timings${limit ? `?limit=${limit}` : ''}`);
  if (!res.ok) return [];
  const data = await res.json();
  return data.records || [];
}

export async function clearFaceTimingRecords(): Promise<void> {
  if (isTauri) {
    try {
      return await invoke<void>('clear_face_timing_records');
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/debug/face-timings/clear`, {
    method: 'POST',
  });
}

export async function exportFaceTimingCsv(): Promise<string> {
  if (isTauri) {
    try {
      return await invoke<string>('export_face_timing_csv');
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/debug/face-timings/export.csv`);
  return res.ok ? await res.text() : '';
}

export async function getPhotoMetadata(photoId: number): Promise<PhotoMetadata> {
  if (isTauri) {
    try {
      return await invoke<PhotoMetadata>('get_photo_metadata', { photoId });
    } catch {
      // Fallback
    }
  }
  const res = await fetch(`${SERVER_URL}/api/photo/metadata?id=${photoId}`);
  if (!res.ok) throw new Error('Failed to load photo metadata');
  return await res.json();
}

export async function getViewerFolderContext(filePath: string): Promise<ViewerFolderContext> {
  if (isTauri) {
    try {
      return await invoke<ViewerFolderContext>('get_viewer_folder_context', { filePath });
    } catch (e) {
      console.warn('Tauri get_viewer_folder_context failed, falling back to HTTP:', e);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/viewer/folder?path=${encodeURIComponent(filePath)}`);
  if (!res.ok) throw new Error('Failed to load viewer folder context');
  return await res.json();
}

export async function getViewerPhotoMetadata(filePath: string): Promise<PhotoMetadata> {
  if (isTauri) {
    try {
      return await invoke<PhotoMetadata>('get_viewer_photo_metadata', { filePath });
    } catch (e) {
      console.warn('Tauri get_viewer_photo_metadata failed, falling back to HTTP:', e);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/viewer/metadata?path=${encodeURIComponent(filePath)}`);
  if (!res.ok) throw new Error('Failed to load viewer photo metadata');
  return await res.json();
}

export async function openViewerWindow(filePath?: string): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('open_viewer_window', { filePath });
    } catch (e) {
      console.error('Failed to open viewer window:', e);
    }
  }
  // Fallback for web mode: open new tab with ?mode=viewer
  const url = filePath
    ? `/?mode=viewer&file=${encodeURIComponent(filePath)}`
    : '/?mode=viewer';
  window.open(url, '_blank');
}

export async function openInPicaEditor(filePath: string): Promise<void> {
  if (isTauri) {
    try {
      return await invoke('open_in_pica_editor', { filePath });
    } catch (e) {
      console.error('Failed to open in Pica editor:', e);
    }
  }
  // Web fallback: navigate to main app with photo focus
  window.location.href = `/?file=${encodeURIComponent(filePath)}`;
}

export async function rotateViewerPhoto(filePath: string, degrees: number): Promise<EditResult> {
  if (isTauri) {
    try {
      return await invoke<EditResult>('rotate_viewer_photo', { filePath, degrees });
    } catch (e) {
      console.warn('Tauri rotate_viewer_photo failed:', e);
    }
  }
  const res = await fetch(`${SERVER_URL}/api/viewer/rotate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ file_path: filePath, degrees }),
  });
  if (!res.ok) throw new Error('Failed to rotate photo');
  return await res.json();
}

export async function getAppVersion(): Promise<string> {
  if (isTauri) {
    try {
      return await invoke<string>('get_app_version');
    } catch {
      // Fallback
    }
  }
  try {
    const res = await fetch(`${SERVER_URL}/api/version`);
    if (res.ok) {
      const data = await res.json();
      if (data.version) return data.version;
    }
  } catch {
    // Fallback
  }
  return '0.2.1';
}

export async function openExternalUrl(url: string): Promise<void> {
  if (isTauri) {
    try {
      await invoke('open_external_url', { url });
      return;
    } catch (e) {
      console.warn('Tauri open_external_url failed, falling back:', e);
    }
  }
  try {
    const res = await fetch(`${SERVER_URL}/api/open-url`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ url }),
    });
    if (res.ok) return;
  } catch {
    // Fallback
  }
  if (typeof window !== 'undefined') {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

export async function restartApp(): Promise<void> {
  if (isTauri) {
    try {
      await invoke('restart_app');
      return;
    } catch (e) {
      console.error('Failed to restart app:', e);
    }
  }
  if (typeof window !== 'undefined') {
    window.location.reload();
  }
}

