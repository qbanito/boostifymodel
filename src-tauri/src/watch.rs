use crate::scanner;
use crate::{db, AppState};
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// Start watching a folder. New video files are auto-indexed (and, when watch
/// mode is on, queued for processing). Replaces any existing watcher.
pub fn start(app: &AppHandle, path: String) -> Result<(), String> {
    let app_handle = app.clone();
    let state = app.state::<Arc<AppState>>().inner().clone();

    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        for p in event.paths {
            if !scanner::is_video(&p) {
                continue;
            }
            handle_new_file(&app_handle, &state, &p);
        }
    })
    .map_err(|e| e.to_string())?;

    watcher
        .watch(Path::new(&path), RecursiveMode::Recursive)
        .map_err(|e| e.to_string())?;

    // Keep the watcher alive by storing it in app state.
    let state = app.state::<Arc<AppState>>();
    *state.watcher.lock().unwrap() = Some(watcher);
    let _ = app.emit("app:log", format!("👁 Watching {path}"));
    Ok(())
}

/// Stop watching.
pub fn stop(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();
    *state.watcher.lock().unwrap() = None;
    let _ = app.emit("app:log", "👁 Watch mode stopped".to_string());
}

fn handle_new_file(app: &AppHandle, state: &AppState, path: &Path) {
    // Debounce-ish: ensure the file is readable/stable.
    let Ok(meta) = std::fs::metadata(path) else {
        return;
    };
    if meta.len() == 0 {
        return;
    }

    let hash = match scanner::content_signature(path, meta.len()) {
        Ok(h) => h,
        Err(_) => return,
    };

    let path_str = path.to_string_lossy().to_string();
    let conn = state.conn.lock().unwrap();
    if db::hash_exists(&conn, &hash).unwrap_or(false)
        || db::path_exists(&conn, &path_str).unwrap_or(false)
    {
        return; // never reprocess
    }

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let container = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let _ = db::insert_video(
        &conn,
        &path_str,
        &filename,
        &hash,
        meta.len() as i64,
        scanner::mtime_secs(&meta),
        Some(&container),
        None,
        None,
    );
    drop(conn);
    let _ = app.emit("app:log", format!("＋ New file indexed: {filename}"));
}
