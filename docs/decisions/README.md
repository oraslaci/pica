# 🧠 Architecture Decision Records (`docs/decisions/`)

> **TL;DR:** Short, plain-English notes explaining *why* Pica is built the way it is, so neither humans nor AI bots accidentally undo a good idea.

---

## 📋 Decisions Index

| ADR | Title | Status | One-Sentence Summary |
| :--- | :--- | :---: | :--- |
| **[001](001-localhost-image-server.md)** | Localhost HTTP Image Server (`127.0.0.1`) | ✅ Accepted | Stream thumbnails and previews over a local Rust HTTP server instead of Tauri IPC base64 strings so scrolling hits 120 FPS. |
| **[002](002-non-destructive-edit-sessions.md)** | Non-Destructive Edit Sessions & `.pica_originals` | ✅ Accepted | Store edit steps in `~/.local/share/pica/edits/<hash>/session.json` and back up untouched files to `.pica_originals/` on save. |
| **[003](003-offline-face-pipeline-yunet-sowgrow.md)** | Offline Face Pipeline: YuNet + MobileFaceNet + Sow & Grow | ✅ Accepted | Use YuNet for detection, 5-point aligned MobileFaceNet for 512-D embeddings, and Picasa's Sow & Grow algorithm for high-precision clustering. |
| **[004](004-dev-vs-release-feature-gating.md)** | Dev vs. Release UI Feature Gating (`import.meta.env.DEV`) | ✅ Accepted | Hide unimplemented/CSS-only UI controls in release builds using `import.meta.env.DEV` while keeping them visible in dev mode. |

---

## ➕ Copy-Paste Template for a New Decision

Want to add `005-your-decision.md`? Copy this block:

```markdown
# ADR 00X: Short Title

> **TL;DR:** One sentence explaining what we decided and why.

- **Status:** Accepted
- **Date:** YYYY-MM-DD

## Problem
What was broken, slow, or confusing?

## Decision
What did we do to fix it?

## Consequences
- ✅ **Good:** What got better?
- ⚠️ **Trade-off:** What do we have to watch out for?
```
