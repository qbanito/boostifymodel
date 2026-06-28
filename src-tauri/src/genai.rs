//! Generative B-roll — capture the artist's visual style from their own
//! footage, then generate coherent B-roll using NVIDIA's free hosted models
//! (FLUX.1 for image, Stable Video Diffusion / Cosmos for image→video).
//!
//! All network calls go through NVIDIA's OpenAI-style genai gateway at
//! `ai.api.nvidia.com`, authenticated with the same `nim_api_key` used for
//! captioning (env fallback `NVIDIA_API_KEY`). Large outputs (video) return a
//! 202 + `NVCF-REQID`, which we poll until ready.
//!
//! The module is deliberately pure: it produces bytes/plans and never touches
//! the database. Orchestration (persistence, progress events) lives in `lib.rs`.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use crate::models::*;

const GENAI_HOST: &str = "https://ai.api.nvidia.com/v1/genai";
const NVCF_STATUS: &str = "https://api.nvcf.nvidia.com/v2/nvcf/exec/status";
const NVCF_ASSETS: &str = "https://api.nvcf.nvidia.com/v2/nvcf/assets";

// FLUX.1 on NVIDIA only accepts specific dimensions (multiples that include
// 768..1344); 1344x768 is the widest valid 16:9 option for cinematic B-roll.
const GEN_W: u32 = 1344;
const GEN_H: u32 = 768;
// Stable Video Diffusion requires its input image to be exactly 1024x576, so
// the generated still is resized to this before animation.
#[allow(dead_code)]
const SVD_W: u32 = 1024;
#[allow(dead_code)]
const SVD_H: u32 = 576;

fn ffmpeg_bin() -> String {
    crate::system::resolve_bin("ffmpeg", "FFMPEG_PATH")
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn image_model() -> String {
    // FLUX.2 Klein 4B is NVIDIA's free, distilled (4-step) image model that
    // supports BOTH text-to-image and image-conditioned editing — the cloud
    // fallback used whenever the local engine (H200) is off.
    env_or("NVIDIA_IMAGE_MODEL", "black-forest-labs/flux.2-klein-4b")
}

/// True for FLUX.2 / Klein-style models, which take `steps:4`, no `mode`/
/// `cfg_scale`, and accept image inputs as uploaded NVCF assets.
fn is_klein(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.contains("klein") || m.contains("flux.2") || m.contains("flux-2")
}

fn video_model() -> String {
    env_or("NVIDIA_VIDEO_MODEL", "stabilityai/stable-video-diffusion")
}

/// Resolve the NVIDIA API key: explicit settings value wins, else env vars.
pub fn resolve_key(settings: &AppSettings) -> String {
    let k = settings.nim_api_key.trim();
    if !k.is_empty() {
        return k.to_string();
    }
    for v in ["NVIDIA_API_KEY", "NIM_API_KEY"] {
        if let Ok(val) = std::env::var(v) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return val;
            }
        }
    }
    String::new()
}

// ---------------------------------------------------------------------------
// Style capture
// ---------------------------------------------------------------------------

/// Build a visual style reference from the session's existing footage:
/// dominant color palette + a natural-language descriptor derived from the
/// per-clip scene analysis, plus a handful of representative keyframes.
pub fn build_style_reference(
    session: &EditSession,
    media: &[SessionMedia],
    max_keyframes: usize,
) -> StyleReference {
    // Prefer real footage (story first for mood, then performance) with a thumb.
    let mut ranked: Vec<&SessionMedia> = media
        .iter()
        .filter(|m| m.kind == "video" && m.thumbnail_path.is_some())
        .collect();
    ranked.sort_by_key(|m| match m.role.as_str() {
        "story" => 0,
        "performance" => 1,
        _ => 2,
    });

    let keyframes: Vec<String> = ranked
        .iter()
        .take(max_keyframes.max(1))
        .filter_map(|m| m.thumbnail_path.clone())
        .collect();

    // Palette: sample the dominant colors of the chosen keyframes.
    let mut palette_counts: HashMap<(u8, u8, u8), u32> = HashMap::new();
    for kf in keyframes.iter().take(4) {
        for (c, n) in dominant_colors(Path::new(kf), 6) {
            *palette_counts.entry(c).or_default() += n;
        }
    }
    let mut palette_vec: Vec<((u8, u8, u8), u32)> = palette_counts.into_iter().collect();
    palette_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let palette: Vec<String> = palette_vec
        .into_iter()
        .take(6)
        .map(|((r, g, b), _)| format!("#{:02x}{:02x}{:02x}", r, g, b))
        .collect();

    let descriptor = style_descriptor(media);

    StyleReference {
        palette,
        descriptor,
        keyframes,
        artist: session.artist.clone().filter(|a| !a.trim().is_empty()),
    }
}

/// Most-common quantized colors of an image (via a tiny ffmpeg downscale).
fn dominant_colors(thumb: &Path, max: usize) -> Vec<((u8, u8, u8), u32)> {
    let out = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(thumb)
        .args([
            "-vf", "scale=16:16", "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();

    let buf = match out {
        Some(o) if o.status.success() => o.stdout,
        _ => return vec![],
    };

    let mut counts: HashMap<(u8, u8, u8), u32> = HashMap::new();
    let q = |c: u8| -> u8 { (c / 32).saturating_mul(32).saturating_add(16) };
    for px in buf.chunks_exact(3) {
        // Skip near-black / near-white so the palette reflects real tones.
        let lum = px[0] as u32 + px[1] as u32 + px[2] as u32;
        if lum < 60 || lum > 720 {
            continue;
        }
        *counts.entry((q(px[0]), q(px[1]), q(px[2]))).or_default() += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v.into_iter().take(max).collect()
}

/// Aggregate the per-clip scene analysis into one descriptive style sentence.
fn style_descriptor(media: &[SessionMedia]) -> String {
    let mut mood: HashMap<String, u32> = HashMap::new();
    let mut lighting: HashMap<String, u32> = HashMap::new();
    let mut time: HashMap<String, u32> = HashMap::new();
    let mut setting: HashMap<String, u32> = HashMap::new();
    let mut camera: HashMap<String, u32> = HashMap::new();

    for m in media.iter() {
        if let Some(a) = m.analysis.as_ref() {
            if let Some(v) = a.mood.as_ref() {
                *mood.entry(v.replace('_', " ")).or_default() += 1;
            }
            if let Some(v) = a.lighting.as_ref() {
                *lighting.entry(v.replace('_', " ")).or_default() += 1;
            }
            if let Some(v) = a.time_of_day.as_ref() {
                *time.entry(v.replace('_', " ")).or_default() += 1;
            }
            if let Some(v) = a.setting.as_ref() {
                *setting.entry(v.replace('_', " ")).or_default() += 1;
            }
            if let Some(v) = a.camera_movement.as_ref() {
                *camera.entry(v.replace('_', " ")).or_default() += 1;
            }
        }
    }

    let top = |m: &HashMap<String, u32>| -> Option<String> {
        m.iter().max_by_key(|(_, n)| **n).map(|(k, _)| k.clone())
    };

    let mut parts: Vec<String> = Vec::new();
    parts.push(format!(
        "{} music video",
        top(&mood).unwrap_or_else(|| "cinematic".into())
    ));
    if let Some(l) = top(&lighting) {
        parts.push(format!("{l} lighting"));
    }
    if let Some(t) = top(&time) {
        parts.push(t);
    }
    if let Some(s) = top(&setting) {
        parts.push(format!("{s} locations"));
    }
    if let Some(c) = top(&camera) {
        parts.push(format!("{c} camera"));
    }
    format!("{}.", parts.join(", "))
}

// ---------------------------------------------------------------------------
// B-roll planning
// ---------------------------------------------------------------------------

/// A planned B-roll shot before any pixels are generated.
#[derive(Debug, Clone)]
pub struct BrollPlan {
    pub section: String,
    pub idea: String,
    pub prompt: String,
    pub seed: i64,
}

/// Plan `count` coherent B-roll shots, biased toward the calm sections of the
/// song (intro/low/bridge/outro) where cutaways read best, and toward ideas
/// that match the captured style.
pub fn plan_broll(style: &StyleReference, analysis: &MasterAnalysis, count: usize) -> Vec<BrollPlan> {
    // Section pool: calm sections first, then everything else.
    let mut sections: Vec<String> = analysis
        .sections
        .iter()
        .filter(|s| matches!(s.label.as_str(), "intro" | "low" | "bridge" | "outro"))
        .map(|s| s.label.clone())
        .collect();
    if sections.is_empty() {
        sections = analysis.sections.iter().map(|s| s.label.clone()).collect();
    }
    if sections.is_empty() {
        sections.push("bridge".into());
    }

    let ideas = broll_ideas(style);
    let mut plans = Vec::with_capacity(count);
    for i in 0..count {
        let section = sections[i % sections.len()].clone();
        let idea = ideas[i % ideas.len()].to_string();
        let seed = (1000 + (i as i64) * 7919) % 2_147_483_647;
        let prompt = broll_prompt(style, &section, &idea);
        plans.push(BrollPlan { section, idea, prompt, seed });
    }
    plans
}

/// Curated, style-aware cinematic B-roll ideas. Reorders so ideas matching the
/// captured setting/mood come first.
fn broll_ideas(style: &StyleReference) -> Vec<&'static str> {
    let desc = style.descriptor.to_lowercase();
    let mut ideas: Vec<&'static str> = vec![
        "slow push-in on the empty horizon at golden hour",
        "abstract bokeh of city lights melting out of focus",
        "extreme close-up of rain hitting a window in slow motion",
        "wide aerial drift over a moody coastline",
        "neon reflections rippling across wet asphalt at night",
        "dust particles floating through a single shaft of light",
        "hands gently brushing across textured fabric, macro",
        "silhouette walking down an empty corridor toward the light",
        "clouds time-lapsing over a dramatic skyline",
        "ink diffusing through water, swirling color",
    ];
    // Light reordering for coherence with the captured scene.
    let prioritize = |ideas: &mut Vec<&'static str>, needle: &str| {
        if let Some(pos) = ideas.iter().position(|s| s.contains(needle)) {
            let item = ideas.remove(pos);
            ideas.insert(0, item);
        }
    };
    if desc.contains("night") || desc.contains("neon") || desc.contains("club") {
        prioritize(&mut ideas, "neon");
        prioritize(&mut ideas, "city");
    }
    if desc.contains("nature") || desc.contains("beach") || desc.contains("coast") {
        prioritize(&mut ideas, "coastline");
        prioritize(&mut ideas, "horizon");
    }
    if desc.contains("studio") || desc.contains("close") {
        prioritize(&mut ideas, "macro");
    }
    ideas
}

fn section_mood(section: &str) -> &'static str {
    match section {
        "intro" => "calm, atmospheric, establishing",
        "low" => "introspective, intimate, restrained",
        "build" => "rising tension, momentum, anticipation",
        "drop" => "high energy, bold, kinetic",
        "bridge" => "dreamy, transitional, reflective",
        "outro" => "fading, resolved, cinematic farewell",
        _ => "cinematic, balanced",
    }
}

fn broll_prompt(style: &StyleReference, section: &str, idea: &str) -> String {
    let palette = if style.palette.is_empty() {
        String::new()
    } else {
        format!(" Color palette: {}.", style.palette.join(", "))
    };
    format!(
        "Cinematic music-video B-roll: {idea}. Visual style: {desc}{palette} \
         Mood for this moment: {mood}. Photorealistic, filmic, anamorphic, shallow \
         depth of field, volumetric light, rich film grain, 16:9 widescreen. \
         No text, no words, no letters, no logo, no watermark, no captions, no people \
         facing camera unless implied.",
        desc = style.descriptor,
        mood = section_mood(section)
    )
}

// ---------------------------------------------------------------------------
// NVIDIA generation calls
// ---------------------------------------------------------------------------

/// Generate a single still image (PNG bytes) with the NVIDIA image model.
pub fn generate_image(key: &str, prompt: &str, seed: i64) -> Option<Vec<u8>> {
    let model = image_model();
    let url = format!("{GENAI_HOST}/{model}");
    let body = if is_klein(&model) {
        // FLUX.2 Klein: distilled 4-step, no cfg/mode knobs.
        json!({
            "prompt": prompt,
            "width": GEN_W,
            "height": GEN_H,
            "seed": seed,
            "steps": 4
        })
    } else {
        json!({
            "prompt": prompt,
            "mode": "base",
            "cfg_scale": 3.5,
            "width": GEN_W,
            "height": GEN_H,
            "seed": seed,
            "steps": 40
        })
    };
    let v = nv_invoke(&url, key, body)?;
    let b64 = find_b64(&v)?;
    decode_b64(&b64)
}

/// Edit an existing image (PNG/JPEG bytes) with a text instruction using the
/// NVIDIA image model (FLUX.2 Klein supports image-conditioned editing). The
/// input is uploaded as an NVCF asset and referenced by index, per NVIDIA's
/// genai API. Returns the edited image (PNG/JPEG bytes) or `None`.
pub fn edit_image(key: &str, image: &[u8], prompt: &str, seed: i64) -> Option<Vec<u8>> {
    let model = image_model();
    if !is_klein(&model) {
        eprintln!("[genai] edit_image needs a FLUX.2/Klein model (got {model})");
        return None;
    }
    let ct = sniff_image_ct(image);
    let asset = nvcf_upload_asset(key, image, ct, "boostify-edit-input")?;
    let url = format!("{GENAI_HOST}/{model}");
    // The image is referenced by its index (0) into NVCF-INPUT-ASSET-REFERENCES;
    // the literal token is `example_id`. No width/height → preserve input size.
    let body = json!({
        "prompt": prompt,
        "image": [format!("data:{ct};example_id,0")],
        "seed": seed,
        "steps": 4
    });
    let v = nv_invoke_with_assets(&url, key, body, &asset)?;
    let b64 = find_b64(&v)?;
    decode_b64(&b64)
}

/// Sniff a PNG/JPEG/WebP content type from the leading magic bytes.
fn sniff_image_ct(b: &[u8]) -> &'static str {
    if b.len() >= 8 && &b[0..8] == b"\x89PNG\r\n\x1a\n" {
        "image/png"
    } else if b.len() >= 3 && &b[0..3] == b"\xff\xd8\xff" {
        "image/jpeg"
    } else if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/png"
    }
}

/// Reserve + upload an NVCF asset, returning its `assetId`. Used to pass image
/// inputs to genai models that don't accept inline base64 (e.g. FLUX.2 Klein).
fn nvcf_upload_asset(key: &str, bytes: &[u8], content_type: &str, desc: &str) -> Option<String> {
    // 1) Reserve an asset slot → { assetId, uploadUrl }.
    let reserved = match ureq::post(NVCF_ASSETS)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(60))
        .send_json(json!({ "contentType": content_type, "description": desc }))
    {
        Ok(r) => r.into_json::<Value>().ok()?,
        Err(e) => {
            eprintln!("[genai] asset reserve failed: {e}");
            return None;
        }
    };
    let asset_id = reserved.get("assetId").and_then(|v| v.as_str())?.to_string();
    let upload_url = reserved.get("uploadUrl").and_then(|v| v.as_str())?.to_string();

    // 2) PUT the bytes to the presigned URL (no auth header — it's presigned).
    match ureq::put(&upload_url)
        .set("Content-Type", content_type)
        .set("x-amz-meta-nvcf-asset-description", desc)
        .timeout(Duration::from_secs(120))
        .send_bytes(bytes)
    {
        Ok(_) => Some(asset_id),
        Err(e) => {
            eprintln!("[genai] asset upload failed: {e}");
            None
        }
    }
}

/// Animate a still image into a short video clip (MP4 bytes) with the NVIDIA
/// image-to-video model (Stable Video Diffusion). Optional — not all accounts
/// have SVD provisioned; the local Ken Burns path is the default. Kept so it
/// can be wired back in when an account has access to a hosted video model.
#[allow(dead_code)]
pub fn image_to_video(key: &str, png: &[u8], seed: i64) -> Option<Vec<u8>> {
    let url = format!("{GENAI_HOST}/{}", video_model());
    // SVD only accepts a 1024x576 input frame — conform the still first.
    let frame = resize_png(png, SVD_W, SVD_H).unwrap_or_else(|| png.to_vec());
    let data_url = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&frame)
    );
    let body = json!({
        "image": data_url,
        "cfg_scale": 2.5,
        "seed": seed
    });
    let v = nv_invoke(&url, key, body)?;
    let b64 = find_b64(&v)?;
    decode_b64(&b64)
}

/// Resize PNG bytes to an exact width×height (cover + center-crop) via ffmpeg.
#[allow(dead_code)]
fn resize_png(png: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    use std::io::Write;
    let mut child = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-i", "pipe:0", "-vf"])
        .arg(format!(
            "scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}"
        ))
        .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "pipe:1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    if let Some(mut stdin) = child.stdin.take() {
        let png = png.to_vec();
        // Write on a thread so a large image can't deadlock the pipe.
        std::thread::spawn(move || {
            let _ = stdin.write_all(&png);
        });
    }
    let out = child.wait_with_output().ok()?;
    if out.status.success() && !out.stdout.is_empty() {
        Some(out.stdout)
    } else {
        None
    }
}

/// POST to a genai endpoint, transparently polling NVCF when the result is
/// produced asynchronously (HTTP 202 + `NVCF-REQID`).
fn nv_invoke(url: &str, key: &str, body: Value) -> Option<Value> {
    nv_invoke_inner(url, key, body, None)
}

/// Like `nv_invoke` but references uploaded NVCF input assets (comma-separated
/// asset ids) via the `NVCF-INPUT-ASSET-REFERENCES` header — required for image
/// editing on FLUX.2 Klein.
fn nv_invoke_with_assets(url: &str, key: &str, body: Value, asset_refs: &str) -> Option<Value> {
    nv_invoke_inner(url, key, body, Some(asset_refs))
}

fn nv_invoke_inner(url: &str, key: &str, body: Value, asset_refs: Option<&str>) -> Option<Value> {
    let mut req = ureq::post(url)
        .set("Authorization", &format!("Bearer {key}"))
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .timeout(Duration::from_secs(180));
    if let Some(refs) = asset_refs {
        req = req.set("NVCF-INPUT-ASSET-REFERENCES", refs);
    }
    let resp = req.send_json(body);

    match resp {
        Ok(r) => {
            if r.status() == 202 {
                let reqid = r.header("NVCF-REQID")?.to_string();
                poll_nvcf(key, &reqid)
            } else {
                r.into_json().ok()
            }
        }
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_string()
                .unwrap_or_default()
                .chars()
                .take(400)
                .collect::<String>();
            eprintln!("[genai] HTTP {code}: {detail}");
            None
        }
        Err(e) => {
            eprintln!("[genai] request failed: {e}");
            None
        }
    }
}

/// Poll the NVCF status endpoint until the async job completes (≈3 min cap).
fn poll_nvcf(key: &str, reqid: &str) -> Option<Value> {
    let url = format!("{NVCF_STATUS}/{reqid}");
    for _ in 0..90 {
        std::thread::sleep(Duration::from_secs(2));
        let resp = ureq::get(&url)
            .set("Authorization", &format!("Bearer {key}"))
            .set("Accept", "application/json")
            .timeout(Duration::from_secs(30))
            .call();
        match resp {
            Ok(r) => {
                if r.status() == 202 {
                    continue;
                }
                return r.into_json().ok();
            }
            Err(ureq::Error::Status(202, _)) => continue,
            Err(e) => {
                eprintln!("[genai] poll failed: {e}");
                return None;
            }
        }
    }
    eprintln!("[genai] poll timed out for {reqid}");
    None
}

/// Find a base64 image/video payload in any of NVIDIA's response shapes.
fn find_b64(v: &Value) -> Option<String> {
    let candidates = [
        "/artifacts/0/base64",
        "/artifacts/0/b64_json",
        "/data/0/b64_json",
        "/data/0/base64",
        "/image",
        "/video",
        "/b64_json",
        "/images/0",
    ];
    for p in candidates {
        if let Some(s) = v.pointer(p).and_then(|x| x.as_str()) {
            let s = s.trim();
            if !s.is_empty() {
                return Some(strip_data_url(s));
            }
        }
    }
    None
}

/// Strip a `data:...;base64,` prefix if present.
fn strip_data_url(s: &str) -> String {
    if let Some(idx) = s.find("base64,") {
        s[idx + 7..].to_string()
    } else {
        s.to_string()
    }
}

fn decode_b64(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .ok()
        .filter(|b| !b.is_empty())
}

// ---------------------------------------------------------------------------
// Local image → video (Ken Burns) — always available, no account dependency
// ---------------------------------------------------------------------------

/// Animate a still image into a cinematic clip locally with ffmpeg (a slow
/// Ken Burns zoom/pan). This is the reliable, free path that turns every
/// generated frame into timeline-ready footage and keeps the clip identical to
/// the previewed image, so B-roll stays perfectly coherent.
///
/// `variant` cycles the motion (zoom-in, zoom-out, pan) for visual variety.
/// Returns true on success (an H.264 MP4 written to `out`).
pub fn animate_still_local(img: &Path, out: &Path, seconds: f64, fps: f64, variant: u8) -> bool {
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let fps = if fps.is_finite() && fps > 1.0 { fps } else { 24.0 };
    let secs = seconds.clamp(2.0, 8.0);
    let frames = (secs * fps).round().max(2.0) as i64;

    // zoompan works on an upscaled frame for smooth, jitter-free motion.
    // The moves are deliberately pronounced so every clip reads clearly as
    // motion (cinematic drift) rather than a static photo.
    let motion = match variant % 4 {
        // Zoom-in while drifting up-left → down-right (diagonal push).
        0 => "zoompan=z='min(zoom+0.0022,1.30)':x='(iw-iw/zoom)*on/DUR':y='(ih-ih/zoom)*on/DUR'".replace("DUR", &frames.to_string()),
        // Zoom-out, centered (reveal).
        1 => "zoompan=z='if(eq(on,0),1.30,max(zoom-0.0022,1.0))':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)'".to_string(),
        // Zoom-in drifting left→right.
        2 => "zoompan=z='min(zoom+0.0020,1.26)':x='(iw-iw/zoom)*on/DUR':y='ih/2-(ih/zoom/2)'".replace("DUR", &frames.to_string()),
        // Zoom-in drifting bottom→top.
        _ => "zoompan=z='min(zoom+0.0020,1.26)':x='iw/2-(iw/zoom/2)':y='(ih-ih/zoom)*(1-on/DUR)'".replace("DUR", &frames.to_string()),
    };
    let vf = format!(
        "scale=-2:2160,{motion}:d={frames}:s=1920x1080:fps={fps},format=yuv420p",
        fps = fps as i64
    );

    let st = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-loop", "1", "-i"])
        .arg(img)
        .args(["-t"])
        .arg(format!("{secs:.3}"))
        .args(["-vf", &vf])
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "19", "-pix_fmt", "yuv420p",
            "-movflags", "+faststart", "-an",
        ])
        .arg(out)
        .status();

    matches!(st, Ok(s) if s.success()) && out.exists()
}

/// Suggested B-roll clip length for a song section.
pub fn section_clip_seconds(section: &str) -> f64 {
    match section {
        "intro" | "outro" => 5.0,
        "low" | "bridge" => 4.5,
        "build" => 3.5,
        "drop" => 3.0,
        _ => 4.0,
    }
}

