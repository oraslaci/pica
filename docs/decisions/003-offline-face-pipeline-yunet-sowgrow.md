# ADR 003: Offline Face Pipeline (YuNet + MobileFaceNet + Sow & Grow)

> **TL;DR:** Pica detects faces with embedded **YuNet** (`yunet.onnx`), aligns them via 5 facial landmarks to `112×112`, extracts 512-D embeddings with **MobileFaceNet** (`w600k_mbf.onnx`), and groups them using Picasa's two-stage **Sow & Grow** clustering algorithm.

- **Status:** Accepted
- **Date:** 2026-10-28

---

## Problem
Grouping faces across thousands of family photos completely offline is notoriously tricky:
- Simple single-threshold clustering either **false-merges** different family members into one giant mega-cluster (if the threshold is too loose) or **fragments** the same person into 15 tiny clusters (if the threshold is too strict).
- Unaligned bounding boxes cause embedding accuracy to plummet when someone tilts their head.

## Decision
1. **Detection (`yunet_face` crate):**
   - Use **YuNet** (`yunet.onnx` compiled into the binary) as the default detector because it outputs both tight bounding boxes and 5 accurate facial landmarks (`right_eye`, `left_eye`, `nose_tip`, `right_mouth`, `left_mouth`).
2. **5-Point Similarity Alignment & ArcFace Embedding ([`src-tauri/src/faces/embedder.rs`](../../src-tauri/src/faces/embedder.rs)):**
   - Compute a 2D similarity transform (`align_face_chip`) mapping the 5 detected landmarks to ArcFace's canonical `112×112` coordinates before running **MobileFaceNet** (`w600k_mbf.onnx`).
3. **Sow & Grow Clustering ([`picasa_face/src/clustering.rs`](../../picasa_face/src/clustering.rs)):**
   - **Stage 1 (Sow — Strict Core Seeds, threshold `0.60`):** Form high-confidence seed clusters from near-identical face pairs while enforcing a strict **same-photo cannot-link constraint** (two faces in the same photo cannot be the same person) and **mutual Top-K verification**.
   - **Stage 2 (Grow — Centroid Expansion, threshold `0.50`):** Compare remaining unclustered faces against the L2-normalized centroid of each seed cluster, absorbing profile/lighting variations without chaining unrelated people together.

## Consequences
- ✅ **Good:** Zero cloud calls, fast CPU inference via ONNX Runtime (`ort`), and high cluster purity without over-fragmenting people.
- ✅ **Clean Codebase:** Alternative experimental detectors (`BlazeFace`, `PicasaCascadeDetector`) and groupers (`Chinese Whispers`, `Union-Find`) were removed so the codebase maintains a single, well-tested production pipeline.
