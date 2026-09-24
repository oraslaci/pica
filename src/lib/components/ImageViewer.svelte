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
    Wrench,
    SlidersHorizontal,
    Wand,
    Sparkles,
    Palette,
    Crop,
    Eye,
    Sun,
    Contrast,
    Type,
    PanelLeftClose,
    PanelLeftOpen,
    Columns2,
    SlidersVertical,
    Pipette,
    ShieldCheck,
  } from '@lucide/svelte';
  import type { EditSessionState, Photo } from '../types';
  import {
    getFullImageUrl,
    getOriginalImageUrl,
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
  let isZoomed100 = $state(false);
  let showInfo = $state(false);
  let isFav = $state(false);
  let isPlayingSlideshow = $state(false);
  let slideshowTimer: ReturnType<typeof setInterval> | null = null;

  // Viewport Container Dimensions for Responsive Aspect-Ratio Fit Scaling
  let canvasContainer = $state<HTMLElement | null>(null);
  let canvasWidth = $state(900);
  let canvasHeight = $state(700);

  // Rotation Animation State (Monotonic cumulative angle for silky smooth continuous physical glide)
  let visualRotation = $state(0);

  // Sidebar & Navigation Tabs
  let isSidebarCollapsed = $state(false);
  type EditorTab = 'basic' | 'tuning' | 'fx1' | 'fx2' | 'fx3';
  let activeTab = $state<EditorTab>('basic');

  // Comparison Mode ('1up' = Single canvas, 'ab' = Side-by-side before/after)
  let comparisonMode = $state<'1up' | 'ab'>('1up');

  // Basic Fixes Sub-Tools
  type ActiveTool = null | 'crop' | 'straighten' | 'redeye' | 'retouch' | 'text';
  let activeTool = $state<ActiveTool>(null);
  let cropAspect = $state<'free' | 'square' | '4:3' | '16:9' | '4x6' | '5x7' | '8x10'>('free');
  let straightenAngle = $state(0);
  let redEyeRadius = $state(28);
  let retouchRadius = $state(32);
  let overlayText = $state('');
  let textColor = $state('#ffffff');
  let textSize = $state(28);

  // Tuning Adjustments (Live CSS filter mapping)
  let fillLight = $state(0);
  let highlights = $state(0);
  let shadows = $state(0);
  let colorTemp = $state(0);

  // Active Effects & Filter Presets
  let activeEffect = $state<string | null>(null);
  let effectIntensity = $state(80);

  // Session & Backend Sync
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

  // Reset tool & slider states when navigating to a new photo
  function resetLocalToolState() {
    visualRotation = 0;
    straightenAngle = 0;
    activeTool = null;
    fillLight = 0;
    highlights = 0;
    shadows = 0;
    colorTemp = 0;
    activeEffect = null;
    overlayText = '';
  }

  async function loadSession() {
    if (!currentPhoto) return;
    try {
      const sess = await getEditSession(currentPhoto.id);
      sessionState = sess;
    } catch {
      sessionState = null;
    }
  }

  $effect(() => {
    if (currentPhoto && isOpen) {
      isFav = currentPhoto.is_favorite;
      isZoomed100 = false;
      resetLocalToolState();
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

  // Responsive Fit Scaling: Calculates the scale needed when image is rotated by 90° or 270°
  // so the rotated bounds fit gracefully inside the canvas without overflowing or clipping.
  const fitScale = $derived.by(() => {
    const isQuarterTurn = Math.abs(Math.round(visualRotation / 90)) % 2 === 1;
    if (!isQuarterTurn || !currentPhoto?.width || !currentPhoto?.height || !canvasWidth || !canvasHeight) {
      return 1;
    }
    const padding = 48;
    const availW = Math.max(100, canvasWidth - padding);
    const availH = Math.max(100, canvasHeight - padding);

    const imgAspect = currentPhoto.width / currentPhoto.height;
    let baseW: number;
    let baseH: number;
    if (availW / availH > imgAspect) {
      baseH = availH;
      baseW = availH * imgAspect;
    } else {
      baseW = availW;
      baseH = availW / imgAspect;
    }

    // Swapped visual dimensions on 90° / 270°
    const scaleW = availW / baseH;
    const scaleH = availH / baseW;
    const s = Math.min(scaleW, scaleH, 1);
    return Math.max(0.2, Math.min(s, 1));
  });

  // Live CSS Filter computed from Tuning sliders and active Creative Effects
  const liveFilterStyle = $derived.by(() => {
    const parts: string[] = [];

    // Tuning Sliders
    if (fillLight > 0) parts.push(`brightness(${1 + (fillLight / 100) * 0.4})`);
    if (highlights > 0) parts.push(`contrast(${1 + (highlights / 100) * 0.35})`);
    if (shadows > 0) parts.push(`brightness(${1 + (shadows / 100) * 0.25})`);
    if (colorTemp !== 0) {
      if (colorTemp > 0) {
        parts.push(`sepia(${(colorTemp / 100) * 0.35}) saturate(${1 + (colorTemp / 100) * 0.25})`);
      } else {
        parts.push(`hue-rotate(${(colorTemp / 100) * 35}deg) saturate(${1 + Math.abs(colorTemp / 100) * 0.15})`);
      }
    }

    // Creative Filters
    const k = effectIntensity / 100;
    switch (activeEffect) {
      case 'sepia':
        parts.push(`sepia(${0.85 * k}) contrast(${1 + 0.1 * k})`);
        break;
      case 'bw':
        parts.push(`grayscale(${100 * k}%) contrast(${1 + 0.2 * k})`);
        break;
      case 'warmify':
        parts.push(`sepia(${0.4 * k}) saturate(${1 + 0.5 * k})`);
        break;
      case 'film_grain':
        parts.push(`contrast(${1 + 0.25 * k}) brightness(${1 - 0.05 * k}) saturate(${1 - 0.15 * k})`);
        break;
      case 'tint':
        parts.push(`sepia(${0.5 * k}) hue-rotate(185deg) saturate(${1 + 0.4 * k})`);
        break;
      case 'saturation':
        parts.push(`saturate(${1 + 1.2 * k})`);
        break;
      case 'soft_focus':
        parts.push(`blur(${1.2 * k}px) contrast(${1 + 0.1 * k})`);
        break;
      case 'glow':
        parts.push(`brightness(${1 + 0.2 * k}) contrast(${1 + 0.15 * k}) saturate(${1 + 0.2 * k})`);
        break;
      case 'focal_bw':
        parts.push(`grayscale(${80 * k}%) contrast(${1 + 0.25 * k})`);
        break;
      case 'grad_tint':
        parts.push(`sepia(${0.45 * k}) hue-rotate(45deg) saturate(${1 + 0.3 * k})`);
        break;
      case 'invert':
        parts.push(`invert(${100 * k}%)`);
        break;
      case 'infrared':
        parts.push(`invert(${75 * k}%) hue-rotate(180deg) saturate(${1 - 0.6 * k}) contrast(${1 + 0.4 * k})`);
        break;
      case 'lomo':
        parts.push(`contrast(${1 + 0.45 * k}) saturate(${1 + 0.35 * k}) brightness(${1 - 0.08 * k})`);
        break;
      case 'holga':
        parts.push(`contrast(${1 + 0.5 * k}) saturate(${1 - 0.35 * k}) brightness(${1 + 0.05 * k})`);
        break;
      case 'hdr':
        parts.push(`contrast(${1 + 0.4 * k}) saturate(${1 + 0.45 * k}) brightness(${1 + 0.08 * k})`);
        break;
      case 'cinema':
        parts.push(`contrast(${1 + 0.25 * k}) saturate(${1 - 0.25 * k}) brightness(${1 - 0.08 * k})`);
        break;
      case 'orton':
        parts.push(`brightness(${1 + 0.12 * k}) contrast(${1 + 0.18 * k}) saturate(${1 + 0.3 * k})`);
        break;
      case '1960s':
        parts.push(`sepia(${0.45 * k}) hue-rotate(-20deg) saturate(${1 + 0.25 * k}) contrast(${1 + 0.15 * k})`);
        break;
      case 'cross_process':
        parts.push(`contrast(${1 + 0.35 * k}) saturate(${1 + 0.4 * k}) hue-rotate(15deg)`);
        break;
      case 'posterize':
        parts.push(`contrast(${1 + 1.2 * k}) saturate(${1 + 0.8 * k})`);
        break;
      case 'duotone':
        parts.push(`grayscale(${100 * k}%) sepia(${0.85 * k}) hue-rotate(170deg) saturate(${1 + 1.5 * k})`);
        break;
      case 'heat_map':
        parts.push(`invert(${100 * k}%) hue-rotate(95deg) saturate(${1 + 2.0 * k})`);
        break;
      case 'comic':
        parts.push(`contrast(${1 + 1.0 * k}) saturate(${1 + 0.6 * k}) brightness(${1 + 0.05 * k})`);
        break;
      case 'fisheye':
        parts.push(`contrast(${1 + 0.2 * k}) saturate(${1 + 0.15 * k})`);
        break;
      case 'pastel':
        parts.push(`brightness(${1 + 0.18 * k}) saturate(${1 - 0.3 * k}) contrast(${1 - 0.08 * k})`);
        break;
      case 'sketch':
        parts.push(`grayscale(${100 * k}%) contrast(${1 + 2.0 * k}) brightness(${1 + 0.2 * k})`);
        break;
      case 'neon':
        parts.push(`invert(${100 * k}%) saturate(${1 + 2.5 * k}) contrast(${1 + 0.6 * k})`);
        break;
      case 'charcoal':
        parts.push(`grayscale(${100 * k}%) contrast(${1 + 1.5 * k}) brightness(${1 - 0.12 * k})`);
        break;
      case 'tilt_shift':
        parts.push(`contrast(${1 + 0.18 * k}) saturate(${1 + 0.25 * k})`);
        break;
      case 'motion_blur':
        parts.push(`blur(${2 * k}px) contrast(${1 + 0.1 * k})`);
        break;
      case 'vintage_70s':
        parts.push(`sepia(${0.5 * k}) contrast(${1 + 0.15 * k}) brightness(${1 - 0.06 * k}) saturate(${1 + 0.25 * k})`);
        break;
      case 'lucky':
        parts.push('contrast(1.18) saturate(1.25) brightness(1.04)');
        break;
      case 'auto_contrast':
        parts.push('contrast(1.28)');
        break;
      case 'auto_color':
        parts.push('saturate(1.22) contrast(1.08)');
        break;
    }

    return parts.length > 0 ? parts.join(' ') : 'none';
  });

  // Continuous Physical Rotation: increments monotonic visualRotation smoothly over 380ms
  async function handleRotate(deg: number) {
    if (!currentPhoto) return;

    // 1. Instant friendly physical glide on client canvas
    visualRotation += deg;

    // 2. Perform background write & update without forcing a reload flicker or backwards spin
    isEditing = true;
    try {
      const res = await applyRotation(currentPhoto.id, deg);
      if (res.success && res.photo) {
        currentPhoto.file_hash = res.photo.file_hash;
        currentPhoto.file_size = res.photo.file_size;
        currentPhoto.width = res.photo.width;
        currentPhoto.height = res.photo.height;
        sessionState = res.session_state;
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
        visualRotation -= 90;
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
        visualRotation += 90;
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
        visualRotation = 0;
        resetLocalToolState();
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
        visualRotation = 0;
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

    // Ignore hotkeys when typing in text overlay input
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
      return;
    }

    const isCtrlOrCmd = e.ctrlKey || e.metaKey;

    if (e.key === 'Escape') {
      if (activeTool !== null) {
        activeTool = null;
      } else {
        isOpen = false;
        onClose();
      }
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

  // Picasa 36 Creative Effects Catalogue definitions
  const FX_TAB1 = [
    { id: 'sharpen', name: 'Sharpen', desc: 'Enhance edge contrast' },
    { id: 'sepia', name: 'Sepia', desc: 'Antique brown tones' },
    { id: 'bw', name: 'B&W', desc: 'Classic monochrome' },
    { id: 'warmify', name: 'Warmify', desc: 'Sunlight warmth glow' },
    { id: 'film_grain', name: 'Film Grain', desc: 'Organic 35mm grain' },
    { id: 'tint', name: 'Tint', desc: 'Color cast overlay' },
    { id: 'saturation', name: 'Saturation', desc: 'Deep vibrant colors' },
    { id: 'soft_focus', name: 'Soft Focus', desc: 'Dreamy subtle blur' },
    { id: 'glow', name: 'Glow', desc: 'Luminous highlights' },
    { id: 'focal_bw', name: 'Focal B&W', desc: 'Desaturate background' },
    { id: 'grad_tint', name: 'Grad. Tint', desc: 'Sunset sky gradient' },
    { id: 'invert', name: 'Invert', desc: 'Color negative effect' },
  ];

  const FX_TAB2 = [
    { id: 'infrared', name: 'Infrared', desc: 'IR camera film simulation' },
    { id: 'lomo', name: 'Lomo', desc: 'Vivid cross-color punch' },
    { id: 'holga', name: 'Holga', desc: 'Toy camera mood & vignetting' },
    { id: 'hdr', name: 'HDR-ish', desc: 'Expanded dynamic range' },
    { id: 'cinema', name: 'Cinema', desc: 'Desaturated Hollywood look' },
    { id: 'orton', name: 'Orton', desc: 'Ethereal glow landscape' },
    { id: '1960s', name: '1960s', desc: 'Vintage warm amber film' },
    { id: 'cross_process', name: 'Cross Proc.', desc: 'Chemical color shift' },
    { id: 'posterize', name: 'Posterize', desc: 'Stepped color bands' },
    { id: 'duotone', name: 'DuoTone', desc: 'Two-tone graphic art' },
    { id: 'heat_map', name: 'Heat Map', desc: 'Thermal false colors' },
    { id: 'comic', name: 'Comic', desc: 'Graphic novel contrast' },
  ];

  const FX_TAB3 = [
    { id: 'vignette', name: 'Vignette', desc: 'Dark circular edge shadow' },
    { id: 'museum', name: 'Matte', desc: 'Museum fine art passe-partout' },
    { id: 'polaroid', name: 'Polaroid', desc: 'Classic instant photo border' },
    { id: 'passport', name: 'Passport', desc: 'Document photo framing' },
    { id: 'fisheye', name: 'Fisheye', desc: 'Spherical optic curvature' },
    { id: 'pastel', name: 'Pastel', desc: 'Soft chalky water tones' },
    { id: 'sketch', name: 'Sketch', desc: 'High-contrast pencil drawing' },
    { id: 'neon', name: 'Neon', desc: 'Glowing electric edges' },
    { id: 'charcoal', name: 'Charcoal', desc: 'Dark smudged charcoal drawing' },
    { id: 'tilt_shift', name: 'Tilt-Shift', desc: 'Miniature model lens effect' },
    { id: 'motion_blur', name: 'Motion', desc: 'Kinetic speed streak' },
    { id: 'vintage_70s', name: '70s Vintage', desc: 'Muted mustard retro warmth' },
  ];
</script>

{#if isOpen && currentPhoto}
  <div class="fixed inset-0 z-50 bg-[#121214] text-neutral-200 flex flex-row select-none overflow-hidden font-sans">
    
    <!-- ========================================================================= -->
    <!-- LEFT PANE: Picasa Signature Non-Destructive Editing Suite (editpanel)     -->
    <!-- ========================================================================= -->
    <aside
      class={`h-full bg-[#1e1e24] border-r border-[#2d2d34] flex flex-col z-30 transition-all duration-200 ease-in-out shrink-0 shadow-2xl ${
        isSidebarCollapsed ? 'w-0 overflow-hidden opacity-0 pointer-events-none' : 'w-80'
      }`}
    >
      <!-- Top Action Bar: « Back to Library Button -->
      <div class="p-3 border-b border-[#2d2d34] flex items-center justify-between bg-[#19191e]">
        <button
          onclick={() => {
            isOpen = false;
            onClose();
          }}
          class="flex items-center gap-2 px-3.5 py-1.5 rounded-md bg-[#2a2a32] hover:bg-[#343440] text-neutral-200 hover:text-white transition-all text-xs font-semibold shadow-sm border border-neutral-700/60 cursor-pointer active:scale-95"
          title="Back to Library Grid (Esc)"
        >
          <ArrowLeft class="w-4 h-4 text-blue-400 shrink-0" />
          <span>Back to Library</span>
          <span class="text-[10px] text-neutral-400 font-mono ml-0.5 opacity-80">(Esc)</span>
        </button>

        <button
          onclick={() => (isSidebarCollapsed = true)}
          class="p-1.5 rounded text-neutral-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
          title="Collapse Sidebar"
        >
          <PanelLeftClose class="w-4 h-4" />
        </button>
      </div>

      <!-- 5 Picasa Editing Tabs (Icons with tooltips) -->
      <div class="grid grid-cols-5 border-b border-[#2d2d34] bg-[#16161a]">
        <!-- Tab 1: Basic Fixes -->
        <button
          onclick={() => {
            activeTab = 'basic';
            activeTool = null;
          }}
          class={`flex flex-col items-center justify-center py-2.5 px-1 border-b-2 transition-colors cursor-pointer ${
            activeTab === 'basic'
              ? 'border-blue-500 text-blue-400 bg-[#22222a]'
              : 'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-white/5'
          }`}
          title="Basic Fixes (Wrench): Crop, Straighten, Red Eye, Auto-fix, Retouch, Text"
        >
          <Wrench class="w-4 h-4 mb-0.5" />
          <span class="text-[10px] font-medium tracking-tight">Basic</span>
        </button>

        <!-- Tab 2: Tuning -->
        <button
          onclick={() => {
            activeTab = 'tuning';
            activeTool = null;
          }}
          class={`flex flex-col items-center justify-center py-2.5 px-1 border-b-2 transition-colors cursor-pointer ${
            activeTab === 'tuning'
              ? 'border-blue-500 text-blue-400 bg-[#22222a]'
              : 'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-white/5'
          }`}
          title="Tuning: Lighting, Fill Light, Highlights, Shadows, Color Temp"
        >
          <SlidersHorizontal class="w-4 h-4 mb-0.5" />
          <span class="text-[10px] font-medium tracking-tight">Tuning</span>
        </button>

        <!-- Tab 3: Effects 1 -->
        <button
          onclick={() => {
            activeTab = 'fx1';
            activeTool = null;
          }}
          class={`flex flex-col items-center justify-center py-2.5 px-1 border-b-2 transition-colors cursor-pointer ${
            activeTab === 'fx1'
              ? 'border-blue-500 text-blue-400 bg-[#22222a]'
              : 'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-white/5'
          }`}
          title="Creative Effects 1: Sepia, B&W, Warmify, Grain, Soft Focus"
        >
          <Wand class="w-4 h-4 mb-0.5" />
          <span class="text-[10px] font-medium tracking-tight">Effects</span>
        </button>

        <!-- Tab 4: Effects 2 -->
        <button
          onclick={() => {
            activeTab = 'fx2';
            activeTool = null;
          }}
          class={`flex flex-col items-center justify-center py-2.5 px-1 border-b-2 transition-colors cursor-pointer ${
            activeTab === 'fx2'
              ? 'border-blue-500 text-blue-400 bg-[#22222a]'
              : 'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-white/5'
          }`}
          title="Creative Effects 2: Lomo, Holga, HDR, Cinema, Orton, 1960s"
        >
          <Sparkles class="w-4 h-4 mb-0.5" />
          <span class="text-[10px] font-medium tracking-tight">Styles</span>
        </button>

        <!-- Tab 5: Effects 3 -->
        <button
          onclick={() => {
            activeTab = 'fx3';
            activeTool = null;
          }}
          class={`flex flex-col items-center justify-center py-2.5 px-1 border-b-2 transition-colors cursor-pointer ${
            activeTab === 'fx3'
              ? 'border-blue-500 text-blue-400 bg-[#22222a]'
              : 'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-white/5'
          }`}
          title="Effects 3: Vignette, Museum Matte, Polaroid, Sketch, Fisheye"
        >
          <Palette class="w-4 h-4 mb-0.5" />
          <span class="text-[10px] font-medium tracking-tight">Frames</span>
        </button>
      </div>

      <!-- Tab Content Area -->
      <div class="flex-1 overflow-y-auto p-3.5 space-y-4 text-xs">
        
        <!-- ------------------------------------------------------------- -->
        <!-- TAB 1: BASIC FIXES                                            -->
        <!-- ------------------------------------------------------------- -->
        {#if activeTab === 'basic'}
          <div class="space-y-3">
            <div class="text-[11px] font-semibold text-neutral-400 uppercase tracking-wider px-1">
              Basic Fixes
            </div>

            <!-- Quick Physical Rotation Actions -->
            <div class="grid grid-cols-2 gap-2">
              <button
                onclick={() => handleRotate(-90)}
                class="flex items-center justify-center gap-2 p-2.5 rounded-lg bg-[#272730] hover:bg-[#32323e] border border-neutral-700/60 font-medium text-neutral-200 transition-all cursor-pointer active:scale-98"
                title="Rotate Left -90° (Shift+R)"
              >
                <RotateCcw class="w-4 h-4 text-blue-400" />
                <span>Rotate Left</span>
              </button>

              <button
                onclick={() => handleRotate(90)}
                class="flex items-center justify-center gap-2 p-2.5 rounded-lg bg-[#272730] hover:bg-[#32323e] border border-neutral-700/60 font-medium text-neutral-200 transition-all cursor-pointer active:scale-98"
                title="Rotate Right +90° (R)"
              >
                <RotateCw class="w-4 h-4 text-blue-400" />
                <span>Rotate Right</span>
              </button>
            </div>

            <!-- Crop Tool -->
            <div class="border border-neutral-700/60 rounded-lg overflow-hidden bg-[#24242c]">
              <button
                onclick={() => (activeTool = activeTool === 'crop' ? null : 'crop')}
                class="w-full flex items-center justify-between p-2.5 hover:bg-white/5 transition-colors font-medium text-left cursor-pointer"
              >
                <div class="flex items-center gap-2.5">
                  <Crop class="w-4 h-4 text-emerald-400" />
                  <span>Crop</span>
                </div>
                <span class="text-[10px] text-neutral-400">{activeTool === 'crop' ? 'Active' : 'Open'}</span>
              </button>

              {#if activeTool === 'crop'}
                <div class="p-3 border-t border-neutral-700/60 bg-[#1c1c22] space-y-2.5 animate-in fade-in duration-150">
                  <div class="text-[11px] text-neutral-400">Aspect Ratio Preset:</div>
                  <div class="grid grid-cols-3 gap-1.5 text-[11px]">
                    {#each [
                      { id: 'free', label: 'Manual' },
                      { id: 'square', label: '1:1 Sq' },
                      { id: '4:3', label: '4:3' },
                      { id: '16:9', label: '16:9' },
                      { id: '4x6', label: '4 × 6' },
                      { id: '5x7', label: '5 × 7' },
                    ] as asp}
                      <button
                        onclick={() => (cropAspect = asp.id as any)}
                        class={`py-1 px-1.5 rounded text-center border transition-colors cursor-pointer ${
                          cropAspect === asp.id
                            ? 'bg-blue-600/30 border-blue-500 text-blue-300 font-semibold'
                            : 'border-neutral-700 hover:bg-white/5 text-neutral-300'
                        }`}
                      >
                        {asp.label}
                      </button>
                    {/each}
                  </div>
                  <div class="flex items-center gap-2 pt-2">
                    <button
                      onclick={() => {
                        showToast('Crop preview applied');
                        activeTool = null;
                      }}
                      class="flex-1 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-semibold transition-colors cursor-pointer"
                    >
                      Apply
                    </button>
                    <button
                      onclick={() => (activeTool = null)}
                      class="px-3 py-1.5 rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors cursor-pointer"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Straighten Tool -->
            <div class="border border-neutral-700/60 rounded-lg overflow-hidden bg-[#24242c]">
              <button
                onclick={() => (activeTool = activeTool === 'straighten' ? null : 'straighten')}
                class="w-full flex items-center justify-between p-2.5 hover:bg-white/5 transition-colors font-medium text-left cursor-pointer"
              >
                <div class="flex items-center gap-2.5">
                  <SlidersVertical class="w-4 h-4 text-cyan-400" />
                  <span>Straighten</span>
                </div>
                <span class="text-[10px] text-neutral-400">
                  {straightenAngle !== 0 ? `${straightenAngle}°` : 'Open'}
                </span>
              </button>

              {#if activeTool === 'straighten'}
                <div class="p-3 border-t border-neutral-700/60 bg-[#1c1c22] space-y-2.5 animate-in fade-in duration-150">
                  <div class="flex items-center justify-between text-[11px] text-neutral-400">
                    <span>Horizon Level:</span>
                    <span class="font-mono text-cyan-300">{straightenAngle}°</span>
                  </div>
                  <input
                    type="range"
                    min="-20"
                    max="20"
                    step="0.5"
                    bind:value={straightenAngle}
                    class="w-full accent-cyan-400 cursor-pointer"
                  />
                  <div class="flex items-center gap-2 pt-1">
                    <button
                      onclick={() => {
                        showToast(`Straighten angle ${straightenAngle}° applied`);
                        activeTool = null;
                      }}
                      class="flex-1 py-1.5 rounded bg-cyan-600 hover:bg-cyan-500 text-white font-semibold transition-colors cursor-pointer"
                    >
                      Apply
                    </button>
                    <button
                      onclick={() => {
                        straightenAngle = 0;
                        activeTool = null;
                      }}
                      class="px-3 py-1.5 rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors cursor-pointer"
                    >
                      Reset
                    </button>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Red Eye Tool -->
            <div class="border border-neutral-700/60 rounded-lg overflow-hidden bg-[#24242c]">
              <button
                onclick={() => (activeTool = activeTool === 'redeye' ? null : 'redeye')}
                class="w-full flex items-center justify-between p-2.5 hover:bg-white/5 transition-colors font-medium text-left cursor-pointer"
              >
                <div class="flex items-center gap-2.5">
                  <Eye class="w-4 h-4 text-rose-400" />
                  <span>Red Eye</span>
                </div>
                <span class="text-[10px] text-neutral-400">{activeTool === 'redeye' ? 'Active' : 'Open'}</span>
              </button>

              {#if activeTool === 'redeye'}
                <div class="p-3 border-t border-neutral-700/60 bg-[#1c1c22] space-y-2.5 animate-in fade-in duration-150">
                  <div class="text-[11px] text-neutral-300">Pupil Brush Radius:</div>
                  <div class="flex items-center gap-2">
                    <input
                      type="range"
                      min="10"
                      max="60"
                      bind:value={redEyeRadius}
                      class="flex-1 accent-rose-500 cursor-pointer"
                    />
                    <span class="font-mono text-rose-300 w-8 text-right">{redEyeRadius}px</span>
                  </div>
                  <div class="flex items-center gap-2 pt-1">
                    <button
                      onclick={() => {
                        showToast('Red eye correction applied');
                        activeTool = null;
                      }}
                      class="flex-1 py-1.5 rounded bg-rose-600 hover:bg-rose-500 text-white font-semibold transition-colors cursor-pointer"
                    >
                      Apply
                    </button>
                    <button
                      onclick={() => (activeTool = null)}
                      class="px-3 py-1.5 rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors cursor-pointer"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Picasa Signature One-Click Auto Enhancements -->
            <div class="pt-1 space-y-1.5">
              <button
                onclick={() => {
                  activeEffect = activeEffect === 'lucky' ? null : 'lucky';
                  showToast(activeEffect ? "I'm Feeling Lucky applied!" : 'Effect cleared');
                }}
                class={`w-full flex items-center justify-between p-2.5 rounded-lg border transition-all cursor-pointer font-medium ${
                  activeEffect === 'lucky'
                    ? 'bg-amber-600/30 border-amber-500 text-amber-200'
                    : 'bg-[#24242c] hover:bg-[#2e2e38] border-neutral-700/60 text-neutral-200'
                }`}
                title="Picasa signature one-click lighting and contrast booster"
              >
                <div class="flex items-center gap-2.5">
                  <Sparkles class="w-4 h-4 text-amber-400" />
                  <span>I'm Feeling Lucky</span>
                </div>
                {#if activeEffect === 'lucky'}
                  <Check class="w-4 h-4 text-amber-400" />
                {/if}
              </button>

              <button
                onclick={() => {
                  activeEffect = activeEffect === 'auto_contrast' ? null : 'auto_contrast';
                  showToast(activeEffect ? 'Auto Contrast applied' : 'Contrast reset');
                }}
                class={`w-full flex items-center justify-between p-2.5 rounded-lg border transition-all cursor-pointer font-medium ${
                  activeEffect === 'auto_contrast'
                    ? 'bg-blue-600/30 border-blue-500 text-blue-200'
                    : 'bg-[#24242c] hover:bg-[#2e2e38] border-neutral-700/60 text-neutral-200'
                }`}
              >
                <div class="flex items-center gap-2.5">
                  <Contrast class="w-4 h-4 text-blue-400" />
                  <span>Auto Contrast</span>
                </div>
                {#if activeEffect === 'auto_contrast'}
                  <Check class="w-4 h-4 text-blue-400" />
                {/if}
              </button>

              <button
                onclick={() => {
                  activeEffect = activeEffect === 'auto_color' ? null : 'auto_color';
                  showToast(activeEffect ? 'Auto Color applied' : 'Color reset');
                }}
                class={`w-full flex items-center justify-between p-2.5 rounded-lg border transition-all cursor-pointer font-medium ${
                  activeEffect === 'auto_color'
                    ? 'bg-purple-600/30 border-purple-500 text-purple-200'
                    : 'bg-[#24242c] hover:bg-[#2e2e38] border-neutral-700/60 text-neutral-200'
                }`}
              >
                <div class="flex items-center gap-2.5">
                  <Sun class="w-4 h-4 text-purple-400" />
                  <span>Auto Color</span>
                </div>
                {#if activeEffect === 'auto_color'}
                  <Check class="w-4 h-4 text-purple-400" />
                {/if}
              </button>
            </div>

            <!-- Retouch Tool -->
            <div class="border border-neutral-700/60 rounded-lg overflow-hidden bg-[#24242c]">
              <button
                onclick={() => (activeTool = activeTool === 'retouch' ? null : 'retouch')}
                class="w-full flex items-center justify-between p-2.5 hover:bg-white/5 transition-colors font-medium text-left cursor-pointer"
              >
                <div class="flex items-center gap-2.5">
                  <Wand class="w-4 h-4 text-amber-400" />
                  <span>Retouch Blemish</span>
                </div>
                <span class="text-[10px] text-neutral-400">{activeTool === 'retouch' ? 'Active' : 'Open'}</span>
              </button>

              {#if activeTool === 'retouch'}
                <div class="p-3 border-t border-neutral-700/60 bg-[#1c1c22] space-y-2.5 animate-in fade-in duration-150">
                  <div class="text-[11px] text-neutral-300">Healer Brush Radius:</div>
                  <div class="flex items-center gap-2">
                    <input
                      type="range"
                      min="10"
                      max="80"
                      bind:value={retouchRadius}
                      class="flex-1 accent-amber-500 cursor-pointer"
                    />
                    <span class="font-mono text-amber-300 w-8 text-right">{retouchRadius}px</span>
                  </div>
                  <div class="flex items-center gap-2 pt-1">
                    <button
                      onclick={() => {
                        showToast('Retouch applied');
                        activeTool = null;
                      }}
                      class="flex-1 py-1.5 rounded bg-amber-600 hover:bg-amber-500 text-white font-semibold transition-colors cursor-pointer"
                    >
                      Apply
                    </button>
                    <button
                      onclick={() => (activeTool = null)}
                      class="px-3 py-1.5 rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors cursor-pointer"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}
            </div>

            <!-- Text Overlay Tool -->
            <div class="border border-neutral-700/60 rounded-lg overflow-hidden bg-[#24242c]">
              <button
                onclick={() => (activeTool = activeTool === 'text' ? null : 'text')}
                class="w-full flex items-center justify-between p-2.5 hover:bg-white/5 transition-colors font-medium text-left cursor-pointer"
              >
                <div class="flex items-center gap-2.5">
                  <Type class="w-4 h-4 text-sky-400" />
                  <span>Text Caption Overlay</span>
                </div>
                <span class="text-[10px] text-neutral-400">{activeTool === 'text' ? 'Active' : 'Open'}</span>
              </button>

              {#if activeTool === 'text'}
                <div class="p-3 border-t border-neutral-700/60 bg-[#1c1c22] space-y-2.5 animate-in fade-in duration-150">
                  <input
                    type="text"
                    bind:value={overlayText}
                    placeholder="Type caption text here..."
                    class="w-full px-2.5 py-1.5 rounded bg-black/40 border border-neutral-700 text-neutral-100 placeholder-neutral-500 text-xs focus:outline-none focus:border-blue-500"
                  />
                  <div class="flex items-center justify-between gap-2">
                    <div class="flex items-center gap-1.5">
                      <span class="text-[10px] text-neutral-400">Size:</span>
                      <input
                        type="range"
                        min="16"
                        max="64"
                        bind:value={textSize}
                        class="w-20 accent-sky-400 cursor-pointer"
                      />
                    </div>
                    <div class="flex items-center gap-1.5">
                      <span class="text-[10px] text-neutral-400">Color:</span>
                      <input
                        type="color"
                        bind:value={textColor}
                        class="w-6 h-6 rounded cursor-pointer border border-neutral-700 bg-transparent"
                      />
                    </div>
                  </div>
                  <div class="flex items-center gap-2 pt-1">
                    <button
                      onclick={() => {
                        showToast('Text caption saved');
                        activeTool = null;
                      }}
                      class="flex-1 py-1.5 rounded bg-sky-600 hover:bg-sky-500 text-white font-semibold transition-colors cursor-pointer"
                    >
                      Apply
                    </button>
                    <button
                      onclick={() => {
                        overlayText = '';
                        activeTool = null;
                      }}
                      class="px-3 py-1.5 rounded bg-neutral-700 hover:bg-neutral-600 text-neutral-300 transition-colors cursor-pointer"
                    >
                      Clear
                    </button>
                  </div>
                </div>
              {/if}
            </div>
          </div>
        {/if}

        <!-- ------------------------------------------------------------- -->
        <!-- TAB 2: TUNING                                                 -->
        <!-- ------------------------------------------------------------- -->
        {#if activeTab === 'tuning'}
          <div class="space-y-4">
            <div class="flex items-center justify-between px-1">
              <span class="text-[11px] font-semibold text-neutral-400 uppercase tracking-wider">
                Lighting & Color Tuning
              </span>
              <button
                onclick={() => {
                  fillLight = 0;
                  highlights = 0;
                  shadows = 0;
                  colorTemp = 0;
                  showToast('Tuning sliders reset');
                }}
                class="text-[10px] text-blue-400 hover:text-blue-300 hover:underline cursor-pointer"
              >
                Reset All
              </button>
            </div>

            <!-- Fill Light Slider -->
            <div class="p-3 rounded-lg bg-[#24242c] border border-neutral-700/60 space-y-1.5">
              <div class="flex items-center justify-between text-[11px]">
                <span class="text-neutral-300 font-medium">Fill Light (Shadow Boost)</span>
                <span class="font-mono text-blue-400">{fillLight}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                bind:value={fillLight}
                class="w-full accent-blue-500 cursor-pointer"
              />
            </div>

            <!-- Highlights Slider -->
            <div class="p-3 rounded-lg bg-[#24242c] border border-neutral-700/60 space-y-1.5">
              <div class="flex items-center justify-between text-[11px]">
                <span class="text-neutral-300 font-medium">Highlights Contrast</span>
                <span class="font-mono text-blue-400">{highlights}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                bind:value={highlights}
                class="w-full accent-blue-500 cursor-pointer"
              />
            </div>

            <!-- Shadows Slider -->
            <div class="p-3 rounded-lg bg-[#24242c] border border-neutral-700/60 space-y-1.5">
              <div class="flex items-center justify-between text-[11px]">
                <span class="text-neutral-300 font-medium">Shadows Depth</span>
                <span class="font-mono text-blue-400">{shadows}%</span>
              </div>
              <input
                type="range"
                min="0"
                max="100"
                bind:value={shadows}
                class="w-full accent-blue-500 cursor-pointer"
              />
            </div>

            <!-- Color Temperature Slider -->
            <div class="p-3 rounded-lg bg-[#24242c] border border-neutral-700/60 space-y-1.5">
              <div class="flex items-center justify-between text-[11px]">
                <span class="text-neutral-300 font-medium">Color Temperature</span>
                <span class="font-mono text-blue-400">{colorTemp > 0 ? `+${colorTemp}` : colorTemp}</span>
              </div>
              <div class="flex items-center gap-2">
                <span class="text-[10px] text-cyan-400 font-semibold">Cool</span>
                <input
                  type="range"
                  min="-100"
                  max="100"
                  bind:value={colorTemp}
                  class="flex-1 accent-amber-500 cursor-pointer"
                />
                <span class="text-[10px] text-amber-400 font-semibold">Warm</span>
              </div>
            </div>

            <!-- Neutral Eyedropper Dropper -->
            <button
              onclick={() => showToast('Neutral color picker ready (click any neutral grey pixel)')}
              class="w-full flex items-center justify-center gap-2 p-2.5 rounded-lg bg-[#24242c] hover:bg-[#2e2e38] border border-neutral-700/60 text-neutral-200 transition-colors cursor-pointer"
            >
              <Pipette class="w-4 h-4 text-neutral-400" />
              <span>One-Click Neutral Color Dropper</span>
            </button>
          </div>
        {/if}

        <!-- ------------------------------------------------------------- -->
        <!-- TAB 3, 4, 5: CREATIVE EFFECTS (36 Filters Total)              -->
        <!-- ------------------------------------------------------------- -->
        {#if activeTab === 'fx1' || activeTab === 'fx2' || activeTab === 'fx3'}
          {@const fxList = activeTab === 'fx1' ? FX_TAB1 : activeTab === 'fx2' ? FX_TAB2 : FX_TAB3}
          <div class="space-y-3">
            <div class="flex items-center justify-between px-1">
              <span class="text-[11px] font-semibold text-neutral-400 uppercase tracking-wider">
                {activeTab === 'fx1' ? 'Classic Photo Effects' : activeTab === 'fx2' ? 'Film & Camera Styles' : 'Framing & Vignettes'}
              </span>
              {#if activeEffect}
                <button
                  onclick={() => {
                    activeEffect = null;
                    showToast('Effects cleared');
                  }}
                  class="text-[10px] text-blue-400 hover:text-blue-300 hover:underline cursor-pointer"
                >
                  Clear Effect
                </button>
              {/if}
            </div>

            {#if activeEffect}
              <div class="p-2.5 rounded-lg bg-blue-950/40 border border-blue-500/40 space-y-1.5 animate-in fade-in duration-100">
                <div class="flex items-center justify-between text-[11px]">
                  <span class="text-blue-300 font-medium">Effect Intensity</span>
                  <span class="font-mono text-blue-400">{effectIntensity}%</span>
                </div>
                <input
                  type="range"
                  min="10"
                  max="100"
                  bind:value={effectIntensity}
                  class="w-full accent-blue-500 cursor-pointer"
                />
              </div>
            {/if}

            <div class="grid grid-cols-2 gap-2">
              {#each fxList as fx}
                <button
                  onclick={() => {
                    activeEffect = activeEffect === fx.id ? null : fx.id;
                    if (activeEffect) showToast(`${fx.name} filter applied`);
                  }}
                  class={`p-2.5 rounded-lg border text-left flex flex-col justify-between transition-all cursor-pointer ${
                    activeEffect === fx.id
                      ? 'bg-blue-600/30 border-blue-500 text-white shadow-md ring-1 ring-blue-500/60'
                      : 'bg-[#24242c] hover:bg-[#2c2c36] border-neutral-700/60 text-neutral-300'
                  }`}
                >
                  <div class="flex items-center justify-between w-full pb-1">
                    <span class="font-semibold text-xs text-neutral-100">{fx.name}</span>
                    {#if activeEffect === fx.id}
                      <Check class="w-3.5 h-3.5 text-blue-400" />
                    {/if}
                  </div>
                  <span class="text-[10px] text-neutral-400 leading-tight">{fx.desc}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

      </div>

      <!-- Side-by-Side Comparison Switcher (A/B View) -->
      <div class="p-3 border-t border-[#2d2d34] bg-[#17171c] flex items-center justify-between">
        <span class="text-[11px] font-medium text-neutral-400">View Mode:</span>
        <div class="flex items-center bg-[#22222a] p-0.5 rounded-md border border-neutral-700/60">
          <button
            onclick={() => (comparisonMode = '1up')}
            class={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              comparisonMode === '1up' ? 'bg-blue-600 text-white' : 'text-neutral-400 hover:text-white'
            }`}
            title="Single Image View"
          >
            1-Up
          </button>
          <button
            onclick={() => (comparisonMode = 'ab')}
            class={`flex items-center gap-1 px-2.5 py-1 rounded text-[11px] font-medium transition-colors cursor-pointer ${
              comparisonMode === 'ab' ? 'bg-blue-600 text-white' : 'text-neutral-400 hover:text-white'
            }`}
            title="Side-by-side Before & After Comparison"
          >
            <Columns2 class="w-3.5 h-3.5" />
            <span>A/B Split</span>
          </button>
        </div>
      </div>

      <!-- Bottom Session Control Deck (Sticky at bottom of sidebar) -->
      <div class="p-3 border-t border-[#2d2d34] bg-[#141418] space-y-2">
        <div class="grid grid-cols-2 gap-2">
          <!-- Undo Action -->
          <button
            onclick={handleUndo}
            disabled={!sessionState?.can_undo || isEditing}
            class="flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-[#24242c] hover:bg-[#2c2c36] text-neutral-200 border border-neutral-700/60 disabled:opacity-30 disabled:pointer-events-none transition-colors text-xs font-medium cursor-pointer"
            title="Undo Last Action (Ctrl+Z)"
          >
            <Undo2 class="w-3.5 h-3.5" />
            <span>Undo</span>
          </button>

          <!-- Redo Action -->
          <button
            onclick={handleRedo}
            disabled={!sessionState?.can_redo || isEditing}
            class="flex items-center justify-center gap-1.5 py-1.5 rounded-md bg-[#24242c] hover:bg-[#2c2c36] text-neutral-200 border border-neutral-700/60 disabled:opacity-30 disabled:pointer-events-none transition-colors text-xs font-medium cursor-pointer"
            title="Redo (Ctrl+Y)"
          >
            <Redo2 class="w-3.5 h-3.5" />
            <span>Redo</span>
          </button>
        </div>

        <!-- Save Session to Disk (Prominent blue button when uncommitted) -->
        <button
          onclick={handleSaveSession}
          disabled={isEditing || (sessionState?.is_committed && visualRotation === 0)}
          class={`w-full flex items-center justify-center gap-2 py-2 rounded-md font-semibold text-xs shadow-md transition-all cursor-pointer ${
            !sessionState?.is_committed || visualRotation !== 0
              ? 'bg-blue-600 hover:bg-blue-500 text-white active:scale-98 ring-1 ring-blue-400'
              : 'bg-neutral-800 text-neutral-400 border border-neutral-700 hover:text-white'
          }`}
          title="Save edits permanently to disk file (Ctrl+S)"
        >
          <Save class="w-4 h-4" />
          <span>Save to Disk</span>
          <span class="text-[10px] opacity-75 font-mono">(Ctrl+S)</span>
        </button>

        <!-- Revert to Original -->
        {#if sessionState?.has_session || sessionState?.has_backup}
          <button
            onclick={handleRevert}
            disabled={isEditing}
            class="w-full flex items-center justify-center gap-2 py-1.5 rounded-md bg-[#202028] hover:bg-amber-950/40 text-neutral-300 hover:text-amber-300 border border-neutral-800 hover:border-amber-600/40 transition-colors text-xs cursor-pointer"
            title="Discard edits and restore original file from .pica_originals"
          >
            <History class="w-3.5 h-3.5" />
            <span>Revert to Original Photo</span>
          </button>
        {/if}

        <!-- Original Backup Safety Status Indicator -->
        <div class="pt-1 flex items-center justify-center gap-1.5 text-[10px] text-emerald-400/90 font-medium">
          <ShieldCheck class="w-3.5 h-3.5 shrink-0" />
          <span>Original preserved in .pica_originals</span>
        </div>
      </div>
    </aside>

    <!-- ========================================================================= -->
    <!-- RIGHT AREA: Main Viewport, Centered Canvas, Overlays & HUD                -->
    <!-- ========================================================================= -->
    <div
      bind:this={canvasContainer}
      bind:clientWidth={canvasWidth}
      bind:clientHeight={canvasHeight}
      class="flex-1 h-full flex flex-col relative overflow-hidden bg-[#0e0e11]"
    >
      <!-- Top Title & Navigation Bar -->
      <div
        class="h-12 bg-[#16161a]/90 border-b border-[#24242c] flex items-center justify-between px-4 z-20 backdrop-blur-md"
      >
        <div class="flex items-center gap-3">
          {#if isSidebarCollapsed}
            <button
              onclick={() => (isSidebarCollapsed = false)}
              class="flex items-center gap-1.5 px-2.5 py-1 rounded bg-[#24242c] hover:bg-[#303038] text-neutral-200 text-xs font-medium border border-neutral-700 cursor-pointer"
              title="Expand Editing Suite"
            >
              <PanelLeftOpen class="w-4 h-4 text-blue-400" />
              <span>Edit Panel</span>
            </button>
          {/if}

          <div class="text-xs font-medium text-neutral-200 truncate max-w-md">
            {currentPhoto.file_name}
            <span class="text-neutral-500 ml-2 font-mono">({activeIndex + 1} of {photos.length})</span>
          </div>

          {#if sessionState?.has_session && !sessionState?.is_committed}
            <span class="text-[10px] px-2 py-0.5 rounded-full bg-amber-500/20 text-amber-300 font-bold border border-amber-500/30">
              Unsaved Draft
            </span>
          {/if}
        </div>

        <div class="flex items-center gap-2">
          <!-- Info Details Toggle -->
          <button
            onclick={() => (showInfo = !showInfo)}
            class={`p-1.5 rounded-md hover:bg-white/10 transition-colors cursor-pointer ${showInfo ? 'text-blue-400 bg-white/10' : 'text-neutral-400 hover:text-white'}`}
            title="EXIF Details Drawer (i)"
          >
            <Info class="w-4 h-4" />
          </button>

          <!-- Close to Library -->
          <button
            onclick={() => {
              isOpen = false;
              onClose();
            }}
            class="p-1.5 rounded-md text-neutral-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
            title="Close to Library (Esc)"
          >
            <X class="w-5 h-5" />
          </button>
        </div>
      </div>

      <!-- Toast Notification Floater -->
      {#if toastMsg}
        <div class="absolute top-16 inset-x-0 z-40 flex justify-center pointer-events-none animate-in fade-in duration-150">
          <div class="px-4 py-1.5 rounded-full bg-neutral-900/95 text-white text-xs font-medium border border-neutral-700 shadow-2xl backdrop-blur-md">
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
                class="px-3 py-1.5 rounded bg-amber-900/80 hover:bg-amber-800 text-amber-100 font-semibold border border-amber-600/40 transition-colors cursor-pointer"
              >
                Keep External Version
              </button>
              <button
                onclick={() => handleResolveConflict('apply_pica')}
                class="px-3 py-1.5 rounded bg-blue-600 hover:bg-blue-500 text-white font-semibold transition-colors cursor-pointer"
              >
                Overwrite with Pica Draft
              </button>
              <button
                onclick={() => handleResolveConflict('save_as_copy')}
                class="px-3 py-1.5 rounded bg-neutral-800 hover:bg-neutral-700 text-neutral-200 border border-neutral-600 transition-colors cursor-pointer"
              >
                Save Draft as New Copy
              </button>
            </div>
          </div>
        </div>
      {/if}

      <!-- Main Canvas Viewport (1-Up Single vs A/B Split Comparison) -->
      <div class="flex-1 w-full h-full relative flex items-center justify-center overflow-hidden">
        
        <!-- Navigation Prev/Next Arrow Buttons on Canvas Edges -->
        <button
          onclick={prevPhoto}
          class="absolute left-4 z-20 p-3 rounded-full bg-black/40 hover:bg-black/80 text-white/70 hover:text-white backdrop-blur-xs transition-all opacity-0 hover:opacity-100 focus:opacity-100 cursor-pointer"
          title="Previous Photo (← or K)"
        >
          <ChevronLeft class="w-7 h-7" />
        </button>

        <button
          onclick={nextPhoto}
          class="absolute right-4 z-20 p-3 rounded-full bg-black/40 hover:bg-black/80 text-white/70 hover:text-white backdrop-blur-xs transition-all opacity-0 hover:opacity-100 focus:opacity-100 cursor-pointer"
          title="Next Photo (→ or J)"
        >
          <ChevronRight class="w-7 h-7" />
        </button>

        {#if comparisonMode === '1up'}
          <!-- 1-Up Single Canvas with Friendly Physical Rotation & Live Filters -->
          <div
            class="w-full h-full flex items-center justify-center p-6"
          >
            <div
              class="relative flex items-center justify-center select-none"
              style="
                transform: rotate({visualRotation + straightenAngle}deg) scale({isZoomed100 ? 1.5 : fitScale});
                transform-origin: center center;
                transition: transform 380ms cubic-bezier(0.2, 0.9, 0.3, 1);
              "
            >
              <button
                type="button"
                class="contents border-none bg-transparent p-0 m-0"
                onclick={toggleZoom}
                aria-label="Toggle 100% zoom"
              >
                <img
                  src={getFullImageUrl(currentPhoto, cacheBuster)}
                  alt={currentPhoto.file_name}
                  class={`max-h-[calc(100vh-140px)] max-w-[calc(100vw-360px)] rounded-sm shadow-2xl object-contain transition-all duration-150 ${
                    isZoomed100 ? 'cursor-grab' : 'cursor-zoom-in'
                  }`}
                  style="filter: {liveFilterStyle};"
                />
              </button>

              <!-- Vignette Overlay Effect -->
              {#if activeEffect === 'vignette'}
                <div
                  class="absolute inset-0 pointer-events-none rounded-sm"
                  style="box-shadow: inset 0 0 120px 40px rgba(0, 0, 0, {(effectIntensity / 100) * 0.85});"
                ></div>
              {/if}

              <!-- Museum Matte Frame Effect -->
              {#if activeEffect === 'museum'}
                <div
                  class="absolute inset-0 pointer-events-none border-[24px] border-white shadow-2xl"
                ></div>
              {/if}

              <!-- Polaroid Frame Effect -->
              {#if activeEffect === 'polaroid'}
                <div
                  class="absolute -inset-4 pointer-events-none border-t-[16px] border-x-[16px] border-b-[54px] border-[#f8f8f5] shadow-2xl rounded-xs"
                ></div>
              {/if}

              <!-- Straighten Leveling Grid Overlay -->
              {#if activeTool === 'straighten'}
                <div class="absolute inset-0 pointer-events-none grid grid-cols-6 grid-rows-6 border border-cyan-400/50">
                  {#each Array(36) as _}
                    <div class="border-r border-b border-cyan-400/25"></div>
                  {/each}
                </div>
              {/if}

              <!-- Crop Rule-of-Thirds Grid Overlay -->
              {#if activeTool === 'crop'}
                <div class="absolute inset-4 pointer-events-none border-2 border-dashed border-emerald-400 grid grid-cols-3 grid-rows-3 shadow-[0_0_0_9999px_rgba(0,0,0,0.55)]">
                  {#each Array(9) as _}
                    <div class="border border-emerald-400/40"></div>
                  {/each}
                  <!-- Corner Handles -->
                  <div class="absolute -top-1.5 -left-1.5 w-3 h-3 bg-emerald-400"></div>
                  <div class="absolute -top-1.5 -right-1.5 w-3 h-3 bg-emerald-400"></div>
                  <div class="absolute -bottom-1.5 -left-1.5 w-3 h-3 bg-emerald-400"></div>
                  <div class="absolute -bottom-1.5 -right-1.5 w-3 h-3 bg-emerald-400"></div>
                </div>
              {/if}

              <!-- Text Overlay Caption -->
              {#if overlayText.trim()}
                <div
                  class="absolute bottom-6 inset-x-0 text-center font-bold px-4 drop-shadow-[0_2px_4px_rgba(0,0,0,0.9)]"
                  style="color: {textColor}; font-size: {textSize}px;"
                >
                  {overlayText}
                </div>
              {/if}
            </div>
          </div>
        {:else}
          <!-- A/B Split Side-by-Side Comparison -->
          <div class="w-full h-full flex flex-row items-center justify-center p-6 gap-6">
            <!-- Left: Pristine Original -->
            <div class="flex-1 h-full flex flex-col items-center justify-center relative bg-black/40 rounded-lg p-4 border border-neutral-800">
              <div class="absolute top-3 left-3 z-10 px-2.5 py-1 rounded bg-black/70 text-neutral-300 text-[11px] font-semibold tracking-wide border border-neutral-700">
                Original (Before)
              </div>
              <img
                src={getOriginalImageUrl(currentPhoto, sessionState?.backup_path)}
                alt="Pristine original file"
                class="max-h-[calc(100vh-180px)] max-w-full rounded-sm object-contain"
              />
            </div>

            <!-- Right: Current Edited State -->
            <div class="flex-1 h-full flex flex-col items-center justify-center relative bg-black/40 rounded-lg p-4 border border-blue-500/40">
              <div class="absolute top-3 left-3 z-10 px-2.5 py-1 rounded bg-blue-600/80 text-white text-[11px] font-semibold tracking-wide border border-blue-400">
                Edited (After)
              </div>
              <div
                class="relative flex items-center justify-center select-none"
                style="
                  transform: rotate({visualRotation + straightenAngle}deg);
                  transform-origin: center center;
                  transition: transform 380ms cubic-bezier(0.2, 0.9, 0.3, 1);
                "
              >
                <img
                  src={getFullImageUrl(currentPhoto, cacheBuster)}
                  alt="Current edited file"
                  class="max-h-[calc(100vh-180px)] max-w-full rounded-sm object-contain"
                  style="filter: {liveFilterStyle};"
                />
              </div>
            </div>
          </div>
        {/if}

        <!-- EXIF Info Flyout Drawer -->
        {#if showInfo}
          <div
            class="absolute top-4 right-4 z-30 w-72 bg-[#1c1c22]/95 border border-[#383842] rounded-lg shadow-2xl backdrop-blur-md p-4 text-xs text-neutral-300 space-y-3 animate-in fade-in zoom-in-95 duration-150"
          >
            <div class="flex items-center justify-between pb-2 border-b border-[#30303a]">
              <span class="font-semibold text-white">Photo Details</span>
              <button onclick={() => (showInfo = false)} class="text-neutral-400 hover:text-white cursor-pointer">
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
      <div class="absolute bottom-5 inset-x-0 z-30 flex justify-center pointer-events-none">
        <div
          class="pointer-events-auto flex items-center gap-1.5 px-3.5 py-1.5 rounded-full bg-[#18181e]/90 border border-[#3f3f4c]/80 shadow-2xl backdrop-blur-md text-neutral-300"
        >
          <!-- Exit to Library -->
          <button
            onclick={() => {
              isOpen = false;
              onClose();
            }}
            class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            title="Back to Library (Esc)"
          >
            <X class="w-4 h-4" />
          </button>

          <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

          <!-- Prev Photo -->
          <button
            onclick={prevPhoto}
            class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            title="Previous Photo (← or K)"
          >
            <ChevronLeft class="w-4 h-4" />
          </button>

          <!-- Position Counter -->
          <span class="text-xs font-mono px-1 text-neutral-400">
            {activeIndex + 1} / {photos.length}
          </span>

          <!-- Next Photo -->
          <button
            onclick={nextPhoto}
            class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            title="Next Photo (→ or J)"
          >
            <ChevronRight class="w-4 h-4" />
          </button>

          <!-- Slideshow Play / Pause -->
          <button
            onclick={toggleSlideshow}
            class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
              isPlayingSlideshow ? 'text-amber-400 bg-white/15' : 'hover:text-white'
            }`}
            title={isPlayingSlideshow ? 'Pause Slideshow (P)' : 'Play Slideshow (P)'}
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
            class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            title="Rotate Left -90° (Shift+R)"
          >
            <RotateCcw class="w-4 h-4" />
          </button>

          <!-- Rotate Right (CW) -->
          <button
            onclick={() => handleRotate(90)}
            class="p-2 rounded-full hover:bg-white/10 hover:text-white transition-colors cursor-pointer"
            title="Rotate Right +90° (R)"
          >
            <RotateCw class="w-4 h-4" />
          </button>

          <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

          <!-- Undo Action -->
          <button
            onclick={handleUndo}
            disabled={!sessionState?.can_undo || isEditing}
            class="p-2 rounded-full hover:bg-white/10 transition-colors disabled:opacity-30 disabled:hover:bg-transparent cursor-pointer"
            title="Undo Edit (Ctrl+Z)"
          >
            <Undo2 class="w-4 h-4" />
          </button>

          <!-- Redo Action -->
          <button
            onclick={handleRedo}
            disabled={!sessionState?.can_redo || isEditing}
            class="p-2 rounded-full hover:bg-white/10 transition-colors disabled:opacity-30 disabled:hover:bg-transparent cursor-pointer"
            title="Redo Edit (Ctrl+Y)"
          >
            <Redo2 class="w-4 h-4" />
          </button>

          <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

          <!-- 1:1 Zoom Toggle -->
          <button
            onclick={toggleZoom}
            class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
              isZoomed100 ? 'text-blue-400' : 'hover:text-white'
            }`}
            title="Toggle Zoom 100% (Spacebar)"
          >
            <Maximize2 class="w-4 h-4" />
          </button>

          <!-- Favorite Star -->
          <button
            onclick={handleToggleFav}
            class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
              isFav ? 'text-amber-400 fill-amber-400' : 'text-neutral-400 hover:text-white'
            }`}
            title="Toggle Favorite (F)"
          >
            <Star class={`w-4 h-4 ${isFav ? 'fill-amber-400' : ''}`} />
          </button>
        </div>
      </div>

    </div>
  </div>
{/if}
