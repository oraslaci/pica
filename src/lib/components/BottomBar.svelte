<script lang="ts">
  import {
    RefreshCw,
    LayoutGrid,
    Square,
    Users,
  } from '@lucide/svelte';
  import PhotoTray from './PhotoTray.svelte';
  import type { LibraryStats, Photo, ScanProgress } from '../types';

  let {
    stats = {
      total_photos: 0,
      total_folders: 0,
      total_favorites: 0,
      is_scanning: false,
    },
    scanProgress = null as ScanProgress | null,
    faceScanPending = 0,
    zoomSize = $bindable(180),
    selectedPhotos = [] as Photo[],
    isHeld = false,
    onToggleHold = () => {},
    onClearTray = () => {},
    onRemovePhoto = (_photoId: number) => {},
    onPlaySlideshow = (_photos: Photo[]) => {},
    onExport = (_photos: Photo[]) => {},
    onOpenProcessingNotification = () => {},
  }: {
    stats: LibraryStats;
    scanProgress: ScanProgress | null;
    faceScanPending?: number;
    zoomSize: number;
    selectedPhotos: Photo[];
    isHeld: boolean;
    onToggleHold: () => void;
    onClearTray: () => void;
    onRemovePhoto: (photoId: number) => void;
    onPlaySlideshow: (photos: Photo[]) => void;
    onExport: (photos: Photo[]) => void;
    onOpenProcessingNotification?: () => void;
  } = $props();
</script>

<div
  class="h-12 border-t px-3 flex items-center justify-between select-none text-xs shadow-md transition-colors"
  style="
    background-color: var(--footer-bg);
    border-color: var(--border-subtle);
    color: var(--text-main);
  "
>
  <!-- Left Side: Authentic Picasa Photo Tray -->
  <div class="h-full flex items-center">
    <PhotoTray
      {selectedPhotos}
      {isHeld}
      {onToggleHold}
      {onClearTray}
      {onRemovePhoto}
      {onPlaySlideshow}
      {onExport}
    />
  </div>

  <!-- Center: Background Scan Progress & Total Library Stats -->
  <div class="flex items-center gap-3 px-2">
    {#if stats.is_scanning || (scanProgress && !scanProgress.is_complete)}
      <button
        type="button"
        onclick={onOpenProcessingNotification}
        class="flex items-center gap-2 bg-blue-500/10 hover:bg-blue-500/20 border border-blue-500/30 px-3 py-1 rounded-full text-blue-500 dark:text-blue-300 transition-colors cursor-pointer"
        title="Click to view file processing progress"
      >
        <RefreshCw class="w-3 h-3 animate-spin" />
        <span class="text-[11px] font-medium truncate max-w-xs">
          Scanning {scanProgress?.current_folder || 'Library'}... ({scanProgress?.scanned_photos || 0} photos)
        </span>
      </button>
    {:else}
      <div class="flex items-center gap-2 text-xs" style="color: var(--text-muted);">
        <span class="font-medium" style="color: var(--text-main);">{stats.total_photos.toLocaleString()} photos</span>
        <span>•</span>
        <span>{stats.total_folders} {stats.total_folders === 1 ? 'folder' : 'folders'}</span>
        {#if stats.total_favorites > 0}
          <span>•</span>
          <span class="text-amber-500 font-medium">{stats.total_favorites} starred</span>
        {/if}
        {#if faceScanPending > 0}
          <span>•</span>
          <button
            type="button"
            onclick={onOpenProcessingNotification}
            class="text-purple-400 hover:text-purple-300 flex items-center gap-1 font-medium text-[11px] cursor-pointer transition-colors"
            title="Click to view face processing progress"
          >
            <Users class="w-3 h-3 animate-pulse" />
            <span>Scanning faces ({faceScanPending} left)</span>
          </button>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Right Side: Picasa Kinetic Zoom Slider -->
  <div class="flex items-center gap-2.5">
    <button
      onclick={() => (zoomSize = Math.max(90, zoomSize - 30))}
      class="p-1 rounded hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
      style="color: var(--text-muted);"
      title="Smaller thumbnails"
    >
      <LayoutGrid class="w-3.5 h-3.5" />
    </button>

    <input
      type="range"
      min="90"
      max="360"
      step="10"
      bind:value={zoomSize}
      class="w-28 h-1.5 rounded-lg appearance-none cursor-pointer accent-blue-500 hover:accent-blue-400"
      style="background-color: var(--border-subtle);"
      title={`Thumbnail size: ${zoomSize}px`}
    />

    <button
      onclick={() => (zoomSize = Math.min(360, zoomSize + 30))}
      class="p-1 rounded hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
      style="color: var(--text-muted);"
      title="Larger thumbnails"
    >
      <Square class="w-4 h-4" />
    </button>
  </div>
</div>
