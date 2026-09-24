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
  } from '@lucide/svelte';
  import { clearThumbnailCache, getThumbStats } from '../api';

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

  async function loadThumbStats() {
    thumbStats = await getThumbStats();
  }

  function toggleDebug() {
    debugOpen = !debugOpen;
    if (debugOpen) {
      debugMsg = '';
      loadThumbStats();
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
    <div class="flex items-center gap-2 cursor-pointer" onclick={() => { onlyFavorites = false; searchQuery = ''; }}>
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
    </div>

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
    <!-- Debug Menu -->
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
          class="absolute right-0 top-full mt-1.5 z-50 w-72 rounded-lg shadow-2xl border text-xs"
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
