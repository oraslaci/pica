<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Folder as FolderIcon,
    ChevronDown,
    ChevronRight,
    Play,
    Star,
    Image as ImageIcon,
  } from '@lucide/svelte';
  import type { FolderWithPhotos, Photo } from '../types';
  import { getThumbnailUrl, setThumbnailPriority, toggleFavorite } from '../api';

  let {
    folderGroups = [],
    zoomSize = 180,
    selectedPhotos = [] as Photo[],
    onPhotoSelected = (_photo: Photo, _index: number, _allPhotos: Photo[]) => {},
    onPhotoClick = (_photo: Photo, _isMulti: boolean) => {},
    onPlayFolder = (_photos: Photo[]) => {},
  }: {
    folderGroups: FolderWithPhotos[];
    zoomSize: number;
    selectedPhotos?: Photo[];
    onPhotoSelected?: (photo: Photo, index: number, allPhotos: Photo[]) => void;
    onPhotoClick?: (photo: Photo, isMulti: boolean) => void;
    onPlayFolder?: (photos: Photo[]) => void;
  } = $props();

  let collapsedFolders = $state<Record<number, boolean>>({});

  // Container viewport dimensions
  let scrollContainer = $state<HTMLElement | null>(null);
  let containerWidth = $state(1000);
  let containerHeight = $state(800);
  let scrollTop = $state(0);

  // Layout constants
  const HEADER_HEIGHT = 40;
  const HEADER_GAP = 10;
  const FOLDER_GAP = 24;
  const GAP = 12;

  // Flattened photos for global navigation
  const allPhotos = $derived(folderGroups.flatMap((g) => g.photos));

  // Selected photos hash set for instant lookup
  const selectedIds = $derived(new Set(selectedPhotos.map((p) => p.id)));

  // Grid geometry
  const contentWidth = $derived(Math.max(100, containerWidth - 32)); // 16px padding on each side
  const columns = $derived(Math.max(1, Math.floor((contentWidth + GAP) / (zoomSize + GAP))));
  const colWidth = $derived(Math.max(50, (contentWidth - (columns - 1) * GAP) / columns));
  const rowHeight = $derived(colWidth + GAP);

  // Folder layouts with exact vertical positioning
  interface FolderLayout {
    folder: FolderWithPhotos['folder'];
    photos: Photo[];
    isCollapsed: boolean;
    numRows: number;
    top: number;
    headerTop: number;
    photosTop: number;
    photosHeight: number;
    totalHeight: number;
  }

  const folderLayouts = $derived.by(() => {
    let currentTop = 0;
    const layouts: FolderLayout[] = [];

    for (const group of folderGroups) {
      const isCollapsed = !!collapsedFolders[group.folder.id];
      const numRows = isCollapsed ? 0 : Math.ceil(group.photos.length / columns);
      const photosHeight = numRows > 0 ? numRows * colWidth + (numRows - 1) * GAP : 0;

      const headerTop = currentTop;
      const photosTop = headerTop + HEADER_HEIGHT + HEADER_GAP;
      const totalHeight =
        HEADER_HEIGHT + (isCollapsed ? 0 : HEADER_GAP + photosHeight) + FOLDER_GAP;

      layouts.push({
        folder: group.folder,
        photos: group.photos,
        isCollapsed,
        numRows,
        top: currentTop,
        headerTop,
        photosTop,
        photosHeight,
        totalHeight,
      });

      currentTop += totalHeight;
    }

    return layouts;
  });

  // Bidirectional virtual slice calculation
  const visibleFolderSlices = $derived.by(() => {
    const slices: Record<number, { photos: Photo[]; gridTopOffset: number }> = {};
    const bufferPx = Math.max(600, containerHeight * 1.5);
    const windowTop = Math.max(0, scrollTop - bufferPx);
    const windowBottom = scrollTop + containerHeight + bufferPx;

    let totalRendered = 0;

    for (const layout of folderLayouts) {
      if (layout.isCollapsed || layout.photos.length === 0) {
        slices[layout.folder.id] = { photos: [], gridTopOffset: 0 };
        continue;
      }

      const pTop = layout.photosTop;
      const pBottom = pTop + layout.photosHeight;

      // Check if folder photos intersect visible window
      if (pBottom < windowTop || pTop > windowBottom) {
        slices[layout.folder.id] = { photos: [], gridTopOffset: 0 };
        continue;
      }

      const visibleTop = Math.max(pTop, windowTop);
      const visibleBottom = Math.min(pBottom, windowBottom);

      const startRow = Math.max(
        0,
        Math.min(layout.numRows - 1, Math.floor((visibleTop - pTop) / rowHeight))
      );
      const endRow = Math.max(
        startRow + 1,
        Math.min(layout.numRows, Math.ceil((visibleBottom - pTop) / rowHeight))
      );

      const startIndex = startRow * columns;
      const endIndex = Math.min(layout.photos.length, endRow * columns);

      const slicePhotos = layout.photos.slice(startIndex, endIndex);
      slices[layout.folder.id] = {
        photos: slicePhotos,
        gridTopOffset: startRow * rowHeight,
      };
      totalRendered += slicePhotos.length;
    }

    // Safety fallback: if no photos were rendered but folders exist (e.g. scroll position desync on resize)
    if (totalRendered === 0 && folderGroups.length > 0) {
      let closestLayout = folderLayouts[0];
      for (const layout of folderLayouts) {
        if (!layout.isCollapsed && layout.photos.length > 0) {
          closestLayout = layout;
          if (scrollTop <= layout.top + layout.totalHeight) break;
        }
      }

      if (closestLayout && closestLayout.photos.length > 0) {
        const numRowsToRender = Math.max(3, Math.ceil(containerHeight / rowHeight) + 3);
        const startRow = Math.max(
          0,
          Math.min(
            closestLayout.numRows - 1,
            Math.floor((scrollTop - closestLayout.photosTop) / rowHeight)
          )
        );
        const endRow = Math.min(closestLayout.numRows, startRow + numRowsToRender);
        const startIndex = startRow * columns;
        const endIndex = Math.min(closestLayout.photos.length, endRow * columns);

        slices[closestLayout.folder.id] = {
          photos: closestLayout.photos.slice(startIndex, endIndex),
          gridTopOffset: startRow * rowHeight,
        };
      }
    }

    return slices;
  });

  // Collect all photos currently in the visible window slice
  const visiblePhotos = $derived.by(() => {
    const list: Photo[] = [];
    for (const group of folderGroups) {
      const slice = visibleFolderSlices[group.folder.id];
      if (slice && slice.photos.length > 0) {
        list.push(...slice.photos);
      }
    }
    return list;
  });

  // Dual-priority thumbnail scheduler integration:
  // Notify backend of visible photo hashes, preempting/emptying stale high-priority queue
  let priorityDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const hashes = visiblePhotos.map((p) => p.file_hash);
    if (hashes.length === 0) return;

    if (priorityDebounceTimer) clearTimeout(priorityDebounceTimer);
    priorityDebounceTimer = setTimeout(() => {
      setThumbnailPriority(hashes, true);
    }, 60);

    return () => {
      if (priorityDebounceTimer) clearTimeout(priorityDebounceTimer);
    };
  });

  // Scroll anchoring across window resizes & zoom size changes
  let anchorFolderId: number | null = null;
  let anchorPhotoIndex = 0;
  let prevColumns = 0;

  function captureAnchor() {
    if (!scrollContainer) return;
    const currentScroll = scrollContainer.scrollTop;
    for (const layout of folderLayouts) {
      if (layout.isCollapsed || layout.photos.length === 0) continue;
      if (currentScroll >= layout.headerTop && currentScroll < layout.top + layout.totalHeight) {
        anchorFolderId = layout.folder.id;
        if (currentScroll <= layout.photosTop) {
          anchorPhotoIndex = 0;
        } else {
          const row = Math.floor((currentScroll - layout.photosTop) / rowHeight);
          anchorPhotoIndex = Math.min(layout.photos.length - 1, row * columns);
        }
        return;
      }
    }
  }

  function restoreAnchor() {
    if (!scrollContainer || anchorFolderId === null) return;
    const targetLayout = folderLayouts.find((l) => l.folder.id === anchorFolderId);
    if (!targetLayout) return;

    const targetRow = Math.floor(anchorPhotoIndex / columns);
    const newTop = targetLayout.photosTop + targetRow * rowHeight;
    const maxScroll = Math.max(0, scrollContainer.scrollHeight - scrollContainer.clientHeight);
    const clampedTop = Math.max(0, Math.min(newTop, maxScroll));

    scrollContainer.scrollTop = clampedTop;
    scrollTop = clampedTop;
  }

  // Effect to re-anchor when column count changes
  $effect(() => {
    if (columns !== prevColumns) {
      prevColumns = columns;
      requestAnimationFrame(() => {
        restoreAnchor();
      });
    }
  });

  function handleResize() {
    if (!scrollContainer) return;
    captureAnchor();
    const w = scrollContainer.clientWidth;
    const h = scrollContainer.clientHeight;
    if (w > 0) containerWidth = w;
    if (h > 0) containerHeight = h;
    const st = scrollContainer.scrollTop;
    scrollTop = st;
    requestAnimationFrame(() => {
      restoreAnchor();
    });
  }

  onMount(() => {
    handleResize();

    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== 'undefined' && scrollContainer) {
      ro = new ResizeObserver(() => {
        handleResize();
      });
      ro.observe(scrollContainer);
    }

    return () => {
      if (ro) ro.disconnect();
    };
  });

  let ticking = false;
  function handleScroll(e: Event) {
    const target = e.target as HTMLElement;
    const st = target.scrollTop;
    if (!ticking) {
      requestAnimationFrame(() => {
        scrollTop = st;
        captureAnchor();
        ticking = false;
      });
      ticking = true;
    }
  }

  function toggleFolder(folderId: number) {
    collapsedFolders[folderId] = !collapsedFolders[folderId];
  }

  function handleCardClick(e: MouseEvent, photo: Photo) {
    const isMulti = e.ctrlKey || e.metaKey || e.shiftKey;
    onPhotoClick(photo, isMulti);
  }

  function handlePhotoDblClick(photo: Photo) {
    const idx = allPhotos.findIndex((p) => p.id === photo.id);
    if (idx !== -1) {
      onPhotoSelected(photo, idx, allPhotos);
    }
  }

  async function handleToggleFav(e: MouseEvent, photo: Photo) {
    e.stopPropagation();
    const newState = await toggleFavorite(photo.id);
    photo.is_favorite = newState;
  }

  function formatDateRange(photos: Photo[]): string {
    if (photos.length === 0) return '';
    const dates = photos
      .map((p) => p.date_taken)
      .filter((d): d is number => d !== null)
      .sort((a, b) => a - b);

    if (dates.length === 0) return '';
    const first = new Date(dates[0]).toLocaleDateString(undefined, {
      month: 'short',
      year: 'numeric',
    });
    const last = new Date(dates[dates.length - 1]).toLocaleDateString(undefined, {
      month: 'short',
      year: 'numeric',
    });

    return first === last ? first : `${first} - ${last}`;
  }
</script>

<svelte:window onresize={handleResize} />

<div
  bind:this={scrollContainer}
  bind:clientWidth={containerWidth}
  bind:clientHeight={containerHeight}
  onscroll={handleScroll}
  class="flex-1 overflow-y-auto p-4 select-none transition-colors relative"
  style="background-color: var(--canvas-bg);"
>
  {#if folderGroups.length === 0}
    <div class="h-96 flex flex-col items-center justify-center text-sm" style="color: var(--text-dim);">
      <ImageIcon class="w-12 h-12 mb-3 opacity-30" />
      <p class="font-medium text-base" style="color: var(--text-muted);">No photos found in watched folders</p>
      <p class="text-xs mt-1" style="color: var(--text-dim);">
        Use <span class="text-blue-500 font-medium">Folder Manager</span> or the <span class="text-blue-500 font-medium">Import</span> button to add photo directories.
      </p>
    </div>
  {:else}
    {#each folderLayouts as layout (layout.folder.id)}
      <div
        id="folder-{layout.folder.id}"
        class="relative scroll-mt-4"
        style="
          height: {layout.totalHeight - FOLDER_GAP}px;
          margin-bottom: {FOLDER_GAP}px;
        "
      >
        <!-- Picasa Classic Folder Header Banner -->
        <div
          class="flex items-center justify-between py-1.5 px-3 border rounded shadow-xs select-none transition-colors"
          style="
            height: {HEADER_HEIGHT}px;
            background-color: var(--folder-banner-bg);
            border-color: var(--border-subtle);
          "
        >
          <button
            type="button"
            onclick={() => toggleFolder(layout.folder.id)}
            class="flex items-center gap-2.5 text-left transition-colors hover:opacity-80"
          >
            {#if layout.isCollapsed}
              <ChevronRight class="w-4 h-4 shrink-0" style="color: var(--text-muted);" />
            {:else}
              <ChevronDown class="w-4 h-4 shrink-0" style="color: var(--text-muted);" />
            {/if}

            <FolderIcon class="w-4 h-4 text-amber-500 shrink-0 fill-current" />

            <div class="flex items-baseline gap-2">
              <span class="text-sm font-bold tracking-wide" style="color: var(--text-main);">
                {layout.folder.display_name}
              </span>
              {#if formatDateRange(layout.photos)}
                <span class="text-xs" style="color: var(--text-dim);">
                  • {formatDateRange(layout.photos)}
                </span>
              {/if}
              <span class="text-xs font-medium" style="color: var(--text-dim);">
                ({layout.photos.length} {layout.photos.length === 1 ? 'photo' : 'photos'})
              </span>
            </div>
          </button>

          <!-- Play Slideshow button -->
          <button
            onclick={() => onPlayFolder(layout.photos)}
            class="flex items-center gap-1.5 px-2.5 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold transition-colors shadow-xs"
            title="Play Folder Slideshow"
          >
            <Play class="w-3 h-3 fill-current" />
            <span>Play</span>
          </button>
        </div>

        <!-- Virtualized Photos Container with Invariant Height -->
        {#if !layout.isCollapsed && layout.photosHeight > 0}
          <div
            class="relative w-full"
            style="
              margin-top: {HEADER_GAP}px;
              height: {layout.photosHeight}px;
            "
          >
            {#if visibleFolderSlices[layout.folder.id]?.photos.length > 0}
              {@const slice = visibleFolderSlices[layout.folder.id]}
              <div
                class="grid gap-3 absolute inset-x-0"
                style="
                  top: {slice.gridTopOffset}px;
                  grid-template-columns: repeat({columns}, minmax(0, 1fr));
                "
              >
                {#each slice.photos as photo (photo.id)}
                  {@const isSelected = selectedIds.has(photo.id)}
                  <div
                    role="button"
                    tabindex="0"
                    class="group relative aspect-square rounded-sm overflow-hidden p-1 transition-all cursor-pointer select-none {isSelected ? 'ring-3 ring-blue-500 shadow-md' : 'hover:ring-1 hover:ring-blue-400/50'}"
                    style="
                      background-color: var(--card-bg);
                      border: 1px solid {isSelected ? 'var(--accent-picasa)' : 'var(--card-border)'};
                      box-shadow: var(--photo-shadow);
                    "
                    onclick={(e) => handleCardClick(e, photo)}
                    ondblclick={() => handlePhotoDblClick(photo)}
                    onkeydown={(e) => {
                      if (e.key === 'Enter') handlePhotoDblClick(photo);
                      if (e.key === ' ') handleCardClick(e as any, photo);
                    }}
                    title="{photo.file_name} - Double-click to open"
                  >
                    <!-- Thumbnail Image inside paper frame -->
                    <div class="w-full h-full overflow-hidden bg-neutral-900 rounded-xs flex items-center justify-center">
                      <img
                        src={getThumbnailUrl(photo)}
                        alt={photo.file_name}
                        loading="lazy"
                        class="w-full h-full object-cover group-hover:scale-103 transition-transform duration-150"
                      />
                    </div>

                    <!-- Top Right Favorite Star (Picasa style) -->
                    <button
                      onclick={(e) => handleToggleFav(e, photo)}
                      class="absolute top-2 right-2 p-1 rounded-full backdrop-blur-xs transition-opacity {
                        photo.is_favorite
                          ? 'bg-black/60 text-amber-400 opacity-100'
                          : 'bg-black/40 text-white/80 opacity-0 group-hover:opacity-100 hover:text-amber-300'
                      }"
                      title="Favorite (★)"
                    >
                      <Star class="w-3.5 h-3.5 {photo.is_favorite ? 'fill-amber-400' : ''}" />
                    </button>

                    <!-- Selected Checkmark Badge (Bottom-Right) -->
                    {#if isSelected}
                      <div class="absolute bottom-2 right-2 bg-blue-600 text-white rounded-full p-0.5 shadow-sm">
                        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="3" d="M5 13l4 4L19 7" />
                        </svg>
                      </div>
                    {/if}

                    <!-- Bottom Filename Gradient Overlay on hover -->
                    <div
                      class="absolute inset-x-1 bottom-1 bg-gradient-to-t from-black/85 via-black/50 to-transparent p-1.5 pt-4 text-[10px] text-white truncate opacity-0 group-hover:opacity-100 transition-opacity"
                    >
                      {photo.file_name}
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>
