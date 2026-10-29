# ADR 006: Built-in GitHub Update Checker & In-Place Self-Updating

> **TL;DR:** Integrated an automated GitHub release checker, Tauri v2 in-place self-updater (`@tauri-apps/plugin-updater`), and modern notification system so users can update seamlessly with in-place download, verification, and automatic restart.

- **Status:** Accepted
- **Date:** 2026-10-29

## Problem
Desktop applications distributed across Linux, Windows, and macOS require a smooth mechanism to stay up-to-date without forcing users to manually visit web pages, re-download installers, and manually replace executables.

## Decision
1. **Dual Update Engine:**
   - **In-Place Self-Updater:** Integrated `@tauri-apps/plugin-updater` and `tauri-plugin-updater v2` with cryptographic signature verification (`minisign`), live download chunk progress reporting, and in-place application replacement with one-click app restart (`app_handle.restart()`).
   - **GitHub Releases API Fallback:** Directly queries `https://api.github.com/repos/oraslaci/pica/releases/latest` on startup and on demand for users running standalone builds or web mode.
2. **Modern Notice UX:**
   - In `TopBar.svelte`, a sleek pill button with a pulsing emerald radar beacon indicator appears when an update is available: `[ ✦ Update available v0.3.0 ]`.
   - The Options/Settings button displays a matching status beacon.
   - Clicking either opens the Software Update modal dialog.
3. **One-Click In-Place Update & Direct Download:**
   - Primary CTA: **"Update and Restart Automatically"** (or simulated in dev mode), displaying a real-time download progress bar and "Restart Pica Now" button upon completion.
   - Secondary CTA: Direct download for the user's OS architecture (`.AppImage`, `.deb`, `.rpm`, `.exe`, `.dmg`).
4. **CI Release Workflow & Key Signing:**
   - Configured `.github/workflows/release.yml` with `TAURI_SIGNING_PRIVATE_KEY` for automated release bundle signing and manifest (`latest.json`) publishing.
   - Configured public key in `tauri.conf.json`.

## Consequences
- ✅ **Good:** True in-place self-updating with zero disruption to user workflow.
- ✅ **Good:** Complete fallback to direct platform installer downloads if offline or running in standalone/web mode.
- ✅ **Good:** Built-in dev testing buttons for verifying notifications, progress bars, and restart sequences.
