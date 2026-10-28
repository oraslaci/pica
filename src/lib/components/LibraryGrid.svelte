<script lang="ts">
  import { onMount } from 'svelte';
  import {
    Folder as FolderIcon,
    ChevronDown,
    ChevronRight,
    Play,
    Star,
    Image as ImageIcon,
    RotateCcw,
    RotateCw,
    Check,
    CheckSquare,
    Loader2,
  } from '@lucide/svelte';
  import type { FolderWithPhotos, Photo, FaceBox } from '../types';
  import { getThumbnailUrl, setThumbnailPriority, toggleFavorite, getFacesForPhoto } from '../api';
  import PhotoThumbnail from './PhotoThumbnail.svelte';
  import { showFaceDebugOverlay } from '../debugStore';

  let {
    folderGroups = [],
    zoomSize = 180,
    selectedPhotos = [] as Photo[],
    onPhotoSelected = (_photo: Photo, _index: number, _allPhotos: Photo[]) => {},
    onPhotoClick = (_photo: Photo, _isMulti: boolean) => {},
    onPlayFolder = (_photos: Photo[]) => {},
    onSelectFolderPhotos = (_photos: Photo[]) => {},
  }: {
    folderGroups: FolderWithPhotos[];
    zoomSize: number;
    selectedPhotos?: Photo[];
    onPhotoSelected?: (photo: Photo, index: number, allPhotos: Photo[]) => void;
    onPhotoClick?: (photo: Photo, isMulti: boolean) => void;
    onPlayFolder?: (photos: Photo[]) => void;
    onSelectFolderPhotos?: (photos: Photo[]) => void;
  } = $props();

  let collapsedFolders = $state<Record<number, boolean>>({});
  let photoRotations = $state<Record<number, number>>({});
  let photoFacesCache = $state<Record<number, FaceBox[]>>({});
  let requestedFaceIds = new Set<number>();

  async function loadFacesForVisiblePhotos(photosToLoad: Photo[]) {
    if (!$showFaceDebugOverlay) return;
    for (const p of photosToLoad) {
      if (!requestedFaceIds.has(p.id)) {
        requestedFaceIds.add(p.id);
        getFacesForPhoto(p.id).then((faces) => {
          if (faces && faces.length > 0) {
            photoFacesCache = { ...photoFacesCache, [p.id]: faces };
          }
        }).catch(() => {});
      }
    }
  }

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
    // Bounded buffer: 2-3 rows above and below (250px - 500px max) to prevent DOM bloat on zoom out
    const bufferPx = Math.min(500, Math.max(250, rowHeight * 2.5));
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

    // When face debug overlay is enabled, fetch faces for rendered photos
    if ($showFaceDebugOverlay) {
      const allRendered = Object.values(slices).flatMap((s) => s.photos);
      loadFacesForVisiblePhotos(allRendered);
    }

    return slices;
  });

  // Scroll direction tracking
  let lastScrollTop = $state(0);
  let scrollDirection = $state<'down' | 'up'>('down');

  // Calculate photos within an arbitrary vertical range [rangeTop, rangeBottom] across all folders
  function getPhotosInRange(rangeTop: number, rangeBottom: number): Photo[] {
    if (rangeBottom <= rangeTop) return [];
    const result: Photo[] = [];
    for (const layout of folderLayouts) {
      if (layout.isCollapsed || layout.photos.length === 0) continue;
      const pTop = layout.photosTop;
      const pBottom = pTop + layout.photosHeight;

      if (pBottom <= rangeTop || pTop >= rangeBottom) continue;

      const overlapTop = Math.max(pTop, rangeTop);
      const overlapBottom = Math.min(pBottom, rangeBottom);

      const startRow = Math.max(
        0,
        Math.min(layout.numRows - 1, Math.floor((overlapTop - pTop) / rowHeight))
      );
      const endRow = Math.max(
        startRow + 1,
        Math.min(layout.numRows, Math.ceil((overlapBottom - pTop) / rowHeight))
      );

      const startIndex = startRow * columns;
      const endIndex = Math.min(layout.photos.length, endRow * columns);
      result.push(...layout.photos.slice(startIndex, endIndex));
    }
    return result;
  }

  // Compute on-screen hashes and previous/next 0.x screen lookaround hashes
  const viewPriority = $derived.by(() => {
    // If the user is actively scrubbing across thousands of photos,
    // do not waste CPU cycles calculating lookaround batches for intermediate positions!
    if (isDraggingScrubber) {
      return { screenHashes: [], nearHashes: [] };
    }

    // 1. Photos currently visible on the screen
    const onScreen = getPhotosInRange(scrollTop, scrollTop + containerHeight);
    const screenHashes: string[] = [];
    const seen = new Set<string>();

    for (const p of onScreen) {
      if (!seen.has(p.file_hash)) {
        seen.add(p.file_hash);
        screenHashes.push(p.file_hash);
      }
    }

    // 2. Previous and next 0.7 part of the screen (lookaround)
    const lookaroundTop = Math.max(0, scrollTop - containerHeight * 0.7);
    const lookaroundBottom = scrollTop + containerHeight * 1.7;
    const nearPhotos = getPhotosInRange(lookaroundTop, lookaroundBottom);
    const nearHashes: string[] = [];

    for (const p of nearPhotos) {
      if (!seen.has(p.file_hash)) {
        seen.add(p.file_hash);
        nearHashes.push(p.file_hash);
      }
    }

    return { screenHashes, nearHashes };
  });

  // Custom Picasa Fast-Scroll Scrubber state & geometry
  let isDraggingScrubber = $state(false);
  let isScrubberHovered = $state(false);
  let grabOffsetY = 0;
  let dragThumbTop = $state<number | null>(null);
  let pendingScrollTop = 0;
  let trackElement: HTMLElement | null = null;
  let thumbElement: HTMLElement | null = null;
  let badgeElement = $state<HTMLElement | null>(null);
  let gridUpdateTimer: ReturnType<typeof setTimeout> | null = null;
  let lastGridUpdate = 0;

  const totalContentHeight = $derived(
    folderLayouts.reduce((sum, l) => sum + l.totalHeight, 0) + 32
  );
  const maxScroll = $derived(Math.max(1, totalContentHeight - containerHeight));
  const trackHeight = $derived(Math.max(100, containerHeight));
  const thumbHeight = $derived(
    Math.max(
      56,
      Math.min(
        trackHeight * 0.4,
        (containerHeight / Math.max(containerHeight, totalContentHeight)) * trackHeight
      )
    )
  );
  const maxThumbTop = $derived(Math.max(1, trackHeight - thumbHeight));
  const thumbTop = $derived(
    Math.max(0, Math.min(maxThumbTop, (scrollTop / maxScroll) * maxThumbTop))
  );
  const currentThumbTop = $derived(dragThumbTop !== null ? dragThumbTop : thumbTop);

  function getDateBadge(top: number): string {
    if (folderLayouts.length === 0) return '';
    for (const layout of folderLayouts) {
      if (top >= layout.headerTop && top < layout.top + layout.totalHeight) {
        if (top < layout.photosTop || layout.photos.length === 0) {
          return layout.folder.display_name;
        }
        const row = Math.floor((top - layout.photosTop) / rowHeight);
        const photoIdx = Math.min(layout.photos.length - 1, Math.max(0, row * columns));
        const photo = layout.photos[photoIdx];
        if (photo?.date_taken) {
          const d = new Date(photo.date_taken);
          const monthYear = d.toLocaleDateString(undefined, { month: 'short', year: 'numeric' });
          return `${layout.folder.display_name} • ${monthYear}`;
        }
        return layout.folder.display_name;
      }
    }
    return folderLayouts[0]?.folder.display_name || '';
  }

  function getPhotoAtScroll(top: number): { photo: Photo; index: number; total: number } | null {
    if (folderLayouts.length === 0) return null;
    let globalIndex = 0;
    const totalPhotos = folderLayouts.reduce((sum, l) => sum + l.photos.length, 0);

    for (const layout of folderLayouts) {
      if (top >= layout.headerTop && top < layout.top + layout.totalHeight) {
        if (top < layout.photosTop || layout.photos.length === 0) {
          const first = layout.photos[0];
          return first ? { photo: first, index: globalIndex + 1, total: totalPhotos } : null;
        }
        const row = Math.floor((top - layout.photosTop) / rowHeight);
        const photoIdx = Math.min(layout.photos.length - 1, Math.max(0, row * columns));
        const photo = layout.photos[photoIdx];
        return photo ? { photo, index: globalIndex + photoIdx + 1, total: totalPhotos } : null;
      }
      globalIndex += layout.photos.length;
    }
    const firstLayout = folderLayouts[0];
    if (firstLayout && firstLayout.photos.length > 0) {
      return { photo: firstLayout.photos[0], index: 1, total: totalPhotos };
    }
    return null;
  }

  let currentScrubPhoto = $state<{ photo: Photo; index: number; total: number } | null>(null);
  const hoverScrubPhoto = $derived(getPhotoAtScroll(scrollTop));
  const activeScrub = $derived(isDraggingScrubber ? currentScrubPhoto : (isScrubberHovered ? hoverScrubPhoto : null));

  const currentDateBadge = $derived.by(() => {
    const top = isDraggingScrubber ? pendingScrollTop : scrollTop;
    return getDateBadge(top);
  });

  function scheduleGridUpdate() {
    if (gridUpdateTimer) clearTimeout(gridUpdateTimer);
    // Debounce virtual grid DOM mounting so dragging is buttery smooth at 120fps with zero mouse lag
    gridUpdateTimer = setTimeout(() => {
      gridUpdateTimer = null;
      if (scrollContainer && isDraggingScrubber) {
        scrollContainer.scrollTop = pendingScrollTop;
        scrollTop = pendingScrollTop;
      }
    }, 100);
  }

  function handleThumbPointerDown(e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!thumbElement || !trackElement) return;
    const thumbRect = thumbElement.getBoundingClientRect();
    // Record where inside the thumb the user clicked
    grabOffsetY = e.clientY - thumbRect.top;
    isDraggingScrubber = true;
    dragThumbTop = thumbTop;
    pendingScrollTop = scrollTop;
    currentScrubPhoto = getPhotoAtScroll(scrollTop);
  }

  function handleWindowPointerMove(e: PointerEvent) {
    if (!isDraggingScrubber || !trackElement) return;
    const trackRect = trackElement.getBoundingClientRect();
    // Direct mathematical cursor tracking within the track
    const targetThumbTop = e.clientY - trackRect.top - grabOffsetY;
    const clampedThumbTop = Math.max(0, Math.min(maxThumbTop, targetThumbTop));

    // 1. Direct synchronous DOM update (0ms latency, zero Svelte scheduling lag)
    if (thumbElement) {
      thumbElement.style.top = `${clampedThumbTop}px`;
    }
    dragThumbTop = clampedThumbTop;

    // 2. Compute pending scroll position
    const targetPct = maxThumbTop > 0 ? clampedThumbTop / maxThumbTop : 0;
    pendingScrollTop = targetPct * maxScroll;

    // 3. Update preview photo and floating date badge text directly
    currentScrubPhoto = getPhotoAtScroll(pendingScrollTop);
    if (badgeElement) {
      badgeElement.textContent = getDateBadge(pendingScrollTop);
    }

    // 4. Throttle heavy grid DOM virtualization
    scheduleGridUpdate();
  }

  function handleWindowPointerUp(e: PointerEvent) {
    if (!isDraggingScrubber) return;
    if (gridUpdateTimer) {
      clearTimeout(gridUpdateTimer);
      gridUpdateTimer = null;
    }
    if (scrollContainer) {
      scrollContainer.scrollTop = pendingScrollTop;
      scrollTop = pendingScrollTop;
    }
    dragThumbTop = null;
    currentScrubPhoto = null;
    isDraggingScrubber = false;
  }

  function handleTrackPointerDown(e: PointerEvent) {
    if (e.target !== e.currentTarget || !scrollContainer || !trackElement) return;
    e.preventDefault();
    const rect = trackElement.getBoundingClientRect();
    const clickY = e.clientY - rect.top;
    const targetThumbTop = Math.max(0, Math.min(maxThumbTop, clickY - thumbHeight / 2));
    grabOffsetY = thumbHeight / 2;
    isDraggingScrubber = true;
    dragThumbTop = targetThumbTop;
    const targetPct = maxThumbTop > 0 ? targetThumbTop / maxThumbTop : 0;
    pendingScrollTop = targetPct * maxScroll;
    currentScrubPhoto = getPhotoAtScroll(pendingScrollTop);
    if (thumbElement) {
      thumbElement.style.top = `${targetThumbTop}px`;
    }
    scheduleGridUpdate();
  }

  // Notify backend of current screen and near-screen lookaround
  let priorityDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    const { screenHashes, nearHashes } = viewPriority;
    if (screenHashes.length === 0 && nearHashes.length === 0) return;

    if (priorityDebounceTimer) clearTimeout(priorityDebounceTimer);
    // When actively dragging scrubber, debounce by 250ms so UI stays 120fps smooth
    // When normal scrolling, debounce by 80ms
    const delay = isDraggingScrubber ? 250 : 80;
    priorityDebounceTimer = setTimeout(() => {
      setThumbnailPriority(screenHashes, nearHashes);
    }, delay);

    return () => {
      if (priorityDebounceTimer) clearTimeout(priorityDebounceTimer);
    };
  });

  // Scroll anchoring across window resizes & zoom size changes  // Zoom and resize anchor preservation
  let anchorFolderId = $state<number | null>(null);
  let anchorPhotoIndex = 0;
  let isHeaderAnchor = true;
  let prevColumns = 0;

  function captureAnchor() {
    if (!scrollContainer) return;
    const currentScroll = scrollContainer.scrollTop;
    if (currentScroll <= 10) {
      anchorFolderId = folderLayouts[0]?.folder.id ?? null;
      anchorPhotoIndex = 0;
      isHeaderAnchor = true;
      return;
    }
    for (const layout of folderLayouts) {
      if (layout.isCollapsed || layout.photos.length === 0) continue;
      if (currentScroll >= layout.headerTop && currentScroll < layout.top + layout.totalHeight) {
        anchorFolderId = layout.folder.id;
        if (currentScroll < layout.photosTop) {
          anchorPhotoIndex = 0;
          isHeaderAnchor = true;
        } else {
          isHeaderAnchor = false;
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

    let newTop: number;
    if (isHeaderAnchor) {
      newTop = targetLayout.headerTop;
    } else {
      const targetRow = Math.floor(anchorPhotoIndex / columns);
      newTop = targetLayout.photosTop + targetRow * rowHeight;
    }
    const maxScroll = Math.max(0, scrollContainer.scrollHeight - scrollContainer.clientHeight);
    const clampedTop = Math.max(0, Math.min(newTop, maxScroll));

    scrollContainer.scrollTo({ top: clampedTop, behavior: 'instant' });
    scrollTop = clampedTop;
  }

  // Effect to re-anchor when column count changes
  $effect(() => {
    if (columns !== prevColumns) {
      captureAnchor();
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
    if (isDraggingScrubber) return;
    const target = e.target as HTMLElement;
    const st = target.scrollTop;
    if (st > lastScrollTop + 2) {
      scrollDirection = 'down';
    } else if (st < lastScrollTop - 2) {
      scrollDirection = 'up';
    }
    lastScrollTop = st;

    if (!ticking) {
      requestAnimationFrame(() => {
        scrollTop = st;
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

  function handleRotateCard(e: MouseEvent, photo: Photo, deg: number) {
    e.stopPropagation();
    photoRotations[photo.id] = ((photoRotations[photo.id] || 0) + deg + 360) % 360;
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

<svelte:window
  onresize={handleResize}
  onpointermove={handleWindowPointerMove}
  onpointerup={handleWindowPointerUp}
  onpointercancel={handleWindowPointerUp}
/>

<div class="relative flex-1 w-full h-full overflow-hidden flex flex-row">
  <div
    bind:this={scrollContainer}
    bind:clientWidth={containerWidth}
    bind:clientHeight={containerHeight}
    onscroll={handleScroll}
    id="library-grid-container"
    class="flex-1 overflow-y-auto p-4 select-none transition-colors relative library-grid-scroller"
    style="background-color: var(--canvas-bg);"
  >
  {#if isDraggingScrubber}
    <!-- Floating Scrubbing Status Pill on the Left Side / Top Center of the Grid -->
    <div class="sticky top-2 inset-x-0 z-40 flex items-center justify-center pointer-events-none mb-3">
      <div class="flex items-center gap-2 px-3.5 py-1.5 rounded-full bg-neutral-900/90 text-white text-xs font-semibold shadow-2xl border border-blue-500/40 backdrop-blur-md animate-in fade-in slide-in-from-top-2 duration-150">
        <Loader2 class="w-3.5 h-3.5 animate-spin text-blue-400" />
        <span class="text-neutral-200">Scrubbing to <strong class="text-amber-400">{currentDateBadge}</strong></span>
      </div>
    </div>
  {/if}

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

          <!-- Folder Header Actions: Select All & Play -->
          <div class="flex items-center gap-2">
            <button
              onclick={() => onSelectFolderPhotos(layout.photos)}
              class="flex items-center gap-1.5 px-2.5 py-1 rounded bg-black/10 dark:bg-white/10 hover:bg-black/20 dark:hover:bg-white/20 text-xs font-medium transition-colors"
              style="color: var(--text-main);"
              title="Select all photos in this folder"
            >
              <CheckSquare class="w-3.5 h-3.5 text-blue-400" />
              <span class="hidden sm:inline">Select All</span>
            </button>

            <button
              onclick={() => onPlayFolder(layout.photos)}
              class="flex items-center gap-1.5 px-2.5 py-1 rounded bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold transition-colors shadow-xs"
              title="Play Folder Slideshow"
            >
              <Play class="w-3 h-3 fill-current" />
              <span>Play</span>
            </button>
          </div>
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
                    class="group relative aspect-square rounded-sm overflow-hidden p-1 transition-[transform,box-shadow] duration-150 cursor-pointer select-none {isSelected ? 'ring-3 ring-blue-500 shadow-md -translate-y-0.5' : 'hover:ring-1 hover:ring-blue-400/50 hover:-translate-y-0.5 hover:shadow-lg'}"
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
                    <!-- Thumbnail Image with rotation container -->
                    <div
                      class="w-full h-full overflow-hidden flex items-center justify-center transition-transform duration-200 relative"
                      style="transform: rotate({photoRotations[photo.id] || 0}deg);"
                    >
                      <PhotoThumbnail
                        {photo}
                        class="w-full h-full object-cover group-hover:scale-103 transition-transform duration-150"
                      />

                      <!-- Debug Face Detection & Person Labels Overlay -->
                      {#if $showFaceDebugOverlay && photoFacesCache[photo.id] && photoFacesCache[photo.id].length > 0}
                        <div class="absolute inset-0 pointer-events-none z-15">
                          {#each photoFacesCache[photo.id] as face (face.id)}
                            <div
                              class="absolute border border-purple-400 bg-purple-500/20 shadow-[0_0_0_1px_rgba(0,0,0,0.5)] rounded-xs"
                              style="
                                left: {face.box_x * 100}%;
                                top: {face.box_y * 100}%;
                                width: {face.box_w * 100}%;
                                height: {face.box_h * 100}%;
                              "
                            >
                              <div
                                class="absolute -top-4 left-1/2 -translate-x-1/2 px-1 py-0.2 rounded-xs bg-black/85 text-[9px] font-bold text-purple-200 whitespace-nowrap shadow border border-purple-400/40"
                              >
                                {face.person_name || 'Person'} ({Math.round(face.confidence * 100)}%)
                              </div>
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </div>

                    <!-- Top-Left Status Indicator & Select Checkbox Button (Picasa style) -->
                    <div class="absolute top-2 left-2 z-10 flex items-center gap-1.5 pointer-events-none">
                      <button
                        type="button"
                        onclick={(e) => {
                          e.stopPropagation();
                          onPhotoClick(photo, true);
                        }}
                        class="p-1 rounded backdrop-blur-xs transition-opacity pointer-events-auto {
                          isSelected
                            ? 'bg-blue-600 text-white opacity-100'
                            : 'bg-black/50 text-white/80 opacity-0 group-hover:opacity-100 hover:bg-blue-600 hover:text-white'
                        }"
                        title={isSelected ? 'Deselect photo' : 'Select photo'}
                      >
                        <Check class="w-3.5 h-3.5" />
                      </button>

                      {#if photo.has_unsaved_edits}
                        <div
                          class="flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-amber-500/90 text-white text-[9px] font-bold tracking-tight shadow-md backdrop-blur-xs border border-amber-400/50 animate-in fade-in zoom-in-95 duration-150"
                          title="Unsaved edits present. Changes are preserved in non-destructive session."
                        >
                          <span class="w-1.5 h-1.5 rounded-full bg-white animate-pulse"></span>
                          <span>Edited</span>
                        </div>
                      {/if}
                    </div>

                    <!-- Top Right Favorite Star (Picasa style) -->
                    <button
                      onclick={(e) => handleToggleFav(e, photo)}
                      class="absolute top-2 right-2 p-1 rounded-full backdrop-blur-xs transition-opacity z-10 {
                        photo.is_favorite
                          ? 'bg-black/60 text-amber-400 opacity-100'
                          : 'bg-black/50 text-white/80 opacity-0 group-hover:opacity-100 hover:text-amber-300'
                      }"
                      title="Favorite (★)"
                    >
                      <Star class="w-3.5 h-3.5 {photo.is_favorite ? 'fill-amber-400' : ''}" />
                    </button>

                    {#if import.meta.env.DEV}
                      <!-- Bottom-Left Rotate CCW Button on hover (Picasa classic) -->
                      <button
                        onclick={(e) => handleRotateCard(e, photo, -90)}
                        class="absolute bottom-2 left-2 p-1 rounded-full bg-black/60 text-white/90 hover:text-white hover:bg-black/80 backdrop-blur-xs opacity-0 group-hover:opacity-100 transition-opacity z-10"
                        title="Rotate Counter-Clockwise (-90°)"
                      >
                        <RotateCcw class="w-3.5 h-3.5" />
                      </button>

                      <!-- Bottom-Right Rotate CW Button on hover (Picasa classic) -->
                      <button
                        onclick={(e) => handleRotateCard(e, photo, 90)}
                        class="absolute bottom-2 right-2 p-1 rounded-full bg-black/60 text-white/90 hover:text-white hover:bg-black/80 backdrop-blur-xs opacity-0 group-hover:opacity-100 transition-opacity z-10"
                        title="Rotate Clockwise (+90°)"
                      >
                        <RotateCw class="w-3.5 h-3.5" />
                      </button>
                    {/if}

                    <!-- Bottom Filename Gradient Overlay on hover -->
                    <div
                      class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/85 via-black/50 to-transparent p-1 pt-4 text-[10px] text-white text-center truncate opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none px-7"
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

  <!-- Custom Picasa Fast-Scroll Scrubber Bar (20px wide, zero-lag pointer tracking) -->
  <div
    bind:this={trackElement}
    role="presentation"
    class="relative w-5 h-full shrink-0 flex items-center justify-center select-none cursor-pointer transition-colors"
    style="
      background-color: var(--scrollbar-track);
      border-left: 1px solid var(--border-subtle);
    "
    onpointerdown={handleTrackPointerDown}
    onmouseenter={() => (isScrubberHovered = true)}
    onmouseleave={() => (isScrubberHovered = false)}
  >
    <!-- Scrubber Thumb -->
    <div
      bind:this={thumbElement}
      role="scrollbar"
      aria-controls="library-grid-container"
      aria-label="Photo library scrollbar"
      aria-valuenow={Math.round(scrollTop)}
      aria-valuemin={0}
      aria-valuemax={Math.round(maxScroll)}
      tabindex="0"
      onpointerdown={handleThumbPointerDown}
      class="absolute w-3.5 rounded-full cursor-grab active:cursor-grabbing transition-colors duration-100 flex items-center justify-center {
        isDraggingScrubber
          ? 'bg-blue-500 shadow-md ring-2 ring-blue-400/50'
          : 'bg-neutral-500/80 hover:bg-neutral-400'
      }"
      style="
        top: {currentThumbTop}px;
        height: {thumbHeight}px;
        touch-action: none;
      "
    >
      <!-- Center grip ridges (classic Picasa aesthetic) -->
      <div class="flex flex-col items-center justify-center gap-1 opacity-60 pointer-events-none">
        <span class="w-1.5 h-0.5 bg-white rounded-full"></span>
        <span class="w-1.5 h-0.5 bg-white rounded-full"></span>
        <span class="w-1.5 h-0.5 bg-white rounded-full"></span>
      </div>

      <!-- Floating Date / Folder Badge with Sliding Thumbnail Preview (Picasa style, visible while hovering or dragging) -->
      {#if isDraggingScrubber || isScrubberHovered}
        <div
          class="absolute right-8 top-1/2 -translate-y-1/2 flex flex-col items-center gap-2 p-2 rounded-xl bg-neutral-900/95 text-white shadow-2xl border border-white/20 backdrop-blur-md pointer-events-none select-none z-50 animate-in fade-in zoom-in-95 duration-100 min-w-32"
        >
          {#if activeScrub?.photo}
            <!-- Sliding Thumbnail Preview (80x80) -->
            <div class="relative w-20 h-20 rounded-lg overflow-hidden bg-neutral-800 border border-white/10 shadow-inner flex items-center justify-center shrink-0">
              <div class="absolute inset-0 bg-neutral-800 flex items-center justify-center">
                <Loader2 class="w-4 h-4 text-neutral-400 animate-spin" />
              </div>
              <img
                src={getThumbnailUrl(activeScrub.photo)}
                alt={activeScrub.photo.file_name}
                class="relative w-full h-full object-cover transition-opacity duration-150"
                onload={(e) => {
                  (e.currentTarget as HTMLElement).style.opacity = '1';
                }}
                style="opacity: 0;"
              />
            </div>
          {/if}

          <div class="flex flex-col items-center text-center px-1">
            <span bind:this={badgeElement} class="text-amber-400 font-bold text-xs whitespace-nowrap">
              {currentDateBadge}
            </span>
            {#if activeScrub}
              <span class="text-[10px] text-neutral-400 font-medium whitespace-nowrap mt-0.5">
                Photo {activeScrub.index.toLocaleString()} of {activeScrub.total.toLocaleString()}
              </span>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
</div>
