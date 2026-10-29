<div align="center">
  <img src="assets/logo.png" alt="Pica Logo" width="128" height="128" />
  <h1>Pica</h1>
  <p><strong>A blazing-fast, 100% offline desktop photo manager and non-destructive editor.</strong></p>
  <p><em>Inspired by the legendary Google Picasa 3.9 — rebuilt from scratch with Rust (Tauri v2) and Svelte 5.</em></p>

  <p>
    <a href="https://github.com/oraslaci/pica/releases"><img src="https://img.shields.io/github/v/release/oraslaci/pica?color=7e14ff&label=latest%20release&logo=github" alt="Latest Release"></a>
    <a href="https://github.com/oraslaci/pica/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/oraslaci/pica/release.yml?branch=main&label=build&logo=githubactions&logoColor=white" alt="Build Status"></a>
    <a href="https://github.com/oraslaci/pica/releases"><img src="https://img.shields.io/github/downloads/oraslaci/pica/total?color=blue&logo=github" alt="Downloads"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-Non--Commercial-purple" alt="License"></a>
  </p>

  <p>
    <a href="https://tauri.app"><img src="https://img.shields.io/badge/Tauri-v2-24C8DB?logo=tauri&logoColor=white" alt="Tauri v2"></a>
    <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-2021-DEA584?logo=rust&logoColor=black" alt="Rust"></a>
    <a href="https://svelte.dev"><img src="https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white" alt="Svelte 5"></a>
    <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-informational?logo=linux&logoColor=white" alt="Cross-Platform">
  </p>
</div>

<br>

> [!WARNING]
> **Disclaimer:** This is 100% vibecoded, with good intentions using gemini. Take it with a grain of salt and don't hold me responsible, if this turns into skynet. You're also welcome to contribute. The intention is to recreate a really awesome piece of software, that let you manage your own stuff without a paid subscription. The goal is to make it just as slick and fast as the original.

---

## ⚡ Why Does This Exist?

Remember when photo apps just opened **instantly**, let you scroll through **20,000 photos at 120 FPS** without lagging, fixed red-eye and crooked horizons in two clicks, grouped your friends' faces *without* uploading your life to a paid cloud subscription, and never held your folders hostage?

That was **Google Picasa**. Google retired it in 2016. We missed it, so we're building **Pica**.

- 🏠 **100% Local & Offline:** Zero cloud accounts, zero telemetry, zero subscriptions. Your photos stay in your folders where you put them.
- 🚀 **Stupidly Fast:** Rust backend + SIMD thumbnailer + virtualized Svelte 5 grid + custom localhost streaming server = butter-smooth scrolling.
- 🛡️ **Non-Destructive Editing:** Crop, straighten, fix lighting, heal blemishes, or slap text on a photo. Undo/redo works even after you close and reopen the app. Originals are safely backed up in `.pica_originals/`.
- 🧠 **Offline Face Grouping:** Detects and groups faces locally on your CPU using embedded neural nets (YuNet + MobileFaceNet) and Picasa's classic "Sow & Grow" clustering algorithm.

---

## ✅ What Works Right Now vs. 🚧 What's Cooking

> **Note:** Unfinished/experimental features are **hidden in Release builds** so normal humans get a clean, working app, and **visible in Dev mode** (`npm run tauri dev`) so contributors can hack on them.

| Feature | Release Build | Dev Mode | What It Does |
| :--- | :---: | :---: | :--- |
| **Folder Watcher & Fast Scanner** | ✅ | ✅ | Scans your photo folders with multi-threaded Rust + EXIF extraction |
| **120 FPS Virtualized Grid & Scrubber** | ✅ | ✅ | Grouped by folder/date with Picasa's fast-scroll timeline scrubber |
| **Basic Fixes Suite (Tab 1)** | ✅ | ✅ | **Crop** (with smart suggestions), **Straighten** (auto-horizon), **Redeye**, **I'm Feeling Lucky**, **Auto Contrast**, **Auto Color**, **Retouch** (SpotBlend brush), **Text** (with Picasa's compass dial!), and **Fill Light** |
| **Persistent Undo / Redo & Backups** | ✅ | ✅ | Survives app restarts via `session.json`; backs up untouched files to `.pica_originals/` |
| **1-Up & A/B Split Viewer + Histogram** | ✅ | ✅ | Zoom, pan, live RGB histogram, EXIF "Info for Nerds" drawer, and before/after split view |
| **Offline Face Detection & People View** | ✅ | ✅ | Finds faces, groups them into People albums, lets you name, merge, or ignore clusters |
| **Fine Tuning Sliders (Tab 2)** | 🙈 Hidden | 🧪 Visible | Live CSS preview for Highlights, Shadows, Color Temp (backend baking still WIP) |
| **36 Creative Effects / Frames (Tabs 3–5)** | 🙈 Hidden | 🧪 Visible | Live CSS preview for Sepia, Lomo, Polaroid, Holga, etc. (backend baking still WIP) |
| **Batch Export Tray Button** | 🙈 Hidden | 🧪 Visible | UI button ready in Photo Tray; batch disk export pipeline still WIP |
| **Developer Debug Menu & HUD** | 🙈 Hidden | 🧪 Visible | Top-bar `Bug` menu for switching face detectors, profiling CSV timings, and inspecting edit steps |

---

## 🏃‍♂️ One-Click Quickstart (Linux)

**TL;DR:** Install the Tauri system libraries once, then run one command to launch Pica.

### 1. Install System Dependencies (Ubuntu / Debian)
Click the copy button on the top-right of this box and paste into your terminal:

```bash
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

*(On Fedora or Arch? Grab the one-click command in [docs/development.md](docs/development.md)!)*

### 2. Clone & Launch in Development Mode
```bash
git clone https://github.com/oraslaci/pica.git && cd pica && npm install && npm run tauri dev
```

### 3. Build a Clean Release Binary (Hides Unfinished UI)
```bash
npm run tauri build
```

---

## 📚 Documentation Map (ADHD & Bot Friendly)

Hate reading long docs? Every file in [`docs/`](docs/) starts with a **TL;DR**, uses plain English, and has pre-filled copy-paste commands:

- 🏗️ **[Architecture Guide (`docs/architecture.md`)](docs/architecture.md)** — How the Rust engine, localhost image server, non-destructive editor, and face AI work together.
- 🧠 **[Face Detection & Grouping (`docs/face-detection.md`)](docs/face-detection.md)** — Deep dive (in plain English) into YuNet, 5-point alignment, MobileFaceNet embeddings, and Picasa's "Sow & Grow" clustering.
- 🛠️ **[Developer Guide (`docs/development.md`)](docs/development.md)** — One-click commands to run, test, debug, and reset caches.
- 🤖 **[Bot & Agent Manual (`docs/bots.md`)](docs/bots.md)** (and [`AGENTS.md`](AGENTS.md)) — Strict rules, file maps, and IPC contracts for AI coding assistants.
- 🧠 **[Architecture Decisions (`docs/decisions/`)](docs/decisions/README.md)** — Short ADRs explaining *why* we built things the way we did.
- 📝 **[Changelog (`docs/changelog.md`)](docs/changelog.md)** — What changed and when.
- 📜 **[License (`LICENSE`)](LICENSE) & [Third-Party Notices (`THIRD_PARTY_NOTICES.md`)](THIRD_PARTY_NOTICES.md)** — Permissive Non-Commercial License (do almost anything, contact the author if you want to make a profit).

---

## 🤝 Contributing

Found a bug? Want to wire up the Rust pixel shader for the 36 Creative Effects or build the Batch Export pipeline?
1. Check out [docs/development.md](docs/development.md) and [docs/architecture.md](docs/architecture.md).
2. Point your favorite AI coding assistant (or your own fingers) at [AGENTS.md](AGENTS.md).
3. Open a PR!

---

## ⚖️ License

**TL;DR:** Pica is licensed under the **[Pica Permissive Non-Commercial License (`LICENSE`)](LICENSE)**. You are free to use, modify, fork, and share it for almost anything (personal, academic, open-source, or non-profit use). **If you want to make commercial profit from this software, you must contact the author first.** See [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md) for details on third-party libraries and models.

