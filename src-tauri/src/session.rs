//! Music-video editing sessions: ingest footage, probe technical metadata
//! (frame rate / slow-motion), and auto-classify each clip as performance vs
//! story footage so the edit engine can later assemble a cut.

use std::path::Path;

use rusqlite::Connection;
use tauri::{AppHandle, Emitter};

use crate::models::*;
use crate::{ai, db, probe, splitter};

const AUDIO_EXTS: &[&str] = &[
    "wav", "mp3", "aac", "flac", "m4a", "aif", "aiff", "ogg", "opus",
];

fn emit_progress(app: &AppHandle, p: &SessionProgress) {
    let _ = app.emit("session:progress", p.clone());
}

fn ext_lower(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Probe + classify every path and persist it as session media. Emits
/// `session:progress` events. Returns the freshly inserted rows.
pub fn ingest_paths(
    conn: &Connection,
    app: &AppHandle,
    settings: &AppSettings,
    work_dir: &Path,
    session: &EditSession,
    paths: &[String],
) -> anyhow::Result<Vec<SessionMedia>> {
    let thumbs_dir = work_dir.join("session_thumbs");
    let _ = std::fs::create_dir_all(&thumbs_dir);

    let total = paths.len() as u64;
    let artist = session.artist.as_deref();

    let mut master_set = session.master_path.is_some();
    let mut added: Vec<SessionMedia> = Vec::new();

    for (i, path) in paths.iter().enumerate() {
        let p = Path::new(path);
        let filename = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        emit_progress(
            app,
            &SessionProgress {
                session_id: session.id,
                stage: "probe".into(),
                message: format!("Reading {filename}"),
                processed: i as u64,
                total,
                done: false,
            },
        );

        if db::session_media_path_exists(conn, session.id, path)? {
            continue;
        }

        let pr = probe::probe(p);
        let ext = ext_lower(p);
        let is_audio = AUDIO_EXTS.contains(&ext.as_str())
            || pr
                .as_ref()
                .map(|r| r.width.is_none() && r.has_audio)
                .unwrap_or(false);

        let duration = pr.as_ref().and_then(|r| r.duration);
        let container_fps = pr.as_ref().and_then(|r| r.fps);
        let source_fps = pr.as_ref().and_then(|r| r.r_fps.or(r.fps));
        let (is_slow_mo, speed_pct) = probe::slow_mo_plan(source_fps, session.sequence_fps);
        let width = pr.as_ref().and_then(|r| r.width);
        let height = pr.as_ref().and_then(|r| r.height);

        let mut media = SessionMedia {
            id: 0,
            session_id: session.id,
            path: path.clone(),
            filename: filename.clone(),
            kind: if is_audio { "audio" } else { "video" }.into(),
            role: "unsorted".into(),
            role_locked: false,
            duration_seconds: duration,
            width,
            height,
            container_fps,
            source_fps,
            is_slow_mo,
            speed_pct,
            layer_group: None,
            confidence: None,
            audio_offset: None,
            sync_confidence: None,
            note: None,
            analysis: None,
            thumbnail_path: None,
            proxy_path: None,
            created_at: String::new(),
        };

        if is_audio {
            media.role = "master".into();
            media.confidence = Some(1.0);
            media.note = Some("Audio master track".into());
        } else {
            let dur = duration.unwrap_or(0.0);
            let mid = if dur > 0.0 { dur * 0.5 } else { 1.0 };
            let thumb_name = format!("s{}_{}_{}.jpg", session.id, i, now_nanos());
            let thumb_out = thumbs_dir.join(&thumb_name);

            emit_progress(
                app,
                &SessionProgress {
                    session_id: session.id,
                    stage: "thumb".into(),
                    message: format!("Sampling {filename}"),
                    processed: i as u64,
                    total,
                    done: false,
                },
            );

            if splitter::extract_thumbnail(p, mid, &thumb_out) {
                media.thumbnail_path = Some(thumb_out.to_string_lossy().to_string());
            }

            if let Some(stats) = splitter::frame_stats(p, mid) {
                let analysis = ai::analyze_scene(&stats, &filename, dur);

                emit_progress(
                    app,
                    &SessionProgress {
                        session_id: session.id,
                        stage: "classify".into(),
                        message: format!("Classifying {filename}"),
                        processed: i as u64,
                        total,
                        done: false,
                    },
                );

                let (role, conf, reason) = ai::classify_role(
                    &analysis,
                    artist,
                    &thumb_out,
                    &settings.openai_api_key,
                    &settings.nim_api_key,
                    &settings.nim_model,
                );
                media.role = role;
                media.confidence = Some(conf);
                media.note = Some(reason);
                media.analysis = Some(analysis);
            }
        }

        let id = db::insert_session_media(conn, &media)?;
        media.id = id;

        // The first audio file becomes the session master if none is set yet.
        if media.kind == "audio" && !master_set {
            let _ = db::set_session_master(conn, session.id, &media.path, media.duration_seconds);
            master_set = true;
        }

        added.push(media);
    }

    emit_progress(
        app,
        &SessionProgress {
            session_id: session.id,
            stage: "done".into(),
            message: "Ingest complete".into(),
            processed: total,
            total,
            done: true,
        },
    );

    Ok(added)
}
