<script lang="ts">
  import { onMount } from 'svelte';
  import { X, Shield, FolderCheck, EyeOff, Save, Check } from '@lucide/svelte';
  import { getAppSettings, updateAppSettings } from '../api';
  import type { AppSettings } from '../types';

  let {
    isOpen = $bindable(false),
    onSettingsChanged = () => {},
  }: {
    isOpen: boolean;
    onSettingsChanged?: () => void;
  } = $props();

  let settings = $state<AppSettings>({
    original_handling: 'backup',
    hide_originals: true,
  });

  let isLoading = $state(false);
  let isSaved = $state(false);

  async function loadSettings() {
    isLoading = true;
    try {
      settings = await getAppSettings();
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
    return () => window.removeEventListener('keydown', handleKeyDown);
  });
</script>

{#if isOpen}
  <!-- Backdrop -->
  <div
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
      <div class="p-5 space-y-5 text-xs">
        <!-- Section: Original Image Policy -->
        <div>
          <label class="block text-[11px] font-bold uppercase tracking-wider mb-2" style="color: var(--text-muted);">
            Original Photo Handling
          </label>
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
          <label class="block text-[11px] font-bold uppercase tracking-wider mb-2" style="color: var(--text-muted);">
            Library Display
          </label>

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
      </div>

      <!-- Footer Buttons -->
      <div
        class="px-5 py-3 border-t flex items-center justify-end gap-2"
        style="border-color: var(--border-subtle); background-color: var(--header-bg);"
      >
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
{/if}
