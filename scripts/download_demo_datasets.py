#!/usr/bin/env python3
"""Download the 3 demo music-video datasets in full from the Hugging Face Hub.

Each dataset is a `repo_type="dataset"` snapshot pulled into demo-datasets/<name>/.
Run inside the project's .venv-demo:

    source .venv-demo/bin/activate
    python scripts/download_demo_datasets.py
"""
import os
import sys
import time

from huggingface_hub import snapshot_download
from huggingface_hub.utils import GatedRepoError, RepositoryNotFoundError, HfHubHTTPError

REPOS = [
    "mbzmusic/human-music-moves",      # ~12.9 GB  (CONFIDENTIAL - may be gated)
    "Night-Quiet/Music-avqa-video",    # ~51.1 GB
    "FYQ12138/MUSIC_duet_videos",      # small
]

DEST_ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "demo-datasets")


def human(seconds: float) -> str:
    m, s = divmod(int(seconds), 60)
    h, m = divmod(m, 60)
    return f"{h}h{m:02d}m{s:02d}s" if h else f"{m}m{s:02d}s"


def main() -> int:
    os.makedirs(DEST_ROOT, exist_ok=True)
    token = os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_TOKEN")
    results = []
    for repo in REPOS:
        local_dir = os.path.join(DEST_ROOT, repo.replace("/", "__"))
        print(f"\n=== Downloading {repo} -> {local_dir} ===", flush=True)
        t0 = time.time()
        try:
            path = snapshot_download(
                repo_id=repo,
                repo_type="dataset",
                local_dir=local_dir,
                token=token,
                max_workers=4,
            )
            dt = time.time() - t0
            print(f"OK  {repo}  in {human(dt)}  -> {path}", flush=True)
            results.append((repo, "ok", local_dir))
        except GatedRepoError:
            print(f"GATED  {repo}: requires accepting terms / access request. "
                  f"Set HF_TOKEN and request access on the dataset page.", flush=True)
            results.append((repo, "gated", ""))
        except RepositoryNotFoundError:
            print(f"NOT FOUND  {repo}", flush=True)
            results.append((repo, "not_found", ""))
        except HfHubHTTPError as e:
            print(f"HTTP ERROR  {repo}: {e}", flush=True)
            results.append((repo, f"http_error", ""))
        except Exception as e:  # noqa
            print(f"FAILED  {repo}: {type(e).__name__}: {e}", flush=True)
            results.append((repo, "failed", ""))

    print("\n=== SUMMARY ===", flush=True)
    for repo, status, path in results:
        print(f"  {status:10s} {repo} {path}", flush=True)
    ok = sum(1 for _, s, _ in results if s == "ok")
    print(f"\n{ok}/{len(REPOS)} downloaded.", flush=True)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
