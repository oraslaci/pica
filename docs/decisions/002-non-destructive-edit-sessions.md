# ADR 002: Non-Destructive Edit Sessions & `.pica_originals`

> **TL;DR:** Interactive edits are stored as step manifests (`session.json`) and proxy JPEGs in `~/.local/share/pica/edits/<file_hash>/`. Originals are never modified until the user clicks **Save to Disk**, which backs up the untouched original into `.pica_originals/` first.

- **Status:** Accepted
- **Date:** 2026-10-28

---

## Problem
Users want Picasa's signature editing workflow:
1. Instant feedback when clicking Crop, Straighten, Redeye, Retouch, Text, or Fill Light.
2. Full Undo/Redo history that still works even if you close Pica and reopen it next week.
3. Normal folders on disk (no proprietary library lock-in), with zero risk of permanently ruining an original photo.

## Decision
1. **Session Directory (`~/.local/share/pica/edits/<file_hash>/`):**
   - Managed by [`EditorManager`](../../src-tauri/src/editor.rs).
   - Stores `session.json` (ordered list of edit operations, `current_step_index`, base file `mtime`/`size` for conflict detection) alongside rendered step images (`step_001.jpg`, etc.).
2. **Commit & Backup Strategy (`.pica_originals/`):**
   - When the user clicks **Save to Disk (`Ctrl+S`)**, Pica checks the user's backup setting (`backup` by default).
   - In `backup` mode, if `.pica_originals/<filename>` does not exist yet in the photo's folder, Pica copies the pristine original there before writing the edited pixels to the main file path.
   - The folder scanner ([`src-tauri/src/scanner.rs`](../../src-tauri/src/scanner.rs)) automatically skips `.pica_originals` and `.picasaoriginals` directories so backups never clutter the library grid.

## Consequences
- ✅ **Good:** Undo/Redo survives app restarts, external file managers see saved edits immediately, and **Revert to Original** works forever.
- ⚠️ **Trade-off:** Any new editing tool added to [`ImageViewer.svelte`](../../src/lib/components/ImageViewer.svelte) must be implemented as a step in [`src-tauri/src/editor.rs`](../../src-tauri/src/editor.rs) so it participates in the `session.json` pipeline.
