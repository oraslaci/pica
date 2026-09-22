<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import {
    ChevronLeft,
    ChevronRight,
    X,
    RotateCcw,
    RotateCw,
    ZoomIn,
    ZoomOut,
    Maximize2,
    Star,
    Info,
    Calendar,
    Camera,
    HardDrive,
  } from '@lucide/svelte';
  import type { Photo } from '../types';
  import { getFullImageUrl, toggleFavorite } from '../api';

  let {
    photos = [],
    currentIndex = 0,
    isOpen = $bindable(false),
    onClose = () => {},
  }: {
    photos: Photo[];
    currentIndex: number;
    isOpen: boolean;
    onClose?: () => void;
  } = $props();

  let activeIndex = $state(0);
  let rotation = $state(0);
  let isZoomed100 = $state(false);
  let showInfo = $state(false);
  let isFav = $state(false);

  $effect(() => {
    activeIndex = currentIndex;
  });

  const currentPhoto = $derived(photos[activeIndex] || null);

  $effect(() => {
    if (currentPhoto) {
      isFav = currentPhoto.is_favorite;
      rotation = 0;
      isZoomed100 = false;
    }
  });

  function nextPhoto() {
    if (photos.length === 0) return;
    activeIndex = (activeIndex + 1) % photos.length;
  }

  function prevPhoto() {
    if (photos.length === 0) return;
    activeIndex = (activeIndex - 1 + photos.length) % photos.length;
  }

  function handleRotate(deg: number) {
    rotation = (rotation + deg) % 360;
  }

  function toggleZoom() {
    isZoomed100 = !isZoomed100;
  }

  async function handleToggleFav() {
    if (!currentPhoto) return;
    const newState = await toggleFavorite(currentPhoto.id);
    isFav = newState;
    currentPhoto.is_favorite = newState;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!isOpen) return;

    if (e.key === 'Escape') {
      isOpen = false;
      onClose();
    } else if (e.key === 'ArrowRight' || e.key === 'j') {
      nextPhoto();
    } else if (e.key === 'ArrowLeft' || e.key === 'k') {
      prevPhoto();
    } else if (e.key === ' ') {
      e.preventDefault();
      toggleZoom();
    } else if (e.key === 'f') {
      handleToggleFav();
    } else if (e.key === 'r') {
      handleRotate(90);
    } else if (e.key === 'i') {
      showInfo = !showInfo;
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeyDown);
  });

  function formatBytes(bytes: number) {
    if (!bytes) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  function formatDate(timestamp: number | null) {
    if (!timestamp) return 'Unknown Date';
    return new Date(timestamp).toLocaleString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }
</script>

{#if isOpen && currentPhoto}
  <div class="fixed inset-0 z-50 bg-[#0e0e0e] flex flex-col select-none overflow-hidden">
    <!-- Top Bar: Photo title & Close button -->
    <div
      class="absolute top-0 inset-x-0 z-20 h-12 bg-linear-to-b from-black/80 to-transparent flex items-center justify-between px-6 opacity-0 hover:opacity-100 transition-opacity duration-200"
    >
      <div class="text-xs font-medium text-neutral-300 truncate max-w-lg drop-shadow">
        {currentPhoto.file_name}
        <span class="text-neutral-500 ml-2">({activeIndex + 1} of {photos.length})</span>
      </div>

      <button
        onclick={() => {
          isOpen = false;
          onClose();
        }}
        class="p-2 rounded-full text-neutral-400 hover:text-white hover:bg-neutral-800/80 transition-colors drop-shadow"
        title="Back to Library (Esc)"
      >
        <X class="w-5 h-5" />
      </button>
    </div>

    <!-- Main Canvas Viewport -->
    <div
      class="flex-1 w-full h-full relative flex items-center justify-center cursor-default overflow-hidden"
      onclick={(e) => {
        if (e.target === e.currentTarget) {
          // clicked background
        }
      }}
    >
      <!-- Navigation Edge Zones -->
      <button
        onclick={prevPhoto}
        class="absolute left-4 z-20 p-3 rounded-full bg-black/40 hover:bg-black/80 text-white/70 hover:text-white backdrop-blur-xs transition-all opacity-0 hover:opacity-100 focus:opacity-100"
        title="Previous (←)"
      >
        <ChevronLeft class="w-7 h-7" />
      </button>

      <button
        onclick={nextPhoto}
        class="absolute right-4 z-20 p-3 rounded-full bg-black/40 hover:bg-black/80 text-white/70 hover:text-white backdrop-blur-xs transition-all opacity-0 hover:opacity-100 focus:opacity-100"
        title="Next (→)"
      >
        <ChevronRight class="w-7 h-7" />
      </button>

      <!-- The Image Element -->
      <div
        class="w-full h-full flex items-center justify-center p-6 transition-transform duration-150"
        style={`transform: rotate(${rotation}deg);`}
      >
        <img
          src={getFullImageUrl(currentPhoto)}
          alt={currentPhoto.file_name}
          class={`max-h-full max-w-full rounded-sm shadow-2xl transition-all duration-100 ${
            isZoomed100 ? 'scale-150 cursor-grab' : 'object-contain cursor-zoom-in'
          }`}
          onclick={toggleZoom}
        />
      </div>

      <!-- EXIF Info Flyout Drawer -->
      {#if showInfo}
        <div
          class="absolute top-14 right-6 z-30 w-72 bg-[#1c1c1c]/95 border border-[#383838] rounded-lg shadow-2xl backdrop-blur-md p-4 text-xs text-neutral-300 space-y-3 animate-in fade-in zoom-in-95 duration-150"
        >
          <div class="flex items-center justify-between pb-2 border-b border-[#303030]">
            <span class="font-semibold text-white">Photo Details</span>
            <button onclick={() => (showInfo = false)} class="text-neutral-400 hover:text-white">
              <X class="w-3.5 h-3.5" />
            </button>
          </div>

          <div class="space-y-2">
            <div>
              <div class="text-[10px] text-neutral-500 uppercase">File Name</div>
              <div class="font-mono text-white truncate">{currentPhoto.file_name}</div>
            </div>

            <div class="flex items-center gap-2">
              <Calendar class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
              <div>
                <div class="text-[10px] text-neutral-500 uppercase">Date Taken</div>
                <div>{formatDate(currentPhoto.date_taken)}</div>
              </div>
            </div>

            {#if currentPhoto.width && currentPhoto.height}
              <div class="flex items-center gap-2">
                <Maximize2 class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
                <div>
                  <div class="text-[10px] text-neutral-500 uppercase">Dimensions</div>
                  <div>
                    {currentPhoto.width} × {currentPhoto.height} ({((currentPhoto.width * currentPhoto.height) / 1000000).toFixed(1)} MP)
                  </div>
                </div>
              </div>
            {/if}

            {#if currentPhoto.camera_make || currentPhoto.camera_model}
              <div class="flex items-center gap-2">
                <Camera class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
                <div>
                  <div class="text-[10px] text-neutral-500 uppercase">Camera</div>
                  <div class="truncate">{currentPhoto.camera_make || ''} {currentPhoto.camera_model || ''}</div>
                </div>
              </div>
            {/if}

            <div class="flex items-center gap-2">
              <HardDrive class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
              <div>
                <div class="text-[10px] text-neutral-500 uppercase">File Size</div>
                <div>{formatBytes(currentPhoto.file_size)}</div>
              </div>
            </div>
          </div>
        </div>
      {/if}
    </div>

    <!-- Picasa Signature Floating HUD Overlay at Bottom Center -->
    <div class="absolute bottom-6 inset-x-0 z-30 flex justify-center pointer-events-none">
      <div
        class="pointer-events-auto flex items-center gap-1.5 px-3 py-2 rounded-full bg-[#18181b]/85 border border-[#3f3f46]/70 shadow-2xl backdrop-blur-md text-neutral-300"
      >
        <!-- Exit to Library -->
        <button
          onclick={() => {
            isOpen = false;
            onClose();
          }}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Back to Library (Esc)"
        >
          <X class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-1"></div>

        <!-- Prev -->
        <button
          onclick={prevPhoto}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Previous Photo (←)"
        >
          <ChevronLeft class="w-4 h-4" />
        </button>

        <!-- Position Counter -->
        <span class="text-xs font-medium px-2 text-neutral-400">
          {activeIndex + 1} / {photos.length}
        </span>

        <!-- Next -->
        <button
          onclick={nextPhoto}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Next Photo (→)"
        >
          <ChevronRight class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-1"></div>

        <!-- Rotate Left -->
        <button
          onclick={() => handleRotate(-90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Rotate Left (-90°)"
        >
          <RotateCcw class="w-4 h-4" />
        </button>

        <!-- Rotate Right -->
        <button
          onclick={() => handleRotate(90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Rotate Right (+90°)"
        >
          <RotateCw class="w-4 h-4" />
        </button>

        <!-- 1:1 Zoom Toggle -->
        <button
          onclick={toggleZoom}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors ${
            isZoomed100 ? 'text-blue-400' : 'hover:text-white'
          }`}
          title="Toggle Zoom 100% (Spacebar)"
        >
          <Maximize2 class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-1"></div>

        <!-- Favorite Star -->
        <button
          onclick={handleToggleFav}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors ${
            isFav ? 'text-amber-400 fill-amber-400' : 'text-neutral-400 hover:text-white'
          }`}
          title="Toggle Favorite (f)"
        >
          <Star class={`w-4 h-4 ${isFav ? 'fill-amber-400' : ''}`} />
        </button>

        <!-- Info Toggle -->
        <button
          onclick={() => (showInfo = !showInfo)}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors ${
            showInfo ? 'text-blue-400' : 'hover:text-white'
          }`}
          title="Show EXIF Photo Info (i)"
        >
          <Info class="w-4 h-4" />
        </button>
      </div>
    </div>
  </div>
{/if}
