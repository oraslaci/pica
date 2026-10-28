# ADR 001: Localhost HTTP Image Server (`127.0.0.1`)

> **TL;DR:** We serve all thumbnails, full-res previews, and face crops from a tiny multi-threaded Rust HTTP server on `127.0.0.1` ([`src-tauri/src/server.rs`](../../src-tauri/src/server.rs)) instead of sending base64 strings over Tauri IPC.

- **Status:** Accepted
- **Date:** 2026-10-28

---

## Problem
When scrolling through a grid of 10,000+ photos at 120 FPS, the browser needs to load dozens of thumbnails per frame. Sending binary image data through Tauri's `invoke()` IPC serializes bytes into JSON/base64 strings, which causes massive CPU/memory overhead and stutters the UI thread.

## Decision
On startup, Pica binds a lightweight `tiny_http` server to an ephemeral port on `127.0.0.1` (`http://127.0.0.1:<port>`) with 8 worker threads:
- `/thumb?id=...&path=...&hash=...&size=...` — Streams cached thumbnails or triggers high-priority LIFO generation on the fly.
- `/image?path=...` — Streams full-resolution images or active edit step proxies (`step_001.jpg`).
- `/face_crop?path=...&x=...&y=...&w=...&h=...` — Streams cropped face avatars with `Cache-Control: public, max-age=31536000, immutable`.

## Consequences
- ✅ **Good:** Native browser `<img>` decoding, HTTP caching, and zero JSON serialization overhead. Scrolling is butter-smooth.
- ⚠️ **Trade-off:** Frontend components must always build image URLs via the helpers in [`src/lib/api.ts`](../../src/lib/api.ts) (`getThumbnailUrl`, `getPreviewImageUrl`, `getFaceCropUrl`) after `initImageServerPort()` resolves.
