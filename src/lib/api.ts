import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Folder, FolderWithPhotos, LibraryStats, Photo, ScanProgress } from './types';

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

export async function setThumbnailPriority(hashes: string[], replace = true): Promise<void> {
  if (hashes.length === 0 && !replace) return;
  if (isTauri) {
    try {
      return await invoke('set_thumbnail_priority', { hashes, replace });
    } catch {
      // Fallback
    }
  }
  await fetch(`${SERVER_URL}/api/thumbs/priority`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ hashes, replace }),
  });
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

export function getThumbnailUrl(photo: Photo): string {
  if (isTauri) {
    return `pica://localhost/thumb/${photo.file_hash}`;
  }
  return `${SERVER_URL}/thumb/${photo.file_hash}`;
}

export function getFullImageUrl(photo: Photo): string {
  if (isTauri) {
    return `pica://localhost/file?path=${encodeURIComponent(photo.file_path)}`;
  }
  return `${SERVER_URL}/photo?path=${encodeURIComponent(photo.file_path)}`;
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
