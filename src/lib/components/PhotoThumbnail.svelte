<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Image as ImageIcon } from '@lucide/svelte';
  import type { Photo } from '../types';
  import { getThumbnailUrl } from '../api';

  let { photo, class: className = '' }: { photo: Photo; class?: string } = $props();

  let isLoaded = $state(false);
  let retryCount = $state(0);
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let lastHash = $state('');

  let currentSrc = $derived(
    retryCount === 0
      ? getThumbnailUrl(photo)
      : `${getThumbnailUrl(photo)}?r=${retryCount}`
  );

  $effect(() => {
    if (photo.file_hash !== lastHash) {
      lastHash = photo.file_hash;
      retryCount = 0;
      isLoaded = false;
      if (retryTimer) clearTimeout(retryTimer);
    }
  });

  function handleLoad() {
    isLoaded = true;
    if (retryTimer) clearTimeout(retryTimer);
  }

  function handleError() {
    isLoaded = false;
    if (retryTimer) clearTimeout(retryTimer);
    // Fast initial retries, then gentle polling every 2.5s while photo is in viewport
    const delay = retryCount < 6
      ? 200 + retryCount * 150
      : (retryCount < 15 ? 1000 : 2500);
    retryTimer = setTimeout(() => {
      retryCount += 1;
    }, delay);
  }

  onDestroy(() => {
    if (retryTimer) clearTimeout(retryTimer);
  });
</script>

<div class="relative w-full h-full flex items-center justify-center overflow-hidden bg-neutral-900 rounded-xs">
  {#if !isLoaded}
    <div class="absolute inset-0 flex items-center justify-center bg-neutral-800/30 animate-pulse">
      <ImageIcon class="w-6 h-6 text-neutral-700/50" />
    </div>
  {/if}

  <img
    src={currentSrc}
    alt={photo.file_name}
    onload={handleLoad}
    onerror={handleError}
    class="{className} transition-opacity duration-200 {isLoaded ? 'opacity-100' : 'opacity-0'}"
  />
</div>
