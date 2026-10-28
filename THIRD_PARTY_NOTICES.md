# 📜 Third-Party Notices & License Audit

> **TL;DR:** Every Rust crate (570+) and NPM package (67) used by Pica is under a standard permissive open-source license (`MIT`, `Apache-2.0`, `BSD`, `ISC`, `Zlib`, `MPL-2.0`). Below is the complete breakdown of our embedded AI models, cascades, and libraries.

---

## 1. Embedded AI Models & Cascade Data

| File Path | Component / Origin | License | Notes |
| :--- | :--- | :---: | :--- |
| `yunet_face/models/yunet.onnx` | **YuNet Face Detector** (`libfacedetection` / OpenCV Zoo by Shiqi Yu) | **MIT License** | Permissive; allows any use with attribution. |
| `src-tauri/models/arcface_mbf.onnx` | **InsightFace MobileFaceNet (`w600k_mbf.onnx`)** | **InsightFace Non-Commercial Research License** | Pre-trained on WebFace600K; free for non-commercial research and personal use (matching Pica's own non-commercial license). Commercial use requires a separate license from InsightFace or substituting an Apache-2.0 embedder (e.g., OpenCV Zoo SFace). |

---

## 2. Procedural UI Icons (`src/assets/picasa/*.png`)

All 18 tool preview icons in `src/assets/picasa/` (`crop`, `straighten`, `redeye`, `lucky`, `contrast`, `color`, `retouch`, `text`, `fill_light` at `1x` and `@2x`) are **100% original procedural artwork** generated mathematically via Python (`numpy` + `Pillow`) for the Pica project and licensed under the repository's [`LICENSE`](LICENSE). No proprietary Google Picasa image assets or binary weights are distributed in this repository.

---

## 3. Rust Crates (`src-tauri/Cargo.toml`, `yunet_face/Cargo.toml`, `picasa_face/Cargo.toml`)

All direct and transitive Rust dependencies use permissive open-source licenses:
- **`tauri`, `tauri-build`, `tauri-plugin-dialog`, `tauri-plugin-log`**: `MIT OR Apache-2.0`
- **`rusqlite`**: `MIT`
- **`image`, `fast_image_resize`, `rayon`, `walkdir`, `dirs`, `sha2`, `chrono`, `percent-encoding`, `tiny_http`, `base64`, `byteorder`**: `MIT` / `Apache-2.0` / `BSD-2-Clause`
- **`kamadak-exif`**: `BSD-2-Clause`
- **`ort` (ONNX Runtime Rust bindings)**: `MIT OR Apache-2.0`

---

## 4. Frontend NPM Packages (`package.json`)

All direct and transitive frontend dependencies use permissive open-source licenses:
- **`svelte`, `@sveltejs/vite-plugin-svelte`, `svelte-check`, `vite`**: `MIT`
- **`tailwindcss`, `@tailwindcss/vite`**: `MIT` (with `lightningcss` under `MPL-2.0`)
- **`@lucide/svelte`**: `ISC`
- **`@tauri-apps/api`, `@tauri-apps/cli`, `@tauri-apps/plugin-dialog`, `@tauri-apps/plugin-shell`**: `MIT OR Apache-2.0`
- **`typescript`**: `Apache-2.0`
