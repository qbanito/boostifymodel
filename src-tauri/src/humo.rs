//! HuMo-17B remote inference client: image + audio -> performance video.
//!
//! Talks to the HuMo FastAPI server (`serve_humo.py`) running on the GPU box,
//! reached through a local tunnel (default `http://localhost:8000`):
//!
//! ```text
//! brev port-forward boostify-wan -p 8000:8000
//! ```
//!
//! The generated clip is downloaded and registered as session media (role
//! `performance`) so the video editor can place it on the timeline. Set the
//! `HUMO_API` env var to point at a different host/port.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use crate::models::{SceneAnalysis, SessionMedia};
use crate::{db, probe, splitter, AppState};

const DEFAULT_API: &str = "http://localhost:8000";

fn humo_api() -> String {
    std::env::var("HUMO_API")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_API.to_string())
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// `(height, width, steps)` for a quality preset.
fn quality_params(quality: &str) -> (u32, u32, u32) {
    match quality {
        "standard" => (480, 832, 50),
        "high" => (720, 1280, 50),
        // "fast" (default) — quickest usable preview.
        _ => (480, 832, 30),
    }
}

/// Assemble a `multipart/form-data` body with text fields + file parts.
/// `files` entries are `(field, filename, content_type, bytes)`.
fn build_multipart(
    boundary: &str,
    fields: &[(&str, &str)],
    files: &[(&str, &str, &str, &[u8])],
) -> Vec<u8> {
    let mut body: Vec<u8> = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(value.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    for (name, filename, content_type, bytes) in files {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

fn ext_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

fn guess_image_ct(path: &Path) -> &'static str {
    match ext_lower(path).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

fn guess_audio_ct(path: &Path) -> &'static str {
    match ext_lower(path).as_deref() {
        Some("mp3") => "audio/mpeg",
        Some("m4a") | Some("aac") => "audio/mp4",
        Some("flac") => "audio/flac",
        Some("ogg") => "audio/ogg",
        _ => "audio/wav",
    }
}

/// Generate an audio-reactive performance clip from a still image + an audio
/// clip with HuMo-17B, then register it as session media so the editor can use
/// it. Emits `humo:progress` events while the H100 renders (a few minutes).
#[tauri::command]
pub fn humo_generate(
    session_id: i64,
    image_path: String,
    audio_path: String,
    prompt: String,
    quality: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SessionMedia, String> {
    let session = {
        let conn = state.conn.lock().unwrap();
        db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?
    };

    let img = Path::new(&image_path);
    let aud = Path::new(&audio_path);
    if !img.exists() {
        return Err("the selected image file does not exist".into());
    }
    if !aud.exists() {
        return Err("the selected audio file does not exist".into());
    }
    let img_bytes = std::fs::read(img).map_err(|e| format!("read image: {e}"))?;
    let aud_bytes = std::fs::read(aud).map_err(|e| format!("read audio: {e}"))?;

    let (height, width, steps) = quality_params(&quality);
    let api = humo_api();

    let emit = |stage: &str, message: String, progress: f64, done: bool| {
        let _ = app.emit(
            "humo:progress",
            serde_json::json!({
                "sessionId": session_id,
                "stage": stage,
                "message": message,
                "progress": progress,
                "done": done,
            }),
        );
    };

    emit(
        "upload",
        "Uploading image + audio to the HuMo server…".into(),
        0.02,
        false,
    );

    let boundary = format!("----boostifyHuMo{}", nanos());
    let prompt_v = if prompt.trim().is_empty() {
        "a music artist performing to camera, cinematic lighting, expressive".to_string()
    } else {
        prompt.trim().to_string()
    };
    let h_s = height.to_string();
    let w_s = width.to_string();
    let st_s = steps.to_string();
    let img_name = img.file_name().and_then(|s| s.to_str()).unwrap_or("image.png");
    let aud_name = aud.file_name().and_then(|s| s.to_str()).unwrap_or("audio.wav");
    let body = build_multipart(
        &boundary,
        &[
            ("prompt", prompt_v.as_str()),
            ("height", &h_s),
            ("width", &w_s),
            ("steps", &st_s),
        ],
        &[
            ("image", img_name, guess_image_ct(img), &img_bytes),
            ("audio", aud_name, guess_audio_ct(aud), &aud_bytes),
        ],
    );

    let submit = ureq::post(&format!("{api}/generate"))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .timeout(Duration::from_secs(120))
        .send_bytes(&body);

    let job_id: String = match submit {
        Ok(r) => {
            let v: Value = r.into_json().map_err(|e| format!("bad server reply: {e}"))?;
            v.get("job_id")
                .and_then(|x| x.as_str())
                .map(str::to_string)
                .ok_or_else(|| "server did not return a job id".to_string())?
        }
        Err(ureq::Error::Status(code, r)) => {
            let d = r.into_string().unwrap_or_default();
            return Err(format!(
                "HuMo server error {code}: {}",
                d.chars().take(200).collect::<String>()
            ));
        }
        Err(e) => {
            return Err(format!(
                "could not reach the HuMo server at {api}. Start the tunnel with \
                 `brev port-forward boostify-wan -p 8000:8000` ({e})"
            ));
        }
    };

    emit(
        "render",
        "Rendering on the H100 (this takes a few minutes)…".into(),
        0.05,
        false,
    );

    // Poll until done. ~40 min ceiling at a 4s interval.
    let mut video_url: Option<String> = None;
    for _ in 0..600u32 {
        std::thread::sleep(Duration::from_secs(4));
        let resp = ureq::get(&format!("{api}/jobs/{job_id}"))
            .timeout(Duration::from_secs(30))
            .call();
        let v: Value = match resp {
            Ok(r) => match r.into_json() {
                Ok(j) => j,
                Err(_) => continue,
            },
            Err(_) => continue,
        };
        let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
        let progress = v.get("progress").and_then(|p| p.as_f64()).unwrap_or(0.05);
        match status {
            "done" => {
                video_url = v
                    .get("video_url")
                    .and_then(|u| u.as_str())
                    .map(str::to_string);
                emit(
                    "render",
                    "Render complete — downloading clip…".into(),
                    0.97,
                    false,
                );
                break;
            }
            "error" => {
                let err = v.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error");
                return Err(format!(
                    "HuMo render failed: {}",
                    err.lines().next().unwrap_or(err)
                ));
            }
            _ => {
                emit(
                    "render",
                    format!("Rendering… {}%", (progress * 100.0).round() as i64),
                    0.05 + progress * 0.9,
                    false,
                );
            }
        }
    }
    let video_url = video_url.ok_or_else(|| "render timed out".to_string())?;

    // Download the finished mp4 into the session's work folder.
    let out_dir = state.work_dir().join("humo").join(session_id.to_string());
    let _ = std::fs::create_dir_all(&out_dir);
    let out_path = out_dir.join(format!("humo_{}_{}.mp4", session_id, nanos()));
    let dl = ureq::get(&format!("{api}{video_url}"))
        .timeout(Duration::from_secs(120))
        .call()
        .map_err(|e| format!("download failed: {e}"))?;
    let mut reader = dl.into_reader();
    let mut buf: Vec<u8> = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut buf)
        .map_err(|e| format!("download read: {e}"))?;
    if buf.len() < 1000 {
        return Err("the downloaded clip looks empty".into());
    }
    std::fs::write(&out_path, &buf).map_err(|e| format!("write clip: {e}"))?;

    // Probe + thumbnail so the editor can place + preview it.
    let pr = probe::probe(&out_path);
    let duration = pr.as_ref().and_then(|r| r.duration);
    let container_fps = pr.as_ref().and_then(|r| r.fps);
    let source_fps = pr.as_ref().and_then(|r| r.r_fps.or(r.fps));
    let (is_slow_mo, speed_pct) = probe::slow_mo_plan(source_fps, session.sequence_fps);
    let width_px = pr.as_ref().and_then(|r| r.width);
    let height_px = pr.as_ref().and_then(|r| r.height);

    let thumbs_dir = state.work_dir().join("session_thumbs");
    let _ = std::fs::create_dir_all(&thumbs_dir);
    let thumb_out = thumbs_dir.join(format!("humo_{}_{}.jpg", session_id, nanos()));
    let mid = duration.map(|d| d * 0.5).unwrap_or(1.0);
    let thumbnail_path = if splitter::extract_thumbnail(&out_path, mid, &thumb_out) {
        Some(thumb_out.to_string_lossy().to_string())
    } else {
        None
    };

    let mut analysis = SceneAnalysis::default();
    analysis.shot_type = Some("medium".into());
    analysis.mood = Some("performance".into());
    analysis.setting = Some(prompt_v.clone());
    analysis.labels = vec!["ai performance".into(), "humo".into()];

    let mut media = SessionMedia {
        id: 0,
        session_id,
        path: out_path.to_string_lossy().to_string(),
        filename: "AI Performance · HuMo".to_string(),
        kind: "video".into(),
        role: "performance".into(),
        role_locked: true,
        duration_seconds: duration,
        width: width_px,
        height: height_px,
        container_fps,
        source_fps,
        is_slow_mo,
        speed_pct,
        layer_group: None,
        confidence: Some(0.95),
        audio_offset: None,
        sync_confidence: None,
        note: Some("Generated by HuMo-17B from a photo + audio clip".into()),
        analysis: Some(analysis),
        thumbnail_path,
        proxy_path: None,
        created_at: String::new(),
    };

    let conn = state.conn.lock().unwrap();
    let id = db::insert_session_media(&conn, &media).map_err(|e| e.to_string())?;
    media.id = id;
    drop(conn);

    emit("done", "AI performance clip added to the session.".into(), 1.0, true);
    Ok(media)
}

/// Lightweight health probe so the UI can show whether the HuMo server is
/// reachable before the user uploads anything.
#[tauri::command]
pub fn humo_status() -> Result<Value, String> {
    let api = humo_api();
    let resp = ureq::get(&format!("{api}/health"))
        .timeout(Duration::from_secs(6))
        .call();
    match resp {
        Ok(r) => r
            .into_json::<Value>()
            .map_err(|e| format!("bad health reply: {e}")),
        Err(ureq::Error::Status(code, _)) => {
            Err(format!("HuMo server returned HTTP {code}"))
        }
        Err(_) => Ok(serde_json::json!({
            "ok": false,
            "reachable": false,
            "api": api,
        })),
    }
}
