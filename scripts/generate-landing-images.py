#!/usr/bin/env python3
"""Generate brand imagery for the Boostify MotionDNA landing page.

Uses NVIDIA's free FLUX.1-schnell endpoint (key read from the project .env as
NIM_API_KEY / NVIDIA_API_KEY). Saves PNGs into landing/assets/.

Run:  python3 scripts/generate-landing-images.py
"""
import base64
import json
import os
import sys
import time
import urllib.request
import urllib.error

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(ROOT, "landing", "assets")
ENDPOINT = "https://ai.api.nvidia.com/v1/genai/black-forest-labs/flux.1-schnell"

# Shared style so every image feels like one brand system.
STYLE = (
    "cinematic, dark studio, deep black background, vivid orange #ff6a2b to amber "
    "#ffb02e gradient lighting, volumetric haze, high detail, premium tech brand, "
    "no text, no watermark, no logo"
)

# name -> (prompt, width, height)
IMAGES = {
    "hero": (
        "abstract motion-capture skeleton of a dancer mid-movement, glowing wireframe "
        "body made of light particles and motion trails, futuristic stage, " + STYLE,
        1344, 768,
    ),
    "motion-panel": (
        "side profile of a performer dissolving into a flowing point-cloud of motion "
        "tracking dots and bone lines, data visualization of choreography, " + STYLE,
        1024, 1024,
    ),
    "usecase-music": (
        "professional female pop singer holding a microphone on a glowing concert "
        "stage, warm orange and amber spotlights, confetti, vibrant, energetic, "
        "cinematic photography, no text, no watermark",
        768, 768,
    ),
    "usecase-dance": (
        "abstract neon wireframe of a breakdancer captured as motion trails, " + STYLE,
        768, 768,
    ),
    "usecase-camera": (
        "cinematic camera rig tracking a glowing motion-capture figure, depth of field, " + STYLE,
        768, 768,
    ),
}


def load_env_key():
    for var in ("NVIDIA_API_KEY", "NIM_API_KEY"):
        v = os.environ.get(var)
        if v:
            return v.strip()
    env_path = os.path.join(ROOT, ".env")
    if os.path.exists(env_path):
        with open(env_path, "r", encoding="utf-8") as fh:
            for line in fh:
                line = line.strip()
                if line.startswith(("NVIDIA_API_KEY=", "NIM_API_KEY=")):
                    return line.split("=", 1)[1].strip()
    return None


def generate(key, name, prompt, width, height, seed=7):
    body = json.dumps({
        "prompt": prompt,
        "width": width,
        "height": height,
        "steps": 4,
        "seed": seed,
        "cfg_scale": 0,
    }).encode("utf-8")
    req = urllib.request.Request(
        ENDPOINT,
        data=body,
        headers={
            "Authorization": "Bearer " + key,
            "Accept": "application/json",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    artifacts = data.get("artifacts") or []
    if not artifacts:
        raise RuntimeError("no artifacts returned: " + json.dumps(data)[:200])
    b64 = artifacts[0]["base64"]
    # NVIDIA returns a near-black ~3.7KB image when a prompt is content-filtered.
    if len(b64) < 8000:
        raise RuntimeError("likely content-filtered (tiny image)")
    out = os.path.join(ASSETS, name + ".png")
    with open(out, "wb") as fh:
        fh.write(base64.b64decode(b64))
    print(f"  saved {out} ({width}x{height})")


def main():
    key = load_env_key()
    if not key:
        print("ERROR: no NVIDIA_API_KEY / NIM_API_KEY found in env or .env", file=sys.stderr)
        sys.exit(1)
    os.makedirs(ASSETS, exist_ok=True)
    print(f"Generating {len(IMAGES)} images into {ASSETS}")
    for i, (name, (prompt, w, h)) in enumerate(IMAGES.items()):
        for attempt in range(1, 4):
            try:
                print(f"[{name}] attempt {attempt} ...")
                generate(key, name, prompt, w, h, seed=7 + i)
                break
            except (urllib.error.HTTPError, urllib.error.URLError, RuntimeError) as exc:
                detail = ""
                if isinstance(exc, urllib.error.HTTPError):
                    try:
                        detail = exc.read().decode("utf-8")[:200]
                    except Exception:
                        pass
                print(f"  failed: {exc} {detail}", file=sys.stderr)
                if attempt == 3:
                    print(f"  GIVING UP on {name}", file=sys.stderr)
                else:
                    time.sleep(2 * attempt)
    print("Done.")


if __name__ == "__main__":
    main()
