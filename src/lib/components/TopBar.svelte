<script lang="ts">
  import {
    Search,
    Star,
    Grid,
    RefreshCw,
    FolderCog,
    Sun,
    Moon,
    DownloadCloud,
    Bug,
    Trash2,
    BarChart2,
    Settings,
    Layers,
    Clock,
    Download,
    Eye,
    Users,
    Loader2,
    SlidersHorizontal,
  } from '@lucide/svelte';
  import {
    clearThumbnailCache,
    getThumbStats,
    getFaceTimingStats,
    setFaceTimingRecording,
    clearFaceTimingRecords,
    exportFaceTimingCsv,
    reclusterFaces,
    realignFaceEmbeddings,
    getFaceSettings,
  } from '../api';
  import type { FaceTimingStats } from '../types';
  import FaceTimingModal from './FaceTimingModal.svelte';
  import { showEditOverlay, showFaceDebugOverlay } from '../debugStore';

  let {
    searchQuery = $bindable(''),
    onlyFavorites = $bindable(false),
    theme = $bindable('dark'),
    onRefresh = () => {},
    onOpenFolderManager = () => {},
    onOpenSettings = () => {},
    onTriggerScan = () => {},
  }: {
    searchQuery: string;
    onlyFavorites: boolean;
    theme: 'dark' | 'light';
    onRefresh: () => void;
    onOpenFolderManager: () => void;
    onOpenSettings?: () => void;
    onTriggerScan?: () => void;
  } = $props();

  function toggleTheme() {
    theme = theme === 'dark' ? 'light' : 'dark';
    if (typeof document !== 'undefined') {
      if (theme === 'light') {
        document.documentElement.classList.add('light');
      } else {
        document.documentElement.classList.remove('light');
      }
      localStorage.setItem('pica_theme', theme);
    }
  }

  // Debug menu state
  let debugOpen = $state(false);
  let debugMsg = $state('');
  let thumbStats = $state<{ cached_thumbs: number; total_bytes: number } | null>(null);
  let isBusy = $state(false);

  let isFaceTimingModalOpen = $state(false);
  let faceTimingStats = $state<FaceTimingStats | null>(null);
  let isRecordingFaceTimings = $state(false);
  let isTimingBusy = $state(false);

  async function loadThumbStats() {
    thumbStats = await getThumbStats();
  }

  async function loadFaceTimingStats() {
    try {
      faceTimingStats = await getFaceTimingStats();
      isRecordingFaceTimings = faceTimingStats.is_recording;
    } catch (e) {
      console.error('Failed to load face timing stats:', e);
    }
  }

  function toggleDebug() {
    debugOpen = !debugOpen;
    if (debugOpen) {
      debugMsg = '';
      reclusterResultMsg = null;
      loadThumbStats();
      loadFaceTimingStats();
      getFaceSettings()
        .then((fs) => {
          if (
            fs.cluster_threshold &&
            typeof window !== 'undefined' &&
            !localStorage.getItem('pica_face_cluster_threshold')
          ) {
            faceClusterThreshold = Math.round(fs.cluster_threshold * 100) / 100;
          }
        })
        .catch(() => {});
    }
  }

  async function handleToggleFaceTimingRecording(e: Event) {
    const target = e.currentTarget as HTMLInputElement;
    const newEnabled = target.checked;
    isTimingBusy = true;
    try {
      await setFaceTimingRecording(newEnabled);
      await loadFaceTimingStats();
      debugMsg = newEnabled
        ? '⏱️ Face scan timing recording started'
        : '⏹️ Face scan timing recording stopped';
    } catch (err) {
      console.error('Failed to toggle face timing recording:', err);
      debugMsg = `❌ Error: ${err}`;
      isRecordingFaceTimings = !newEnabled;
    } finally {
      isTimingBusy = false;
    }
  }

  async function handleDownloadFaceTimingCsv() {
    try {
      const csv = await exportFaceTimingCsv();
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'face_scan_timings.csv';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) {
      debugMsg = `❌ Export error: ${e}`;
    }
  }

  async function handleClearFaceTimings() {
    if (isTimingBusy) return;
    isTimingBusy = true;
    try {
      await clearFaceTimingRecords();
      await loadFaceTimingStats();
      debugMsg = '🗑️ Face scan timings cleared';
    } catch (e) {
      debugMsg = `❌ Error: ${e}`;
    } finally {
      isTimingBusy = false;
    }
  }

  function formatBytes(b: number) {
    if (b === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(b) / Math.log(k));
    return parseFloat((b / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  }

  async function handleClearThumbs() {
    if (isBusy) return;
    isBusy = true;
    debugMsg = 'Clearing…';
    try {
      const result = await clearThumbnailCache();
      debugMsg = `✅ Deleted ${result.deleted} thumbnails`;
      await loadThumbStats();
    } catch (e) {
      debugMsg = `❌ Error: ${e}`;
    } finally {
      isBusy = false;
    }
  }

  let faceClusterThreshold = $state<number>(0.50);
  let isReclustering = $state(false);
  let reclusterResultMsg = $state<string | null>(null);

  if (typeof window !== 'undefined') {
    const saved = localStorage.getItem('pica_face_cluster_threshold');
    if (saved) {
      const val = parseFloat(saved);
      if (!isNaN(val) && val >= 0.20 && val <= 0.85) {
        faceClusterThreshold = Math.round(val * 100) / 100;
      }
    }
  }

  function handleThresholdInput(e: Event) {
    const target = e.target as HTMLInputElement;
    faceClusterThreshold = parseFloat(target.value);
    if (typeof window !== 'undefined') {
      localStorage.setItem('pica_face_cluster_threshold', faceClusterThreshold.toFixed(2));
    }
    reclusterResultMsg = null;
  }

  function setPresetThreshold(val: number) {
    faceClusterThreshold = val;
    if (typeof window !== 'undefined') {
      localStorage.setItem('pica_face_cluster_threshold', faceClusterThreshold.toFixed(2));
    }
    reclusterResultMsg = null;
  }

  async function handleRerunFaceGrouping() {
    if (isReclustering) return;
    isReclustering = true;
    reclusterResultMsg = null;
    debugMsg = `Re-clustering faces with threshold ${faceClusterThreshold.toFixed(2)}...`;
    try {
      const res = await reclusterFaces(faceClusterThreshold);
      reclusterResultMsg = `Found ${res.people_count} people`;
      debugMsg = `✅ Re-clustered into ${res.people_count} people (threshold: ${faceClusterThreshold.toFixed(2)})`;
      onRefresh();
    } catch (e) {
      reclusterResultMsg = `Error: ${e}`;
      debugMsg = `❌ Re-cluster error: ${e}`;
    } finally {
      isReclustering = false;
    }
  }

  let isRealigning = $state(false);

  async function handleRealignEmbeddings() {
    if (isRealigning || isReclustering) return;
    isRealigning = true;
    reclusterResultMsg = null;
    debugMsg = 'Re-aligning all facial landmark embeddings...';
    try {
      const res = await realignFaceEmbeddings();
      reclusterResultMsg = `Realigned ${res.updated_faces} faces, ${res.people_count} people`;
      debugMsg = `✅ Realigned ${res.updated_faces} faces into ${res.people_count} people!`;
      onRefresh();
    } catch (e) {
      reclusterResultMsg = `Error: ${e}`;
      debugMsg = `❌ Realign error: ${e}`;
    } finally {
      isRealigning = false;
    }
  }
</script>

<header
  class="h-12 border-b px-4 flex items-center justify-between select-none shadow-xs transition-colors"
  style="
    background-color: var(--header-bg);
    border-color: var(--border-subtle);
    color: var(--text-main);
  "
>
  <!-- App Branding & Import Button -->
  <div class="flex items-center gap-3">
    <!-- Picasa Colorful Shutter Logo -->
    <button type="button" class="flex items-center gap-2 cursor-pointer" onclick={() => { onlyFavorites = false; searchQuery = ''; }}>
      <div class="relative w-6 h-6 flex items-center justify-center">
        <!-- Iconic Picasa Shutter Petals -->
        <svg viewBox="0 0 100 100" class="w-6 h-6 drop-shadow-xs">
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
      <span class="text-sm font-black tracking-wider uppercase" style="color: var(--text-main);">
        Pica
      </span>
    </button>

    <!-- Picasa Classic "Import" Button -->
    <button
      onclick={onTriggerScan || onRefresh}
      title="Scan and import photos into Pica"
      class="flex items-center gap-1.5 px-3 py-1 text-xs font-semibold rounded bg-blue-600 hover:bg-blue-500 text-white shadow-xs transition-colors"
    >
      <DownloadCloud class="w-3.5 h-3.5" />
      <span>Import</span>
    </button>

    <div class="h-4 w-px mx-1" style="background-color: var(--border-subtle);"></div>

    <!-- View Filter Tabs -->
    <div
      class="flex items-center gap-1 p-0.5 rounded border"
      style="background-color: var(--panel-bg); border-color: var(--border-subtle);"
    >
      <button
        onclick={() => (onlyFavorites = false)}
        class="flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors {
          !onlyFavorites ? 'bg-blue-600 text-white shadow-xs' : 'hover:bg-black/5 dark:hover:bg-white/10'
        }"
        style={!onlyFavorites ? '' : 'color: var(--text-muted);'}
      >
        <Grid class="w-3 h-3" />
        <span>Library</span>
      </button>

      <button
        onclick={() => (onlyFavorites = true)}
        class="flex items-center gap-1 px-2.5 py-1 rounded text-xs font-medium transition-colors {
          onlyFavorites ? 'bg-amber-600 text-white shadow-xs' : 'hover:bg-black/5 dark:hover:bg-white/10'
        }"
        style={onlyFavorites ? '' : 'color: var(--text-muted);'}
      >
        <Star class="w-3 h-3 fill-current" />
        <span>Starred</span>
      </button>
    </div>
  </div>

  <!-- Realtime Search Bar -->
  <div class="flex-1 max-w-md mx-6">
    <div class="relative flex items-center">
      <Search class="w-3.5 h-3.5 absolute left-2.5 pointer-events-none" style="color: var(--text-dim);" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="Search photos by filename..."
        class="w-full border rounded-full pl-8 pr-7 py-1 text-xs focus:outline-none transition-colors"
        style="
          background-color: var(--panel-bg);
          border-color: var(--border-subtle);
          color: var(--text-main);
        "
      />
      {#if searchQuery}
        <button
          onclick={() => (searchQuery = '')}
          class="absolute right-2.5 text-xs font-bold hover:text-red-400"
          style="color: var(--text-dim);"
          title="Clear search"
        >
          ✕
        </button>
      {/if}
    </div>
  </div>

  <!-- Right Actions: Debug, Theme Toggle, Refresh, Folder Manager -->
  <div class="flex items-center gap-2">
    <!-- Debug Menu (Dev Mode Only) -->
    {#if import.meta.env.DEV}
    <div class="relative">
      <button
        onclick={toggleDebug}
        class="p-1.5 rounded border transition-colors flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10 {debugOpen ? 'bg-amber-500/20 border-amber-500/50' : ''}"
        style="
          background-color: {debugOpen ? '' : 'var(--panel-bg)'};
          border-color: {debugOpen ? '' : 'var(--border-subtle)'};
          color: var(--text-main);
        "
        title="Debug Tools"
      >
        <Bug class="w-3.5 h-3.5 {debugOpen ? 'text-amber-400' : ''}" />
      </button>

      {#if debugOpen}
        <!-- Click-outside overlay -->
        <button
          class="fixed inset-0 z-40"
          onclick={() => (debugOpen = false)}
          tabindex="-1"
          aria-label="Close debug menu"
        ></button>

        <!-- Dropdown Panel -->
        <div
          class="absolute right-0 top-full mt-1.5 z-50 w-84 max-h-[85vh] overflow-y-auto rounded-lg shadow-2xl border text-xs"
          style="background-color: var(--panel-bg); border-color: var(--border-subtle); color: var(--text-main);"
        >
          <!-- Header -->
          <div class="px-3 py-2.5 border-b flex items-center gap-2" style="border-color: var(--border-subtle);">
            <Bug class="w-3.5 h-3.5 text-amber-400" />
            <span class="font-bold text-amber-400 tracking-wide uppercase text-[10px]">Debug Tools</span>
          </div>

          <!-- Thumb Stats -->
          <div class="px-3 py-3 border-b space-y-1.5" style="border-color: var(--border-subtle);">
            <div class="flex items-center gap-1.5 mb-2" style="color: var(--text-dim);">
              <BarChart2 class="w-3 h-3" />
              <span class="font-semibold uppercase tracking-wider text-[10px]">Thumbnail Cache</span>
            </div>
            {#if thumbStats}
              <div class="flex justify-between" style="color: var(--text-muted);">
                <span>Cached thumbnails</span>
                <span class="font-mono font-semibold" style="color: var(--text-main);">{thumbStats.cached_thumbs.toLocaleString()}</span>
              </div>
              <div class="flex justify-between" style="color: var(--text-muted);">
                <span>Cache size on disk</span>
                <span class="font-mono font-semibold" style="color: var(--text-main);">{formatBytes(thumbStats.total_bytes)}</span>
              </div>
            {:else}
              <div style="color: var(--text-dim);">Loading stats…</div>
            {/if}
          </div>

          <!-- Image Edit Debug Overlay Checkbox -->
          <div class="px-3 py-2.5 border-b space-y-1.5" style="border-color: var(--border-subtle);">
            <label class="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                bind:checked={$showEditOverlay}
                class="w-3.5 h-3.5 rounded accent-amber-500 cursor-pointer"
              />
              <div class="flex items-center gap-1.5 font-semibold text-amber-400">
                <Layers class="w-3.5 h-3.5" />
                <span>Image Edit Debug Overlay</span>
              </div>
            </label>
            <div class="text-[10px] pl-5.5 leading-relaxed" style="color: var(--text-dim);">
              Follow live session JSON, intermediate thumbs, render status, and proxy paths on the image edit page.
            </div>
          </div>

          <!-- Face Detection Debug Overlay Checkbox -->
          <div class="px-3 py-2.5 border-b space-y-1.5" style="border-color: var(--border-subtle);">
            <label class="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                bind:checked={$showFaceDebugOverlay}
                class="w-3.5 h-3.5 rounded accent-purple-500 cursor-pointer"
              />
              <div class="flex items-center gap-1.5 font-semibold text-purple-400">
                <Users class="w-3.5 h-3.5" />
                <span>Face Detection & Grouping Overlay</span>
              </div>
            </label>
            <div class="text-[10px] pl-5.5 leading-relaxed" style="color: var(--text-dim);">
              Display detected face boxes, person cluster labels, and confidence tags directly over photos in the grid and viewer.
            </div>
          </div>

          <!-- Face Scan Profiler / Timings Section -->
          <div class="px-3 py-2.5 border-b space-y-2" style="border-color: var(--border-subtle);">
            <div class="flex items-center justify-between">
              <label class="flex items-center gap-2 cursor-pointer select-none">
                <input
                  type="checkbox"
                  checked={isRecordingFaceTimings}
                  disabled={isTimingBusy}
                  onchange={handleToggleFaceTimingRecording}
                  class="w-3.5 h-3.5 rounded accent-amber-500 cursor-pointer disabled:opacity-50"
                />
                <div class="flex items-center gap-1.5 font-semibold text-amber-400">
                  <Clock class="w-3.5 h-3.5" />
                  <span>Face Scan Timings</span>
                </div>
              </label>
              {#if isRecordingFaceTimings}
                <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] font-bold bg-amber-500/20 text-amber-400 uppercase tracking-wider animate-pulse">
                  REC
                </span>
              {/if}
            </div>

            <div class="text-[10px] pl-5.5 leading-relaxed" style="color: var(--text-dim);">
              Record per-photo face detection, embedding & decode timings to CSV log for later debug.
            </div>

            {#if faceTimingStats && (faceTimingStats.record_count > 0 || isRecordingFaceTimings)}
              <div class="p-2 rounded border bg-black/20 space-y-1 font-mono text-[10px]" style="border-color: var(--border-subtle);">
                <div class="flex justify-between" style="color: var(--text-muted);">
                  <span style="color: var(--text-dim);">Logged:</span>
                  <span class="font-semibold" style="color: var(--text-main);">{faceTimingStats.record_count} photos</span>
                </div>
                {#if faceTimingStats.record_count > 0}
                  <div class="flex justify-between" style="color: var(--text-muted);">
                    <span style="color: var(--text-dim);">Avg Detect:</span>
                    <span class="font-semibold text-emerald-400">{faceTimingStats.avg_detect_ms} ms</span>
                  </div>
                  <div class="flex justify-between" style="color: var(--text-muted);">
                    <span style="color: var(--text-dim);">Avg Embed:</span>
                    <span class="font-semibold text-sky-400">{faceTimingStats.avg_embed_ms} ms</span>
                  </div>
                  <div class="flex justify-between" style="color: var(--text-muted);">
                    <span style="color: var(--text-dim);">Avg Total:</span>
                    <span class="font-semibold text-amber-400">{faceTimingStats.avg_total_ms} ms</span>
                  </div>
                {/if}
              </div>

              <!-- Timing Action Buttons -->
              <div class="flex items-center gap-1.5 pt-0.5">
                <button
                  onclick={() => (isFaceTimingModalOpen = true)}
                  class="flex-1 px-2 py-1 rounded border text-[11px] font-medium text-center hover:bg-white/10 transition-colors flex items-center justify-center gap-1"
                  style="border-color: var(--border-subtle); color: var(--text-main);"
                >
                  <Eye class="w-3 h-3 text-neutral-400" />
                  <span>Inspect</span>
                </button>
                <button
                  onclick={handleDownloadFaceTimingCsv}
                  disabled={!faceTimingStats || faceTimingStats.record_count === 0}
                  class="px-2 py-1 rounded border text-[11px] font-medium hover:bg-white/10 disabled:opacity-40 disabled:cursor-not-allowed transition-colors flex items-center gap-1"
                  style="border-color: var(--border-subtle); color: var(--text-main);"
                  title="Download CSV log"
                >
                  <Download class="w-3 h-3 text-amber-400" />
                  <span>CSV</span>
                </button>
                <button
                  onclick={handleClearFaceTimings}
                  disabled={!faceTimingStats || faceTimingStats.record_count === 0}
                  class="p-1 rounded border text-red-400 hover:bg-red-500/20 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                  style="border-color: var(--border-subtle);"
                  title="Clear timings"
                >
                  <Trash2 class="w-3 h-3" />
                </button>
              </div>
            {/if}
          </div>

          <!-- Face Grouping Settings & Clustering Section -->
          <div class="px-3 py-2.5 border-b space-y-2.5" style="border-color: var(--border-subtle);">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-1.5 font-semibold text-purple-400">
                <SlidersHorizontal class="w-3.5 h-3.5" />
                <span>Face Grouping (Sow-Grow)</span>
              </div>
              <span class="font-mono text-[11px] font-bold px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 border border-purple-500/30">
                {faceClusterThreshold.toFixed(2)}
              </span>
            </div>

            <!-- Slider -->
            <div class="space-y-1">
              <input
                type="range"
                min="0.30"
                max="0.75"
                step="0.01"
                value={faceClusterThreshold}
                oninput={handleThresholdInput}
                class="w-full accent-purple-500 cursor-pointer h-1.5 rounded-lg bg-black/30"
              />
              <div class="flex justify-between text-[9px] font-mono" style="color: var(--text-dim);">
                <span>0.30 (Loose)</span>
                <span>0.50 (Default)</span>
                <span>0.75 (Strict)</span>
              </div>
            </div>

            <!-- Dynamic Sensitivity Explanation -->
            <div class="text-[10px] leading-relaxed p-2 rounded bg-black/20 border space-y-1" style="border-color: var(--border-subtle); color: var(--text-muted);">
              {#if faceClusterThreshold < 0.46}
                <div class="flex items-start gap-1.5">
                  <span class="inline-block w-1.5 h-1.5 rounded-full bg-amber-400 shrink-0 mt-1"></span>
                  <span><strong>Loose</strong>: Merges similar faces into fewer, larger people groups. Tolerates angles & expressions, but may merge similar-looking people.</span>
                </div>
              {:else if faceClusterThreshold <= 0.55}
                <div class="flex items-start gap-1.5">
                  <span class="inline-block w-1.5 h-1.5 rounded-full bg-emerald-400 shrink-0 mt-1"></span>
                  <span><strong>Balanced (Recommended)</strong>: Optimal sweet spot for ArcFace. Keeps distinct individuals separated while grouping the same person across shots.</span>
                </div>
              {:else}
                <div class="flex items-start gap-1.5">
                  <span class="inline-block w-1.5 h-1.5 rounded-full bg-sky-400 shrink-0 mt-1"></span>
                  <span><strong>Strict</strong>: High precision. Only groups near-identical faces; will split the same person into multiple groups across lighting/angles.</span>
                </div>
              {/if}
            </div>

            <!-- Quick Presets -->
            <div class="flex items-center gap-1.5">
              <button
                onclick={() => setPresetThreshold(0.42)}
                class="flex-1 px-1.5 py-1 rounded border text-[10px] text-center hover:bg-white/10 transition-colors {faceClusterThreshold === 0.42 ? 'border-purple-500 text-purple-300 font-bold bg-purple-500/20' : ''}"
                style={faceClusterThreshold !== 0.42 ? "border-color: var(--border-subtle); color: var(--text-dim);" : ""}
              >
                Loose (0.42)
              </button>
              <button
                onclick={() => setPresetThreshold(0.50)}
                class="flex-1 px-1.5 py-1 rounded border text-[10px] text-center hover:bg-white/10 transition-colors {faceClusterThreshold === 0.50 ? 'border-purple-500 text-purple-300 font-bold bg-purple-500/20' : ''}"
                style={faceClusterThreshold !== 0.50 ? "border-color: var(--border-subtle); color: var(--text-dim);" : ""}
              >
                Default (0.50)
              </button>
              <button
                onclick={() => setPresetThreshold(0.58)}
                class="flex-1 px-1.5 py-1 rounded border text-[10px] text-center hover:bg-white/10 transition-colors {faceClusterThreshold === 0.58 ? 'border-purple-500 text-purple-300 font-bold bg-purple-500/20' : ''}"
                style={faceClusterThreshold !== 0.58 ? "border-color: var(--border-subtle); color: var(--text-dim);" : ""}
              >
                Strict (0.58)
              </button>
            </div>

            <!-- Re-cluster Action Button -->
            <button
              onclick={handleRerunFaceGrouping}
              disabled={isReclustering}
              class="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-md border text-xs font-semibold transition-colors
                {isReclustering ? 'opacity-50 cursor-not-allowed bg-purple-500/10 border-purple-500/30 text-purple-300' : 'bg-purple-600/20 hover:bg-purple-600/30 border-purple-500/50 text-purple-200'}"
            >
              {#if isReclustering}
                <Loader2 class="w-3.5 h-3.5 text-purple-300 animate-spin" />
                <span>Re-clustering with {faceClusterThreshold.toFixed(2)}...</span>
              {:else}
                <RefreshCw class="w-3.5 h-3.5 text-purple-300" />
                <span>Rerun Grouping ({faceClusterThreshold.toFixed(2)})</span>
              {/if}
            </button>

            <!-- Realign Embeddings Button -->
            <button
              onclick={handleRealignEmbeddings}
              disabled={isRealigning || isReclustering}
              class="w-full flex items-center justify-center gap-2 px-3 py-1.5 rounded-md border text-[11px] font-medium transition-colors
                {isRealigning ? 'opacity-50 cursor-not-allowed bg-emerald-500/10 border-emerald-500/30 text-emerald-300' : 'bg-emerald-600/10 hover:bg-emerald-600/20 border-emerald-500/40 text-emerald-200'}"
            >
              {#if isRealigning}
                <Loader2 class="w-3 h-3 text-emerald-300 animate-spin" />
                <span>Re-aligning landmarks (112x112)...</span>
              {:else}
                <SlidersHorizontal class="w-3 h-3 text-emerald-300" />
                <span>Refresh Aligned Embeddings</span>
              {/if}
            </button>

            {#if reclusterResultMsg}
              <div class="text-[10px] text-center font-medium text-emerald-400">
                ✅ {reclusterResultMsg}
              </div>
            {/if}
          </div>

          <!-- Actions -->
          <div class="px-3 py-3 space-y-2">

            <button
              onclick={handleClearThumbs}
              disabled={isBusy}
              class="w-full flex items-center gap-2 px-3 py-2 rounded-md border transition-colors text-left
                {isBusy ? 'opacity-50 cursor-not-allowed' : 'hover:bg-red-500/10 hover:border-red-500/40'}"
              style="border-color: var(--border-subtle);"
            >
              <Trash2 class="w-3.5 h-3.5 text-red-400 shrink-0" />
              <div>
                <div class="font-semibold text-red-400">Clear Thumbnail Cache</div>
                <div class="text-[10px] mt-0.5" style="color: var(--text-dim);">
                  Deletes all cached .jpg thumbs — lets you watch the queue prioritization in action from a fresh state.
                </div>
              </div>
            </button>

            {#if debugMsg}
              <div class="px-2 py-1.5 rounded text-[11px] font-medium" style="background-color: var(--canvas-bg); color: var(--text-muted);">
                {debugMsg}
              </div>
            {/if}
          </div>
        </div>
      {/if}
    </div>
    {/if}

    <!-- Theme Switch (Dark / Light) -->
    <button
      onclick={toggleTheme}
      class="p-1.5 rounded border transition-colors flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10"
      style="
        background-color: var(--panel-bg);
        border-color: var(--border-subtle);
        color: var(--text-main);
      "
      title={theme === 'dark' ? 'Switch to Classic Light Mode' : 'Switch to Dark Mode'}
    >
      {#if theme === 'dark'}
        <Sun class="w-3.5 h-3.5 text-amber-400" />
      {:else}
        <Moon class="w-3.5 h-3.5 text-slate-700" />
      {/if}
    </button>

    <!-- Refresh Library -->
    <button
      onclick={onRefresh}
      class="p-1.5 rounded border transition-colors flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10"
      style="
        background-color: var(--panel-bg);
        border-color: var(--border-subtle);
        color: var(--text-main);
      "
      title="Refresh Library"
    >
      <RefreshCw class="w-3.5 h-3.5" />
    </button>

    <!-- Folder Manager Button -->
    <button
      onclick={onOpenFolderManager}
      class="flex items-center gap-1.5 px-2.5 py-1.5 rounded border text-xs font-medium transition-colors hover:bg-black/5 dark:hover:bg-white/10"
      style="
        background-color: var(--panel-bg);
        border-color: var(--border-subtle);
        color: var(--text-main);
      "
      title="Configure watched folders and scanning options"
    >
      <FolderCog class="w-3.5 h-3.5 text-blue-500" />
      <span class="hidden sm:inline">Folder Manager...</span>
    </button>

    <!-- Options / Settings Button -->
    <button
      onclick={onOpenSettings}
      class="p-1.5 rounded border transition-colors flex items-center justify-center hover:bg-black/5 dark:hover:bg-white/10"
      style="
        background-color: var(--panel-bg);
        border-color: var(--border-subtle);
        color: var(--text-main);
      "
      title="Options & Preferences"
    >
      <Settings class="w-3.5 h-3.5" />
    </button>
  </div>
</header>

<FaceTimingModal bind:isOpen={isFaceTimingModalOpen} />
