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
    Bug,
    FileCode2,
    Layers,
    Loader2,
    Users,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Bold,
    Italic,
    Underline,
    Expand,
  } from '@lucide/svelte';
  import type { CropSuggestion, EditSessionState, FaceBox, Photo, PhotoMetadata, RedEyeBox, RetouchSpot, StraightenSuggestion, TextItem } from '../types';
  import {
    getFullImageUrl,
    getOriginalImageUrl,
    getPreviewImageUrl,
    getThumbnailUrl,
    toggleFavorite,
    getEditSession,
    applyRotation,
    applyCrop,
    applyStraighten,
    detectRedEyes,
    applyRedEyeStep,
    applyRetouchStep,
    applyTextStep,
    applyFeelingLucky,
    applyAutoContrast,
    applyAutoColor,
    applyFillLight,
    getCropSuggestions,
    getStraightenSuggestions,
    undoEdit,
    redoEdit,
    saveEditSession,
    revertEditSession,
    resolveEditConflict,
    jumpToEditStep,
    getCachedEditSession,
    getFacesForPhoto,
    getPhotoMetadata,
    openViewerWindow,
  } from '../api';
  import Histogram from './Histogram.svelte';
  import PropertiesSidebar from './PropertiesSidebar.svelte';
  import { showEditOverlay, showFaceDebugOverlay } from '../debugStore';
  import cropPicasaIcon from '../../assets/picasa/crop.png';
  import cropPicasaIcon2x from '../../assets/picasa/crop@2x.png';
  import straightenPicasaIcon from '../../assets/picasa/straighten.png';
  import straightenPicasaIcon2x from '../../assets/picasa/straighten@2x.png';
  import redeyePicasaIcon from '../../assets/picasa/redeye.png';
  import redeyePicasaIcon2x from '../../assets/picasa/redeye@2x.png';
  import luckyPicasaIcon from '../../assets/picasa/lucky.png';
  import luckyPicasaIcon2x from '../../assets/picasa/lucky@2x.png';
  import contrastPicasaIcon from '../../assets/picasa/contrast.png';
  import contrastPicasaIcon2x from '../../assets/picasa/contrast@2x.png';
  import colorPicasaIcon from '../../assets/picasa/color.png';
  import colorPicasaIcon2x from '../../assets/picasa/color@2x.png';
  import retouchPicasaIcon from '../../assets/picasa/retouch.png';
  import retouchPicasaIcon2x from '../../assets/picasa/retouch@2x.png';
  import textPicasaIcon from '../../assets/picasa/text.png';
  import textPicasaIcon2x from '../../assets/picasa/text@2x.png';
  import fillLightPicasaIcon from '../../assets/picasa/fill_light.png';
  import fillLightPicasaIcon2x from '../../assets/picasa/fill_light@2x.png';

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
  let zoomLevel = $state(1.0);
  let isZoomed100 = $derived(zoomLevel > 1.05);
  let panX = $state(0);
  let panY = $state(0);
  let isPanning = $state(false);
  let isWheelZooming = $state(false);
  let wheelZoomTimer: ReturnType<typeof setTimeout> | null = null;
  let panStartPointer = { x: 0, y: 0 };
  let panStartOffset = { x: 0, y: 0 };
  let showInfo = $state(false);
  let isFav = $state(false);
  let isPlayingSlideshow = $state(false);
  let slideshowTimer: ReturnType<typeof setInterval> | null = null;

  // Face Detection State
  let faces = $state<FaceBox[]>([]);
  let showFaceBoxes = $state(false);

  async function loadFacesForPhoto(photoId: number) {
    try {
      faces = await getFacesForPhoto(photoId);
    } catch {
      faces = [];
    }
  }

  // Metadata & EXIF State (Info for Nerds)
  let photoMetadata = $state<PhotoMetadata | null>(null);

  async function loadMetadataForPhoto(photoId: number) {
    try {
      photoMetadata = await getPhotoMetadata(photoId);
    } catch {
      photoMetadata = null;
    }
  }

  // Viewport Container Dimensions for Responsive Aspect-Ratio Fit Scaling
  let canvasContainer = $state<HTMLElement | null>(null);
  let canvasWidth = $state(900);
  let canvasHeight = $state(700);

  // Rotation Animation State (Monotonic cumulative angle for silky smooth continuous physical glide)
  let visualRotation = $state(0);
  let isTransitioning = $state(false);
  let transitionTimer: ReturnType<typeof setTimeout> | null = null;

  function triggerAnimatedRotation(applyFn: () => void) {
    if (transitionTimer) clearTimeout(transitionTimer);
    isTransitioning = true;
    applyFn();
    transitionTimer = setTimeout(() => {
      isTransitioning = false;
      transitionTimer = null;
    }, 420);
  }

  // Sidebar & Navigation Tabs
  let isSidebarCollapsed = $state(false);
  type EditorTab = 'basic' | 'tuning' | 'fx1' | 'fx2' | 'fx3';
  let activeTab = $state<EditorTab>('basic');

  // Comparison Mode ('1up' = Single canvas, 'ab' = Side-by-side before/after)
  let comparisonMode = $state<'1up' | 'ab'>('1up');

  // Basic Fixes Sub-Tools
  type ActiveTool = null | 'crop' | 'straighten' | 'redeye' | 'retouch' | 'text';
  let activeTool = $state<ActiveTool>(null);
  type CropAspectPreset = '4x6' | '5x7' | '8x10' | 'square' | '4:3' | '16:9' | 'original' | 'free';
  let cropAspect = $state<CropAspectPreset>('4x6');
  let cropOrientation = $state<'portrait' | 'landscape'>('portrait');
  let cropBox = $state<{ x: number; y: number; width: number; height: number }>({
    x: 0.1,
    y: 0.1,
    width: 0.8,
    height: 0.8,
  });
  let cropSuggestions = $state<CropSuggestion[]>([]);
  let isLoadingCropSuggestions = $state(false);
  let isCropPreviewMode = $state(false);
  let cropPreviewTimer: any = null;
  let imageContainerRef = $state<HTMLElement | null>(null);
  let dragHandle = $state<string | null>(null);
  let dragStartMouse = $state<{ x: number; y: number }>({ x: 0, y: 0 });
  let dragStartBox = $state<{ x: number; y: number; width: number; height: number }>({
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  });

  let previewScale = $derived.by(() => {
    const w = Math.max(0.01, cropBox.width);
    const h = Math.max(0.01, cropBox.height);
    return Math.min(1 / w, 1 / h);
  });

  let previewTx = $derived.by(() => {
    const w = Math.max(0.01, cropBox.width);
    const cx = cropBox.x + w / 2;
    return (0.5 - cx) * 100;
  });

  let previewTy = $derived.by(() => {
    const h = Math.max(0.01, cropBox.height);
    const cy = cropBox.y + h / 2;
    return (0.5 - cy) * 100;
  });

  let previewClipTop = $derived(cropBox.y * 100);
  let previewClipRight = $derived((1 - cropBox.x - cropBox.width) * 100);
  let previewClipBottom = $derived((1 - cropBox.y - cropBox.height) * 100);
  let previewClipLeft = $derived(cropBox.x * 100);

  function triggerCropPreview() {
    if (isCropPreviewMode) {
      cancelCropPreview();
      return;
    }
    isCropPreviewMode = true;
    if (cropPreviewTimer) clearTimeout(cropPreviewTimer);
    cropPreviewTimer = setTimeout(() => {
      isCropPreviewMode = false;
      cropPreviewTimer = null;
    }, 2800);
  }

  function cancelCropPreview() {
    isCropPreviewMode = false;
    if (cropPreviewTimer) {
      clearTimeout(cropPreviewTimer);
      cropPreviewTimer = null;
    }
  }

  function getTargetAspectValue(): number | null {
    if (cropAspect === 'free') return null;
    if (cropAspect === 'square') return 1.0;
    let baseRatio = 1.0;
    if (cropAspect === '4x6') baseRatio = 4 / 6;
    else if (cropAspect === '5x7') baseRatio = 5 / 7;
    else if (cropAspect === '8x10') baseRatio = 8 / 10;
    else if (cropAspect === '4:3') baseRatio = 4 / 3;
    else if (cropAspect === '16:9') baseRatio = 16 / 9;
    else if (cropAspect === 'original') {
      if (renderedNaturalWidth > 0 && renderedNaturalHeight > 0) {
        baseRatio = renderedNaturalWidth / renderedNaturalHeight;
      } else {
        const isQuarterTurn = Math.abs(Math.round(visualRotation / 90)) % 2 === 1;
        const nw = currentPhoto?.width || 1;
        const nh = currentPhoto?.height || 1;
        baseRatio = isQuarterTurn ? nh / nw : nw / nh;
      }
    }

    if (cropOrientation === 'landscape') {
      return baseRatio < 1 ? 1 / baseRatio : baseRatio;
    } else {
      return baseRatio > 1 ? 1 / baseRatio : baseRatio;
    }
  }

  function generateFallbackCropSuggestions(targetAspect: number | null): CropSuggestion[] {
    const imgAspect = activePhotoAspect;
    const r_norm = targetAspect ? targetAspect / imgAspect : 1.0;

    const makeBox = (id: number, label: string, scale: number, cy_bias: number = 0) => {
      let w = scale;
      let h = w / r_norm;
      if (h > scale) {
        h = scale;
        w = h * r_norm;
      }
      w = Math.max(0.1, Math.min(0.95, w));
      h = Math.max(0.1, Math.min(0.95, h));
      const x = Math.max(0, Math.min(1 - w, 0.5 - w / 2));
      const y = Math.max(0, Math.min(1 - h, 0.5 - h / 2 + cy_bias));
      return { id, label, x, y, width: w, height: h };
    };

    return [
      makeBox(1, 'Tight Focus', 0.52, -0.05),
      makeBox(2, 'Rule of Thirds', 0.72, 0.0),
      makeBox(3, 'Wide Context', 0.88, 0.0),
    ];
  }

  function applyAspectToCurrentCropBox(newAspect: typeof cropAspect) {
    cropAspect = newAspect;
    const targetAspect = getTargetAspectValue();
    if (targetAspect == null) {
      return;
    }

    const imgAspect = activePhotoAspect;
    const r_norm = targetAspect / imgAspect;
    const cx = cropBox.x + cropBox.width / 2;
    const cy = cropBox.y + cropBox.height / 2;

    let newW = cropBox.width;
    let newH = newW / r_norm;

    if (newH > 0.95 || newH > cropBox.height * 1.4) {
      newH = Math.min(0.9, cropBox.height);
      newW = newH * r_norm;
    }
    if (newW > 0.95) {
      newW = 0.9;
      newH = newW / r_norm;
    }

    const clampedW = Math.max(0.05, Math.min(0.95, newW));
    const clampedH = Math.max(0.05, Math.min(0.95, newH));
    const clampedX = Math.max(0, Math.min(1 - clampedW, cx - clampedW / 2));
    const clampedY = Math.max(0, Math.min(1 - clampedH, cy - clampedH / 2));

    cropBox = {
      x: clampedX,
      y: clampedY,
      width: clampedW,
      height: clampedH,
    };
  }

  async function loadCropSuggestions() {
    if (!currentPhoto?.id) return;
    const targetAspect = getTargetAspectValue();
    // Immediately regenerate suggestions for the new aspect ratio so the UI updates with 0ms lag!
    cropSuggestions = generateFallbackCropSuggestions(targetAspect);
    isLoadingCropSuggestions = true;
    try {
      const suggestions = await getCropSuggestions(currentPhoto.id, targetAspect ?? undefined);
      if (suggestions && suggestions.length > 0) {
        cropSuggestions = suggestions;
      }
    } catch (err) {
      console.error('Failed to load crop suggestions:', err);
    } finally {
      isLoadingCropSuggestions = false;
    }
  }

  async function openCropTool() {
    cancelCropPreview();
    // Await any pending rotation/edit writes in the queue
    await editQueue;
    if (sessionState?.preview_path) {
      const targetSrc = getPreviewImageUrl(sessionState.preview_path, cacheBuster);
      const img = new Image();
      img.src = targetSrc;
      try {
        await img.decode();
      } catch {}
    }
    activeTool = 'crop';
    applyAspectToCurrentCropBox(cropAspect);
    loadCropSuggestions();
  }

  function selectCropSuggestion(sug: CropSuggestion) {
    cancelCropPreview();
    cropBox = {
      x: sug.x,
      y: sug.y,
      width: sug.width,
      height: sug.height,
    };
  }

  function rotateCropOrientation() {
    cancelCropPreview();
    cropOrientation = cropOrientation === 'landscape' ? 'portrait' : 'landscape';
    applyAspectToCurrentCropBox(cropAspect);
    loadCropSuggestions();
  }

  function resetCrop() {
    cancelCropPreview();
    cropBox = { x: 0, y: 0, width: 1.0, height: 1.0 };
    applyAspectToCurrentCropBox(cropAspect);
  }

  async function handleApplyCrop() {
    cancelCropPreview();
    if (!currentPhoto?.id) return;
    const photoId = currentPhoto.id;
    const box = { ...cropBox };
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await applyCrop(
          photoId,
          box.x,
          box.y,
          box.width,
          box.height
        );
        if (res.success && res.session_state) {
          if (res.session_state.preview_path) {
            const nextSrc = getPreviewImageUrl(res.session_state.preview_path, Date.now());
            const img = new Image();
            img.src = nextSrc;
            try {
              await img.decode();
            } catch {}
          }
          sessionState = res.session_state;
          cacheBuster = Date.now();
          activeTool = null;
          showToast('Crop applied');
          onPhotoEdited();
        } else {
          activeTool = null;
          showToast(res.message || 'Failed to apply crop');
        }
      } catch (err: any) {
        activeTool = null;
        showToast(err?.message || 'Failed to apply crop');
      } finally {
        isEditing = false;
      }
    });
  }

  function handleCancelCrop() {
    cancelCropPreview();
    activeTool = null;
  }

  function startDrag(e: PointerEvent, handle: string) {
    e.stopPropagation();
    e.preventDefault();
    dragHandle = handle;
    dragStartMouse = { x: e.clientX, y: e.clientY };
    dragStartBox = { ...cropBox };
    window.addEventListener('pointermove', onWindowPointerMove);
    window.addEventListener('pointerup', onWindowPointerUp);
  }

  function handleCanvasPointerDown(e: PointerEvent) {
    if (isCropPreviewMode || !imageContainerRef) return;
    const rect = imageContainerRef.getBoundingClientRect();
    const startX = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const startY = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));

    if (
      startX >= cropBox.x &&
      startX <= cropBox.x + cropBox.width &&
      startY >= cropBox.y &&
      startY <= cropBox.y + cropBox.height
    ) {
      startDrag(e, 'move');
      return;
    }

    const targetAspect = getTargetAspectValue();
    const imgAspect = rect.width / rect.height;
    const initialW = 0.05;
    const initialH = targetAspect ? initialW * (imgAspect / targetAspect) : 0.05;

    cropBox = {
      x: Math.min(startX, 1 - initialW),
      y: Math.min(startY, 1 - initialH),
      width: initialW,
      height: initialH,
    };
    dragStartMouse = { x: e.clientX, y: e.clientY };
    dragStartBox = { ...cropBox };
    dragHandle = 'se';
    window.addEventListener('pointermove', onWindowPointerMove);
    window.addEventListener('pointerup', onWindowPointerUp);
  }

  function onWindowPointerMove(e: PointerEvent) {
    if (!dragHandle || !imageContainerRef) return;
    const rect = imageContainerRef.getBoundingClientRect();
    const dx = (e.clientX - dragStartMouse.x) / rect.width;
    const dy = (e.clientY - dragStartMouse.y) / rect.height;
    const imgAspect = rect.width / rect.height;
    const targetAspect = getTargetAspectValue();

    if (dragHandle === 'move') {
      let newX = dragStartBox.x + dx;
      let newY = dragStartBox.y + dy;

      // Magnetic edge snapping: cleanly snap to photo borders within 1.5%
      const snapThresh = 0.015;
      if (Math.abs(newX) < snapThresh) newX = 0;
      if (Math.abs(newX + dragStartBox.width - 1) < snapThresh) newX = 1 - dragStartBox.width;
      if (Math.abs(newY) < snapThresh) newY = 0;
      if (Math.abs(newY + dragStartBox.height - 1) < snapThresh) newY = 1 - dragStartBox.height;

      // Magnetic center snapping: snap to 50% line within 1.2%
      const cx = newX + dragStartBox.width / 2;
      const cy = newY + dragStartBox.height / 2;
      if (Math.abs(cx - 0.5) < 0.012) newX = 0.5 - dragStartBox.width / 2;
      if (Math.abs(cy - 0.5) < 0.012) newY = 0.5 - dragStartBox.height / 2;

      newX = Math.max(0, Math.min(1 - dragStartBox.width, newX));
      newY = Math.max(0, Math.min(1 - dragStartBox.height, newY));
      cropBox = { ...cropBox, x: newX, y: newY };
      return;
    }

    if (dragHandle === 'se') {
      if (targetAspect == null) {
        const newW = Math.max(0.05, Math.min(1 - dragStartBox.x, dragStartBox.width + dx));
        const newH = Math.max(0.05, Math.min(1 - dragStartBox.y, dragStartBox.height + dy));
        cropBox = { ...cropBox, width: newW, height: newH };
      } else {
        let newW = Math.max(0.05, dragStartBox.width + dx);
        let newH = newW * (imgAspect / targetAspect);
        if (dragStartBox.x + newW > 1) {
          newW = 1 - dragStartBox.x;
          newH = newW * (imgAspect / targetAspect);
        }
        if (dragStartBox.y + newH > 1) {
          newH = 1 - dragStartBox.y;
          newW = newH / (imgAspect / targetAspect);
        }
        cropBox = { ...cropBox, width: Math.max(0.05, newW), height: Math.max(0.05, newH) };
      }
      return;
    }

    if (dragHandle === 'nw') {
      const right = dragStartBox.x + dragStartBox.width;
      const bottom = dragStartBox.y + dragStartBox.height;
      if (targetAspect == null) {
        const newX = Math.min(right - 0.05, Math.max(0, dragStartBox.x + dx));
        const newY = Math.min(bottom - 0.05, Math.max(0, dragStartBox.y + dy));
        cropBox = { x: newX, y: newY, width: right - newX, height: bottom - newY };
      } else {
        let newW = Math.max(0.05, dragStartBox.width - dx);
        let newH = newW * (imgAspect / targetAspect);
        if (right - newW < 0) {
          newW = right;
          newH = newW * (imgAspect / targetAspect);
        }
        if (bottom - newH < 0) {
          newH = bottom;
          newW = newH / (imgAspect / targetAspect);
        }
        cropBox = { x: right - newW, y: bottom - newH, width: newW, height: newH };
      }
      return;
    }

    if (dragHandle === 'ne') {
      const left = dragStartBox.x;
      const bottom = dragStartBox.y + dragStartBox.height;
      if (targetAspect == null) {
        const newW = Math.max(0.05, Math.min(1 - left, dragStartBox.width + dx));
        const newY = Math.min(bottom - 0.05, Math.max(0, dragStartBox.y + dy));
        cropBox = { x: left, y: newY, width: newW, height: bottom - newY };
      } else {
        let newW = Math.max(0.05, dragStartBox.width + dx);
        let newH = newW * (imgAspect / targetAspect);
        if (left + newW > 1) {
          newW = 1 - left;
          newH = newW * (imgAspect / targetAspect);
        }
        if (bottom - newH < 0) {
          newH = bottom;
          newW = newH / (imgAspect / targetAspect);
        }
        cropBox = { x: left, y: bottom - newH, width: newW, height: newH };
      }
      return;
    }

    if (dragHandle === 'sw') {
      const right = dragStartBox.x + dragStartBox.width;
      const top = dragStartBox.y;
      if (targetAspect == null) {
        const newX = Math.min(right - 0.05, Math.max(0, dragStartBox.x + dx));
        const newH = Math.max(0.05, Math.min(1 - top, dragStartBox.height + dy));
        cropBox = { x: newX, y: top, width: right - newX, height: newH };
      } else {
        let newW = Math.max(0.05, dragStartBox.width - dx);
        let newH = newW * (imgAspect / targetAspect);
        if (right - newW < 0) {
          newW = right;
          newH = newW * (imgAspect / targetAspect);
        }
        if (top + newH > 1) {
          newH = 1 - top;
          newW = newH / (imgAspect / targetAspect);
        }
        cropBox = { x: right - newW, y: top, width: newW, height: newH };
      }
      return;
    }

    if (dragHandle === 'n' || dragHandle === 's') {
      if (targetAspect == null) {
        if (dragHandle === 'n') {
          const newY = Math.min(dragStartBox.y + dragStartBox.height - 0.05, Math.max(0, dragStartBox.y + dy));
          cropBox = { ...cropBox, y: newY, height: dragStartBox.y + dragStartBox.height - newY };
        } else {
          const newH = Math.max(0.05, Math.min(1 - dragStartBox.y, dragStartBox.height + dy));
          cropBox = { ...cropBox, height: newH };
        }
      } else {
        const newH = Math.max(0.05, Math.min(1 - dragStartBox.y, dragStartBox.height + (dragHandle === 's' ? dy : -dy)));
        const newW = newH / (imgAspect / targetAspect);
        const centerX = dragStartBox.x + dragStartBox.width / 2;
        const newX = Math.max(0, Math.min(1 - newW, centerX - newW / 2));
        const newY = dragHandle === 's' ? dragStartBox.y : Math.max(0, dragStartBox.y + dragStartBox.height - newH);
        cropBox = { x: newX, y: newY, width: Math.max(0.05, newW), height: Math.max(0.05, newH) };
      }
      return;
    }

    if (dragHandle === 'e' || dragHandle === 'w') {
      if (targetAspect == null) {
        if (dragHandle === 'w') {
          const newX = Math.min(dragStartBox.x + dragStartBox.width - 0.05, Math.max(0, dragStartBox.x + dx));
          cropBox = { ...cropBox, x: newX, width: dragStartBox.x + dragStartBox.width - newX };
        } else {
          const newW = Math.max(0.05, Math.min(1 - dragStartBox.x, dragStartBox.width + dx));
          cropBox = { ...cropBox, width: newW };
        }
      } else {
        const newW = Math.max(0.05, Math.min(1 - dragStartBox.x, dragStartBox.width + (dragHandle === 'e' ? dx : -dx)));
        const newH = newW * (imgAspect / targetAspect);
        const centerY = dragStartBox.y + dragStartBox.height / 2;
        const newY = Math.max(0, Math.min(1 - newH, centerY - newH / 2));
        const newX = dragHandle === 'e' ? dragStartBox.x : Math.max(0, dragStartBox.x + dragStartBox.width - newW);
        cropBox = { x: newX, y: newY, width: Math.max(0.05, newW), height: Math.max(0.05, newH) };
      }
      return;
    }
  }

  function onWindowPointerUp() {
    dragHandle = null;
    window.removeEventListener('pointermove', onWindowPointerMove);
    window.removeEventListener('pointerup', onWindowPointerUp);
  }

  let straightenAngle = $state(0);
  let straightenSuggestions = $state<StraightenSuggestion[]>([]);
  let isLoadingStraightenSuggestions = $state(false);

  function getStraightenZoom(angleDeg: number, aspect: number): number {
    if (Math.abs(angleDeg) < 0.001) return 1.0;
    const rad = Math.abs((angleDeg * Math.PI) / 180);
    const a = aspect > 0 ? aspect : 1.333;
    const maxRatio = Math.max(a, 1 / a);
    return Math.cos(rad) + Math.sin(rad) * maxRatio;
  }

  function formatStraightenAngle(ang: number): string {
    const rounded = Math.abs(ang) < 0.05 ? 0 : Math.round(ang * 10) / 10;
    return `${rounded > 0 ? '+' : ''}${rounded.toFixed(1)}°`;
  }

  function generateFallbackStraightenSuggestions(): StraightenSuggestion[] {
    return [
      { id: 1, label: 'Horizon', angle: 0.0, confidence: 0.5, description: 'Neutral horizon alignment' },
      { id: 2, label: 'Subtle', angle: 1.0, confidence: 0.4, description: 'Minor tilt adjustment' },
      { id: 3, label: 'Alternative', angle: -1.0, confidence: 0.3, description: 'Counter-tilt adjustment' },
    ];
  }

  async function loadStraightenSuggestions() {
    if (!currentPhoto?.id) return;
    straightenSuggestions = generateFallbackStraightenSuggestions();
    isLoadingStraightenSuggestions = true;
    try {
      const suggestions = await getStraightenSuggestions(currentPhoto.id);
      if (suggestions && suggestions.length > 0) {
        straightenSuggestions = suggestions;
      }
    } catch (err) {
      console.error('Failed to load straighten suggestions:', err);
    } finally {
      isLoadingStraightenSuggestions = false;
    }
  }

  function selectStraightenSuggestion(sug: StraightenSuggestion) {
    straightenAngle = sug.angle;
  }

  async function openStraightenTool() {
    if (activeTool === 'straighten') {
      handleCancelStraighten();
      return;
    }
    cancelCropPreview();
    await editQueue;
    if (sessionState?.preview_path) {
      const targetSrc = getPreviewImageUrl(sessionState.preview_path, cacheBuster);
      const img = new Image();
      img.src = targetSrc;
      try {
        await img.decode();
      } catch {}
    }
    straightenAngle = 0;
    activeTool = 'straighten';
    loadStraightenSuggestions();
  }

  async function handleApplyStraighten() {
    if (!currentPhoto?.id || Math.abs(straightenAngle) < 0.01) {
      activeTool = null;
      straightenAngle = 0;
      return;
    }
    const photoId = currentPhoto.id;
    const angleToApply = straightenAngle;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await applyStraighten(photoId, angleToApply);
        if (res.success && res.session_state) {
          if (res.session_state.preview_path) {
            const nextSrc = getPreviewImageUrl(res.session_state.preview_path, Date.now());
            const img = new Image();
            img.src = nextSrc;
            try {
              await img.decode();
            } catch {}
          }
          sessionState = res.session_state;
          cacheBuster = Date.now();
          activeTool = null;
          straightenAngle = 0;
          showToast(`Straighten ${angleToApply > 0 ? '+' : ''}${angleToApply}° applied`);
          onPhotoEdited();
        } else {
          activeTool = null;
          straightenAngle = 0;
          showToast(res.message || 'Failed to apply straighten');
        }
      } catch (err: any) {
        activeTool = null;
        straightenAngle = 0;
        showToast(err?.message || 'Failed to apply straighten');
      } finally {
        isEditing = false;
      }
    });
  }

  function handleCancelStraighten() {
    straightenAngle = 0;
    activeTool = null;
  }

  // Red Eye Repair State (Picasa Heritage)
  let redEyeBoxes = $state<RedEyeBox[]>([]);
  let isDetectingRedEyes = $state(false);
  let isRedEyePreviewHidden = $state(false);
  let isApplyingRedEye = $state(false);
  let redEyeDragStart = $state<{ x: number; y: number } | null>(null);
  let redEyeCurrentDrag = $state<{ x: number; y: number } | null>(null);
  let redEyeCanvasRef = $state<HTMLCanvasElement | null>(null);

  async function openRedEyeTool() {
    activeTool = 'redeye';
    isRedEyePreviewHidden = false;
    redEyeBoxes = [];
    if (currentPhoto) {
      isDetectingRedEyes = true;
      try {
        const detected = await detectRedEyes(currentPhoto.id);
        redEyeBoxes = detected;
      } catch (e: any) {
        console.error('Failed to auto-detect red eyes:', e);
        showToast(e?.message || 'Failed to detect red eyes');
      } finally {
        isDetectingRedEyes = false;
      }
    }
  }

  async function handleAutoRedEye() {
    if (!currentPhoto) return;
    isDetectingRedEyes = true;
    try {
      const detected = await detectRedEyes(currentPhoto.id);
      redEyeBoxes = detected;
      if (detected.length === 0) {
        showToast('No red eyes automatically detected');
      }
    } catch (e: any) {
      console.error('Failed to auto-detect red eyes:', e);
      showToast(e?.message || 'Failed to detect red eyes');
    } finally {
      isDetectingRedEyes = false;
    }
  }

  function handleResetRedEye() {
    redEyeBoxes = [];
  }

  async function handleApplyRedEye() {
    if (!currentPhoto || isApplyingRedEye) return;
    isApplyingRedEye = true;
    try {
      const res = await applyRedEyeStep(currentPhoto.id, redEyeBoxes);
      if (res.session_state) {
        sessionState = res.session_state;
        cacheBuster = Date.now();
      }
      activeTool = null;
      showToast('Red eye repair applied');
      onPhotoEdited();
    } catch (e: any) {
      showToast(e.message || 'Failed to apply red eye repair');
    } finally {
      isApplyingRedEye = false;
    }
  }

  function handleCancelRedEye() {
    activeTool = null;
    redEyeBoxes = [];
    isRedEyePreviewHidden = false;
    redEyeDragStart = null;
    redEyeCurrentDrag = null;
  }

  function handleDeleteRedEyeBox(id: string) {
    redEyeBoxes = redEyeBoxes.filter((b) => b.id !== id);
  }

  function getRedEyeDragBox(start: { x: number; y: number }, current: { x: number; y: number }) {
    const minX = Math.min(start.x, current.x);
    const minY = Math.min(start.y, current.y);
    const maxX = Math.max(start.x, current.x);
    const maxY = Math.max(start.y, current.y);
    return {
      x: minX,
      y: minY,
      width: maxX - minX,
      height: maxY - minY,
    };
  }

  let isRedEyeDragging = false;

  function handleRedEyePointerDown(e: PointerEvent) {
    if (activeTool !== 'redeye') return;
    const overlay = e.currentTarget as HTMLElement;
    overlay.setPointerCapture(e.pointerId);
    isRedEyeDragging = true;
    const rect = overlay.getBoundingClientRect();
    const curX = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const curY = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    redEyeDragStart = { x: curX, y: curY };
    redEyeCurrentDrag = { x: curX, y: curY };
  }

  function handleRedEyePointerMove(e: PointerEvent) {
    if (!isRedEyeDragging || !redEyeDragStart) return;
    const overlay = e.currentTarget as HTMLElement;
    const rect = overlay.getBoundingClientRect();
    const curX = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const curY = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    redEyeCurrentDrag = { x: curX, y: curY };
  }

  function handleRedEyePointerUp(e: PointerEvent) {
    if (!isRedEyeDragging || !redEyeDragStart || !redEyeCurrentDrag) {
      isRedEyeDragging = false;
      redEyeDragStart = null;
      redEyeCurrentDrag = null;
      return;
    }
    const overlay = e.currentTarget as HTMLElement;
    try {
      overlay.releasePointerCapture(e.pointerId);
    } catch (_) {}

    const drag = getRedEyeDragBox(redEyeDragStart, redEyeCurrentDrag);
    isRedEyeDragging = false;
    redEyeDragStart = null;
    redEyeCurrentDrag = null;

    // Minimum size filter (0.5% width and height) so accidental taps don't add boxes
    if (drag.width >= 0.005 && drag.height >= 0.005) {
      const newBox: RedEyeBox = {
        id: `manual_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
        x: drag.x,
        y: drag.y,
        width: drag.width,
        height: drag.height,
      };
      redEyeBoxes = [...redEyeBoxes, newBox];
    }
  }

  function handleRedEyePointerCancel(e: PointerEvent) {
    isRedEyeDragging = false;
    redEyeDragStart = null;
    redEyeCurrentDrag = null;
  }

  function updateRedEyeCanvasPreview() {
    if (!redEyeCanvasRef || !mainImgRef || activeTool !== 'redeye') return;
    const canvas = redEyeCanvasRef;
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) return;

    const w = mainImgRef.naturalWidth || mainImgRef.width;
    const h = mainImgRef.naturalHeight || mainImgRef.height;
    if (w === 0 || h === 0) return;

    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    ctx.clearRect(0, 0, w, h);

    if (redEyeBoxes.length === 0) return;

    for (const box of redEyeBoxes) {
      const bx = Math.round(box.x * w);
      const by = Math.round(box.y * h);
      const bw = Math.max(1, Math.round(box.width * w));
      const bh = Math.max(1, Math.round(box.height * h));

      try {
        const offCanvas = document.createElement('canvas');
        offCanvas.width = bw;
        offCanvas.height = bh;
        const offCtx = offCanvas.getContext('2d', { willReadFrequently: true });
        if (!offCtx) continue;

        offCtx.drawImage(mainImgRef, bx, by, bw, bh, 0, 0, bw, bh);
        const imgData = offCtx.getImageData(0, 0, bw, bh);
        const data = imgData.data;

        const cx = bw * 0.5;
        const cy = bh * 0.5;
        const rx = bw * 0.5;
        const ry = bh * 0.5;

        for (let py = 0; py < bh; py++) {
          for (let px = 0; px < bw; px++) {
            const idx = (py * bw + px) * 4;
            const r = data[idx];
            const g = data[idx + 1];
            const b = data[idx + 2];

            // 1. Catchlight preservation
            if (r > 185 && g > 185 && b > 185 && Math.abs(r - g) < 35 && Math.abs(r - b) < 35) {
              continue;
            }

            const dx = (px - cx) / Math.max(1, rx);
            const dy = (py - cy) / Math.max(1, ry);
            const distSq = dx * dx + dy * dy;
            if (distSq > 1.0) continue;
            const falloff = Math.max(0, 1.0 - Math.sqrt(distSq));

            const rednessDiff = r - (g + b) * 0.5;
            const maxGb = Math.max(g, b);
            const rednessRatio = r / (maxGb + 1.0);

            if (rednessDiff > 12 && rednessRatio > 1.25 && r > 40) {
              const targetLum = (g + b) * 0.5;
              const darkPupil = targetLum * 0.45;
              const ratioWeight = Math.min(1.0, Math.max(0.0, (rednessRatio - 1.25) / 0.4));
              const totalWeight = ratioWeight * (0.6 + 0.4 * falloff);

              data[idx] = Math.round(r * (1.0 - totalWeight) + darkPupil * totalWeight);
              data[idx + 1] = Math.round(g * (1.0 - totalWeight) + targetLum * 0.65 * totalWeight);
              data[idx + 2] = Math.round(b * (1.0 - totalWeight) + targetLum * 0.65 * totalWeight);
            } else if (distSq < 0.4 && g > 100 && g > 1.35 * b && r > 80) {
              const targetLum = (r + b) * 0.4;
              const glowWeight = 0.7 * (1.0 - distSq / 0.4);
              data[idx] = Math.round(r * (1.0 - glowWeight) + targetLum * 0.5 * glowWeight);
              data[idx + 1] = Math.round(g * (1.0 - glowWeight) + targetLum * 0.5 * glowWeight);
              data[idx + 2] = Math.round(b * (1.0 - glowWeight) + targetLum * 0.5 * glowWeight);
            }
          }
        }

        offCtx.putImageData(imgData, 0, 0);
        ctx.drawImage(offCanvas, bx, by);
      } catch (err) {
        // Ignore canvas read errors
      }
    }
  }

  $effect(() => {
    if (activeTool === 'redeye' && redEyeBoxes) {
      requestAnimationFrame(() => {
        updateRedEyeCanvasPreview();
      });
    }
  });

  let retouchRadius = $state(32); // in pixels on natural image
  let retouchSpots = $state<RetouchSpot[]>([]);
  let retouchCanvasRef = $state<HTMLCanvasElement | null>(null);
  let showRetouchCircles = $state(false); // Hidden by default, toggled in sidebar
  let isApplyingRetouch = $state(false);

  // Offscreen canvas storing cumulative baked edits for current retouch session
  let retouchWorkingCanvas: HTMLCanvasElement | null = null;

  // 2-click SpotBlend state:
  // Phase 1: pending target placed at (target_x, target_y), waiting for source selection click.
  let pendingRetouchTarget = $state<{ x: number; y: number } | null>(null);
  // Current mouse position in normalized [0, 1] coordinates
  let retouchCursorPos = $state<{ x: number; y: number } | null>(null);

  function resetRetouchState() {
    retouchSpots = [];
    pendingRetouchTarget = null;
    retouchCursorPos = null;
    retouchWorkingCanvas = null;
    if (retouchCanvasRef) {
      const ctx = retouchCanvasRef.getContext('2d');
      if (ctx) ctx.clearRect(0, 0, retouchCanvasRef.width, retouchCanvasRef.height);
    }
  }

  function handleDeleteRetouchSpot(id: string) {
    retouchSpots = retouchSpots.filter((s) => s.id !== id);
    initRetouchWorkingCanvas();
  }

  function handleResetRetouch() {
    retouchSpots = [];
    pendingRetouchTarget = null;
    retouchCursorPos = null;
    initRetouchWorkingCanvas();
    showToast('Retouch reset');
  }

  async function handleApplyRetouch() {
    if (!currentPhoto || retouchSpots.length === 0) {
      activeTool = null;
      return;
    }
    isApplyingRetouch = true;
    try {
      const res = await applyRetouchStep(currentPhoto.id, retouchSpots);
      if (res.session_state) {
        sessionState = res.session_state;
        cacheBuster = Date.now();
      }
      showToast(`Applied ${retouchSpots.length} retouch spot${retouchSpots.length > 1 ? 's' : ''}`);
      resetRetouchState();
      activeTool = null;
    } catch (err) {
      console.error('Failed to apply retouch:', err);
      showToast('Failed to apply retouch');
    } finally {
      isApplyingRetouch = false;
    }
  }

  function handleCancelRetouch() {
    resetRetouchState();
    activeTool = null;
  }

  function getNormalizedCoords(e: PointerEvent): { x: number; y: number } | null {
    const target = e.currentTarget as HTMLElement | null;
    if (!target) return null;
    const rect = target.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    const x = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    const y = Math.max(0, Math.min(1, (e.clientY - rect.top) / rect.height));
    return { x, y };
  }

  // Core SpotBlend algorithm applied directly onto a target canvas (mutates canvas in-place)
  function applySpotBlendToCanvas(canvas: HTMLCanvasElement, spot: RetouchSpot) {
    const ctx = canvas.getContext('2d', { willReadFrequently: true });
    if (!ctx) return;

    const w = canvas.width;
    const h = canvas.height;
    if (w <= 0 || h <= 0) return;

    const minDim = Math.min(w, h);
    const tx = spot.target_x * w;
    const ty = spot.target_y * h;
    const sx = spot.source_x * w;
    const sy = spot.source_y * h;
    const r = spot.radius * minDim;
    if (r <= 0.5) return;

    const minTx = Math.max(0, Math.floor(tx - r));
    const maxTx = Math.min(w - 1, Math.ceil(tx + r));
    const minTy = Math.max(0, Math.floor(ty - r));
    const maxTy = Math.min(h - 1, Math.ceil(ty + r));
    const boxW = maxTx - minTx + 1;
    const boxH = maxTy - minTy + 1;
    if (boxW <= 0 || boxH <= 0) return;

    // Snapshot target region
    const tImgData = ctx.getImageData(minTx, minTy, boxW, boxH);
    const tData = tImgData.data;

    // Snapshot donor source region into an offscreen canvas to handle self-overlap cleanly
    const srcMinX = Math.round(sx - (tx - minTx));
    const srcMinY = Math.round(sy - (ty - minTy));
    const donorOffCanvas = document.createElement('canvas');
    donorOffCanvas.width = boxW;
    donorOffCanvas.height = boxH;
    const donorCtx = donorOffCanvas.getContext('2d', { willReadFrequently: true });
    if (!donorCtx) return;
    donorCtx.drawImage(canvas, srcMinX, srcMinY, boxW, boxH, 0, 0, boxW, boxH);
    const sData = donorCtx.getImageData(0, 0, boxW, boxH).data;

    // Sample Dirichlet boundary discrepancy
    const numSamples = Math.min(128, Math.max(32, Math.ceil(2 * Math.PI * r)));
    let avgDiffR = 0, avgDiffG = 0, avgDiffB = 0;
    let gradRx = 0, gradRy = 0;
    let gradGx = 0, gradGy = 0;
    let gradBx = 0, gradBy = 0;
    let validSamples = 0;

    for (let i = 0; i < numSamples; i++) {
      const theta = (i * 2 * Math.PI) / numSamples;
      const cosT = Math.cos(theta);
      const sinT = Math.sin(theta);
      const bpx = Math.round(tx + r * cosT) - minTx;
      const bpy = Math.round(ty + r * sinT) - minTy;

      if (bpx >= 0 && bpx < boxW && bpy >= 0 && bpy < boxH) {
        const idx = (bpy * boxW + bpx) * 4;
        const diffR = tData[idx] - sData[idx];
        const diffG = tData[idx + 1] - sData[idx + 1];
        const diffB = tData[idx + 2] - sData[idx + 2];

        avgDiffR += diffR;
        avgDiffG += diffG;
        avgDiffB += diffB;

        gradRx += diffR * cosT;
        gradRy += diffR * sinT;
        gradGx += diffG * cosT;
        gradGy += diffG * sinT;
        gradBx += diffB * cosT;
        gradBy += diffB * sinT;

        validSamples++;
      }
    }

    if (validSamples > 0) {
      avgDiffR /= validSamples;
      avgDiffG /= validSamples;
      avgDiffB /= validSamples;

      const gradFactor = 2.0 / (validSamples * r);
      gradRx *= gradFactor; gradRy *= gradFactor;
      gradGx *= gradFactor; gradGy *= gradFactor;
      gradBx *= gradFactor; gradBy *= gradFactor;
    }

    // Wide smooth margin falloff: inner core is 25% of radius, outer 75% transitions smoothly
    const rInner = 0.25 * r;
    const rBand = Math.max(1e-4, r - rInner);
    const rSq = r * r;

    for (let py = 0; py < boxH; py++) {
      const dy = (minTy + py) - ty;
      for (let px = 0; px < boxW; px++) {
        const dx = (minTx + px) - tx;
        const distSq = dx * dx + dy * dy;
        if (distSq >= rSq) continue;

        const dist = Math.sqrt(distSq);
        const t = dist <= rInner ? 0.0 : Math.min(1.0, Math.max(0.0, (dist - rInner) / rBand));
        // Quintic smootherstep: 6t^5 - 15t^4 + 10t^3
        const smoothAlpha = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);

        const idx = (py * boxW + px) * 4;
        const corrR = avgDiffR + gradRx * dx + gradRy * dy;
        const corrG = avgDiffG + gradGx * dx + gradGy * dy;
        const corrB = avgDiffB + gradBx * dx + gradBy * dy;

        const healedR = Math.min(255, Math.max(0, sData[idx] + corrR));
        const healedG = Math.min(255, Math.max(0, sData[idx + 1] + corrG));
        const healedB = Math.min(255, Math.max(0, sData[idx + 2] + corrB));

        tData[idx] = Math.round((1.0 - smoothAlpha) * healedR + smoothAlpha * tData[idx]);
        tData[idx + 1] = Math.round((1.0 - smoothAlpha) * healedG + smoothAlpha * tData[idx + 1]);
        tData[idx + 2] = Math.round((1.0 - smoothAlpha) * healedB + smoothAlpha * tData[idx + 2]);
      }
    }

    ctx.putImageData(tImgData, minTx, minTy);
  }

  function initRetouchWorkingCanvas() {
    if (!mainImgRef) return;
    const w = mainImgRef.naturalWidth || mainImgRef.width;
    const h = mainImgRef.naturalHeight || mainImgRef.height;
    if (w <= 0 || h <= 0) return;

    if (!retouchWorkingCanvas || retouchWorkingCanvas.width !== w || retouchWorkingCanvas.height !== h) {
      retouchWorkingCanvas = document.createElement('canvas');
      retouchWorkingCanvas.width = w;
      retouchWorkingCanvas.height = h;
    }

    const wCtx = retouchWorkingCanvas.getContext('2d', { willReadFrequently: true });
    if (!wCtx) return;
    wCtx.clearRect(0, 0, w, h);
    wCtx.drawImage(mainImgRef, 0, 0, w, h);

    // Sequentially apply all committed spots to bake them into the working canvas
    for (const spot of retouchSpots) {
      applySpotBlendToCanvas(retouchWorkingCanvas, spot);
    }

    renderRetouchDisplay();
  }

  function renderRetouchDisplay() {
    const canvas = retouchCanvasRef;
    if (!canvas || !retouchWorkingCanvas) return;
    const w = retouchWorkingCanvas.width;
    const h = retouchWorkingCanvas.height;
    if (w <= 0 || h <= 0) return;

    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(retouchWorkingCanvas, 0, 0);
  }

  function handleRetouchPointerDown(e: PointerEvent) {
    if (e.button !== 0) return; // Left click only
    const coords = getNormalizedCoords(e);
    if (!coords || !mainImgRef) return;

    const w = mainImgRef.naturalWidth || mainImgRef.width;
    const h = mainImgRef.naturalHeight || mainImgRef.height;
    if (w <= 0 || h <= 0) return;

    const normRadius = retouchRadius / Math.min(w, h);

    if (!pendingRetouchTarget) {
      // Step 1: Place target on blemish
      pendingRetouchTarget = { x: coords.x, y: coords.y };
    } else {
      // Step 2: Lock donor source patch and commit spot
      const newSpot: RetouchSpot = {
        id: `spot-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
        target_x: pendingRetouchTarget.x,
        target_y: pendingRetouchTarget.y,
        radius: normRadius,
        source_x: coords.x,
        source_y: coords.y,
      };

      if (!retouchWorkingCanvas) {
        initRetouchWorkingCanvas();
      }
      if (retouchWorkingCanvas) {
        // Immediately bake new spot into working canvas
        applySpotBlendToCanvas(retouchWorkingCanvas, newSpot);
      }

      retouchSpots = [...retouchSpots, newSpot];
      pendingRetouchTarget = null;
      retouchCursorPos = null;

      // Render the newly updated working canvas to the display overlay
      renderRetouchDisplay();
    }
  }

  function handleRetouchPointerMove(e: PointerEvent) {
    const coords = getNormalizedCoords(e);
    if (coords) {
      retouchCursorPos = coords;
      if (pendingRetouchTarget) {
        requestAnimationFrame(() => updateRetouchCanvasPreview());
      }
    }
  }

  function updateRetouchCanvasPreview() {
    const canvas = retouchCanvasRef;
    if (!canvas || !retouchWorkingCanvas || !mainImgRef) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const w = retouchWorkingCanvas.width;
    const h = retouchWorkingCanvas.height;
    if (w <= 0 || h <= 0) return;

    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    // Always draw base working canvas containing all previously committed spots
    ctx.clearRect(0, 0, w, h);
    ctx.drawImage(retouchWorkingCanvas, 0, 0);

    // If currently picking donor source (Phase 2), render live blended patch
    if (pendingRetouchTarget && retouchCursorPos) {
      const minDim = Math.min(w, h);
      const normRadius = retouchRadius / minDim;
      const previewSpot: RetouchSpot = {
        id: 'preview-pending',
        target_x: pendingRetouchTarget.x,
        target_y: pendingRetouchTarget.y,
        radius: normRadius,
        source_x: retouchCursorPos.x,
        source_y: retouchCursorPos.y,
      };

      const tx = previewSpot.target_x * w;
      const ty = previewSpot.target_y * h;
      const sx = previewSpot.source_x * w;
      const sy = previewSpot.source_y * h;
      const r = previewSpot.radius * minDim;
      if (r > 0.5) {
        const minTx = Math.max(0, Math.floor(tx - r));
        const maxTx = Math.min(w - 1, Math.ceil(tx + r));
        const minTy = Math.max(0, Math.floor(ty - r));
        const maxTy = Math.min(h - 1, Math.ceil(ty + r));
        const boxW = maxTx - minTx + 1;
        const boxH = maxTy - minTy + 1;

        if (boxW > 0 && boxH > 0) {
          const patchCanvas = document.createElement('canvas');
          patchCanvas.width = boxW;
          patchCanvas.height = boxH;
          const patchCtx = patchCanvas.getContext('2d', { willReadFrequently: true });
          if (patchCtx) {
            patchCtx.drawImage(retouchWorkingCanvas, minTx, minTy, boxW, boxH, 0, 0, boxW, boxH);
            const tImgData = patchCtx.getImageData(0, 0, boxW, boxH);
            const tData = tImgData.data;

            const donorCanvas = document.createElement('canvas');
            donorCanvas.width = boxW;
            donorCanvas.height = boxH;
            const donorCtx = donorCanvas.getContext('2d', { willReadFrequently: true });
            if (donorCtx) {
              const srcMinX = Math.round(sx - (tx - minTx));
              const srcMinY = Math.round(sy - (ty - minTy));
              donorCtx.drawImage(retouchWorkingCanvas, srcMinX, srcMinY, boxW, boxH, 0, 0, boxW, boxH);
              const sData = donorCtx.getImageData(0, 0, boxW, boxH).data;

              const numSamples = Math.min(128, Math.max(32, Math.ceil(2 * Math.PI * r)));
              let avgDiffR = 0, avgDiffG = 0, avgDiffB = 0;
              let gradRx = 0, gradRy = 0;
              let gradGx = 0, gradGy = 0;
              let gradBx = 0, gradBy = 0;
              let validSamples = 0;

              for (let i = 0; i < numSamples; i++) {
                const theta = (i * 2 * Math.PI) / numSamples;
                const cosT = Math.cos(theta);
                const sinT = Math.sin(theta);
                const bpx = Math.round(tx + r * cosT) - minTx;
                const bpy = Math.round(ty + r * sinT) - minTy;

                if (bpx >= 0 && bpx < boxW && bpy >= 0 && bpy < boxH) {
                  const idx = (bpy * boxW + bpx) * 4;
                  const diffR = tData[idx] - sData[idx];
                  const diffG = tData[idx + 1] - sData[idx + 1];
                  const diffB = tData[idx + 2] - sData[idx + 2];

                  avgDiffR += diffR;
                  avgDiffG += diffG;
                  avgDiffB += diffB;

                  gradRx += diffR * cosT;
                  gradRy += diffR * sinT;
                  gradGx += diffG * cosT;
                  gradGy += diffG * sinT;
                  gradBx += diffB * cosT;
                  gradBy += diffB * sinT;

                  validSamples++;
                }
              }

              if (validSamples > 0) {
                avgDiffR /= validSamples;
                avgDiffG /= validSamples;
                avgDiffB /= validSamples;

                const gradFactor = 2.0 / (validSamples * r);
                gradRx *= gradFactor; gradRy *= gradFactor;
                gradGx *= gradFactor; gradGy *= gradFactor;
                gradBx *= gradFactor; gradBy *= gradFactor;
              }

              const rInner = 0.25 * r;
              const rBand = Math.max(1e-4, r - rInner);
              const rSq = r * r;

              for (let py = 0; py < boxH; py++) {
                const dy = (minTy + py) - ty;
                for (let px = 0; px < boxW; px++) {
                  const dx = (minTx + px) - tx;
                  const distSq = dx * dx + dy * dy;
                  if (distSq >= rSq) continue;

                  const dist = Math.sqrt(distSq);
                  const t = dist <= rInner ? 0.0 : Math.min(1.0, Math.max(0.0, (dist - rInner) / rBand));
                  const smoothAlpha = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);

                  const idx = (py * boxW + px) * 4;
                  const corrR = avgDiffR + gradRx * dx + gradRy * dy;
                  const corrG = avgDiffG + gradGx * dx + gradGy * dy;
                  const corrB = avgDiffB + gradBx * dx + gradBy * dy;

                  const healedR = Math.min(255, Math.max(0, sData[idx] + corrR));
                  const healedG = Math.min(255, Math.max(0, sData[idx + 1] + corrG));
                  const healedB = Math.min(255, Math.max(0, sData[idx + 2] + corrB));

                  tData[idx] = Math.round((1.0 - smoothAlpha) * healedR + smoothAlpha * tData[idx]);
                  tData[idx + 1] = Math.round((1.0 - smoothAlpha) * healedG + smoothAlpha * tData[idx + 1]);
                  tData[idx + 2] = Math.round((1.0 - smoothAlpha) * healedB + smoothAlpha * tData[idx + 2]);
                }
              }

              patchCtx.putImageData(tImgData, 0, 0);
              ctx.drawImage(patchCanvas, minTx, minTy);
            }
          }
        }
      }
    }
  }

  $effect(() => {
    if (activeTool === 'retouch') {
      requestAnimationFrame(() => initRetouchWorkingCanvas());
    } else {
      retouchWorkingCanvas = null;
    }
  });

  $effect(() => {
    if (activeTool === 'retouch' && pendingRetouchTarget) {
      requestAnimationFrame(() => updateRetouchCanvasPreview());
    }
  });

  // =========================================================================
  // PICASA CLASSIC TEXT TOOL STATE & LOGIC
  // =========================================================================
  const FONT_FAMILIES = [
    'Arial',
    'Arial Black',
    'Comic Sans MS',
    'Courier New',
    'Georgia',
    'Impact',
    'Lucida Console',
    'Palatino Linotype',
    'Tahoma',
    'Times New Roman',
    'Trebuchet MS',
    'Verdana',
    'Roboto',
    'system-ui',
  ];
  const FONT_SIZES = [12, 14, 16, 18, 20, 24, 28, 32, 36, 42, 48, 56, 64, 72, 84, 96, 108, 120, 144, 160, 180, 200, 240];

  let textItems = $state<TextItem[]>([]);
  let activeTextId = $state<string | null>(null);
  let isApplyingText = $state(false);

  // Active item reference
  let activeTextItem = $derived.by(() => {
    return textItems.find((t) => t.id === activeTextId) || null;
  });

  // Current UI settings (synchronized with activeTextItem or defaults)
  let selectedFontFamily = $state('Arial');
  let selectedFontSize = $state(48);
  let selectedBold = $state(false);
  let selectedItalic = $state(false);
  let selectedUnderline = $state(false);
  let selectedAlignment = $state<'left' | 'center' | 'right'>('left');
  let selectedFillColor = $state('#ffffff');
  let selectedHasFill = $state(true);
  let selectedHasOutline = $state(false);
  let selectedOutlineColor = $state('#000000');
  let selectedOutlineWidth = $state(0);
  let selectedTransparency = $state(0);

  // Outline thickness stations: 0 = None, 3 = Medium, 7 = Thick (matches Picasa's 3 stations)
  const OUTLINE_STATIONS = [0, 3, 7];

  let outlineStationIndex = $derived.by(() => {
    if (!selectedHasOutline || selectedOutlineWidth <= 0) return 0;
    let closest = 1;
    let minDiff = Math.abs(selectedOutlineWidth - OUTLINE_STATIONS[1]);
    for (let i = 2; i < OUTLINE_STATIONS.length; i++) {
      const diff = Math.abs(selectedOutlineWidth - OUTLINE_STATIONS[i]);
      if (diff < minDiff) {
        minDiff = diff;
        closest = i;
      }
    }
    return closest;
  });

  function setOutlineStation(stationIdx: number) {
    if (stationIdx <= 0) {
      selectedOutlineWidth = 0;
      selectedHasOutline = false;
      updateActiveText((i) => {
        i.outlineWidth = 0;
        i.hasOutline = false;
      });
    } else {
      const w = OUTLINE_STATIONS[stationIdx];
      selectedOutlineWidth = w;
      selectedHasOutline = true;
      updateActiveText((i) => {
        i.outlineWidth = w;
        i.hasOutline = true;
      });
    }
  }

  // Synchronize UI settings when activeTextId changes
  $effect(() => {
    const item = activeTextItem;
    if (item) {
      selectedFontFamily = item.fontFamily;
      selectedFontSize = item.fontSize;
      selectedBold = item.bold;
      selectedItalic = item.italic;
      selectedUnderline = item.underline;
      selectedAlignment = item.alignment;
      selectedFillColor = item.color;
      selectedHasFill = item.hasFill;
      selectedHasOutline = item.hasOutline;
      selectedOutlineColor = item.outlineColor;
      selectedOutlineWidth = item.outlineWidth;
      selectedTransparency = item.transparency;
    }
  });

  function updateActiveText(updater: (item: TextItem) => void) {
    if (!activeTextId) return;
    textItems = textItems.map((item) => {
      if (item.id === activeTextId) {
        const copy = { ...item };
        updater(copy);
        return copy;
      }
      return item;
    });
  }

  // Custom action for contenteditable div that prevents Svelte reactivity from resetting the caret on input
  function editableText(node: HTMLElement, text: string) {
    node.innerText = text;
    if (text === 'Type text here') {
      setTimeout(() => {
        node.focus();
        const sel = window.getSelection();
        const range = document.createRange();
        range.selectNodeContents(node);
        sel?.removeAllRanges();
        sel?.addRange(range);
      }, 30);
    }
    return {
      update(newText: string) {
        if (document.activeElement !== node && node.innerText !== newText) {
          node.innerText = newText;
        }
      }
    };
  }

  function setFontFamily(font: string) {
    selectedFontFamily = font;
    updateActiveText((i) => (i.fontFamily = font));
  }
  function setFontSize(size: number) {
    selectedFontSize = size;
    updateActiveText((i) => (i.fontSize = size));
  }
  function toggleBold() {
    selectedBold = !selectedBold;
    updateActiveText((i) => (i.bold = selectedBold));
  }
  function toggleItalic() {
    selectedItalic = !selectedItalic;
    updateActiveText((i) => (i.italic = selectedItalic));
  }
  function toggleUnderline() {
    selectedUnderline = !selectedUnderline;
    updateActiveText((i) => (i.underline = selectedUnderline));
  }
  function setAlignment(align: 'left' | 'center' | 'right') {
    selectedAlignment = align;
    updateActiveText((i) => (i.alignment = align));
  }
  function setFillColor(col: string) {
    selectedFillColor = col;
    selectedHasFill = true;
    updateActiveText((i) => {
      i.color = col;
      i.hasFill = true;
    });
  }
  function toggleNoFill() {
    selectedHasFill = !selectedHasFill;
    // If fill was disabled and outline is off/zero, auto-enable medium outline so text stays visible
    if (!selectedHasFill && (!selectedHasOutline || selectedOutlineWidth === 0)) {
      selectedHasOutline = true;
      selectedOutlineWidth = OUTLINE_STATIONS[1] || 3;
    }
    updateActiveText((i) => {
      i.hasFill = selectedHasFill;
      i.hasOutline = selectedHasOutline;
      i.outlineWidth = selectedOutlineWidth;
    });
  }
  function setOutlineColor(col: string) {
    selectedOutlineColor = col;
    selectedHasOutline = true;
    // If outline width was 0, auto-activate medium outline station (3px)
    if (selectedOutlineWidth === 0) {
      selectedOutlineWidth = OUTLINE_STATIONS[1] || 3;
    }
    updateActiveText((i) => {
      i.outlineColor = col;
      i.hasOutline = true;
      i.outlineWidth = selectedOutlineWidth;
    });
  }
  function setOutlineWidth(width: number) {
    selectedOutlineWidth = width;
    selectedHasOutline = width > 0;
    updateActiveText((i) => {
      i.outlineWidth = width;
      i.hasOutline = width > 0;
    });
  }
  function setTransparency(trans: number) {
    selectedTransparency = trans;
    updateActiveText((i) => (i.transparency = trans));
  }

  function computeDefaultFontSize(): number {
    const rect = imageContainerRef?.getBoundingClientRect();
    const h = imageContainerRef?.clientHeight || rect?.height || (currentPhoto ? 720 : 600);
    // Proportionally scales to ~9% of image height, bounded between 48px and 120px
    return Math.max(48, Math.min(120, Math.round(h * 0.09)));
  }

  function activateTextTool() {
    activeTool = 'text';
    const computedFontSize = computeDefaultFontSize();

    if (textItems.length === 0) {
      selectedFontSize = computedFontSize;
      const newItem: TextItem = {
        id: 'text_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6),
        text: 'Type text here',
        x: 0.5,
        y: 0.5,
        angle: 0,
        scale: 1.0,
        fontSize: computedFontSize,
        fontFamily: selectedFontFamily,
        bold: selectedBold,
        italic: selectedItalic,
        underline: selectedUnderline,
        alignment: selectedAlignment,
        color: selectedFillColor,
        hasFill: selectedHasFill,
        hasOutline: selectedHasOutline,
        outlineColor: selectedOutlineColor,
        outlineWidth: selectedOutlineWidth,
        transparency: selectedTransparency,
      };
      textItems = [newItem];
      activeTextId = newItem.id;
    } else {
      if (!activeTextId) {
        activeTextId = textItems[0].id;
      }
      // If active text had a tiny font size (< 36px), bump it up to the image-fitting default
      updateActiveText((item) => {
        if (item.fontSize < 36) {
          item.fontSize = computedFontSize;
          selectedFontSize = computedFontSize;
        }
      });
    }
  }

  // Dragging and Dial Interaction State
  let textDrag = $state<{
    mode: 'move' | 'dial';
    itemId: string;
    startPointerX: number;
    startPointerY: number;
    origX: number;
    origY: number;
    origAngle: number;
    origScale: number;
    centerX: number;
    centerY: number;
    baseRadius: number;
  } | null>(null);

  let textHud = $state<{ angle: number; scale: number } | null>(null);

  function startDialHandleDrag(item: TextItem, e: PointerEvent) {
    e.stopPropagation();
    e.preventDefault();
    if (!imageContainerRef) return;
    
    // Find dial center in viewport client coordinates
    const target = e.currentTarget as HTMLElement;
    const dialRoot = target.closest('.picasa-text-box') as HTMLElement | null;
    let centerX = e.clientX;
    let centerY = e.clientY;
    if (dialRoot) {
      const rect = dialRoot.getBoundingClientRect();
      centerX = rect.left + rect.width / 2;
      centerY = rect.top + rect.height / 2;
    }

    const startDist = Math.hypot(e.clientX - centerX, e.clientY - centerY);

    textDrag = {
      mode: 'dial',
      itemId: item.id,
      startPointerX: e.clientX,
      startPointerY: e.clientY,
      origX: item.x,
      origY: item.y,
      origAngle: item.angle,
      origScale: item.scale,
      centerX,
      centerY,
      baseRadius: Math.max(15, startDist),
    };

    textHud = {
      angle: Math.round(item.angle),
      scale: Math.round(item.scale * 100),
    };

    window.addEventListener('pointermove', onTextWindowPointerMove);
    window.addEventListener('pointerup', onTextWindowPointerUp);
  }

  function startTextMoveDrag(item: TextItem, e: PointerEvent) {
    const target = e.target as HTMLElement;
    if (target.closest('.picasa-dial-handle')) return;
    if (target.closest('.picasa-editable-text')) {
      activeTextId = item.id;
      return;
    }
    activeTextId = item.id;
    if (!imageContainerRef) return;
    textDrag = {
      mode: 'move',
      itemId: item.id,
      startPointerX: e.clientX,
      startPointerY: e.clientY,
      origX: item.x,
      origY: item.y,
      origAngle: item.angle,
      origScale: item.scale,
      centerX: 0,
      centerY: 0,
      baseRadius: 0,
    };
    window.addEventListener('pointermove', onTextWindowPointerMove);
    window.addEventListener('pointerup', onTextWindowPointerUp);
  }

  function onTextWindowPointerMove(e: PointerEvent) {
    if (!textDrag || !imageContainerRef) return;
    if (textDrag.mode === 'move') {
      const rect = imageContainerRef.getBoundingClientRect();
      const dx = (e.clientX - textDrag.startPointerX) / rect.width;
      const dy = (e.clientY - textDrag.startPointerY) / rect.height;
      const newX = Math.max(0.01, Math.min(0.99, textDrag.origX + dx));
      const newY = Math.max(0.01, Math.min(0.99, textDrag.origY + dy));
      updateActiveText((item) => {
        item.x = newX;
        item.y = newY;
      });
    } else if (textDrag.mode === 'dial') {
      const dx = e.clientX - textDrag.centerX;
      const dy = e.clientY - textDrag.centerY;
      const rad = Math.atan2(dy, dx);
      let deg = (rad * 180 / Math.PI + 360) % 360;
      if (e.shiftKey) {
        deg = Math.round(deg / 15) * 15 % 360;
      } else {
        if (Math.abs(deg - 0) < 3.5 || Math.abs(deg - 360) < 3.5) deg = 0;
        else if (Math.abs(deg - 90) < 3.5) deg = 90;
        else if (Math.abs(deg - 180) < 3.5) deg = 180;
        else if (Math.abs(deg - 270) < 3.5) deg = 270;
      }

      const curDist = Math.hypot(dx, dy);
      const scaleMultiplier = curDist / textDrag.baseRadius;
      const newScale = Math.max(0.2, Math.min(4.0, textDrag.origScale * scaleMultiplier));
      const roundedDeg = Math.round(deg * 10) / 10;
      const roundedScalePct = Math.round(newScale * 100);

      textHud = { angle: Math.round(roundedDeg), scale: roundedScalePct };

      updateActiveText((item) => {
        item.angle = roundedDeg;
        item.scale = newScale;
      });
    }
  }

  function onTextWindowPointerUp() {
    textDrag = null;
    textHud = null;
    window.removeEventListener('pointermove', onTextWindowPointerMove);
    window.removeEventListener('pointerup', onTextWindowPointerUp);
  }

  function handleCanvasTextPointerDown(e: PointerEvent) {
    if (activeTool !== 'text' || !imageContainerRef) return;
    const target = e.target as HTMLElement;
    if (target.closest('.picasa-text-box') || target.closest('.picasa-dial-handle')) {
      return;
    }
    const rect = imageContainerRef.getBoundingClientRect();
    const clickX = (e.clientX - rect.left) / rect.width;
    const clickY = (e.clientY - rect.top) / rect.height;
    if (clickX < 0 || clickX > 1 || clickY < 0 || clickY > 1) return;

    if (activeTextId) {
      activeTextId = null;
      return;
    }

    const defaultFontSize = computeDefaultFontSize();
    selectedFontSize = defaultFontSize;

    const newItem: TextItem = {
      id: 'text_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6),
      text: 'Type text here',
      x: Math.max(0.1, Math.min(0.9, clickX)),
      y: Math.max(0.1, Math.min(0.9, clickY)),
      angle: 0,
      scale: 1.0,
      fontSize: defaultFontSize,
      fontFamily: selectedFontFamily,
      bold: selectedBold,
      italic: selectedItalic,
      underline: selectedUnderline,
      alignment: selectedAlignment,
      color: selectedFillColor,
      hasFill: selectedHasFill,
      hasOutline: selectedHasOutline,
      outlineColor: selectedOutlineColor,
      outlineWidth: selectedOutlineWidth,
      transparency: selectedTransparency,
    };
    textItems = [...textItems, newItem];
    activeTextId = newItem.id;
  }

  function handleCopyCaption() {
    if (!currentPhoto) return;
    const caption = currentPhoto.file_name.replace(/\.[^/.]+$/, '');
    const defaultFontSize = computeDefaultFontSize();
    if (activeTextId) {
      updateActiveText((item) => {
        item.text = caption;
      });
    } else {
      selectedFontSize = defaultFontSize;
      const newItem: TextItem = {
        id: 'text_' + Date.now() + '_' + Math.random().toString(36).slice(2, 6),
        text: caption,
        x: 0.5,
        y: 0.85,
        angle: 0,
        scale: 1.0,
        fontSize: defaultFontSize,
        fontFamily: selectedFontFamily,
        bold: selectedBold,
        italic: selectedItalic,
        underline: selectedUnderline,
        alignment: 'center',
        color: selectedFillColor,
        hasFill: selectedHasFill,
        hasOutline: selectedHasOutline,
        outlineColor: selectedOutlineColor,
        outlineWidth: selectedOutlineWidth,
        transparency: selectedTransparency,
      };
      textItems = [...textItems, newItem];
      activeTextId = newItem.id;
    }
  }

  function handleClearAllText() {
    textItems = [];
    activeTextId = null;
    showToast('Cleared all text');
  }

  function handleCancelText() {
    textItems = [];
    activeTextId = null;
    activeTool = null;
  }

  function handleDeleteActiveText() {
    if (activeTextId === null) return;
    textItems = textItems.filter((t) => t.id !== activeTextId);
    activeTextId = null;
  }


  async function renderTextToOffscreenCanvas(): Promise<string | null> {
    if (!currentPhoto || textItems.length === 0) return null;
    const natW = renderedNaturalWidth || currentPhoto.width || 1920;
    const natH = renderedNaturalHeight || currentPhoto.height || 1080;
    const canvas = document.createElement('canvas');
    canvas.width = natW;
    canvas.height = natH;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;

    const dispH = imageContainerRef?.clientHeight || 600;
    const scaleFactor = natH / dispH;

    for (const item of textItems) {
      if (!item.text.trim()) continue;
      ctx.save();
      const posX = item.x * natW;
      const posY = item.y * natH;
      ctx.translate(posX, posY);
      ctx.rotate((item.angle * Math.PI) / 180);
      ctx.scale(item.scale, item.scale);
      ctx.globalAlpha = Math.max(0, Math.min(1, (100 - item.transparency) / 100));

      const scaledFontSize = Math.max(12, item.fontSize * scaleFactor);
      let fontStr = '';
      if (item.italic) fontStr += 'italic ';
      if (item.bold) fontStr += 'bold ';
      fontStr += `${scaledFontSize}px ${item.fontFamily}, sans-serif`;
      ctx.font = fontStr;
      ctx.textAlign = item.alignment;
      ctx.textBaseline = 'middle';

      const lines = item.text.split('\n');
      const lineHeight = scaledFontSize * 1.25;
      const totalH = lines.length * lineHeight;
      const startY = -(totalH / 2) + lineHeight / 2;

      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const curY = startY + i * lineHeight;
        if (item.hasOutline && item.outlineWidth > 0) {
          ctx.strokeStyle = item.outlineColor;
          ctx.lineWidth = item.outlineWidth * scaleFactor * 1.5;
          ctx.lineJoin = 'round';
          ctx.strokeText(line, 0, curY);
        }
        if (item.hasFill) {
          ctx.fillStyle = item.color;
          ctx.fillText(line, 0, curY);
        }
        if (item.underline) {
          const metrics = ctx.measureText(line);
          const textW = metrics.width;
          let uStartX = -textW / 2;
          if (item.alignment === 'left') uStartX = 0;
          else if (item.alignment === 'right') uStartX = -textW;
          ctx.strokeStyle = item.hasFill ? item.color : item.outlineColor;
          ctx.lineWidth = Math.max(1, scaledFontSize / 14);
          ctx.beginPath();
          ctx.moveTo(uStartX, curY + scaledFontSize * 0.45);
          ctx.lineTo(uStartX + textW, curY + scaledFontSize * 0.45);
          ctx.stroke();
        }
      }
      ctx.restore();
    }

    return canvas.toDataURL('image/png');
  }

  async function handleApplyText() {
    if (!currentPhoto || textItems.length === 0) {
      activeTool = null;
      return;
    }
    isApplyingText = true;
    try {
      const overlayBase64 = await renderTextToOffscreenCanvas();
      if (!overlayBase64) {
        activeTool = null;
        return;
      }
      const res = await applyTextStep(currentPhoto.id, textItems, overlayBase64);
      if (res.session_state) {
        sessionState = res.session_state;
        cacheBuster = Date.now();
        showToast('Text caption applied');
        onPhotoEdited();
      }
    } catch (e) {
      console.error('Failed to apply text caption:', e);
      showToast('Failed to apply text');
    } finally {
      isApplyingText = false;
      textItems = [];
      activeTextId = null;
      activeTool = null;
    }
  }

  // Tuning Adjustments (Live CSS filter mapping)
  let fillLight = $state(0);
  let isAdjustingFillLight = $state(false);
  let isApplyingFillLight = $state(false);
  let highlights = $state(0);
  let shadows = $state(0);
  let colorTemp = $state(0);

  // Real-time Picasa Fill Light lookup table for SVG GPU shader
  const fillLightTableValues = $derived.by(() => {
    if (fillLight <= 0) return '0 1';
    const amount = fillLight / 100;
    const values: string[] = [];
    for (let i = 0; i < 256; i++) {
      const norm = i / 255;
      const fill = amount * norm * (1 - norm) * (2 - norm);
      const lifted = Math.min(1, Math.max(0, norm + fill));
      values.push(lifted.toFixed(4));
    }
    return values.join(' ');
  });

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

  // Edit action queue: strictly serializes all backend persistence operations
  // (rotations, crops, undo, redo, revert) to prevent file system collisions,
  // lost updates, or out-of-order session manifests when user clicks rapidly.
  let editQueue: Promise<void> = Promise.resolve();

  function enqueueEdit<T>(action: () => Promise<T>): Promise<T> {
    const run = editQueue.then(action, action);
    editQueue = run.then(() => {}, () => {});
    return run;
  }

  // Active base step for raster operations: tracks pixel-altering operations (like crops).
  // Pure rotations do not alter the base raster image; they are rotated smoothly on the GPU via CSS.
  let activeBaseStep = $derived.by(() => {
    if (!sessionState?.has_session || !sessionState.steps || sessionState.current_step === 0) {
      return null;
    }
    const maxIdx = Math.min(sessionState.current_step, sessionState.steps.length - 1);
    for (let i = maxIdx; i >= 1; i--) {
      const step = sessionState.steps[i];
      if (step.op !== 'rotate' && step.render_file) {
        return step;
      }
    }
    return null;
  });

  // Clean pre-fill step to act as the pristine base during real-time GPU fill light adjustments
  let preFillStep = $derived.by(() => {
    if (!sessionState?.has_session || !sessionState.steps || sessionState.current_step === 0) {
      return null;
    }
    const currentStepObj = sessionState.steps[sessionState.current_step];
    if (currentStepObj && (currentStepObj.op === 'filllight' || currentStepObj.op === 'fill')) {
      for (let i = sessionState.current_step - 1; i >= 1; i--) {
        const step = sessionState.steps[i];
        if (step.op !== 'rotate' && step.render_file) {
          return step;
        }
      }
      return null;
    }
    return activeBaseStep;
  });

  // Natural image dimensions and loading state measured from the currently loaded image
  let mainImgRef = $state<HTMLImageElement | null>(null);
  let renderedNaturalWidth = $state(0);
  let renderedNaturalHeight = $state(0);
  let isMainImageLoading = $state(false);
  let isCropHovered = $state(false);

  function handleImageLoaded(img: HTMLImageElement) {
    if (img && img.naturalWidth > 0 && img.naturalHeight > 0) {
      renderedNaturalWidth = img.naturalWidth;
      renderedNaturalHeight = img.naturalHeight;
    }
    isMainImageLoading = false;
  }

  $effect(() => {
    if (mainImgRef?.complete && mainImgRef.naturalWidth > 0 && mainImgRef.naturalHeight > 0) {
      renderedNaturalWidth = mainImgRef.naturalWidth;
      renderedNaturalHeight = mainImgRef.naturalHeight;
      isMainImageLoading = false;
    }
  });

  let activeImageSrc = $derived.by(() => {
    if (!currentPhoto) return '';
    // When cropping, straightening, red-eye repairing, or retouching, we render the current step's physical proxy (which has all prior rotations baked in)
    // with 0deg canvas rotation so the user edits in standard unrotated 2D screen space!
    if ((activeTool === 'crop' || activeTool === 'straighten' || activeTool === 'redeye' || activeTool === 'retouch') && sessionState?.has_session && sessionState.preview_path) {
      return getPreviewImageUrl(sessionState.preview_path, cacheBuster);
    }
    if (isAdjustingFillLight) {
      if (preFillStep && sessionState?.session_dir && preFillStep.render_file) {
        const stepPath = `${sessionState.session_dir}/${preFillStep.render_file}`;
        return getPreviewImageUrl(stepPath, cacheBuster);
      }
      return getFullImageUrl(currentPhoto, cacheBuster);
    }
    if (activeBaseStep && sessionState?.session_dir && activeBaseStep.render_file) {
      const stepPath = `${sessionState.session_dir}/${activeBaseStep.render_file}`;
      return getPreviewImageUrl(stepPath, cacheBuster);
    }
    return getFullImageUrl(currentPhoto, cacheBuster);
  });

  let previewThumbSrc = $derived.by(() => {
    if (!currentPhoto) return '';
    if (sessionState?.has_session && sessionState.current_step > 0 && sessionState.preview_path) {
      return getPreviewImageUrl(sessionState.preview_path, cacheBuster);
    }
    return getThumbnailUrl(currentPhoto, cacheBuster);
  });

  let activePhotoAspect = $derived.by(() => {
    if (activeTool === 'crop' || activeTool === 'straighten' || activeTool === 'redeye' || activeTool === 'retouch' || activeTool === 'text') {
      if (renderedNaturalWidth > 0 && renderedNaturalHeight > 0) {
        return renderedNaturalWidth / renderedNaturalHeight;
      }
      const isQuarterTurn = Math.abs(Math.round(visualRotation / 90)) % 2 === 1;
      const pw = currentPhoto?.width || 1;
      const ph = currentPhoto?.height || 1;
      return isQuarterTurn ? ph / pw : pw / ph;
    }

    if (activeBaseStep?.params && typeof activeBaseStep.params === 'object') {
      const p = activeBaseStep.params as { width?: number; height?: number };
      if (p.width && p.height && currentPhoto?.width && currentPhoto?.height) {
        const w = currentPhoto.width * p.width;
        const h = currentPhoto.height * p.height;
        if (h > 0) return w / h;
      }
    }
    if (renderedNaturalWidth > 0 && renderedNaturalHeight > 0) {
      return renderedNaturalWidth / renderedNaturalHeight;
    }
    if (currentPhoto?.width && currentPhoto?.height && currentPhoto.height > 0) {
      return currentPhoto.width / currentPhoto.height;
    }
    if (imageContainerRef) {
      const rect = imageContainerRef.getBoundingClientRect();
      if (rect.height > 0) return rect.width / rect.height;
    }
    return 1.333;
  });

  let straightenZoom = $derived(getStraightenZoom(straightenAngle, activePhotoAspect));

  let effectiveCanvasRotation = $derived.by(() => {
    if (activeTool === 'crop' || activeTool === 'straighten' || activeTool === 'redeye' || activeTool === 'retouch' || activeTool === 'text') {
      return 0;
    }
    const bakedRotation = activeBaseStep?.cumulative_rotation ?? 0;
    const baseRotation = visualRotation - bakedRotation;
    return baseRotation;
  });

  // Helper: Computes the continuous angle nearest to currentAngle that matches targetNorm (0, 90, 180, 270)
  // Prevents spinning 270 degrees in reverse when crossing between 270° and 0°!
  function snapToNearestAngle(currentAngle: number, targetNorm: number): number {
    const normTarget = ((targetNorm % 360) + 360) % 360;
    const normCurrent = ((currentAngle % 360) + 360) % 360;
    let delta = normTarget - normCurrent;
    if (delta > 180) delta -= 360;
    if (delta <= -180) delta += 360;
    return currentAngle + delta;
  }

  // Reset tool & slider states when navigating to a new photo
  function resetLocalToolState(photo?: Photo | null) {
    if (transitionTimer) {
      clearTimeout(transitionTimer);
      transitionTimer = null;
    }
    isTransitioning = false;
    // Immediately initialize to the photo's known draft rotation or cached session rotation (eliminates FOUC)
    const initialRot = photo?.edit_rotation ?? (photo ? getCachedEditSession(photo.id)?.cumulative_rotation : null) ?? 0;
    visualRotation = initialRot;
    straightenAngle = 0;
    activeTool = null;
    renderedNaturalWidth = 0;
    renderedNaturalHeight = 0;
    fillLight = 0;
    isAdjustingFillLight = false;
    highlights = 0;
    shadows = 0;
    colorTemp = 0;
    activeEffect = null;
    textItems = [];
    activeTextId = null;
    textDrag = null;
    textHud = null;
  }

  let lastCommittedFillLight: number | null = null;

  function syncSliderFromSession(sess: EditSessionState | null) {
    if (!sess || !sess.has_session || !sess.steps || sess.steps.length === 0) {
      fillLight = 0;
      lastCommittedFillLight = 0;
      return;
    }
    const maxIdx = Math.min(sess.current_step, sess.steps.length - 1);
    for (let i = maxIdx; i >= 1; i--) {
      const step = sess.steps[i];
      if (step.op === 'filllight' || step.op === 'fill') {
        const amt = (step.params as any)?.amount;
        if (typeof amt === 'number') {
          fillLight = Math.round(amt * 100);
          lastCommittedFillLight = amt;
          return;
        }
      }
    }
    fillLight = 0;
    lastCommittedFillLight = 0;
  }

  async function loadSession() {
    if (!currentPhoto) return;
    const targetPhotoId = currentPhoto.id;
    try {
      const sess = await getEditSession(targetPhotoId);
      if (currentPhoto?.id !== targetPhotoId) return;
      sessionState = sess;
      syncSliderFromSession(sess);
      if (sess.has_session && !sess.is_committed) {
        // Without CSS transition animation, cleanly sync the angle
        visualRotation = snapToNearestAngle(visualRotation, sess.cumulative_rotation);
      }
    } catch {
      if (currentPhoto?.id === targetPhotoId) {
        sessionState = null;
        fillLight = 0;
      }
    }
  }

  let lastPhotoId: number | null = null;
  $effect(() => {
    if (currentPhoto && isOpen) {
      if (currentPhoto.id !== lastPhotoId) {
        lastPhotoId = currentPhoto.id;
        isMainImageLoading = true;
        editQueue = Promise.resolve();
        isFav = currentPhoto.is_favorite;
        zoomLevel = 1.0;
        panX = 0;
        panY = 0;
        isPanning = false;
        resetLocalToolState(currentPhoto);
        loadSession();
        loadFacesForPhoto(currentPhoto.id);
        loadMetadataForPhoto(currentPhoto.id);
      }
    } else if (!isOpen) {
      lastPhotoId = null;
      zoomLevel = 1.0;
      panX = 0;
      panY = 0;
      isPanning = false;
      faces = [];
      photoMetadata = null;
      if (transitionTimer) {
        clearTimeout(transitionTimer);
        transitionTimer = null;
      }
      isTransitioning = false;
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
    const isQuarterTurn = Math.abs(Math.round(effectiveCanvasRotation / 90)) % 2 === 1;
    if (!isQuarterTurn || !canvasWidth || !canvasHeight) {
      return 1;
    }
    const padding = 48;
    const availW = Math.max(100, canvasWidth - padding);
    const availH = Math.max(100, canvasHeight - padding);

    const aspect = activePhotoAspect;
    if (!aspect || aspect <= 0) return 1;

    let baseW: number;
    let baseH: number;
    if (availW / availH > aspect) {
      baseH = availH;
      baseW = availH * aspect;
    } else {
      baseW = availW;
      baseH = availW / aspect;
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
    if (isAdjustingFillLight && fillLight > 0) parts.push('url(#pica-fill-light-filter)');
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
    if (!currentPhoto || activeTool === 'crop') return;
    const photoId = currentPhoto.id;

    // 1. Smooth physical glide on client canvas immediately (0ms latency visual feedback)
    triggerAnimatedRotation(() => {
      visualRotation += deg;
    });

    // 2. Perform background write & update sequentially through the queue
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await applyRotation(photoId, deg);
        if (res.success && res.photo) {
          sessionState = res.session_state;
          if (currentPhoto && currentPhoto.id === photoId) {
            currentPhoto.edit_rotation = res.session_state?.cumulative_rotation ?? 0;
          }
        } else if (res.session_state?.has_conflict) {
          sessionState = res.session_state;
        }
      } catch (e) {
        console.error('Failed to rotate:', e);
        showToast('Error rotating image');
      } finally {
        isEditing = false;
      }
    });
  }

  let isApplyingLucky = $state(false);

  async function handleApplyFeelingLucky() {
    if (!currentPhoto || isApplyingLucky || isEditing) return;
    const photoId = currentPhoto.id;
    isApplyingLucky = true;

    enqueueEdit(async () => {
      try {
        const res = await applyFeelingLucky(photoId);
        if (res.success && res.session_state) {
          sessionState = res.session_state;
          cacheBuster = Date.now();
          showToast("I'm Feeling Lucky applied!");
          onPhotoEdited();
        } else if (res.session_state?.has_conflict) {
          sessionState = res.session_state;
        } else if (res.message) {
          showToast(res.message);
        }
      } catch (e: any) {
        console.error('Failed to apply I\'m Feeling Lucky:', e);
        showToast('Error applying I\'m Feeling Lucky');
      } finally {
        isApplyingLucky = false;
      }
    });
  }

  let isApplyingAutoContrast = $state(false);

  async function handleApplyAutoContrast() {
    if (!currentPhoto || isApplyingAutoContrast || isEditing) return;
    const photoId = currentPhoto.id;
    isApplyingAutoContrast = true;

    enqueueEdit(async () => {
      try {
        const res = await applyAutoContrast(photoId);
        if (res.success && res.session_state) {
          sessionState = res.session_state;
          cacheBuster = Date.now();
          showToast('Auto Contrast applied!');
          onPhotoEdited();
        } else if (res.session_state?.has_conflict) {
          sessionState = res.session_state;
        } else if (res.message) {
          showToast(res.message);
        }
      } catch (e: any) {
        console.error('Failed to apply Auto Contrast:', e);
        showToast('Error applying Auto Contrast');
      } finally {
        isApplyingAutoContrast = false;
      }
    });
  }

  let isApplyingAutoColor = $state(false);

  async function handleApplyAutoColor() {
    if (!currentPhoto || isApplyingAutoColor || isEditing) return;
    const photoId = currentPhoto.id;
    isApplyingAutoColor = true;

    enqueueEdit(async () => {
      try {
        const res = await applyAutoColor(photoId);
        if (res.success && res.session_state) {
          sessionState = res.session_state;
          cacheBuster = Date.now();
          showToast('Auto Color applied!');
          onPhotoEdited();
        } else if (res.session_state?.has_conflict) {
          sessionState = res.session_state;
        } else if (res.message) {
          showToast(res.message);
        }
      } catch (e: any) {
        console.error('Failed to apply Auto Color:', e);
        showToast('Error applying Auto Color');
      } finally {
        isApplyingAutoColor = false;
      }
    });
  }

  let undoLabel = $derived.by(() => {
    if (!sessionState || !sessionState.can_undo || !sessionState.steps || sessionState.steps.length === 0) {
      return 'Undo';
    }
    const currentStepObj = sessionState.steps[sessionState.current_step];
    if (!currentStepObj) return 'Undo';
    switch (currentStepObj.op) {
      case 'enhance':
      case 'lucky':
        return "Undo I'm Feeling Lucky";
      case 'autocontrast':
        return 'Undo Auto Contrast';
      case 'autocolor':
        return 'Undo Auto Color';
      case 'crop':
        return 'Undo Crop';
      case 'straighten':
        return 'Undo Straighten';
      case 'rotate':
        return 'Undo Rotate';
      case 'redeye':
        return 'Undo Red-Eye';
      case 'retouch':
      case 'spotblend':
        return 'Undo Retouch';
      case 'text':
        return 'Undo Text';
      case 'fill':
      case 'filllight':
        return 'Undo Fill Light';
      default:
        return 'Undo';
    }
  });

  let redoLabel = $derived.by(() => {
    if (!sessionState || !sessionState.can_redo || !sessionState.steps) {
      return 'Redo';
    }
    const nextStepObj = sessionState.steps[sessionState.current_step + 1];
    if (!nextStepObj) return 'Redo';
    switch (nextStepObj.op) {
      case 'enhance':
      case 'lucky':
        return "Redo I'm Feeling Lucky";
      case 'autocontrast':
        return 'Redo Auto Contrast';
      case 'autocolor':
        return 'Redo Auto Color';
      case 'crop':
        return 'Redo Crop';
      case 'straighten':
        return 'Redo Straighten';
      case 'rotate':
        return 'Redo Rotate';
      case 'redeye':
        return 'Redo Red-Eye';
      case 'retouch':
      case 'spotblend':
        return 'Redo Retouch';
      case 'text':
        return 'Redo Text';
      case 'fill':
      case 'filllight':
        return 'Redo Fill Light';
      default:
        return 'Redo';
    }
  });

  async function handleCommitFillLight(val: number) {
    if (!currentPhoto || isEditing) return;
    const photoId = currentPhoto.id;
    const amountToApply = Math.max(0, Math.min(100, val)) / 100;
    if (lastCommittedFillLight === amountToApply && !isAdjustingFillLight) {
      return;
    }

    enqueueEdit(async () => {
      isEditing = true;
      isApplyingFillLight = true;
      try {
        const res = await applyFillLight(photoId, amountToApply);
        if (res.success && res.session_state) {
          sessionState = res.session_state;
          lastCommittedFillLight = amountToApply;
          cacheBuster = Date.now();
          syncSliderFromSession(res.session_state);
          onPhotoEdited();
        } else if (res.session_state?.has_conflict) {
          sessionState = res.session_state;
        } else if (res.message) {
          showToast(res.message);
        }
      } catch (e: any) {
        console.error('Failed to apply Fill Light:', e);
        showToast('Error applying Fill Light');
      } finally {
        isEditing = false;
        isApplyingFillLight = false;
        isAdjustingFillLight = false;
      }
    });
  }

  async function handleUndo() {
    if (!currentPhoto || !sessionState?.can_undo) return;
    const photoId = currentPhoto.id;
    const actionLabel = undoLabel;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await undoEdit(photoId);
        if (res.success) {
          sessionState = res.session_state;
          syncSliderFromSession(res.session_state);
          cacheBuster = Date.now();
          if (activeTool === 'text') {
            textItems = [];
            activeTextId = null;
          }
          if (currentPhoto && currentPhoto.id === photoId) {
            currentPhoto.edit_rotation = res.session_state?.cumulative_rotation ?? 0;
          }
          triggerAnimatedRotation(() => {
            visualRotation = snapToNearestAngle(visualRotation, res.session_state.cumulative_rotation);
          });
          showToast(`${actionLabel} applied`);
          onPhotoEdited();
        }
      } catch (e) {
        console.error('Failed to undo:', e);
      } finally {
        isEditing = false;
        isAdjustingFillLight = false;
      }
    });
  }

  async function handleRedo() {
    if (!currentPhoto || !sessionState?.can_redo) return;
    const photoId = currentPhoto.id;
    const actionLabel = redoLabel;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await redoEdit(photoId);
        if (res.success) {
          sessionState = res.session_state;
          syncSliderFromSession(res.session_state);
          cacheBuster = Date.now();
          if (activeTool === 'text') {
            textItems = [];
            activeTextId = null;
          }
          if (currentPhoto && currentPhoto.id === photoId) {
            currentPhoto.edit_rotation = res.session_state?.cumulative_rotation ?? 0;
          }
          triggerAnimatedRotation(() => {
            visualRotation = snapToNearestAngle(visualRotation, res.session_state.cumulative_rotation);
          });
          showToast(`${actionLabel} applied`);
          onPhotoEdited();
        }
      } catch (e) {
        console.error('Failed to redo:', e);
      } finally {
        isEditing = false;
        isAdjustingFillLight = false;
      }
    });
  }

  async function handleJumpToStep(stepId: number) {
    if (!currentPhoto || stepId === sessionState?.current_step) return;
    const photoId = currentPhoto.id;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await jumpToEditStep(photoId, stepId);
        if (res.success) {
          sessionState = res.session_state;
          syncSliderFromSession(res.session_state);
          if (currentPhoto && currentPhoto.id === photoId) {
            currentPhoto.edit_rotation = res.session_state?.cumulative_rotation ?? 0;
          }
          triggerAnimatedRotation(() => {
            visualRotation = snapToNearestAngle(visualRotation, res.session_state.cumulative_rotation);
          });
          showToast(`Jumped to step #${stepId}`);
        }
      } catch (e) {
        console.error('Failed to jump to step:', e);
      } finally {
        isEditing = false;
      }
    });
  }

  async function handleSaveSession() {
    if (!currentPhoto) return;
    const photoId = currentPhoto.id;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const sess = await saveEditSession(photoId);
        sessionState = sess;
        if (currentPhoto && currentPhoto.id === photoId) {
          currentPhoto.edit_rotation = 0;
        }
        isTransitioning = false;
        visualRotation = 0;
        cacheBuster = Date.now();
        showToast('Edits saved to disk');
        onPhotoEdited();
      } catch (e) {
        console.error('Failed to save session:', e);
        showToast('Error saving edits');
      } finally {
        isEditing = false;
      }
    });
  }

  async function handleRevert() {
    if (!currentPhoto) return;
    const photoId = currentPhoto.id;
    enqueueEdit(async () => {
      isEditing = true;
      try {
        const res = await revertEditSession(photoId);
        if (res.success && res.photo) {
          if (currentPhoto && currentPhoto.id === photoId) {
            currentPhoto.file_hash = res.photo.file_hash;
            currentPhoto.file_size = res.photo.file_size;
            currentPhoto.width = res.photo.width;
            currentPhoto.height = res.photo.height;
            currentPhoto.edit_rotation = 0;
            sessionState = res.session_state;
            isTransitioning = false;
            visualRotation = 0;
            cacheBuster = Date.now();
            resetLocalToolState(currentPhoto);
          }
          showToast('Reverted to original photo');
          onPhotoEdited();
        }
      } catch (e) {
        console.error('Failed to revert:', e);
        showToast('Error reverting to original');
      } finally {
        isEditing = false;
      }
    });
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

  function resetZoom() {
    zoomLevel = 1.0;
    panX = 0;
    panY = 0;
    isPanning = false;
  }

  function toggleZoom() {
    if (zoomLevel > 1.05) {
      resetZoom();
    } else {
      // Zoom into 2.0x (100% detail view) centered
      zoomLevel = 2.0;
      panX = 0;
      panY = 0;
    }
  }

  function handleWheel(e: WheelEvent) {
    if (comparisonMode !== '1up') return;
    // Don't intercept wheel if interacting with tools that require wheel/scrolling
    e.preventDefault();

    isWheelZooming = true;
    if (wheelZoomTimer) clearTimeout(wheelZoomTimer);
    wheelZoomTimer = setTimeout(() => {
      isWheelZooming = false;
      wheelZoomTimer = null;
    }, 140);

    const zoomFactor = Math.exp(-e.deltaY * 0.002);
    const newZoom = Math.max(1.0, Math.min(8.0, zoomLevel * zoomFactor));

    if (newZoom <= 1.001) {
      resetZoom();
      return;
    }

    if (imageContainerRef) {
      const rect = imageContainerRef.getBoundingClientRect();
      // Mouse position relative to center of the image container
      const mouseRelX = e.clientX - (rect.left + rect.width / 2);
      const mouseRelY = e.clientY - (rect.top + rect.height / 2);

      const ratio = newZoom / zoomLevel;
      // Adjust pan so the point under cursor remains stationary
      panX = (panX - mouseRelX) * ratio + mouseRelX;
      panY = (panY - mouseRelY) * ratio + mouseRelY;
    }

    zoomLevel = newZoom;
  }

  function handleViewerPointerDown(e: PointerEvent) {
    // Only left clicks
    if (e.button !== 0) return;
    // Do not pan if active tool is crop, straighten, redeye, retouch
    if (activeTool !== null) return;
    if (zoomLevel <= 1.05) return;

    isPanning = true;
    panStartPointer = { x: e.clientX, y: e.clientY };
    panStartOffset = { x: panX, y: panY };
    window.addEventListener('pointermove', handleViewerPointerMove);
    window.addEventListener('pointerup', handleViewerPointerUp);
    window.addEventListener('pointercancel', handleViewerPointerUp);
  }

  function handleViewerPointerMove(e: PointerEvent) {
    if (!isPanning) return;
    panX = panStartOffset.x + (e.clientX - panStartPointer.x);
    panY = panStartOffset.y + (e.clientY - panStartPointer.y);
  }

  function handleViewerPointerUp() {
    if (!isPanning) return;
    isPanning = false;
    window.removeEventListener('pointermove', handleViewerPointerMove);
    window.removeEventListener('pointerup', handleViewerPointerUp);
    window.removeEventListener('pointercancel', handleViewerPointerUp);
  }

  async function handleToggleFav() {
    if (!currentPhoto) return;
    const newState = await toggleFavorite(currentPhoto.id);
    isFav = newState;
    currentPhoto.is_favorite = newState;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!isOpen) return;

    // Ignore hotkeys when typing in text overlay input or contenteditable
    const target = e.target as HTMLElement | null;
    const isRangeInput = target instanceof HTMLInputElement && target.type === 'range';
    const isTextInput =
      (target instanceof HTMLInputElement && !['range', 'checkbox', 'radio', 'button'].includes(target.type)) ||
      target instanceof HTMLTextAreaElement ||
      (target?.isContentEditable ?? false);

    if (isTextInput) {
      if (e.key === 'Escape') {
        target?.blur();
        e.preventDefault();
      }
      return;
    }

    const isCtrlOrCmd = e.ctrlKey || e.metaKey;

    if (e.key === 'Escape') {
      if (isCropPreviewMode) {
        cancelCropPreview();
        return;
      }
      if (activeTool === 'crop') {
        handleCancelCrop();
      } else if (activeTool === 'redeye') {
        handleCancelRedEye();
      } else if (activeTool === 'retouch') {
        handleCancelRetouch();
      } else if (activeTool === 'text') {
        if (activeTextId !== null) {
          activeTextId = null;
        } else {
          handleCancelText();
        }
      } else if (activeTool !== null) {
        activeTool = null;
      } else {
        isOpen = false;
        onClose();
      }
    } else if (e.key === 'Enter') {
      if (activeTool === 'crop') {
        e.preventDefault();
        handleApplyCrop();
      } else if (activeTool === 'redeye') {
        e.preventDefault();
        handleApplyRedEye();
      } else if (activeTool === 'retouch') {
        e.preventDefault();
        handleApplyRetouch();
      } else if (activeTool === 'text') {
        e.preventDefault();
        handleApplyText();
      }
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && activeTool === 'text' && activeTextId !== null) {
      e.preventDefault();
      handleDeleteActiveText();
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (isRangeInput) target?.blur();
      if (e.shiftKey) {
        handleRedo();
      } else {
        handleUndo();
      }
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      if (isRangeInput) target?.blur();
      handleRedo();
    } else if (isCtrlOrCmd && e.key.toLowerCase() === 's') {
      e.preventDefault();
      handleSaveSession();
    } else if (e.key === 'ArrowRight' || e.key === 'j') {
      if (activeTool === 'crop') {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        cropBox = { ...cropBox, x: Math.min(1 - cropBox.width, cropBox.x + step) };
      } else if (activeTool === 'text' && activeTextId !== null) {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        textItems = textItems.map((item) => item.id === activeTextId ? { ...item, x: Math.min(1, item.x + step) } : item);
      } else if (activeTool === null) {
        nextPhoto();
      }
    } else if (e.key === 'ArrowLeft' || e.key === 'k') {
      if (activeTool === 'crop') {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        cropBox = { ...cropBox, x: Math.max(0, cropBox.x - step) };
      } else if (activeTool === 'text' && activeTextId !== null) {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        textItems = textItems.map((item) => item.id === activeTextId ? { ...item, x: Math.max(0, item.x - step) } : item);
      } else if (activeTool === null) {
        prevPhoto();
      }
    } else if (e.key === 'ArrowUp') {
      if (activeTool === 'crop') {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        cropBox = { ...cropBox, y: Math.max(0, cropBox.y - step) };
      } else if (activeTool === 'text' && activeTextId !== null) {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        textItems = textItems.map((item) => item.id === activeTextId ? { ...item, y: Math.max(0, item.y - step) } : item);
      }
    } else if (e.key === 'ArrowDown') {
      if (activeTool === 'crop') {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        cropBox = { ...cropBox, y: Math.min(1 - cropBox.height, cropBox.y + step) };
      } else if (activeTool === 'text' && activeTextId !== null) {
        e.preventDefault();
        const step = e.shiftKey ? 0.05 : 0.01;
        textItems = textItems.map((item) => item.id === activeTextId ? { ...item, y: Math.min(1, item.y + step) } : item);
      }
    } else if (e.key === ' ') {
      e.preventDefault();
      if (isCropPreviewMode) {
        cancelCropPreview();
      } else if (activeTool === null) {
        toggleZoom();
      }
    } else if (e.key === 'f') {
      if (activeTool === null) handleToggleFav();
    } else if (e.key === 'r') {
      if (activeTool === null) {
        if (e.shiftKey) {
          handleRotate(-90);
        } else {
          handleRotate(90);
        }
      }
    } else if (e.key === 'p') {
      if (activeTool === null) toggleSlideshow();
    } else if (e.key === 'i') {
      showInfo = !showInfo;
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);
  });

  onDestroy(() => {
    window.removeEventListener('keydown', handleKeyDown);
    window.removeEventListener('pointermove', onWindowPointerMove);
    window.removeEventListener('pointerup', onWindowPointerUp);
    window.removeEventListener('pointermove', handleViewerPointerMove);
    window.removeEventListener('pointerup', handleViewerPointerUp);
    window.removeEventListener('pointercancel', handleViewerPointerUp);
    if (toastTimer) clearTimeout(toastTimer);
    if (cropPreviewTimer) clearTimeout(cropPreviewTimer);
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

      <!-- 5 Picasa Editing Tabs (Icons with tooltips) or Dedicated Tool Header -->
      {#if activeTool === 'crop'}
        <div class="px-4 py-3 border-b flex items-center justify-between" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
          <div class="flex items-center gap-2">
            <Crop class="w-4 h-4 text-emerald-500" />
            <span class="font-bold text-xs tracking-wider uppercase">Crop Photo</span>
          </div>
          <button
            type="button"
            onclick={handleCancelCrop}
            class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer text-neutral-400 hover:text-inherit"
            title="Cancel Crop (Esc)"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {:else if activeTool === 'redeye'}
        <div class="px-4 py-3 border-b flex items-center justify-between" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
          <div class="flex items-center gap-2">
            <Eye class="w-4 h-4 text-rose-500" />
            <span class="font-bold text-xs tracking-wider uppercase">Redeye Repair</span>
          </div>
          <button
            type="button"
            onclick={handleCancelRedEye}
            class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer text-neutral-400 hover:text-inherit"
            title="Exit Red-Eye repair without applying effect"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {:else if activeTool === 'straighten'}
        <div class="px-4 py-3 border-b flex items-center justify-between" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
          <div class="flex items-center gap-2">
            <SlidersVertical class="w-4 h-4 text-cyan-500" />
            <span class="font-bold text-xs tracking-wider uppercase">Straighten Photo</span>
          </div>
          <button
            type="button"
            onclick={handleCancelStraighten}
            class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer text-neutral-400 hover:text-inherit"
            title="Cancel Straighten (Esc)"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {:else if activeTool === 'retouch'}
        <div class="px-4 py-3 border-b flex items-center justify-between" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
          <div class="flex items-center gap-2">
            <Wand class="w-4 h-4 text-amber-500" />
            <span class="font-bold text-xs tracking-wider uppercase">Retouch</span>
          </div>
          <button
            type="button"
            onclick={() => (activeTool = null)}
            class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer text-neutral-400 hover:text-inherit"
            title="Cancel Retouch"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {:else if activeTool === 'text'}
        <div class="px-4 py-3 border-b flex items-center justify-between" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
          <div class="flex items-center gap-2">
            <Type class="w-4 h-4 text-sky-500" />
            <span class="font-bold text-xs tracking-wider uppercase">Text Caption</span>
          </div>
          <button
            type="button"
            onclick={() => (activeTool = null)}
            class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer text-neutral-400 hover:text-inherit"
            title="Cancel Text Tool"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      {:else}
        <div class="grid {import.meta.env.DEV ? 'grid-cols-5' : 'grid-cols-1'} border-b" style="background-color: var(--header-bg); border-color: var(--border-subtle);">
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
            <span class="text-[10px] font-medium tracking-tight">{import.meta.env.DEV ? 'Basic' : 'Basic Fixes'}</span>
          </button>

          {#if import.meta.env.DEV}
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
          {/if}
        </div>
      {/if}

      <!-- Tab Content Area -->
      <div class="flex-1 overflow-y-auto p-3.5 space-y-4 text-xs">
        {#if activeTool === 'crop'}
          <!-- Dedicated Modal Crop Panel (Hides all other tools while cropping) -->
          <div class="space-y-4 animate-in fade-in duration-150">
            <!-- Helper Text matching Picasa -->
            <p class="text-[11px] leading-relaxed text-neutral-300 bg-[#22222a] p-3 rounded-lg border border-neutral-700/50">
              Select a dimension below and then click and drag on the image to select the portion of the image you wish to crop.
            </p>

            <!-- Aspect Ratio Dropdown -->
            <div>
              <label class="block text-[10px] uppercase font-bold text-neutral-400 tracking-wider mb-1.5" for="crop-aspect-select">
                Aspect Ratio
              </label>
              <select
                id="crop-aspect-select"
                value={cropAspect}
                onchange={(e) => {
                  applyAspectToCurrentCropBox((e.target as HTMLSelectElement).value as any);
                  loadCropSuggestions();
                }}
                class="w-full bg-[#121216] border border-neutral-700 hover:border-neutral-500 rounded-md px-3 py-2 text-xs text-neutral-200 focus:outline-none focus:border-emerald-500 cursor-pointer shadow-inner"
              >
                <option value="4x6">4 × 6 : Small print</option>
                <option value="5x7">5 × 7 : Standard print</option>
                <option value="8x10">8 × 10 : Large print</option>
                <option value="square">1 : 1 : Square</option>
                <option value="4:3">4 : 3 : Standard TV / Display</option>
                <option value="16:9">16 : 9 : Widescreen</option>
                <option value="original">Original photo aspect</option>
                <option value="free">Manual / Unconstrained</option>
              </select>
            </div>

            <!-- 3 Smart Mathematical Crop Suggestions -->
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-[10px] uppercase font-bold text-neutral-400 tracking-wider">Crop Suggestions</span>
                {#if isLoadingCropSuggestions}
                  <span class="text-[10px] text-emerald-400 font-medium animate-pulse">Analyzing image...</span>
                {/if}
              </div>

              <div class="grid grid-cols-3 gap-2">
                {#each cropSuggestions as sug}
                  {@const isSelected = Math.abs(cropBox.x - sug.x) < 0.02 && Math.abs(cropBox.y - sug.y) < 0.02 && Math.abs(cropBox.width - sug.width) < 0.02}
                  {@const targetAspect = getTargetAspectValue()}
                  {@const sugAspect = targetAspect != null ? targetAspect : Math.max(0.2, Math.min(5.0, (sug.width / Math.max(0.001, sug.height)) * activePhotoAspect))}
                  {@const boxW = sugAspect >= 68 / 76 ? 68 : Math.max(34, Math.round(76 * sugAspect))}
                  {@const boxH = sugAspect >= 68 / 76 ? Math.max(28, Math.round(68 / sugAspect)) : 76}
                  <button
                    type="button"
                    onclick={() => selectCropSuggestion(sug)}
                    class={`flex flex-col items-center justify-between p-1 rounded-lg border transition-all cursor-pointer group bg-[#181820] hover:bg-[#22222c] shadow-sm ${
                      isSelected
                        ? 'border-emerald-400 ring-2 ring-emerald-500/40 bg-emerald-950/20 shadow-emerald-950/50'
                        : 'border-neutral-700/80 hover:border-neutral-500'
                    }`}
                    title="{sug.label} ({Math.round(sug.width * 100)}% × {Math.round(sug.height * 100)}%)"
                  >
                    <!-- Centered Aspect-Ratio-Preserved Crop Preview Box -->
                    <div class="w-full h-[80px] flex items-center justify-center p-0.5">
                      <div
                        class={`relative overflow-hidden rounded bg-black/90 shadow-inner flex items-center justify-center transition-all ${
                          isSelected
                            ? 'border-2 border-emerald-400 ring-2 ring-emerald-500/50 shadow-emerald-950/60'
                            : 'border border-neutral-700/80 group-hover:border-neutral-400'
                        }`}
                        style="width: {boxW}px; height: {boxH}px;"
                      >
                        <img
                          src={previewThumbSrc}
                          alt={sug.label}
                          class="absolute select-none pointer-events-none"
                          style="
                            width: {(1 / Math.max(0.01, sug.width)) * 100}%;
                            height: {(1 / Math.max(0.01, sug.height)) * 100}%;
                            left: -{(sug.x / Math.max(0.01, sug.width)) * 100}%;
                            top: -{(sug.y / Math.max(0.01, sug.height)) * 100}%;
                            max-width: none !important;
                            max-height: none !important;
                            object-fit: fill;
                            display: block;
                          "
                          onerror={(e) => {
                            const el = e.currentTarget as HTMLImageElement;
                            if (activeImageSrc && el.src !== activeImageSrc) {
                              el.src = activeImageSrc;
                            }
                          }}
                        />
                      </div>
                    </div>
                    <div class="w-full text-center text-[10px] font-medium text-neutral-300 group-hover:text-emerald-300 truncate pt-1">
                      {sug.label}
                    </div>
                  </button>
                {/each}
              </div>
            </div>

            <!-- Action Buttons: Rotate & Preview -->
            <div class="grid grid-cols-2 gap-2 pt-1">
              <button
                type="button"
                onclick={rotateCropOrientation}
                class="flex items-center justify-center gap-1.5 py-2 px-3 rounded-md border border-neutral-700 hover:bg-[#282832] text-neutral-200 hover:text-white transition-colors cursor-pointer text-xs font-medium"
                title="Switch between portrait and landscape crop ratio"
              >
                <RotateCw class="w-3.5 h-3.5 text-blue-400" />
                <span>Rotate</span>
              </button>

              <button
                type="button"
                onclick={triggerCropPreview}
                class={`flex items-center justify-center gap-1.5 py-2 px-3 rounded-md border transition-all cursor-pointer text-xs font-medium shadow-sm ${
                  isCropPreviewMode
                    ? 'bg-emerald-600/30 border-emerald-500 text-emerald-300 ring-2 ring-emerald-500/30'
                    : 'border-neutral-700 hover:bg-[#282832] text-neutral-200 hover:text-white'
                }`}
                title="Preview cutout in full size (auto-reverts in 2.8s)"
              >
                <Eye class="w-3.5 h-3.5 text-emerald-400" />
                <span>{isCropPreviewMode ? 'Revert' : 'Preview'}</span>
              </button>
            </div>

            <!-- Action Buttons: Reset -->
            <div>
              <button
                type="button"
                onclick={resetCrop}
                class="w-full py-1.5 px-3 rounded-md border border-neutral-700/80 hover:bg-[#282832] text-neutral-300 hover:text-white transition-colors cursor-pointer text-xs"
                title="Reset crop selection to default"
              >
                Reset
              </button>
            </div>

            <!-- Action Buttons: Apply & Cancel (Iconic Picasa style) -->
            <div class="flex items-center gap-2 pt-2 border-t border-neutral-800">
              <button
                type="button"
                onclick={handleApplyCrop}
                class="flex-1 flex items-center justify-center gap-2 py-2 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-semibold transition-all cursor-pointer text-xs shadow-md active:scale-98"
                title="Apply Crop (Enter)"
              >
                <Check class="w-4 h-4" />
                <span>Apply</span>
              </button>

              <button
                type="button"
                onclick={handleCancelCrop}
                class="flex-1 flex items-center justify-center gap-2 py-2 rounded-md bg-neutral-700 hover:bg-neutral-600 text-neutral-200 hover:text-white transition-all cursor-pointer text-xs active:scale-98"
                title="Cancel Crop (Esc)"
              >
                <X class="w-4 h-4 text-red-400" />
                <span>Cancel</span>
              </button>
            </div>
          </div>
        {:else if activeTool === 'redeye'}
          <!-- Dedicated Modal Redeye Repair Panel (Picasa layout & copy) -->
          <div class="space-y-4 animate-in fade-in duration-150">
            <!-- Picasa Instructions matching human copy from screenshot -->
            <div class="p-3.5 rounded-lg border border-neutral-700/50 bg-[#22222a] space-y-2 text-[11px] leading-relaxed text-neutral-300">
              {#if isDetectingRedEyes}
                <div class="flex items-center gap-2 text-rose-400 py-1">
                  <Loader2 class="w-4 h-4 animate-spin" />
                  <span>Scanning for eyes...</span>
                </div>
              {:else if redEyeBoxes.length > 0}
                <p class="font-medium text-white">
                  Picasa has found and corrected red eye(s).
                </p>
                <p class="text-neutral-400">
                  <span class="font-semibold text-neutral-300">Note:</span> You can click on a box to delete a change.
                </p>
                <p class="text-neutral-400">
                  You can also draw a square around any red eye that Picasa may have missed.
                </p>
              {:else}
                <p>
                  Click, hold, and drag the mouse around each eye separately to select it. A selection box appears over the area. Release the mouse to complete your selection. The Photo is displayed with the red-eye removed.
                </p>
                <p class="text-neutral-400">
                  <span class="font-semibold text-neutral-300">Note:</span> You can click on a box to delete a change.
                </p>
                <p class="text-neutral-400">
                  You can also click 'auto' to reapply the auto-corrections.
                </p>
              {/if}
            </div>

            <!-- Action Controls Group matching Picasa layout -->
            <div class="space-y-2.5 pt-1">
              <!-- Row 1: Auto and Preview -->
              <div class="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onclick={handleAutoRedEye}
                  disabled={isDetectingRedEyes}
                  class="px-3 py-2 rounded-md bg-[#2a2a34] hover:bg-[#343440] active:bg-[#202028] text-neutral-200 hover:text-white border border-neutral-700/60 font-medium text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer disabled:opacity-50"
                  title="Reapply auto redeye corrections"
                >
                  {#if isDetectingRedEyes}
                    <Loader2 class="w-3.5 h-3.5 animate-spin" />
                  {:else}
                    <Sparkles class="w-3.5 h-3.5 text-rose-400" />
                  {/if}
                  <span>Auto</span>
                </button>

                <button
                  type="button"
                  onpointerdown={() => (isRedEyePreviewHidden = true)}
                  onpointerup={() => (isRedEyePreviewHidden = false)}
                  onpointerleave={() => (isRedEyePreviewHidden = false)}
                  onclick={() => (isRedEyePreviewHidden = !isRedEyePreviewHidden)}
                  class={`px-3 py-2 rounded-md border font-medium text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer select-none ${
                    isRedEyePreviewHidden
                      ? 'bg-rose-950/60 border-rose-500/60 text-rose-200'
                      : 'bg-[#2a2a34] hover:bg-[#343440] border-neutral-700/60 text-neutral-200 hover:text-white'
                  }`}
                  title="Preview changes without square outlines (click to toggle, or press and hold)"
                >
                  <Eye class="w-3.5 h-3.5 text-blue-400" />
                  <span>Preview</span>
                </button>
              </div>

              <!-- Row 2: Reset -->
              <div>
                <button
                  type="button"
                  onclick={handleResetRedEye}
                  disabled={redEyeBoxes.length === 0}
                  class="w-full px-3 py-2 rounded-md bg-[#2a2a34] hover:bg-[#343440] text-neutral-300 hover:text-white border border-neutral-700/60 font-medium text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                  title="Undo Red-Eye changes"
                >
                  <RotateCcw class="w-3.5 h-3.5 text-amber-400" />
                  <span>Reset</span>
                </button>
              </div>

              <!-- Row 3: Apply and Cancel with check & cancel icons -->
              <div class="grid grid-cols-2 gap-2 pt-2 border-t border-neutral-800">
                <button
                  type="button"
                  onclick={handleApplyRedEye}
                  disabled={isApplyingRedEye}
                  class="px-3 py-2.5 rounded-md bg-emerald-600 hover:bg-emerald-500 active:bg-emerald-700 text-white font-semibold text-xs transition-colors flex items-center justify-center gap-1.5 shadow-sm cursor-pointer disabled:opacity-50"
                  title="Apply effect and exit Red-Eye repair"
                >
                  {#if isApplyingRedEye}
                    <Loader2 class="w-4 h-4 animate-spin" />
                  {:else}
                    <Check class="w-4 h-4 stroke-[2.5]" />
                  {/if}
                  <span>Apply</span>
                </button>

                <button
                  type="button"
                  onclick={handleCancelRedEye}
                  class="px-3 py-2.5 rounded-md bg-neutral-700 hover:bg-neutral-600 active:bg-neutral-800 text-neutral-200 hover:text-white font-medium text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer"
                  title="Exit Red-Eye repair without applying effect"
                >
                  <X class="w-4 h-4 text-rose-400 stroke-[2.5]" />
                  <span>Cancel</span>
                </button>
              </div>
            </div>
          </div>
        {:else if activeTool === 'straighten'}
          <div class="space-y-4 animate-in fade-in duration-150">
            <p class="text-[11px] leading-relaxed p-3 rounded-lg border" style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-muted);">
              Drag the slider to level the photo. A grid is displayed over the image to help you align with the horizon.
            </p>

            <div class="p-3 border rounded-lg space-y-3" style="background-color: var(--card-bg); border-color: var(--card-border);">
              <div class="flex items-center justify-between text-[11px]" style="color: var(--text-muted);">
                <span>Horizon Level:</span>
                <span class="font-mono text-cyan-500 font-bold">{formatStraightenAngle(straightenAngle)}</span>
              </div>
              <input
                type="range"
                min="-45"
                max="45"
                step="0.1"
                bind:value={straightenAngle}
                class="w-full accent-cyan-500 cursor-pointer"
              />

              <!-- Suggestions -->
              <div>
                <div class="flex items-center justify-between mb-1.5 pt-1">
                  <span class="text-[10px] uppercase font-bold tracking-wider" style="color: var(--text-dim);">Level Suggestions</span>
                  {#if isLoadingStraightenSuggestions}
                    <span class="text-[10px] text-cyan-500 font-medium animate-pulse">Analyzing lines...</span>
                  {/if}
                </div>

                <div class="grid grid-cols-3 gap-2">
                  {#each straightenSuggestions as sug}
                    {@const isSelected = Math.abs(straightenAngle - sug.angle) < 0.15}
                    {@const sugZoom = getStraightenZoom(sug.angle, activePhotoAspect)}
                    <button
                      type="button"
                      onclick={() => selectStraightenSuggestion(sug)}
                      class={`flex flex-col items-center justify-between p-1.5 rounded-lg border transition-all cursor-pointer group shadow-xs ${
                        isSelected
                          ? 'border-cyan-500 ring-2 ring-cyan-500/40 bg-cyan-950/20'
                          : 'border-neutral-300 dark:border-neutral-700/80 hover:border-neutral-500'
                      }`}
                      style="background-color: var(--card-bg);"
                      title="{sug.label}: {formatStraightenAngle(sug.angle)} ({sug.description})"
                    >
                      <div class="w-full h-[54px] flex items-center justify-center p-0.5">
                        <div
                          class={`relative w-full h-full overflow-hidden rounded bg-black/90 shadow-inner flex items-center justify-center transition-all ${
                            isSelected
                              ? 'border-2 border-cyan-500 ring-2 ring-cyan-500/50'
                              : 'border border-neutral-700/80 group-hover:border-neutral-400'
                          }`}
                        >
                          <img
                            src={previewThumbSrc}
                            alt={sug.label}
                            class="max-w-none w-full h-full object-cover select-none pointer-events-none"
                            style="
                              transform: rotate({sug.angle}deg) scale({sugZoom});
                              transform-origin: center center;
                            "
                            onerror={(e) => {
                              const el = e.currentTarget as HTMLImageElement;
                              if (activeImageSrc && el.src !== activeImageSrc) {
                                el.src = activeImageSrc;
                              }
                            }}
                          />
                        </div>
                      </div>

                      <div class="w-full text-center mt-1">
                        <div class="text-[10px] font-semibold truncate group-hover:text-cyan-500 transition-colors" style="color: var(--text-main);">
                          {sug.label}
                        </div>
                        <div class="text-[9px] font-mono {isSelected ? 'text-cyan-500 font-bold' : 'text-neutral-400'}">
                          {formatStraightenAngle(sug.angle)}
                        </div>
                      </div>
                    </button>
                  {/each}
                </div>
              </div>

              <div class="flex items-center gap-2 pt-1 border-t" style="border-color: var(--border-subtle);">
                <button
                  type="button"
                  onclick={handleApplyStraighten}
                  class="flex-1 py-1.5 rounded-md bg-cyan-600 hover:bg-cyan-500 text-white font-semibold transition-colors cursor-pointer text-xs shadow-xs"
                >
                  Apply
                </button>
                <button
                  type="button"
                  onclick={handleCancelStraighten}
                  class="px-3 py-1.5 rounded-md border text-xs transition-colors cursor-pointer"
                  style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-muted);"
                >
                  Reset
                </button>
              </div>
            </div>
          </div>

        {:else if activeTool === 'retouch'}
          <div class="space-y-4 animate-in fade-in duration-150">
            <!-- Picasa Explanatory Prompt -->
            <div class="p-3 rounded-lg border text-[11px] leading-relaxed space-y-1.5" style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-muted);">
              {#if pendingRetouchTarget}
                <p class="font-medium text-amber-400">
                  Step 2: Move cursor to donor skin/texture patch and click to heal blemish.
                </p>
              {:else}
                <p>
                  <strong class="text-neutral-200">Healer Brush (SpotBlend)</strong>: Click on a blemish, dust spec, or wrinkle to place the target circle.
                </p>
              {/if}
            </div>

            <div class="p-3 border rounded-lg space-y-3" style="background-color: var(--card-bg); border-color: var(--card-border);">
              <!-- Radius slider -->
              <div class="space-y-1.5">
                <div class="flex items-center justify-between text-[11px]" style="color: var(--text-muted);">
                  <span>Brush Radius:</span>
                  <span class="font-mono text-amber-500 font-bold">{retouchRadius}px</span>
                </div>
                <input
                  type="range"
                  min="6"
                  max="120"
                  bind:value={retouchRadius}
                  class="w-full accent-amber-500 cursor-pointer"
                />
              </div>

              <!-- Live Spots Count & Show Circles Checkbox (Picasa Heritage: Off by default) -->
              <div class="flex items-center justify-between pt-1 text-xs" style="color: var(--text-muted);">
                <span>Healed spots: <strong class="text-neutral-200">{retouchSpots.length}</strong></span>
                <label class="flex items-center gap-1.5 cursor-pointer select-none text-xs text-neutral-300 hover:text-white transition-colors">
                  <input
                    type="checkbox"
                    bind:checked={showRetouchCircles}
                    class="rounded border-neutral-600 bg-neutral-800 text-amber-500 focus:ring-0 focus:ring-offset-0 cursor-pointer w-3.5 h-3.5 accent-amber-500"
                  />
                  <span>Show circles</span>
                </label>
              </div>

              <!-- Reset Button -->
              <div>
                <button
                  type="button"
                  onclick={handleResetRetouch}
                  disabled={retouchSpots.length === 0 && !pendingRetouchTarget}
                  class="w-full px-3 py-1.5 rounded-md border text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
                  style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-muted);"
                  title="Clear pending retouch spots"
                >
                  <RotateCcw class="w-3.5 h-3.5 text-amber-400" />
                  <span>Reset All Spots</span>
                </button>
              </div>

              <!-- Row: Apply and Cancel with check & cancel icons -->
              <div class="grid grid-cols-2 gap-2 pt-2 border-t" style="border-color: var(--border-subtle);">
                <button
                  type="button"
                  onclick={handleApplyRetouch}
                  disabled={isApplyingRetouch || (retouchSpots.length === 0 && !pendingRetouchTarget)}
                  class="px-3 py-2 rounded-md bg-emerald-600 hover:bg-emerald-500 active:bg-emerald-700 text-white font-semibold text-xs transition-colors flex items-center justify-center gap-1.5 shadow-sm cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                  title="Apply retouch spots to image"
                >
                  {#if isApplyingRetouch}
                    <Loader2 class="w-4 h-4 animate-spin" />
                  {:else}
                    <Check class="w-4 h-4 stroke-[2.5]" />
                  {/if}
                  <span>Apply</span>
                </button>

                <button
                  type="button"
                  onclick={handleCancelRetouch}
                  class="px-3 py-2 rounded-md border text-xs transition-colors flex items-center justify-center gap-1.5 cursor-pointer"
                  style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-muted);"
                  title="Exit Retouch without applying changes"
                >
                  <X class="w-4 h-4 text-rose-400 stroke-[2.5]" />
                  <span>Cancel</span>
                </button>
              </div>
            </div>
          </div>

        {:else if activeTool === 'text'}
          <div class="space-y-3.5 animate-in fade-in duration-150 select-none text-xs">
            <!-- Header Banner matching Screenshot 1 -->
            <div class="flex items-center gap-2.5 pb-2 border-b border-[#3a3a44]">
              <img src={textPicasaIcon} srcset="{textPicasaIcon2x} 2x" alt="Edit Text" class="w-8 h-8 rounded shrink-0 shadow-xs object-contain" />
              <div class="flex flex-col">
                <span class="text-sm font-semibold tracking-wide text-neutral-100">Edit Text</span>
                <span class="text-[10px] text-neutral-400">Click photo to place or type text</span>
              </div>
            </div>

            <!-- Controls Container -->
            <div class="p-3.5 border rounded-xl space-y-3 bg-[#1e222b] border-[#313947] shadow-lg text-neutral-200">
              
              <!-- Row 1: Font Family -->
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] font-medium text-neutral-300 w-16 text-right shrink-0">Font:</span>
                <div class="relative flex-1">
                  <select
                    value={selectedFontFamily}
                    onchange={(e) => setFontFamily(e.currentTarget.value)}
                    class="w-full appearance-none pl-2.5 pr-7 py-1.5 rounded-md bg-[#13161c] border border-[#3a4454] text-neutral-100 text-xs focus:outline-none focus:border-sky-400 cursor-pointer shadow-inner"
                  >
                    {#each FONT_FAMILIES as font}
                      <option value={font} style="font-family: {font};">{font}</option>
                    {/each}
                  </select>
                  <div class="absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none text-neutral-400">
                    <svg class="w-3 h-3 fill-current" viewBox="0 0 20 20"><path d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z"/></svg>
                  </div>
                </div>
              </div>

              <!-- Row 2: Size & Style -->
              <div class="flex items-center justify-between gap-2">
                <div class="flex items-center gap-1.5">
                  <span class="text-[11px] font-medium text-neutral-300 w-16 text-right shrink-0">Size:</span>
                  <div class="relative w-18">
                    <select
                      value={selectedFontSize}
                      onchange={(e) => setFontSize(parseInt(e.currentTarget.value, 10))}
                      class="w-full appearance-none pl-2 pr-6 py-1.5 rounded-md bg-[#13161c] border border-[#3a4454] text-neutral-100 text-xs focus:outline-none focus:border-sky-400 cursor-pointer shadow-inner"
                    >
                      {#if !FONT_SIZES.includes(selectedFontSize)}
                        <option value={selectedFontSize}>{selectedFontSize}</option>
                      {/if}
                      {#each FONT_SIZES as sz}
                        <option value={sz}>{sz}</option>
                      {/each}
                    </select>
                    <div class="absolute right-1.5 top-1/2 -translate-y-1/2 pointer-events-none text-neutral-400">
                      <svg class="w-2.5 h-2.5 fill-current" viewBox="0 0 20 20"><path d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z"/></svg>
                    </div>
                  </div>
                </div>

                <!-- Style Buttons: Bold, Italic, Underline -->
                <div class="flex items-center gap-1">
                  <span class="text-[10px] text-neutral-400 mr-0.5">Style:</span>
                  <button
                    type="button"
                    onclick={toggleBold}
                    class="w-7 h-7 rounded-md flex items-center justify-center font-serif font-bold text-sm border transition-all cursor-pointer shadow-xs {selectedBold ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Bold"
                  >
                    B
                  </button>
                  <button
                    type="button"
                    onclick={toggleItalic}
                    class="w-7 h-7 rounded-md flex items-center justify-center font-serif italic text-sm border transition-all cursor-pointer shadow-xs {selectedItalic ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Italic"
                  >
                    I
                  </button>
                  <button
                    type="button"
                    onclick={toggleUnderline}
                    class="w-7 h-7 rounded-md flex items-center justify-center font-serif underline text-sm border transition-all cursor-pointer shadow-xs {selectedUnderline ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Underline"
                  >
                    U
                  </button>
                </div>
              </div>

              <!-- Row 3: Alignment -->
              <div class="flex items-center justify-between gap-2">
                <span class="text-[11px] font-medium text-neutral-300 w-16 text-right shrink-0">Alignment:</span>
                <div class="flex items-center gap-1.5 flex-1">
                  <button
                    type="button"
                    onclick={() => setAlignment('left')}
                    class="flex-1 py-1.5 rounded-md flex items-center justify-center border transition-all cursor-pointer shadow-xs {selectedAlignment === 'left' ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Align Left"
                  >
                    <AlignLeft class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    onclick={() => setAlignment('center')}
                    class="flex-1 py-1.5 rounded-md flex items-center justify-center border transition-all cursor-pointer shadow-xs {selectedAlignment === 'center' ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Align Center"
                  >
                    <AlignCenter class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    onclick={() => setAlignment('right')}
                    class="flex-1 py-1.5 rounded-md flex items-center justify-center border transition-all cursor-pointer shadow-xs {selectedAlignment === 'right' ? 'bg-sky-600/50 border-sky-400 text-white shadow-inner' : 'bg-gradient-to-b from-[#2a303d] to-[#1e232c] border-[#3c4656] text-neutral-200 hover:text-white hover:border-[#525f75]'}"
                    title="Align Right"
                  >
                    <AlignRight class="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              <div class="h-px bg-[#323946] my-1"></div>

              <!-- Row 4: Colors & Outlines matching Screenshot 1 -->
              <div class="flex items-center justify-between gap-3">
                <!-- Text Fill Color -->
                <div class="flex items-center gap-1.5">
                  <span class="font-bold text-neutral-400 text-xs">T</span>
                  <label
                    class="relative w-6 h-6 rounded-full border-2 border-white/80 shadow-md cursor-pointer block overflow-hidden shrink-0 hover:scale-105 transition-transform"
                    style="background-color: {selectedHasFill ? selectedFillColor : 'transparent'};"
                    title="Text Fill Color"
                  >
                    <input
                      type="color"
                      value={selectedFillColor}
                      oninput={(e) => setFillColor(e.currentTarget.value)}
                      class="opacity-0 absolute inset-0 w-full h-full cursor-pointer"
                    />
                  </label>
                  <button
                    type="button"
                    onclick={toggleNoFill}
                    class="w-6 h-6 rounded-md border flex items-center justify-center transition-all cursor-pointer shadow-xs {selectedHasFill ? 'bg-[#1e222a] border-[#3b4454] text-neutral-400 hover:text-white' : 'bg-rose-950/60 border-rose-500 text-rose-300 shadow-inner'}"
                    title={selectedHasFill ? 'Disable Fill (Transparent)' : 'Enable Fill'}
                  >
                    <div class="relative w-3.5 h-3.5 bg-white border border-neutral-400 rounded-xs flex items-center justify-center overflow-hidden">
                      <div class="absolute w-5 h-0.5 bg-rose-600 rotate-45"></div>
                    </div>
                  </button>
                </div>

                <!-- Text Outline Color & Station Slider -->
                <div class="flex items-center gap-2 flex-1 justify-end">
                  <span class="font-bold text-neutral-400 text-xs">T</span>
                  <label
                    class="relative w-6 h-6 rounded-full border-2 border-white/80 shadow-md cursor-pointer block overflow-hidden shrink-0 hover:scale-105 transition-transform"
                    style="background-color: {selectedOutlineColor};"
                    title="Text Outline Color"
                  >
                    <input
                      type="color"
                      value={selectedOutlineColor}
                      oninput={(e) => setOutlineColor(e.currentTarget.value)}
                      class="opacity-0 absolute inset-0 w-full h-full cursor-pointer"
                    />
                  </label>

                  <!-- Station Slider (3 clean stations: None, Medium, Thick) -->
                  <div class="flex flex-col flex-1 max-w-[95px]">
                    <!-- Station Ticks -->
                    <div class="relative w-full h-2.5 flex items-end justify-between px-1 pointer-events-none mb-0.5">
                      <!-- Station 0: Red tick -->
                      <div class="w-0.5 h-2.5 bg-rose-500 rounded-full shadow-[0_0_3px_rgba(244,63,94,0.9)]" title="No Outline"></div>
                      <!-- Station 1: Medium (Center) -->
                      <div class="w-1.5 h-1.5 bg-neutral-400 rounded-xs" title="Medium (2.5px)"></div>
                      <!-- Station 2: Thick (Right) -->
                      <div class="w-1.5 h-1.5 bg-neutral-400 rounded-xs" title="Thick (6px)"></div>
                    </div>
                    <input
                      type="range"
                      min="0"
                      max={OUTLINE_STATIONS.length - 1}
                      step="1"
                      value={outlineStationIndex}
                      oninput={(e) => setOutlineStation(parseInt(e.currentTarget.value, 10))}
                      class="picasa-station-slider w-full cursor-pointer h-2"
                      title={outlineStationIndex === 0 ? 'No Outline' : outlineStationIndex === 1 ? 'Medium Outline (2.5px)' : 'Thick Outline (6px)'}
                    />
                  </div>
                </div>
              </div>

              <div class="h-px bg-[#323946] my-1"></div>

              <!-- Row 5: Transparency Slider -->
              <div class="space-y-1">
                <div class="flex items-center justify-center text-[11px] font-medium text-neutral-300">
                  <span>Transparency</span>
                </div>
                <div class="relative w-full flex flex-col items-center">
                  <div class="relative w-full h-1.5 flex justify-center pointer-events-none mb-0.5">
                    <div class="w-0.5 h-1.5 bg-neutral-400 rounded-full"></div>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    value={selectedTransparency}
                    oninput={(e) => setTransparency(parseInt(e.currentTarget.value, 10))}
                    class="picasa-station-slider w-full cursor-pointer h-2"
                    title="Transparency ({selectedTransparency}%)"
                  />
                </div>
              </div>

              <!-- Row 6: Copy Caption & Clear All -->
              <div class="grid grid-cols-2 gap-2 pt-1">
                <button
                  type="button"
                  onclick={handleCopyCaption}
                  class="px-2.5 py-1.5 rounded-md border text-xs font-medium transition-colors bg-[#2b313d] hover:bg-[#353d4c] border-[#3e4858] text-neutral-200 cursor-pointer shadow-xs flex items-center justify-center gap-1.5 active:scale-95"
                  title="Insert picture caption onto photo"
                >
                  <span>Copy Caption</span>
                </button>
                <button
                  type="button"
                  onclick={handleClearAllText}
                  disabled={textItems.length === 0}
                  class="px-2.5 py-1.5 rounded-md border text-xs font-medium transition-colors bg-[#2b313d] hover:bg-[#353d4c] border-[#3e4858] text-neutral-200 cursor-pointer shadow-xs flex items-center justify-center gap-1.5 disabled:opacity-40 disabled:cursor-not-allowed active:scale-95"
                  title="Clear all text items on this photo"
                >
                  <span>Clear All</span>
                </button>
              </div>

              <!-- Row 7: Apply & Cancel with Picasa checkmark & X circle icons -->
              <div class="grid grid-cols-2 gap-2 pt-2 border-t border-[#313947]">
                <button
                  type="button"
                  onclick={handleApplyText}
                  disabled={isApplyingText || textItems.length === 0}
                  class="px-3 py-2 rounded-md bg-gradient-to-b from-neutral-100 to-neutral-200 hover:from-white hover:to-neutral-100 active:from-neutral-200 active:to-neutral-300 text-neutral-900 font-semibold text-xs transition-all flex items-center justify-center gap-2 shadow-sm cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed active:scale-95"
                  title="Apply and bake text into photo"
                >
                  {#if isApplyingText}
                    <Loader2 class="w-4 h-4 animate-spin text-neutral-800" />
                  {:else}
                    <div class="w-4 h-4 rounded-full bg-emerald-600 text-white flex items-center justify-center shrink-0 shadow-xs">
                      <Check class="w-2.5 h-2.5 stroke-[3]" />
                    </div>
                  {/if}
                  <span>Apply</span>
                </button>

                <button
                  type="button"
                  onclick={handleCancelText}
                  class="px-3 py-2 rounded-md bg-gradient-to-b from-neutral-100 to-neutral-200 hover:from-white hover:to-neutral-100 active:from-neutral-200 active:to-neutral-300 text-neutral-900 font-semibold text-xs transition-all flex items-center justify-center gap-2 shadow-sm cursor-pointer active:scale-95"
                  title="Cancel and discard text edits"
                >
                  <div class="w-4 h-4 rounded-full bg-rose-600 text-white flex items-center justify-center shrink-0 shadow-xs">
                    <X class="w-2.5 h-2.5 stroke-[3]" />
                  </div>
                  <span>Cancel</span>
                </button>
              </div>
            </div>
          </div>

        {:else}
        
        <!-- ------------------------------------------------------------- -->
        <!-- TAB 1: BASIC FIXES (Picasa Signature 3-Column Layout)         -->
        <!-- ------------------------------------------------------------- -->
        {#if activeTab === 'basic'}
          <div class="space-y-3">
            <!-- 3-Column Grid of Signature Picasa Tool Buttons -->
            <div class="grid grid-cols-3 gap-1.5 pt-0.5">
              <!-- 1. Crop -->
              <button
                type="button"
                onclick={openCropTool}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Crop Photo"
              >
                <div class="w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={cropPicasaIcon}
                    srcset="{cropPicasaIcon2x} 2x"
                    alt="Crop"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-blue-500 transition-colors" style="color: var(--text-main);">
                  Crop
                </span>
              </button>

              <!-- 2. Straighten -->
              <button
                type="button"
                onclick={openStraightenTool}
                class={`flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center ${
                  straightenAngle !== 0 ? 'ring-1 ring-cyan-500 border-cyan-500' : ''
                }`}
                style="background-color: var(--card-bg); border-color: {straightenAngle !== 0 ? 'var(--accent-picasa)' : 'var(--card-border)'};"
                title="Straighten Photo"
              >
                <div class="w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={straightenPicasaIcon}
                    srcset="{straightenPicasaIcon2x} 2x"
                    alt="Straighten"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-cyan-500 transition-colors" style="color: var(--text-main);">
                  Straighten
                </span>
              </button>

              <!-- 3. Redeye -->
              <button
                type="button"
                onclick={openRedEyeTool}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Redeye Repair"
              >
                <div class="w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={redeyePicasaIcon}
                    srcset="{redeyePicasaIcon2x} 2x"
                    alt="Redeye"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-rose-500 transition-colors" style="color: var(--text-main);">
                  Redeye
                </span>
              </button>

              <!-- 4. I'm Feeling Lucky -->
              <button
                type="button"
                onclick={handleApplyFeelingLucky}
                disabled={isApplyingLucky || isEditing}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center disabled:opacity-40"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="I'm Feeling Lucky: One-click fix for lighting and color"
              >
                <div class="relative w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={luckyPicasaIcon}
                    srcset="{luckyPicasaIcon2x} 2x"
                    alt="I'm Feeling Lucky"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                  {#if isApplyingLucky}
                    <div class="absolute inset-0 bg-black/50 flex items-center justify-center">
                      <Loader2 class="w-3.5 h-3.5 text-amber-400 animate-spin" />
                    </div>
                  {/if}
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-amber-500 transition-colors" style="color: var(--text-main);">
                  I'm Feeling Lucky
                </span>
              </button>

              <!-- 5. Auto Contrast -->
              <button
                type="button"
                onclick={handleApplyAutoContrast}
                disabled={isApplyingAutoContrast || isEditing}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center disabled:opacity-40"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Auto Contrast: Optimize dynamic range and highlights"
              >
                <div class="relative w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={contrastPicasaIcon}
                    srcset="{contrastPicasaIcon2x} 2x"
                    alt="Auto Contrast"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                  {#if isApplyingAutoContrast}
                    <div class="absolute inset-0 bg-black/50 flex items-center justify-center">
                      <Loader2 class="w-3.5 h-3.5 text-blue-400 animate-spin" />
                    </div>
                  {/if}
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-blue-500 transition-colors" style="color: var(--text-main);">
                  Auto Contrast
                </span>
              </button>

              <!-- 6. Auto Color -->
              <button
                type="button"
                onclick={handleApplyAutoColor}
                disabled={isApplyingAutoColor || isEditing}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center disabled:opacity-40"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Auto Color: Neutralize ambient lighting and color cast"
              >
                <div class="relative w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={colorPicasaIcon}
                    srcset="{colorPicasaIcon2x} 2x"
                    alt="Auto Color"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                  {#if isApplyingAutoColor}
                    <div class="absolute inset-0 bg-black/50 flex items-center justify-center">
                      <Loader2 class="w-3.5 h-3.5 text-purple-400 animate-spin" />
                    </div>
                  {/if}
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-purple-500 transition-colors" style="color: var(--text-main);">
                  Auto Color
                </span>
              </button>

              <!-- 7. Retouch -->
              <button
                type="button"
                onclick={() => (activeTool = 'retouch')}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Retouch Blemish"
              >
                <div class="w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={retouchPicasaIcon}
                    srcset="{retouchPicasaIcon2x} 2x"
                    alt="Retouch"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-amber-500 transition-colors" style="color: var(--text-main);">
                  Retouch
                </span>
              </button>

              <!-- 8. Text -->
              <button
                type="button"
                onclick={activateTextTool}
                class="flex flex-col items-center justify-start p-2 rounded-lg border hover:shadow-xs transition-all cursor-pointer group active:scale-95 text-center"
                style="background-color: var(--card-bg); border-color: var(--card-border);"
                title="Text Caption Overlay"
              >
                <div class="w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs mb-1.5 flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={textPicasaIcon}
                    srcset="{textPicasaIcon2x} 2x"
                    alt="Text"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <span class="text-[11px] font-medium leading-tight group-hover:text-sky-500 transition-colors" style="color: var(--text-main);">
                  Text
                </span>
              </button>

              <!-- 9. Empty slot / grid balancing -->
              <div class="p-2"></div>
            </div>

            <!-- Row 4: Fill Light (Signature Picasa Slider) -->
            <div class="pt-3 border-t" style="border-color: var(--border-subtle);">
              <div class="flex items-center gap-3 p-2.5 rounded-lg border" style="background-color: var(--card-bg); border-color: var(--card-border);">
                <div class="shrink-0 w-11 h-[29px] rounded-[2px] overflow-hidden border border-neutral-300 dark:border-neutral-600/70 shadow-xs flex items-center justify-center bg-black/10 dark:bg-black/40">
                  <img
                    src={fillLightPicasaIcon}
                    srcset="{fillLightPicasaIcon2x} 2x"
                    alt="Fill Light"
                    class="w-full h-full object-cover select-none pointer-events-none"
                  />
                </div>
                <div class="flex-1 flex flex-col justify-center min-w-0">
                  <div class="flex items-center justify-between text-[11px] font-medium mb-1" style="color: var(--text-main);">
                    <span>Fill Light</span>
                    <span class="font-mono text-[10px] text-blue-500 font-semibold">{fillLight}%</span>
                  </div>
                  <input
                    type="range"
                    min="0"
                    max="100"
                    bind:value={fillLight}
                    oninput={() => {
                      isAdjustingFillLight = true;
                    }}
                    onpointerup={() => {
                      if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                    }}
                    onmouseup={() => {
                      if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                    }}
                    ontouchend={() => {
                      if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                    }}
                    onkeyup={(e) => {
                      if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
                        handleCommitFillLight(fillLight);
                      }
                    }}
                    onchange={() => {
                      handleCommitFillLight(fillLight);
                    }}
                    disabled={isApplyingFillLight}
                    class="w-full accent-blue-600 cursor-pointer h-1.5"
                    title="Fill Light: Boost shadow exposure"
                  />
                </div>
              </div>
            </div>

            <!-- Row 5: Picasa Undo & Redo Controls -->
            <div class="grid grid-cols-2 gap-2 pt-1">
              <button
                type="button"
                onclick={handleUndo}
                disabled={!sessionState?.can_undo || isEditing}
                class="flex items-center justify-center gap-1.5 py-2 px-2.5 rounded-md border font-medium text-xs shadow-xs transition-all cursor-pointer disabled:opacity-30 disabled:pointer-events-none active:scale-98"
                style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-main);"
                title={`${undoLabel} (Ctrl+Z)`}
              >
                <Undo2 class="w-3.5 h-3.5 shrink-0 opacity-70" />
                <span class="truncate">{undoLabel}</span>
              </button>

              <button
                type="button"
                onclick={handleRedo}
                disabled={!sessionState?.can_redo || isEditing}
                class="flex items-center justify-center gap-1.5 py-2 px-2.5 rounded-md border font-medium text-xs shadow-xs transition-all cursor-pointer disabled:opacity-30 disabled:pointer-events-none active:scale-98"
                style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-main);"
                title={`${redoLabel} (Ctrl+Y)`}
              >
                <Redo2 class="w-3.5 h-3.5 shrink-0 opacity-70" />
                <span class="truncate">{redoLabel}</span>
              </button>
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
                  const hadFill = fillLight > 0;
                  fillLight = 0;
                  highlights = 0;
                  shadows = 0;
                  colorTemp = 0;
                  if (hadFill) {
                    handleCommitFillLight(0);
                  }
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
                oninput={() => {
                  isAdjustingFillLight = true;
                }}
                onpointerup={() => {
                  if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                }}
                onmouseup={() => {
                  if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                }}
                ontouchend={() => {
                  if (isAdjustingFillLight) handleCommitFillLight(fillLight);
                }}
                onkeyup={(e) => {
                  if (['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End', 'PageUp', 'PageDown'].includes(e.key)) {
                    handleCommitFillLight(fillLight);
                  }
                }}
                onchange={() => {
                  handleCommitFillLight(fillLight);
                }}
                disabled={isApplyingFillLight}
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

            <!-- Tab 2 Dedicated Undo & Redo Controls -->
            <div class="grid grid-cols-2 gap-2 pt-1 border-t border-neutral-700/60">
              <button
                type="button"
                onclick={handleUndo}
                disabled={!sessionState?.can_undo || isEditing}
                class="flex items-center justify-center gap-1.5 py-2 px-2.5 rounded-md border font-medium text-xs shadow-xs transition-all cursor-pointer disabled:opacity-30 disabled:pointer-events-none active:scale-98"
                style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-main);"
                title={`${undoLabel} (Ctrl+Z)`}
              >
                <Undo2 class="w-3.5 h-3.5 shrink-0 opacity-70" />
                <span class="truncate">{undoLabel}</span>
              </button>

              <button
                type="button"
                onclick={handleRedo}
                disabled={!sessionState?.can_redo || isEditing}
                class="flex items-center justify-center gap-1.5 py-2 px-2.5 rounded-md border font-medium text-xs shadow-xs transition-all cursor-pointer disabled:opacity-30 disabled:pointer-events-none active:scale-98"
                style="background-color: var(--card-bg); border-color: var(--card-border); color: var(--text-main);"
                title={`${redoLabel} (Ctrl+Y)`}
              >
                <Redo2 class="w-3.5 h-3.5 shrink-0 opacity-70" />
                <span class="truncate">{redoLabel}</span>
              </button>
            </div>
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
        {#if activeTab !== 'basic'}
          <div class="grid grid-cols-2 gap-2">
            <!-- Undo Action -->
            <button
              onclick={handleUndo}
              disabled={!sessionState?.can_undo || isEditing}
              class="flex items-center justify-center gap-1.5 py-1.5 px-2 rounded-md bg-[#24242c] hover:bg-[#2c2c36] text-neutral-200 border border-neutral-700/60 disabled:opacity-30 disabled:pointer-events-none transition-colors text-[11px] font-medium cursor-pointer"
              title={`${undoLabel} (Ctrl+Z)`}
            >
              <Undo2 class="w-3.5 h-3.5 shrink-0" />
              <span class="truncate">{undoLabel}</span>
            </button>

            <!-- Redo Action -->
            <button
              onclick={handleRedo}
              disabled={!sessionState?.can_redo || isEditing}
              class="flex items-center justify-center gap-1.5 py-1.5 px-2 rounded-md bg-[#24242c] hover:bg-[#2c2c36] text-neutral-200 border border-neutral-700/60 disabled:opacity-30 disabled:pointer-events-none transition-colors text-[11px] font-medium cursor-pointer"
              title={`${redoLabel} (Ctrl+Y)`}
            >
              <Redo2 class="w-3.5 h-3.5 shrink-0" />
              <span class="truncate">{redoLabel}</span>
            </button>
          </div>
        {/if}

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

        <!-- Picasa Signature Bottom-Left Histogram & Camera Information -->
        <div class="pt-2">
          <Histogram
            photo={currentPhoto}
            metadata={photoMetadata}
            imageElement={mainImgRef}
            imageSrc={activeImageSrc}
          />
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
      <!-- Bottom-Left Floating Histogram (When Left Sidebar is Collapsed) -->
      {#if isSidebarCollapsed}
        <div class="absolute bottom-5 left-5 z-30 w-72 pointer-events-auto animate-in fade-in slide-in-from-bottom-2 duration-150">
          <Histogram
            photo={currentPhoto}
            metadata={photoMetadata}
            imageElement={mainImgRef}
            imageSrc={activeImageSrc}
          />
        </div>
      {/if}
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
          <!-- Debug Overlay Toggle (Dev Mode Only) -->
          {#if import.meta.env.DEV}
            <button
              onclick={() => ($showEditOverlay = !$showEditOverlay)}
              class={`px-2 py-1 rounded text-xs font-medium border transition-colors cursor-pointer flex items-center gap-1.5 ${
                $showEditOverlay
                  ? 'bg-amber-500/20 border-amber-500/50 text-amber-300'
                  : 'bg-[#24242c] border-neutral-700 text-neutral-400 hover:text-white'
              }`}
              title="Toggle Image Edit Debug Overlay"
            >
              <Bug class="w-3.5 h-3.5" />
              <span class="text-[11px]">Debug HUD</span>
            </button>
          {/if}

          <!-- Face Detection Toggle -->
          <button
            onclick={() => (showFaceBoxes = !showFaceBoxes)}
            class={`px-2 py-1 rounded text-xs font-medium border transition-colors cursor-pointer flex items-center gap-1.5 ${
              showFaceBoxes && faces.length > 0
                ? 'bg-purple-500/20 border-purple-500/50 text-purple-300'
                : 'bg-[#24242c] border-neutral-700 text-neutral-400 hover:text-white'
            }`}
            title={faces.length > 0 ? `Toggle Face Boxes (${faces.length} detected)` : 'No faces detected'}
          >
            <Users class="w-3.5 h-3.5" />
            <span class="text-[11px]">{faces.length > 0 ? `${faces.length} ${faces.length === 1 ? 'Face' : 'Faces'}` : 'Faces'}</span>
          </button>

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
        <div class="absolute bottom-8 inset-x-0 z-40 flex justify-center pointer-events-none animate-in fade-in slide-in-from-bottom-2 duration-150">
          <div class="px-4 py-1.5 rounded-full bg-neutral-900/95 text-white text-xs font-medium border border-neutral-700 shadow-2xl backdrop-blur-md">
            {toastMsg}
          </div>
        </div>
      {/if}

      <!-- Image Edit Debug Overlay HUD -->
      {#if $showEditOverlay}
        <aside
          class="absolute top-14 right-4 z-40 w-96 max-h-[calc(100vh-120px)] flex flex-col bg-[#16161d]/95 border border-amber-500/40 rounded-xl shadow-2xl backdrop-blur-md overflow-hidden text-xs text-neutral-200 animate-in fade-in zoom-in-95 duration-150 select-text"
        >
          <!-- HUD Header -->
          <div class="px-3.5 py-2.5 bg-amber-950/40 border-b border-amber-500/30 flex items-center justify-between shrink-0">
            <div class="flex items-center gap-2">
              <Bug class="w-4 h-4 text-amber-400" />
              <span class="font-bold text-amber-300 tracking-wide text-xs">Image Edit Session HUD</span>
            </div>
            <div class="flex items-center gap-2">
              {#if sessionState?.has_session}
                <span class={`text-[10px] px-2 py-0.5 rounded font-mono font-semibold ${
                  sessionState.is_committed
                    ? 'bg-emerald-500/20 text-emerald-300 border border-emerald-500/30'
                    : 'bg-amber-500/20 text-amber-300 border border-amber-500/30'
                }`}>
                  {sessionState.is_committed ? 'COMMITTED' : 'DRAFT'}
                </span>
              {/if}
              <button
                onclick={() => ($showEditOverlay = false)}
                class="p-1 rounded text-neutral-400 hover:text-white hover:bg-white/10 cursor-pointer"
                title="Close Debug Overlay"
              >
                <X class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <!-- HUD Scrollable Content -->
          <div class="flex-1 overflow-y-auto p-3.5 space-y-3 font-sans">
            <!-- Session Location & Disk Target -->
            <div class="space-y-1.5 p-2.5 rounded-lg bg-black/40 border border-neutral-800 text-[11px]">
              <div class="text-[10px] uppercase font-bold tracking-wider text-amber-400 flex items-center gap-1.5">
                <HardDrive class="w-3 h-3" />
                <span>Session Manifest & Paths</span>
              </div>
              <div class="space-y-1 text-neutral-300 font-mono text-[10px] break-all">
                <div><span class="text-neutral-500">Manifest: </span><span class="text-sky-300">{sessionState?.session_manifest_path || 'None'}</span></div>
                <div><span class="text-neutral-500">Session Dir: </span><span class="text-neutral-400">{sessionState?.session_dir || 'None'}</span></div>
                <div><span class="text-neutral-500">Target File: </span><span class="text-neutral-400">{currentPhoto.file_path}</span></div>
                <div><span class="text-neutral-500">Backup (.pica_originals): </span><span class={sessionState?.has_backup ? 'text-emerald-400' : 'text-neutral-500'}>{sessionState?.backup_path || 'No backup on disk'}</span></div>
                <div class="flex items-center justify-between pt-1 text-[11px] font-sans">
                  <span>Current Step: <strong class="text-amber-300 font-mono">{sessionState?.current_step ?? 0}</strong> / {sessionState?.total_steps ?? 0}</span>
                  <span>Rotation: <strong class="text-blue-300 font-mono">{sessionState?.cumulative_rotation ?? 0}°</strong></span>
                </div>
              </div>
            </div>

            <!-- Intermediate Steps & Render Status (The core visual chain) -->
            <div class="space-y-2">
              <div class="flex items-center justify-between text-[11px]">
                <span class="text-[10px] uppercase font-bold tracking-wider text-amber-400 flex items-center gap-1.5">
                  <Layers class="w-3 h-3" />
                  <span>Intermediate Steps & Renders ({sessionState?.steps?.length ?? 0})</span>
                </span>
                <span class="text-[10px] text-neutral-400">
                  {sessionState?.steps?.length ? 'Fast Proxy Mode' : 'No steps'}
                </span>
              </div>

              {#if sessionState?.steps && sessionState.steps.length > 0}
                <div class="space-y-2">
                  {#each sessionState.steps as step, idx}
                    {@const isCurrent = idx === sessionState.current_step}
                    {@const stepFullPath = sessionState.session_dir && step.render_file ? `${sessionState.session_dir}/${step.render_file}` : null}
                    <button
                      type="button"
                      onclick={() => handleJumpToStep(step.step_id)}
                      disabled={isEditing}
                      class={`w-full text-left p-2.5 rounded-lg border transition-all cursor-pointer ${
                        isCurrent
                          ? 'bg-blue-950/40 border-blue-500/70 shadow-lg shadow-blue-950/50 ring-1 ring-blue-500/50'
                          : 'bg-black/30 border-neutral-800/80 hover:border-neutral-600 hover:bg-neutral-900/60'
                      } ${isEditing ? 'opacity-70 cursor-not-allowed' : ''}`}
                    >
                      <div class="flex items-start gap-2.5">
                        <!-- Mini Preview Thumb of this intermediate render -->
                        {#if stepFullPath}
                          <div class="relative w-12 h-12 rounded overflow-hidden bg-black/60 shrink-0 border border-neutral-700">
                            <img
                              src={getPreviewImageUrl(stepFullPath, cacheBuster)}
                              alt={`Step ${step.step_id}`}
                              class="w-full h-full object-cover"
                            />
                            {#if isCurrent}
                              <div class="absolute inset-0 border-2 border-blue-400 pointer-events-none"></div>
                            {/if}
                          </div>
                        {/if}

                        <!-- Step Details -->
                        <div class="flex-1 min-w-0 space-y-1">
                          <div class="flex items-center justify-between">
                            <div class="flex items-center gap-1.5">
                              <span class={`text-[10px] px-1.5 py-0.2 rounded font-bold font-mono ${
                                isCurrent ? 'bg-blue-500 text-white' : 'bg-neutral-800 text-neutral-300'
                              }`}>
                                #{step.step_id}
                              </span>
                              <span class="font-semibold text-neutral-200 capitalize">
                                {step.op === 'rotate' ? `Rotate (${step.params.degrees > 0 ? '+' : ''}${step.params.degrees}°)` : step.op}
                              </span>
                            </div>

                            {#if isCurrent}
                              <span class="text-[9px] px-1.5 py-0.5 rounded bg-blue-500/30 text-blue-300 font-bold tracking-wider">
                                ACTIVE RENDER
                              </span>
                            {/if}
                          </div>

                          <div class="text-[10px] text-neutral-400 font-mono truncate">
                            File: <span class="text-emerald-400">{step.render_file || 'None'}</span>
                          </div>

                          {#if stepFullPath}
                            <div class="text-[9px] text-neutral-500 font-mono break-all leading-tight">
                              {stepFullPath}
                            </div>
                          {/if}

                          <div class="flex items-center justify-between text-[10px] text-neutral-400 pt-0.5">
                            <span>Cum. Rot: <strong class="text-neutral-200">{step.cumulative_rotation}°</strong></span>
                            <span>{new Date(step.timestamp * 1000).toLocaleTimeString()}</span>
                          </div>
                        </div>
                      </div>
                    </button>
                  {/each}
                </div>
              {:else}
                <div class="p-4 rounded-lg bg-black/30 border border-neutral-800/80 text-center text-neutral-400 text-xs">
                  No editing steps recorded yet. Apply a rotation or edit tool to create the session proxy.
                </div>
              {/if}
            </div>

            <!-- Raw Session JSON Collapsible -->
            {#if sessionState?.has_session}
              <details class="p-2.5 rounded-lg bg-black/40 border border-neutral-800 text-[11px] group">
                <summary class="font-bold text-amber-400 cursor-pointer flex items-center justify-between select-none">
                  <span class="flex items-center gap-1.5 uppercase text-[10px]">
                    <FileCode2 class="w-3 h-3" />
                    Raw session.json Content
                  </span>
                  <span class="text-[10px] text-neutral-500 group-open:rotate-180 transition-transform">▼</span>
                </summary>
                <pre class="mt-2 p-2 rounded bg-black/80 text-[10px] font-mono text-emerald-300 overflow-x-auto max-h-48 border border-neutral-800">
{JSON.stringify(sessionState, null, 2)}
                </pre>
              </details>
            {/if}
          </div>
        </aside>
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
            role="presentation"
            class="w-full h-full flex items-center justify-center p-6 {zoomLevel > 1.05 ? (isPanning ? 'cursor-grabbing' : 'cursor-grab') : ''}"
            onwheel={handleWheel}
            onpointerdown={handleViewerPointerDown}
            onclick={() => {
              if (isCropPreviewMode) cancelCropPreview();
            }}
          >
            <!-- Indeterminate Progress Shimmer for Image Loading & Rendering -->
            {#if isMainImageLoading || isEditing}
              <div class="absolute top-0 inset-x-0 h-0.5 bg-neutral-900/60 overflow-hidden z-40 pointer-events-none">
                <div class="h-full bg-gradient-to-r from-blue-500 via-emerald-400 to-cyan-400 w-1/3 animate-[progress_1.1s_ease-in-out_infinite]"></div>
              </div>
            {/if}

            <!-- Floating Preview Cutout Badge -->
            {#if isCropPreviewMode}
              <button
                type="button"
                onclick={cancelCropPreview}
                class="absolute top-4 left-1/2 -translate-x-1/2 z-50 bg-black/85 hover:bg-black text-white text-xs px-4 py-2 rounded-full border border-white/25 shadow-2xl flex items-center gap-2 animate-in fade-in zoom-in-95 cursor-pointer transition-all active:scale-95"
                title="Click or press Esc to return"
              >
                <Eye class="w-3.5 h-3.5 text-emerald-400 animate-pulse" />
                <span class="font-medium">Crop Preview</span>
                <span class="text-neutral-400 text-[10px]">• Click or Esc to return</span>
              </button>
            {/if}

            <div
              class="relative flex items-center justify-center select-none"
              style="
                transform: rotate({effectiveCanvasRotation}deg) translate3d({panX}px, {panY}px, 0) scale({zoomLevel > 1.001 ? fitScale * zoomLevel : (isZoomed100 ? 1.5 : fitScale)});
                transform-origin: center center;
                will-change: transform;
                transition: {isTransitioning ? 'transform 380ms cubic-bezier(0.2, 0.9, 0.3, 1)' : (isPanning || isWheelZooming ? 'none' : 'transform 120ms ease-out')};
              "
            >
              <div
                class="relative inline-block select-none {activeTool === 'straighten' ? 'overflow-hidden rounded-sm' : ''}"
                bind:this={imageContainerRef}
              >
                <!-- Animated Cutout Preview Container (Picasa style zoom to full size) -->
                <div
                  class="relative inline-block select-none"
                  style="
                    {isCropPreviewMode
                      ? `transform: scale(${previewScale}) translate(${previewTx}%, ${previewTy}%); clip-path: inset(${previewClipTop}% ${previewClipRight}% ${previewClipBottom}% ${previewClipLeft}%);`
                      : 'transform: scale(1) translate(0%, 0%); clip-path: inset(0% 0% 0% 0%);'}
                    transition: transform 420ms cubic-bezier(0.16, 1, 0.3, 1), clip-path 420ms cubic-bezier(0.16, 1, 0.3, 1);
                    will-change: transform, clip-path;
                  "
                >
                  <button
                    type="button"
                    class="contents border-none bg-transparent p-0 m-0"
                    onclick={() => {
                      if (isCropPreviewMode) {
                        cancelCropPreview();
                        return;
                      }
                      if (activeTool === 'crop' || activeTool === 'straighten' || activeTool === 'redeye' || activeTool === 'retouch' || activeTool === 'text') return;
                      toggleZoom();
                    }}
                    aria-label="Toggle 100% zoom"
                  >
                    <!-- Blurred low-res thumbnail underlay to prevent blank screen during high-res fetch -->
                    {#if isMainImageLoading && previewThumbSrc}
                      <img
                        src={previewThumbSrc}
                        alt="Thumbnail placeholder"
                        class="absolute inset-0 w-full h-full object-contain filter blur-md opacity-45 scale-100 pointer-events-none transition-opacity duration-300"
                      />
                    {/if}

                    <img
                      bind:this={mainImgRef}
                      crossorigin="anonymous"
                      decoding="async"
                      src={activeImageSrc}
                      alt={currentPhoto.file_name}
                      onload={(e) => handleImageLoaded(e.currentTarget as HTMLImageElement)}
                      class={`max-h-[calc(100vh-140px)] max-w-[calc(100vw-360px)] rounded-sm shadow-2xl object-contain block ${
                        isMainImageLoading ? 'opacity-80' : 'opacity-100'
                      } ${
                        activeTool === 'crop'
                          ? isCropPreviewMode ? 'cursor-pointer' : 'cursor-crosshair'
                          : activeTool === 'straighten'
                          ? 'cursor-default'
                          : activeTool === 'redeye' || activeTool === 'retouch'
                          ? 'cursor-crosshair'
                          : activeTool === 'text'
                          ? 'cursor-text'
                          : zoomLevel > 1.05
                          ? (isPanning ? 'cursor-grabbing' : 'cursor-grab')
                          : 'cursor-zoom-in'
                      }`}
                      style="
                        filter: {liveFilterStyle};
                        {activeTool === 'straighten'
                          ? `transform: rotate(${straightenAngle}deg) scale(${straightenZoom}); transform-origin: center center; will-change: transform; transition: none;`
                          : 'transition: opacity 150ms ease-out, filter 150ms ease-out;'
                        }
                      "
                    />
                  </button>

                  <!-- Real-time Red Eye Repair Canvas (Overlays modified pixels on active image) -->
                  <canvas
                    bind:this={redEyeCanvasRef}
                    class="absolute inset-0 w-full h-full pointer-events-none z-15"
                    class:hidden={activeTool !== 'redeye'}
                  ></canvas>

                  <!-- Real-time Retouch SpotBlend Canvas (Overlays modified pixels on active image) -->
                  <canvas
                    bind:this={retouchCanvasRef}
                    class="absolute inset-0 w-full h-full pointer-events-none z-15"
                    class:hidden={activeTool !== 'retouch'}
                  ></canvas>

                  <!-- Retouch Interactive Overlay (Picasa SpotBlend Heritage) -->
                  {#if activeTool === 'retouch'}
                    <div
                      role="presentation"
                      class="absolute inset-0 z-25 select-none touch-none cursor-crosshair"
                      onpointerdown={handleRetouchPointerDown}
                      onpointermove={handleRetouchPointerMove}
                    >
                      <!-- Render Committed Spots (Only when enabled in sidebar) -->
                      {#if showRetouchCircles}
                        {#each retouchSpots as spot (spot.id)}
                          {@const minDim = Math.min(renderedNaturalWidth || 1000, renderedNaturalHeight || 1000)}
                          {@const rPx = spot.radius * minDim}
                          {@const rPctX = (rPx / (renderedNaturalWidth || 1000)) * 100}
                          {@const rPctY = (rPx / (renderedNaturalHeight || 1000)) * 100}

                          <!-- SVG Tether Line between Target and Source -->
                          <svg class="absolute inset-0 w-full h-full pointer-events-none z-28 overflow-visible">
                            <line
                              x1="{spot.target_x * 100}%"
                              y1="{spot.target_y * 100}%"
                              x2="{spot.source_x * 100}%"
                              y2="{spot.source_y * 100}%"
                              stroke="#f59e0b"
                              stroke-width="1.5"
                              stroke-dasharray="3 3"
                              opacity="0.8"
                            />
                          </svg>

                          <!-- Target Circle (Healed Location) -->
                          <button
                            type="button"
                            aria-label="Remove retouch spot"
                            class="absolute rounded-full border-2 border-amber-400 bg-amber-400/10 hover:bg-rose-500/20 hover:border-rose-400 cursor-pointer transition-colors group z-30 pointer-events-auto p-0 m-0 outline-none -translate-x-1/2 -translate-y-1/2"
                            style="
                              left: {spot.target_x * 100}%;
                              top: {spot.target_y * 100}%;
                              width: {rPctX * 2}%;
                              height: {rPctY * 2}%;
                            "
                            onclick={(e) => {
                              e.stopPropagation();
                              handleDeleteRetouchSpot(spot.id);
                            }}
                            onpointerdown={(e) => e.stopPropagation()}
                            title="Click to remove retouch spot"
                          >
                            <div class="absolute -top-2 -right-2 w-4 h-4 rounded-full bg-neutral-900 border border-amber-400 text-amber-400 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity shadow pointer-events-none">
                              <X class="w-2.5 h-2.5 stroke-[3]" />
                            </div>
                          </button>

                          <!-- Source Donor Circle -->
                          <div
                            class="absolute rounded-full border border-dashed border-amber-300/80 bg-transparent pointer-events-none z-29 -translate-x-1/2 -translate-y-1/2"
                            style="
                              left: {spot.source_x * 100}%;
                              top: {spot.source_y * 100}%;
                              width: {rPctX * 2}%;
                              height: {rPctY * 2}%;
                            "
                          ></div>
                        {/each}
                      {/if}

                      <!-- Pending Spot Interaction (Phase 2: Target placed, choosing source) -->
                      {#if pendingRetouchTarget}
                        {@const minDim = Math.min(renderedNaturalWidth || 1000, renderedNaturalHeight || 1000)}
                        {@const rPx = retouchRadius}
                        {@const rPctX = (rPx / (renderedNaturalWidth || 1000)) * 100}
                        {@const rPctY = (rPx / (renderedNaturalHeight || 1000)) * 100}

                        <!-- Target Circle (Fixed at Step 1 click) -->
                        <div
                          class="absolute rounded-full border-2 border-amber-400 bg-amber-400/20 pointer-events-none z-30 -translate-x-1/2 -translate-y-1/2 shadow-lg"
                          style="
                            left: {pendingRetouchTarget.x * 100}%;
                            top: {pendingRetouchTarget.y * 100}%;
                            width: {rPctX * 2}%;
                            height: {rPctY * 2}%;
                          "
                        ></div>

                        {#if retouchCursorPos}
                          <!-- Tether Line to Current Cursor -->
                          <svg class="absolute inset-0 w-full h-full pointer-events-none z-28 overflow-visible">
                            <line
                              x1="{pendingRetouchTarget.x * 100}%"
                              y1="{pendingRetouchTarget.y * 100}%"
                              x2="{retouchCursorPos.x * 100}%"
                              y2="{retouchCursorPos.y * 100}%"
                              stroke="#f59e0b"
                              stroke-width="2"
                              stroke-dasharray="4 4"
                            />
                          </svg>

                          <!-- Donor Source Circle (Following Cursor) -->
                          <div
                            class="absolute rounded-full border-2 border-dashed border-amber-300 bg-amber-500/10 pointer-events-none z-30 -translate-x-1/2 -translate-y-1/2 shadow-lg"
                            style="
                              left: {retouchCursorPos.x * 100}%;
                              top: {retouchCursorPos.y * 100}%;
                              width: {rPctX * 2}%;
                              height: {rPctY * 2}%;
                            "
                          ></div>
                        {/if}
                      {:else if retouchCursorPos}
                        <!-- Phase 1: Brush Cursor Circle following mouse before first click -->
                        {@const minDim = Math.min(renderedNaturalWidth || 1000, renderedNaturalHeight || 1000)}
                        {@const rPx = retouchRadius}
                        {@const rPctX = (rPx / (renderedNaturalWidth || 1000)) * 100}
                        {@const rPctY = (rPx / (renderedNaturalHeight || 1000)) * 100}
                        <div
                          class="absolute rounded-full border border-amber-400/80 bg-amber-400/10 pointer-events-none z-30 -translate-x-1/2 -translate-y-1/2 shadow-xs"
                          style="
                            left: {retouchCursorPos.x * 100}%;
                            top: {retouchCursorPos.y * 100}%;
                            width: {rPctX * 2}%;
                            height: {rPctY * 2}%;
                          "
                        ></div>
                      {/if}
                    </div>
                  {/if}

                  <!-- Detected Face Bounding Boxes Overlay -->
                  {#if (showFaceBoxes || $showFaceDebugOverlay) && faces.length > 0 && activeTool === null && !isCropPreviewMode}
                    <div class="absolute inset-0 pointer-events-none z-20">
                      {#each faces as face (face.id)}
                        <div
                          class="absolute border-2 border-purple-400 bg-purple-500/15 shadow-[0_0_0_1px_rgba(0,0,0,0.6)] rounded-xs pointer-events-auto group cursor-default"
                          style="
                            left: {face.box_x * 100}%;
                            top: {face.box_y * 100}%;
                            width: {face.box_w * 100}%;
                            height: {face.box_h * 100}%;
                          "
                        >
                          <div
                            class="absolute -top-7 left-1/2 -translate-x-1/2 px-2 py-0.5 rounded-full bg-black/85 backdrop-blur-sm text-[11px] font-semibold text-purple-200 whitespace-nowrap shadow-md border border-purple-400/50 group-hover:bg-purple-600 group-hover:border-purple-300 group-hover:text-white transition-all pointer-events-none select-none flex items-center gap-1.5"
                          >
                            <span>{face.person_name || 'Person'}</span>
                            <span class="text-[10px] text-purple-300 font-mono">({Math.round(face.confidence * 100)}%)</span>
                          </div>
                        </div>
                      {/each}
                    </div>
                  {/if}

                  <!-- Processing / Rendering Spinner Feedback -->
                  {#if isEditing}
                    <div class="absolute inset-0 bg-black/25 backdrop-blur-[1px] flex items-center justify-center rounded-sm pointer-events-none transition-opacity duration-200 z-30">
                      <div class="flex items-center gap-2 px-3 py-1.5 rounded-full bg-black/85 backdrop-blur-md border border-white/20 text-white text-xs font-medium shadow-2xl animate-pulse">
                        <Loader2 class="w-3.5 h-3.5 animate-spin text-emerald-400" />
                        <span class="tracking-wide">Rendering edit...</span>
                      </div>
                    </div>
                  {/if}

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
                </div>

                <!-- Picasa Straighten Leveling Grid Overlay (Stationary unrotated frame overlay) -->
                {#if activeTool === 'straighten'}
                  <div class="absolute inset-0 pointer-events-none z-20 border border-white/40 shadow-inner overflow-hidden">
                    <svg class="w-full h-full" xmlns="http://www.w3.org/2000/svg">
                      <defs>
                        <pattern id="picasa-straighten-grid" width="48" height="48" patternUnits="userSpaceOnUse">
                          <path d="M 48 0 L 0 0 0 48" fill="none" stroke="rgba(255, 255, 255, 0.45)" stroke-width="0.75" stroke-dasharray="3 3" />
                        </pattern>
                      </defs>
                      <rect width="100%" height="100%" fill="url(#picasa-straighten-grid)" />
                    </svg>
                  </div>
                {/if}

                <!-- Interactive Picasa-Style Crop Box & Darkened Outer Shroud -->
                {#if activeTool === 'crop'}
                  <div
                    role="presentation"
                    class="absolute inset-0 select-none touch-none overflow-hidden"
                    onpointerdown={(e) => {
                      if (isCropPreviewMode) {
                        cancelCropPreview();
                        return;
                      }
                      handleCanvasPointerDown(e);
                    }}
                  >
                    {#if !isCropPreviewMode}
                      <!-- 4-Piece Outer Dimming Shroud with Smooth Transitions on Preset Selection -->
                      <!-- Top -->
                      <div
                        class="absolute top-0 left-0 right-0 bg-black/70 backdrop-blur-[0.5px] pointer-events-none"
                        style="height: {cropBox.y * 100}%; transition: {dragHandle ? 'none' : 'height 280ms cubic-bezier(0.16, 1, 0.3, 1)'};"
                      ></div>
                      <!-- Bottom -->
                      <div
                        class="absolute bottom-0 left-0 right-0 bg-black/70 backdrop-blur-[0.5px] pointer-events-none"
                        style="top: {(cropBox.y + cropBox.height) * 100}%; transition: {dragHandle ? 'none' : 'top 280ms cubic-bezier(0.16, 1, 0.3, 1)'};"
                      ></div>
                      <!-- Left -->
                      <div
                        class="absolute left-0 bg-black/70 backdrop-blur-[0.5px] pointer-events-none"
                        style="top: {cropBox.y * 100}%; height: {cropBox.height * 100}%; width: {cropBox.x * 100}%; transition: {dragHandle ? 'none' : 'all 280ms cubic-bezier(0.16, 1, 0.3, 1)'};"
                      ></div>
                      <!-- Right -->
                      <div
                        class="absolute right-0 bg-black/70 backdrop-blur-[0.5px] pointer-events-none"
                        style="top: {cropBox.y * 100}%; height: {cropBox.height * 100}%; left: {(cropBox.x + cropBox.width) * 100}%; transition: {dragHandle ? 'none' : 'all 280ms cubic-bezier(0.16, 1, 0.3, 1)'};"
                      ></div>

                      <!-- Draggable / Resizable Crop Box with L-Bracket Handles & Live Ratio Badge -->
                      <div
                        class={`absolute select-none touch-none border border-white/95 shadow-[0_0_0_1px_rgba(0,0,0,0.85),0_4px_24px_rgba(0,0,0,0.6)] ${
                          dragHandle === 'move'
                            ? 'cursor-grabbing ring-2 ring-emerald-400/80 shadow-[0_0_24px_rgba(52,211,153,0.35)]'
                            : 'cursor-grab hover:border-white'
                        }`}
                        style="
                          left: {cropBox.x * 100}%;
                          top: {cropBox.y * 100}%;
                          width: {cropBox.width * 100}%;
                          height: {cropBox.height * 100}%;
                          transition: {dragHandle ? 'none' : 'left 280ms cubic-bezier(0.16, 1, 0.3, 1), top 280ms cubic-bezier(0.16, 1, 0.3, 1), width 280ms cubic-bezier(0.16, 1, 0.3, 1), height 280ms cubic-bezier(0.16, 1, 0.3, 1)'};
                        "
                        onpointerdown={(e) => startDrag(e, 'move')}
                        onpointerenter={() => (isCropHovered = true)}
                        onpointerleave={() => (isCropHovered = false)}
                        role="region"
                        aria-label="Crop selection box"
                      >
                        <!-- Floating Real-Time Aspect & Dimension Badge -->
                        <div
                          class={`absolute -top-7 left-1/2 -translate-x-1/2 px-2.5 py-0.5 rounded-full bg-neutral-950/90 backdrop-blur-md border border-white/25 text-white font-mono text-[10px] font-medium shadow-xl pointer-events-none select-none flex items-center gap-1.5 whitespace-nowrap transition-all duration-200 ${
                            dragHandle !== null || isCropHovered ? 'opacity-100 scale-100 translate-y-0' : 'opacity-0 scale-95 translate-y-1'
                          }`}
                        >
                          <span class="text-emerald-400 font-semibold">{cropAspect === 'free' ? 'Free' : cropAspect.toUpperCase()}</span>
                          <span class="text-white/40">•</span>
                          <span>
                            {Math.round((renderedNaturalWidth || (currentPhoto?.width || 1000)) * cropBox.width)} × {Math.round((renderedNaturalHeight || (currentPhoto?.height || 1000)) * cropBox.height)} px
                          </span>
                        </div>

                        <!-- 3x3 Rule-of-Thirds Grid Lines -->
                        <div class={`absolute inset-0 grid grid-cols-3 grid-rows-3 pointer-events-none transition-opacity duration-200 ${
                          dragHandle !== null ? 'opacity-85' : isCropHovered ? 'opacity-60' : 'opacity-30'
                        }`}>
                          {#each Array(9) as _}
                            <div class="border border-white/40 border-dashed"></div>
                          {/each}
                        </div>

                        <!-- 4 Corner L-Bracket Handles -->
                        <!-- NW -->
                        <div
                          class="absolute -top-1.5 -left-1.5 w-7 h-7 cursor-nwse-resize z-30 flex items-start justify-start group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'nw')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop top-left"
                        >
                          <div class="relative w-4 h-4 pointer-events-none">
                            <div class="absolute top-0 left-0 w-4 h-1 bg-white rounded-l-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-t border-l border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                            <div class="absolute top-0 left-0 w-1 h-4 bg-white rounded-t-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-l border-b border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                          </div>
                        </div>

                        <!-- NE -->
                        <div
                          class="absolute -top-1.5 -right-1.5 w-7 h-7 cursor-nesw-resize z-30 flex items-start justify-end group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'ne')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop top-right"
                        >
                          <div class="relative w-4 h-4 pointer-events-none">
                            <div class="absolute top-0 right-0 w-4 h-1 bg-white rounded-r-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-t border-r border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                            <div class="absolute top-0 right-0 w-1 h-4 bg-white rounded-t-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-r border-b border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                          </div>
                        </div>

                        <!-- SW -->
                        <div
                          class="absolute -bottom-1.5 -left-1.5 w-7 h-7 cursor-nesw-resize z-30 flex items-end justify-start group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'sw')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop bottom-left"
                        >
                          <div class="relative w-4 h-4 pointer-events-none">
                            <div class="absolute bottom-0 left-0 w-4 h-1 bg-white rounded-l-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-b border-l border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                            <div class="absolute bottom-0 left-0 w-1 h-4 bg-white rounded-b-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-l border-t border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                          </div>
                        </div>

                        <!-- SE -->
                        <div
                          class="absolute -bottom-1.5 -right-1.5 w-7 h-7 cursor-nwse-resize z-30 flex items-end justify-end group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'se')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop bottom-right"
                        >
                          <div class="relative w-4 h-4 pointer-events-none">
                            <div class="absolute bottom-0 right-0 w-4 h-1 bg-white rounded-r-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-b border-r border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                            <div class="absolute bottom-0 right-0 w-1 h-4 bg-white rounded-b-xs shadow-[0_1px_3px_rgba(0,0,0,0.9)] border-r border-t border-neutral-900 group-hover/h:bg-emerald-300 transition-colors"></div>
                          </div>
                        </div>

                        <!-- 4 Edge Center Grab Pill Handles -->
                        <!-- N -->
                        <div
                          class="absolute -top-2.5 left-1/2 -translate-x-1/2 w-10 h-5 cursor-ns-resize z-30 flex items-center justify-center group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'n')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.height * 100)}
                          aria-label="Resize crop top edge"
                        >
                          <div class="w-6 h-1.5 bg-white rounded-full shadow-[0_1px_3px_rgba(0,0,0,0.9)] border border-neutral-900 group-hover/h:bg-emerald-300 group-hover/h:scale-110 transition-all pointer-events-none"></div>
                        </div>

                        <!-- S -->
                        <div
                          class="absolute -bottom-2.5 left-1/2 -translate-x-1/2 w-10 h-5 cursor-ns-resize z-30 flex items-center justify-center group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 's')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.height * 100)}
                          aria-label="Resize crop bottom edge"
                        >
                          <div class="w-6 h-1.5 bg-white rounded-full shadow-[0_1px_3px_rgba(0,0,0,0.9)] border border-neutral-900 group-hover/h:bg-emerald-300 group-hover/h:scale-110 transition-all pointer-events-none"></div>
                        </div>

                        <!-- W -->
                        <div
                          class="absolute top-1/2 -translate-y-1/2 -left-2.5 w-5 h-10 cursor-ew-resize z-30 flex items-center justify-center group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'w')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop left edge"
                        >
                          <div class="w-1.5 h-6 bg-white rounded-full shadow-[0_1px_3px_rgba(0,0,0,0.9)] border border-neutral-900 group-hover/h:bg-emerald-300 group-hover/h:scale-110 transition-all pointer-events-none"></div>
                        </div>

                        <!-- E -->
                        <div
                          class="absolute top-1/2 -translate-y-1/2 -right-2.5 w-5 h-10 cursor-ew-resize z-30 flex items-center justify-center group/h touch-none"
                          onpointerdown={(e) => startDrag(e, 'e')}
                          role="slider"
                          tabindex="0"
                          aria-valuemin="0"
                          aria-valuemax="100"
                          aria-valuenow={Math.round(cropBox.width * 100)}
                          aria-label="Resize crop right edge"
                        >
                          <div class="w-1.5 h-6 bg-white rounded-full shadow-[0_1px_3px_rgba(0,0,0,0.9)] border border-neutral-900 group-hover/h:bg-emerald-300 group-hover/h:scale-110 transition-all pointer-events-none"></div>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}

                <!-- ========================================================= -->
                <!-- Interactive Picasa Text Overlay & Dial                    -->
                <!-- ========================================================= -->
                {#if activeTool === 'text' || textItems.length > 0}
                  <div
                    role="presentation"
                    class="absolute inset-0 z-35 {activeTool === 'text' ? 'cursor-text select-none' : 'pointer-events-none'}"
                    onpointerdown={handleCanvasTextPointerDown}
                  >
                    {#each textItems as item (item.id)}
                      {@const isActive = activeTool === 'text' && item.id === activeTextId}
                      
                      <div
                        role="presentation"
                        class="absolute picasa-text-box touch-none flex items-center justify-center {isActive ? 'z-40' : 'z-30 cursor-pointer pointer-events-auto'}"
                        style="
                          left: {item.x * 100}%;
                          top: {item.y * 100}%;
                          transform: translate(-50%, -50%) rotate({item.angle}deg) scale({item.scale});
                          transform-origin: center center;
                        "
                        onpointerdown={(e) => {
                          if (activeTool !== 'text') return;
                          startTextMoveDrag(item, e);
                        }}
                      >
                        <!-- Text Bounding Box (Semi-transparent rounded square matching Picasa Screenshot 2 & 3) -->
                        <div
                          class="relative transition-all duration-150 {
                            isActive
                              ? 'border-2 border-white/70 rounded-[20px] shadow-[0_0_0_1px_rgba(0,0,0,0.6),0_8px_32px_rgba(0,0,0,0.45)] px-6 py-3.5 backdrop-blur-[2px] cursor-move'
                              : 'p-1'
                          }"
                          style="background-color: {isActive ? 'rgba(16, 22, 30, 0.55)' : 'transparent'};"
                        >
                          <!-- Editable / Rendered Text Content -->
                          {#if isActive}
                            <div
                              contenteditable="plaintext-only"
                              spellcheck="false"
                              role="textbox"
                              tabindex="0"
                              dir="ltr"
                              use:editableText={item.text}
                              class="picasa-editable-text outline-none bg-transparent whitespace-pre border-none p-0 m-0 block select-text cursor-text"
                              style="
                                --text-color: {item.hasFill ? item.color : 'transparent'};
                                --text-fill: {item.hasFill ? item.color : 'transparent'};
                                --text-stroke: {item.hasOutline && item.outlineWidth > 0 ? `${item.outlineWidth}px ${item.outlineColor}` : '0px transparent'};
                                --text-size: {item.fontSize}px;
                                --text-font: {item.fontFamily}, sans-serif;
                                --text-weight: {item.bold ? 'bold' : 'normal'};
                                --text-style: {item.italic ? 'italic' : 'normal'};
                                --text-decor: {item.underline ? 'underline' : 'none'};
                                --text-opacity: {(100 - item.transparency) / 100};
                                text-align: {item.alignment};
                                caret-color: {item.hasFill ? item.color : (item.hasOutline ? item.outlineColor : '#ffffff')};
                              "
                              oninput={(e) => {
                                const newText = (e.currentTarget as HTMLElement).innerText;
                                updateActiveText((it) => (it.text = newText));
                              }}
                              onpaste={(e) => {
                                e.preventDefault();
                                const text = e.clipboardData?.getData('text/plain') || '';
                                document.execCommand('insertText', false, text);
                              }}
                              onkeydown={(e) => {
                                e.stopPropagation();
                                if (e.key === 'Escape') {
                                  (e.currentTarget as HTMLElement).blur();
                                }
                              }}
                            ></div>
                          {:else}
                            <div
                              dir="ltr"
                              class="picasa-editable-text whitespace-pre select-none drop-shadow-[0_2px_4px_rgba(0,0,0,0.8)]"
                              style="
                                --text-color: {item.hasFill ? item.color : 'transparent'};
                                --text-fill: {item.hasFill ? item.color : 'transparent'};
                                --text-stroke: {item.hasOutline && item.outlineWidth > 0 ? `${item.outlineWidth}px ${item.outlineColor}` : '0px transparent'};
                                --text-size: {item.fontSize}px;
                                --text-font: {item.fontFamily}, sans-serif;
                                --text-weight: {item.bold ? 'bold' : 'normal'};
                                --text-style: {item.italic ? 'italic' : 'normal'};
                                --text-decor: {item.underline ? 'underline' : 'none'};
                                --text-opacity: {(100 - item.transparency) / 100};
                                text-align: {item.alignment};
                              "
                            >
                              {item.text}
                            </div>
                          {/if}

                        <!-- The Iconic Picasa Circular Compass Dial Widget -->
                        {#if isActive}
                          {@const handleDist = Math.max(50, Math.min(220, 65 * item.scale))}
                          <!-- Upright Compass Dial Overlay (Counter-rotated & counter-scaled so it stays upright on screen) -->
                          <div
                            class="absolute inset-0 flex items-center justify-center pointer-events-none overflow-visible"
                            style="transform: rotate({-item.angle}deg) scale({1 / item.scale}); transform-origin: center center;"
                          >
                            <!-- Dial Root (Fixed size, perfectly screen-aligned) -->
                            <div class="relative w-[130px] h-[130px] flex items-center justify-center pointer-events-none select-none">
                              
                              <!-- Compass Outer Bezel & Stationary Degree Ticks -->
                              <svg class="w-full h-full pointer-events-none drop-shadow-xl overflow-visible" viewBox="0 0 130 130">
                                <!-- Bezel Donut (Frosted smoky glass ring) -->
                                <circle
                                  cx="65"
                                  cy="65"
                                  r="48"
                                  stroke="rgba(24, 30, 42, 0.72)"
                                  stroke-width="20"
                                  fill="none"
                                />
                                <!-- Outer Rim Highlight -->
                                <circle
                                  cx="65"
                                  cy="65"
                                  r="58"
                                  stroke="rgba(255, 255, 255, 0.4)"
                                  stroke-width="1.5"
                                  fill="none"
                                />
                                <!-- Inner Rim Highlight -->
                                <circle
                                  cx="65"
                                  cy="65"
                                  r="38"
                                  stroke="rgba(255, 255, 255, 0.3)"
                                  stroke-width="1"
                                  fill="none"
                                />

                                <!-- 36 Radial Compass Tick Marks (every 10 degrees, stationary & screen-aligned) -->
                                {#each Array(36) as _, i}
                                  {@const deg = i * 10}
                                  {@const isMajor = deg % 30 === 0}
                                  {@const rad = (deg * Math.PI) / 180}
                                  {@const rOuter = 58}
                                  {@const rInner = isMajor ? 38 : 46}
                                  {@const x1 = 65 + rInner * Math.cos(rad)}
                                  {@const y1 = 65 + rInner * Math.sin(rad)}
                                  {@const x2 = 65 + rOuter * Math.cos(rad)}
                                  {@const y2 = 65 + rOuter * Math.sin(rad)}
                                  <line
                                    {x1}
                                    {y1}
                                    {x2}
                                    {y2}
                                    stroke={isMajor ? 'rgba(255, 255, 255, 0.9)' : 'rgba(255, 255, 255, 0.45)'}
                                    stroke-width={isMajor ? '1.5' : '1'}
                                  />
                                {/each}

                                <!-- 0° Stationary Index Arrow (pointing inward from right edge of bezel) -->
                                <polygon
                                  points="58,62 58,68 53,65"
                                  fill="rgba(255, 255, 255, 0.9)"
                                />

                                <!-- Upright Center Crosshair (+) in central clear aperture -->
                                <line x1="56" y1="65" x2="74" y2="65" stroke="rgba(255, 255, 255, 0.85)" stroke-width="1.25" />
                                <line x1="65" y1="56" x2="65" y2="74" stroke="rgba(255, 255, 255, 0.85)" stroke-width="1.25" />
                              </svg>

                              <!-- Arm & Bullseye Knob (Rotates to item.angle) -->
                              <div
                                class="absolute inset-0 flex items-center justify-center pointer-events-none"
                                style="transform: rotate({item.angle}deg); transform-origin: center center;"
                              >
                                <!-- Stalk line connecting bezel to the knob -->
                                <svg class="absolute inset-0 w-full h-full pointer-events-none overflow-visible">
                                  <line
                                    x1="65"
                                    y1="65"
                                    x2={65 + handleDist}
                                    y2="65"
                                    stroke="rgba(255, 255, 255, 0.9)"
                                    stroke-width="2.5"
                                    stroke-linecap="round"
                                  />
                                </svg>

                                <!-- Iconic Bullseye Knob -->
                                <button
                                  type="button"
                                  aria-label="Rotate and scale text"
                                  class="picasa-dial-handle absolute top-1/2 -translate-y-1/2 -translate-x-1/2 w-6 h-6 rounded-full bg-white border border-black/25 shadow-[0_2px_10px_rgba(0,0,0,0.6),0_0_0_1px_rgba(255,255,255,0.9)] flex items-center justify-center cursor-grab active:cursor-grabbing pointer-events-auto transition-transform hover:scale-115 active:scale-95 group/knob"
                                  style="left: {65 + handleDist}px;"
                                  onpointerdown={(e) => startDialHandleDrag(item, e)}
                                >
                                  <div class="w-3 h-3 rounded-full bg-gradient-to-br from-amber-400 via-orange-500 to-rose-600 shadow-inner group-hover/knob:from-amber-300 group-hover/knob:to-orange-500 transition-colors"></div>
                                </button>
                              </div>

                              <!-- Live HUD Readout Pill (Positioned cleanly below the dial) -->
                              <div class="absolute -bottom-7 left-1/2 -translate-x-1/2 bg-black/80 backdrop-blur-md px-2.5 py-0.5 rounded-full border border-white/20 text-[10px] font-mono text-white whitespace-nowrap shadow-md pointer-events-none select-none flex items-center gap-2">
                                <span>Angle: {textHud ? textHud.angle : Math.round(item.angle)}°</span>
                                <span class="w-1 h-1 rounded-full bg-neutral-400"></span>
                                <span>Scale: {textHud ? textHud.scale : Math.round(item.scale * 100)}%</span>
                              </div>

                            </div>
                          </div>
                        {/if}
                      </div>
                    </div>
                  {/each}
                </div>
                {/if}
              </div>
            </div>

            <!-- Floating Picasa Straighten Control Bar -->
            {#if activeTool === 'straighten'}
              <div
                class="absolute bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#282828]/95 backdrop-blur-md rounded-full px-5 py-2.5 border border-white/20 shadow-2xl flex items-center gap-3.5 animate-in fade-in slide-in-from-bottom-3 duration-200 select-none"
              >
                <!-- Quick Auto-Level Recommendation Pill -->
                {#if straightenSuggestions.length > 0}
                  <button
                    type="button"
                    onclick={() => selectStraightenSuggestion(straightenSuggestions[0])}
                    class="px-3 py-1 rounded-full bg-cyan-950/70 hover:bg-cyan-900/90 active:bg-cyan-800 text-cyan-300 text-xs font-semibold tracking-wide transition-colors cursor-pointer border border-cyan-500/40 flex items-center gap-1.5 shadow-sm"
                    title="Auto-level: {straightenSuggestions[0].label} ({formatStraightenAngle(straightenSuggestions[0].angle)})"
                  >
                    <span class="text-[11px] font-bold">Auto</span>
                    <span class="font-mono text-[10px] text-cyan-200 font-bold">
                      {formatStraightenAngle(straightenSuggestions[0].angle)}
                    </span>
                  </button>
                  <div class="h-4 w-px bg-white/20"></div>
                {/if}

                <div class="flex items-center gap-3 w-56">
                  <input
                    type="range"
                    min="-45"
                    max="45"
                    step="0.1"
                    bind:value={straightenAngle}
                    class="w-full accent-cyan-400 cursor-pointer h-1.5 bg-neutral-600 rounded-lg appearance-none"
                  />
                  <span class="font-mono text-xs font-semibold text-cyan-300 w-12 text-right">
                    {formatStraightenAngle(straightenAngle)}
                  </span>
                </div>
                <div class="h-4 w-px bg-white/20"></div>
                <button
                  type="button"
                  onclick={handleApplyStraighten}
                  class="px-4 py-1 rounded bg-[#484848] hover:bg-[#585858] active:bg-[#383838] text-white text-xs font-bold uppercase tracking-wider transition-colors cursor-pointer border border-white/15"
                >
                  Apply
                </button>
                <button
                  type="button"
                  onclick={handleCancelStraighten}
                  class="px-4 py-1 rounded bg-[#484848] hover:bg-[#585858] active:bg-[#383838] text-neutral-300 hover:text-white text-xs font-bold uppercase tracking-wider transition-colors cursor-pointer border border-white/15"
                >
                  Cancel
                </button>
              </div>
            {/if}
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
                  transform: rotate({effectiveCanvasRotation}deg);
                  transform-origin: center center;
                  transition: {isTransitioning ? 'transform 380ms cubic-bezier(0.2, 0.9, 0.3, 1)' : 'none'};
                "
              >
                <img
                  src={activeImageSrc}
                  alt="Current edited file"
                  class="max-h-[calc(100vh-180px)] max-w-full rounded-sm object-contain"
                  style="filter: {liveFilterStyle};"
                />
              </div>
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
            title={`${undoLabel} (Ctrl+Z)`}
          >
            <Undo2 class="w-4 h-4" />
          </button>

          <!-- Redo Action -->
          <button
            onclick={handleRedo}
            disabled={!sessionState?.can_redo || isEditing}
            class="p-2 rounded-full hover:bg-white/10 transition-colors disabled:opacity-30 disabled:hover:bg-transparent cursor-pointer"
            title={`${redoLabel} (Ctrl+Y)`}
          >
            <Redo2 class="w-4 h-4" />
          </button>

          <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

          <!-- 1:1 Zoom Toggle -->
          <!-- Zoom Percentage Pill & 1:1 Zoom Toggle -->
          {#if zoomLevel > 1.05}
            <button
              onclick={resetZoom}
              class="px-2 py-0.5 rounded-full bg-blue-600/30 hover:bg-blue-600/50 text-blue-300 text-[11px] font-mono font-medium border border-blue-500/40 transition-colors cursor-pointer"
              title="Click to reset zoom (Fit)"
            >
              {Math.round(zoomLevel * 100)}%
            </button>
          {/if}
          <button
            onclick={toggleZoom}
            class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
              zoomLevel > 1.05 ? 'text-blue-400' : 'hover:text-white'
            }`}
            title={zoomLevel > 1.05 ? "Reset to Fit (Spacebar)" : "Zoom 100% (Spacebar)"}
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

          <div class="w-px h-5 bg-neutral-700/60 mx-0.5"></div>

          <!-- Properties / Info for Nerds Toggle Button -->
          <button
            onclick={() => (showInfo = !showInfo)}
            class={`p-2 rounded-full hover:bg-white/10 transition-colors cursor-pointer ${
              showInfo ? 'text-blue-400 bg-white/15' : 'text-neutral-400 hover:text-white'
            }`}
            title="Properties / Info for Nerds (i)"
          >
            <Info class="w-4 h-4" />
          </button>

          <!-- Pop out into Standalone Pica Photo Viewer -->
          <button
            onclick={() => currentPhoto && openViewerWindow(currentPhoto.file_path)}
            class="p-2 rounded-full hover:bg-white/10 text-neutral-400 hover:text-purple-400 transition-colors cursor-pointer"
            title="Pop out into Pica Photo Viewer"
          >
            <Expand class="w-4 h-4" />
          </button>
        </div>
      </div>

    </div>

    <!-- Collapsible Right "Properties" ("Info for Nerds") Sidebar -->
    <PropertiesSidebar
      photo={currentPhoto}
      metadata={photoMetadata}
      isOpen={showInfo}
      onClose={() => (showInfo = false)}
    />
  </div>
{/if}

<!-- SVG Filter Definitions for Real-time Hardware-Accelerated Adjustments -->
<svg class="sr-only" aria-hidden="true" style="position: absolute; width: 0; height: 0; overflow: hidden;">
  <defs>
    <filter id="pica-fill-light-filter" color-interpolation-filters="sRGB">
      <feComponentTransfer>
        <feFuncR type="table" tableValues={fillLightTableValues} />
        <feFuncG type="table" tableValues={fillLightTableValues} />
        <feFuncB type="table" tableValues={fillLightTableValues} />
      </feComponentTransfer>
    </filter>
  </defs>
</svg>

<style>
  /* Picasa station & transparency sliders */
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
    width: 10px;
    height: 16px;
    margin-top: -6px;
    background: linear-gradient(180deg, #ffffff 0%, #cbd5e1 50%, #94a3b8 100%);
    border: 1px solid #475569;
    border-radius: 3px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.6), inset 0 1px 0 #ffffff;
    cursor: pointer;
  }
  input[type='range'].picasa-station-slider::-webkit-slider-thumb:hover {
    background: linear-gradient(180deg, #ffffff 0%, #e2e8f0 50%, #cbd5e1 100%);
    border-color: #38bdf8;
    box-shadow: 0 0 6px rgba(56, 189, 248, 0.6);
  }

  /* Active and preview text element styling driven by CSS variables */
  :global(.picasa-editable-text) {
    color: var(--text-color) !important;
    -webkit-text-fill-color: var(--text-fill) !important;
    -webkit-text-stroke: var(--text-stroke) !important;
    font-size: var(--text-size) !important;
    font-family: var(--text-font) !important;
    font-weight: var(--text-weight) !important;
    font-style: var(--text-style) !important;
    text-decoration: var(--text-decor) !important;
    opacity: var(--text-opacity) !important;
    direction: ltr !important;
    unicode-bidi: isolate !important;
    paint-order: stroke fill markers !important;
  }

  /* Any child nodes generated by browser or contenteditable inherit from .picasa-editable-text */
  :global(.picasa-editable-text *) {
    color: inherit !important;
    -webkit-text-fill-color: inherit !important;
    -webkit-text-stroke: inherit !important;
    font-family: inherit !important;
    font-size: inherit !important;
    font-weight: inherit !important;
    font-style: inherit !important;
    text-decoration: inherit !important;
    background-color: transparent !important;
  }
</style>

