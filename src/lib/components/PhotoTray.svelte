<script lang="ts">
  import type { Photo } from '../types';

  interface Props {
    selectedPhotos: Photo[];
    isHeld: boolean;
    onToggleHold: () => void;
    onClearTray: () => void;
    onRemovePhoto: (photoId: number) => void;
    onPlaySlideshow: (photos: Photo[]) => void;
    onExport: (photos: Photo[]) => void;
  }

  let {
    selectedPhotos,
    isHeld,
    onToggleHold,
    onClearTray,
    onRemovePhoto,
    onPlaySlideshow,
    onExport,
  }: Props = $props();
</script>

<div
  class="flex items-center gap-3 px-3 py-1.5 h-full select-none"
  style="
    background-color: var(--tray-bg);
    border-right: 1px solid var(--border-subtle);
  "
>
  <!-- Hold & Clear Buttons (Classic Picasa Style) -->
  <div class="flex items-center gap-1">
    <!-- Hold Button (Green Pin) -->
    <button
      onclick={onToggleHold}
      disabled={selectedPhotos.length === 0}
      title={isHeld ? 'Unhold Selection (Ctrl+H)' : 'Hold Selection in Tray (Ctrl+H)'}
      class="flex items-center gap-1.5 px-2.5 py-1 text-xs font-semibold rounded transition-all disabled:opacity-30 disabled:pointer-events-none {isHeld ? 'bg-emerald-600 text-white shadow-sm ring-2 ring-emerald-400/40' : 'hover:bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/30'}"
    >
      <!-- Pushpin Icon -->
      <svg class="w-3.5 h-3.5 {isHeld ? 'rotate-[-30deg]' : ''} transition-transform" fill="currentColor" viewBox="0 0 24 24">
        <path d="M16 12V4h1V2H7v2h1v8l-2 2v2h5.2v6h1.6v-6H18v-2l-2-2z"/>
      </svg>
      <span>{isHeld ? 'Held' : 'Hold'}</span>
    </button>

    <!-- Clear Button (Red Icon) -->
    <button
      onclick={onClearTray}
      disabled={selectedPhotos.length === 0}
      title="Clear Tray"
      class="p-1 rounded text-red-500 hover:text-red-400 hover:bg-red-500/10 transition-colors disabled:opacity-30 disabled:pointer-events-none"
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" />
      </svg>
    </button>
  </div>

  <!-- Divider -->
  <div class="h-6 w-px" style="background-color: var(--border-subtle);"></div>

  <!-- Tray Thumbnails Preview Area -->
  <div class="flex items-center gap-1.5 overflow-x-auto max-w-[280px] lg:max-w-[420px] py-0.5 px-1 scrollbar-none">
    {#if selectedPhotos.length === 0}
      <span class="text-xs italic truncate" style="color: var(--text-dim);">
        Select photos to hold or batch-action
      </span>
    {:else}
      {#each selectedPhotos as photo (photo.id)}
        <div class="relative group shrink-0 w-8 h-8 rounded border overflow-hidden shadow-xs" style="border-color: var(--border-focus); background-color: var(--card-bg);">
          <img
            src={photo.thumbnail_path || `http://127.0.0.1:3456/thumb/${photo.file_hash}`}
            alt={photo.file_name}
            class="w-full h-full object-cover"
            loading="lazy"
          />
          <button
            onclick={(e) => { e.stopPropagation(); onRemovePhoto(photo.id); }}
            title="Remove from tray"
            class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center text-white transition-opacity"
          >
            <svg class="w-3.5 h-3.5 text-red-300" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2.5" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      {/each}
    {/if}
  </div>

  <!-- Tray Badge Count -->
  {#if selectedPhotos.length > 0}
    <span
      class="text-xs font-medium px-2 py-0.5 rounded-full shrink-0"
      style="background-color: var(--border-subtle); color: var(--text-muted);"
    >
      {selectedPhotos.length} {selectedPhotos.length === 1 ? 'item' : 'items'}
    </span>

    <!-- Quick Batch Actions -->
    <div class="flex items-center gap-1 shrink-0 ml-1">
      <button
        onclick={() => onPlaySlideshow(selectedPhotos)}
        title="Slideshow of selected photos"
        class="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
        style="color: var(--text-main);"
      >
        <svg class="w-3.5 h-3.5 text-blue-500" fill="currentColor" viewBox="0 0 24 24">
          <path d="M8 5v14l11-7z" />
        </svg>
        <span class="hidden sm:inline">Play</span>
      </button>

      {#if import.meta.env.DEV}
        <button
          onclick={() => onExport(selectedPhotos)}
          title="Export selected photos"
          class="flex items-center gap-1 px-2 py-1 text-xs font-medium rounded hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
          style="color: var(--text-main);"
        >
          <svg class="w-3.5 h-3.5 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
          </svg>
          <span class="hidden sm:inline">Export</span>
        </button>
      {/if}
    </div>
  {/if}
</div>
