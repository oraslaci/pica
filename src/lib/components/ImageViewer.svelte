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
    Calendar,
    Camera,
    HardDrive,
    Play,
    Pause,
    ArrowLeft,
    Undo2,
    Redo2,
    Save,
    History,
    AlertTriangle,
    Check,
  } from '@lucide/svelte';
  import type { EditSessionState, Photo } from '../types';
  import {
    getFullImageUrl,
    toggleFavorite,
    getEditSession,
    applyRotation,
    undoEdit,
    redoEdit,
    saveEditSession,
    revertEditSession,
    resolveEditConflict,
  } from '../api';

  let {
    photos = [],
    currentIndex = 0,
    isOpen = $bindable(false),
    onClose = () => {},
    onPhotoEdited = () => {},
  }: {
    photos: Photo[];
    currentIndex: number;
    isOpen: boolean;
    onClose?: () => void;
    onPhotoEdited?: () => void;
  } = $props();

  let activeIndex = $state(0);
  let rotation = $state(0);
  let isZoomed100 = $state(false);
  let showInfo = $state(false);
  let isFav = $state(false);
  let isPlayingSlideshow = $state(false);
  let slideshowTimer: ReturnType<typeof setInterval> | null = null;

  // Editing & Undo/Redo State
  let sessionState = $state<EditSessionState | null>(null);
  let isEditing = $state(false);
  let cacheBuster = $state(Date.now());
  let toastMsg = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showToast(msg: string) {
    toastMsg = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toastMsg = null;
    }, 2500);
  }

  $effect(() => {
    activeIndex = currentIndex;
  });

  const currentPhoto = $derived(photos[activeIndex] || null);

  async function loadSession() {
    if (!currentPhoto) return;
    try {
      const sess = await getEditSession(currentPhoto.id);
      sessionState = sess;
    } catch (e) {
      sessionState = null;
    }
  }

  $effect(() => {
    if (currentPhoto && isOpen) {
      isFav = currentPhoto.is_favorite;
      rotation = 0;
      isZoomed100 = false;
      loadSession();
    }
  });

  $effect(() => {
    if (isPlayingSlideshow && isOpen && photos.length > 1) {
      slideshowTimer = setInterval(() => {
        nextPhoto();
      }, 3000);
    } else {
      if (slideshowTimer) {
        clearInterval(slideshowTimer);
        slideshowTimer = null;
      }
    }
    return () => {
      if (slideshowTimer) {
        clearInterval(slideshowTimer);
        slideshowTimer = null;
      }
    };
  });

  $effect(() => {
    if (!isOpen) {
      isPlayingSlideshow = false;
    }
  });

  function toggleSlideshow() {
    isPlayingSlideshow = !isPlayingSlideshow;
  }

  function nextPhoto() {
    if (photos.length === 0) return;
    activeIndex = (activeIndex + 1) % photos.length;
  }

  function prevPhoto() {
    if (photos.length === 0) return;
    activeIndex = (activeIndex - 1 + photos.length) % photos.length;
  }

  async function handleRotate(deg: number) {
    if (!currentPhoto) return;

    // 1. Instant 0ms visual client feedback
    rotation = (rotation + deg) % 360;

    // 2. Perform background write & update
    isEditing = true;
    try {
      const res = await applyRotation(currentPhoto.id, deg);
      if (res.success && res.photo) {
        // Update photo details
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
        cacheBuster = Date.now();
        // Since image on disk now has the rotation, reset client CSS delta
        rotation = 0;
        onPhotoEdited();
      } else if (res.session_state?.has_conflict) {
        sessionState = res.session_state;
      }
    } catch (e) {
      console.error('Failed to rotate:', e);
      showToast('Error rotating image');
    } finally {
      isEditing = false;
    }
  }

  async function handleUndo() {
    if (!currentPhoto || !sessionState?.can_undo) return;
    isEditing = true;
    try {
      const res = await undoEdit(currentPhoto.id);
      if (res.success && res.photo) {
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
        cacheBuster = Date.now();
        rotation = 0;
        showToast('Undo applied');
        onPhotoEdited();
      }
    } catch (e) {
      console.error('Failed to undo:', e);
    } finally {
      isEditing = false;
    }
  }

  async function handleRedo() {
    if (!currentPhoto || !sessionState?.can_redo) return;
    isEditing = true;
    try {
      const res = await redoEdit(currentPhoto.id);
      if (res.success && res.photo) {
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
        cacheBuster = Date.now();
        rotation = 0;
        showToast('Redo applied');
        onPhotoEdited();
      }
    } catch (e) {
      console.error('Failed to redo:', e);
    } finally {
      isEditing = false;
    }
  }

  async function handleSaveSession() {
    if (!currentPhoto) return;
    isEditing = true;
    try {
      const sess = await saveEditSession(currentPhoto.id);
      sessionState = sess;
      showToast('Edits saved to disk');
      onPhotoEdited();
    } catch (e) {
      console.error('Failed to save session:', e);
      showToast('Error saving edits');
    } finally {
      isEditing = false;
    }
  }

  async function handleRevert() {
    if (!currentPhoto) return;
    isEditing = true;
    try {
      const res = await revertEditSession(currentPhoto.id);
      if (res.success && res.photo) {
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
        cacheBuster = Date.now();
        rotation = 0;
        showToast('Reverted to original photo');
        onPhotoEdited();
      }
    } catch (e) {
      console.error('Failed to revert:', e);
      showToast('Error reverting to original');
    } finally {
      isEditing = false;
    }
  }

  async function handleResolveConflict(choice: string) {
    if (!currentPhoto) return;
    isEditing = true;
    try {
      const res = await resolveEditConflict(currentPhoto.id, choice);
      if (res.success && res.photo) {
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
        cacheBuster = Date.now();
        rotation = 0;
        showToast(res.message || 'Conflict resolved');
        onPhotoEdited();
      }
    } catch (e) {
      console.error('Failed to resolve conflict:', e);
    } finally {
      isEditing = false;
    }
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

    const isCtrlOrCmd = e.ctrlKey || e.metaKey;

    if (e.key === 'Escape') {
      isOpen = false;
      onClose();
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) {
        handleRedo();
      } else {
        handleUndo();
      }
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      handleRedo();
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 's') {
      e.preventDefault();
      handleSaveSession();
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
      if (e.shiftKey) {
        handleRotate(-90);
      } else {
        handleRotate(90);
      }
    } else if (e.key === 'p') {
      toggleSlideshow();
    } else if (e.key === 'i') {
      showInfo = !showInfo;
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeyDown);
    if (toastTimer) clearTimeout(toastTimer);
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
    <!-- Top Bar: Back button, photo title, toast, and close button -->
    <div
      class="absolute top-0 inset-x-0 z-20 h-14 bg-gradient-to-b from-black/80 to-transparent flex items-center justify-between px-6 opacity-0 hover:opacity-100 transition-opacity duration-200"
    >
      <button
        onclick={() => {
          isOpen = false;
          onClose();
        }}
        class="flex items-center gap-2 px-3 py-1.5 rounded-full bg-neutral-800/80 hover:bg-neutral-700 text-neutral-300 hover:text-white transition-colors text-xs font-medium backdrop-blur-xs cursor-pointer shadow-md"
        title="Back to Library (Esc)"
      >
        <ArrowLeft class="w-4 h-4" />
        <span>Library</span>
      </button>

      <div class="flex items-center gap-3">
        <div class="text-xs font-medium text-neutral-300 truncate max-w-sm drop-shadow">
          {currentPhoto.file_name}
          <span class="text-neutral-500 ml-2">({activeIndex + 1} of {photos.length})</span>
        </div>

        {#if sessionState?.has_session && !sessionState?.is_committed}
          <span class="text-[10px] px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-300 font-bold border border-amber-500/30">
            Unsaved Edits
          </span>
        {/if}
      </div>

      <button
        onclick={() => {
          isOpen = false;
          onClose();
        }}
        class="p-2 rounded-full text-neutral-400 hover:text-white hover:bg-neutral-800/80 transition-colors drop-shadow cursor-pointer"
        title="Close (Esc)"
      >
        <X class="w-5 h-5" />
      </button>
    </div>

    <!-- Toast Notification Banner -->
    {#if toastMsg}
      <div class="absolute top-16 inset-x-0 z-30 flex justify-center pointer-events-none animate-in fade-in duration-150">
        <div class="px-4 py-1.5 rounded-full bg-neutral-900/90 text-white text-xs font-medium border border-neutral-700 shadow-xl backdrop-blur-md">
          {toastMsg}
        </div>
      </div>
    {/if}

    <!-- External Modification Conflict Banner -->
    {#if sessionState?.has_conflict}
      <div class="absolute top-16 inset-x-0 z-40 flex justify-center px-4">
        <div class="max-w-xl w-full p-4 rounded-xl bg-amber-950/95 border border-amber-500/60 shadow-2xl backdrop-blur-md text-amber-200 text-xs space-y-3 animate-in zoom-in-95 duration-150">
          <div class="flex items-center gap-2 text-amber-300 font-bold text-sm">
            <AlertTriangle class="w-5 h-5 shrink-0 text-amber-400" />
            <span>External Changes Detected on Disk</span>
          </div>
          <p class="text-amber-200/90 leading-relaxed text-[11px]">
            This photo was modified outside of Pica while you had an uncommitted editing draft. How would you like to proceed?
          </p>
          <div class="flex flex-wrap items-center gap-2 pt-1">
            <button
              onclick={() => handleResolveConflict('keep_external')}
              class="px-3 py-1.5 rounded bg-amber-900/80 hover:bg-amber-800 text-amber-100 font-semibold border border-amber-600/40 transition-colors"
            >
              Keep External Version
            </button>
            <button
              onclick={() => handleResolveConflict('apply_pica')}
              class="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-semibold transition-colors"
            >
              Overwrite with Pica Draft
            </button>
            <button
              onclick={() => handleResolveConflict('save_as_copy')}
              class="px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-600 transition-colors"
            >
              Save Draft as New Copy
            </button>
          </div>
        </div>
      </div>
    {/if}

    <!-- Main Canvas Viewport -->
    <div
      class="flex-1 w-full h-full relative flex items-center justify-center cursor-default overflow-hidden"
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

      <!-- The Image Element (Immediate hardware-accelerated CSS rotation feedback) -->
      <div
        class="w-full h-full flex items-center justify-center p-6 transition-transform duration-100 ease-out"
        style={`transform: rotate(${rotation}deg);`}
      >
        <img
          src={getFullImageUrl(currentPhoto, cacheBuster)}
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

            {#if sessionState?.has_backup}
              <div class="pt-2 border-t border-neutral-800 text-[10px] text-emerald-400 flex items-center gap-1.5">
                <Check class="w-3.5 h-3.5" />
                <span>Pristine original preserved in .pica_originals</span>
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    <!-- Picasa Signature Floating HUD Overlay at Bottom Center -->
    <div class="absolute bottom-6 inset-x-0 z-30 flex justify-center pointer-events-none">
      <div
        class="pointer-events-auto flex items-center gap-1.5 px-3.5 py-2 rounded-full bg-[#18181b]/90 border border-[#3f3f46]/80 shadow-2xl backdrop-blur-md text-neutral-300"
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

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Prev Photo -->
        <button
          onclick={prevPhoto}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Previous Photo (←)"
        >
          <ChevronLeft class="w-4 h-4" />
        </button>

        <!-- Position Counter -->
        <span class="text-xs font-medium px-1.5 text-neutral-400">
          {activeIndex + 1} / {photos.length}
        </span>

        <!-- Next Photo -->
        <button
          onclick={nextPhoto}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Next Photo (→)"
        >
          <ChevronRight class="w-4 h-4" />
        </button>

        <!-- Slideshow Play / Pause -->
        <button
          onclick={toggleSlideshow}
          class={`p-2 rounded-full hover:bg-white/10 transition-colors ${
            isPlayingSlideshow ? 'text-amber-400 bg-white/15' : 'hover:text-white'
          }`}
          title={isPlayingSlideshow ? 'Pause Slideshow (p)' : 'Play Slideshow (p)'}
        >
          {#if isPlayingSlideshow}
            <Pause class="w-4 h-4 fill-current" />
          {:else}
            <Play class="w-4 h-4 fill-current" />
          {/if}
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Rotate Left (CCW) -->
        <button
          onclick={() => handleRotate(-90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Rotate Left -90° (Shift+R)"
        >
          <RotateCcw class="w-4 h-4" />
        </button>

        <!-- Rotate Right (CW) -->
        <button
          onclick={() => handleRotate(90)}
          class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors"
          title="Rotate Right +90° (r)"
        >
          <RotateCw class="w-4 h-4" />
        </button>

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

        <!-- Undo Action -->
        <button
          onclick={handleUndo}
          disabled={!sessionState?.can_undo || isEditing}
          class="p-2 rounded-full hover:bg-white/10 transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
          title="Undo Edit (Ctrl+Z)"
        >
          <Undo2 class="w-4 h-4" />
        </button>

        <!-- Redo Action -->
        <button
          onclick={handleRedo}
          disabled={!sessionState?.can_redo || isEditing}
          class="p-2 rounded-full hover:bg-white/10 transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
          title="Redo Edit (Ctrl+Y)"
        >
          <Redo2 class="w-4 h-4" />
        </button>

        <!-- Commit / Save Button -->
        {#if sessionState?.has_session && !sessionState?.is_committed}
          <button
            onclick={handleSaveSession}
            disabled={isEditing}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-blue-600 hover:bg-blue-500 text-white font-medium text-xs shadow-md transition-colors"
            title="Save edits to disk (Ctrl+S)"
          >
            <Save class="w-3.5 h-3.5" />
            <span>Save</span>
          </button>
        {/if}

        <!-- Revert to Original -->
        {#if sessionState?.has_session || sessionState?.has_backup}
          <button
            onclick={handleRevert}
            disabled={isEditing}
            class="p-2 rounded-full hover:bg-white/10 hover:text-amber-400 transition-colors"
            title="Revert to original photo"
          >
            <History class="w-4 h-4" />
          </button>
        {/if}

        <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

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
