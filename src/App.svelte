<script lang="ts">
  import { onMount } from 'svelte';
  import TopBar from './lib/components/TopBar.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import LibraryGrid from './lib/components/LibraryGrid.svelte';
  import BottomBar from './lib/components/BottomBar.svelte';
  import FolderManagerModal from './lib/components/FolderManagerModal.svelte';
  import ImageViewer from './lib/components/ImageViewer.svelte';
  import {
    getFoldersWithPhotos,
    getLibraryStats,
    onScanProgress,
    onScanComplete,
    triggerScan,
    setThumbnailPriority,
  } from './lib/api';
  import type { FolderWithPhotos, LibraryStats, Photo, ScanProgress } from './lib/types';

  let folderGroups = $state<FolderWithPhotos[]>([]);
  let stats = $state<LibraryStats>({
    total_photos: 0,
    total_folders: 0,
    total_favorites: 0,
    is_scanning: false,
  });
  let scanProgress = $state<ScanProgress | null>(null);

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
  let isViewerOpen = $state(false);
  let viewerPhotos = $state<Photo[]>([]);
  let viewerIndex = $state(0);

  async function refreshData() {
    try {
      const [groups, st] = await Promise.all([
        getFoldersWithPhotos(),
        getLibraryStats(),
      ]);
      folderGroups = groups;
      stats = st;
    } catch (err) {
      console.error('Failed to load library:', err);
    }
  }

  // Filtered folder groups based on search & starred filter
  const filteredGroups = $derived(
    folderGroups
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
      .filter((g) => g.photos.length > 0)
  );

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

  function handleSelectFolder(folderId: number | null) {
    activeFolderId = folderId;
    if (folderId !== null) {
      const group = folderGroups.find((g) => g.folder.id === folderId);
      if (group && group.photos.length > 0) {
        // Preempt high-priority queue immediately with the clicked folder's photos
        const targetHashes = group.photos.map((p) => p.file_hash);
        setThumbnailPriority(targetHashes, true);
      }
      const el = document.getElementById(`folder-${folderId}`);
      if (el) {
        el.scrollIntoView({ behavior: 'smooth' });
      }
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
      scanProgress = null;
      stats.is_scanning = false;
      refreshData();
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

    window.addEventListener('keydown', handleKeyDown);

    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      if (typeof unsubProgress === 'function') unsubProgress();
      if (typeof unsubComplete === 'function') unsubComplete();
    };
  });
</script>

<div
  class="h-screen w-screen flex flex-col select-none overflow-hidden font-sans transition-colors"
  style="background-color: var(--canvas-bg); color: var(--text-main);"
>
  <!-- Classic Picasa Header Bar -->
  <TopBar
    bind:searchQuery
    bind:onlyFavorites
    bind:theme
    onRefresh={refreshData}
    onOpenFolderManager={() => (isFolderManagerOpen = true)}
    onTriggerScan={async () => {
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
      isCollapsed={isSidebarCollapsed}
      onToggleCollapse={() => (isSidebarCollapsed = !isSidebarCollapsed)}
      onSelectFolder={handleSelectFolder}
      onToggleFavorites={(val) => {
        onlyFavorites = val;
        activeFolderId = null;
      }}
      onOpenFolderManager={() => (isFolderManagerOpen = true)}
    />

    <!-- Main Scrollable Library Grid -->
    <main class="flex-1 overflow-hidden flex flex-col">
      <LibraryGrid
        folderGroups={filteredGroups}
        {zoomSize}
        {selectedPhotos}
        onPhotoSelected={handlePhotoSelected}
        onPhotoClick={handlePhotoClick}
        onPlayFolder={handlePlayFolder}
      />
    </main>
  </div>

  <!-- Classic Picasa Bottom Status Dock, Photo Tray & Zoom Slider -->
  <BottomBar
    {stats}
    {scanProgress}
    bind:zoomSize
    {selectedPhotos}
    isHeld={isTrayHeld}
    onToggleHold={handleToggleHold}
    onClearTray={handleClearTray}
    onRemovePhoto={handleRemoveFromTray}
    onPlaySlideshow={handlePlaySlideshow}
    onExport={handleExport}
  />

  <!-- Folder Manager Dialog -->
  <FolderManagerModal
    bind:isOpen={isFolderManagerOpen}
    onFoldersUpdated={refreshData}
  />

  <!-- Fullscreen / In-Place HUD Photo Viewer -->
  <ImageViewer
    bind:isOpen={isViewerOpen}
    photos={viewerPhotos}
    currentIndex={viewerIndex}
  />
</div>
