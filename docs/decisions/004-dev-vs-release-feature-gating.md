# ADR 004: Dev vs. Release UI Feature Gating (`import.meta.env.DEV`)

> **TL;DR:** Unimplemented or debug-only UI features are wrapped in `{#if import.meta.env.DEV}` so release builds (`npm run tauri build`) only show 100% functional tools while development builds (`npm run tauri dev`) still display work-in-progress features.

- **Status:** Accepted
- **Date:** 2026-10-28

---

## Problem
Several UI areas were built ahead of their Rust backend persistence (such as the CSS-only filter previews in Editor Tabs 2–5 and the placeholder Export button in the Photo Tray), alongside developer-only diagnostic tools (the TopBar `Bug` menu and ImageViewer `Debug HUD`). Shipping non-functional buttons in a public release confuses users, while deleting the code outright would throw away valuable UI work needed during active development.

## Decision
Use Vite's built-in compile-time constant `import.meta.env.DEV` inside Svelte templates:
- When running `npm run tauri dev`, `import.meta.env.DEV === true`, so all experimental tabs, buttons, and debug menus render normally.
- When running `npm run tauri build`, `import.meta.env.DEV === false`, so Vite dead-code-eliminates those blocks from the production bundle.

### Currently Gated Elements
1. **[`ImageViewer.svelte`](../../src/lib/components/ImageViewer.svelte):** Editor Tabs 2–5 (`Tuning`, `Effects`, `Styles`, `Frames`) and the top-right `Debug HUD` button.
2. **[`TopBar.svelte`](../../src/lib/components/TopBar.svelte):** Top-right `Bug` Debug Tools menu.
3. **[`PhotoTray.svelte`](../../src/lib/components/PhotoTray.svelte):** Bottom tray `Export` button.
4. **[`LibraryGrid.svelte`](../../src/lib/components/LibraryGrid.svelte):** Thumbnail card hover Rotate CCW/CW buttons.

## Consequences
- ✅ **Good:** Public release builds feel rock-solid with zero "fake" buttons, and developers keep 100% of their experimental UI and profiling tools in `tauri dev`.
- ⚠️ **Trade-off:** Whenever a contributor finishes wiring a gated feature to the Rust backend, they should remove its `{#if import.meta.env.DEV}` guard and record the graduation in [`docs/changelog.md`](../changelog.md).
