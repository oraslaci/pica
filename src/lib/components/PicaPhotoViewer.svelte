<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    ChevronLeft,
    ChevronRight,
    X,
    RotateCcw,
    RotateCw,
    Maximize2,
    Star,
    Info,
    Play,
    Pause,
    Minus,
    Plus,
    Film,
    Expand,
    Shrink,
    BarChart2,
    Sparkles,
  } from '@lucide/svelte';
  import type { EditSessionState, Photo, PhotoMetadata, ViewerFolderContext } from '../types';
  import {
    getViewerFolderContext,
    getViewerPhotoMetadata,
    getFullImageUrl,
    getPreviewImageUrl,
    getThumbnailUrl,
    getEditSession,
    rotateViewerPhoto,
    toggleFavorite,
    openInPicaEditor,
    isTauri,
  } from '../api';
  import Histogram from './Histogram.svelte';
  import PropertiesSidebar from './PropertiesSidebar.svelte';

  // --- Props & Route Inputs ---
  interface Props {
    initialFilePath?: string;
  }
  const { initialFilePath }: Props = $props();

  // --- Core Viewer State ---
  let folderContext = $state<ViewerFolderContext | null>(null);
  let currentIndex = $state(0);
  let isLoading = $state(true);
  let errorMessage = $state<string | null>(null);
  let metadata = $state<PhotoMetadata | null>(null);
  let sessionState = $state<EditSessionState | null>(null);
  let cacheBuster = $state<number>(Date.now());

  // --- Drawers & Overlays ---
  let isInfoOpen = $state(false);
  let showHistogram = $state(false);
  let isFilmstripOpen = $state(true);
  let isSlideshowPlaying = $state(false);
  let slideshowTimer = $state<ReturnType<typeof setInterval> | null>(null);
  let toastMsg = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  // --- Editor-Grade Pan, Zoom & Rotation State ---
  let zoomLevel = $state(1.0); // 1.0 = Fit to Viewport, > 1.0 = Zoomed
  let panX = $state(0);
  let panY = $state(0);
  let isPanning = $state(false);
  let isWheelZooming = $state(false);
  let wheelZoomTimeout: ReturnType<typeof setTimeout> | null = null;
  let panStartPointer = { x: 0, y: 0 };
  let panStartOffset = { x: 0, y: 0 };

  // Monotonic rotation angle for 380ms cubic-bezier spring glide (matches ImageViewer.svelte)
  let visualRotation = $state(0);
  let isTransitioning = $state(false);
  let rotationTimeout: ReturnType<typeof setTimeout> | null = null;

  // --- Window / Fullscreen State ---
  let isFullscreen = $state(false);

  // --- DOM References ---
  let imageContainerRef = $state<HTMLDivElement | null>(null);
  let mainImgRef = $state<HTMLImageElement | null>(null);
  let filmstripEl = $state<HTMLDivElement | null>(null);

  // Derived current photo
  const currentPhoto = $derived(
    folderContext && folderContext.photos.length > 0
      ? folderContext.photos[currentIndex] || folderContext.photos[0]
      : null
  );

  // Active image source (respects non-destructive Pica edit sessions!)
  const activeImageSrc = $derived.by(() => {
    if (!currentPhoto) return '';
    if (
      sessionState &&
      sessionState.has_session &&
      sessionState.current_step > 0 &&
      sessionState.preview_path
    ) {
      return getPreviewImageUrl(sessionState.preview_path, cacheBuster);
    }
    return getFullImageUrl(currentPhoto, cacheBuster);
  });

  // Quick EXIF summary pill for top-left floating badge
  const quickExifSummary = $derived.by(() => {
    if (!metadata) return '';
    const parts: string[] = [];
    if (metadata.focal_length) parts.push(metadata.focal_length);
    if (metadata.f_number) parts.push(metadata.f_number);
    if (metadata.exposure_time) parts.push(metadata.exposure_time);
    if (metadata.iso) parts.push(`ISO ${metadata.iso}`);
    return parts.join(' · ');
  });

  function showToast(msg: string) {
    toastMsg = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastMsg = null;
    }, 2200);
  }

  function triggerAnimatedRotation(fn: () => void) {
    isTransitioning = true;
    fn();
    if (rotationTimeout) clearTimeout(rotationTimeout);
    rotationTimeout = setTimeout(() => {
      isTransitioning = false;
    }, 400);
  }

  async function loadPath(filePath: string) {
    isLoading = true;
    errorMessage = null;
    try {
      const ctx = await getViewerFolderContext(filePath);
      folderContext = ctx;
      currentIndex =
        ctx.current_index >= 0 && ctx.current_index < ctx.photos.length
          ? ctx.current_index
          : 0;
      resetViewForNewPhoto();
      await loadCurrentPhotoDetails();
      scrollActiveThumbnailIntoView();
    } catch (err: any) {
      console.error('Failed to load viewer folder context:', err);
      errorMessage = err?.message || 'Failed to open image';
    } finally {
      isLoading = false;
    }
  }

  async function loadCurrentPhotoDetails() {
    if (!currentPhoto) {
      metadata = null;
      sessionState = null;
      return;
    }
    const targetPath = currentPhoto.file_path;
    const targetId = currentPhoto.id;

    try {
      const [meta, sess] = await Promise.all([
        getViewerPhotoMetadata(targetPath).catch(() => null),
        targetId > 0 ? getEditSession(targetId).catch(() => null) : Promise.resolve(null),
      ]);
      if (currentPhoto && currentPhoto.file_path === targetPath) {
        metadata = meta;
        sessionState = sess;
      }
    } catch (e) {
      console.warn('Failed to load photo details:', e);
    }
  }

  function resetViewForNewPhoto() {
    isTransitioning = false;
    visualRotation = 0;
    zoomLevel = 1.0;
    panX = 0;
    panY = 0;
    isPanning = false;
  }

  function resetZoom() {
    zoomLevel = 1.0;
    panX = 0;
    panY = 0;
    isPanning = false;
  }

  function toggleOneToOne() {
    if (zoomLevel > 1.05) {
      resetZoom();
    } else {
      zoomLevel = 2.0;
      panX = 0;
      panY = 0;
    }
  }

  function setZoomPercent(percent: number) {
    const clamped = Math.max(100, Math.min(800, percent));
    if (clamped <= 102) {
      resetZoom();
    } else {
      zoomLevel = clamped / 100;
    }
  }

  // Navigation
  function goToNext() {
    if (!folderContext || folderContext.photos.length <= 1) return;
    currentIndex = (currentIndex + 1) % folderContext.photos.length;
    resetViewForNewPhoto();
    loadCurrentPhotoDetails();
    scrollActiveThumbnailIntoView();
  }

  function goToPrev() {
    if (!folderContext || folderContext.photos.length <= 1) return;
    currentIndex =
      (currentIndex - 1 + folderContext.photos.length) % folderContext.photos.length;
    resetViewForNewPhoto();
    loadCurrentPhotoDetails();
    scrollActiveThumbnailIntoView();
  }

  function selectPhoto(index: number) {
    if (!folderContext || index < 0 || index >= folderContext.photos.length) return;
    if (index === currentIndex) return;
    currentIndex = index;
    resetViewForNewPhoto();
    loadCurrentPhotoDetails();
    scrollActiveThumbnailIntoView();
  }

  function scrollActiveThumbnailIntoView() {
    setTimeout(() => {
      const el = document.getElementById(`viewer-thumb-${currentIndex}`);
      if (el && filmstripEl) {
        el.scrollIntoView({
          behavior: 'smooth',
          inline: 'center',
          block: 'nearest',
        });
      }
    }, 40);
  }

  // Cursor-Anchored Exponential Wheel Zoom (identical to ImageViewer.svelte)
  function handleWheel(e: WheelEvent) {
    e.preventDefault();

    isWheelZooming = true;
    if (wheelZoomTimeout) clearTimeout(wheelZoomTimeout);
    wheelZoomTimeout = setTimeout(() => {
      isWheelZooming = false;
      wheelZoomTimeout = null;
    }, 140);

    const zoomFactor = Math.exp(-e.deltaY * 0.002);
    const newZoom = Math.max(1.0, Math.min(8.0, zoomLevel * zoomFactor));

    if (newZoom <= 1.005) {
      resetZoom();
      return;
    }

    if (imageContainerRef) {
      const rect = imageContainerRef.getBoundingClientRect();
      const mouseRelX = e.clientX - (rect.left + rect.width / 2);
      const mouseRelY = e.clientY - (rect.top + rect.height / 2);

      const ratio = newZoom / zoomLevel;
      panX = (panX - mouseRelX) * ratio + mouseRelX;
      panY = (panY - mouseRelY) * ratio + mouseRelY;
    }

    zoomLevel = newZoom;
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if (zoomLevel <= 1.02) return;

    isPanning = true;
    panStartPointer = { x: e.clientX, y: e.clientY };
    panStartOffset = { x: panX, y: panY };
    window.addEventListener('pointermove', handlePointerMove);
    window.addEventListener('pointerup', handlePointerUp);
    window.addEventListener('pointercancel', handlePointerUp);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isPanning) return;
    panX = panStartOffset.x + (e.clientX - panStartPointer.x);
    panY = panStartOffset.y + (e.clientY - panStartPointer.y);
  }

  function handlePointerUp() {
    if (!isPanning) return;
    isPanning = false;
    window.removeEventListener('pointermove', handlePointerMove);
    window.removeEventListener('pointerup', handlePointerUp);
    window.removeEventListener('pointercancel', handlePointerUp);
  }

  // Continuous Physical 380ms Spring Rotation
  async function handleRotate(degrees: number) {
    if (!currentPhoto) return;
    const targetPath = currentPhoto.file_path;

    // 1. Instant 0ms physical glide on canvas
    triggerAnimatedRotation(() => {
      visualRotation += degrees;
    });

    // 2. Persist in backend edit session
    try {
      const res = await rotateViewerPhoto(targetPath, degrees);
      if (res.success && res.session_state) {
        sessionState = res.session_state;
        // Keep visualRotation aligned with new rendered proxy
        isTransitioning = false;
        visualRotation = 0;
        cacheBuster = Date.now();
      }
      showToast(`Rotated ${degrees > 0 ? '+90° CW' : '-90° CCW'}`);
    } catch (err) {
      console.error('Failed to rotate photo:', err);
      showToast('Failed to rotate photo');
    }
  }

  async function handleToggleFavorite() {
    if (!currentPhoto) return;
    const nextFav = !currentPhoto.is_favorite;
    currentPhoto.is_favorite = nextFav;
    showToast(nextFav ? 'Added to Starred Photos' : 'Removed Star');
    if (currentPhoto.id > 0) {
      try {
        const updated = await toggleFavorite(currentPhoto.id);
        currentPhoto.is_favorite = updated;
      } catch (err) {
        console.error('Failed to toggle favorite:', err);
      }
    }
  }

  function toggleSlideshow() {
    if (isSlideshowPlaying) {
      if (slideshowTimer) clearInterval(slideshowTimer);
      slideshowTimer = null;
      isSlideshowPlaying = false;
      showToast('Slideshow Paused');
    } else {
      isSlideshowPlaying = true;
      showToast('Slideshow Started (3.5s)');
      slideshowTimer = setInterval(() => {
        goToNext();
      }, 3500);
    }
  }

  async function handleEditInPica() {
    if (!currentPhoto) return;
    try {
      await openInPicaEditor(currentPhoto.file_path);
    } catch (err) {
      console.error('Failed to handoff to Pica editor:', err);
    }
  }

  // Window Controls (Frameless Tauri Window or Browser Fallback)
  async function handleStartWindowDrag(e: MouseEvent) {
    if (e.button !== 0 || zoomLevel > 1.02) return;
    if (isTauri) {
      try {
        const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
        await getCurrentWebviewWindow().startDragging();
      } catch {
        // Ignore if not supported
      }
    }
  }

  async function handleMinimize() {
    if (isTauri) {
      try {
        const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
        await getCurrentWebviewWindow().minimize();
      } catch (e) {
        console.warn('Minimize failed:', e);
      }
    }
  }

  async function handleToggleMaximize() {
    if (isTauri) {
      try {
        const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
        await getCurrentWebviewWindow().toggleMaximize();
      } catch (e) {
        console.warn('Toggle maximize failed:', e);
      }
    }
  }

  async function handleToggleFullscreen() {
    if (isTauri) {
      try {
        const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
        isFullscreen = !isFullscreen;
        await getCurrentWebviewWindow().setFullscreen(isFullscreen);
      } catch (e) {
        console.warn('Fullscreen failed:', e);
      }
    } else {
      if (!document.fullscreenElement) {
        await document.documentElement.requestFullscreen();
        isFullscreen = true;
      } else {
        await document.exitFullscreen();
        isFullscreen = false;
      }
    }
  }

  async function handleClose() {
    if (isTauri) {
      try {
        const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
        await getCurrentWebviewWindow().close();
      } catch {
        window.close();
      }
    } else {
      window.close();
    }
  }

  // Keyboard Shortcuts (Consistent across Editor & Viewer)
  function handleKeyDown(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
      return;
    }

    switch (e.key) {
      case 'ArrowRight':
      case 'j':
      case 'J':
        e.preventDefault();
        goToNext();
        break;
      case 'ArrowLeft':
      case 'k':
      case 'K':
        e.preventDefault();
        goToPrev();
        break;
      case ' ':
        e.preventDefault();
        toggleOneToOne();
        break;
      case 'p':
      case 'P':
        e.preventDefault();
        toggleSlideshow();
        break;
      case '+':
      case '=':
        e.preventDefault();
        setZoomPercent(Math.round(zoomLevel * 100) + 25);
        break;
      case '-':
      case '_':
        e.preventDefault();
        setZoomPercent(Math.round(zoomLevel * 100) - 25);
        break;
      case '1':
        e.preventDefault();
        toggleOneToOne();
        break;
      case '0':
        e.preventDefault();
        resetZoom();
        break;
      case '[':
        e.preventDefault();
        handleRotate(-90);
        break;
      case ']':
        e.preventDefault();
        handleRotate(90);
        break;
      case 'r':
      case 'R':
        e.preventDefault();
        handleRotate(e.shiftKey ? -90 : 90);
        break;
      case 'f':
      case 'F':
      case 's':
      case 'S':
        e.preventDefault();
        handleToggleFavorite();
        break;
      case 'i':
      case 'I':
        e.preventDefault();
        isInfoOpen = !isInfoOpen;
        break;
      case 'h':
      case 'H':
        e.preventDefault();
        showHistogram = !showHistogram;
        break;
      case 't':
      case 'T':
        e.preventDefault();
        isFilmstripOpen = !isFilmstripOpen;
        break;
      case 'Enter':
      case 'e':
      case 'E':
        e.preventDefault();
        handleEditInPica();
        break;
      case 'F11':
        e.preventDefault();
        handleToggleFullscreen();
        break;
      case 'Escape':
        e.preventDefault();
        if (isInfoOpen) {
          isInfoOpen = false;
        } else if (zoomLevel > 1.02) {
          resetZoom();
        } else if (isFullscreen) {
          handleToggleFullscreen();
        } else {
          handleClose();
        }
        break;
    }
  }

  let unlistenLoad: (() => void) | null = null;

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);

    // Ensure native OS titlebar is removed in Tauri viewer window
    if (isTauri) {
      import('@tauri-apps/api/webviewWindow')
        .then(({ getCurrentWebviewWindow }) => {
          getCurrentWebviewWindow().setDecorations(false).catch(() => {});
        })
        .catch(() => {});
    }

    const params = new URLSearchParams(window.location.search);
    const fileParam = initialFilePath || params.get('file') || params.get('path');

    if (params.get('info') === '1' || params.has('info')) {
      isInfoOpen = true;
    }
    if (params.get('hist') === '1' || params.has('hist')) {
      showHistogram = true;
    }

    if (fileParam) {
      loadPath(fileParam);
    } else {
      loadPath('.');
    }

    if (isTauri) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen<string>('load-photo', (event) => {
          if (event.payload) {
            loadPath(event.payload);
          }
        })
          .then((unsub) => {
            unlistenLoad = unsub;
          })
          .catch((e) => {
            console.warn('Failed to listen for load-photo event:', e);
          });
      });
    }
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeyDown);
    window.removeEventListener('pointermove', handlePointerMove);
    window.removeEventListener('pointerup', handlePointerUp);
    window.removeEventListener('pointercancel', handlePointerUp);
    if (slideshowTimer) clearInterval(slideshowTimer);
    if (toastTimer) clearTimeout(toastTimer);
    if (rotationTimeout) clearTimeout(rotationTimeout);
    if (unlistenLoad) unlistenLoad();
  });
</script>

<!-- Frameless Darkroom Shell (Matches ImageViewer.svelte #0e0e11 / #121214) -->
<div
  class="fixed inset-0 bg-[#0e0e11] text-neutral-200 flex flex-row select-none overflow-hidden font-sans ring-1 ring-white/10"
>
  <!-- Main Darkroom Canvas Area -->
  <div class="flex-1 h-full flex flex-col relative overflow-hidden bg-[#0e0e11]">
    <!-- Invisible Top Window Drag Strip for Frameless Window -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute top-0 inset-x-0 h-14 z-10"
      data-tauri-drag-region
      onmousedown={handleStartWindowDrag}
      ondblclick={handleToggleMaximize}
    ></div>

    <!-- Ambient Dynamic Color Glow Backdrop (Sampled from Current Photo Thumbnail for 320x lower GPU cost) -->
    {#if currentPhoto}
      <div class="absolute inset-0 pointer-events-none overflow-hidden z-0">
        <img
          src={getThumbnailUrl(currentPhoto)}
          alt=""
          aria-hidden="true"
          decoding="async"
          class="w-full h-full object-cover opacity-20 blur-2xl scale-125 saturate-150"
        />
        <div
          class="absolute inset-0"
          style="background: radial-gradient(circle at center, rgba(14,14,17,0.55) 0%, rgba(14,14,17,0.94) 85%);"
        ></div>
      </div>
    {/if}

    <!-- =================================================================== -->
    <!-- TOP-LEFT FLOATING PILL: Pica Brand Emblem, Filename & EXIF Readout  -->
    <!-- =================================================================== -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="absolute top-4 left-4 z-30 flex items-center gap-2.5 pl-2.5 pr-4 py-1.5 rounded-full bg-[#18181e]/90 border border-[#3f3f4c]/80 shadow-2xl backdrop-blur-xl max-w-[60vw]"
      data-tauri-drag-region
      onmousedown={handleStartWindowDrag}
    >
      <!-- Canonical Picasa 6-Petal Colorful Shutter Logo -->
      <div class="relative w-5 h-5 flex items-center justify-center shrink-0 pointer-events-none">
        <svg viewBox="0 0 100 100" class="w-5 h-5 drop-shadow-xs">
          <circle cx="50" cy="50" r="48" fill="#1e293b" />
          <path d="M50 10 A40 40 0 0 1 90 50 L50 50 Z" fill="#ef4444" />
          <path d="M90 50 A40 40 0 0 1 78 84 L50 50 Z" fill="#f97316" />
          <path d="M78 84 A40 40 0 0 1 32 87 L50 50 Z" fill="#eab308" />
          <path d="M32 87 A40 40 0 0 1 12 59 L50 50 Z" fill="#22c55e" />
          <path d="M12 59 A40 40 0 0 1 25 21 L50 50 Z" fill="#3b82f6" />
          <path d="M25 21 A40 40 0 0 1 50 10 L50 50 Z" fill="#a855f7" />
          <circle cx="50" cy="50" r="18" fill="#0f172a" />
          <circle cx="50" cy="50" r="14" fill="#38bdf8" opacity="0.6" />
        </svg>
      </div>

      {#if currentPhoto}
        <div class="flex items-center gap-2 min-w-0 pointer-events-none">
          <span class="text-xs font-semibold text-neutral-100 truncate" title={currentPhoto.file_path}>
            {currentPhoto.file_name}
          </span>

          {#if folderContext && folderContext.photos.length > 0}
            <span class="text-[11px] font-mono text-neutral-400 shrink-0">
              ({currentIndex + 1} of {folderContext.photos.length})
            </span>
          {/if}

          {#if sessionState?.has_session && !sessionState?.is_committed}
            <span
              class="text-[10px] px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-300 font-bold border border-amber-500/30 shrink-0"
            >
              Edited Draft
            </span>
          {/if}

          {#if quickExifSummary}
            <span class="hidden md:inline-block w-px h-3.5 bg-neutral-700/70 mx-0.5 shrink-0"></span>
            <span class="hidden md:inline-block text-[10px] font-mono text-blue-300/90 truncate">
              {quickExifSummary}
            </span>
          {/if}
        </div>
      {:else}
        <span class="text-xs font-bold tracking-wider uppercase text-neutral-300 pointer-events-none">
          Pica Viewer
        </span>
      {/if}
    </div>

    <!-- =================================================================== -->
    <!-- TOP-RIGHT FLOATING PILL: Inspector Toggles & Frameless Controls     -->
    <!-- =================================================================== -->
    <div
      class="absolute top-4 right-4 z-30 flex items-center gap-1 px-2 py-1 rounded-full bg-[#18181e]/90 border border-[#3f3f4c]/80 shadow-2xl backdrop-blur-xl text-neutral-300"
    >
      <!-- Live RGB Histogram Toggle -->
      <button
        onclick={() => (showHistogram = !showHistogram)}
        class={`p-1.5 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
          showHistogram ? 'text-blue-400 bg-white/15' : 'text-neutral-400 hover:text-white'
        }`}
        title="Toggle Live RGB Histogram & Camera Card (H)"
      >
        <BarChart2 class="w-4 h-4" />
      </button>

      <!-- Properties ("Info for Nerds") Sidebar Toggle -->
      <button
        onclick={() => (isInfoOpen = !isInfoOpen)}
        class={`p-1.5 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
          isInfoOpen ? 'text-blue-400 bg-white/15' : 'text-neutral-400 hover:text-white'
        }`}
        title="Properties / Info for Nerds Drawer (I)"
      >
        <Info class="w-4 h-4" />
      </button>

      <!-- Fullscreen Toggle -->
      <button
        onclick={handleToggleFullscreen}
        class="p-1.5 rounded-full hover:bg-white/10 text-neutral-400 hover:text-white transition-colors cursor-pointer"
        title="Toggle Fullscreen (F11)"
      >
        {#if isFullscreen}
          <Shrink class="w-4 h-4" />
        {:else}
          <Expand class="w-4 h-4" />
        {/if}
      </button>

      <div class="w-px h-4 bg-neutral-700/60 mx-0.5"></div>

      <!-- Minimize Window -->
      <button
        onclick={handleMinimize}
        class="p-1.5 rounded-full hover:bg-white/10 text-neutral-400 hover:text-white transition-colors cursor-pointer"
        title="Minimize Window"
      >
        <Minus class="w-4 h-4" />
      </button>

      <!-- Maximize / Restore Window -->
      <button
        onclick={handleToggleMaximize}
        class="p-1.5 rounded-full hover:bg-white/10 text-neutral-400 hover:text-white transition-colors cursor-pointer"
        title="Maximize / Restore Window"
      >
        <Maximize2 class="w-3.5 h-3.5" />
      </button>

      <!-- Close Viewer -->
      <button
        onclick={handleClose}
        class="p-1.5 rounded-full hover:bg-red-500/25 text-neutral-400 hover:text-red-300 transition-colors cursor-pointer"
        title="Close Viewer (Esc)"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- =================================================================== -->
    <!-- CENTER PHOTO VIEWPORT (1-Up Darkroom Canvas with Pan/Zoom/Rotate)   -->
    <!-- =================================================================== -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={imageContainerRef}
      class="relative z-10 flex-1 w-full h-full flex items-center justify-center overflow-hidden"
      onwheel={handleWheel}
      onpointerdown={handlePointerDown}
      ondblclick={toggleOneToOne}
      role="main"
    >
      {#if isLoading}
        <div class="flex flex-col items-center gap-3 text-neutral-400 animate-pulse">
          <Sparkles class="w-8 h-8 text-blue-400 animate-spin" />
          <span class="text-xs font-medium tracking-wide">Loading high-resolution photo...</span>
        </div>
      {:else if errorMessage}
        <div
          class="max-w-md p-6 rounded-2xl bg-[#18181e]/95 border border-[#3f3f4c]/80 text-center shadow-2xl backdrop-blur-xl"
        >
          <h3 class="text-sm font-bold text-red-400 mb-2">Unable to open photo</h3>
          <p class="text-xs text-neutral-400 mb-4">{errorMessage}</p>
          <button
            onclick={() => loadPath('.')}
            class="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold cursor-pointer shadow-md"
          >
            Browse Current Folder
          </button>
        </div>
      {:else if currentPhoto}
        <!-- Pan & Zoom Wrapper -->
        <div
          class="relative flex items-center justify-center select-none"
          style="
            transform: translate3d({panX}px, {panY}px, 0) scale({zoomLevel});
            transform-origin: center center;
            will-change: transform;
            transition: {isPanning || isWheelZooming ? 'none' : 'transform 120ms cubic-bezier(0.2, 0, 0, 1)'};
            cursor: {zoomLevel > 1.02 ? (isPanning ? 'grabbing' : 'grab') : 'default'};
          "
        >
          <!-- Physical 380ms Spring Rotation Wrapper -->
          <div
            class="relative flex items-center justify-center select-none"
            style="
              transform: rotate({visualRotation}deg);
              transform-origin: center center;
              will-change: transform;
              transition: {isTransitioning ? 'transform 380ms cubic-bezier(0.2, 0.9, 0.3, 1)' : 'none'};
            "
          >
            <img
              bind:this={mainImgRef}
              src={activeImageSrc}
              alt={currentPhoto.file_name}
              decoding="async"
              draggable="false"
              class="rounded-sm object-contain select-none shadow-[0_24px_80px_rgba(0,0,0,0.85)] ring-1 ring-white/10"
              style="
                max-height: {isFilmstripOpen ? 'calc(100vh - 185px)' : 'calc(100vh - 110px)'};
                max-width: calc(100vw - 64px);
              "
            />
          </div>
        </div>

        <!-- Edge Hover Navigation Chevrons (Matches ImageViewer.svelte) -->
        {#if folderContext && folderContext.photos.length > 1}
          <button
            onclick={goToPrev}
            class="absolute left-4 z-20 p-3 rounded-full bg-black/45 hover:bg-black/80 text-white/70 hover:text-white border border-white/10 backdrop-blur-md transition-all opacity-0 hover:opacity-100 focus:opacity-100 cursor-pointer shadow-xl group"
            title="Previous Photo (← or K)"
          >
            <ChevronLeft class="w-7 h-7 group-hover:-translate-x-0.5 transition-transform" />
          </button>

          <button
            onclick={goToNext}
            class="absolute right-4 z-20 p-3 rounded-full bg-black/45 hover:bg-black/80 text-white/70 hover:text-white border border-white/10 backdrop-blur-md transition-all opacity-0 hover:opacity-100 focus:opacity-100 cursor-pointer shadow-xl group"
            title="Next Photo (→ or J)"
          >
            <ChevronRight class="w-7 h-7 group-hover:translate-x-0.5 transition-transform" />
          </button>
        {/if}
      {/if}
    </div>

    <!-- =================================================================== -->
    <!-- BOTTOM-LEFT FLOATING HISTOGRAM & CAMERA CARD (Matches Editor)       -->
    <!-- =================================================================== -->
    {#if showHistogram && currentPhoto}
      <div
        class="absolute left-5 z-30 w-72 pointer-events-auto animate-in fade-in slide-in-from-bottom-2 duration-150"
        style="bottom: {isFilmstripOpen ? '140px' : '72px'};"
      >
        <Histogram
          photo={currentPhoto}
          metadata={metadata}
          imageElement={mainImgRef}
          imageSrc={activeImageSrc}
        />
      </div>
    {/if}

    <!-- =================================================================== -->
    <!-- TOAST NOTIFICATION FLOATER (Matches Editor)                         -->
    <!-- =================================================================== -->
    {#if toastMsg}
      <div
        class="absolute inset-x-0 z-40 flex justify-center pointer-events-none animate-in fade-in slide-in-from-bottom-2 duration-150"
        style="bottom: {isFilmstripOpen ? '148px' : '76px'};"
      >
        <div
          class="px-4 py-1.5 rounded-full bg-neutral-900/95 text-white text-xs font-medium border border-neutral-700 shadow-2xl backdrop-blur-md"
        >
          {toastMsg}
        </div>
      </div>
    {/if}

    <!-- =================================================================== -->
    <!-- BOTTOM CENTER UNIFIED FLOATING CONSOLE (HUD Pill + Filmstrip)       -->
    <!-- =================================================================== -->
    <div
      class="absolute bottom-4 inset-x-0 z-30 flex flex-col items-center gap-2.5 pointer-events-none px-4"
    >
      <!-- Tier 1: Picasa Signature Floating HUD Control Pill -->
      <div
        class="pointer-events-auto flex items-center gap-1.5 px-3.5 py-1.5 rounded-full bg-[#18181e]/92 border border-[#3f3f4c]/85 shadow-[0_12px_40px_rgba(0,0,0,0.8)] backdrop-blur-xl text-neutral-300"
      >
        <!-- Prev Photo -->
        <button
          onclick={goToPrev}
          disabled={!folderContext || folderContext.photos.length <= 1}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white disabled:opacity-30 disabled:pointer-events-none transition-colors cursor-pointer"
          title="Previous Photo (← or K)"
        >
          <ChevronLeft class="w-4 h-4" />
        </button>

        <!-- Position Counter -->
        <span class="text-xs font-mono px-1 text-neutral-400 select-none">
          {folderContext && folderContext.photos.length > 0
            ? `${currentIndex + 1} / ${folderContext.photos.length}`
            : '0 / 0'}
        </span>

        <!-- Next Photo -->
        <button
          onclick={goToNext}
          disabled={!folderContext || folderContext.photos.length <= 1}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white disabled:opacity-30 disabled:pointer-events-none transition-colors cursor-pointer"
          title="Next Photo (→ or J)"
        >
          <ChevronRight class="w-4 h-4" />
        </button>

        <!-- Slideshow Play / Pause -->
        <button
          onclick={toggleSlideshow}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
            isSlideshowPlaying ? 'text-amber-400 bg-white/15' : 'hover:text-white'
          }`}
          title={isSlideshowPlaying ? 'Pause Slideshow (P)' : 'Play Slideshow (P)'}
        >
          {#if isSlideshowPlaying}
            <Pause class="w-4 h-4 fill-current" />
          {:else}
            <Play class="w-4 h-4 fill-current" />
          {/if}
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Rotate Left (CCW) -->
        <button
          onclick={() => handleRotate(-90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          title="Rotate Left -90° ([ or Shift+R)"
        >
          <RotateCcw class="w-4 h-4" />
        </button>

        <!-- Rotate Right (CW) -->
        <button
          onclick={() => handleRotate(90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          title="Rotate Right +90° (] or R)"
        >
          <RotateCw class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Zoom Out -->
        <button
          onclick={() => setZoomPercent(Math.round(zoomLevel * 100) - 25)}
          class="p-1.5 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          title="Zoom Out (-)"
        >
          <Minus class="w-3.5 h-3.5" />
        </button>

        <!-- Picasa Station Zoom Slider -->
        <input
          type="range"
          min="100"
          max="500"
          step="5"
          value={Math.round(zoomLevel * 100)}
          oninput={(e) => setZoomPercent(Number((e.target as HTMLInputElement).value))}
          class="picasa-station-slider w-20 sm:w-24 cursor-pointer"
          title={`Zoom: ${Math.round(zoomLevel * 100)}%`}
        />

        <!-- Zoom In -->
        <button
          onclick={() => setZoomPercent(Math.round(zoomLevel * 100) + 25)}
          class="p-1.5 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
          title="Zoom In (+)"
        >
          <Plus class="w-3.5 h-3.5" />
        </button>

        <!-- Zoom Percentage Pill (Matches ImageViewer.svelte) -->
        {#if zoomLevel > 1.02}
          <button
            onclick={resetZoom}
            class="px-2 py-0.5 rounded-full bg-blue-600/30 hover:bg-blue-600/50 text-blue-300 text-[11px] font-mono font-medium border border-blue-500/40 transition-colors cursor-pointer"
            title="Click to reset zoom to Fit (0)"
          >
            {Math.round(zoomLevel * 100)}%
          </button>
        {/if}

        <!-- 1:1 / Fit Toggle Button -->
        <button
          onclick={toggleOneToOne}
          class={`px-2 py-0.5 rounded-md text-[11px] font-mono font-semibold transition-colors cursor-pointer ${
            zoomLevel > 1.02
              ? 'bg-blue-600 text-white shadow-xs'
              : 'hover:bg-white/10 text-neutral-300 hover:text-white'
          }`}
          title={zoomLevel > 1.02 ? 'Reset to Fit (Spacebar or 0)' : 'Zoom 1:1 Detail (Spacebar or 1)'}
        >
          {zoomLevel > 1.02 ? 'Fit' : '1:1'}
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Favorite Star -->
        <button
          onclick={handleToggleFavorite}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
            currentPhoto?.is_favorite
              ? 'text-amber-400 fill-amber-400'
              : 'text-neutral-400 hover:text-white'
          }`}
          title="Toggle Star Favorite (F)"
        >
          <Star class={`w-4 h-4 ${currentPhoto?.is_favorite ? 'fill-amber-400' : ''}`} />
        </button>

        <!-- Filmstrip Toggle -->
        <button
          onclick={() => (isFilmstripOpen = !isFilmstripOpen)}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
            isFilmstripOpen ? 'text-blue-400 bg-white/15' : 'text-neutral-400 hover:text-white'
          }`}
          title="Toggle Filmstrip Scrubber (T)"
        >
          <Film class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- "Edit in Pica" Primary Brand Handoff Button -->
        <button
          onclick={handleEditInPica}
          class="flex items-center gap-1.5 pl-2.5 pr-3.5 py-1.5 rounded-full bg-blue-600 hover:bg-blue-500 active:scale-95 text-white text-xs font-semibold shadow-lg shadow-blue-950/50 ring-1 ring-blue-400/50 transition-all cursor-pointer"
          title="Open photo in Pica Non-Destructive Editor (Enter)"
        >
          <!-- Picasa Mini Shutter Emblem -->
          <svg viewBox="0 0 100 100" class="w-4 h-4 shrink-0 drop-shadow-xs">
            <circle cx="50" cy="50" r="48" fill="#0f172a" />
            <path d="M50 10 A40 40 0 0 1 90 50 L50 50 Z" fill="#ef4444" />
            <path d="M90 50 A40 40 0 0 1 78 84 L50 50 Z" fill="#f97316" />
            <path d="M78 84 A40 40 0 0 1 32 87 L50 50 Z" fill="#eab308" />
            <path d="M32 87 A40 40 0 0 1 12 59 L50 50 Z" fill="#22c55e" />
            <path d="M12 59 A40 40 0 0 1 25 21 L50 50 Z" fill="#3b82f6" />
            <path d="M25 21 A40 40 0 0 1 50 10 L50 50 Z" fill="#a855f7" />
            <circle cx="50" cy="50" r="18" fill="#0f172a" />
            <circle cx="50" cy="50" r="14" fill="#38bdf8" opacity="0.7" />
          </svg>
          <span>Edit in Pica</span>
        </button>
      </div>

      <!-- Tier 2: Floating Glass Filmstrip Scrubber Ribbon -->
      {#if isFilmstripOpen && folderContext && folderContext.photos.length > 0}
        <div
          class="pointer-events-auto max-w-[min(92vw,880px)] px-3 py-2 rounded-2xl bg-[#16161c]/90 border border-[#333340]/85 backdrop-blur-xl shadow-[0_16px_48px_rgba(0,0,0,0.85)] flex items-center"
        >
          <div
            bind:this={filmstripEl}
            class="flex items-center gap-2 overflow-x-auto py-0.5 px-1 w-full filmstrip-scrollbar"
          >
            {#each folderContext.photos as photo, idx (photo.file_path)}
              {@const isActive = idx === currentIndex}
              {@const isNear = Math.abs(idx - currentIndex) <= 36}
              <button
                id="viewer-thumb-{idx}"
                onclick={() => selectPhoto(idx)}
                class={`relative shrink-0 h-12 w-16 rounded-lg overflow-hidden bg-neutral-900 cursor-pointer border ${
                  isActive
                    ? 'ring-2 ring-blue-500 border-blue-400 scale-105 shadow-lg shadow-blue-950/60 opacity-100'
                    : 'border-white/10 opacity-60 hover:opacity-100 hover:border-white/30'
                }`}
                title={`${photo.file_name} (${idx + 1} of ${folderContext.photos.length})`}
              >
                {#if isNear}
                  <img
                    src={getThumbnailUrl(photo)}
                    alt={photo.file_name}
                    decoding="async"
                    class="w-full h-full object-cover"
                    loading="lazy"
                  />
                {/if}
                {#if photo.is_favorite}
                  <Star
                    class="absolute top-1 right-1 w-2.5 h-2.5 fill-amber-400 text-amber-400 drop-shadow"
                  />
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  </div>

  <!-- ===================================================================== -->
  <!-- RIGHT INSPECTOR DRAWER: Properties ("Info for Nerds" - Shared w/Editor) -->
  <!-- ===================================================================== -->
  <PropertiesSidebar
    photo={currentPhoto}
    metadata={metadata}
    isOpen={isInfoOpen}
    onClose={() => (isInfoOpen = false)}
  />
</div>

<style>
  /* Picasa station slider (Shared with ImageViewer.svelte) */
  input[type='range'].picasa-station-slider {
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
  }
  input[type='range'].picasa-station-slider::-webkit-slider-runnable-track {
    height: 5px;
    background: linear-gradient(180deg, #181d26 0%, #252d3a 100%);
    border: 1px solid rgba(255, 255, 255, 0.22);
    border-radius: 3px;
    box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.7);
  }
  input[type='range'].picasa-station-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 11px;
    height: 15px;
    border-radius: 2px;
    background: linear-gradient(180deg, #ffffff 0%, #cbd5e1 100%);
    border: 1px solid #0f172a;
    margin-top: -6px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.6);
  }
  input[type='range'].picasa-station-slider:hover::-webkit-slider-thumb {
    background: linear-gradient(180deg, #60a5fa 0%, #3b82f6 100%);
    border-color: #ffffff;
  }

  /* Sleek micro scrollbar for floating filmstrip */
  .filmstrip-scrollbar::-webkit-scrollbar {
    height: 4px;
  }
  .filmstrip-scrollbar::-webkit-scrollbar-track {
    background: transparent;
  }
  .filmstrip-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.18);
    border-radius: 4px;
  }
  .filmstrip-scrollbar::-webkit-scrollbar-thumb:hover {
    background: rgba(255, 255, 255, 0.35);
  }
</style>
