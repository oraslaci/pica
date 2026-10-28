<script lang="ts">
  import { onMount } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import {
    Folder as FolderIcon,
    FolderPlus,
    Eye,
    Zap,
    Trash2,
    X,
    Check,
    RefreshCw,
    HardDrive,
    Users,
    EyeOff,
  } from '@lucide/svelte';
  import {
    getWatchFolders,
    addWatchFolder,
    updateWatchFolderMode,
    updateFolderFaceDetect,
    removeWatchFolder,
    triggerScan,
    triggerFaceScan,
    isTauri,
  } from '../api';
  import type { Folder } from '../types';

  let { isOpen = $bindable(false), onFoldersUpdated = () => {} }: {
    isOpen: boolean;
    onFoldersUpdated?: () => void;
  } = $props();

  let folders = $state<Folder[]>([]);
  let selectedFolder = $state<Folder | null>(null);
  let customPathInput = $state('');
  let isAddingCustom = $state(false);
  let isLoading = $state(false);
  let isScanning = $state(false);
  let confirmDialog = $state<{
    title: string;
    message: string;
    dangerText: string;
    onConfirm: () => Promise<void> | void;
  } | null>(null);

  async function loadFolders() {
    isLoading = true;
    try {
      folders = await getWatchFolders();
      if (folders.length > 0 && !selectedFolder) {
        selectedFolder = folders[0];
      } else if (selectedFolder) {
        selectedFolder = folders.find((f) => f.id === selectedFolder?.id) || folders[0] || null;
      }
    } catch (err) {
      console.error('Failed to load watch folders:', err);
    } finally {
      isLoading = false;
    }
  }

  $effect(() => {
    if (isOpen) {
      loadFolders();
    }
  });

  async function handleAddFolderDialog() {
    if (isTauri) {
      try {
        const selected = await open({
          directory: true,
          multiple: false,
          title: 'Select Photo Folder to Watch in Pica',
        });
        if (selected && typeof selected === 'string') {
          await addWatchFolder(selected, 'always');
          await loadFolders();
          onFoldersUpdated();
        }
      } catch (err) {
        console.error('Directory picker error:', err);
      }
    } else {
      isAddingCustom = true;
    }
  }

  async function handleAddCustomPath() {
    if (!customPathInput.trim()) return;
    try {
      await addWatchFolder(customPathInput.trim(), 'always');
      customPathInput = '';
      isAddingCustom = false;
      await loadFolders();
      onFoldersUpdated();
    } catch (err) {
      console.error('Add path error:', err);
    }
  }

  async function handleUpdateMode(mode: 'always' | 'once' | 'exclude') {
    if (!selectedFolder) return;
    if (mode === 'exclude') {
      const folderToRemove = selectedFolder;
      confirmDialog = {
        title: 'Remove Folder from Pica?',
        message: `Remove "${folderToRemove.display_name}" from your Pica library?\n\nThis will remove the folder, its indexed photos, and all detected faces from Pica storage. Your original files on disk remain 100% untouched.`,
        dangerText: 'Remove from Pica',
        onConfirm: async () => {
          try {
            await removeWatchFolder(folderToRemove.id);
            selectedFolder = null;
            await loadFolders();
            onFoldersUpdated();
          } catch (err) {
            console.error('Failed to remove watch folder:', err);
          } finally {
            confirmDialog = null;
          }
        },
      };
      return;
    }

    await updateWatchFolderMode(selectedFolder.id, mode);
    selectedFolder.scan_mode = mode;
    await loadFolders();
    onFoldersUpdated();
  }

  async function handleEnableFaceDetect() {
    if (!selectedFolder) return;
    if (selectedFolder.face_detect_enabled) return;
    await applyEnableFaceDetect(selectedFolder);
  }

  function handleDisableFaceDetect() {
    if (!selectedFolder) return;
    if (!selectedFolder.face_detect_enabled) return;
    const targetFolder = selectedFolder;
    confirmDialog = {
      title: 'Disable Face Detection?',
      message: `Disable face detection for "${targetFolder.display_name}"?\n\nAll detected faces and person groupings for photos in this folder will be deleted from storage. Your original photos on disk remain untouched.`,
      dangerText: 'Disable & Remove Faces',
      onConfirm: async () => {
        try {
          await updateFolderFaceDetect(targetFolder.id, false);
          targetFolder.face_detect_enabled = false;
          await loadFolders();
          onFoldersUpdated();
        } catch (e) {
          console.error('Failed to disable folder face detect:', e);
        } finally {
          confirmDialog = null;
        }
      },
    };
  }

  function handleToggleFaceDetect(enabled: boolean) {
    if (enabled) {
      handleEnableFaceDetect();
    } else {
      handleDisableFaceDetect();
    }
  }

  async function applyEnableFaceDetect(folder: Folder) {
    try {
      folder.face_detect_enabled = true;
      await updateFolderFaceDetect(folder.id, true);
      await triggerFaceScan();
      await loadFolders();
      onFoldersUpdated();
    } catch (e) {
      console.error('Failed to enable folder face detect:', e);
    }
  }

  async function handleScanNow() {
    isScanning = true;
    try {
      await triggerScan();
      onFoldersUpdated();
    } finally {
      setTimeout(() => {
        isScanning = false;
      }, 1200);
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-xs p-4">
    <div
      class="w-full max-w-3xl bg-[#232323] border border-[#3c3c3c] rounded-lg shadow-2xl overflow-hidden flex flex-col text-[#e2e8f0] max-h-[90vh]"
    >
      <!-- Title Bar -->
      <div class="px-5 py-3.5 bg-[#2b2b2b] border-b border-[#383838] flex items-center justify-between">
        <div class="flex items-center gap-2.5">
          <FolderIcon class="w-5 h-5 text-blue-400" />
          <h2 class="text-base font-semibold tracking-wide text-white">Folder Manager</h2>
        </div>
        <button
          onclick={() => (isOpen = false)}
          class="p-1 rounded-sm text-neutral-400 hover:text-white hover:bg-[#383838] transition-colors"
          title="Close (Esc)"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Main Two-Column Picasa Layout -->
      <div class="grid grid-cols-5 h-[520px] max-h-[calc(90vh-110px)] overflow-hidden">
        <!-- Left Column: Folders List -->
        <div class="col-span-2 border-r border-[#383838] bg-[#1d1d1d] flex flex-col">
          <div class="p-3 border-b border-[#303030] flex items-center justify-between">
            <span class="text-xs font-semibold uppercase text-neutral-400 tracking-wider">Watched Folders</span>
            <button
              onclick={handleAddFolderDialog}
              class="flex items-center gap-1 text-xs text-blue-400 hover:text-blue-300 transition-colors"
            >
              <FolderPlus class="w-3.5 h-3.5" />
              <span>Add Folder</span>
            </button>
          </div>

          {#if isAddingCustom}
            <div class="p-2 border-b border-[#333] bg-[#222]">
              <input
                type="text"
                bind:value={customPathInput}
                placeholder="/home/username/Pictures"
                class="w-full bg-[#181818] border border-[#444] rounded px-2 py-1 text-xs text-white placeholder-neutral-500 focus:outline-none focus:border-blue-500"
                onkeydown={(e) => e.key === 'Enter' && handleAddCustomPath()}
              />
              <div class="flex justify-end gap-1 mt-1.5">
                <button
                  onclick={() => (isAddingCustom = false)}
                  class="px-2 py-0.5 text-xs text-neutral-400 hover:text-white"
                >
                  Cancel
                </button>
                <button
                  onclick={handleAddCustomPath}
                  class="px-2 py-0.5 text-xs bg-blue-600 hover:bg-blue-500 text-white rounded"
                >
                  Add
                </button>
              </div>
            </div>
          {/if}

          <div class="flex-1 overflow-y-auto p-1.5 space-y-1">
            {#if folders.length === 0}
              <div class="p-6 text-center text-xs text-neutral-500">
                <HardDrive class="w-8 h-8 mx-auto mb-2 opacity-40" />
                No folders configured.<br />Click "+ Add Folder" to scan your drives.
              </div>
            {:else}
              {#each folders as folder}
                <button
                  type="button"
                  onclick={() => (selectedFolder = folder)}
                  class={`w-full text-left px-3 py-2 rounded flex items-center gap-2.5 transition-colors ${
                    selectedFolder?.id === folder.id
                      ? 'bg-blue-600/30 border border-blue-500/50 text-white'
                      : 'hover:bg-[#282828] text-neutral-300'
                  }`}
                >
                  <FolderIcon class={`w-4 h-4 shrink-0 ${folder.scan_mode === 'always' ? 'text-blue-400' : 'text-amber-400'}`} />
                  <div class="min-w-0 flex-1">
                    <div class="text-xs font-medium truncate flex items-center gap-1.5">
                      <span class="truncate">{folder.display_name}</span>
                      {#if folder.face_detect_enabled}
                        <span title="Face detection active for this folder" class="shrink-0 text-purple-400">
                          <Users class="w-3 h-3" />
                        </span>
                      {/if}
                    </div>
                    <div class="text-[10px] text-neutral-400 truncate">{folder.path}</div>
                  </div>
                  <span class="text-[10px] bg-black/40 px-1.5 py-0.5 rounded text-neutral-400">
                    {folder.photo_count}
                  </span>
                </button>
              {/each}
            {/if}
          </div>
        </div>

        <!-- Right Column: Picasa Scan Action Selector -->
        <div class="col-span-3 p-5 flex flex-col overflow-y-auto bg-[#222222]">
          {#if selectedFolder}
            <div>
              <div class="mb-4">
                <h3 class="text-sm font-semibold text-white truncate">{selectedFolder.display_name}</h3>
                <p class="text-xs text-neutral-400 truncate mt-0.5">{selectedFolder.path}</p>
                <p class="text-xs text-neutral-500 mt-1">Photos indexed: {selectedFolder.photo_count}</p>
              </div>

              <div class="text-xs font-semibold text-neutral-400 mb-2 uppercase tracking-wide">
                Folder Configuration
              </div>

              <!-- Picasa Action Buttons -->
              <div class="space-y-2.5">
                <!-- Scan Always -->
                <button
                  type="button"
                  onclick={() => handleUpdateMode('always')}
                  class={`w-full text-left p-3 rounded-md border flex items-start gap-3 transition-colors ${
                    selectedFolder.scan_mode === 'always'
                      ? 'bg-blue-600/20 border-blue-500 text-white'
                      : 'bg-[#1e1e1e] border-[#363636] hover:border-neutral-500 text-neutral-300'
                  }`}
                >
                  <div class="p-1.5 rounded bg-blue-500/20 text-blue-400 shrink-0 mt-0.5">
                    <Eye class="w-4 h-4" />
                  </div>
                  <div>
                    <div class="text-xs font-semibold flex items-center gap-1.5">
                      Scan Always
                      {#if selectedFolder.scan_mode === 'always'}
                        <Check class="w-3.5 h-3.5 text-blue-400" />
                      {/if}
                    </div>
                    <div class="text-[11px] text-neutral-400 mt-0.5 leading-snug">
                      Continuously watch this directory and automatically index new or modified photos.
                    </div>
                  </div>
                </button>

                <!-- Scan Once -->
                <button
                  type="button"
                  onclick={() => handleUpdateMode('once')}
                  class={`w-full text-left p-3 rounded-md border flex items-start gap-3 transition-colors ${
                    selectedFolder.scan_mode === 'once'
                      ? 'bg-amber-600/20 border-amber-500 text-white'
                      : 'bg-[#1e1e1e] border-[#363636] hover:border-neutral-500 text-neutral-300'
                  }`}
                >
                  <div class="p-1.5 rounded bg-amber-500/20 text-amber-400 shrink-0 mt-0.5">
                    <Zap class="w-4 h-4" />
                  </div>
                  <div>
                    <div class="text-xs font-semibold flex items-center gap-1.5">
                      Scan Once
                      {#if selectedFolder.scan_mode === 'once'}
                        <Check class="w-3.5 h-3.5 text-amber-400" />
                      {/if}
                    </div>
                    <div class="text-[11px] text-neutral-400 mt-0.5 leading-snug">
                      Index photos in this folder now, but do not watch for future modifications.
                    </div>
                  </div>
                </button>

                <!-- Remove from Pica -->
                <button
                  type="button"
                  onclick={() => handleUpdateMode('exclude')}
                  class="w-full text-left p-3 rounded-md border border-[#363636] bg-[#1e1e1e] hover:border-rose-700/60 hover:bg-rose-950/20 text-neutral-300 transition-colors flex items-start gap-3"
                >
                  <div class="p-1.5 rounded bg-rose-500/20 text-rose-400 shrink-0 mt-0.5">
                    <Trash2 class="w-4 h-4" />
                  </div>
                  <div>
                    <div class="text-xs font-semibold text-rose-400">Remove from Pica</div>
                    <div class="text-[11px] text-neutral-400 mt-0.5 leading-snug">
                      Remove from library. Your original files on disk remain 100% untouched.
                    </div>
                  </div>
                </button>
              </div>

              <!-- Per-Folder Face Detection & Grouping Setting -->
              <div class="mt-4 pt-4 border-t border-[#363636]">
                <div class="text-xs font-semibold text-neutral-400 mb-2 uppercase tracking-wide flex items-center justify-between">
                  <span>Face Detection</span>
                  {#if selectedFolder.face_detect_enabled}
                    <span class="text-[10px] text-purple-400 font-semibold bg-purple-500/15 px-2 py-0.5 rounded border border-purple-500/30 flex items-center gap-1">
                      <Users class="w-3 h-3" />
                      Active
                    </span>
                  {:else}
                    <span class="text-[10px] text-neutral-400 font-medium bg-neutral-800 px-2 py-0.5 rounded border border-neutral-700">
                      Disabled
                    </span>
                  {/if}
                </div>

                <div class="space-y-2">
                  <!-- Enable Face Detection Button -->
                  <button
                    type="button"
                    onclick={handleEnableFaceDetect}
                    class={`w-full text-left p-3 rounded-md border flex items-start gap-3 transition-colors cursor-pointer ${
                      selectedFolder.face_detect_enabled
                        ? 'bg-purple-600/20 border-purple-500 text-white'
                        : 'bg-[#1e1e1e] border-[#363636] hover:border-neutral-500 text-neutral-300'
                    }`}
                  >
                    <div class="p-1.5 rounded bg-purple-500/20 text-purple-400 shrink-0 mt-0.5">
                      <Users class="w-4 h-4" />
                    </div>
                    <div class="flex-1 min-w-0">
                      <div class="text-xs font-semibold flex items-center gap-1.5">
                        Enable Face Detection
                        {#if selectedFolder.face_detect_enabled}
                          <Check class="w-3.5 h-3.5 text-purple-400" />
                        {/if}
                      </div>
                      <div class="text-[11px] text-neutral-400 mt-0.5 leading-snug">
                        Detect faces and automatically group photos from this folder into People collections.
                      </div>
                    </div>
                  </button>

                  <!-- Disable Face Detection Button -->
                  <button
                    type="button"
                    onclick={handleDisableFaceDetect}
                    class={`w-full text-left p-3 rounded-md border flex items-start gap-3 transition-colors cursor-pointer ${
                      !selectedFolder.face_detect_enabled
                        ? 'bg-neutral-800/90 border-neutral-500 text-white'
                        : 'bg-[#1e1e1e] border-[#363636] hover:border-rose-700/50 hover:bg-rose-950/10 text-neutral-300'
                    }`}
                  >
                    <div class="p-1.5 rounded bg-neutral-700/50 text-neutral-400 shrink-0 mt-0.5">
                      <EyeOff class="w-4 h-4" />
                    </div>
                    <div class="flex-1 min-w-0">
                      <div class="text-xs font-semibold flex items-center gap-1.5">
                        Disable Face Detection
                        {#if !selectedFolder.face_detect_enabled}
                          <Check class="w-3.5 h-3.5 text-neutral-300" />
                        {/if}
                      </div>
                      <div class="text-[11px] text-neutral-400 mt-0.5 leading-snug">
                        Skip face recognition for this folder. Disabling removes detected faces from storage (original photos remain untouched).
                      </div>
                    </div>
                  </button>
                </div>
              </div>
            </div>
          {:else}
            <div class="h-full flex flex-col items-center justify-center text-neutral-500 text-xs text-center p-6">
              <FolderIcon class="w-10 h-10 mb-2 opacity-30" />
              Select a watched folder on the left or add a new folder to configure scanning.
            </div>
          {/if}
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="px-5 py-3 bg-[#1e1e1e] border-t border-[#383838] flex items-center justify-between">
        <button
          onclick={handleScanNow}
          disabled={isScanning || folders.length === 0}
          class="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs font-medium bg-[#2e2e2e] hover:bg-[#3a3a3a] border border-[#444] text-neutral-200 transition-colors disabled:opacity-40"
        >
          <RefreshCw class={`w-3.5 h-3.5 ${isScanning ? 'animate-spin text-blue-400' : ''}`} />
          <span>{isScanning ? 'Scanning Folders...' : 'Scan All Folders Now'}</span>
        </button>

        <button
          onclick={() => (isOpen = false)}
          class="px-4 py-1.5 rounded text-xs font-medium bg-blue-600 hover:bg-blue-500 text-white transition-colors"
        >
          Done
        </button>
      </div>
    </div>

    <!-- Confirmation Dialog Modal -->
    {#if confirmDialog}
      <div class="fixed inset-0 z-60 flex items-center justify-center bg-black/75 backdrop-blur-xs p-4 animate-in fade-in duration-150">
        <div class="w-full max-w-md bg-[#252525] border border-[#444] rounded-lg shadow-2xl p-5 text-[#e2e8f0]">
          <div class="flex items-center gap-2.5 mb-3">
            <div class="p-2 rounded-full bg-rose-500/20 text-rose-400 shrink-0">
              <Trash2 class="w-4 h-4" />
            </div>
            <h3 class="text-sm font-semibold text-white">{confirmDialog.title}</h3>
          </div>
          <p class="text-xs text-neutral-300 leading-relaxed whitespace-pre-line mb-5">
            {confirmDialog.message}
          </p>
          <div class="flex items-center justify-end gap-2.5">
            <button
              type="button"
              onclick={() => (confirmDialog = null)}
              class="px-3 py-1.5 rounded text-xs font-medium border border-[#444] hover:bg-[#333] text-neutral-300 transition-colors cursor-pointer"
            >
              Cancel
            </button>
            <button
              type="button"
              onclick={confirmDialog.onConfirm}
              class="px-3.5 py-1.5 rounded text-xs font-medium bg-rose-600 hover:bg-rose-500 text-white transition-colors cursor-pointer"
            >
              {confirmDialog.dangerText}
            </button>
          </div>
        </div>
      </div>
    {/if}
  </div>
{/if}
