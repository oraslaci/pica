<script lang="ts">
  import { onMount } from 'svelte';
  import {
    X,
    Download,
    ExternalLink,
    Sparkles,
    ChevronDown,
    ChevronUp,
    Check,
    Clock,
    FileCode,
    Laptop,
    RotateCw,
    Loader2,
    Zap,
  } from '@lucide/svelte';
  import {
    updateStore,
    formatBytes,
    formatRelativeDate,
    detectUserPlatform,
  } from '../updateStore';
  import type { ReleaseAsset } from '../types';

  let showAllAssets = $state(false);
  let isManualDownloading = $state(false);
  let downloadedAsset = $state<string | null>(null);

  const detectedPlatform = detectUserPlatform();

  function handleClose() {
    updateStore.closeModal();
    showAllAssets = false;
  }

  function handleDismiss() {
    if ($updateStore.release?.version) {
      updateStore.dismissUpdate($updateStore.release.version);
    } else {
      updateStore.closeModal();
    }
  }

  async function handleDirectDownload(asset?: ReleaseAsset | null) {
    const url = asset?.browser_download_url || $updateStore.release?.html_url;
    if (!url) return;

    isManualDownloading = true;
    downloadedAsset = asset?.name || 'installer';
    try {
      await updateStore.downloadUpdate(url);
    } finally {
      setTimeout(() => {
        isManualDownloading = false;
      }, 2500);
    }
  }

  async function handleStartSelfUpdate() {
    await updateStore.startSelfUpdate();
  }

  async function handleRestart() {
    await updateStore.restartApplication();
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (!$updateStore.isModalOpen) return;
    if (e.key === 'Escape') {
      handleClose();
    }
  }

  onMount(() => {
    window.addEventListener('keydown', handleKeyDown);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
    };
  });
</script>

{#if $updateStore.isModalOpen && $updateStore.release}
  <!-- Backdrop -->
  <div
    role="presentation"
    class="fixed inset-0 z-50 bg-black/75 backdrop-blur-xs flex items-center justify-center p-4 animate-in fade-in duration-200 select-none"
    onclick={(e) => {
      if (e.target === e.currentTarget && !$updateStore.isSelfUpdating) handleClose();
    }}
  >
    <!-- Modal Window -->
    <div
      class="w-full max-w-lg rounded-2xl border shadow-2xl flex flex-col overflow-hidden animate-in zoom-in-95 duration-200"
      style="
        background-color: var(--panel-bg, #18181e);
        border-color: var(--border-subtle, #3f3f4c);
        color: var(--text-main, #f3f4f6);
      "
    >
      <!-- Header -->
      <div
        class="px-5 py-4 border-b flex items-center justify-between"
        style="
          border-color: var(--border-subtle, #2e2e38);
          background-color: var(--header-bg, #1c1c23);
        "
      >
        <div class="flex items-center gap-3">
          <!-- Picasa Colorful Shutter Emblem -->
          <div class="relative w-7 h-7 flex items-center justify-center shrink-0">
            <svg viewBox="0 0 100 100" class="w-7 h-7 drop-shadow-xs">
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
            <span class="absolute -top-1 -right-1 flex h-3 w-3">
              <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span class="relative inline-flex rounded-full h-3 w-3 bg-emerald-500"></span>
            </span>
          </div>

          <div>
            <div class="flex items-center gap-2">
              <h2 class="text-sm font-bold tracking-wide uppercase">Software Update</h2>
              <span class="px-2 py-0.5 text-[10px] font-bold rounded-full bg-emerald-500/20 text-emerald-400 border border-emerald-500/30">
                New Version
              </span>
            </div>
            <p class="text-[11px] mt-0.5" style="color: var(--text-dim, #9ca3af);">
              {$updateStore.isReadyToRestart
                ? 'Update is installed and ready to apply.'
                : 'A new release of Pica is ready for your system.'}
            </p>
          </div>
        </div>

        {#if !$updateStore.isSelfUpdating}
          <button
            onclick={handleClose}
            class="p-1.5 rounded-full text-neutral-400 hover:text-white hover:bg-white/10 transition-colors cursor-pointer"
            title="Close (Esc)"
          >
            <X class="w-4 h-4" />
          </button>
        {/if}
      </div>

      <!-- Body Content -->
      <div class="p-5 space-y-4 text-xs max-h-[72vh] overflow-y-auto">
        <!-- Version Highlights Card -->
        <div
          class="p-3.5 rounded-xl border flex items-center justify-between"
          style="
            background-color: var(--canvas-bg, #121214);
            border-color: var(--border-subtle, #2e2e38);
          "
        >
          <div class="space-y-1">
            <div class="text-[11px] font-semibold uppercase tracking-wider text-neutral-400">
              Version Transition
            </div>
            <div class="flex items-center gap-2">
              <span class="font-mono text-xs px-2 py-0.5 rounded bg-white/5 border border-white/10 text-neutral-300">
                v{$updateStore.currentVersion}
              </span>
              <span class="text-neutral-500">→</span>
              <span class="font-mono text-sm font-bold px-2 py-0.5 rounded bg-emerald-500/20 border border-emerald-500/40 text-emerald-300">
                v{$updateStore.latestVersion}
              </span>
            </div>
          </div>

          <div class="text-right space-y-1">
            <div class="text-[10px] text-neutral-500 flex items-center gap-1 justify-end">
              <Clock class="w-3 h-3" />
              <span>{formatRelativeDate($updateStore.release.published_at)}</span>
            </div>
            <div class="text-[11px] font-medium text-neutral-400">
              {$updateStore.release.name || `Pica v${$updateStore.latestVersion}`}
            </div>
          </div>
        </div>

        <!-- Release Notes Section -->
        <div>
          <div class="flex items-center justify-between mb-1.5">
            <span class="text-[11px] font-bold uppercase tracking-wider text-neutral-400">
              Release Notes
            </span>
            <a
              href={$updateStore.release.html_url}
              target="_blank"
              rel="noopener noreferrer"
              class="flex items-center gap-1 text-[11px] text-blue-400 hover:text-blue-300 transition-colors"
            >
              <span>View on GitHub</span>
              <ExternalLink class="w-3 h-3" />
            </a>
          </div>

          <div
            class="p-3.5 rounded-xl border max-h-40 overflow-y-auto font-sans leading-relaxed text-xs whitespace-pre-wrap select-text selection:bg-blue-600 selection:text-white"
            style="
              background-color: var(--canvas-bg, #121214);
              border-color: var(--border-subtle, #2e2e38);
              color: var(--text-main, #d1d5db);
            "
          >
            {#if $updateStore.release.body}
              {$updateStore.release.body}
            {:else}
              <span class="italic text-neutral-500">No release notes provided for this version.</span>
            {/if}
          </div>
        </div>

        <!-- In-Place Self-Updating Status / Actions -->
        {#if $updateStore.isReadyToRestart}
          <!-- Step 3: Ready to restart -->
          <div
            class="p-4 rounded-xl border border-emerald-500/50 bg-emerald-500/10 space-y-3 text-center animate-in zoom-in-95"
          >
            <div class="flex items-center justify-center gap-2 text-emerald-400 font-bold text-sm">
              <Check class="w-5 h-5 text-emerald-400" />
              <span>Update Ready to Apply!</span>
            </div>
            <p class="text-xs text-neutral-300 leading-relaxed">
              Pica v{$updateStore.latestVersion} has been downloaded and installed in place. Restart now to run the new version.
            </p>
            <button
              onclick={handleRestart}
              class="w-full py-2.5 px-4 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white font-bold text-xs shadow-lg flex items-center justify-center gap-2 cursor-pointer transition-all"
            >
              <RotateCw class="w-4 h-4" />
              <span>Restart Pica Now</span>
            </button>
          </div>
        {:else if $updateStore.isSelfUpdating}
          <!-- Step 2: Downloading & installing in progress -->
          <div
            class="p-4 rounded-xl border border-blue-500/40 bg-blue-500/10 space-y-3 animate-in fade-in"
          >
            <div class="flex items-center justify-between text-xs text-blue-300 font-semibold">
              <div class="flex items-center gap-2">
                <Loader2 class="w-4 h-4 animate-spin text-blue-400" />
                <span>{$updateStore.selfUpdateStatusText || 'Downloading update package...'}</span>
              </div>
              <span class="font-mono text-xs font-bold text-blue-200">
                {$updateStore.selfUpdateProgress}%
              </span>
            </div>

            <!-- Progress bar -->
            <div class="w-full bg-black/40 h-2.5 rounded-full overflow-hidden border border-white/5">
              <div
                class="bg-blue-500 h-full transition-all duration-300 rounded-full"
                style="width: {$updateStore.selfUpdateProgress}%"
              ></div>
            </div>
            <p class="text-[11px] text-neutral-400">
              The application will update itself seamlessly without needing manual installation.
            </p>
          </div>
        {:else}
          <!-- Step 1: Update options card -->
          <div
            class="p-4 rounded-xl border space-y-3"
            style="
              background-color: var(--panel-bg, #1c1c23);
              border-color: var(--border-subtle, #2e2e38);
            "
          >
            {#if $updateStore.canSelfUpdate}
              <!-- Primary Option: Seamless In-Place Self-Update -->
              <div class="space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-bold uppercase tracking-wider text-emerald-400 flex items-center gap-1.5">
                    <Zap class="w-3.5 h-3.5" />
                    <span>In-Place Auto-Update (Recommended)</span>
                  </span>
                  <span class="text-[10px] text-neutral-400">Fast & Zero-Fuss</span>
                </div>
                <button
                  onclick={handleStartSelfUpdate}
                  class="w-full flex items-center justify-center gap-2 px-4 py-2.5 rounded-lg
                         bg-emerald-600 hover:bg-emerald-500 text-white font-semibold text-xs shadow-lg
                         transition-all duration-150 cursor-pointer"
                >
                  <RotateCw class="w-4 h-4 text-white" />
                  <span>Update and Restart Automatically</span>
                </button>
              </div>

              <div class="flex items-center gap-2 my-2 text-[10px] text-neutral-500">
                <div class="h-px flex-1 bg-neutral-800"></div>
                <span>OR DOWNLOAD MANUALLY</span>
                <div class="h-px flex-1 bg-neutral-800"></div>
              </div>
            {/if}

            <!-- Secondary / Fallback Option: Direct Package Download -->
            <div class="space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-[11px] font-bold uppercase tracking-wider text-neutral-300 flex items-center gap-1.5">
                  <Laptop class="w-3.5 h-3.5 text-blue-400" />
                  <span>Manual Installer for your OS ({detectedPlatform})</span>
                </span>
                {#if $updateStore.recommendedAsset}
                  <span class="text-[11px] font-mono text-neutral-400">
                    {formatBytes($updateStore.recommendedAsset.size)}
                  </span>
                {/if}
              </div>

              {#if $updateStore.recommendedAsset}
                <button
                  onclick={() => handleDirectDownload($updateStore.recommendedAsset)}
                  disabled={isManualDownloading}
                  class="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg
                         border border-neutral-700 hover:bg-white/10 text-neutral-200 font-medium text-xs
                         transition-all duration-150 cursor-pointer disabled:opacity-50"
                >
                  {#if isManualDownloading}
                    <Check class="w-4 h-4 text-emerald-400 animate-bounce" />
                    <span>Downloading {downloadedAsset}...</span>
                  {:else}
                    <Download class="w-4 h-4 text-blue-400" />
                    <span>Download {$updateStore.recommendedAsset.name}</span>
                  {/if}
                </button>
              {:else}
                <button
                  onclick={() => handleDirectDownload(null)}
                  class="w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg
                         border border-neutral-700 hover:bg-white/10 text-neutral-200 font-medium text-xs
                         transition-all duration-150 cursor-pointer"
                >
                  <Download class="w-4 h-4 text-blue-400" />
                  <span>Download from GitHub Releases</span>
                </button>
              {/if}
            </div>

            <!-- Expandable Alternate Downloads -->
            {#if $updateStore.release.assets && $updateStore.release.assets.length > 0}
              <div class="pt-2 border-t" style="border-color: var(--border-subtle, #2e2e38);">
                <button
                  onclick={() => (showAllAssets = !showAllAssets)}
                  class="flex items-center justify-between w-full text-[11px] text-neutral-400 hover:text-neutral-200 transition-colors cursor-pointer py-1"
                >
                  <span>All packages ({$updateStore.release.assets.length} assets)</span>
                  {#if showAllAssets}
                    <ChevronUp class="w-3.5 h-3.5" />
                  {:else}
                    <ChevronDown class="w-3.5 h-3.5" />
                  {/if}
                </button>

                {#if showAllAssets}
                  <div class="mt-2 space-y-1.5 max-h-40 overflow-y-auto pr-1">
                    {#each $updateStore.release.assets as asset}
                      <div
                        class="flex items-center justify-between p-2 rounded-lg border text-[11px] transition-colors hover:bg-white/5"
                        style="
                          background-color: var(--canvas-bg, #121214);
                          border-color: var(--border-subtle, #2e2e38);
                        "
                      >
                        <div class="flex items-center gap-2 truncate mr-2">
                          <FileCode class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
                          <span class="font-mono truncate text-neutral-300" title={asset.name}>
                            {asset.name}
                          </span>
                        </div>
                        <div class="flex items-center gap-2 shrink-0">
                          <span class="font-mono text-[10px] text-neutral-500">
                            {formatBytes(asset.size)}
                          </span>
                          <button
                            onclick={() => handleDirectDownload(asset)}
                            class="px-2 py-1 rounded bg-white/10 hover:bg-white/20 text-neutral-200 hover:text-white transition-colors cursor-pointer flex items-center gap-1"
                            title={`Download ${asset.name}`}
                          >
                            <Download class="w-3 h-3" />
                            <span>Get</span>
                          </button>
                        </div>
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/if}

        {#if $updateStore.error}
          <div class="p-3 rounded-lg border border-red-500/30 bg-red-500/10 text-red-300 text-xs leading-relaxed">
            {$updateStore.error}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div
        class="px-5 py-3 border-t flex items-center justify-between gap-3 text-xs"
        style="
          border-color: var(--border-subtle, #2e2e38);
          background-color: var(--header-bg, #1c1c23);
        "
      >
        <label class="flex items-center gap-2 cursor-pointer text-[11px] text-neutral-400 hover:text-neutral-300 select-none">
          <input
            type="checkbox"
            checked={$updateStore.autoCheckEnabled}
            onchange={(e) => updateStore.setAutoCheck((e.currentTarget as HTMLInputElement).checked)}
            class="rounded border-neutral-700 text-blue-600 focus:ring-0 cursor-pointer"
          />
          <span>Check for updates automatically</span>
        </label>

        <div class="flex items-center gap-2">
          {#if !$updateStore.isSelfUpdating && !$updateStore.isReadyToRestart}
            <button
              onclick={handleDismiss}
              class="px-3 py-1.5 rounded-md border text-neutral-400 hover:text-neutral-200 hover:bg-white/5 transition-colors cursor-pointer"
              style="border-color: var(--border-subtle, #3f3f4c);"
            >
              Remind Later
            </button>
          {/if}

          {#if $updateStore.isReadyToRestart}
            <button
              onclick={handleRestart}
              class="flex items-center gap-1.5 px-4 py-1.5 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-bold shadow-xs transition-colors cursor-pointer"
            >
              <RotateCw class="w-3.5 h-3.5" />
              <span>Restart Now</span>
            </button>
          {:else if $updateStore.canSelfUpdate && !$updateStore.isSelfUpdating}
            <button
              onclick={handleStartSelfUpdate}
              class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-semibold shadow-xs transition-colors cursor-pointer"
            >
              <Zap class="w-3.5 h-3.5" />
              <span>Update Now</span>
            </button>
          {:else if !$updateStore.isSelfUpdating}
            <button
              onclick={() => handleDirectDownload($updateStore.recommendedAsset)}
              class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-emerald-600 hover:bg-emerald-500 text-white font-semibold shadow-xs transition-colors cursor-pointer"
            >
              <Download class="w-3.5 h-3.5" />
              <span>Download</span>
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
