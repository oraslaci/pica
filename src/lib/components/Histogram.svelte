<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { BarChart2 } from '@lucide/svelte';
  import type { Photo, PhotoMetadata } from '../types';
  import { getThumbnailUrl, getPreviewImageUrl } from '../api';

  let {
    photo = null,
    metadata = null,
    imageElement = null,
    imageSrc = '',
  }: {
    photo: Photo | null;
    metadata: PhotoMetadata | null;
    imageElement?: HTMLImageElement | null;
    imageSrc?: string;
  } = $props();

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let activeChannel = $state<'rgb' | 'luma' | 'r' | 'g' | 'b'>('rgb');
  let isLoading = $state(false);
  let hasData = $state(false);

  // Cached histograms [256]
  let rHist = new Uint32Array(256);
  let gHist = new Uint32Array(256);
  let bHist = new Uint32Array(256);
  let lumaHist = new Uint32Array(256);

  let lastProcessedKey = '';

  function smoothHistogram(raw: Uint32Array): Float32Array {
    const smoothed = new Float32Array(256);
    for (let i = 0; i < 256; i++) {
      const p2 = raw[Math.max(0, i - 2)];
      const p1 = raw[Math.max(0, i - 1)];
      const c = raw[i];
      const n1 = raw[Math.min(255, i + 1)];
      const n2 = raw[Math.min(255, i + 2)];
      smoothed[i] = (p2 + 2 * p1 + 4 * c + 2 * n1 + n2) / 10;
    }
    return smoothed;
  }

  function processImageData(imgData: ImageData) {
    const data = imgData.data;
    rHist.fill(0);
    gHist.fill(0);
    bHist.fill(0);
    lumaHist.fill(0);

    for (let i = 0; i < data.length; i += 4) {
      const r = data[i];
      const g = data[i + 1];
      const b = data[i + 2];
      const luma = Math.round(0.299 * r + 0.587 * g + 0.114 * b);

      rHist[r]++;
      gHist[g]++;
      bHist[b]++;
      lumaHist[luma]++;
    }

    hasData = true;
    isLoading = false;
    drawHistogram();
  }

  function sampleFromElement(img: HTMLImageElement): boolean {
    if (!img.complete || img.naturalWidth === 0) return false;
    try {
      const sampleW = 240;
      const sampleH = Math.max(1, Math.round((img.naturalHeight / img.naturalWidth) * sampleW));
      const offscreen = document.createElement('canvas');
      offscreen.width = sampleW;
      offscreen.height = sampleH;
      const ctx = offscreen.getContext('2d', { willReadFrequently: true });
      if (!ctx) return false;

      ctx.drawImage(img, 0, 0, sampleW, sampleH);
      const imgData = ctx.getImageData(0, 0, sampleW, sampleH);
      processImageData(imgData);
      return true;
    } catch {
      return false;
    }
  }

  async function loadFromUrl(url: string): Promise<boolean> {
    if (!url) return false;
    return new Promise((resolve) => {
      const img = new Image();
      img.crossOrigin = 'anonymous';
      img.onload = () => {
        const success = sampleFromElement(img);
        resolve(success);
      };
      img.onerror = async () => {
        // Fallback: try fetching as blob if CORS image load was tainted
        try {
          const res = await fetch(url);
          if (!res.ok) return resolve(false);
          const blob = await res.blob();
          const objectUrl = URL.createObjectURL(blob);
          const blobImg = new Image();
          blobImg.onload = () => {
            const ok = sampleFromElement(blobImg);
            URL.revokeObjectURL(objectUrl);
            resolve(ok);
          };
          blobImg.onerror = () => {
            URL.revokeObjectURL(objectUrl);
            resolve(false);
          };
          blobImg.src = objectUrl;
        } catch {
          resolve(false);
        }
      };
      img.src = url;
    });
  }

  async function updateHistogram() {
    if (!photo) {
      hasData = false;
      drawHistogram();
      return;
    }

    const currentKey = `${photo.file_hash}_${imageSrc || ''}`;
    if (currentKey === lastProcessedKey && hasData) {
      drawHistogram();
      return;
    }

    // Try sampling directly from rendered image element first (fastest)
    if (imageElement && imageElement.complete && imageElement.naturalWidth > 0) {
      if (sampleFromElement(imageElement)) {
        lastProcessedKey = currentKey;
        return;
      }
    }

    isLoading = true;

    // Try thumbnail URL first (super fast 5ms load from SQLite cache)
    const thumbUrl = getThumbnailUrl(photo);
    if (thumbUrl) {
      const ok = await loadFromUrl(thumbUrl);
      if (ok) {
        lastProcessedKey = currentKey;
        return;
      }
    }

    // Fallback to activeImageSrc or preview URL
    const targetUrl = imageSrc || (photo.file_path ? getPreviewImageUrl(photo.file_path) : '');
    if (targetUrl) {
      const ok = await loadFromUrl(targetUrl);
      if (ok) {
        lastProcessedKey = currentKey;
        return;
      }
    }

    isLoading = false;
    drawHistogram();
  }

  function drawHistogram() {
    if (!canvasEl) return;
    const ctx = canvasEl.getContext('2d');
    if (!ctx) return;

    const dpr = typeof window !== 'undefined' ? window.devicePixelRatio || 1 : 1;
    const rect = canvasEl.getBoundingClientRect();
    const width = Math.max(200, Math.round(rect.width || 256));
    const height = Math.max(50, Math.round(rect.height || 64));

    if (canvasEl.width !== width * dpr || canvasEl.height !== height * dpr) {
      canvasEl.width = width * dpr;
      canvasEl.height = height * dpr;
    }

    ctx.save();
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, width, height);

    // Rich dark studio background
    const bgGrad = ctx.createLinearGradient(0, 0, 0, height);
    bgGrad.addColorStop(0, '#1c1d25');
    bgGrad.addColorStop(1, '#111217');
    ctx.fillStyle = bgGrad;
    ctx.fillRect(0, 0, width, height);

    // Subtle level dividers (Shadows 25%, Midtones 50%, Highlights 75%)
    ctx.lineWidth = 1;
    for (let xFraction of [0.25, 0.5, 0.75]) {
      const gx = Math.round(width * xFraction);
      ctx.beginPath();
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.07)';
      ctx.setLineDash([2, 3]);
      ctx.moveTo(gx, 0);
      ctx.lineTo(gx, height);
      ctx.stroke();
    }
    ctx.setLineDash([]);

    // 50% horizontal reference line
    ctx.beginPath();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.04)';
    ctx.moveTo(0, Math.round(height * 0.5));
    ctx.lineTo(width, Math.round(height * 0.5));
    ctx.stroke();

    // Baseline bar
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.12)';
    ctx.beginPath();
    ctx.moveTo(0, height - 0.5);
    ctx.lineTo(width, height - 0.5);
    ctx.stroke();

    if (!hasData) {
      if (isLoading) {
        ctx.fillStyle = 'rgba(255, 255, 255, 0.25)';
        ctx.font = '10px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText('Analyzing histogram...', width / 2, height / 2 + 3);
      }
      ctx.restore();
      return;
    }

    // Smooth curves
    const sR = smoothHistogram(rHist);
    const sG = smoothHistogram(gHist);
    const sB = smoothHistogram(bHist);
    const sL = smoothHistogram(lumaHist);

    // Calculate robust peak: ignore 0 and 255 boundary spikes, use 99th percentile
    function getPeak(arr: Float32Array): number {
      const sample: number[] = [];
      for (let i = 2; i < 254; i++) {
        if (arr[i] > 0) sample.push(arr[i]);
      }
      if (sample.length === 0) return 1;
      sample.sort((a, b) => a - b);
      const p99 = sample[Math.floor(sample.length * 0.985)] || sample[sample.length - 1];
      return Math.max(1, p99 * 1.05);
    }

    let maxPeak = 1;
    if (activeChannel === 'rgb') {
      maxPeak = Math.max(getPeak(sR), getPeak(sG), getPeak(sB));
    } else if (activeChannel === 'luma') {
      maxPeak = getPeak(sL);
    } else if (activeChannel === 'r') {
      maxPeak = getPeak(sR);
    } else if (activeChannel === 'g') {
      maxPeak = getPeak(sG);
    } else if (activeChannel === 'b') {
      maxPeak = getPeak(sB);
    }

    const scaleY = (height - 3) / maxPeak;

    function drawCurve(
      data: Float32Array,
      topFill: string,
      bottomFill: string,
      strokeColor: string,
      strokeWidth = 1.3
    ) {
      if (!ctx) return;
      ctx.beginPath();
      ctx.moveTo(0, height);

      for (let i = 0; i < 256; i++) {
        const x = (i / 255) * width;
        const val = Math.min(height - 2, data[i] * scaleY);
        const y = Math.max(1, height - val);
        ctx.lineTo(x, y);
      }

      ctx.lineTo(width, height);
      ctx.closePath();

      const grad = ctx.createLinearGradient(0, 0, 0, height);
      grad.addColorStop(0, topFill);
      grad.addColorStop(1, bottomFill);
      ctx.fillStyle = grad;
      ctx.fill();

      ctx.strokeStyle = strokeColor;
      ctx.lineWidth = strokeWidth;
      ctx.stroke();
    }

    if (activeChannel === 'rgb') {
      // Soft luminance underlay
      drawCurve(
        sL,
        'rgba(255, 255, 255, 0.08)',
        'rgba(255, 255, 255, 0.01)',
        'rgba(255, 255, 255, 0.25)',
        1.0
      );

      // Multi-spectral RGB blending using screen mode
      ctx.save();
      ctx.globalCompositeOperation = 'screen';
      drawCurve(
        sR,
        'rgba(244, 63, 94, 0.45)',
        'rgba(244, 63, 94, 0.02)',
        'rgba(251, 113, 133, 0.95)',
        1.3
      );
      drawCurve(
        sG,
        'rgba(34, 197, 94, 0.45)',
        'rgba(34, 197, 94, 0.02)',
        'rgba(74, 222, 128, 0.95)',
        1.3
      );
      drawCurve(
        sB,
        'rgba(56, 189, 248, 0.45)',
        'rgba(56, 189, 248, 0.02)',
        'rgba(96, 165, 250, 0.95)',
        1.3
      );
      ctx.restore();
    } else if (activeChannel === 'luma') {
      drawCurve(
        sL,
        'rgba(226, 232, 240, 0.55)',
        'rgba(226, 232, 240, 0.05)',
        'rgba(255, 255, 255, 0.95)',
        1.4
      );
    } else if (activeChannel === 'r') {
      drawCurve(
        sR,
        'rgba(244, 63, 94, 0.60)',
        'rgba(244, 63, 94, 0.05)',
        'rgba(251, 113, 133, 0.95)',
        1.4
      );
    } else if (activeChannel === 'g') {
      drawCurve(
        sG,
        'rgba(34, 197, 94, 0.60)',
        'rgba(34, 197, 94, 0.05)',
        'rgba(74, 222, 128, 0.95)',
        1.4
      );
    } else if (activeChannel === 'b') {
      drawCurve(
        sB,
        'rgba(56, 189, 248, 0.60)',
        'rgba(56, 189, 248, 0.05)',
        'rgba(96, 165, 250, 0.95)',
        1.4
      );
    }

    ctx.restore();
  }

  // Reactive triggers
  $effect(() => {
    if (photo) {
      updateHistogram();
    }
  });

  $effect(() => {
    if (imageElement && imageElement.complete && imageElement.naturalWidth > 0) {
      sampleFromElement(imageElement);
      lastProcessedKey = `${photo?.file_hash}_${imageSrc || ''}`;
    }
  });

  $effect(() => {
    if (activeChannel) {
      drawHistogram();
    }
  });

  onMount(() => {
    tick().then(() => {
      updateHistogram();
    });
  });

  // Camera specs helpers (safely sanitized)
  const cameraTitle = $derived.by(() => {
    if (metadata?.camera_model) return metadata.camera_model;
    if (photo?.camera_make || photo?.camera_model) {
      return `${photo.camera_make || ''} ${photo.camera_model || ''}`.trim();
    }
    return 'Camera Information';
  });

  const shutterSpeed = $derived.by(() => {
    if (!metadata?.exposure_time) return '';
    const et = metadata.exposure_time;
    if (et.includes('(')) {
      const match = et.match(/\((.*?)\)/);
      if (match) return match[1];
    }
    return et;
  });

  const apertureValue = $derived.by(() => {
    if (metadata?.f_number) return metadata.f_number;
    if (metadata?.aperture) return `f/${metadata.aperture}`;
    return '';
  });

  const focalLength = $derived.by(() => {
    if (!metadata?.focal_length) return '';
    const fl = metadata.focal_length.trim();
    if (!fl || fl.toLowerCase().includes('unknown') || fl.toLowerCase().includes('null') || fl === '0mm') {
      return '';
    }
    return fl.toLowerCase().includes('focal') ? fl : `Focal Length: ${fl}`;
  });

  const focalLength35 = $derived.by(() => {
    if (!metadata?.focal_length_35mm) return '';
    const fl = metadata.focal_length_35mm.trim();
    if (!fl || fl.toLowerCase().includes('unknown') || fl.toLowerCase().includes('null') || fl === '0mm' || fl === 'mm') {
      return '';
    }
    return `(35mm-equivalent: ${fl})`;
  });

  const isoValue = $derived.by(() => {
    if (!metadata?.iso) return '';
    const iso = metadata.iso.trim();
    if (!iso || iso.toLowerCase().includes('unknown') || iso.toLowerCase().includes('null')) {
      return '';
    }
    return `ISO: ${iso}`;
  });
</script>

<div
  class="rounded-lg bg-[#181820]/95 border border-[#33333f] shadow-xl overflow-hidden text-neutral-300 font-sans backdrop-blur-md select-none transition-all duration-150"
>
  <!-- Card Header & Channel Selector -->
  <div class="px-2.5 py-1.5 border-b border-[#2b2b36] bg-[#1d1d28] flex items-center justify-between">
    <div class="flex items-center gap-1.5 min-w-0">
      <BarChart2 class="w-3.5 h-3.5 text-blue-400 shrink-0" />
      <span class="text-[11px] font-semibold text-neutral-200 tracking-tight truncate">
        Histogram & Camera Info
      </span>
    </div>

    <!-- Mini Channel Selector Pills -->
    <div class="flex items-center bg-[#13131a] p-0.5 rounded border border-neutral-800 text-[9px] font-medium tracking-tight">
      <button
        type="button"
        onclick={() => (activeChannel = 'rgb')}
        class="px-1.5 py-0.5 rounded cursor-pointer transition-colors {activeChannel === 'rgb' ? 'bg-[#2a2a38] text-white font-semibold shadow-xs' : 'text-neutral-400 hover:text-neutral-200'}"
        title="RGB Composite"
      >
        RGB
      </button>
      <button
        type="button"
        onclick={() => (activeChannel = 'r')}
        class="px-1.5 py-0.5 rounded cursor-pointer transition-colors {activeChannel === 'r' ? 'bg-red-950/80 text-red-300 font-semibold' : 'text-red-400/70 hover:text-red-300'}"
        title="Red Channel"
      >
        R
      </button>
      <button
        type="button"
        onclick={() => (activeChannel = 'g')}
        class="px-1.5 py-0.5 rounded cursor-pointer transition-colors {activeChannel === 'g' ? 'bg-emerald-950/80 text-emerald-300 font-semibold' : 'text-emerald-400/70 hover:text-emerald-300'}"
        title="Green Channel"
      >
        G
      </button>
      <button
        type="button"
        onclick={() => (activeChannel = 'b')}
        class="px-1.5 py-0.5 rounded cursor-pointer transition-colors {activeChannel === 'b' ? 'bg-blue-950/80 text-blue-300 font-semibold' : 'text-blue-400/70 hover:text-blue-300'}"
        title="Blue Channel"
      >
        B
      </button>
      <button
        type="button"
        onclick={() => (activeChannel = 'luma')}
        class="px-1.5 py-0.5 rounded cursor-pointer transition-colors {activeChannel === 'luma' ? 'bg-neutral-700 text-white font-semibold' : 'text-neutral-400 hover:text-neutral-200'}"
        title="Luminance"
      >
        L
      </button>
    </div>
  </div>

  <!-- Histogram Canvas Container -->
  <div class="p-2 pb-1.5 flex flex-col items-center">
    <div class="w-full relative rounded overflow-hidden border border-neutral-700/60 shadow-inner bg-[#121319]">
      <canvas
        bind:this={canvasEl}
        class="w-full h-16 block cursor-pointer"
        onclick={() => {
          // Quick cycle channels on click
          const cycle: Array<'rgb' | 'r' | 'g' | 'b' | 'luma'> = ['rgb', 'r', 'g', 'b', 'luma'];
          const nextIdx = (cycle.indexOf(activeChannel) + 1) % cycle.length;
          activeChannel = cycle[nextIdx];
        }}
        title="Click to cycle color channels"
      ></canvas>

      <!-- Dynamic Channel indicator badge inside canvas -->
      <div class="absolute top-1 left-1.5 pointer-events-none flex items-center gap-1 text-[8px] font-mono uppercase font-semibold">
        {#if activeChannel === 'rgb'}
          <span class="text-neutral-400/80 bg-black/40 px-1 py-0.2 rounded">RGB Composite</span>
        {:else if activeChannel === 'r'}
          <span class="text-rose-400 bg-red-950/60 px-1 py-0.2 rounded border border-rose-900/40">Red</span>
        {:else if activeChannel === 'g'}
          <span class="text-emerald-400 bg-emerald-950/60 px-1 py-0.2 rounded border border-emerald-900/40">Green</span>
        {:else if activeChannel === 'b'}
          <span class="text-sky-400 bg-blue-950/60 px-1 py-0.2 rounded border border-sky-900/40">Blue</span>
        {:else if activeChannel === 'luma'}
          <span class="text-neutral-200 bg-neutral-800/80 px-1 py-0.2 rounded border border-neutral-700/40">Luma</span>
        {/if}
      </div>

      <!-- Tone Range Markers (Shadows, Midtones, Highlights) -->
      <div class="absolute bottom-0.5 inset-x-0 px-2 flex justify-between pointer-events-none text-[7.5px] font-mono text-neutral-500/70 select-none">
        <span>0</span>
        <span>64</span>
        <span>128</span>
        <span>192</span>
        <span>255</span>
      </div>
    </div>
  </div>

  <!-- Camera Specs (2 Columns matching Picasa) -->
  <div class="px-2.5 pb-2 pt-0.5 grid grid-cols-2 gap-x-2 text-[10px] leading-tight text-neutral-400">
    <!-- Left Column -->
    <div class="space-y-0.5 truncate">
      <div class="font-medium text-neutral-200 truncate" title={cameraTitle}>
        {cameraTitle}
      </div>
      {#if focalLength}
        <div class="truncate text-neutral-300" title={focalLength}>{focalLength}</div>
      {/if}
      {#if focalLength35}
        <div class="text-[9px] text-neutral-500 truncate" title={focalLength35}>{focalLength35}</div>
      {/if}
    </div>

    <!-- Right Column (right-aligned) -->
    <div class="space-y-0.5 text-right font-mono truncate">
      {#if shutterSpeed}
        <div class="text-neutral-200 truncate" title={shutterSpeed}>{shutterSpeed}</div>
      {:else}
        <div>&nbsp;</div>
      {/if}
      {#if apertureValue}
        <div class="truncate text-neutral-300" title={apertureValue}>{apertureValue}</div>
      {:else}
        <div>&nbsp;</div>
      {/if}
      {#if isoValue}
        <div class="text-neutral-200 font-semibold truncate" title={isoValue}>{isoValue}</div>
      {/if}
    </div>
  </div>
</div>
