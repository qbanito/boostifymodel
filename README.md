# Boostify Dataset Studio

Professional desktop app (Tauri + React + Vite + TailwindCSS) that automatically
turns entire music-video libraries into training-ready datasets for NVIDIA Cosmos
Predict / Transfer, LoRA, NeMo and future Boostify models.

> Point it at a folder or a whole SSD — it scans, indexes, splits, analyzes,
> captions, scores, de-duplicates and exports. The user never writes metadata by hand.

## Stack

- **Shell:** Tauri v2 (Rust) — native filesystem, SQLite, FFmpeg orchestration
- **UI:** React 18 + TypeScript + Vite + TailwindCSS (dark, DaVinci/Lightroom feel)
- **Index:** SQLite (rusqlite, bundled) with SHA-256 dedup — files are never reprocessed
- **Media:** FFmpeg / FFprobe (scene detection, cutting, thumbnails, perceptual hashing)

## Architecture (modular services)

Every stage is an independent module under `src-tauri/src/`:

| Module | Responsibility |
| --- | --- |
| `scanner.rs` | Recursive discovery (MP4/MOV/MXF/BRAW/R3D/ProRes/XAVC…), SHA-256 hashing, artist/project inference |
| `db.rs` | SQLite schema + CRUD (videos, clips, datasets, settings) |
| `probe.rs` | FFprobe metadata + FFmpeg scene-cut detection |
| `splitter.rs` | Clip cutting, thumbnails, 8×8 average-hash, brightness/sharpness probes |
| `ai.rs` | Scene analysis + rich captioning + auto-tagging (NVIDIA NIM hook, heuristic fallback) |
| `pipeline.rs` | Orchestrator: split → analyze → caption → score → dedup → approve |
| `dataset.rs` | Dataset tree + `train/validation/test.jsonl` export (Cosmos/LoRA/NeMo…) |
| `system.rs` | GPU detection (CUDA/Metal/CPU), dependency + storage checks |
| `watch.rs` | Watch mode — auto-index new files as they land |

UI pages: **Dashboard**, **Library**, **Pipeline**, **Review** (Lightroom-style),
**Smart Search**, **Datasets**, **Settings**.

## Prerequisites

- Node 18+ and Rust (stable)
- **FFmpeg + FFprobe** on `PATH` (required for splitting/probing). On macOS:
  `brew install ffmpeg`. You can also set `FFMPEG_PATH` / `FFPROBE_PATH`.

## Develop

```bash
npm install
npm run app:dev      # tauri dev — opens the desktop window
```

## Build installers

```bash
npm run app:build    # .dmg / .app (macOS), .msi / .exe (Windows)
```

## Plugging in real AI models

The pipeline runs fully offline with local heuristics. To upgrade quality:

- **Captions:** add an NVIDIA NIM API key in Settings — wire the call in
  `ai.rs::caption_with_nim` (vision-language model, e.g. `nvidia/vila`).
- **Detection / pose:** run YOLO + MediaPipe as sidecars and feed results into
  `ai.rs::analyze_scene`.
- **Motion signals:** populate `pose/`, `depth/`, `edges/`, `segmentation/`,
  `optical_flow/` sidecars during export in `dataset.rs`.

## Rules honored

- Never reprocess a file already seen (hash + path guard).
- Metadata is always editable in the Review panel.
- Every stage is a separate, swappable service with structured logs.
