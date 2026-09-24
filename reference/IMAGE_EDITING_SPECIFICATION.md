# Pica Image Editing Specification & Architecture Document

**Version:** 1.0  
**Status:** Draft for Review & Feedback  
**Target Platform:** Tauri v2 (Rust Backend) + Svelte 5 (Frontend)  
**Authors:** Pica Core Team & Google Antigravity  

---

## 1. Executive Summary & Goals

Pica is an offline-first, ultra-responsive photo management suite inspired by Google Picasa. The image editing subsystem must satisfy four non-negotiable principles:
1. **Snappiness**: Zero-latency interactive editing (instant 60/120 FPS visual feedback in the viewer).
2. **Persistence across Sessions**: Undo/redo history must survive application restarts until explicitly committed ("Saved") or reverted.
3. **Transparent File System Integration**: The user's external file browser must always show the latest edited state, without polluting the database with heavy binary image data.
4. **Safety & Flexibility**: Robust handling of original files (Overwrite vs. Backup) and graceful handling of external modifications (e.g. edits done in external tools like GIMP, Photoshop, or file managers).

---

## 2. Capabilities Analysis: Picasa Heritage & Reference Assets

A detailed audit of the extracted Picasa 3.9 assets (`reference/picasa_original/filterdesc.xml`, `options.fen`, and `ui_assets/`) reveals the complete feature set that defines the iconic editing experience:

### 2.1. The 5-Tab Editing Suite Anatomy
* **Tab 1: Basic Fixes (`basic_icon`)**:
  * **Crop (`crop64`)**: Interactive drag crop with rule-of-thirds grid, custom and fixed aspect ratios (Original, 4:3, 16:9, 1:1 Square, 5:7, 8:10, 8.5×11).
  * **Straighten (`tilt` / `horizonadjust`)**: Continuous horizon leveling slider (-45° to +45°) with alignment grid overlay and automatic zoom-crop to prevent blank corners.
  * **Red-Eye Removal (`redeye`)**: Circular puck-based cursor for local pupil desaturation and darkening.
  * **One-Click Auto Fixes**:
    * *"I'm Feeling Lucky"* (`enhance`): Balanced histogram equalization and warm balance.
    * *Auto Contrast* (`autolight` / `autocontrast`): Shadow/highlight clipping stretch.
    * *Auto Color* (`autocolor`): Gray-world white balance correction.
  * **Fill Light (`fill`)**: Curve-based shadow lifter without blowing out highlights.
  * **Retouch (`retouch`)**: Spot healing clone tool with dual pucks (target & sample).
  * **Text Tool (`edittext`)**: On-canvas typography overlay with rotation and opacity.
* **Tab 2: Fine Tuning (`tuning_icon`)**:
  * **Fill Light**: Midtone and deep shadow illumination.
  * **Highlights**: Specular recovery and highlight compression.
  * **Shadows**: Deep black point clipping and contrast control.
  * **Color Temperature (`colortemp`)**: Kelvin slider (Cooler/Blue $\leftrightarrow$ Warmer/Amber) with neutral gray eyedropper (`dropper_n`).
* **Tabs 3, 4, 5: Creative Effects (36 presets)**:
  * Classic Film: B&W, Sepia, Warmify, Film Grain, Ansel Adams filtered B&W.
  * Lens & Optical: Soft Focus (`radblur`), Focal B&W (`radsat`), Linear Blur, Vignette, Glow.
  * Stylistic / Color: Tint, Graduated Tint, Saturation, Comic Book half-tone, Cinemascope letterbox, Cross Process, Orton Bloom, Lomo, HDR look.
* **Comparison Modes**:
  * `aa_2up_icon`: Side-by-side original vs. original.
  * `ab_2up_icon`: Split A/B comparison (Left: Original untouched, Right: Current live edits) or split-screen wiping curtain.

---

## 3. Original Image Management & Options Menu Design

The user must have complete control over how original photos are handled. This will be configured in a dedicated **Options / Preferences** dialog.

### 3.1. Options Menu Settings

| Setting Key | Type | Default | Options / Description |
| :--- | :--- | :--- | :--- |
| `original_image_handling` | Enum | `backup` | • `backup`: Never destroy original photos.<br>• `overwrite`: Overwrite the original file on Save. |
| `backup_strategy` | Enum | `hidden_folder` | • `hidden_folder`: Picasa-style `.pica_originals/` in the same directory.<br>• `copy_suffix`: Create a new file with suffix (e.g., `IMG_001_edited.jpg`). |
| `hide_originals_in_library`| Boolean | `true` | When copies or backups exist, hide the original untouched files from the Pica library view so the grid is clean. |

### 3.2. Deep Dive: Strategy A (Overwrite) vs. Strategy B (Backup)

#### Strategy A: Overwrite Original (`overwrite`)
1. While editing, a lightweight draft copy is modified or drafted.
2. When the user clicks **Save (Commit)**: The edited pixels are written directly into the original file path.
3. The internal undo history for that session is purged.

#### Strategy B1: Picasa-Style Hidden Backup Folder (`backup` + `hidden_folder`) — **Recommended Default**
1. When editing begins (or at the moment of first save), Pica copies the pristine file into `.pica_originals/` located in the *same folder* as the image (e.g. `/photos/2026/vacation/.pica_originals/beach.jpg`).
2. The active file at `/photos/2026/vacation/beach.jpg` is updated with the user's edits.
3. **Key Advantages**:
   * All external tools, file managers, and cloud sync tools see the updated, edited photo immediately under its canonical name.
   * Zero duplicate cards appear in external folders or in Pica (Pica’s scanner automatically ignores folders starting with `.pica`).
   * Pica can offer a permanent **"Revert to Original"** button at any time in the future, even years later!
   * Compatible with original Picasa conventions (can also read `.picasaoriginals`).

#### Strategy B2: Working Copy / Suffix Mode (`backup` + `copy_suffix`)
1. When editing starts, the original remains at `beach.jpg`. Pica creates `beach_edited.jpg` (or increments `beach_edited_1.jpg`).
2. The user works on `beach_edited.jpg`.
3. In this mode, both files reside side-by-side in the filesystem.

---

## 4. Metadata & EXIF Analysis: How Much Can Be Stored in EXIF?

The user specifically inquired:
> *"how much of this can be stored in the EXIF? Maybe there should be an option to hide the originals in this case"*

### 4.1. What EXIF/XMP Can and Cannot Do

| Capability | Supported in EXIF/XMP? | Implementation Details |
| :--- | :---: | :--- |
| **Track Lineage / Original Reference** | **YES** | Standard Adobe XMP `xmpMM:DerivedFrom` references the original document ID, hash, and filename. We can also write `Xmp.pica.OriginalPath` and `Xmp.pica.IsOriginal = False`. |
| **Preserve Camera EXIF** | **YES** | Shutter speed, Aperture, ISO, Lens, GPS coordinates, and Date Taken can and must be copied faithfully from the original to the edited image. |
| **Tag as "Edited by Pica"** | **YES** | `Exif.Image.Software = "Pica Photo Suite 1.0"`. |
| **Hide file from OS File Manager** | **NO** | Windows Explorer, macOS Finder, and Linux Nautilus/Dolphin **do not** read EXIF tags to determine file visibility. Operating systems only hide files via filesystem-level attributes (dot prefix `.` on Unix, or the Windows `FILE_ATTRIBUTE_HIDDEN` flag). |
| **Hide file inside Pica Application** | **YES** | Pica's SQLite indexer reads the EXIF/XMP metadata and database linkage. If `hide_originals_in_library` is checked, Pica filters out any photo marked as an original that has an active edited version. |

---

## 5. Snappy UX, Persistent Undo/Redo & Internal Storage Architecture

### 5.1. Performance & Snappiness
* **Immediate Client-Side Feedback**: Edits (such as 90° rotation or slider adjustments) must render in the UI viewport **instantly** (0ms latency) using hardware-accelerated CSS transforms / WebGL preview.
* **Asynchronous Processing**: Heavy image encoding (JPEG compression, SIMD rotations) runs in a background thread pool via Rust and never blocks the UI thread.

### 5.2. Persistent Session Storage (Pica Internal Folder)
Rather than bloating the SQLite database (`pica.db`) with large image blobs, Pica will store draft states in the dedicated application data directory:
* **Linux**: `~/.local/share/pica/edits/<file_hash>/`
* **Windows**: `%LOCALAPPDATA%\pica\edits\<file_hash>\`
* **macOS**: `~/Library/Application Support/pica/edits/<file_hash>/`

```
~/.local/share/pica/edits/
└── a3f88b0e7c1.../                 (Base image SHA-256 hash)
    ├── session.json                (Manifest & step history)
    ├── original_meta.json          (File size, mtime, dimensions, EXIF)
    ├── current_preview.jpg         (Fast-render preview of current step)
    └── snapshots/                  (Periodic checkpoint full-res cache)
        ├── step_000.jpg            (Pristine base)
        └── step_003.jpg            (Checkpoint after multiple edits)
```

### 5.3. Session Manifest Schema (`session.json`)
```json
{
  "version": 1,
  "photo_id": 42,
  "file_path": "/home/user/Pictures/2026/DSC_0102.JPG",
  "base_hash": "a3f88b0e7c1...",
  "base_mtime": 1727181045,
  "base_size": 4812904,
  "created_at": 1727181200,
  "current_step_index": 2,
  "is_committed": false,
  "steps": [
    {
      "step_id": "step_0",
      "op": "original",
      "params": {},
      "timestamp": 1727181200
    },
    {
      "step_id": "step_1",
      "op": "rotate",
      "params": { "degrees": 90 },
      "timestamp": 1727181215
    },
    {
      "step_id": "step_2",
      "op": "rotate",
      "params": { "degrees": 90 },
      "timestamp": 1727181220
    }
  ]
}
```

### 5.4. Undo / Redo Lifecycle
1. **User Action (e.g. Rotate 90° CW)**:
   - UI applies CSS transform immediately.
   - IPC call `apply_edit_step(photo_id, op)` appends to `steps` array in `session.json`.
   - Any redo history ahead of `current_step_index` is trimmed (standard undo stack behavior).
   - If the user restarts Pica, the session is reloaded from `session.json`. The user can immediately hit **Undo** (Ctrl+Z) or **Redo** (Ctrl+Y / Ctrl+Shift+Z).
2. **User clicks "Save" (Ctrl+S / Commit)**:
   - Pipeline renders the final image at full resolution.
   - Original is preserved according to the configured backup strategy (e.g., copied to `.pica_originals/` or overwritten).
   - The file on disk receives the final pixels and preserved EXIF metadata.
   - The internal draft session in `~/.local/share/pica/edits/` is marked committed or safely cleaned up.
   - Pica updates the thumbnail and database index.
3. **User clicks "Revert to Original" / "Discard Edits"**:
   - The draft session is discarded.
   - The image reverts to the untouched original state.

---

## 6. External Modification & Inconsistency Handling

Because users may move, rename, or edit images in external applications (Photoshop, GIMP, darktable, exiftool) while an uncommitted session exists in Pica:

### 6.1. Conflict Detection Mechanism
Before loading a session or applying an edit, Pica compares the target file's current filesystem metadata (`size`, `mtime`) against `base_mtime` and `base_size` stored in `session.json`:
1. **Match**: The file is untouched externally. Safe to continue.
2. **Mismatch (Size or Mtime changed)**: Compute quick SHA-256 / partial hash.
   * If the hash matches the uncommitted draft output, Pica was the last writer (normal).
   * If the hash differs from both the original and draft, an **External Inconsistency** has occurred.

### 6.2. Conflict Resolution Flow
When an inconsistency is detected, Pica presents a non-destructive conflict prompt:

```
┌─────────────────────────────────────────────────────────────┐
│  ⚠️ External File Change Detected                           │
│  "beach.jpg" was modified outside of Pica since your draft.  │
├─────────────────────────────────────────────────────────────┤
│  How would you like to proceed?                             │
│                                                             │
│  [1] Keep External Changes                                  │
│      Discard Pica's uncommitted draft session.             │
│                                                             │
│  [2] Re-apply Pica Edits on Top                             │
│      Apply your rotation/edits to the new external image.   │
│                                                             │
│  [3] Save Pica Draft as a New Copy                          │
│      Keep both: creates "beach_pica_draft.jpg".             │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. Extensible Engine Design & Phase 1 Scope

To ensure the architecture is clean and future-proof for future filters, curves, and adjustments, the edit engine is designed around a polymorphic command pattern:

### 7.1. Extensible Operation Model (Rust)
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum EditOperation {
    Rotate { degrees: i32 },                      // Phase 1 (90, 180, 270)
    Crop { x: f32, y: f32, width: f32, height: f32 }, // Phase 2
    Straighten { angle_deg: f32 },               // Phase 2
    Tuning { fill: f32, highlights: f32, shadows: f32, temp: f32 },
    Filter { filter_id: String, intensity: f32 },
}
```

### 7.2. Phase 1 Implementation Target: Rotation Editor & Options
For this initial test milestone, we implement:
1. **Options Dialog**:
   * Quick, elegant modal accessible from the TopBar (Settings gear / options).
   * Configure "Overwrite original" vs "Backup original (`.pica_originals`)".
   * Stored in SQLite `settings` table.
2. **Rotation Editor**:
   * Rotate CW (+90°) and Rotate CCW (-90°).
   * Real-time snappy client viewer rotation.
   * Backend persistence in `~/.local/share/pica/edits/`.
   * Persistent Undo & Redo (survives app restart).
   * Save (Commit) button & Revert button.
   * Conflict detection if file is changed externally.

---

## 8. Open Questions & Recommendations for You

1. **Backup Directory Name**:
   * Should we use `.pica_originals` or `.picasaoriginals`?
   * *Recommendation*: Use `.pica_originals` by default, but automatically recognize existing `.picasaoriginals` folders if a user has an old library previously managed by Google Picasa!
2. **Lossless JPEG Rotation vs. High-Quality Re-encode**:
   * For standard 90°/180°/270° rotations of JPEG files, lossless DCT coefficient rotation is technically possible without recompression loss.
   * *Recommendation*: Use lossless JPEG rotation when rotating unedited JPEGs, and fallback to image pipeline for general filters/crops.
3. **Draft View in OS File Browser**:
   * Should the file on disk be updated on every single click (e.g. each rotation instantly updates the file on disk, with `.pica_originals` preserving the pristine file), OR should the file on disk only be updated when the user clicks "Save"?
   * *Analysis*:
     * In original Picasa, the file on disk was *not* modified until explicit "Save" (Ctrl+S); only Pica showed the edited preview.
     * In your prompt, you noted: *"For the user, if he view the pictures in the file browser, only the last edit should be there, internally however, we save the editing process, so we can go back. When the user 'saves', it means he commits and the internal process can be discarded."*
     * This means writing the current edit state to the file on disk (with an active session in Pica's internal folder allowing undo back to pristine original until "Save" commits).
