//! NVIDIA Relight — re-illuminate the people in a clip to match the lighting of
//! a 360° HDRI environment map.
//!
//! This is the Tauri command side. The heavy lifting runs on the Boostify AI
//! Engine (`POST /video/relight`): when the downloadable NVIDIA Relight NIM is
//! deployed the engine uses it; otherwise it falls back to an ffmpeg colour /
//! exposure match derived from the HDRI's dominant tone, so the feature works
//! even before the NIM is on the GPU box.
//!
//! The relit clip is downloaded and registered as session media so the video
//! editor can drop it straight onto the timeline.

use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use tauri::{AppHandle, Emitter, State};

use crate::models::{SceneAnalysis, SessionMedia};
use crate::{db, engine, probe, splitter, AppState};

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Re-light an existing clip to match an HDRI environment map and add the
/// result as session media. Emits `relight:progress` events while it runs.
#[tauri::command]
pub fn relight_clip(
    session_id: i64,
    video_path: String,
    hdri_path: String,
    intensity: f64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SessionMedia, String> {
    let (session, settings) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let settings = state.load_settings(&conn);
        (session, settings)
    };

    if !engine::is_configured(&settings) {
        return Err(
            "Set the AI Engine URL in Settings — relighting runs on the engine.".into(),
        );
    }

    let vid = Path::new(&video_path);
    let hdri = Path::new(&hdri_path);
    if !vid.exists() {
        return Err("the selected clip file does not exist".into());
    }
    if !hdri.exists() {
        return Err("the selected HDRI / environment map does not exist".into());
    }
    let vid_bytes = std::fs::read(vid).map_err(|e| format!("read clip: {e}"))?;
    let hdri_bytes = std::fs::read(hdri).map_err(|e| format!("read HDRI: {e}"))?;
    let hdri_name = hdri
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("env.hdr");

    let emit = |stage: &str, message: String, progress: f64, done: bool| {
        let _ = app.emit(
            "relight:progress",
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
        "render",
        "Relighting the clip to match the HDRI…".into(),
        0.05,
        false,
    );

    let fps = {
        let f = session.sequence_fps;
        if f.is_finite() && f > 1.0 {
            (f.round() as u32).clamp(8, 60)
        } else {
            24
        }
    };
    let intensity = if intensity.is_finite() {
        intensity.clamp(0.0, 1.0)
    } else {
        0.8
    };

    let mp4 = engine::relight_video(
        &settings,
        &vid_bytes,
        &hdri_bytes,
        hdri_name,
        intensity,
        fps,
        -1,
    )
    .map_err(|e| format!("relight failed: {e}"))?;
    if mp4.len() < 1000 {
        return Err("the relit clip looks empty".into());
    }

    emit("render", "Relit clip ready — saving…".into(), 0.9, false);

    let out_dir = state
        .work_dir()
        .join("relight")
        .join(session_id.to_string());
    let _ = std::fs::create_dir_all(&out_dir);
    let out_path = out_dir.join(format!("relit_{}_{}.mp4", session_id, nanos()));
    std::fs::write(&out_path, &mp4).map_err(|e| format!("write clip: {e}"))?;

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
    let thumb_out = thumbs_dir.join(format!("relit_{}_{}.jpg", session_id, nanos()));
    let mid = duration.map(|d| d * 0.5).unwrap_or(1.0);
    let thumbnail_path = if splitter::extract_thumbnail(&out_path, mid, &thumb_out) {
        Some(thumb_out.to_string_lossy().to_string())
    } else {
        None
    };

    let mut analysis = SceneAnalysis::default();
    analysis.mood = Some("relit".into());
    analysis.setting = Some(format!("HDRI: {hdri_name}"));
    analysis.labels = vec!["relight".into(), "hdri".into()];

    let mut media = SessionMedia {
        id: 0,
        session_id,
        path: out_path.to_string_lossy().to_string(),
        filename: "Relit · HDRI".to_string(),
        kind: "video".into(),
        role: "broll".into(),
        role_locked: false,
        duration_seconds: duration,
        width: width_px,
        height: height_px,
        container_fps,
        source_fps,
        is_slow_mo,
        speed_pct,
        layer_group: None,
        confidence: Some(0.9),
        audio_offset: None,
        sync_confidence: None,
        note: Some(format!(
            "Re-lit to match {hdri_name} (NVIDIA Relight, intensity {:.0}%)",
            intensity * 100.0
        )),
        analysis: Some(analysis),
        thumbnail_path,
        proxy_path: None,
        created_at: String::new(),
    };

    let conn = state.conn.lock().unwrap();
    let id = db::insert_session_media(&conn, &media).map_err(|e| e.to_string())?;
    media.id = id;
    drop(conn);

    emit("done", "Relit clip added to the session.".into(), 1.0, true);
    Ok(media)
}
