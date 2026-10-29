<script lang="ts">
  import { onMount } from 'svelte';
  import {
    X,
    Shield,
    EyeOff,
    Save,
    Users,
    Sparkles,
    RefreshCw,
    Loader2,
    Check,
    Download,
    ArrowUpCircle,
  } from '@lucide/svelte';
  import {
    getAppSettings,
    updateAppSettings,
    getFaceSettings,
    updateFaceSettings,
    reclusterFaces,
  } from '../api';
  import type { AppSettings, FaceSettings } from '../types';
  import { updateStore, formatRelativeDate } from '../updateStore';

  let {
    isOpen = $bindable(false),
    onSettingsChanged = () => {},
  }: {
    isOpen: boolean;
    onSettingsChanged?: () => void;
  } = $props();

  let isReclustering = $state(false);
  let reclusterSuccess = $state(false);

  async function handleRecluster() {
    isReclustering = true;
    try {
      await reclusterFaces();
      reclusterSuccess = true;
      faceSettings = await getFaceSettings();
      onSettingsChanged();
      setTimeout(() => {
        reclusterSuccess = false;
      }, 3000);
    } catch (e) {
      console.error('Failed to recluster faces:', e);
    } finally {
      isReclustering = false;
    }
  }

  let settings = $state<AppSettings>({
    original_handling: 'backup',
    hide_originals: true,
  });

  let faceSettings = $state<FaceSettings>({
    enabled: false,
    models_installed: true,
    is_downloading: false,
    download_progress: 1.0,
    model_size_mb: 10.1,
    pending_photos: 0,
    total_faces: 0,
    total_people: 0,
  });

  let isLoading = $state(false);
  let isSaved = $state(false);

  async function loadSettings() {
    isLoading = true;
    try {
      const [appSt, faceSt] = await Promise.all([getAppSettings(), getFaceSettings()]);
      settings = appSt;
      faceSettings = faceSt;
    } catch (e) {
      console.error('Failed to load settings:', e);
    } finally {
      isLoading = false;
    }
  }

  $effect(() => {
    if (isOpen) {
      loadSettings();
      isSaved = false;
    }
  });

  async function handleToggleFaceDetection(enabled: boolean) {
    faceSettings.enabled = enabled;
    try {
      await updateFaceSettings(enabled);
      onSettingsChanged();
    } catch (e) {
      console.error('Failed to update face settings:', e);
    }
  }

  async function handleSave() {
    isLoading = true;
    try {
      await updateAppSettings(settings);
      isSaved = true;
      onSettingsChanged();
      setTimeout(() => {
        isOpen = false;
      }, 400);
    } catch (e) {
      console.error('Failed to save settings:', e);
    } finally {
      isLoading = false;
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!isOpen) return;
    if (e.key === 'Escape') {
      isOpen = false;
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  });
</script>

{#if isOpen}
  <!-- Backdrop -->
  <div
    role="presentation"
    class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4 animate-in fade-in duration-150 select-none"
    onclick={(e) => {
      if (e.target === e.currentTarget) isOpen = false;
    }}
  >
    <!-- Modal Window -->
    <div
      class="w-full max-w-md rounded-xl border shadow-2xl flex flex-col overflow-hidden animate-in zoom-in-95 duration-150"
      style="
        background-color: var(--panel-bg);
        border-color: var(--border-subtle);
        color: var(--text-main);
      "
    >
      <!-- Header -->
      <div
        class="px-5 py-3.5 border-b flex items-center justify-between"
        style="border-color: var(--border-subtle); background-color: var(--header-bg);"
      >
        <div class="flex items-center gap-2">
          <Shield class="w-4 h-4 text-blue-500" />
          <h2 class="text-sm font-bold tracking-wide uppercase">Preferences & Options</h2>
        </div>
        <button
          onclick={() => (isOpen = false)}
          class="p-1 rounded-full text-neutral-400 hover:text-white hover:bg-black/10 dark:hover:bg-white/10 transition-colors"
          title="Close (Esc)"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Body Content -->
      <div class="p-5 space-y-5 text-xs max-h-[75vh] overflow-y-auto">
        <!-- Section: Original Image Policy -->
        <div>
          <div class="block text-[11px] font-bold uppercase tracking-wider mb-2" style="color: var(--text-muted);">
            Original Photo Handling
          </div>
          <p class="text-[11px] mb-3 leading-relaxed" style="color: var(--text-dim);">
            Choose how Pica protects your original files when making edits (such as rotation, adjustments, or creative fixes).
          </p>

          <div class="space-y-2.5">
            <!-- Option 1: Backup (Recommended) -->
            <label
              class="flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors {
                settings.original_handling === 'backup'
                  ? 'bg-blue-500/10 border-blue-500/50'
                  : 'hover:bg-black/5 dark:hover:bg-white/5 border-neutral-700/40'
              }"
            >
              <input
                type="radio"
                name="original_handling"
                value="backup"
                checked={settings.original_handling === 'backup'}
                onchange={() => (settings.original_handling = 'backup')}
                class="mt-0.5 text-blue-600 focus:ring-blue-500"
              />
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-neutral-200">Back up originals (Recommended)</span>
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400 font-bold uppercase">
                    Non-Destructive
                  </span>
                </div>
                <div class="text-[11px] text-neutral-400 mt-0.5 leading-relaxed">
                  Preserves pristine untouched files in a hidden <code class="px-1 py-0.5 bg-black/30 rounded text-neutral-300">.pica_originals/</code> folder in the photo's directory. You can revert to original anytime.
                </div>
              </div>
            </label>

            <!-- Option 2: Overwrite -->
            <label
              class="flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors {
                settings.original_handling === 'overwrite'
                  ? 'bg-amber-500/10 border-amber-500/50'
                  : 'hover:bg-black/5 dark:hover:bg-white/5 border-neutral-700/40'
              }"
            >
              <input
                type="radio"
                name="original_handling"
                value="overwrite"
                checked={settings.original_handling === 'overwrite'}
                onchange={() => (settings.original_handling = 'overwrite')}
                class="mt-0.5 text-amber-600 focus:ring-amber-500"
              />
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-neutral-200">Overwrite originals</span>
                  <span class="text-[9px] px-1.5 py-0.5 rounded bg-amber-500/20 text-amber-400 font-bold uppercase">
                    Destructive on Save
                  </span>
                </div>
                <div class="text-[11px] text-neutral-400 mt-0.5 leading-relaxed">
                  Directly replaces original files when you commit changes. No backups are retained on disk.
                </div>
              </div>
            </label>
          </div>
        </div>

        <div class="w-full h-px" style="background-color: var(--border-subtle);"></div>

        <!-- Section: Library Display -->
        <div>
          <div class="block text-[11px] font-bold uppercase tracking-wider mb-2" style="color: var(--text-muted);">
            Library Display
          </div>

          <label
            class="flex items-center gap-2.5 p-2.5 rounded-lg border cursor-pointer transition-colors hover:bg-black/5 dark:hover:bg-white/5 border-neutral-700/40"
          >
            <input
              type="checkbox"
              bind:checked={settings.hide_originals}
              class="rounded text-blue-600 focus:ring-blue-500"
            />
            <div class="flex items-center gap-2">
              <EyeOff class="w-3.5 h-3.5 text-neutral-400" />
              <span class="text-xs font-medium text-neutral-200">
                Hide original copies from Pica photo grid
              </span>
            </div>
          </label>
          <div class="text-[10px] text-neutral-500 mt-1 pl-6">
            Prevents duplicate photo cards from appearing in the grid when backups exist in <code class="text-neutral-400">.pica_originals</code>.
          </div>
        </div>

        <div class="w-full h-px" style="background-color: var(--border-subtle);"></div>

        <!-- Section: Face Recognition & People -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <div class="block text-[11px] font-bold uppercase tracking-wider" style="color: var(--text-muted);">
              Face Detection & People Grouping
            </div>
            <span class="text-[9px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-400 font-bold uppercase flex items-center gap-1">
              <Sparkles class="w-2.5 h-2.5" />
              Modular
            </span>
          </div>

          <p class="text-[11px] mb-3 leading-relaxed" style="color: var(--text-dim);">
            Scan photos for faces in the background, group people automatically, and view people collections in the sidebar. Runs as a non-intrusive low-priority background process.
          </p>

          <div class="space-y-3">
            <!-- Global Face Toggle -->
            <label
              class="flex items-start gap-3 p-3 rounded-lg border cursor-pointer transition-colors {
                faceSettings.enabled
                  ? 'bg-purple-500/10 border-purple-500/50'
                  : 'hover:bg-black/5 dark:hover:bg-white/5 border-neutral-700/40'
              }"
            >
              <input
                type="checkbox"
                checked={faceSettings.enabled}
                onchange={(e) => handleToggleFaceDetection(e.currentTarget.checked)}
                class="mt-0.5 rounded text-purple-600 focus:ring-purple-500 cursor-pointer"
              />
              <div class="flex-1">
                <div class="flex items-center gap-2">
                  <span class="font-semibold text-neutral-200">Enable Face Detection</span>
                  {#if faceSettings.enabled}
                    <span class="text-[9px] px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-400 font-bold uppercase">
                      Active
                    </span>
                  {/if}
                </div>
                <div class="text-[11px] text-neutral-400 mt-0.5 leading-relaxed">
                  Processes photos in enabled folders at low CPU priority. You can also toggle detection on a per-folder basis in Folder Manager.
                </div>
              </div>
            </label>

            <!-- Model Status & Built-in Badge -->
            <div class="p-3 rounded-lg border border-neutral-700/50 bg-black/15 flex flex-col gap-2">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <Users class="w-4 h-4 text-purple-400" />
                  <span class="font-medium text-neutral-200 text-xs">High-Accuracy Neural Models</span>
                </div>
                <span class="flex items-center gap-1 text-[10px] text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20 font-medium">
                  <Check class="w-3 h-3" />
                  Built-in & Ready
                </span>
              </div>

              <p class="text-[10px] text-neutral-400 leading-relaxed">
                Pre-bundled with YuNet neural detector &amp; MobileFaceNet recognition models (512-D) with Picasa Sow &amp; Grow clustering. Zero download needed.
              </p>

              <!-- Face Stats -->
              {#if faceSettings.enabled}
                <div class="grid grid-cols-3 gap-2 pt-2 mt-1 border-t border-neutral-800/80 text-center">
                  <div class="bg-black/20 p-1.5 rounded">
                    <div class="text-[10px] text-neutral-400">Faces</div>
                    <div class="text-xs font-semibold text-neutral-200">{faceSettings.total_faces}</div>
                  </div>
                  <div class="bg-black/20 p-1.5 rounded">
                    <div class="text-[10px] text-neutral-400">People</div>
                    <div class="text-xs font-semibold text-neutral-200">{faceSettings.total_people}</div>
                  </div>
                  <div class="bg-black/20 p-1.5 rounded">
                    <div class="text-[10px] text-neutral-400">Pending</div>
                    <div class="text-xs font-semibold {faceSettings.pending_photos > 0 ? 'text-amber-400' : 'text-emerald-400'}">
                      {faceSettings.pending_photos}
                    </div>
                  </div>
                </div>

                <div class="pt-2 mt-1 border-t border-neutral-800/80 flex items-center justify-between">
                  <span class="text-[10px] text-neutral-400">Regroup people using medoid clustering:</span>
                  <button
                    onclick={handleRecluster}
                    disabled={isReclustering}
                    class="px-2.5 py-1 text-xs font-medium rounded border border-neutral-700 hover:bg-neutral-800 text-neutral-300 disabled:opacity-50 flex items-center gap-1.5 transition-colors"
                  >
                    {#if isReclustering}
                      <Loader2 class="w-3 h-3 animate-spin text-purple-400" />
                      <span>Clustering...</span>
                    {:else if reclusterSuccess}
                      <Check class="w-3 h-3 text-emerald-400" />
                      <span class="text-emerald-400">Re-clustered!</span>
                    {:else}
                      <RefreshCw class="w-3 h-3 text-purple-400" />
                      <span>Re-cluster People</span>
                    {/if}
                  </button>
                </div>
              {/if}
            </div>
          </div>
        </div>

        <div class="w-full h-px" style="background-color: var(--border-subtle);"></div>

        <!-- Section: Software Updates -->
        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="text-[11px] font-bold uppercase tracking-wider" style="color: var(--text-muted);">
              Software Updates
            </span>
            <span class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-white/5 border border-white/10 text-neutral-400">
              Current: v{$updateStore.currentVersion}
            </span>
          </div>

          <div
            class="p-3.5 rounded-lg border space-y-3"
            style="
              background-color: var(--canvas-bg);
              border-color: var(--border-subtle);
            "
          >
            {#if $updateStore.updateAvailable}
              <!-- Update Available Notification Banner -->
              <div class="p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/40 flex items-center justify-between gap-2">
                <div class="space-y-0.5">
                  <div class="flex items-center gap-1.5 font-semibold text-emerald-300 text-xs">
                    <ArrowUpCircle class="w-4 h-4 text-emerald-400 shrink-0" />
                    <span>Version v{$updateStore.latestVersion} is available!</span>
                  </div>
                  <div class="text-[10px] text-neutral-400">
                    {$updateStore.release?.name || 'A new version with enhancements is ready.'}
                  </div>
                </div>
                <button
                  onclick={() => updateStore.openModal()}
                  class="px-2.5 py-1.5 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-semibold text-[11px] flex items-center gap-1.5 shadow transition-colors shrink-0 cursor-pointer"
                >
                  <Download class="w-3.5 h-3.5" />
                  <span>Update</span>
                </button>
              </div>
            {:else if $updateStore.isChecking}
              <div class="flex items-center gap-2 text-xs py-1 text-blue-400">
                <Loader2 class="w-4 h-4 animate-spin text-blue-400" />
                <span>Checking GitHub for newer releases...</span>
              </div>
            {:else}
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-2">
                  <div class="w-2 h-2 rounded-full bg-emerald-500"></div>
                  <span class="text-neutral-300 font-medium text-xs">Pica is up to date</span>
                </div>
                {#if $updateStore.lastChecked}
                  <span class="text-[10px]" style="color: var(--text-dim);">
                    Checked {formatRelativeDate(new Date($updateStore.lastChecked).toISOString())}
                  </span>
                {/if}
              </div>
            {/if}

            {#if $updateStore.error}
              <div class="text-[11px] text-red-400">
                ❌ {$updateStore.error}
              </div>
            {/if}

            <div class="flex items-center justify-between pt-1">
              <label class="flex items-center gap-2 cursor-pointer text-[11px] text-neutral-400 hover:text-neutral-300 select-none">
                <input
                  type="checkbox"
                  checked={$updateStore.autoCheckEnabled}
                  onchange={(e) => updateStore.setAutoCheck((e.currentTarget as HTMLInputElement).checked)}
                  class="rounded border-neutral-700 text-blue-600 focus:ring-0 cursor-pointer"
                />
                <span>Check for updates automatically on startup</span>
              </label>

              <button
                onclick={() => updateStore.checkForUpdates(true)}
                disabled={$updateStore.isChecking}
                class="px-2.5 py-1 rounded border text-[11px] font-medium transition-colors hover:bg-white/10 disabled:opacity-50 cursor-pointer flex items-center gap-1.5"
                style="border-color: var(--border-subtle); color: var(--text-main);"
              >
                {#if $updateStore.isChecking}
                  <Loader2 class="w-3 h-3 animate-spin" />
                  <span>Checking...</span>
                {:else}
                  <RefreshCw class="w-3 h-3" />
                  <span>Check Now</span>
                {/if}
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer Buttons -->
      <div
        class="px-5 py-3 border-t flex items-center justify-between gap-2"
        style="border-color: var(--border-subtle); background-color: var(--header-bg);"
      >
        <span class="text-[10px]" style="color: var(--text-dim);">Pica v{$updateStore.currentVersion}</span>
        <div class="flex items-center gap-2">
          <button
            onclick={() => (isOpen = false)}
            class="px-3 py-1.5 rounded-md border text-xs font-medium hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
            style="border-color: var(--border-subtle);"
          >
            Cancel
          </button>

          <button
            onclick={handleSave}
            disabled={isLoading}
            class="flex items-center gap-1.5 px-4 py-1.5 rounded-md bg-blue-600 hover:bg-blue-500 text-white font-semibold text-xs shadow transition-colors disabled:opacity-50"
          >
            {#if isSaved}
              <Check class="w-3.5 h-3.5" />
              <span>Saved!</span>
            {:else}
              <Save class="w-3.5 h-3.5" />
              <span>Save Preferences</span>
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
