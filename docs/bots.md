# 🤖 Bot & AI Agent Guide (`docs/bots.md`)

> **TL;DR for AI Agents:** Read this file before touching code. Check [`docs/decisions/`](decisions/README.md) before changing architecture, and update [`docs/changelog.md`](changelog.md) when you ship a feature or fix.

---

## 🚨 Golden Rules for Bots (Non-Negotiable)

1. **NEVER Hardcode Personal File Paths:**
   - Do not put `/home/<username>/...` or local photo paths in UI components, backend code, or unit tests.
   - In Rust production code, always use `dirs::data_local_dir()` or `dirs::cache_dir()`.
   - In Rust unit tests (`#[cfg(test)]`), always generate synthetic images in memory (`image::DynamicImage::new_rgb8(...)`) or use `tempfile::tempdir()`.
2. **Release vs. Dev Feature Gating (`import.meta.env.DEV`):**
   - Any UI button, tab, or slider that does **not** yet persist changes in the Rust backend **MUST** be wrapped in `{#if import.meta.env.DEV}` so production builds (`npm run tauri build`) only expose working features while dev builds (`npm run tauri dev`) show everything.
   - See [ADR 004: Dev vs. Release Feature Gating](decisions/004-dev-vs-release-feature-gating.md).
3. **Never Block the UI or Image Server Threads:**
   - All heavy image decoding, SIMD resizing, pixel filtering, and ONNX face inference must run asynchronously on background worker threads (`rayon` / `std::thread`), never on the main window loop.
   - Thumbnails and edit step previews must be served via the localhost HTTP server ([`src-tauri/src/server.rs`](../src-tauri/src/server.rs)), never serialized as huge base64 strings over IPC (see [ADR 001](decisions/001-localhost-image-server.md)).
4. **Preserve Original Photos:**
   - Never mutate a user's photo on disk during interactive editing. Interactive edits write to `~/.local/share/pica/edits/<file_hash>/session.json`.
   - Only write to the user's photo folder when `save_edit_session` is called, and always honor the `.pica_originals/` backup mechanism (see [ADR 002](decisions/002-non-destructive-edit-sessions.md)).
5. **Follow the Canonical Design Guide & Keep Docs in Sync:**
   - All UI additions or changes across the Editor and Viewer must adhere to **[`docs/design-guide.md`](design-guide.md)** (darkroom palette, translucent `#18181e]/90` HUD pills, `#3b82f6` Picasa Blue accents, `font-mono` technical readouts, 380ms spring rotation physics, and shared inspector widgets).
   - When you add/complete a feature, update [`docs/changelog.md`](changelog.md).
   - When you make a significant architectural choice, add a short ADR in [`docs/decisions/`](decisions/README.md).

---

## 🧭 Codebase Map for Fast Context Lookup

### Frontend (`src/` — Svelte 5 + TypeScript + Tailwind CSS v4)
- **Entry & Layout:** [`src/main.ts`](../src/main.ts) & [`src/App.svelte`](../src/App.svelte) — Routes between the main Library/Editor window (`App.svelte`) and the standalone frameless Viewer (`PicaPhotoViewer.svelte`), managing global library state, active filters, and IPC event handoff.
- **IPC Bridge:** [`src/lib/api.ts`](../src/lib/api.ts) — Typed wrappers around every `@tauri-apps/api/core` `invoke()` call and localhost image server URL builders (`getThumbnailUrl`, `getPreviewImageUrl`, `getFaceCropUrl`).
- **Types:** [`src/lib/types.ts`](../src/lib/types.ts) — TypeScript interfaces mirroring [`src-tauri/src/models.rs`](../src-tauri/src/models.rs).
- **Components:**
  - [`src/lib/components/TopBar.svelte`](../src/lib/components/TopBar.svelte) — 6-petal Picasa shutter brand, search bar, filter chips, theme switcher, settings trigger, and dev-only `Bug` debug menu.
  - [`src/lib/components/Sidebar.svelte`](../src/lib/components/Sidebar.svelte) — Collapsible Folders tree (flat or nested) and People (face clusters) list with rename/merge/ignore actions.
  - [`src/lib/components/LibraryGrid.svelte`](../src/lib/components/LibraryGrid.svelte) — Virtualized photo grid grouped by folder + Picasa fast-scroll scrubber bar.
  - [`src/lib/components/ImageViewer.svelte`](../src/lib/components/ImageViewer.svelte) — 1-Up & A/B Split viewer, Basic Fixes editing suite, Text compass dial, Crop/Straighten/Redeye/Retouch overlays, and floating HUD.
  - [`src/lib/components/PicaPhotoViewer.svelte`](../src/lib/components/PicaPhotoViewer.svelte) — Standalone frameless (`decorations: false`) eye-candy photo viewer with ambient color glow, floating 6-petal shutter badge, shared `<Histogram />` & `<PropertiesSidebar />`, 380ms spring rotation, floating HUD pill, glass filmstrip scrubber, and "Edit in Pica" handoff.
  - [`src/lib/components/Histogram.svelte`](../src/lib/components/Histogram.svelte) & [`PropertiesSidebar.svelte`](../src/lib/components/PropertiesSidebar.svelte) — Shared RGB luminance histogram and EXIF/Camera inspector drawer used by both the Editor and Standalone Viewer.
  - [`src/lib/components/BottomBar.svelte`](../src/lib/components/BottomBar.svelte) & [`PhotoTray.svelte`](../src/lib/components/PhotoTray.svelte) — Persistent bottom selection tray and thumbnail size slider.

### Backend (`src-tauri/` & Workspace Crates — Rust)
- **State & App Setup:** [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs) — Initializes SQLite schema (`WAL` mode), starts `ImageServer` and `FaceBackgroundWorker`, and registers all `tauri::generate_handler!` commands.
- **Tauri IPC Commands:** [`src-tauri/src/commands.rs`](../src-tauri/src/commands.rs) — All `#[tauri::command]` endpoints called by `src/lib/api.ts`.
- **Non-Destructive Editor Engine:** [`src-tauri/src/editor.rs`](../src-tauri/src/editor.rs) — `EditorManager`, `session.json` step manifest, `.pica_originals/` backup/revert, conflict resolution, and pixel algorithms (Crop suggestions, Hough auto-straighten, Red-eye detection/repair, SpotBlend retouch, Text overlay rendering, Fill Light, Auto Contrast, Auto Color, I'm Feeling Lucky).
- **Thumbnailer & Priority Queue:** [`src-tauri/src/thumbnailer.rs`](../src-tauri/src/thumbnailer.rs) — `ThumbnailManager` and `ThumbnailScheduler` (LIFO viewport priority stack + background pre-generator).
- **Localhost Image Server:** [`src-tauri/src/server.rs`](../src-tauri/src/server.rs) — Multi-threaded `tiny_http` server on `127.0.0.1` serving `/thumb`, `/image`, and `/face_crop`.
- **Face Detection & Clustering Pipeline** *(see [`docs/face-detection.md`](face-detection.md) for full details)*:
  - [`src-tauri/src/faces/worker.rs`](../src-tauri/src/faces/worker.rs) — Background scanning loop, incremental & full re-clustering, and CSV timing profiler.
  - [`src-tauri/src/faces/detector.rs`](../src-tauri/src/faces/detector.rs) — `YuNetFaceDetector` wrapping `yunet_face`.
  - [`src-tauri/src/faces/embedder.rs`](../src-tauri/src/faces/embedder.rs) — 5-point similarity alignment (`112x112`) and MobileFaceNet ArcFace 512-D embeddings.
  - [`src-tauri/src/faces/clustering.rs`](../src-tauri/src/faces/clustering.rs) — `SowGrowFaceClusterer` bridging `picasa_face::SowGrowClusterer`.

---

## ✅ Pre-Commit Verification Checklist for Bots

Run these three commands before finishing any task:

```bash
npm run check
```
```bash
cargo test --manifest-path src-tauri/Cargo.toml
```
```bash
grep -rnI --exclude-dir=target --exclude-dir=node_modules --exclude-dir=dist --exclude-dir=.git -E "/home/" src/ src-tauri/ yunet_face/ picasa_face/
```
*(Note: The `grep` check should only ever match generic placeholder strings like `placeholder="/home/username/Pictures"`—never a real user's home directory!)*
