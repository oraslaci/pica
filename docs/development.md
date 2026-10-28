# 🛠️ Pica Developer Guide (Copy, Paste, Done)

> **TL;DR:** Every command in this file is pre-filled so you only have to click the **Copy** button once and paste it into your terminal. No `<replace-this>` homework unless unavoidable.

---

## 1. 📦 Install System Prerequisites (Once Per Machine)

Pica uses **Tauri v2** (which uses your OS's native webview instead of bundling a 200 MB copy of Chrome) and **Rust**.

### Ubuntu / Debian / Linux Mint
```bash
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

### Fedora
```bash
sudo dnf install -y webkit2gtk4.1-devel openssl-devel curl wget file libappindicator-gtk3-devel librsvg2-devel libxdo-devel gcc-c++
```

### Arch Linux / Manjaro
```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg xdotool
```

### Don't Have Rust Yet?
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y && source "$HOME/.cargo/env"
```

---

## 2. 🚀 Daily Dev Commands (One-Click Copy)

### Install JS Packages & Start Pica in Dev Mode
*(Shows all experimental features: Editor Tabs 2–5, Export button, and the TopBar Bug Debug Menu)*
```bash
npm install && npm run tauri dev
```

### Build a Production Release App
*(Hides all unimplemented features via `!import.meta.env.DEV` and outputs optimized `.deb` / `.AppImage` / binary)*
```bash
npm run tauri build
```

### Run Frontend Typecheck (Svelte 5 + TypeScript)
```bash
npm run check
```

### Run Rust Backend Unit Tests
```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

### Run Face Engine Crate Tests (`yunet_face` & `picasa_face`)
```bash
cargo test --manifest-path yunet_face/Cargo.toml && cargo test --manifest-path picasa_face/Cargo.toml
```

---

## 3. 🗂️ Where Does Pica Store Stuff on My Computer?

> **TL;DR:** Pica never pollutes your photo folders except for a hidden `.pica_originals/` backup folder when you save edits. Everything else lives in standard Linux XDG directories.

| What It Is | Path on Linux |
| :--- | :--- |
| **SQLite Database (`pica.db`)** | `~/.local/share/pica/pica.db` |
| **Non-Destructive Edit Sessions** | `~/.local/share/pica/edits/<file_hash>/` |
| **Downloaded AI Face Models** | `~/.local/share/pica/models/` |
| **Face Scan Timing Logs (CSV)** | `~/.local/share/pica/logs/face_scan_timings.csv` |
| **Generated Thumbnail Cache** | `~/.cache/pica/thumbnails/` |

### 🧹 Useful One-Click Reset Commands

**Wipe ONLY the thumbnail cache** *(safe — Pica will regenerate them automatically)*:
```bash
rm -rf ~/.cache/pica/thumbnails/*
```

**Wipe ONLY unsaved edit drafts** *(keeps your database and saved photos untouched)*:
```bash
rm -rf ~/.local/share/pica/edits/*
```

**Nuclear Reset: Wipe the entire Pica database & caches** *(fresh first-launch state; does NOT delete your actual photos)*:
```bash
rm -rf ~/.local/share/pica/pica.db* ~/.local/share/pica/edits ~/.cache/pica/thumbnails
```

---

## 4. 🧪 How to Unlock a Hidden Feature for Release

Right now, unfinished UI controls are gated behind `import.meta.env.DEV` (see [ADR 004](decisions/004-dev-vs-release-feature-gating.md)).

When you finish implementing the Rust backend for a feature (for example, baking the Tab 2 Tuning sliders into [`src-tauri/src/editor.rs`](../src-tauri/src/editor.rs)):
1. Wire the frontend button/slider in [`src/lib/components/ImageViewer.svelte`](../src/lib/components/ImageViewer.svelte) to call your new Tauri command in [`src/lib/api.ts`](../src/lib/api.ts).
2. Remove the `{#if import.meta.env.DEV}` wrapper around that UI element.
3. Update [`docs/changelog.md`](changelog.md) so humans and bots know it's live in release!
