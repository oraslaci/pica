<script lang="ts">
  import type { FolderWithPhotos, LibraryStats } from '../types';

  interface Props {
    folderGroups: FolderWithPhotos[];
    stats: LibraryStats;
    onlyFavorites: boolean;
    activeFolderId: number | null;
    isCollapsed: boolean;
    onToggleCollapse: () => void;
    onSelectFolder: (folderId: number | null) => void;
    onToggleFavorites: (val: boolean) => void;
    onOpenFolderManager: () => void;
  }

  let {
    folderGroups,
    stats,
    onlyFavorites,
    activeFolderId,
    isCollapsed,
    onToggleCollapse,
    onSelectFolder,
    onToggleFavorites,
    onOpenFolderManager,
  }: Props = $props();

  let isAlbumsExpanded = $state(true);
  let isFoldersExpanded = $state(true);
</script>

<aside
  class="h-full flex flex-col border-r transition-all duration-200 select-none z-10"
  style="
    background-color: var(--panel-bg);
    border-color: var(--border-subtle);
    width: {isCollapsed ? '48px' : '240px'};
    min-width: {isCollapsed ? '48px' : '240px'};
  "
>
  <!-- Sidebar Header / Collapse Toggle -->
  <div
    class="h-10 px-3 flex items-center justify-between border-b"
    style="border-color: var(--border-subtle); background-color: var(--header-bg);"
  >
    {#if !isCollapsed}
      <div class="flex items-center gap-2 overflow-hidden">
        <svg class="w-4 h-4 text-blue-500 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
        </svg>
        <span class="text-xs font-bold tracking-wider uppercase truncate" style="color: var(--text-main);">
          Library
        </span>
      </div>
    {/if}

    <button
      onclick={onToggleCollapse}
      title={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
      class="p-1 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors mx-auto"
      style="color: var(--text-muted);"
    >
      {#if isCollapsed}
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 5l7 7-7 7M5 5l7 7-7 7" />
        </svg>
      {:else}
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 19l-7-7 7-7m8 14l-7-7 7-7" />
        </svg>
      {/if}
    </button>
  </div>

  <!-- Sidebar Content (Scrollable) -->
  <div class="flex-1 overflow-y-auto overflow-x-hidden py-2 space-y-4 text-sm">
    {#if isCollapsed}
      <!-- Collapsed Icon-Only View -->
      <div class="flex flex-col items-center gap-2 px-1">
        <button
          onclick={() => { onToggleFavorites(false); onSelectFolder(null); }}
          title="All Photos ({stats.total_photos})"
          class="w-8 h-8 rounded flex items-center justify-center transition-colors {activeFolderId === null && !onlyFavorites ? 'bg-blue-600 text-white' : 'hover:bg-black/10 dark:hover:bg-white/10'}"
          style={activeFolderId === null && !onlyFavorites ? '' : 'color: var(--text-main);'}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
          </svg>
        </button>

        <button
          onclick={() => { onToggleFavorites(true); onSelectFolder(null); }}
          title="Starred ({stats.total_favorites})"
          class="w-8 h-8 rounded flex items-center justify-center transition-colors {onlyFavorites ? 'bg-amber-500 text-white' : 'hover:bg-black/10 dark:hover:bg-white/10'}"
          style={onlyFavorites ? '' : 'color: var(--text-main);'}
        >
          <svg class="w-4 h-4 {onlyFavorites ? 'text-white' : 'text-amber-400'}" fill="currentColor" viewBox="0 0 20 20">
            <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
          </svg>
        </button>

        <div class="w-6 h-px my-1" style="background-color: var(--border-subtle);"></div>

        {#each folderGroups as group}
          <button
            onclick={() => onSelectFolder(group.folder.id)}
            title="{group.folder.display_name} ({group.photos.length})"
            class="w-8 h-8 rounded flex items-center justify-center transition-colors {activeFolderId === group.folder.id ? 'bg-blue-600 text-white' : 'hover:bg-black/10 dark:hover:bg-white/10'}"
            style={activeFolderId === group.folder.id ? '' : 'color: var(--text-main);'}
          >
            <svg class="w-4 h-4 text-amber-500" fill="currentColor" viewBox="0 0 20 20">
              <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
            </svg>
          </button>
        {/each}
      </div>
    {:else}
      <!-- Expanded Tree View -->
      <!-- SECTION 1: ALBUMS & COLLECTIONS -->
      <div>
        <button
          onclick={() => (isAlbumsExpanded = !isAlbumsExpanded)}
          class="w-full px-3 py-1 flex items-center justify-between text-xs font-semibold uppercase tracking-wider transition-colors hover:bg-black/5 dark:hover:bg-white/5"
          style="color: var(--text-dim);"
        >
          <span>Collections</span>
          <svg class="w-3 h-3 transition-transform {isAlbumsExpanded ? 'rotate-90' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>

        {#if isAlbumsExpanded}
          <div class="mt-1 space-y-0.5 px-2">
            <!-- All Photos -->
            <button
              onclick={() => { onToggleFavorites(false); onSelectFolder(null); }}
              class="w-full px-2 py-1.5 rounded-md flex items-center justify-between transition-colors {activeFolderId === null && !onlyFavorites ? 'bg-blue-600 text-white font-medium shadow-sm' : 'hover:bg-black/5 dark:hover:bg-white/5'}"
              style={activeFolderId === null && !onlyFavorites ? '' : 'color: var(--text-main);'}
            >
              <div class="flex items-center gap-2 truncate">
                <svg class="w-4 h-4 shrink-0 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
                </svg>
                <span class="truncate">All Photos</span>
              </div>
              <span class="text-xs px-1.5 py-0.2 rounded-full {activeFolderId === null && !onlyFavorites ? 'bg-blue-700 text-blue-100' : 'bg-black/10 dark:bg-white/10'}" style={activeFolderId === null && !onlyFavorites ? '' : 'color: var(--text-muted);'}>
                {stats.total_photos}
              </span>
            </button>

            <!-- Starred Photos -->
            <button
              onclick={() => { onToggleFavorites(true); onSelectFolder(null); }}
              class="w-full px-2 py-1.5 rounded-md flex items-center justify-between transition-colors {onlyFavorites ? 'bg-amber-600 text-white font-medium shadow-sm' : 'hover:bg-black/5 dark:hover:bg-white/5'}"
              style={onlyFavorites ? '' : 'color: var(--text-main);'}
            >
              <div class="flex items-center gap-2 truncate">
                <svg class="w-4 h-4 shrink-0 text-amber-400 fill-current" viewBox="0 0 20 20">
                  <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                </svg>
                <span class="truncate">Starred Photos</span>
              </div>
              <span class="text-xs px-1.5 py-0.2 rounded-full {onlyFavorites ? 'bg-amber-700 text-amber-100' : 'bg-black/10 dark:bg-white/10'}" style={onlyFavorites ? '' : 'color: var(--text-muted);'}>
                {stats.total_favorites}
              </span>
            </button>
          </div>
        {/if}
      </div>

      <!-- SECTION 2: FOLDERS -->
      <div>
        <div class="px-3 py-1 flex items-center justify-between">
          <button
            onclick={() => (isFoldersExpanded = !isFoldersExpanded)}
            class="flex items-center gap-1 text-xs font-semibold uppercase tracking-wider transition-colors hover:text-blue-500"
            style="color: var(--text-dim);"
          >
            <span>Folders ({folderGroups.length})</span>
            <svg class="w-3 h-3 transition-transform {isFoldersExpanded ? 'rotate-90' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
            </svg>
          </button>

          <button
            onclick={onOpenFolderManager}
            title="Manage Scanned Folders..."
            class="p-0.5 rounded hover:bg-black/10 dark:hover:bg-white/10 transition-colors"
            style="color: var(--text-dim);"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
            </svg>
          </button>
        </div>

        {#if isFoldersExpanded}
          <div class="mt-1 space-y-0.5 px-2">
            {#each folderGroups as group}
              <button
                onclick={() => onSelectFolder(group.folder.id)}
                class="w-full px-2 py-1.5 rounded-md flex items-center justify-between transition-colors {activeFolderId === group.folder.id ? 'bg-blue-600 text-white font-medium shadow-sm' : 'hover:bg-black/5 dark:hover:bg-white/5'}"
                style={activeFolderId === group.folder.id ? '' : 'color: var(--text-main);'}
              >
                <div class="flex items-center gap-2 truncate">
                  <svg class="w-4 h-4 shrink-0 text-amber-500 fill-current" viewBox="0 0 20 20">
                    <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
                  </svg>
                  <span class="truncate" title={group.folder.path}>
                    {group.folder.display_name}
                  </span>
                </div>
                <span class="text-xs px-1.5 py-0.2 rounded-full {activeFolderId === group.folder.id ? 'bg-blue-700 text-blue-100' : 'bg-black/10 dark:bg-white/10'}" style={activeFolderId === group.folder.id ? '' : 'color: var(--text-muted);'}>
                  {group.photos.length}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  <!-- Bottom Folder Manager Link -->
  {#if !isCollapsed}
    <div
      class="p-2 border-t text-xs flex items-center justify-between"
      style="border-color: var(--border-subtle); background-color: var(--footer-bg);"
    >
      <button
        onclick={onOpenFolderManager}
        class="flex items-center gap-1.5 text-xs text-blue-500 hover:text-blue-400 font-medium py-1 px-1.5 rounded hover:bg-black/5 dark:hover:bg-white/5 transition-colors"
      >
        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
        <span>Folder Manager...</span>
      </button>
    </div>
  {/if}
</aside>
