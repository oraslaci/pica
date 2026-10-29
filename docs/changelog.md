# 📝 Pica Changelog

> **TL;DR:** A chronological log of what works, what changed, and what got cleaned up. Both humans and bots ([`docs/bots.md`](bots.md)) rely on this file to track project progress.

---

## [0.1.0] — 2026-10-28 (Initial Public GitHub Release Prep)

### 🎨 Branding & Visuals
- **Picasa-Inspired Shutter Brand & Logo:** Replaced the legacy purple icon with a high-resolution camera shutter emblem inspired by Google Picasa's iconic multi-colored aperture blades and the in-app top bar.
- **Cross-Platform App Icons:** Regenerated all native app icon formats (`icon.ico`, `icon.icns`, 512px down to 32px PNGs, Windows Square assets) matching the new aperture design.
- **README & Assets:** Added centered brand banner with official app icon (`assets/logo.png`) and Shields.io status badges (latest release, cross-platform build CI, downloads, non-commercial license, Tauri v2, Rust 2021, Svelte 5).

### 🧹 Cleaned Up & Public-Ready
- **Removed Benchmark & Debug Clutter:** Deleted local benchmark HTML dumps (`benchmark_comparison.html`, `benchmark_yunet_comparison.html`), debug alignment PNGs, `wine_python32/`, `rust_face_cli/`, extracted `reference/` assets, and crate benchmark examples (`picasa_face/examples/`, `yunet_face/examples/`, `src-tauri/examples/`).
- **Sanitized Personal Paths:** Removed all `/home/...` personal paths from [`FolderManagerModal.svelte`](../src/lib/components/FolderManagerModal.svelte), [`yunet_face/src/detector.rs`](../yunet_face/src/detector.rs), and Rust unit tests across `thumbnailer.rs`, `editor.rs`, `detector.rs`, `embedder.rs`, and `clustering.rs`.
- **Release vs. Dev Feature Gating (`import.meta.env.DEV`):**
  - Hid unimplemented UI controls in production release builds (`npm run tauri build`) while keeping them visible in development mode (`npm run tauri dev`):
    - [`ImageViewer.svelte`](../src/lib/components/ImageViewer.svelte): Tabs 2–5 (`Tuning`, `Effects`, `Styles`, `Frames`) and Top-Right `Debug HUD` toggle.
    - [`TopBar.svelte`](../src/lib/components/TopBar.svelte): Top-Right `Bug` Debug Tools dropdown menu.
    - [`PhotoTray.svelte`](../src/lib/components/PhotoTray.svelte): Bottom Photo Tray `Export` button.
    - [`LibraryGrid.svelte`](../src/lib/components/LibraryGrid.svelte): Thumbnail card hover Rotate CCW/CW buttons.
- **Documentation Suite:** Added ADHD-friendly, one-click copy-paste documentation in [`README.md`](../README.md), [`AGENTS.md`](../AGENTS.md), [`docs/architecture.md`](architecture.md), [`docs/development.md`](development.md), [`docs/bots.md`](bots.md), and [`docs/decisions/`](decisions/README.md).
- **License & Clean-Room Asset Audit:**
  - Added [`LICENSE`](../LICENSE) (Pica Permissive Non-Commercial License) and [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
  - Removed unused `picasa_face/assets/red_weights.bin` (`2.28 MB`) and `src-tauri/models/sface.onnx` (`9.9 MB`).
  - Replaced `src/assets/picasa/*.png` with 18 clean-room procedurally generated tool preview icons.
- **Dead Code, Dependencies & Compiler Warnings Cleanup:**
  - Consolidated the face pipeline to the single winning stack (**YuNet** detector + **MobileFaceNet** embedder + **Sow-Grow** grouper), deleting alternative experimental detectors (`BlazeFaceFullDetector`, `src-tauri/models/blazeface_full.onnx`, `PicasaCascadeDetector`, `picasa_face/src/detector.rs`, `picasa_face/src/geometry.rs`) and groupers (`Chinese Whispers`, `Union-Find`).
  - Removed unused crates/packages (`ureq`, `tokio`, `lucide-svelte`, `@tauri-apps/plugin-shell`), unused experimental modules (`picasa_face` Gabor/HOG/similarity/adapter, `SpatialTextureEmbedder`), and unused IPC commands (`get_photos_for_folder`, `get_all_photos`, `download_face_models`, `set_face_engines`).
  - Stopped `ModelManager::new()` from unpacking duplicate `.onnx` files to `~/.local/share/pica/models/faces/` on startup since models load directly from memory (`include_bytes!`).
  - Fixed all 14 Svelte 5 reactivity and accessibility warnings (`0 errors and 0 warnings` in `npm run check`).

### ✨ Core Features Included in `0.1.0`
- **Library & Fast Grid (`scanner.rs`, `thumbnailer.rs`, `server.rs`, `LibraryGrid.svelte`):**
  - Multi-threaded folder scanning with SHA-256 hashing and EXIF metadata extraction.
  - 3-tier SIMD thumbnail generation (`64px` EXIF fast-path, `240px` standard, `480px` Retina) with dual-queue LIFO viewport prioritization.
  - Custom localhost HTTP streaming server (`127.0.0.1`) for zero-lag image and face crop loading.
  - Virtualized folder/date grid with Picasa's signature right-hand timeline scrubber and live preview badge.
- **Non-Destructive Basic Fixes Suite (`editor.rs`, `ImageViewer.svelte`):**
  - Persistent draft sessions (`~/.local/share/pica/edits/<hash>/session.json`) with Undo/Redo across restarts.
  - Safe saving with `.pica_originals/` backup folder and external modification conflict detection.
  - **Crop:** Aspect ratio presets, rule-of-thirds overlay, 3 smart composition suggestions, and animated cutout preview.
  - **Straighten:** `-45°` to `+45°` horizon leveling with automatic edge-safe zoom and Hough-transform auto-level suggestion.
  - **Redeye:** Auto-detection via face landmarks + manual click/drag pupil desaturation brush.
  - **One-Click Auto Fixes:** *I'm Feeling Lucky*, *Auto Contrast*, and *Auto Color*.
  - **Retouch:** Picasa SpotBlend dual-circle healing brush with frequency-aware boundary feathering.
  - **Text Tool:** On-canvas typography with Picasa's iconic circular compass dial for rotation & scale, outline stations, and transparency.
  - **Fill Light:** Non-linear shadow boost slider with real-time SVG curve preview and backend pixel baking.
- **100% Offline Face Detection & Recognition (`faces/`, `yunet_face/`, `picasa_face/`):**
  - Embedded YuNet (`yunet.onnx`) 5-landmark face detection + MobileFaceNet (`w600k_mbf.onnx`) 512-D embeddings.
  - Picasa "Sow & Grow" dual-threshold clustering (`0.60` strict seed / `0.50` centroid expansion) with same-photo co-occurrence constraint.
  - Sidebar People manager supporting naming, merging, ignoring, and reassigning faces.
