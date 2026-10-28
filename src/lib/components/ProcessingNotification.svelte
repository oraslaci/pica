<script lang="ts">
  import {
    RefreshCw,
    CheckCircle2,
    X,
    ChevronDown,
    ChevronUp,
    Folder as FolderIcon,
    Image as ImageIcon,
    Users,
    Sparkles,
  } from '@lucide/svelte';
  import type { ScanProgress, FaceScanProgress } from '../types';

  interface Props {
    scanProgress: ScanProgress | null;
    faceProgress?: FaceScanProgress | null;
    isLibraryScanning?: boolean;
    faceScanPending?: number;
    manuallyOpened?: boolean;
    onClose?: () => void;
  }

  let {
    scanProgress = null,
    faceProgress = null,
    isLibraryScanning = false,
    faceScanPending = 0,
    manuallyOpened = false,
    onClose = () => {},
  }: Props = $props();

  let isMinimized = $state(false);
  let isDismissed = $state(false);
  let completionTimer: ReturnType<typeof setTimeout> | null = null;
  let completionInfo = $state<{
    title: string;
    detail: string;
    time: number;
  } | null>(null);

  // Track previous processing states to detect completion transitions
  let wasScanActive = $state(false);
  let wasFaceActive = $state(false);

  // Active status calculations
  let isScanActive = $derived(
    isLibraryScanning || (scanProgress !== null && !scanProgress.is_complete)
  );

  let isFaceActive = $derived(
    (faceProgress !== null && !faceProgress.is_complete && faceProgress.pending > 0) ||
      (!isScanActive && faceScanPending > 0)
  );

  let isAnyActive = $derived(isScanActive || isFaceActive);

  // Reset dismissal whenever new processing begins or manual open is requested
  $effect(() => {
    if (manuallyOpened) {
      isDismissed = false;
    }
  });

  $effect(() => {
    if (isAnyActive) {
      isDismissed = false;
      if (completionTimer) {
        clearTimeout(completionTimer);
        completionTimer = null;
        completionInfo = null;
      }
    }
  });

  // Watch for scan completion
  $effect(() => {
    if (wasScanActive && !isScanActive) {
      const total = scanProgress?.scanned_photos ?? scanProgress?.total_found ?? 0;
      completionInfo = {
        title: 'Library scan complete',
        detail: total > 0 ? `${total.toLocaleString()} photos indexed` : 'All folders up to date',
        time: Date.now(),
      };
      if (completionTimer) clearTimeout(completionTimer);
      completionTimer = setTimeout(() => {
        completionInfo = null;
      }, 4000);
    }
    wasScanActive = isScanActive;
  });

  // Watch for face processing completion
  $effect(() => {
    if (wasFaceActive && !isFaceActive && !isScanActive) {
      const facesFound = faceProgress?.faces_found ?? 0;
      completionInfo = {
        title: 'Face detection complete',
        detail: facesFound > 0 ? `${facesFound} faces grouped` : 'All photos scanned for faces',
        time: Date.now(),
      };
      if (completionTimer) clearTimeout(completionTimer);
      completionTimer = setTimeout(() => {
        completionInfo = null;
      }, 4000);
    }
    wasFaceActive = isFaceActive;
  });

  // Derived progress values
  let scanPercent = $derived.by(() => {
    if (!scanProgress || scanProgress.total_found === 0) return 0;
    return Math.min(100, Math.round((scanProgress.scanned_photos / scanProgress.total_found) * 100));
  });

  let shouldShow = $derived(
    !isDismissed && (isAnyActive || completionInfo !== null)
  );

  function handleDismiss() {
    isDismissed = true;
    if (completionTimer) {
      clearTimeout(completionTimer);
      completionTimer = null;
      completionInfo = null;
    }
    onClose();
  }
</script>

{#if shouldShow}
  <div
    class="fixed bottom-14 right-5 z-40 transition-all duration-300 ease-out select-none"
    style="font-family: inherit;"
  >
    {#if isMinimized}
      <!-- Minimized Compact Pill Badge -->
      <div
        role="button"
        tabindex="0"
        class="flex items-center gap-2.5 px-3 py-1.5 rounded-full shadow-lg border backdrop-blur-md transition-all text-xs font-medium cursor-pointer"
        style="
          background-color: var(--hud-bg);
          border-color: var(--border-subtle);
          color: var(--text-main);
          box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
        "
        onclick={() => (isMinimized = false)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            isMinimized = false;
          }
        }}
        title="Click to expand processing details"
      >
        {#if completionInfo}
          <CheckCircle2 class="w-3.5 h-3.5 text-emerald-400 shrink-0" />
          <span class="truncate max-w-[160px] text-emerald-400">{completionInfo.title}</span>
        {:else if isScanActive}
          <RefreshCw class="w-3.5 h-3.5 text-blue-400 animate-spin shrink-0" />
          <span class="truncate max-w-[160px]">
            Indexing {scanPercent > 0 ? `${scanPercent}%` : '...'}
          </span>
        {:else if isFaceActive}
          <Users class="w-3.5 h-3.5 text-purple-400 animate-pulse shrink-0" />
          <span class="truncate max-w-[160px]">
            Faces ({faceScanPending || faceProgress?.pending || 0} left)
          </span>
        {/if}

        <button
          type="button"
          class="p-0.5 rounded hover:bg-white/10 text-white/50 hover:text-white transition-colors"
          onclick={(e) => {
            e.stopPropagation();
            isMinimized = false;
          }}
          title="Expand"
        >
          <ChevronUp class="w-3.5 h-3.5" />
        </button>

        <button
          type="button"
          class="p-0.5 rounded hover:bg-white/10 text-white/50 hover:text-white transition-colors"
          onclick={(e) => {
            e.stopPropagation();
            handleDismiss();
          }}
          title="Dismiss"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      </div>
    {:else}
      <!-- Expanded Detailed Floating Card -->
      <div
        class="w-80 rounded-xl shadow-2xl border backdrop-blur-md p-3.5 flex flex-col gap-2.5 transition-all text-xs"
        style="
          background-color: var(--hud-bg);
          border-color: var(--border-subtle);
          color: var(--text-main);
          box-shadow: 0 8px 32px rgba(0, 0, 0, 0.35);
        "
      >
        <!-- Header Row -->
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2 font-medium">
            {#if completionInfo}
              <div class="w-5 h-5 rounded-full bg-emerald-500/20 text-emerald-400 flex items-center justify-center shrink-0">
                <CheckCircle2 class="w-3.5 h-3.5" />
              </div>
              <span class="text-emerald-400 font-semibold">{completionInfo.title}</span>
            {:else if isScanActive}
              <div class="w-5 h-5 rounded-full bg-blue-500/20 text-blue-400 flex items-center justify-center shrink-0">
                <RefreshCw class="w-3.5 h-3.5 animate-spin" />
              </div>
              <span class="font-semibold">Indexing Photos</span>
            {:else if isFaceActive}
              <div class="w-5 h-5 rounded-full bg-purple-500/20 text-purple-400 flex items-center justify-center shrink-0">
                <Users class="w-3.5 h-3.5 animate-pulse" />
              </div>
              <span class="font-semibold text-purple-300">Detecting Faces</span>
            {/if}
          </div>

          <!-- Window Controls -->
          <div class="flex items-center gap-1">
            <button
              type="button"
              class="p-1 rounded hover:bg-white/10 text-white/50 hover:text-white transition-colors"
              onclick={() => (isMinimized = true)}
              title="Minimize to pill"
            >
              <ChevronDown class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              class="p-1 rounded hover:bg-white/10 text-white/50 hover:text-white transition-colors"
              onclick={handleDismiss}
              title="Close notification"
            >
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {#if completionInfo}
          <!-- Completion Message -->
          <div class="text-[11px] leading-relaxed" style="color: var(--text-muted);">
            {completionInfo.detail}
          </div>
        {:else if isScanActive}
          <!-- Folder Scanning Details -->
          <div class="flex flex-col gap-1.5">
            <!-- Current Folder & Current File -->
            <div class="flex items-center gap-1.5 text-[11px] truncate" style="color: var(--text-muted);">
              <FolderIcon class="w-3 h-3 text-blue-400 shrink-0" />
              <span class="truncate font-medium" style="color: var(--text-main);">
                {scanProgress?.current_folder || 'Library Folders'}
              </span>
            </div>

            {#if scanProgress?.current_file}
              <div class="flex items-center gap-1.5 text-[11px] truncate pl-4" style="color: var(--text-dim);">
                <ImageIcon class="w-2.5 h-2.5 shrink-0 opacity-70" />
                <span class="truncate">{scanProgress.current_file}</span>
              </div>
            {/if}

            <!-- Progress Bar -->
            <div class="w-full h-1.5 rounded-full bg-black/20 dark:bg-white/10 overflow-hidden mt-1">
              <div
                class="h-full bg-blue-500 rounded-full transition-all duration-200 ease-out"
                style={`width: ${scanPercent > 0 ? scanPercent : 20}%;`}
              ></div>
            </div>

            <!-- Stats & Numbers -->
            <div class="flex items-center justify-between text-[11px] mt-0.5" style="color: var(--text-muted);">
              <span>
                {#if scanProgress && scanProgress.total_found > 0}
                  {scanProgress.scanned_photos.toLocaleString()} of {scanProgress.total_found.toLocaleString()} photos
                {:else}
                  Scanning drive...
                {/if}
              </span>
              <span class="font-medium text-blue-400">
                {scanPercent > 0 ? `${scanPercent}%` : ''}
              </span>
            </div>
          </div>
        {:else if isFaceActive}
          <!-- Face Detection Details -->
          <div class="flex flex-col gap-1.5">
            {#if faceProgress?.current_photo}
              <div class="flex items-center gap-1.5 text-[11px] truncate" style="color: var(--text-muted);">
                <ImageIcon class="w-3 h-3 text-purple-400 shrink-0" />
                <span class="truncate">{faceProgress.current_photo}</span>
              </div>
            {:else}
              <div class="text-[11px] truncate" style="color: var(--text-muted);">
                Scanning photos at low CPU priority...
              </div>
            {/if}

            <!-- Animated Subtle Pulse Track -->
            <div class="w-full h-1.5 rounded-full bg-black/20 dark:bg-white/10 overflow-hidden mt-1">
              <div
                class="h-full bg-gradient-to-r from-purple-500 to-indigo-500 rounded-full animate-pulse"
                style="width: 100%;"
              ></div>
            </div>

            <!-- Stats & Numbers -->
            <div class="flex items-center justify-between text-[11px] mt-0.5" style="color: var(--text-muted);">
              <span class="flex items-center gap-1 text-purple-300">
                <Sparkles class="w-3 h-3" />
                <span>{(faceScanPending || faceProgress?.pending || 0)} photos remaining</span>
              </span>
              {#if (faceProgress?.faces_found ?? 0) > 0}
                <span class="text-purple-400 font-medium">
                  {faceProgress?.faces_found} found
                </span>
              {/if}
            </div>
          </div>
        {/if}
      </div>
    {/if}
  </div>
{/if}
