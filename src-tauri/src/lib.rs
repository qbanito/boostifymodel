mod ai;
mod audio;
mod dataset;
mod db;
mod editor;
mod engine;
mod genai;
mod gpu_server;
mod humo;
mod models;
mod mvgen;
mod pipeline;
mod relight;
mod premiere;
mod probe;
mod scanner;
mod session;
mod splitter;
mod system;
mod watch;

use models::*;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Shared application state.
pub struct AppState {
    pub conn: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub scan_cancel: AtomicBool,
    pub watcher: Mutex<Option<notify::RecommendedWatcher>>,
}

impl AppState {
    pub fn work_dir(&self) -> PathBuf {
        self.data_dir.join("work")
    }
}

fn log(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit("app:log", line.into());
}

// ---------------------------------------------------------------------------
// Library scanning / indexing
// ---------------------------------------------------------------------------

#[tauri::command]
fn scan_folder(path: String, app: AppHandle, state: State<'_, Arc<AppState>>) {
    let state = state.inner().clone();
    state.scan_cancel.store(false, Ordering::SeqCst);

    std::thread::spawn(move || {
        use rayon::prelude::*;
        use std::sync::atomic::AtomicU64;
        use std::time::Instant;

        log(&app, format!("Scanning {path}…"));
        let started = Instant::now();
        let root = PathBuf::from(&path);

        // Read concurrency + snapshot the existing index so the heavy work
        // (walking, hashing) runs WITHOUT holding the DB lock.
        let (concurrency, known_sigs, mut known_hashes) = {
            let conn = state.conn.lock().unwrap();
            let settings = state.load_settings(&conn);
            let c = (settings.concurrency.clamp(1, 32)) as usize;
            let sigs = db::all_path_sigs(&conn).unwrap_or_default();
            let hashes = db::all_hashes(&conn).unwrap_or_default();
            (c, sigs, hashes)
        };

        // --- Phase 1: discovery (with live progress) ---
        let app_disc = app.clone();
        let discovered = scanner::discover(&root, |count, current| {
            let _ = app_disc.emit(
                "scan:progress",
                ScanProgress {
                    phase: "scanning".into(),
                    files_discovered: count,
                    files_indexed: 0,
                    files_skipped: 0,
                    current_path: (!current.is_empty()).then(|| current.to_string()),
                    done: false,
                },
            );
        });
        let total = discovered.len() as u64;
        log(&app, format!("Discovered {total} video files"));

        // --- Phase 2: incremental skip (stat-based, no hashing) ---
        let mut unchanged = 0u64;
        let mut to_hash: Vec<scanner::Discovered> = Vec::with_capacity(discovered.len());
        for d in discovered {
            if let Some(&(size, mtime)) = known_sigs.get(&d.path) {
                if size == d.size_bytes && mtime == d.mtime {
                    unchanged += 1;
                    continue;
                }
            }
            to_hash.push(d);
        }
        log(
            &app,
            format!(
                "{unchanged} unchanged (skipped), {} to fingerprint",
                to_hash.len()
            ),
        );

        // --- Phase 3: parallel fingerprinting ---
        let processed = AtomicU64::new(0);
        let bytes_done = AtomicU64::new(0);
        let processed_ref = &processed;
        let bytes_ref = &bytes_done;
        let cancel = &state.scan_cancel;
        let app_hash = &app;

        let compute = move || {
            to_hash
                .into_par_iter()
                .filter_map(|d| {
                    if cancel.load(Ordering::SeqCst) {
                        return None;
                    }
                    let sig = scanner::content_signature(
                        std::path::Path::new(&d.path),
                        d.size_bytes as u64,
                    )
                    .ok()?;
                    let n = processed_ref.fetch_add(1, Ordering::Relaxed) + 1;
                    bytes_ref.fetch_add(d.size_bytes.max(0) as u64, Ordering::Relaxed);
                    if n % 8 == 0 {
                        let _ = app_hash.emit(
                            "scan:progress",
                            ScanProgress {
                                phase: "indexing".into(),
                                files_discovered: total,
                                files_indexed: n + unchanged,
                                files_skipped: unchanged,
                                current_path: Some(d.path.clone()),
                                done: false,
                            },
                        );
                    }
                    Some((d, sig))
                })
                .collect::<Vec<_>>()
        };

        let hashed: Vec<(scanner::Discovered, String)> =
            match rayon::ThreadPoolBuilder::new().num_threads(concurrency).build() {
                Ok(pool) => pool.install(compute),
                Err(_) => compute(),
            };

        // --- Phase 4: dedup + batched insert in a single transaction ---
        let mut indexed = 0u64;
        let mut dupes = 0u64;
        {
            let conn = state.conn.lock().unwrap();
            let _ = conn.execute_batch("BEGIN");
            for (d, sig) in &hashed {
                if !known_hashes.insert(sig.clone()) {
                    dupes += 1;
                    continue;
                }
                match db::insert_video(
                    &conn,
                    &d.path,
                    &d.filename,
                    sig,
                    d.size_bytes,
                    d.mtime,
                    Some(&d.container),
                    d.artist.as_deref(),
                    d.project.as_deref(),
                ) {
                    Ok(_) => indexed += 1,
                    Err(_) => dupes += 1,
                }
            }
            let _ = conn.execute_batch("COMMIT");
        }

        let skipped = unchanged + dupes;
        let secs = started.elapsed().as_secs_f64().max(0.001);
        let mb = bytes_done.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0);

        let _ = app.emit(
            "scan:progress",
            ScanProgress {
                phase: "done".into(),
                files_discovered: total,
                files_indexed: indexed,
                files_skipped: skipped,
                current_path: None,
                done: true,
            },
        );
        log(
            &app,
            format!(
                "Scan complete in {secs:.1}s: {indexed} indexed, {unchanged} unchanged, {dupes} duplicates — {mb:.0} MB fingerprinted ({:.0} MB/s)",
                mb / secs
            ),
        );
    });
}

#[tauri::command]
fn cancel_scan(state: State<'_, Arc<AppState>>) {
    state.scan_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn list_videos(
    limit: i64,
    offset: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<VideoFile>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_videos(&conn, limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_videos(ids: Vec<i64>, state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let mut conn = state.conn.lock().unwrap();
    db::delete_videos(&mut conn, &ids).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_videos_by_status(
    status: String,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let conn = state.conn.lock().unwrap();
    db::delete_videos_by_status(&conn, &status).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Pipeline
// ---------------------------------------------------------------------------

#[tauri::command]
fn process_video(video_id: i64, app: AppHandle, state: State<'_, Arc<AppState>>) {
    let state = state.inner().clone();
    std::thread::spawn(move || {
        pipeline::process_video(&app, &state, video_id);
    });
}

#[tauri::command]
fn process_all_pending(app: AppHandle, state: State<'_, Arc<AppState>>) {
    let state = state.inner().clone();
    std::thread::spawn(move || {
        pipeline::process_all_pending(&app, &state);
    });
}

// ---------------------------------------------------------------------------
// Clips / review
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_clips(filter: ClipFilter, state: State<'_, Arc<AppState>>) -> Result<Vec<Clip>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_clips(&conn, &filter).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_clip_approval(
    clip_id: i64,
    approved: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::set_clip_approval(&conn, clip_id, approved).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_clips_approval(
    clip_ids: Vec<i64>,
    approved: bool,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let mut conn = state.conn.lock().unwrap();
    db::set_clips_approval(&mut conn, &clip_ids, approved).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Music-video editing sessions
// ---------------------------------------------------------------------------

#[tauri::command]
fn create_edit_session(
    name: String,
    artist: Option<String>,
    sequence_fps: Option<f64>,
    state: State<'_, Arc<AppState>>,
) -> Result<EditSession, String> {
    let conn = state.conn.lock().unwrap();
    let fps = sequence_fps.unwrap_or(24.0);
    let id = db::insert_session(&conn, &name, artist.as_deref(), fps)
        .map_err(|e| e.to_string())?;
    db::get_session(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "session not found after insert".to_string())
}

#[tauri::command]
fn list_edit_sessions(state: State<'_, Arc<AppState>>) -> Result<Vec<EditSession>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_sessions(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_edit_session(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<EditSession>, String> {
    let conn = state.conn.lock().unwrap();
    db::get_session(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_edit_session(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::delete_session(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_session_media(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SessionMedia>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_session_media(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn add_session_media(
    session_id: i64,
    paths: Vec<String>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SessionMedia>, String> {
    let conn = state.conn.lock().unwrap();
    let settings = state.load_settings(&conn);
    let work_dir = state.work_dir();
    let session = db::get_session(&conn, session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "session not found".to_string())?;
    session::ingest_paths(&conn, &app, &settings, &work_dir, &session, &paths)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_session_media_role(
    media_id: i64,
    role: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::set_media_role(&conn, media_id, &role, true).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_session_media(
    media_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::delete_session_media(&conn, media_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn analyze_master_audio(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<MasterAnalysis, String> {
    // Read the master path without holding the lock during the heavy decode.
    let (master_path, hint) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        (session.master_path, session.master_duration)
    };
    let master_path = master_path
        .filter(|p| !p.is_empty())
        .ok_or_else(|| "no master audio set for this session".to_string())?;

    let analysis = audio::analyze_master(std::path::Path::new(&master_path), hint)
        .ok_or_else(|| "could not analyze master audio".to_string())?;

    let conn = state.conn.lock().unwrap();
    db::set_session_analysis(&conn, session_id, analysis.bpm, &analysis)
        .map_err(|e| e.to_string())?;
    Ok(analysis)
}

#[tauri::command]
fn get_master_analysis(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<MasterAnalysis>, String> {
    let conn = state.conn.lock().unwrap();
    db::get_session_analysis(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn build_session_edl(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<EditSegment>, String> {
    let mut conn = state.conn.lock().unwrap();
    let session = db::get_session(&conn, session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "session not found".to_string())?;
    let analysis = db::get_session_analysis(&conn, session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "analyze the master audio first".to_string())?;
    let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
    let profile = db::get_edit_profile(&conn).map_err(|e| e.to_string())?;

    let segments = editor::build_edl(&session, &analysis, &media, &profile);
    if segments.is_empty() {
        return Err("no usable footage to build an edit".to_string());
    }
    db::replace_edit_segments(&mut conn, session_id, &segments).map_err(|e| e.to_string())?;
    db::list_edit_segments(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_session_edl(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<EditSegment>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_edit_segments(&conn, session_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_session_edit(
    session_id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let (session, segments, media, out_root) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let segments = db::list_edit_segments(&conn, session_id).map_err(|e| e.to_string())?;
        let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
        let settings = state.load_settings(&conn);
        let out_root = if settings.output_dir.is_empty() {
            state.data_dir.join("edits")
        } else {
            PathBuf::from(settings.output_dir).join("edits")
        };
        (session, segments, media, out_root)
    };
    if segments.is_empty() {
        return Err("build the edit first".to_string());
    }
    let folder = premiere::export_premiere(&out_root, &session, &segments, &media)
        .map_err(|e| e.to_string())?;
    {
        let conn = state.conn.lock().unwrap();
        let _ = db::set_session_status(&conn, session_id, "exported");
    }
    log(&app, format!("Exported edit → {}", folder.to_string_lossy()));
    Ok(folder.to_string_lossy().to_string())
}

#[tauri::command]
fn get_edit_profile(state: State<'_, Arc<AppState>>) -> Result<EditProfile, String> {
    let conn = state.conn.lock().unwrap();
    db::get_edit_profile(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_edit_profile(
    profile: EditProfile,
    state: State<'_, Arc<AppState>>,
) -> Result<EditProfile, String> {
    let conn = state.conn.lock().unwrap();
    db::set_edit_profile(&conn, &profile).map_err(|e| e.to_string())?;
    Ok(profile)
}

/// Nudge the global edit profile from a one-tap feedback signal. This is the
/// lightweight "training" loop — every correction makes future edits better.
#[tauri::command]
fn record_edit_feedback(
    feedback: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EditProfile, String> {
    let conn = state.conn.lock().unwrap();
    let mut p = db::get_edit_profile(&conn).map_err(|e| e.to_string())?;
    let step = 0.12;
    match feedback.as_str() {
        "faster" => p.cadence = (p.cadence - step).max(0.4),
        "slower" => p.cadence = (p.cadence + step).min(2.0),
        "more_performance" => p.performance_bias = (p.performance_bias + step).min(1.0),
        "more_story" => p.performance_bias = (p.performance_bias - step).max(0.0),
        "more_broll" => p.broll_freq = (p.broll_freq + step).min(1.0),
        "less_broll" => p.broll_freq = (p.broll_freq - step).max(0.0),
        "more_slowmo" => p.slowmo_affinity = (p.slowmo_affinity + step).min(1.0),
        "less_slowmo" => p.slowmo_affinity = (p.slowmo_affinity - step).max(0.0),
        "more_variation" => p.variation = (p.variation + step).min(1.0),
        "less_variation" => p.variation = (p.variation - step).max(0.0),
        other => return Err(format!("unknown feedback: {other}")),
    }
    p.samples += 1;
    db::set_edit_profile(&conn, &p).map_err(|e| e.to_string())?;
    Ok(p)
}

// ---------------------------------------------------------------------------
// AI B-roll Studio — capture style + generate coherent B-roll (NVIDIA)
// ---------------------------------------------------------------------------

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Capture a visual style fingerprint (palette + descriptor + keyframes) from
/// the session's own footage so generated B-roll stays on-brand.
#[tauri::command]
fn capture_style_reference(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<StyleReference, String> {
    let conn = state.conn.lock().unwrap();
    let session = db::get_session(&conn, session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "session not found".to_string())?;
    let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
    drop(conn);
    Ok(genai::build_style_reference(&session, &media, 6))
}

/// Generate `count` coherent B-roll shots with NVIDIA's hosted models. When
/// `animate` is true each still is also turned into a short video clip. Without
/// an API key the shots are still planned (prompt only) so the user sees the
/// plan. Emits `broll:progress` events; persists each candidate as it lands.
/// Render a still image for a shot/B-roll. Prefers an installed engine model
/// (FLUX / Qwen) when an AI Engine URL is configured in Settings, and falls
/// back to the NVIDIA cloud image model otherwise. Returns the PNG bytes.
fn render_still(settings: &AppSettings, nvidia_key: &str, prompt: &str, seed: i64) -> Option<Vec<u8>> {
    if engine::is_configured(settings) {
        let model = if settings.image_model.trim().is_empty() {
            "flux-dev"
        } else {
            settings.image_model.trim()
        };
        match engine::generate_image(settings, prompt, model, 1280, 720, 28, None, seed) {
            Ok(png) => return Some(png),
            Err(e) => eprintln!("[engine] image generation failed, trying NVIDIA: {e}"),
        }
    }
    if !nvidia_key.is_empty() {
        if let Some(png) = genai::generate_image(nvidia_key, prompt, seed) {
            return Some(png);
        }
    }
    None
}

/// Edit a still with a text instruction. Prefers the installed engine edit
/// model (Qwen-Image-Edit) and falls back to NVIDIA's hosted FLUX.2 Klein 4B
/// whenever the engine (H200) is off. Returns the edited image bytes.
fn edit_still(
    settings: &AppSettings,
    nvidia_key: &str,
    image: &[u8],
    prompt: &str,
    seed: i64,
) -> Option<Vec<u8>> {
    if engine::is_configured(settings) {
        let model = if settings.image_model.trim().is_empty() {
            "qwen-image"
        } else {
            settings.image_model.trim()
        };
        match engine::edit_image(settings, image, prompt, model, 30, seed) {
            Ok(png) => return Some(png),
            Err(e) => eprintln!("[engine] image edit failed, trying NVIDIA: {e}"),
        }
    }
    if !nvidia_key.is_empty() {
        if let Some(png) = genai::edit_image(nvidia_key, image, prompt, seed) {
            return Some(png);
        }
    }
    None
}

/// Turn a still into a moving clip. Prefers real image-to-video on the engine
/// (LTX-2.3 / Wan2.2) when a video model is configured, otherwise renders the
/// reliable local Ken Burns animation. Writes an MP4 to `out` and returns true.
#[allow(clippy::too_many_arguments)]
fn render_clip(
    settings: &AppSettings,
    img_path: &std::path::Path,
    png: &[u8],
    prompt: &str,
    out: &std::path::Path,
    seconds: f64,
    fps: f64,
    variant: u8,
    seed: i64,
) -> bool {
    if engine::is_configured(settings) && !settings.video_model.trim().is_empty() {
        let f = if fps.is_finite() && fps > 1.0 { fps } else { 24.0 };
        let frames = ((seconds * f).round() as i64).clamp(9, 257) as u32;
        match engine::generate_video(
            settings,
            prompt,
            settings.video_model.trim(),
            "i2v",
            1280,
            704,
            frames,
            f as u32,
            Some(png),
            None,
            seed,
        ) {
            Ok(mp4) => {
                if let Some(parent) = out.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(out, &mp4).is_ok() && out.exists() {
                    return true;
                }
            }
            Err(e) => eprintln!("[engine] image-to-video failed, using Ken Burns: {e}"),
        }
    }

    // Real AI motion via Hugging Face Inference Providers (Wan 2.2 i2v) when an
    // HF token is configured — turns the generated still into true motion
    // instead of the local Ken Burns pan. Falls back to Ken Burns on any error.
    let hf = genai::resolve_hf_token(settings);
    if !hf.is_empty() {
        if let Some(mp4) = genai::image_to_video_hf(&hf, png, prompt) {
            if let Some(parent) = out.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::write(out, &mp4).is_ok() && out.exists() {
                return true;
            }
        }
        eprintln!("[hf-i2v] failed, using Ken Burns");
    }

    genai::animate_still_local(img_path, out, seconds, fps, variant)
}

#[tauri::command]
fn generate_broll(
    session_id: i64,
    count: i64,
    animate: bool,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<BrollCandidate>, String> {
    let (session, media, analysis, settings) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
        let analysis = db::get_session_analysis(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "analyze the master audio first".to_string())?;
        let settings = state.load_settings(&conn);
        (session, media, analysis, settings)
    };

    let key = genai::resolve_key(&settings);
    let style = genai::build_style_reference(&session, &media, 6);
    let n = count.clamp(1, 12) as usize;
    let plans = genai::plan_broll(&style, &analysis, n);
    let total = plans.len() as u64;

    let out_dir = state.work_dir().join("broll").join(session_id.to_string());
    let _ = std::fs::create_dir_all(&out_dir);

    let emit = |stage: &str, message: String, processed: u64, done: bool| {
        let _ = app.emit(
            "broll:progress",
            serde_json::json!({
                "sessionId": session_id,
                "stage": stage,
                "message": message,
                "processed": processed,
                "total": total,
                "done": done,
            }),
        );
    };

    for (i, plan) in plans.into_iter().enumerate() {
        emit(
            "generate",
            format!("Generating B-roll {}/{}: {}", i + 1, total, plan.idea),
            i as u64,
            false,
        );

        let mut cand = BrollCandidate {
            id: 0,
            session_id,
            section: plan.section.clone(),
            idea: plan.idea.clone(),
            prompt: plan.prompt.clone(),
            image_path: None,
            video_path: None,
            thumbnail_path: None,
            status: "planned".into(),
            note: None,
            created_at: String::new(),
        };

        if let Some(png) = render_still(&settings, &key, &plan.prompt, plan.seed) {
            let img_path = out_dir.join(format!("img_{}_{}.png", i, nanos()));
            if std::fs::write(&img_path, &png).is_ok() {
                cand.image_path = Some(img_path.to_string_lossy().to_string());
                cand.thumbnail_path = cand.image_path.clone();
                cand.status = "image".into();
            }

            if animate && cand.status == "image" {
                emit(
                    "animate",
                    format!("Animating B-roll {}/{} to video…", i + 1, total),
                    i as u64,
                    false,
                );
                let img_path = std::path::PathBuf::from(cand.image_path.clone().unwrap());
                let vid_path = out_dir.join(format!("clip_{}_{}.mp4", i, nanos()));
                let secs = genai::section_clip_seconds(&plan.section);
                let animated = render_clip(
                    &settings,
                    &img_path,
                    &png,
                    &plan.prompt,
                    &vid_path,
                    secs,
                    session.sequence_fps,
                    i as u8,
                    plan.seed,
                );
                if animated {
                    cand.video_path = Some(vid_path.to_string_lossy().to_string());
                    cand.status = "video".into();
                    let thumb = out_dir.join(format!("thumb_{}_{}.jpg", i, nanos()));
                    if splitter::extract_thumbnail(&vid_path, secs * 0.4, &thumb) {
                        cand.thumbnail_path = Some(thumb.to_string_lossy().to_string());
                    }
                } else {
                    cand.note = Some("Image ready — animation failed.".into());
                }
            }
        } else if !engine::is_configured(&settings) && key.is_empty() {
            cand.note =
                Some("Configure the AI Engine or add an NVIDIA API key in Settings.".into());
        } else {
            cand.status = "failed".into();
            cand.note = Some("Image generation failed (check engine / NVIDIA quota).".into());
        }

        let conn = state.conn.lock().unwrap();
        if let Ok(id) = db::insert_broll(&conn, &cand) {
            cand.id = id;
        }
    }

    emit("done", "B-roll generation complete".into(), total, true);
    log(&app, format!("Generated {total} B-roll candidate(s)"));

    let conn = state.conn.lock().unwrap();
    db::list_broll(&conn, session_id).map_err(|e| e.to_string())
}

/// Generate a full music video for the session with our own Boostify workflow:
/// the analyzed master (sections/beats) drives a beat-aligned storyboard, each
/// shot is rendered as an on-brand NVIDIA FLUX still, animated locally into a
/// moving clip, then all shots + the master audio are assembled into one MP4.
/// Returns the path of the final video. Emits `mvgen:progress` events.
#[tauri::command]
fn generate_music_video(
    session_id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let (session, media, analysis, settings) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
        let analysis = db::get_session_analysis(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "analyze the master audio first".to_string())?;
        let settings = state.load_settings(&conn);
        (session, media, analysis, settings)
    };

    let key = genai::resolve_key(&settings);
    if key.is_empty() && !engine::is_configured(&settings) {
        return Err(
            "Configure the AI Engine or add an NVIDIA API key in Settings to generate the music video."
                .into(),
        );
    }

    let master = media
        .iter()
        .find(|m| m.role == "master")
        .map(|m| m.path.clone());

    let style = genai::build_style_reference(&session, &media, 6);
    let shots = mvgen::plan_shots(&style, &analysis);
    let total = shots.len() as u64;

    let out_dir = state
        .work_dir()
        .join("mvgen")
        .join(session_id.to_string())
        .join(nanos().to_string());
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;

    let emit = |stage: &str, message: String, processed: u64, done: bool| {
        let _ = app.emit(
            "mvgen:progress",
            serde_json::json!({
                "sessionId": session_id,
                "stage": stage,
                "message": message,
                "processed": processed,
                "total": total,
                "done": done,
            }),
        );
    };

    let mut clips: Vec<std::path::PathBuf> = Vec::new();
    for shot in &shots {
        emit(
            "generate",
            format!("Shot {}/{}: {}", shot.index + 1, total, shot.idea),
            shot.index as u64,
            false,
        );

        let png = match render_still(&settings, &key, &shot.prompt, shot.seed) {
            Some(p) => p,
            None => continue,
        };
        let img_path = out_dir.join(format!("shot_{:03}.png", shot.index));
        if std::fs::write(&img_path, &png).is_err() {
            continue;
        }

        emit(
            "animate",
            format!("Animating shot {}/{}…", shot.index + 1, total),
            shot.index as u64,
            false,
        );
        let clip_path = out_dir.join(format!("shot_{:03}.mp4", shot.index));
        if render_clip(
            &settings,
            &img_path,
            &png,
            &shot.prompt,
            &clip_path,
            shot.duration,
            session.sequence_fps,
            shot.motion,
            shot.seed,
        ) {
            clips.push(clip_path);
        }
    }

    if clips.is_empty() {
        return Err("No shots were generated (check the engine / NVIDIA quota).".into());
    }

    emit("assemble", "Assembling the final music video…".into(), total, false);
    let final_path = out_dir.join("music_video.mp4");
    let audio_ref = master.as_deref().map(std::path::Path::new);
    if !mvgen::assemble(&clips, audio_ref, &final_path, session.sequence_fps) {
        return Err("Failed to assemble the final video.".into());
    }

    emit("done", "Music video ready".into(), total, true);
    log(
        &app,
        format!("Generated music video for session {session_id}: {} shots", clips.len()),
    );

    Ok(final_path.to_string_lossy().to_string())
}

/// Probe the private AI Engine and report whether it is configured, reachable,
/// and which installed models it serves (so the UI can show a status badge and
/// populate the model selectors).
#[tauri::command]
fn ai_engine_status(state: State<'_, Arc<AppState>>) -> EngineStatus {
    let settings = {
        let conn = state.conn.lock().unwrap();
        state.load_settings(&conn)
    };
    engine::status(&settings)
}

/// Generate an original music track with the installed ACE-Step model and save
/// it under the session work dir. Returns the absolute path of the WAV file.
/// Requires the AI Engine to be configured (this is the only music backend).
#[tauri::command]
fn generate_music_track(
    prompt: String,
    duration_seconds: i64,
    lyrics: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let settings = {
        let conn = state.conn.lock().unwrap();
        state.load_settings(&conn)
    };
    if !engine::is_configured(&settings) {
        return Err("Configure the AI Engine URL in Settings to generate music.".into());
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe the music you want to generate.".into());
    }

    let model = if settings.music_model.trim().is_empty() {
        "ace-step-xl-base"
    } else {
        settings.music_model.trim()
    };
    let kind = if lyrics.as_deref().map(|l| !l.trim().is_empty()).unwrap_or(false) {
        "song"
    } else {
        "instrumental"
    };
    let secs = duration_seconds.clamp(4, 300);
    let seed = (nanos() as i64) & 0x7fff_ffff;

    log(&app, format!("Generating music ({model}, {secs}s)…"));
    let wav = engine::generate_music(
        &settings,
        prompt,
        model,
        kind,
        secs,
        lyrics.as_deref(),
        None,
        seed,
    )?;

    let out_dir = state.work_dir().join("music");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("track_{}.wav", nanos()));
    std::fs::write(&out_path, &wav).map_err(|e| e.to_string())?;
    log(&app, format!("Music saved: {}", out_path.display()));

    Ok(out_path.to_string_lossy().to_string())
}

/// Generate a still image from a text prompt. Prefers the installed engine
/// image model, falling back to NVIDIA's hosted FLUX.2 Klein 4B when the engine
/// (H200) is off. Returns the saved PNG path so the UI can preview it.
#[tauri::command]
fn ai_generate_image(
    prompt: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let settings = {
        let conn = state.conn.lock().unwrap();
        state.load_settings(&conn)
    };
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe the image you want to generate.".into());
    }
    let key = genai::resolve_key(&settings);
    if !engine::is_configured(&settings) && key.is_empty() {
        return Err(
            "Set the AI Engine URL or an NVIDIA API key in Settings to generate images.".into(),
        );
    }
    let seed = (nanos() as i64) & 0x7fff_ffff;
    log(&app, "Generating image…".to_string());
    let png = render_still(&settings, &key, prompt, seed)
        .ok_or_else(|| "image generation failed (engine off and NVIDIA fallback failed).".to_string())?;

    let out_dir = state.work_dir().join("ai_images");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("img_{}.png", nanos()));
    std::fs::write(&out_path, &png).map_err(|e| e.to_string())?;
    log(&app, format!("Image saved: {}", out_path.display()));
    Ok(out_path.to_string_lossy().to_string())
}

/// Edit an existing image with a text instruction. Prefers the installed engine
/// edit model, falling back to NVIDIA FLUX.2 Klein 4B when the engine is off.
/// Returns the saved edited PNG path.
#[tauri::command]
fn ai_edit_image(
    image_path: String,
    prompt: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let settings = {
        let conn = state.conn.lock().unwrap();
        state.load_settings(&conn)
    };
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Describe the edit you want to make.".into());
    }
    let src = std::path::Path::new(&image_path);
    if !src.exists() {
        return Err("the selected image file does not exist".into());
    }
    let bytes = std::fs::read(src).map_err(|e| format!("read image: {e}"))?;
    let key = genai::resolve_key(&settings);
    if !engine::is_configured(&settings) && key.is_empty() {
        return Err(
            "Set the AI Engine URL or an NVIDIA API key in Settings to edit images.".into(),
        );
    }
    let seed = (nanos() as i64) & 0x7fff_ffff;
    log(&app, "Editing image…".to_string());
    let edited = edit_still(&settings, &key, &bytes, prompt, seed)
        .ok_or_else(|| "image edit failed (engine off and NVIDIA fallback failed).".to_string())?;

    let out_dir = state.work_dir().join("ai_images");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let out_path = out_dir.join(format!("edit_{}.png", nanos()));
    std::fs::write(&out_path, &edited).map_err(|e| e.to_string())?;
    log(&app, format!("Edited image saved: {}", out_path.display()));
    Ok(out_path.to_string_lossy().to_string())
}

#[tauri::command]
fn list_broll(
    session_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<BrollCandidate>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_broll(&conn, session_id).map_err(|e| e.to_string())
}

/// Insert an animated B-roll candidate into the session as story footage so the
/// edit engine can place it. Returns the new media row (rebuild the EDL after).
#[tauri::command]
fn insert_broll(
    candidate_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<SessionMedia, String> {
    let (mut cand, session) = {
        let conn = state.conn.lock().unwrap();
        let cand = db::get_broll(&conn, candidate_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "b-roll candidate not found".to_string())?;
        let session = db::get_session(&conn, cand.session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        (cand, session)
    };

    let video_path = if let Some(vp) = cand.video_path.clone() {
        vp
    } else {
        // No animated clip yet — convert the still locally (Ken Burns) on the
        // fly so any generated image can always enter the timeline.
        let img = cand
            .image_path
            .clone()
            .ok_or_else(|| "this b-roll has no image to insert".to_string())?;
        let img_path = std::path::PathBuf::from(&img);
        if !img_path.exists() {
            return Err("the generated image file is missing".to_string());
        }
        let out_dir = state
            .work_dir()
            .join("broll")
            .join(cand.session_id.to_string());
        let _ = std::fs::create_dir_all(&out_dir);
        let vid_path = out_dir.join(format!("clip_{}_{}.mp4", candidate_id, nanos()));
        let secs = genai::section_clip_seconds(&cand.section);
        if !genai::animate_still_local(
            &img_path,
            &vid_path,
            secs,
            session.sequence_fps,
            (candidate_id % 4) as u8,
        ) {
            return Err("could not animate the image to a clip (ffmpeg)".to_string());
        }
        let vp = vid_path.to_string_lossy().to_string();
        cand.video_path = Some(vp.clone());
        cand.status = "video".into();
        vp
    };
    let vpath = std::path::Path::new(&video_path);
    if !vpath.exists() {
        return Err("the generated video file is missing".to_string());
    }

    let pr = probe::probe(vpath);
    let duration = pr.as_ref().and_then(|r| r.duration);
    let container_fps = pr.as_ref().and_then(|r| r.fps);
    let source_fps = pr.as_ref().and_then(|r| r.r_fps.or(r.fps));
    let (is_slow_mo, speed_pct) = probe::slow_mo_plan(source_fps, session.sequence_fps);
    let width = pr.as_ref().and_then(|r| r.width);
    let height = pr.as_ref().and_then(|r| r.height);

    let thumbs_dir = state.work_dir().join("session_thumbs");
    let _ = std::fs::create_dir_all(&thumbs_dir);
    let thumb_out = thumbs_dir.join(format!("broll_{}_{}.jpg", candidate_id, nanos()));
    let mid = duration.map(|d| d * 0.5).unwrap_or(1.0);
    let thumbnail_path = if splitter::extract_thumbnail(vpath, mid, &thumb_out) {
        Some(thumb_out.to_string_lossy().to_string())
    } else {
        cand.thumbnail_path.clone()
    };

    // Tag the clip as a wide story shot so the editor places it in calm sections.
    let mut analysis = SceneAnalysis::default();
    analysis.shot_type = Some("wide".into());
    analysis.mood = Some("cinematic".into());
    analysis.setting = Some(cand.idea.clone());
    analysis.labels = vec!["ai b-roll".into(), cand.section.clone()];

    let filename = format!("AI B-roll · {}", cand.idea);
    let mut media = SessionMedia {
        id: 0,
        session_id: cand.session_id,
        path: video_path.clone(),
        filename,
        kind: "video".into(),
        role: "story".into(),
        role_locked: true,
        duration_seconds: duration,
        width,
        height,
        container_fps,
        source_fps,
        is_slow_mo,
        speed_pct,
        layer_group: None,
        confidence: Some(0.9),
        audio_offset: None,
        sync_confidence: None,
        note: Some(format!("Generated B-roll for the {} section", cand.section)),
        analysis: Some(analysis),
        thumbnail_path,
        created_at: String::new(),
    };

    let conn = state.conn.lock().unwrap();
    let id = db::insert_session_media(&conn, &media).map_err(|e| e.to_string())?;
    media.id = id;
    cand.status = "inserted".into();
    let _ = db::update_broll(&conn, &cand);
    Ok(media)
}

#[tauri::command]
fn delete_broll(candidate_id: i64, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    if let Ok(Some(cand)) = db::get_broll(&conn, candidate_id) {
        for p in [cand.image_path, cand.video_path, cand.thumbnail_path]
            .into_iter()
            .flatten()
        {
            let _ = std::fs::remove_file(p);
        }
    }
    db::delete_broll(&conn, candidate_id).map_err(|e| e.to_string())
}

/// Turn a still B-roll candidate (status 'image') into an animated MP4 clip
/// (local Ken Burns) so it reads as motion video without re-generating.
#[tauri::command]
fn animate_broll(
    candidate_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<BrollCandidate, String> {
    let (mut cand, session) = {
        let conn = state.conn.lock().unwrap();
        let cand = db::get_broll(&conn, candidate_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "b-roll candidate not found".to_string())?;
        let session = db::get_session(&conn, cand.session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        (cand, session)
    };

    let img = cand
        .image_path
        .clone()
        .ok_or_else(|| "this b-roll has no image to animate".to_string())?;
    let img_path = std::path::PathBuf::from(&img);
    if !img_path.exists() {
        return Err("the generated image file is missing".to_string());
    }

    let out_dir = state
        .work_dir()
        .join("broll")
        .join(cand.session_id.to_string());
    let _ = std::fs::create_dir_all(&out_dir);
    let vid_path = out_dir.join(format!("clip_{}_{}.mp4", candidate_id, nanos()));
    let secs = genai::section_clip_seconds(&cand.section);
    if !genai::animate_still_local(
        &img_path,
        &vid_path,
        secs,
        session.sequence_fps,
        (candidate_id % 4) as u8,
    ) {
        return Err("could not animate the image to a clip (ffmpeg)".to_string());
    }

    cand.video_path = Some(vid_path.to_string_lossy().to_string());
    cand.status = "video".into();
    let thumb = out_dir.join(format!("thumb_{}_{}.jpg", candidate_id, nanos()));
    if splitter::extract_thumbnail(&vid_path, secs * 0.4, &thumb) {
        cand.thumbnail_path = Some(thumb.to_string_lossy().to_string());
    }

    let conn = state.conn.lock().unwrap();
    db::update_broll(&conn, &cand).map_err(|e| e.to_string())?;
    Ok(cand)
}

/// Lip-sync alignment: cross-correlate every performance clip's own audio
/// against the master song and store the offset so the edit engine can place
/// each take so the artist's mouth matches the music. Returns the refreshed
/// media list. Heavy ffmpeg decoding runs WITHOUT holding the DB lock.
#[tauri::command]
fn sync_performance_audio(
    session_id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SessionMedia>, String> {
    let (master_path, media) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
        (session.master_path, media)
    };

    let master_path = master_path
        .ok_or_else(|| "add the song master (audio) to this session first".to_string())?;
    let master = std::path::PathBuf::from(&master_path);
    if !master.exists() {
        return Err("the master audio file is missing".to_string());
    }

    let targets: Vec<&SessionMedia> = media
        .iter()
        .filter(|m| m.role == "performance" && m.kind == "video")
        .collect();
    let total = targets.len() as u64;
    let emit = |stage: &str, message: String, processed: u64, done: bool| {
        let _ = app.emit(
            "sync:progress",
            serde_json::json!({
                "sessionId": session_id,
                "stage": stage,
                "message": message,
                "processed": processed,
                "total": total,
                "done": done,
            }),
        );
    };

    if total == 0 {
        emit("done", "No performance clips to sync".into(), 0, true);
        return Ok(media);
    }

    let mut results: Vec<(i64, f64, f64)> = Vec::new();
    for (i, m) in targets.iter().enumerate() {
        emit(
            "align",
            format!("Aligning performance {}/{}: {}", i + 1, total, m.filename),
            i as u64,
            false,
        );
        let clip = std::path::Path::new(&m.path);
        if let Some((offset, conf)) = audio::align_to_master(&master, clip) {
            results.push((m.id, offset, conf));
        }
    }

    {
        let conn = state.conn.lock().unwrap();
        for (id, offset, conf) in &results {
            let _ = db::set_media_audio_sync(&conn, *id, *offset, *conf);
        }
    }
    let synced = results.iter().filter(|(_, _, c)| *c >= 0.15).count() as u64;
    emit(
        "done",
        format!("Synced {synced}/{total} performance clip(s)"),
        total,
        true,
    );
    log(&app, format!("Audio-synced {synced}/{total} performance clip(s)"));

    let conn = state.conn.lock().unwrap();
    db::list_session_media(&conn, session_id).map_err(|e| e.to_string())
}

/// Editor → Dataset bridge. Takes every footage clip in an editing session
/// (already auto-identified as performance vs story/b-roll), registers each
/// source video into the library and runs the full dataset pipeline on it:
/// scene-split into short clips, score, caption and label each clip as
/// `performance` / `b-roll`, then add them to the clip pool that feeds the
/// training dataset. Heavy work runs off the UI thread; emits
/// `session_dataset:progress`. Returns the number of source videos queued.
#[tauri::command]
fn export_session_to_dataset(
    session_id: i64,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let (artist, video_media) = {
        let conn = state.conn.lock().unwrap();
        let session = db::get_session(&conn, session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "session not found".to_string())?;
        let media = db::list_session_media(&conn, session_id).map_err(|e| e.to_string())?;
        let vids: Vec<SessionMedia> = media
            .into_iter()
            .filter(|m| m.kind == "video" && m.role != "master")
            .collect();
        (session.artist, vids)
    };

    if video_media.is_empty() {
        return Err("this session has no footage to send to the dataset".into());
    }

    let queued = video_media.len();
    let state = state.inner().clone();
    std::thread::spawn(move || {
        let total = video_media.len() as u64;
        let emit = |stage: &str, message: String, processed: u64, done: bool| {
            let _ = app.emit(
                "session_dataset:progress",
                serde_json::json!({
                    "sessionId": session_id,
                    "stage": stage,
                    "message": message,
                    "processed": processed,
                    "total": total,
                    "done": done,
                }),
            );
        };

        for (i, m) in video_media.iter().enumerate() {
            emit(
                "ingest",
                format!("Sending {} to the dataset", m.filename),
                i as u64,
                false,
            );

            let path = std::path::Path::new(&m.path);
            if !path.exists() {
                continue;
            }

            // Register (or resolve) the source video in the library so the
            // standard pipeline can process it into short, labeled clips.
            let video_id = {
                let conn = state.conn.lock().unwrap();
                let (size, mtime) = match std::fs::metadata(path) {
                    Ok(md) => (md.len(), scanner::mtime_secs(&md)),
                    Err(_) => (0, 0),
                };
                let sig = scanner::content_signature(path, size)
                    .unwrap_or_else(|_| format!("session-{session_id}-{}", m.id));
                let container = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("mp4")
                    .to_lowercase();
                let _ = db::insert_video(
                    &conn,
                    &m.path,
                    &m.filename,
                    &sig,
                    size as i64,
                    mtime,
                    Some(&container),
                    artist.as_deref(),
                    Some("editor-session"),
                );
                db::video_id_by_path(&conn, &m.path).ok().flatten()
            };

            if let Some(vid) = video_id {
                pipeline::process_video(&app, &state, vid);
            }
        }

        emit(
            "done",
            format!("Sent {total} source clip(s) to the dataset"),
            total,
            true,
        );
        log(
            &app,
            format!("Session {session_id}: {total} source video(s) ingested into the dataset"),
        );
    });

    Ok(queued)
}

#[tauri::command]
fn update_clip_caption(
    clip_id: i64,
    caption: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::update_clip_caption(&conn, clip_id, &caption).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_clip_tags(
    clip_id: i64,
    tags: Vec<String>,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    db::update_clip_tags(&conn, clip_id, &tags).map_err(|e| e.to_string())
}

#[tauri::command]
fn search_clips(query: String, state: State<'_, Arc<AppState>>) -> Result<Vec<Clip>, String> {
    let conn = state.conn.lock().unwrap();
    let filter = ClipFilter {
        query: Some(query),
        limit: Some(200),
        ..Default::default()
    };
    db::list_clips(&conn, &filter).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Dataset
// ---------------------------------------------------------------------------

#[tauri::command]
fn list_datasets(state: State<'_, Arc<AppState>>) -> Result<Vec<DatasetInfo>, String> {
    let conn = state.conn.lock().unwrap();
    db::list_datasets(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn export_dataset(
    name: String,
    format: String,
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, String> {
    let (clips, out_root) = {
        let conn = state.conn.lock().unwrap();
        let settings = state.load_settings(&conn);
        let clips = db::approved_clips(&conn).map_err(|e| e.to_string())?;
        let out_root = if settings.output_dir.is_empty() {
            state.data_dir.join("datasets")
        } else {
            PathBuf::from(settings.output_dir)
        };
        (clips, out_root)
    };

    if clips.is_empty() {
        return Err("No approved clips to export yet.".into());
    }

    let clip_count = clips.len() as i64;
    let root = dataset::export(&out_root, &name, &format, &clips).map_err(|e| e.to_string())?;

    {
        let conn = state.conn.lock().unwrap();
        let _ = db::insert_dataset(&conn, &name, &format, clip_count, &root);
    }
    log(&app, format!("Exported dataset → {}", root.to_string_lossy()));
    Ok(root.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Stats / system
// ---------------------------------------------------------------------------

#[tauri::command]
fn dashboard_stats(state: State<'_, Arc<AppState>>) -> Result<DashboardStats, String> {
    let conn = state.conn.lock().unwrap();
    let mut stats = db::dashboard_stats(&conn).map_err(|e| e.to_string())?;
    let gpu = system::detect_gpu();
    stats.gpu_mode = gpu.mode;
    let path = state.data_dir.to_string_lossy().to_string();
    let (free, total) = system::storage_for(&path);
    stats.storage_free_bytes = free;
    stats.storage_total_bytes = total;
    Ok(stats)
}

#[tauri::command]
fn gpu_info() -> GpuInfo {
    system::detect_gpu()
}

#[tauri::command]
fn check_dependencies() -> DependencyStatus {
    system::check_dependencies()
}

// ---------------------------------------------------------------------------
// Remote GPU server (Brev) control
// ---------------------------------------------------------------------------

fn resolve_gpu_instance(state: &State<'_, Arc<AppState>>) -> String {
    let conn = state.conn.lock().unwrap();
    let s = state.load_settings(&conn);
    let inst = s.gpu_instance.trim().to_string();
    if inst.is_empty() {
        "boostify1".to_string()
    } else {
        inst
    }
}

#[tauri::command]
fn gpu_server_status(state: State<'_, Arc<AppState>>) -> GpuServerStatus {
    let inst = resolve_gpu_instance(&state);
    gpu_server::status(&inst)
}

#[tauri::command]
fn gpu_server_start(state: State<'_, Arc<AppState>>) -> Result<GpuServerStatus, String> {
    let inst = resolve_gpu_instance(&state);
    gpu_server::start(&inst)
}

#[tauri::command]
fn gpu_server_stop(state: State<'_, Arc<AppState>>) -> Result<GpuServerStatus, String> {
    let inst = resolve_gpu_instance(&state);
    gpu_server::stop(&inst)
}

#[tauri::command]
fn gpu_server_list() -> Vec<GpuServerStatus> {
    gpu_server::list()
}

#[tauri::command]
fn gpu_server_start_named(name: String) -> Result<GpuServerStatus, String> {
    gpu_server::start(name.trim())
}

#[tauri::command]
fn gpu_server_stop_named(name: String) -> Result<GpuServerStatus, String> {
    gpu_server::stop(name.trim())
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_settings(state: State<'_, Arc<AppState>>) -> AppSettings {
    let conn = state.conn.lock().unwrap();
    state.load_settings(&conn)
}

#[tauri::command]
fn save_settings(settings: AppSettings, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let conn = state.conn.lock().unwrap();
    let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
    db::set_setting(&conn, "app_settings", &json).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_watch(path: String, enabled: bool, app: AppHandle) -> Result<(), String> {
    if enabled {
        watch::start(&app, path)
    } else {
        watch::stop(&app);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load API keys (NIM_API_KEY / OPENAI_API_KEY) from a .env file if present.
    // dotenv searches the current dir and its parents, so a project-root .env is
    // found in dev; ignore the error when no file exists.
    dotenvy::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            std::fs::create_dir_all(&data_dir).ok();
            std::fs::create_dir_all(data_dir.join("work")).ok();

            let db_path = data_dir.join("boostify.sqlite");
            let conn = Connection::open(&db_path).expect("open sqlite");
            db::init(&conn).expect("init schema");

            app.manage(Arc::new(AppState {
                conn: Mutex::new(conn),
                data_dir,
                scan_cancel: AtomicBool::new(false),
                watcher: Mutex::new(None),
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_folder,
            cancel_scan,
            list_videos,
            delete_videos,
            delete_videos_by_status,
            process_video,
            process_all_pending,
            list_clips,
            set_clip_approval,
            set_clips_approval,
            create_edit_session,
            list_edit_sessions,
            get_edit_session,
            delete_edit_session,
            list_session_media,
            add_session_media,
            set_session_media_role,
            delete_session_media,
            humo::humo_generate,
            humo::humo_status,
            relight::relight_clip,
            analyze_master_audio,
            get_master_analysis,
            build_session_edl,
            list_session_edl,
            export_session_edit,
            get_edit_profile,
            update_edit_profile,
            record_edit_feedback,
            capture_style_reference,
            generate_broll,
            generate_music_video,
            ai_engine_status,
            generate_music_track,
            ai_generate_image,
            ai_edit_image,
            list_broll,
            insert_broll,
            delete_broll,
            animate_broll,
            sync_performance_audio,
            export_session_to_dataset,
            update_clip_caption,
            update_clip_tags,
            search_clips,
            list_datasets,
            export_dataset,
            dashboard_stats,
            gpu_info,
            check_dependencies,
            gpu_server_status,
            gpu_server_start,
            gpu_server_stop,
            gpu_server_list,
            gpu_server_start_named,
            gpu_server_stop_named,
            get_settings,
            save_settings,
            set_watch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Boostify Dataset Studio");
}
