<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import LibraryGrid from './lib/components/LibraryGrid.svelte';
  import BottomBar from './lib/components/BottomBar.svelte';
  import FolderManagerModal from './lib/components/FolderManagerModal.svelte';
  import SettingsModal from './lib/components/SettingsModal.svelte';
  import ImageViewer from './lib/components/ImageViewer.svelte';
  import ProcessingNotification from './lib/components/ProcessingNotification.svelte';
  import { Edit2, Check, X, ArrowLeft } from '@lucide/svelte';
  import {
    getFoldersWithPhotos,
    getLibraryStats,
    onScanProgress,
    onScanComplete,
    onFaceScanProgress,
    triggerScan,
    setThumbnailPriority,
    getFaceCollections,
    getFaceSettings,
    getPhotosForPerson,
    updatePersonName,
    getFaceCropUrl,
    isTauri,
  } from './lib/api';
  import type {
    FaceCollection,
    FaceScanProgress,
    FaceSettings,
    FolderWithPhotos,
    LibraryStats,
    Photo,
    ScanProgress,
  } from './lib/types';

  let folderGroups = $state<FolderWithPhotos[]>([]);
  let stats = $state<LibraryStats>({
    total_photos: 0,
    total_folders: 0,
    total_favorites: 0,
    is_scanning: false,
  });
  let scanProgress = $state<ScanProgress | null>(null);
  let faceProgress = $state<FaceScanProgress | null>(null);
  let isProcessingManuallyOpened = $state(false);

  // Face Detection & People State
  let faceCollections = $state<FaceCollection[]>([]);
  let faceSettings = $state<FaceSettings | null>(null);
  let activePersonId = $state<number | null>(null);
  let personPhotoIds = $state<Set<number> | null>(null);
  let isEditingBannerName = $state(false);
  let editingBannerPersonName = $state('');

  const activePerson = $derived(
    activePersonId !== null
      ? faceCollections.find((p) => p.id === activePersonId) || null
      : null
  );

  let zoomSize = $state(180);
  let searchQuery = $state('');
  let onlyFavorites = $state(false);
  let theme = $state<'dark' | 'light'>('dark');

  // Sidebar & Navigation
  let isSidebarCollapsed = $state(false);
  let activeFolderId = $state<number | null>(null);

  // Photo Tray & Selection
  let selectedPhotos = $state<Photo[]>([]);
  let isTrayHeld = $state(false);

  // Modals & Viewer
  let isFolderManagerOpen = $state(false);
  let isSettingsOpen = $state(false);
  let isViewerOpen = $state(false);
  let viewerPhotos = $state<Photo[]>([]);
  let viewerIndex = $state(0);

  async function refreshData() {
    try {
      const [groups, st, faces, fSettings] = await Promise.all([
        getFoldersWithPhotos(),
        getLibraryStats(),
        getFaceCollections().catch(() => []),
        getFaceSettings().catch(() => null),
      ]);
      folderGroups = groups;
      stats = st;
      faceCollections = faces;
      faceSettings = fSettings;

      if (activePersonId !== null) {
        try {
          const personPhotos = await getPhotosForPerson(activePersonId);
          personPhotoIds = new Set(personPhotos.map((p) => p.id));
        } catch {
          personPhotoIds = new Set();
        }
      }
    } catch (err) {
      console.error('Failed to load library:', err);
    }
  }

  // Filtered folder groups based on search, starred filter, and active person
  const filteredGroups = $derived.by(() => {
    if (activePersonId !== null && activePerson) {
      const currentPersonPhotoIds = personPhotoIds;
      let photos = folderGroups
        .flatMap((g) => g.photos)
        .filter((p) => (currentPersonPhotoIds ? currentPersonPhotoIds.has(p.id) : false));

      if (onlyFavorites) {
        photos = photos.filter((p) => p.is_favorite);
      }
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase().trim();
        photos = photos.filter((p) => p.file_name.toLowerCase().includes(q));
      }

      // Deduplicate photos by id and sort chronologically (newest first, then filename)
      const seen = new Set<number>();
      const dedupedPhotos: Photo[] = [];
      for (const p of photos) {
        if (!seen.has(p.id)) {
          seen.add(p.id);
          dedupedPhotos.push(p);
        }
      }
      dedupedPhotos.sort((a, b) => {
        const da = a.date_taken || 0;
        const db = b.date_taken || 0;
        if (db !== da) return db - da;
        return a.file_name.localeCompare(b.file_name);
      });

      return [
        {
          folder: {
            id: -activePerson.id,
            path: '',
            display_name: activePerson.name,
            scan_mode: 'always' as const,
            last_scanned: null,
            photo_count: dedupedPhotos.length,
            face_detect_enabled: true,
          },
          photos: dedupedPhotos,
        },
      ];
    }

    return folderGroups
      .map((g) => {
        let photos = g.photos;
        if (onlyFavorites) {
          photos = photos.filter((p) => p.is_favorite);
        }
        if (searchQuery.trim()) {
          const q = searchQuery.toLowerCase().trim();
          photos = photos.filter((p) => p.file_name.toLowerCase().includes(q));
        }
        return {
          ...g,
          photos,
        };
      })
      .filter((g) => g.photos.length > 0);
  });

  const allVisiblePhotos = $derived(filteredGroups.flatMap((g) => g.photos));

  function handlePhotoSelected(photo: Photo, index: number, allPhotos: Photo[]) {
    viewerPhotos = allPhotos;
    viewerIndex = index;
    isViewerOpen = true;
  }

  function handlePlayFolder(photos: Photo[]) {
    if (photos.length === 0) return;
    viewerPhotos = photos;
    viewerIndex = 0;
    isViewerOpen = true;
  }

  function handlePlaySlideshow(photos: Photo[]) {
    if (photos.length === 0) return;
    viewerPhotos = photos;
    viewerIndex = 0;
    isViewerOpen = true;
  }

  function handlePhotoClick(photo: Photo, isMulti: boolean) {
    if (isMulti) {
      const idx = selectedPhotos.findIndex((p) => p.id === photo.id);
      if (idx !== -1) {
        selectedPhotos = selectedPhotos.filter((p) => p.id !== photo.id);
      } else {
        selectedPhotos = [...selectedPhotos, photo];
      }
    } else {
      if (isTrayHeld) {
        const idx = selectedPhotos.findIndex((p) => p.id === photo.id);
        if (idx !== -1) {
          selectedPhotos = selectedPhotos.filter((p) => p.id !== photo.id);
        } else {
          selectedPhotos = [...selectedPhotos, photo];
        }
      } else {
        selectedPhotos = [photo];
      }
    }
  }

  function handleToggleHold() {
    isTrayHeld = !isTrayHeld;
  }

  function handleClearTray() {
    selectedPhotos = [];
    isTrayHeld = false;
  }

  function handleRemoveFromTray(photoId: number) {
    selectedPhotos = selectedPhotos.filter((p) => p.id !== photoId);
  }

  function prioritizeFolderJump(folderId: number) {
    const idx = filteredGroups.findIndex((g) => g.folder.id === folderId);
    if (idx === -1) return;

    const currentGroup = filteredGroups[idx];
    if (currentGroup.photos.length === 0) return;

    // 1. Immediately visible view photos (first ~36 photos of target folder)
    const screenBatch = currentGroup.photos.slice(0, 36);

    // 2. Previous and next 0.x part of screen (lookaround)
    const forwardBatch = currentGroup.photos.slice(36, 96);
    const prevGroup = idx > 0 ? filteredGroups[idx - 1] : null;
    const backwardBatch = prevGroup ? prevGroup.photos.slice(-36).reverse() : [];
    const nextGroup = idx < filteredGroups.length - 1 ? filteredGroups[idx + 1] : null;
    const nextFolderBatch = nextGroup ? nextGroup.photos.slice(0, 36) : [];
    const nearBatch = [...forwardBatch, ...backwardBatch, ...nextFolderBatch];

    const screenHashes = screenBatch.map((p) => p.file_hash);
    const nearHashes: string[] = [];
    const seen = new Set<string>(screenHashes);
    for (const p of nearBatch) {
      if (!seen.has(p.file_hash)) {
        seen.add(p.file_hash);
        nearHashes.push(p.file_hash);
      }
    }

    if (screenHashes.length > 0 || nearHashes.length > 0) {
      setThumbnailPriority(screenHashes, nearHashes);
    }
  }

  function prioritizeTopJump() {
    if (filteredGroups.length === 0) return;
    const firstGroup = filteredGroups[0];
    if (firstGroup.photos.length === 0) return;

    const screenBatch = firstGroup.photos.slice(0, 36);
    const forwardBatch = firstGroup.photos.slice(36, 96);
    const nextGroup = filteredGroups.length > 1 ? filteredGroups[1] : null;
    const nextFolderBatch = nextGroup ? nextGroup.photos.slice(0, 36) : [];
    const nearBatch = [...forwardBatch, ...nextFolderBatch];

    const screenHashes = screenBatch.map((p) => p.file_hash);
    const nearHashes: string[] = [];
    const seen = new Set<string>(screenHashes);
    for (const p of nearBatch) {
      if (!seen.has(p.file_hash)) {
        seen.add(p.file_hash);
        nearHashes.push(p.file_hash);
      }
    }

    if (screenHashes.length > 0 || nearHashes.length > 0) {
      setThumbnailPriority(screenHashes, nearHashes);
    }
  }

  function handleSelectFolder(folderId: number | null) {
    activeFolderId = folderId;
    activePersonId = null;
    personPhotoIds = null;
    if (folderId !== null) {
      prioritizeFolderJump(folderId);
      const el = document.getElementById(`folder-${folderId}`);
      if (el) {
        el.scrollIntoView({ behavior: 'smooth' });
      }
    } else {
      prioritizeTopJump();
      if (filteredGroups.length > 0) {
        const firstFolderId = filteredGroups[0].folder.id;
        const el = document.getElementById(`folder-${firstFolderId}`);
        if (el) {
          el.scrollIntoView({ behavior: 'smooth' });
        }
      }
    }
  }

  async function handleSelectPerson(personId: number | null) {
    if (personId === null || activePersonId === personId) {
      activePersonId = null;
      personPhotoIds = null;
    } else {
      activePersonId = personId;
      activeFolderId = null;
      try {
        const personPhotos = await getPhotosForPerson(personId);
        personPhotoIds = new Set(personPhotos.map((p) => p.id));
      } catch (err) {
        console.error('Failed to load photos for person:', err);
        personPhotoIds = new Set();
      }
    }
  }

  async function handleRenamePerson(personId: number, newName: string) {
    try {
      await updatePersonName(personId, newName);
      faceCollections = await getFaceCollections();
    } catch (err) {
      console.error('Failed to rename person:', err);
    }
  }

  function handleExport(photos: Photo[]) {
    alert(`Exporting ${photos.length} photo(s). (Export pipeline ready)`);
  }

  onMount(() => {
    // Initialize Theme
    const savedTheme = localStorage.getItem('pica_theme') as 'dark' | 'light' | null;
    if (savedTheme === 'light') {
      theme = 'light';
      document.documentElement.classList.add('light');
    } else {
      theme = 'dark';
      document.documentElement.classList.remove('light');
    }

    refreshData();

    // Listen to background scanning events
    const unsubProgress = onScanProgress((prog) => {
      scanProgress = prog;
      stats.is_scanning = !prog.is_complete;
      if (prog.is_complete) {
        refreshData();
      }
    });

    const unsubComplete = onScanComplete(() => {
      stats.is_scanning = false;
      if (scanProgress) {
        scanProgress = { ...scanProgress, is_complete: true };
      }
      refreshData();
    });

    let lastKnownFacesCount = 0;
    const unsubFaceProgress = onFaceScanProgress(async (prog) => {
      faceProgress = prog;
      if (faceSettings) {
        faceSettings.pending_photos = prog.pending;
      }
      if (prog.faces_found > lastKnownFacesCount) {
        lastKnownFacesCount = prog.faces_found;
        try {
          faceCollections = await getFaceCollections();
        } catch {}
      }
      if (prog.is_complete) {
        refreshData();
      }
    });

    // Keyboard shortcuts
    function handleKeyDown(e: KeyboardEvent) {
      if (isViewerOpen || isFolderManagerOpen) return;

      // Ctrl+H / Cmd+H : Toggle Tray Hold
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'h') {
        e.preventDefault();
        if (selectedPhotos.length > 0) {
          handleToggleHold();
        }
      }

      // Ctrl+A / Cmd+A : Select All Photos in Current View
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        selectedPhotos = allVisiblePhotos;
      }

      // Escape: Clear Tray Selection
      if (e.key === 'Escape' && selectedPhotos.length > 0) {
        handleClearTray();
      }
    }

    // Periodic face background processing sync
    const faceInterval = setInterval(async () => {
      try {
        const fs = await getFaceSettings();
        const prevPending = faceSettings?.pending_photos ?? 0;
        faceSettings = fs;
        if (fs.enabled && (fs.pending_photos > 0 || prevPending > 0)) {
          faceCollections = await getFaceCollections();
        }
      } catch {}
    }, 4000);

    // Cross-window event listener from Standalone Viewer: "open-in-pica-editor"
    let unlistenOpenEditor: (() => void) | null = null;
    if (isTauri) {
      import('@tauri-apps/api/event').then(({ listen }) => {
        listen<string>('open-in-pica-editor', async (event) => {
          const filePath = event.payload;
          if (!filePath) return;
          for (let attempt = 0; attempt < 2; attempt++) {
            for (const g of folderGroups) {
              const idx = g.photos.findIndex((p) => p.file_path === filePath);
              if (idx !== -1) {
                viewerPhotos = g.photos;
                viewerIndex = idx;
                isViewerOpen = true;
                return;
              }
            }
            if (attempt === 0) await refreshData();
          }
        }).then((unsub) => {
          unlistenOpenEditor = unsub;
        });
      });
    }

    // Check if initial URL had a file param to open immediately in editor
    const initialParams = new URLSearchParams(window.location.search);
    const initialFile = initialParams.get('file');
    if (initialFile) {
      refreshData().then(() => {
        for (const g of folderGroups) {
          const idx = g.photos.findIndex((p) => p.file_path === initialFile);
          if (idx !== -1) {
            viewerPhotos = g.photos;
            viewerIndex = idx;
            isViewerOpen = true;
            break;
          }
        }
      });
    }

    return () => {
      clearInterval(faceInterval);
      window.removeEventListener('keydown', handleKeyDown);
      if (typeof unsubProgress === 'function') unsubProgress();
      if (typeof unsubComplete === 'function') unsubComplete();
      if (typeof unsubFaceProgress === 'function') unsubFaceProgress();
      if (unlistenOpenEditor) unlistenOpenEditor();
    };
  });
</script>

<div
  class="h-screen w-screen flex flex-col select-none overflow-hidden font-sans transition-colors"
  style="background-color: var(--canvas-bg); color: var(--text-main);"
>
  <!-- Background Library Shell (Hidden from GPU compositor when Fullscreen Editor is open) -->
  <div
    class="flex-1 flex flex-col overflow-hidden"
    class:invisible={isViewerOpen}
    class:pointer-events-none={isViewerOpen}
  >
  <!-- Classic Picasa Header Bar -->
  <TopBar
    bind:searchQuery
    bind:onlyFavorites
    bind:theme
    onRefresh={refreshData}
    onOpenFolderManager={() => (isFolderManagerOpen = true)}
    onOpenSettings={() => (isSettingsOpen = true)}
    onTriggerScan={async () => {
      isProcessingManuallyOpened = true;
      await triggerScan();
      refreshData();
    }}
  />

  <!-- 3-Pane Body: Left Sidebar + Main Photo Library Grid -->
  <div class="flex-1 overflow-hidden flex flex-row">
    <!-- Left Drawer (Albums, Folders Tree) -->
    <Sidebar
      folderGroups={filteredGroups}
      {stats}
      {onlyFavorites}
      {activeFolderId}
      {activePersonId}
      {faceCollections}
      isCollapsed={isSidebarCollapsed}
      onToggleCollapse={() => (isSidebarCollapsed = !isSidebarCollapsed)}
      onSelectFolder={handleSelectFolder}
      onSelectPerson={handleSelectPerson}
      onToggleFavorites={(val) => {
        onlyFavorites = val;
        activeFolderId = null;
        activePersonId = null;
        personPhotoIds = null;
      }}
      onOpenFolderManager={() => (isFolderManagerOpen = true)}
      onRenamePerson={handleRenamePerson}
    />

    <!-- Main Scrollable Library Grid -->
    <main class="flex-1 overflow-hidden flex flex-col">
      {#if activePerson}
        <div
          class="px-5 py-3 border-b flex items-center justify-between gap-4 shrink-0 transition-colors"
          style="background-color: var(--folder-banner-bg); border-color: var(--border-subtle);"
        >
          <div class="flex items-center gap-3.5 min-w-0">
            <!-- Back to All Photos -->
            <button
              onclick={() => {
                activePersonId = null;
                personPhotoIds = null;
              }}
              class="p-1.5 rounded-md hover:bg-black/10 dark:hover:bg-white/10 transition-colors flex items-center gap-1.5 text-xs font-medium cursor-pointer"
              style="color: var(--text-muted);"
              title="Back to all photos"
            >
              <ArrowLeft class="w-4 h-4" />
              <span class="hidden sm:inline">All Photos</span>
            </button>

            <!-- Person Avatar -->
            <div class="relative shrink-0">
              {#if activePerson.cover_face_id}
                <img
                  src={getFaceCropUrl(activePerson.cover_face_id)}
                  alt={activePerson.name}
                  class="w-10 h-10 rounded-full object-cover border-2 border-purple-500 shadow-sm"
                  onerror={(e) => {
                    (e.currentTarget as HTMLElement).style.display = 'none';
                  }}
                />
              {:else}
                <div class="w-10 h-10 rounded-full bg-purple-500/20 text-purple-400 border border-purple-500/40 flex items-center justify-center font-bold text-sm">
                  {activePerson.name.slice(0, 1).toUpperCase()}
                </div>
              {/if}
            </div>

            <!-- Name and Controls -->
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                {#if isEditingBannerName}
                  <form
                    onsubmit={(e) => {
                      e.preventDefault();
                      if (editingBannerPersonName.trim() && activePerson) {
                        handleRenamePerson(activePerson.id, editingBannerPersonName.trim());
                      }
                      isEditingBannerName = false;
                    }}
                    class="flex items-center gap-1.5"
                  >
                    <!-- svelte-ignore a11y_autofocus -->
                    <input
                      type="text"
                      bind:value={editingBannerPersonName}
                      class="px-2 py-0.5 text-sm font-semibold rounded bg-black/30 border border-purple-500 text-white focus:outline-hidden"
                      autofocus
                    />
                    <button
                      type="submit"
                      class="p-1 rounded bg-purple-600 hover:bg-purple-500 text-white cursor-pointer"
                      title="Save name"
                    >
                      <Check class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      onclick={() => (isEditingBannerName = false)}
                      class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 cursor-pointer"
                      title="Cancel"
                    >
                      <X class="w-3.5 h-3.5" />
                    </button>
                  </form>
                {:else}
                  <button
                    type="button"
                    class="text-base font-bold tracking-tight truncate hover:underline text-left cursor-pointer flex items-center gap-2"
                    style="color: var(--text-main);"
                    onclick={() => {
                      editingBannerPersonName = activePerson.name;
                      isEditingBannerName = true;
                    }}
                    title="Click to rename"
                  >
                    <span>{activePerson.name}</span>
                    <Edit2 class="w-3.5 h-3.5 opacity-60 hover:opacity-100 transition-opacity" style="color: var(--text-muted);" />
                  </button>
                {/if}
              </div>

              <div class="flex items-center gap-2 text-xs" style="color: var(--text-dim);">
                <span>{allVisiblePhotos.length} {allVisiblePhotos.length === 1 ? 'photo' : 'photos'}</span>
                <span>•</span>
                <span>Sorted chronologically</span>
              </div>
            </div>
          </div>

          <div class="flex items-center gap-2">
            <button
              onclick={() => {
                activePersonId = null;
                personPhotoIds = null;
              }}
              class="p-1.5 rounded-full hover:bg-black/10 dark:hover:bg-white/10 transition-colors cursor-pointer"
              style="color: var(--text-muted);"
              title="Close person view"
            >
              <X class="w-4 h-4" />
            </button>
          </div>
        </div>
      {/if}

      <LibraryGrid
        folderGroups={filteredGroups}
        {zoomSize}
        {selectedPhotos}
        onPhotoSelected={handlePhotoSelected}
        onPhotoClick={handlePhotoClick}
        onPlayFolder={handlePlayFolder}
        onSelectFolderPhotos={(photos) => {
          selectedPhotos = photos;
          isTrayHeld = true;
        }}
      />
    </main>
  </div>

  <!-- Classic Picasa Bottom Status Dock, Photo Tray & Zoom Slider -->
  <BottomBar
    {stats}
    {scanProgress}
    faceScanPending={faceSettings?.pending_photos || 0}
    bind:zoomSize
    {selectedPhotos}
    isHeld={isTrayHeld}
    onToggleHold={handleToggleHold}
    onClearTray={handleClearTray}
    onRemovePhoto={handleRemoveFromTray}
    onPlaySlideshow={handlePlaySlideshow}
    onExport={handleExport}
    onOpenProcessingNotification={() => (isProcessingManuallyOpened = true)}
  />
  </div>

  <!-- Folder Manager Dialog -->
  <FolderManagerModal
    bind:isOpen={isFolderManagerOpen}
    onFoldersUpdated={refreshData}
  />

  <!-- Preferences & Options Dialog -->
  <SettingsModal
    bind:isOpen={isSettingsOpen}
    onSettingsChanged={refreshData}
  />

  <!-- Fullscreen / In-Place HUD Photo Viewer -->
  <ImageViewer
    bind:isOpen={isViewerOpen}
    photos={viewerPhotos}
    currentIndex={viewerIndex}
    onPhotoEdited={refreshData}
  />

  <!-- Floating File Processing Notification -->
  <ProcessingNotification
    {scanProgress}
    {faceProgress}
    isLibraryScanning={stats.is_scanning}
    faceScanPending={faceSettings?.pending_photos || 0}
    manuallyOpened={isProcessingManuallyOpened}
    onClose={() => (isProcessingManuallyOpened = false)}
  />
</div>
