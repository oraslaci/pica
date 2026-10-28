# 🤖 AGENTS.md — Quick Reference for AI Coding Assistants

> **TL;DR:** Welcome, fellow silicon lifeform! Before modifying this codebase, read the full bot manual at **[`docs/bots.md`](docs/bots.md)**, check architectural decisions in **[`docs/decisions/`](docs/decisions/README.md)**, and check recent changes in **[`docs/changelog.md`](docs/changelog.md)**.

## ⚡ 4 Rules You Must Follow

1. **No Personal Paths:** Never commit `/home/<user>/...` paths in code or tests. Use `dirs::data_local_dir()`, `dirs::cache_dir()`, or `tempfile::tempdir()` + synthetic `DynamicImage::new_rgb8` in tests.
2. **Gate Unfinished UI Behind `import.meta.env.DEV`:** If a frontend control does not yet persist changes in the Rust backend (`src-tauri/src/editor.rs` or `commands.rs`), wrap it in `{#if import.meta.env.DEV}` so release builds stay clean and dev builds show work-in-progress features.
3. **Update Docs:** When you ship a feature or architectural change, update [`docs/changelog.md`](docs/changelog.md) and (if applicable) add an ADR in [`docs/decisions/`](docs/decisions/README.md).
4. **Verify Before Finishing:**
   ```bash
   npm run check && cargo test --manifest-path src-tauri/Cargo.toml
   ```

## 📚 Full Documentation Links
- 🏗️ [Architecture Guide (`docs/architecture.md`)](docs/architecture.md)
- 🧠 [Face Detection & Grouping Guide (`docs/face-detection.md`)](docs/face-detection.md)
- 🛠️ [Developer Guide (`docs/development.md`)](docs/development.md)
- 🤖 [Detailed Bot Guide (`docs/bots.md`)](docs/bots.md)
- 🧠 [Architecture Decision Records (`docs/decisions/`)](docs/decisions/README.md)
- 📝 [Changelog (`docs/changelog.md`)](docs/changelog.md)
