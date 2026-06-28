#!/usr/bin/env python3
"""
HuMo-17B inference HTTP server (single H100).

Wraps the proven single-GPU inference path (scripts/infer_tia_1gpu.sh) behind a
small FastAPI job queue so the Boostify landing "try" page and the Dataset Studio
desktop app can request audio-reactive video from an image + audio clip.

Run on the GPU box (boostify-wan):
    cd ~/HuMo
    .venv/bin/python serve_humo.py            # serves on 0.0.0.0:8000

Then expose it to a client with:
    brev port-forward boostify-wan -p 8000:8000

Endpoints:
    GET  /health                 -> {ok, gpu, model_present, busy, queued}
    POST /generate (multipart)   -> {job_id}        fields: image, audio,
                                     prompt?, height?, width?, steps?, frames?
    GET  /jobs/{job_id}          -> {status, progress, error, video_url}
    GET  /result/{job_id}        -> the rendered mp4 (FileResponse)

One GPU => one job at a time (FIFO queue).
"""
import os
import re
import json
import time
import queue
import shutil
import threading
import subprocess
import uuid
from pathlib import Path

from fastapi import FastAPI, UploadFile, File, Form, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse, JSONResponse
import uvicorn

HOME = Path.home()
HUMO_DIR = Path(os.environ.get("HUMO_DIR", HOME / "HuMo"))
VENV_PY = HUMO_DIR / ".venv" / "bin" / "torchrun"
JOBS_ROOT = HUMO_DIR / "serve_jobs"
JOBS_ROOT.mkdir(parents=True, exist_ok=True)
CONFIG = "humo/configs/inference/generate.yaml"
PORT = int(os.environ.get("HUMO_PORT", "8000"))

# In-memory job registry. job_id -> dict
JOBS: dict[str, dict] = {}
JOBS_LOCK = threading.Lock()
WORK_Q: "queue.Queue[str]" = queue.Queue()

# --- Idle auto-shutdown -----------------------------------------------------
# When the box sits unused for this many minutes (no render requested and no
# client watching a job), it powers itself off so we stop paying for the GPU.
# Set HUMO_IDLE_SHUTDOWN_MIN=0 to disable. A render in progress (including the
# slow cold model load, which keeps a job in "running") always blocks shutdown.
IDLE_SHUTDOWN_MIN = float(os.environ.get("HUMO_IDLE_SHUTDOWN_MIN", "15"))
_LAST_ACTIVITY = time.time()
_ACTIVITY_LOCK = threading.Lock()


def _touch():
    global _LAST_ACTIVITY
    with _ACTIVITY_LOCK:
        _LAST_ACTIVITY = time.time()


def _idle_seconds() -> float:
    with _ACTIVITY_LOCK:
        return time.time() - _LAST_ACTIVITY


def _has_active_work() -> bool:
    with JOBS_LOCK:
        running = any(j.get("status") == "running" for j in JOBS.values())
    return running or not WORK_Q.empty()

app = FastAPI(title="HuMo-17B Inference", version="1.0")
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


def _gpu_name() -> str:
    try:
        out = subprocess.run(
            ["nvidia-smi", "--query-gpu=name", "--format=csv,noheader"],
            capture_output=True, text=True, timeout=10,
        )
        return out.stdout.strip().splitlines()[0] if out.returncode == 0 else ""
    except Exception:
        return ""


def _model_present() -> bool:
    idx = HUMO_DIR / "weights" / "HuMo" / "HuMo-17B" / "humo.safetensors.index.json"
    return idx.exists()


def _set(job_id: str, **kw):
    with JOBS_LOCK:
        JOBS[job_id].update(kw)


def _run_job(job_id: str):
    with JOBS_LOCK:
        job = JOBS[job_id]
    job_dir = Path(job["dir"])
    out_dir = job_dir / "out"
    out_dir.mkdir(parents=True, exist_ok=True)
    log_path = job_dir / "run.log"
    case_path = job_dir / "test_case.json"

    cmd = [
        str(VENV_PY),
        "--node_rank=0", "--nproc_per_node=1", "--nnodes=1",
        "--rdzv_endpoint=127.0.0.1:12345",
        "--rdzv_conf=timeout=900,join_timeout=900,read_timeout=900",
        "main.py", CONFIG,
        f"generation.frames={job['frames']}",
        "generation.scale_a=5.5",
        "generation.scale_t=5.0",
        "generation.mode=TIA",
        f"generation.height={job['height']}",
        f"generation.width={job['width']}",
        f"diffusion.timesteps.sampling.steps={job['steps']}",
        f"generation.positive_prompt={case_path}",
        f"generation.output.dir={out_dir}",
        "dit.sp_size=1",
        "generation.sequence_parallel=1",
        "dit.fsdp.sharding_strategy=NO_SHARD",
        "text.fsdp.enabled=False",
    ]
    env = dict(os.environ)
    env.update({
        "CUDA_VISIBLE_DEVICES": "0",
        "TOKENIZERS_PARALLELISM": "false",
        "PYTHONUNBUFFERED": "1",
    })
    _set(job_id, status="running", started=time.time())
    try:
        with open(log_path, "w") as logf:
            proc = subprocess.Popen(
                cmd, cwd=str(HUMO_DIR), env=env,
                stdout=logf, stderr=subprocess.STDOUT,
            )
            with JOBS_LOCK:
                JOBS[job_id]["pid"] = proc.pid
            proc.wait()
        rc = proc.returncode
    except Exception as e:  # pragma: no cover
        _set(job_id, status="error", error=f"launch failed: {e}", ended=time.time())
        return

    mp4s = sorted(out_dir.glob("*.mp4"), key=lambda p: p.stat().st_mtime, reverse=True)
    if rc == 0 and mp4s:
        _set(job_id, status="done", video=str(mp4s[0]), progress=1.0, ended=time.time())
    else:
        tail = ""
        try:
            tail = "\n".join(log_path.read_text(errors="ignore").splitlines()[-12:])
        except Exception:
            pass
        _set(job_id, status="error",
             error=f"exit code {rc}; no video produced.\n{tail}", ended=time.time())


def _worker():
    while True:
        job_id = WORK_Q.get()
        try:
            _run_job(job_id)
        except Exception as e:  # pragma: no cover
            _set(job_id, status="error", error=str(e))
        finally:
            WORK_Q.task_done()


threading.Thread(target=_worker, daemon=True).start()


def _idle_watchdog():
    """Power the box off after IDLE_SHUTDOWN_MIN minutes with no activity."""
    if IDLE_SHUTDOWN_MIN <= 0:
        print(f"[serve_humo] idle auto-shutdown DISABLED (HUMO_IDLE_SHUTDOWN_MIN=0)")
        return
    print(f"[serve_humo] idle auto-shutdown ARMED: {IDLE_SHUTDOWN_MIN:g} min")
    limit = IDLE_SHUTDOWN_MIN * 60.0
    while True:
        time.sleep(30)
        if _has_active_work():
            _touch()           # busy -> keep the box alive
            continue
        idle = _idle_seconds()
        if idle < limit:
            continue
        msg = (f"{time.ctime()} idle {idle/60:.1f} min "
               f">= {IDLE_SHUTDOWN_MIN:g} min -> auto-stop\n")
        print(f"[serve_humo] {msg.strip()}")
        try:
            (JOBS_ROOT / "auto_shutdown.log").open("a").write(msg)
        except Exception:
            pass
        # Passwordless sudo on the box; OS poweroff transitions the brev/cloud
        # instance to STOPPED so GPU billing stops.
        for cmd in (
            ["sudo", "-n", "shutdown", "-h", "now", "HuMo idle auto-stop"],
            ["sudo", "-n", "poweroff"],
            ["systemctl", "poweroff"],
        ):
            try:
                r = subprocess.run(cmd, capture_output=True, text=True, timeout=20)
                if r.returncode == 0:
                    return
            except Exception:
                continue
        # If we could not power off, back off and retry later.
        time.sleep(120)


threading.Thread(target=_idle_watchdog, daemon=True).start()


# Rough per-stage timing model (single H100, cold) used only for ETA hints.
_LOAD_SECONDS = 80.0          # model load from disk before sampling starts
_SECONDS_PER_STEP = 20.5      # diffusion denoising step at 480x832
_ENCODE_SECONDS = 35.0        # VAE decode + ffmpeg mux after sampling


def _job_progress(job_id: str) -> dict:
    """Return a rich progress snapshot with a real pipeline *stage*.

    Stages: queued -> starting -> loading -> sampling -> encoding -> done/error.
    The previous implementation matched the model "Loading weights 1259/1259"
    tqdm bar and reported a fake 99% while the model was merely loading. We now
    key the sampling progress off the `N/<steps> [` bar specifically.
    """
    with JOBS_LOCK:
        job = JOBS.get(job_id, {})
        status = job.get("status")
        steps = int(job.get("steps") or 30)
        started = job.get("started")
        log_dir = job.get("dir")
    elapsed = round(time.time() - started, 1) if started else 0.0

    if status == "done":
        return {"stage": "done", "progress": 1.0, "step": steps,
                "total_steps": steps, "elapsed": elapsed, "eta": 0}
    if status in (None, "queued"):
        return {"stage": "queued", "progress": 0.0, "step": 0,
                "total_steps": steps, "elapsed": elapsed, "eta": None}
    if status == "error":
        return {"stage": "error", "progress": 0.0, "step": 0,
                "total_steps": steps, "elapsed": elapsed, "eta": None}

    text = ""
    try:
        text = (Path(log_dir) / "run.log").read_text(errors="ignore")
    except Exception:
        pass

    total_est = _LOAD_SECONDS + steps * _SECONDS_PER_STEP + _ENCODE_SECONDS

    # Sampling bar: only matches "<cur>/<steps> [" (e.g. 15/30 [), never the
    # weights loader (1259/1259) nor the frame count.
    sm = re.findall(rf"(\d+)/{steps}\s*\[", text)
    if sm:
        cur = int(sm[-1])
        if cur >= steps:
            eta = max(0, round(_ENCODE_SECONDS - max(0.0, elapsed - (_LOAD_SECONDS + steps * _SECONDS_PER_STEP))))
            return {"stage": "encoding", "progress": 0.95, "step": steps,
                    "total_steps": steps, "elapsed": elapsed, "eta": eta}
        frac = cur / max(steps, 1)
        done_s = _LOAD_SECONDS + cur * _SECONDS_PER_STEP
        eta = max(0, round(total_est - done_s))
        return {"stage": "sampling", "progress": round(0.12 + 0.80 * frac, 3),
                "step": cur, "total_steps": steps, "elapsed": elapsed, "eta": eta}

    # No sampling bar yet -> the model is still loading from disk.
    if any(k in text for k in ("Loading weights", "DiT Parameters", "load_state_dict", "materialized")):
        prog = min(0.10, 0.02 + (elapsed / max(_LOAD_SECONDS, 1)) * 0.08)
        return {"stage": "loading", "progress": round(prog, 3), "step": 0,
                "total_steps": steps, "elapsed": elapsed,
                "eta": max(0, round(total_est - elapsed))}

    return {"stage": "starting", "progress": 0.02, "step": 0,
            "total_steps": steps, "elapsed": elapsed,
            "eta": round(total_est)}


@app.get("/health")
def health():
    with JOBS_LOCK:
        busy = any(j["status"] == "running" for j in JOBS.values())
    return {
        "ok": True,
        "model": "HuMo-17B",
        "gpu": _gpu_name(),
        "model_present": _model_present(),
        "busy": busy,
        "queued": WORK_Q.qsize(),
        "idle_shutdown_min": IDLE_SHUTDOWN_MIN,
        "idle_min": round(_idle_seconds() / 60.0, 1),
    }


@app.post("/generate")
async def generate(
    image: UploadFile = File(...),
    audio: UploadFile = File(...),
    prompt: str = Form("A music artist performing to camera, cinematic lighting, expressive."),
    height: int = Form(480),
    width: int = Form(832),
    steps: int = Form(30),
    frames: int = Form(97),
):
    _touch()  # a render request counts as activity
    # clamp to safe single-GPU ranges
    height = max(256, min(720, int(height)))
    width = max(256, min(1280, int(width)))
    steps = max(8, min(60, int(steps)))
    frames = max(25, min(129, int(frames)))

    job_id = uuid.uuid4().hex[:12]
    job_dir = JOBS_ROOT / job_id
    job_dir.mkdir(parents=True, exist_ok=True)

    img_ext = Path(image.filename or "img.png").suffix.lower() or ".png"
    aud_ext = Path(audio.filename or "audio.wav").suffix.lower() or ".wav"
    if img_ext not in (".png", ".jpg", ".jpeg", ".webp"):
        img_ext = ".png"
    if aud_ext not in (".wav", ".mp3", ".m4a", ".flac", ".ogg"):
        aud_ext = ".wav"
    img_path = job_dir / f"image{img_ext}"
    aud_path = job_dir / f"audio{aud_ext}"
    with open(img_path, "wb") as f:
        shutil.copyfileobj(image.file, f)
    with open(aud_path, "wb") as f:
        shutil.copyfileobj(audio.file, f)

    case = {"case_1": {
        "img_paths": [str(img_path)],
        "audio_path": str(aud_path),
        "prompt": prompt.strip() or "A music artist performing to camera.",
    }}
    (job_dir / "test_case.json").write_text(json.dumps(case, indent=2))

    with JOBS_LOCK:
        JOBS[job_id] = {
            "id": job_id, "dir": str(job_dir), "status": "queued",
            "height": height, "width": width, "steps": steps, "frames": frames,
            "created": time.time(), "video": None, "error": None, "progress": 0.0,
        }
    WORK_Q.put(job_id)
    return {"job_id": job_id, "status": "queued", "position": WORK_Q.qsize()}


@app.get("/jobs/{job_id}")
def job_status(job_id: str):
    _touch()  # a client watching a job counts as activity
    with JOBS_LOCK:
        job = JOBS.get(job_id)
        if not job:
            raise HTTPException(404, "unknown job")
        snap = dict(job)
    pr = _job_progress(job_id)
    return {
        "job_id": job_id,
        "status": snap["status"],
        "stage": pr["stage"],
        "progress": round(pr["progress"], 3),
        "step": pr["step"],
        "total_steps": pr["total_steps"],
        "elapsed": pr["elapsed"],
        "eta": pr["eta"],
        "error": snap.get("error"),
        "video_url": f"/result/{job_id}" if snap["status"] == "done" else None,
        "height": snap["height"], "width": snap["width"],
        "steps": snap["steps"], "frames": snap["frames"],
    }


@app.get("/result/{job_id}")
def job_result(job_id: str):
    with JOBS_LOCK:
        job = JOBS.get(job_id)
    if not job:
        raise HTTPException(404, "unknown job")
    if job["status"] != "done" or not job.get("video"):
        raise HTTPException(409, "job not finished")
    return FileResponse(job["video"], media_type="video/mp4",
                        filename=f"humo_{job_id}.mp4")


@app.get("/")
def root():
    return JSONResponse({
        "service": "HuMo-17B inference",
        "endpoints": ["/health", "POST /generate", "/jobs/{id}", "/result/{id}"],
    })


if __name__ == "__main__":
    print(f"[serve_humo] HuMo dir: {HUMO_DIR}  model_present={_model_present()}  gpu={_gpu_name()}")
    uvicorn.run(app, host="0.0.0.0", port=PORT, log_level="info")
