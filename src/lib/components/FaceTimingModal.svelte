<script lang="ts">
  import { onMount } from 'svelte';
  import {
    X,
    Clock,
    Download,
    Trash2,
    Copy,
    Check,
    RefreshCw,
    FileSpreadsheet,
    Cpu,
    Eye,
  } from '@lucide/svelte';
  import {
    getFaceTimingStats,
    getFaceTimingRecords,
    clearFaceTimingRecords,
    exportFaceTimingCsv,
  } from '../api';
  import type { FaceTimingStats, FaceScanTimingRecord } from '../types';

  let {
    isOpen = $bindable(false),
  }: {
    isOpen: boolean;
  } = $props();

  let stats = $state<FaceTimingStats | null>(null);
  let records = $state<FaceScanTimingRecord[]>([]);
  let isLoading = $state(false);
  let copiedPath = $state(false);
  let copiedJson = $state(false);

  async function loadData() {
    isLoading = true;
    try {
      const [s, r] = await Promise.all([
        getFaceTimingStats(),
        getFaceTimingRecords(200),
      ]);
      stats = s;
      records = r;
    } catch (e) {
      console.error('Failed to load face timing data:', e);
    } finally {
      isLoading = false;
    }
  }

  $effect(() => {
    if (isOpen) {
      loadData();
    }
  });

  async function handleDownloadCsv() {
    try {
      const csv = await exportFaceTimingCsv();
      const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'face_scan_timings.csv';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (e) {
      console.error('Failed to export CSV:', e);
    }
  }

  async function handleCopyPath() {
    if (stats?.log_file_path) {
      await navigator.clipboard.writeText(stats.log_file_path);
      copiedPath = true;
      setTimeout(() => (copiedPath = false), 2000);
    }
  }

  async function handleCopyJson() {
    await navigator.clipboard.writeText(JSON.stringify(records, null, 2));
    copiedJson = true;
    setTimeout(() => (copiedJson = false), 2000);
  }

  async function handleClear() {
    if (confirm('Clear all recorded face scan timings and delete the log file?')) {
      await clearFaceTimingRecords();
      await loadData();
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
    class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4 select-none animate-in fade-in duration-150"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) isOpen = false;
    }}
  >
    <!-- Modal Container -->
    <div
      class="w-full max-w-4xl max-h-[88vh] rounded-xl border shadow-2xl flex flex-col overflow-hidden animate-in zoom-in-95 duration-150"
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
          <Clock class="w-4 h-4 text-amber-400" />
          <h2 class="text-sm font-bold tracking-wide uppercase">Face Scanning Timings & Profiler</h2>
          {#if stats?.is_recording}
            <span class="ml-2 px-1.5 py-0.5 rounded text-[10px] font-bold bg-amber-500/20 text-amber-400 uppercase tracking-wider animate-pulse">
              Recording Active
            </span>
          {/if}
        </div>
        <div class="flex items-center gap-1.5">
          <button
            onclick={loadData}
            class="p-1 rounded text-neutral-400 hover:text-white hover:bg-white/10 transition-colors"
            title="Refresh"
          >
            <RefreshCw class="w-4 h-4 {isLoading ? 'animate-spin' : ''}" />
          </button>
          <button
            onclick={() => (isOpen = false)}
            class="p-1 rounded text-neutral-400 hover:text-white hover:bg-white/10 transition-colors"
            title="Close (Esc)"
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Body -->
      <div class="p-5 flex-1 overflow-y-auto space-y-4 text-xs">
        <!-- Summary Cards -->
        <div class="grid grid-cols-2 sm:grid-cols-5 gap-2.5">
          <div class="p-3 rounded-lg border bg-black/10 flex flex-col justify-between" style="border-color: var(--border-subtle);">
            <span class="text-[10px] uppercase font-semibold text-neutral-400">Total Scanned</span>
            <span class="text-base font-bold font-mono text-white mt-1">{stats?.record_count ?? 0}</span>
          </div>

          <div class="p-3 rounded-lg border bg-black/10 flex flex-col justify-between" style="border-color: var(--border-subtle);">
            <span class="text-[10px] uppercase font-semibold text-emerald-400">Avg Detect</span>
            <span class="text-base font-bold font-mono text-emerald-400 mt-1">
              {stats?.avg_detect_ms ?? 0} <span class="text-[10px] font-normal">ms</span>
            </span>
          </div>

          <div class="p-3 rounded-lg border bg-black/10 flex flex-col justify-between" style="border-color: var(--border-subtle);">
            <span class="text-[10px] uppercase font-semibold text-sky-400">Avg Embed</span>
            <span class="text-base font-bold font-mono text-sky-400 mt-1">
              {stats?.avg_embed_ms ?? 0} <span class="text-[10px] font-normal">ms</span>
            </span>
          </div>

          <div class="p-3 rounded-lg border bg-black/10 flex flex-col justify-between" style="border-color: var(--border-subtle);">
            <span class="text-[10px] uppercase font-semibold text-amber-400">Avg Total / Photo</span>
            <span class="text-base font-bold font-mono text-amber-400 mt-1">
              {stats?.avg_total_ms ?? 0} <span class="text-[10px] font-normal">ms</span>
            </span>
          </div>

          <div class="p-3 rounded-lg border bg-black/10 flex flex-col justify-between" style="border-color: var(--border-subtle);">
            <span class="text-[10px] uppercase font-semibold text-purple-400">Throughput</span>
            <span class="text-base font-bold font-mono text-purple-400 mt-1">
              {stats?.photos_per_second ?? 0} <span class="text-[10px] font-normal">img/s</span>
            </span>
          </div>
        </div>

        <!-- Log File Path banner -->
        {#if stats?.log_file_path}
          <div class="p-2.5 rounded-lg border flex items-center justify-between gap-3 text-[11px] bg-black/15 font-mono" style="border-color: var(--border-subtle);">
            <div class="flex items-center gap-2 truncate text-neutral-300">
              <FileSpreadsheet class="w-3.5 h-3.5 text-neutral-400 shrink-0" />
              <span class="text-neutral-500 shrink-0">Log File:</span>
              <span class="truncate select-all text-neutral-200">{stats.log_file_path}</span>
            </div>
            <button
              onclick={handleCopyPath}
              class="px-2 py-1 rounded border text-[10px] flex items-center gap-1 shrink-0 hover:bg-white/10 transition-colors"
              style="border-color: var(--border-subtle);"
            >
              {#if copiedPath}
                <Check class="w-3 h-3 text-emerald-400" />
                <span class="text-emerald-400">Copied</span>
              {:else}
                <Copy class="w-3 h-3" />
                <span>Copy Path</span>
              {/if}
            </button>
          </div>
        {/if}

        <!-- Records Table -->
        <div class="rounded-lg border overflow-hidden" style="border-color: var(--border-subtle);">
          <div class="overflow-x-auto max-h-[46vh]">
            <table class="w-full text-left font-mono text-[11px]">
              <thead class="sticky top-0 z-10 uppercase text-[9px] font-semibold text-neutral-400 bg-neutral-900 border-b" style="border-color: var(--border-subtle);">
                <tr>
                  <th class="px-3 py-2">Photo</th>
                  <th class="px-2.5 py-2">Source</th>
                  <th class="px-2 py-2">Worker</th>
                  <th class="px-2.5 py-2">Size</th>
                  <th class="px-2.5 py-2 text-right">Decode</th>
                  <th class="px-2.5 py-2 text-right text-emerald-400">Detect</th>
                  <th class="px-2 py-2 text-center">Faces</th>
                  <th class="px-2.5 py-2 text-right text-sky-400">Embed</th>
                  <th class="px-2.5 py-2 text-right">DB</th>
                  <th class="px-3 py-2 text-right text-amber-400">Total</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-neutral-800">
                {#if records.length === 0}
                  <tr>
                    <td colspan="10" class="px-4 py-8 text-center text-neutral-500 font-sans">
                      {stats?.is_recording ? 'Waiting for face scans… timings will appear here automatically.' : 'No timing records logged. Enable "Record Face Scan Timings" in Debug Tools to start recording.'}
                    </td>
                  </tr>
                {:else}
                  {#each records as rec}
                    <tr class="hover:bg-white/5 transition-colors">
                      <td class="px-3 py-1.5 max-w-[180px] truncate text-neutral-200" title={rec.file_name}>
                        {rec.file_name}
                      </td>
                      <td class="px-2.5 py-1.5">
                        <span class="inline-block px-1.5 py-0.5 rounded text-[9px] {rec.is_thumbnail ? 'bg-blue-500/20 text-blue-300' : 'bg-orange-500/20 text-orange-300'}">
                          {rec.is_thumbnail ? 'thumb 256' : 'original'}
                        </span>
                      </td>
                      <td class="px-2 py-1.5 text-neutral-400">#{rec.worker_id}</td>
                      <td class="px-2.5 py-1.5 text-neutral-400">{rec.width}×{rec.height}</td>
                      <td class="px-2.5 py-1.5 text-right text-neutral-400">{rec.decode_ms} ms</td>
                      <td class="px-2.5 py-1.5 text-right font-semibold text-emerald-400">{rec.detect_ms} ms</td>
                      <td class="px-2 py-1.5 text-center {rec.faces_detected > 0 ? 'text-amber-400 font-bold' : 'text-neutral-500'}">
                        {rec.faces_detected}
                      </td>
                      <td class="px-2.5 py-1.5 text-right text-sky-400">{rec.embed_ms} ms</td>
                      <td class="px-2.5 py-1.5 text-right text-neutral-400">{rec.db_save_ms} ms</td>
                      <td class="px-3 py-1.5 text-right font-bold text-amber-400">{rec.total_ms} ms</td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <!-- Footer Actions -->
      <div
        class="px-5 py-3 border-t flex items-center justify-between"
        style="border-color: var(--border-subtle); background-color: var(--header-bg);"
      >
        <div class="flex items-center gap-2">
          <button
            onclick={handleClear}
            disabled={records.length === 0}
            class="px-3 py-1.5 rounded border flex items-center gap-1.5 text-red-400 hover:bg-red-500/20 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            style="border-color: var(--border-subtle);"
            title="Delete all timing records and log file"
          >
            <Trash2 class="w-3.5 h-3.5" />
            <span>Clear Logs</span>
          </button>
        </div>

        <div class="flex items-center gap-2">
          <button
            onclick={handleCopyJson}
            disabled={records.length === 0}
            class="px-3 py-1.5 rounded border flex items-center gap-1.5 hover:bg-white/10 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            style="border-color: var(--border-subtle);"
          >
            {#if copiedJson}
              <Check class="w-3.5 h-3.5 text-emerald-400" />
              <span class="text-emerald-400">Copied</span>
            {:else}
              <Copy class="w-3.5 h-3.5" />
              <span>Copy JSON</span>
            {/if}
          </button>

          <button
            onclick={handleDownloadCsv}
            disabled={records.length === 0}
            class="px-3 py-1.5 rounded border bg-amber-500/20 border-amber-500/50 text-amber-300 font-semibold flex items-center gap-1.5 hover:bg-amber-500/30 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <Download class="w-3.5 h-3.5" />
            <span>Export CSV</span>
          </button>

          <button
            onclick={() => (isOpen = false)}
            class="px-3 py-1.5 rounded border hover:bg-white/10 transition-colors ml-1"
            style="border-color: var(--border-subtle);"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
