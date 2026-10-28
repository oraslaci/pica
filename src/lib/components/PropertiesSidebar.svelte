<script lang="ts">
  import { X, Copy, Check, ChevronDown, ChevronRight, HardDrive, Camera, Calendar, Tag, Image as ImageIcon } from '@lucide/svelte';
  import type { Photo, PhotoMetadata } from '../types';
  import { getThumbnailUrl } from '../api';

  let {
    photo,
    metadata = null,
    isOpen = true,
    onClose = () => {},
  }: {
    photo: Photo | null;
    metadata: PhotoMetadata | null;
    isOpen: boolean;
    onClose: () => void;
  } = $props();

  let copiedField = $state<string | null>(null);
  let showAllExif = $state(false);

  function copyToClipboard(text: string, fieldName: string) {
    if (!text) return;
    navigator.clipboard.writeText(text);
    copiedField = fieldName;
    setTimeout(() => {
      if (copiedField === fieldName) copiedField = null;
    }, 1500);
  }

  // Format Helpers
  const locationPath = $derived.by(() => {
    if (metadata?.location) return metadata.location;
    if (photo?.file_path) {
      const idx = Math.max(photo.file_path.lastIndexOf('/'), photo.file_path.lastIndexOf('\\'));
      return idx !== -1 ? photo.file_path.substring(0, idx + 1) : photo.file_path;
    }
    return '';
  });

  const fileSizeStr = $derived.by(() => {
    if (metadata?.file_size_formatted) return metadata.file_size_formatted;
    if (photo?.file_size) {
      const mb = photo.file_size / (1024 * 1024);
      return `${mb.toFixed(1)} MB`;
    }
    return '';
  });

  const dimensionsStr = $derived.by(() => {
    if (metadata?.dimensions) return metadata.dimensions;
    if (photo?.width && photo?.height) return `${photo.width} x ${photo.height} pixels`;
    return '';
  });

  const peopleTagsList = $derived.by(() => {
    if (metadata?.people_tags && metadata.people_tags.length > 0) {
      return metadata.people_tags.join(', ');
    }
    return 'None';
  });
</script>

{#if isOpen && photo}
  <aside
    class="w-80 sm:w-88 h-full bg-[#1c1c23] border-l border-[#2e2e38] flex flex-col z-30 shrink-0 text-xs shadow-2xl transition-all duration-150 select-text"
  >
    <!-- Header -->
    <div class="h-10 px-3 bg-[#23232c] border-b border-[#2e2e38] flex items-center justify-between select-none">
      <div class="flex items-center gap-2">
        <span class="font-bold text-neutral-100 text-sm tracking-tight">Properties</span>
        <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-300 font-mono">Info for Nerds</span>
      </div>
      <button
        onclick={onClose}
        class="p-1 rounded hover:bg-white/10 text-neutral-400 hover:text-white transition-colors cursor-pointer"
        title="Close Properties (i or Esc)"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Subtitle Banner -->
    <div class="px-3.5 py-2 bg-[#19191f] border-b border-[#282832] text-neutral-300 font-medium truncate select-none">
      Properties of <span class="text-white font-mono">{photo.file_name}</span>:
    </div>

    <!-- Scrollable Properties Body -->
    <div class="flex-1 overflow-y-auto px-3.5 py-3 space-y-3 font-sans custom-scrollbar">
      
      <!-- Location -->
      <div class="space-y-0.5 group">
        <div class="text-[11px] font-semibold text-neutral-400">Location</div>
        <div class="flex items-start justify-between gap-1">
          <span class="text-neutral-200 font-mono text-[11px] break-all select-all leading-snug">
            {locationPath || '—'}
          </span>
          {#if locationPath}
            <button
              onclick={() => copyToClipboard(locationPath, 'location')}
              class="opacity-0 group-hover:opacity-100 p-0.5 text-neutral-400 hover:text-white transition-opacity shrink-0 cursor-pointer"
              title="Copy location"
            >
              {#if copiedField === 'location'}
                <Check class="w-3.5 h-3.5 text-emerald-400" />
              {:else}
                <Copy class="w-3.5 h-3.5" />
              {/if}
            </button>
          {/if}
        </div>
      </div>

      <!-- File Size -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">File Size</div>
        <div class="text-neutral-200">{fileSizeStr || '—'}</div>
      </div>

      <!-- Dimensions -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Dimensions</div>
        <div class="text-neutral-200">
          {dimensionsStr || '—'}
          {#if metadata?.megapixels}
            <span class="text-neutral-400 font-mono text-[10px] ml-1">({metadata.megapixels})</span>
          {/if}
        </div>
      </div>

      <!-- Camera Make & Model -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Camera Make</div>
        <div class="text-neutral-200">{metadata?.camera_make || photo.camera_make || '—'}</div>
      </div>

      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Camera Model</div>
        <div class="text-neutral-200">{metadata?.camera_model || photo.camera_model || '—'}</div>
      </div>

      <!-- Dates -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Camera Date</div>
        <div class="text-neutral-200 font-mono text-[11px]">{metadata?.camera_date || '—'}</div>
      </div>

      {#if metadata?.digitized_date}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Digitized Date</div>
          <div class="text-neutral-200 font-mono text-[11px]">{metadata.digitized_date}</div>
        </div>
      {/if}

      {#if metadata?.modified_date}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Modified Date</div>
          <div class="text-neutral-200 font-mono text-[11px]">{metadata.modified_date}</div>
        </div>
      {/if}

      {#if metadata?.file_date}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">File Date</div>
          <div class="text-neutral-200 font-mono text-[11px]">{metadata.file_date}</div>
        </div>
      {/if}

      <!-- Orientation -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Orientation</div>
        <div class="text-neutral-200">{metadata?.orientation || 'Normal'}</div>
      </div>

      <!-- Flash -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">Flash</div>
        <div class="text-neutral-200">{metadata?.flash || 'Not Used'}</div>
      </div>

      <!-- Lens -->
      {#if metadata?.lens}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Lens</div>
          <div class="text-neutral-200">{metadata.lens}</div>
        </div>
      {/if}

      <!-- Focal Length -->
      {#if metadata?.focal_length}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Focal Length</div>
          <div class="text-neutral-200">{metadata.focal_length}</div>
        </div>
      {/if}

      {#if metadata?.focal_length_35mm && !metadata.focal_length_35mm.toLowerCase().includes('unknown') && !metadata.focal_length_35mm.toLowerCase().includes('null')}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Focal Length in 35mm Film</div>
          <div class="text-neutral-200">{metadata.focal_length_35mm}</div>
        </div>
      {/if}

      {#if metadata?.ccd_width}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">CCD Width</div>
          <div class="text-neutral-200">{metadata.ccd_width} mm</div>
        </div>
      {/if}

      <!-- Exposure Settings -->
      {#if metadata?.exposure_time}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Exposure Time</div>
          <div class="text-neutral-200 font-mono">{metadata.exposure_time}</div>
        </div>
      {/if}

      {#if metadata?.aperture}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Aperture</div>
          <div class="text-neutral-200 font-mono">{metadata.aperture}</div>
        </div>
      {/if}

      {#if metadata?.f_number}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">F Number</div>
          <div class="text-neutral-200 font-mono">{metadata.f_number}</div>
        </div>
      {/if}

      {#if metadata?.iso}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">ISO</div>
          <div class="text-neutral-200 font-mono">{metadata.iso}</div>
        </div>
      {/if}

      {#if metadata?.white_balance}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">White Balance</div>
          <div class="text-neutral-200">{metadata.white_balance}</div>
        </div>
      {/if}

      {#if metadata?.metering_mode}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Metering Mode</div>
          <div class="text-neutral-200">{metadata.metering_mode}</div>
        </div>
      {/if}

      {#if metadata?.exposure_program}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Exposure Program</div>
          <div class="text-neutral-200">{metadata.exposure_program}</div>
        </div>
      {/if}

      {#if metadata?.color_space}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Color Space</div>
          <div class="text-neutral-200">{metadata.color_space}</div>
        </div>
      {/if}

      <!-- Embedded Thumbnail Preview -->
      <div class="space-y-1.5 pt-1">
        <div class="text-[11px] font-semibold text-neutral-400">Thumbnail (160 x 120 pixels)</div>
        <div class="w-40 h-30 bg-black/40 rounded border border-neutral-700/80 overflow-hidden flex items-center justify-center shadow-inner">
          <img
            src={getThumbnailUrl(photo)}
            alt="Thumbnail"
            class="max-w-full max-h-full object-contain"
            onerror={(e) => {
              (e.currentTarget as HTMLElement).style.display = 'none';
            }}
          />
        </div>
      </div>

      {#if metadata?.jpeg_quality}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">JPEG Quality</div>
          <div class="text-neutral-200">{metadata.jpeg_quality}</div>
        </div>
      {/if}

      <!-- People Tags -->
      <div class="space-y-0.5">
        <div class="text-[11px] font-semibold text-neutral-400">People Tags</div>
        <div class="text-neutral-200 font-medium">{peopleTagsList}</div>
      </div>

      <!-- Unique ID -->
      <div class="space-y-0.5 group">
        <div class="text-[11px] font-semibold text-neutral-400">Unique ID</div>
        <div class="flex items-center justify-between gap-1">
          <span class="text-neutral-400 font-mono text-[10px] truncate select-all">
            {photo.file_hash}
          </span>
          <button
            onclick={() => copyToClipboard(photo.file_hash, 'hash')}
            class="opacity-0 group-hover:opacity-100 p-0.5 text-neutral-400 hover:text-white transition-opacity shrink-0 cursor-pointer"
            title="Copy Unique ID"
          >
            {#if copiedField === 'hash'}
              <Check class="w-3.5 h-3.5 text-emerald-400" />
            {:else}
              <Copy class="w-3.5 h-3.5" />
            {/if}
          </button>
        </div>
      </div>

      <!-- Resolution -->
      {#if metadata?.x_resolution}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">X Resolution</div>
          <div class="text-neutral-200">{metadata.x_resolution}</div>
        </div>
      {/if}

      {#if metadata?.y_resolution}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Y Resolution</div>
          <div class="text-neutral-200">{metadata.y_resolution}</div>
        </div>
      {/if}

      {#if metadata?.resolution_unit}
        <div class="space-y-0.5">
          <div class="text-[11px] font-semibold text-neutral-400">Resolution Unit</div>
          <div class="text-neutral-200">{metadata.resolution_unit}</div>
        </div>
      {/if}

      <!-- Expandable Complete Raw EXIF Metadata -->
      {#if metadata?.all_tags && metadata.all_tags.length > 0}
        <div class="pt-2 border-t border-neutral-800">
          <button
            onclick={() => (showAllExif = !showAllExif)}
            class="w-full flex items-center justify-between py-1.5 text-[11px] font-bold text-neutral-300 hover:text-white transition-colors cursor-pointer select-none"
          >
            <span>All Raw EXIF Tags ({metadata.all_tags.length})</span>
            {#if showAllExif}
              <ChevronDown class="w-3.5 h-3.5" />
            {:else}
              <ChevronRight class="w-3.5 h-3.5" />
            {/if}
          </button>

          {#if showAllExif}
            <div class="mt-2 space-y-1.5 bg-[#141419] p-2 rounded border border-neutral-800 font-mono text-[10px]">
              {#each metadata.all_tags as tag}
                <div class="flex items-start justify-between gap-2 border-b border-neutral-800/60 pb-1">
                  <span class="text-neutral-400 shrink-0 select-text">{tag.tag}:</span>
                  <span class="text-neutral-200 text-right select-text break-all">{tag.value}</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

    </div>
  </aside>
{/if}

<style>
  .custom-scrollbar::-webkit-scrollbar {
    width: 6px;
  }
  .custom-scrollbar::-webkit-scrollbar-track {
    background: transparent;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb {
    background: #343440;
    border-radius: 3px;
  }
  .custom-scrollbar::-webkit-scrollbar-thumb:hover {
    background: #4a4a5a;
  }
</style>
