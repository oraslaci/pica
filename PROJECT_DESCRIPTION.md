# Pica — The Modern, High-Performance Offline Photo Suite

> Inspired by the speed, simplicity, and iconic design of Google Picasa. Built for collectors, photographers, and anyone with massive local photo libraries.

---

## 1. Executive Summary

**Pica** is a cross-platform, offline-first desktop photo management and viewing application. It reimagines the beloved Google Picasa experience for modern hardware: lightning-fast library scanning, instant thumbnail generation, fluid 60/120 FPS infinite scrolling, non-destructive photo editing, and the iconic Picasa UI elements (floating HUD, Photo Tray, Timeline).

Designed specifically for users with hundreds of gigabytes or terabytes of photos stored locally across internal drives, external USB hard drives, and network shares, Pica requires zero cloud accounts, keeps your file structure intact, and operates with a feather-light system footprint.

---

## 2. Core Pillars & Design Philosophy

| Pillar | Principle | Implementation |
| :--- | :--- | :--- |
| ⚡ **Blazing Speed** | Zero-latency browsing, instant launch, instant zoom. | Multi-threaded I/O, SIMD-accelerated image decoders, tiered caching, virtualized canvas grid. |
| 🔒 **100% Offline & Local** | Your data never leaves your computer. | Local SQLite database, sidecar metadata (`.pica.ini` / XMP), independent of internet access. |
| 🪶 **Lean & Optimized** | Low RAM and disk footprint, no multi-hundred-megabyte runtime bloat. | Native backend engine (Rust) + lightweight Webview2 / WebKit UI (Tauri v2) or native GUI. |
| 🎨 **Iconic, Sleek Aesthetic** | Dark canvas, minimalist UI, floating translucent HUD, Photo Tray. | Polished dark UI with customizable themes, smooth kinetic animations, distraction-free viewing. |
| 🧩 **Plugin Architecture** | Modular, community-extensible without destabilizing the core. | Sandboxed WASM / IPC plugin system for decoders, AI tagging, filters, and exporters. |
| 💻 **Cross-Platform** | Identical experience on Linux, Windows, and macOS. | Cross-platform abstractions for filesystem watching, hardware acceleration, and shortcuts. |

---

## 3. Key Feature Specifications

### 3.1. Library & Ingestion Engine
- **Watch Folders**: Monitor local folders, external drives, and NAS mounts. Automatically detects connected/disconnected external drives.
- **Fast Multi-Threaded Scanning**: Background indexing that never freezes the UI. Fast extraction of EXIF/IPTC/XMP metadata (date taken, camera, lens, ISO, GPS coordinates).
- **Tiered Thumbnail Cache**:
  - *Level 1 (Micro)*: 64x64 blurhashes / compact mipmaps for instantaneous smooth scrolling.
  - *Level 2 (Grid)*: High-quality WebP/JPEG thumbnails (256px / 512px) stored in a compact LMDB/SQLite blob store.
  - *Level 3 (Preview)*: Full-resolution cache on demand.
- **Folder & Album Views**:
  - *Physical View*: Mirrors exact folder hierarchy on disk.
  - *Virtual Albums / Collections*: Group photos across multiple folders without duplicating files.
  - *Timeline View*: Chronological timeline / date scrubber to jump directly to any year or month.

### 3.2. Viewing Experience (The Picasa Heritage)
- **Instant Image Viewer**: Zero-delay transition between grid and single-photo view.
- **Translucent Floating HUD**: Minimalist bottom bar with playback controls, zoom level, rotation, favorite toggle, and photo details.
- **Kinetic Smooth Zoom & Pan**: Fluid mouse wheel / trackpad zoom; **Spacebar / click-and-hold** for 100% pixel inspection.
- **Split Comparison View**: Side-by-side or A/B split-slider to compare edits or duplicate shots.
- **Slideshow Mode**: Fullscreen borderless slideshow with cinematic crossfades and customizable pacing.

### 3.3. Persistent Photo Tray (Bottom Dock)
- **Selection Pinboard**: Select photos from any folder or search result and "hold" them in the tray.
- **Batch Operations**: One-click actions on held photos:
  - Export (resize, watermark, convert format)
  - Batch Rename (custom pattern templates)
  - Create Collage / Contact Sheet
  - Move / Copy / Tag
  - Clear / Hold toggle

### 3.4. Non-Destructive Photo Editing
- Edits are saved as lightweight instructions (e.g. `.pica.ini` or database entries), leaving original files untouched.
- Reversible at any time; original can always be restored.
- **Basic Adjustments**:
  - Crop (freeform, rule of thirds, fixed ratios 4:3, 16:9, 1:1, etc.)
  - Straighten / Leveling tool
  - Auto-Color / Auto-Contrast
  - Red-Eye removal
  - Light tuning: Fill Light, Highlights, Shadows, Color Temperature, Exposure
- **Creative Effects**:
  - B&W, Sepia, Warmify, Vignette, Soft Focus, Film Grain, Tint.

### 3.5. Extensibility & Plugins
- **Plugin Registry**: Discover, install, enable, and configure plugins.
- **Plugin Extension Points**:
  - **Decoders & Formats**: RAW (Canon CR2/CR3, Nikon NEF, Sony ARW), HEIC, AVIF, PSD, SVG, WebP.
  - **AI / Smart Tagging**: Local face recognition / clustering (ONNX Runtime), local CLIP semantic search ("photos with dogs at the beach"), auto-orientation.
  - **Exporters & Integrations**: Web gallery generator, Nextcloud sync, local network sharing, Flickr / Google Photos export.
  - **Custom Filters**: Community GLSL / WebGPU shaders and image processing pipelines.

---

## 4. Technical Architecture Proposal

```
┌─────────────────────────────────────────────────────────────┐
│                 Presentation Layer (UI/UX)                   │
│   • Modern Glassmorphic Dark UI / Floating HUD              │
│   • Virtualized Grid Canvas (WebGL / Canvas2D / Svelte 5)   │
│   • Picasa Photo Tray & Dual Comparison Split View          │
└──────────────────────────────┬──────────────────────────────┘
                               │ IPC / High-Speed Bridge
┌──────────────────────────────▼──────────────────────────────┐
│                    Core Engine (Rust)                       │
│  ┌──────────────────┐  ┌──────────────────┐  ┌────────────┐ │
│  │ Filesystem Watch │  │ Tiered Cache     │  │ Image Proc │ │
│  │ & Ingestion      │  │ & SQLite Index   │  │ & Pipeline │ │
│  └──────────────────┘  └──────────────────┘  └────────────┘ │
└──────────────────────────────┬──────────────────────────────┘
                               │ Plugin Host ABI
┌──────────────────────────────▼──────────────────────────────┐
│              Plugin Ecosystem (WASM / Extism)                │
│   • Camera RAW decoders        • Local Face/CLIP AI         │
│   • Export & Sharing targets   • Custom Shaders/Effects     │
└─────────────────────────────────────────────────────────────┘
```

- **Backend / Core Engine**: Rust (Fast, zero-cost abstractions, multi-threaded SIMD processing, small memory footprint).
- **GUI Application Shell**: Tauri v2 (Native webview wrapper producing ~15MB binaries, using ~50MB RAM compared to Electron's 400MB+).
- **UI Framework**: Svelte 5 or SolidJS + Tailwind CSS + Canvas2D / WebGL image viewport for smooth 120 FPS render loops.
- **Database**: SQLite with `sqlite-vec` or `FTS5` for instant search across millions of metadata entries.
- **Plugin Runtime**: Extism / WebAssembly (WASM) for secure, sandboxed, cross-platform plugins.

---

## 5. Target Milestones

1. **Phase 1 — Core Foundation**: Architecture setup, Rust backend, directory scanning, EXIF extraction, SQLite database, thumbnail generator.
2. **Phase 2 — Picasa Grid & Viewer**: Virtualized grid, instant image loader, floating HUD viewer, keyboard navigation, smooth zoom/pan.
3. **Phase 3 — Photo Tray & Non-Destructive Edits**: Bottom dock selection tray, basic adjustments (crop, straighten, light tuning, presets), sidecar persistence.
4. **Phase 4 — Plugin Host & Advanced Features**: WASM plugin host, batch exporter, timeline view, optional local AI face/search plugin.
