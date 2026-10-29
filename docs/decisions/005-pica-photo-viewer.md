# ADR 005: Pica Photo Viewer (Standalone Multi-Window Viewer)

> **TL;DR:** Inspired by the iconic Picasa Photo Viewer (`PicasaPhotoViewer.exe`), we implement a standalone, lightweight, ultra-slick viewer window in Tauri v2 (`label: "viewer"` + `?mode=viewer`), complete with frosted glass dock, cursor-centered pan/zoom, sibling directory discovery, and instant "Edit in Pica" handoff.

- **Status:** Accepted
- **Date:** 2026-10-29

---

## Problem
In classic Google Picasa, users relied heavily on the separate **Picasa Photo Viewer** for fast, distraction-free photo browsing with a dark translucent backdrop, sleek floating dock, and quick transition into the full Picasa editor. Previously, Pica only had an integrated in-window modal editor/viewer, preventing lightweight standalone photo viewing or external file association launches without starting the full library grid.

## Decision
1. **Multi-Window Tauri v2 Architecture:**
   - Dedicated window labeled `"viewer"` managed via `WebviewWindowBuilder`.
   - Entry routing in `src/main.ts` routes between the main manager (`App.svelte`) and the viewer (`PicaPhotoViewer.svelte`) using `?mode=viewer` or CLI arguments.
   - CLI support in `main.rs` & `lib.rs`: running `pica <file>` or `pica --viewer <file>` automatically launches directly into the standalone viewer without opening the full photo library grid.
2. **Sibling Directory Discovery:**
   - Background folder scan via `commands::get_viewer_folder_context_inner` discovers all adjacent image files in the parent folder, sorts them naturally, and cross-references them against `pica.db`.
   - Populates the bottom retractable filmstrip thumbnail scrubber even for photos outside Pica's watched folders.
3. **Unified Pica Darkroom Design Language & Frameless Window ([`docs/design-guide.md`](../design-guide.md)):**
   - **Frameless Canvas (`decorations: false`):** No native OS title bar and no full-width top header bar—the photo occupies 100% of the window edge-to-edge.
   - **Ambient Photo-Color Glow:** A softly blurred, low-opacity duplicate of the current photo (`blur-[90px] saturate-150 opacity-25`) tints the `#0e0e11` darkroom canvas dynamically.
   - **Floating Corner Pills (`bg-[#18181e]/90 border-[#3f3f4c]/80`):**
     - Top-Left draggable identity badge (`data-tauri-drag-region`) featuring the 6-petal colorful Picasa shutter SVG, filename, `idx / total` counter, and quick camera/lens/ISO summary.
     - Top-Right floating window & inspector capsule with toggles for the live RGB `<Histogram />` (`H`), `<PropertiesSidebar />` (`I`), Fullscreen (`F11`), Minimize, Maximize, and Close.
   - **Shared Inspector Components:** Reuses the exact same `<Histogram />` and `<PropertiesSidebar />` components as `ImageViewer.svelte` so the Viewer and Editor feel like one cohesive studio suite.
   - **Two-Tier Bottom Console:**
     - Tier 1: Compact floating HUD pill matching `ImageViewer.svelte` (`Prev/Next`, `Fit`/`1:1`, `.picasa-station-slider` zoom, `CCW/CW` 380ms spring rotation, Gold Star toggle, Slideshow, Filmstrip toggle, and primary blue **Edit in Pica** CTA).
     - Tier 2: Floating glass filmstrip thumbnail deck (`rounded-2xl`) with blue ring active highlight and star/edited badges.
4. **Close Ties to Pica ("Edit in Pica"):**
   - Prominent **"Edit in Pica"** button on the floating dock.
   - Tauri IPC event bridge (`open_in_pica_editor` command + `open-in-pica-editor` event): focuses the main Pica window, navigates to the target photo, and immediately opens the full non-destructive editor suite.
   - Inside Pica's 1-Up viewer, a "Pop out into Pica Photo Viewer" button allows opening the photo in the standalone viewer anytime.

## Consequences
- ✅ **Good:** Blazing fast image viewing, true Picasa look and feel, standalone CLI launch capability, seamless two-way integration with Pica's editor and database.
- ⚠️ **Trade-off:** Multi-window Tauri environments require window existence checks (`get_webview_window`) and event handoff coordination.
