use crate::models::{AppSettings, PipelineProgress};
use crate::{ai, db, probe, splitter};
use crate::AppState;
use std::path::Path;
use tauri::{AppHandle, Emitter};

fn log(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit("app:log", line.into());
}

fn progress(app: &AppHandle, p: &PipelineProgress) {
    let _ = app.emit("pipeline:progress", p.clone());
}

/// Compute a 0..100 quality score + training value from a frame probe.
/// Returns `None` when the frame should be rejected (black / empty / blur).
fn score_frame(stats: &splitter::FrameStats, width: i64, duration: f64) -> Option<(f64, f64)> {
    // Reject pure black screens and empty/defocused frames.
    if stats.brightness < 12.0 {
        return None; // black screen
    }
    if stats.variance < 28.0 {
        return None; // flat / out of focus / empty
    }

    // Brightness sweet spot ~ 70..170.
    let b = stats.brightness;
    let brightness_score = (100.0 - ((b - 120.0).abs() / 1.5)).clamp(0.0, 100.0);

    // Sharpness proxy from luma variance (cap ~3500).
    let sharpness_score = (stats.variance / 35.0).clamp(0.0, 100.0);

    // Resolution score.
    let resolution_score = match width {
        w if w >= 3840 => 100.0,
        w if w >= 1920 => 85.0,
        w if w >= 1280 => 65.0,
        w if w >= 720 => 45.0,
        _ => 25.0,
    };

    // Duration score (sweet spot 3..8s).
    let duration_score = (duration / 8.0 * 100.0).clamp(20.0, 100.0);

    let quality = brightness_score * 0.25
        + sharpness_score * 0.35
        + resolution_score * 0.25
        + duration_score * 0.15;

    // Training value leans on sharpness + resolution (cinematic detail).
    let training = sharpness_score * 0.45 + resolution_score * 0.35 + quality * 0.20;

    Some((quality.clamp(0.0, 100.0), training.clamp(0.0, 100.0)))
}

/// Process a single video end-to-end. Returns (created, approved, rejected).
pub fn process_video(app: &AppHandle, state: &AppState, video_id: i64) -> (u64, u64, u64) {
    let (video, settings, work_dir) = {
        let conn = state.conn.lock().unwrap();
        let video = match db::get_video(&conn, video_id).ok().flatten() {
            Some(v) => v,
            None => return (0, 0, 0),
        };
        (video, state.load_settings(&conn), state.work_dir())
    };

    let src = Path::new(&video.path);
    let mut created = 0u64;
    let mut approved = 0u64;
    let mut rejected = 0u64;

    log(app, format!("▶ Processing {}", video.filename));

    let emit = |stage: &str, message: &str, c: u64, a: u64, r: u64| {
        progress(
            app,
            &PipelineProgress {
                video_id: Some(video_id),
                stage: stage.into(),
                message: message.into(),
                clips_created: c,
                clips_approved: a,
                clips_rejected: r,
                done: false,
            },
        );
    };

    // --- Probe technical metadata ---
    emit("index", "Probing metadata", 0, 0, 0);
    let pr = probe::probe(src).unwrap_or_default();
    {
        let conn = state.conn.lock().unwrap();
        let _ = db::update_video_probe(
            &conn,
            video_id,
            pr.duration,
            pr.width,
            pr.height,
            pr.fps,
            pr.codec.as_deref(),
        );
    }
    let total = pr.duration.unwrap_or(0.0);
    let width = pr.width.unwrap_or(0);
    if total <= 0.0 {
        log(app, format!("⚠ Could not read duration for {}", video.filename));
        let conn = state.conn.lock().unwrap();
        let _ = db::set_video_status(&conn, video_id, "error", true);
        return (0, 0, 0);
    }

    // --- Scene detection + segment planning ---
    emit("split", "Detecting scenes", 0, 0, 0);
    let cuts = probe::detect_scenes(src, settings.scene_threshold);
    log(app, format!("  scenes detected: {}", cuts.len()));
    let segments = splitter::plan_segments(&cuts, total, settings.min_clip_seconds, 6.0);
    log(app, format!("  planned segments: {}", segments.len()));

    for (i, seg) in segments.iter().enumerate() {
        let mid = seg.start + seg.duration() / 2.0;

        // Quality probe BEFORE the expensive cut — drop black/empty fast.
        emit("score", "Scoring frame", created, approved, rejected);
        let stats = match splitter::frame_stats(src, mid) {
            Some(s) => s,
            None => {
                rejected += 1;
                continue;
            }
        };
        let (quality, training) = match score_frame(&stats, width, seg.duration()) {
            Some(v) => v,
            None => {
                rejected += 1;
                log(app, format!("  ✗ clip {i} rejected (black/empty/blur)"));
                continue;
            }
        };

        // Cut the clip + thumbnail.
        emit("split", "Cutting clip", created, approved, rejected);
        let clip_out = splitter::clip_path(&work_dir, video_id, i);
        let thumb_out = splitter::thumb_path(&work_dir, video_id, i);
        let cut_ok = splitter::extract_clip(src, *seg, &clip_out);
        splitter::extract_thumbnail(src, mid, &thumb_out);
        if !cut_ok {
            log(app, format!("  ✗ ffmpeg failed on clip {i}"));
            rejected += 1;
            continue;
        }

        // Perceptual hash for dedup.
        let phash = splitter::average_hash(src, mid);

        // Scene analysis + caption + tags.
        emit("analyze", "Analyzing scene", created, approved, rejected);
        let analysis = ai::analyze_scene(&stats, &video.filename, seg.duration());
        emit("caption", "Generating caption", created, approved, rejected);
        let caption = ai::generate_caption(
            &analysis,
            video.artist.as_deref(),
            &thumb_out,
            &settings.openai_api_key,
            &settings.nim_api_key,
            &settings.nim_model,
        );
        let mut tags = ai::auto_tags(&analysis);
        tags.extend(video.artist.clone());

        // Identify this short clip as a direct performance shot or narrative
        // b-roll so the dataset (and the editor) carry that label. The role is
        // stored as a tag — it flows straight into the exported metadata/JSONL.
        emit("classify", "Identifying performance / b-roll", created, approved, rejected);
        let (role, _role_conf, _reason) = ai::classify_role(
            &analysis,
            video.artist.as_deref(),
            &thumb_out,
            &settings.openai_api_key,
            &settings.nim_api_key,
            &settings.nim_model,
        );
        let role_tag = if role == "performance" { "performance" } else { "b-roll" };
        if !tags.iter().any(|t| t == role_tag) {
            tags.push(role_tag.to_string());
        }

        // Auto-approve above threshold.
        let auto_approved = quality >= settings.quality_threshold;
        let status = if auto_approved { "approved" } else { "scored" };
        if auto_approved {
            approved += 1;
        }
        created += 1;

        let conn = state.conn.lock().unwrap();
        let _ = db::insert_clip(
            &conn,
            video_id,
            &clip_out.to_string_lossy(),
            seg.start,
            seg.end,
            Some(&caption),
            &tags,
            Some(quality),
            Some(training),
            status,
            if auto_approved { Some(true) } else { None },
            Some(&thumb_out.to_string_lossy()),
            Some(&analysis),
            phash.as_deref(),
        );
    }

    // --- Dedup pass across the whole library ---
    emit("dedup", "Deduplicating", created, approved, rejected);
    let removed = dedup_pass(state);
    if removed > 0 {
        log(app, format!("  near-duplicates collapsed: {removed}"));
    }

    {
        let conn = state.conn.lock().unwrap();
        let _ = db::set_video_status(&conn, video_id, "processed", true);
    }
    log(
        app,
        format!(
            "✔ {} → {created} clips ({approved} approved, {rejected} rejected)",
            video.filename
        ),
    );

    progress(
        app,
        &PipelineProgress {
            video_id: Some(video_id),
            stage: "approve".into(),
            message: "Done".into(),
            clips_created: created,
            clips_approved: approved,
            clips_rejected: rejected,
            done: false,
        },
    );

    (created, approved, rejected)
}

/// Collapse near-duplicate clips (Hamming distance <= 6 on aHash), keeping the
/// highest-quality one. Returns the number of clips marked as duplicates.
fn dedup_pass(state: &AppState) -> u64 {
    let conn = state.conn.lock().unwrap();
    let items = match db::list_phashes(&conn) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    let mut removed = 0u64;
    let mut kept: Vec<(i64, String, f64)> = Vec::new();
    for (id, hash, score) in items {
        if let Some(slot) = kept
            .iter_mut()
            .find(|(_, kh, _)| splitter::hamming(kh, &hash) <= 6)
        {
            // Duplicate of an existing kept clip — keep the better one.
            if score > slot.2 {
                let _ = db::mark_duplicate(&conn, slot.0);
                *slot = (id, hash, score);
            } else {
                let _ = db::mark_duplicate(&conn, id);
            }
            removed += 1;
        } else {
            kept.push((id, hash, score));
        }
    }
    removed
}

/// Process every pending (unprocessed) video sequentially.
pub fn process_all_pending(app: &AppHandle, state: &AppState) {
    let pending = {
        let conn = state.conn.lock().unwrap();
        db::pending_videos(&conn).unwrap_or_default()
    };
    log(app, format!("Pipeline: {} videos pending", pending.len()));
    let mut tc = 0u64;
    let mut ta = 0u64;
    let mut tr = 0u64;
    for v in pending {
        let (c, a, r) = process_video(app, state, v.id);
        tc += c;
        ta += a;
        tr += r;
    }
    progress(
        app,
        &PipelineProgress {
            video_id: None,
            stage: "approve".into(),
            message: "Pipeline complete".into(),
            clips_created: tc,
            clips_approved: ta,
            clips_rejected: tr,
            done: true,
        },
    );
}

impl AppState {
    pub fn load_settings(&self, conn: &rusqlite::Connection) -> AppSettings {
        match db::get_setting(conn, "app_settings") {
            Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_default(),
            _ => AppSettings::default(),
        }
    }
}
