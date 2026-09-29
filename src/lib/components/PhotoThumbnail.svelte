<script module lang="ts">
  const loadedHashes = new Set<string>();
</script>

<script lang="ts">
  import { onDestroy } from 'svelte';
  import { Image as ImageIcon } from '@lucide/svelte';
  import type { Photo } from '../types';
  import { getThumbnailUrl, onThumbnailReady } from '../api';

  let { photo, class: className = '' }: { photo: Photo; class?: string } = $props();

  let isLoaded = $state(loadedHashes.has(photo.file_hash));
  let retryCount = $state(0);
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let lastHash = $state(photo.file_hash);

  let currentSrc = $derived(
    retryCount === 0
      ? getThumbnailUrl(photo)
      : `${getThumbnailUrl(photo)}?r=${retryCount}`
  );

  $effect(() => {
    const hash = photo.file_hash;
    if (hash !== lastHash) {
      lastHash = hash;
      isLoaded = loadedHashes.has(hash);
      retryCount = 0;
      if (retryTimer) clearTimeout(retryTimer);
    }

    const unsub = onThumbnailReady(hash, () => {
      if (!isLoaded) {
        if (retryTimer) clearTimeout(retryTimer);
        retryCount += 1;
      }
    });

    return () => {
      unsub();
      if (retryTimer) clearTimeout(retryTimer);
    };
  });

  function handleLoad() {
    isLoaded = true;
    loadedHashes.add(photo.file_hash);
    if (retryTimer) clearTimeout(retryTimer);
  }

  function handleError() {
    isLoaded = false;
    if (retryTimer) clearTimeout(retryTimer);
    // Don't hammer the network if 100s of images are rendering on zoom out.
    // Rely on onThumbnailReady for instant updates, with gentle fallback backoff.
    const delay = retryCount === 0
      ? 200
      : (retryCount < 5 ? 600 + retryCount * 300 : 2500);
    retryTimer = setTimeout(() => {
      retryCount += 1;
    }, delay);
  }

  onDestroy(() => {
    if (retryTimer) clearTimeout(retryTimer);
  });
</script>

<div class="relative w-full h-full flex items-center justify-center overflow-hidden bg-neutral-900 rounded-xs select-none">
  {#if !isLoaded}
    <div class="absolute inset-0 flex items-center justify-center bg-neutral-800/40 animate-pulse">
      <ImageIcon class="w-5 h-5 text-neutral-600/40" />
    </div>
  {/if}

  <img
    src={currentSrc}
    alt={photo.file_name}
    onload={handleLoad}
    onerror={handleError}
    class="{className} transition-all duration-300 ease-out {isLoaded ? 'opacity-100 scale-100' : 'opacity-0 scale-[0.97]'}"
  />
</div>
