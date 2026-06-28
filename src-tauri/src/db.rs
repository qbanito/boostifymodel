use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::models::*;

/// Initialize the SQLite schema. Safe to call repeatedly.
pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS videos (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            path            TEXT NOT NULL UNIQUE,
            filename        TEXT NOT NULL,
            hash            TEXT UNIQUE,
            size_bytes      INTEGER NOT NULL DEFAULT 0,
            mtime           INTEGER NOT NULL DEFAULT 0,
            duration_seconds REAL,
            width           INTEGER,
            height          INTEGER,
            fps             REAL,
            codec           TEXT,
            container       TEXT,
            status          TEXT NOT NULL DEFAULT 'discovered',
            processed       INTEGER NOT NULL DEFAULT 0,
            dataset_id      INTEGER,
            artist          TEXT,
            project         TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_videos_hash ON videos(hash);
        CREATE INDEX IF NOT EXISTS idx_videos_status ON videos(status);
        CREATE INDEX IF NOT EXISTS idx_videos_path ON videos(path);

        CREATE TABLE IF NOT EXISTS clips (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            video_id        INTEGER NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
            path            TEXT NOT NULL,
            start_seconds   REAL NOT NULL DEFAULT 0,
            end_seconds     REAL NOT NULL DEFAULT 0,
            duration_seconds REAL NOT NULL DEFAULT 0,
            caption         TEXT,
            tags            TEXT NOT NULL DEFAULT '[]',
            quality_score   REAL,
            training_value  REAL,
            status          TEXT NOT NULL DEFAULT 'scored',
            approved        INTEGER,
            thumbnail_path  TEXT,
            analysis        TEXT,
            phash           TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_clips_video ON clips(video_id);
        CREATE INDEX IF NOT EXISTS idx_clips_approved ON clips(approved);

        CREATE TABLE IF NOT EXISTS datasets (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            format      TEXT NOT NULL,
            clip_count  INTEGER NOT NULL DEFAULT 0,
            path        TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS settings (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS edit_sessions (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            name            TEXT NOT NULL,
            artist          TEXT,
            master_path     TEXT,
            master_duration REAL,
            sequence_fps    REAL NOT NULL DEFAULT 24.0,
            status          TEXT NOT NULL DEFAULT 'draft',
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS session_media (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      INTEGER NOT NULL REFERENCES edit_sessions(id) ON DELETE CASCADE,
            path            TEXT NOT NULL,
            filename        TEXT NOT NULL,
            kind            TEXT NOT NULL DEFAULT 'video',
            role            TEXT NOT NULL DEFAULT 'unsorted',
            role_locked     INTEGER NOT NULL DEFAULT 0,
            duration_seconds REAL,
            width           INTEGER,
            height          INTEGER,
            container_fps   REAL,
            source_fps      REAL,
            is_slow_mo      INTEGER NOT NULL DEFAULT 0,
            speed_pct       REAL,
            layer_group     INTEGER,
            confidence      REAL,
            note            TEXT,
            analysis        TEXT,
            thumbnail_path  TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_session_media_session ON session_media(session_id);

        CREATE TABLE IF NOT EXISTS edit_segments (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      INTEGER NOT NULL REFERENCES edit_sessions(id) ON DELETE CASCADE,
            order_index     INTEGER NOT NULL DEFAULT 0,
            media_id        INTEGER NOT NULL,
            src_in          REAL NOT NULL DEFAULT 0,
            src_out         REAL NOT NULL DEFAULT 0,
            timeline_in     REAL NOT NULL DEFAULT 0,
            timeline_out    REAL NOT NULL DEFAULT 0,
            speed_pct       REAL NOT NULL DEFAULT 100,
            section         TEXT,
            reason          TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_edit_segments_session ON edit_segments(session_id);

        CREATE TABLE IF NOT EXISTS broll_candidates (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id      INTEGER NOT NULL REFERENCES edit_sessions(id) ON DELETE CASCADE,
            section         TEXT NOT NULL DEFAULT 'bridge',
            idea            TEXT NOT NULL DEFAULT '',
            prompt          TEXT NOT NULL DEFAULT '',
            image_path      TEXT,
            video_path      TEXT,
            thumbnail_path  TEXT,
            status          TEXT NOT NULL DEFAULT 'planned',
            note            TEXT,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_broll_session ON broll_candidates(session_id);
        "#,
    )?;
    // Migration for databases created before the `mtime` column existed.
    // Ignore the "duplicate column name" error when it is already present.
    let _ = conn.execute(
        "ALTER TABLE videos ADD COLUMN mtime INTEGER NOT NULL DEFAULT 0",
        [],
    );
    // Master-audio analysis columns on edit_sessions (added after Phase 0).
    let _ = conn.execute("ALTER TABLE edit_sessions ADD COLUMN bpm REAL", []);
    let _ = conn.execute("ALTER TABLE edit_sessions ADD COLUMN analysis TEXT", []);
    // Performance lip-sync alignment columns on session_media (Phase 4.5).
    let _ = conn.execute(
        "ALTER TABLE session_media ADD COLUMN audio_offset REAL",
        [],
    );
    let _ = conn.execute(
        "ALTER TABLE session_media ADD COLUMN sync_confidence REAL",
        [],
    );
    Ok(())
}

/// Load every indexed path with its (size_bytes, mtime) signature in one query.
/// Used to skip unchanged files during a rescan without hashing them.
pub fn all_path_sigs(conn: &Connection) -> Result<HashMap<String, (i64, i64)>> {
    let mut stmt = conn.prepare("SELECT path, size_bytes, mtime FROM videos")?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, sig) = row?;
        map.insert(path, sig);
    }
    Ok(map)
}

/// Load every known content hash so dedup can run in memory.
pub fn all_hashes(conn: &Connection) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare("SELECT hash FROM videos WHERE hash IS NOT NULL")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut set = HashSet::new();
    for row in rows {
        set.insert(row?);
    }
    Ok(set)
}

/// Returns true if a file with this hash already exists (never reprocess).
pub fn hash_exists(conn: &Connection, hash: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM videos WHERE hash = ?1", params![hash], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(found.is_some())
}

pub fn path_exists(conn: &Connection, path: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row("SELECT id FROM videos WHERE path = ?1", params![path], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(found.is_some())
}

#[allow(clippy::too_many_arguments)]
pub fn insert_video(
    conn: &Connection,
    path: &str,
    filename: &str,
    hash: &str,
    size_bytes: i64,
    mtime: i64,
    container: Option<&str>,
    artist: Option<&str>,
    project: Option<&str>,
) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO videos (path, filename, hash, size_bytes, mtime, container, artist, project, status)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'indexed')
           ON CONFLICT(path) DO NOTHING"#,
        params![path, filename, hash, size_bytes, mtime, container, artist, project],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
pub fn update_video_probe(
    conn: &Connection,
    id: i64,
    duration: Option<f64>,
    width: Option<i64>,
    height: Option<i64>,
    fps: Option<f64>,
    codec: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"UPDATE videos SET duration_seconds=?2, width=?3, height=?4, fps=?5, codec=?6,
              updated_at=datetime('now') WHERE id=?1"#,
        params![id, duration, width, height, fps, codec],
    )?;
    Ok(())
}

pub fn set_video_status(conn: &Connection, id: i64, status: &str, processed: bool) -> Result<()> {
    conn.execute(
        "UPDATE videos SET status=?2, processed=?3, updated_at=datetime('now') WHERE id=?1",
        params![id, status, processed as i64],
    )?;
    Ok(())
}

fn row_to_video(r: &rusqlite::Row) -> rusqlite::Result<VideoFile> {
    Ok(VideoFile {
        id: r.get("id")?,
        path: r.get("path")?,
        filename: r.get("filename")?,
        hash: r.get("hash")?,
        size_bytes: r.get("size_bytes")?,
        duration_seconds: r.get("duration_seconds")?,
        width: r.get("width")?,
        height: r.get("height")?,
        fps: r.get("fps")?,
        codec: r.get("codec")?,
        container: r.get("container")?,
        status: r.get("status")?,
        processed: r.get::<_, i64>("processed")? != 0,
        dataset_id: r.get("dataset_id")?,
        artist: r.get("artist")?,
        project: r.get("project")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
    })
}

pub fn list_videos(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<VideoFile>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM videos ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], row_to_video)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn pending_videos(conn: &Connection) -> Result<Vec<VideoFile>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM videos WHERE processed = 0 ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([], row_to_video)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_video(conn: &Connection, id: i64) -> Result<Option<VideoFile>> {
    let mut stmt = conn.prepare("SELECT * FROM videos WHERE id = ?1")?;
    let v = stmt.query_row(params![id], row_to_video).optional()?;
    Ok(v)
}

/// Resolve a video's id by its (unique) path. Used when re-registering footage
/// that may already exist in the library (insert does nothing on conflict).
pub fn video_id_by_path(conn: &Connection, path: &str) -> Result<Option<i64>> {
    let mut stmt = conn.prepare("SELECT id FROM videos WHERE path = ?1")?;
    let id = stmt
        .query_row(params![path], |r| r.get::<_, i64>(0))
        .optional()?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
pub fn insert_clip(
    conn: &Connection,
    video_id: i64,
    path: &str,
    start: f64,
    end: f64,
    caption: Option<&str>,
    tags: &[String],
    quality: Option<f64>,
    training: Option<f64>,
    status: &str,
    approved: Option<bool>,
    thumbnail: Option<&str>,
    analysis: Option<&SceneAnalysis>,
    phash: Option<&str>,
) -> Result<i64> {
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
    let analysis_json = analysis.and_then(|a| serde_json::to_string(a).ok());
    conn.execute(
        r#"INSERT INTO clips
           (video_id, path, start_seconds, end_seconds, duration_seconds, caption, tags,
            quality_score, training_value, status, approved, thumbnail_path, analysis, phash)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"#,
        params![
            video_id,
            path,
            start,
            end,
            end - start,
            caption,
            tags_json,
            quality,
            training,
            status,
            approved.map(|b| b as i64),
            thumbnail,
            analysis_json,
            phash
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_clip(r: &rusqlite::Row) -> rusqlite::Result<Clip> {
    let tags_json: String = r.get("tags")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let analysis_json: Option<String> = r.get("analysis")?;
    let analysis = analysis_json.and_then(|j| serde_json::from_str(&j).ok());
    let approved: Option<i64> = r.get("approved")?;
    Ok(Clip {
        id: r.get("id")?,
        video_id: r.get("video_id")?,
        path: r.get("path")?,
        start_seconds: r.get("start_seconds")?,
        end_seconds: r.get("end_seconds")?,
        duration_seconds: r.get("duration_seconds")?,
        caption: r.get("caption")?,
        tags,
        quality_score: r.get("quality_score")?,
        training_value: r.get("training_value")?,
        status: r.get("status")?,
        approved: approved.map(|v| v != 0),
        thumbnail_path: r.get("thumbnail_path")?,
        analysis,
        created_at: r.get("created_at")?,
    })
}

pub fn list_clips(conn: &Connection, filter: &ClipFilter) -> Result<Vec<Clip>> {
    let limit = filter.limit.unwrap_or(300);
    let offset = filter.offset.unwrap_or(0);
    let min_score = filter.min_score.unwrap_or(0.0);
    let approved_only = filter.approved_only.unwrap_or(false);

    let like = filter
        .query
        .as_ref()
        .map(|q| format!("%{}%", q.to_lowercase()));

    let mut sql = String::from(
        "SELECT * FROM clips WHERE COALESCE(quality_score,0) >= ?1",
    );
    if approved_only {
        sql.push_str(" AND approved = 1");
    }
    if like.is_some() {
        sql.push_str(
            " AND (LOWER(caption) LIKE ?4 OR LOWER(tags) LIKE ?4 OR LOWER(COALESCE(analysis,'')) LIKE ?4)",
        );
    }
    sql.push_str(" ORDER BY COALESCE(training_value, quality_score, 0) DESC LIMIT ?2 OFFSET ?3");

    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(like) = like {
        stmt.query_map(params![min_score, limit, offset, like], row_to_clip)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![min_score, limit, offset], row_to_clip)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

pub fn set_clip_approval(conn: &Connection, clip_id: i64, approved: bool) -> Result<()> {
    let status = if approved { "approved" } else { "rejected" };
    conn.execute(
        "UPDATE clips SET approved=?2, status=?3 WHERE id=?1",
        params![clip_id, approved as i64, status],
    )?;
    Ok(())
}

/// Approve or reject many clips in a single transaction. Returns the number of
/// rows updated.
pub fn set_clips_approval(conn: &mut Connection, clip_ids: &[i64], approved: bool) -> Result<usize> {
    if clip_ids.is_empty() {
        return Ok(0);
    }
    let status = if approved { "approved" } else { "rejected" };
    let tx = conn.transaction()?;
    let mut updated = 0usize;
    {
        let mut stmt = tx.prepare("UPDATE clips SET approved=?2, status=?3 WHERE id=?1")?;
        for &id in clip_ids {
            updated += stmt.execute(params![id, approved as i64, status])?;
        }
    }
    tx.commit()?;
    Ok(updated)
}

pub fn update_clip_caption(conn: &Connection, clip_id: i64, caption: &str) -> Result<()> {
    conn.execute(
        "UPDATE clips SET caption=?2 WHERE id=?1",
        params![clip_id, caption],
    )?;
    Ok(())
}

pub fn update_clip_tags(conn: &Connection, clip_id: i64, tags: &[String]) -> Result<()> {
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "UPDATE clips SET tags=?2 WHERE id=?1",
        params![clip_id, tags_json],
    )?;
    Ok(())
}

pub fn list_phashes(conn: &Connection) -> Result<Vec<(i64, String, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT id, phash, COALESCE(quality_score,0) FROM clips WHERE phash IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn mark_duplicate(conn: &Connection, clip_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE clips SET status='duplicate', approved=0 WHERE id=?1",
        params![clip_id],
    )?;
    Ok(())
}

pub fn dashboard_stats(conn: &Connection) -> Result<DashboardStats> {
    let videos_found: i64 =
        conn.query_row("SELECT COUNT(*) FROM videos", [], |r| r.get(0))?;
    let videos_processed: i64 =
        conn.query_row("SELECT COUNT(*) FROM videos WHERE processed=1", [], |r| {
            r.get(0)
        })?;
    let clips_created: i64 =
        conn.query_row("SELECT COUNT(*) FROM clips", [], |r| r.get(0))?;
    let clips_approved: i64 =
        conn.query_row("SELECT COUNT(*) FROM clips WHERE approved=1", [], |r| {
            r.get(0)
        })?;
    let clips_rejected: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clips WHERE approved=0",
        [],
        |r| r.get(0),
    )?;
    let dataset_size_bytes: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(size_bytes),0) FROM videos WHERE processed=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let avg_training_score: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(training_value),0) FROM clips WHERE approved=1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    Ok(DashboardStats {
        videos_found,
        videos_processed,
        clips_created,
        clips_approved,
        clips_rejected,
        avg_process_seconds: 0.0,
        dataset_size_bytes,
        gpu_mode: "cpu".into(),
        storage_free_bytes: 0,
        storage_total_bytes: 0,
        avg_training_score,
    })
}

pub fn list_datasets(conn: &Connection) -> Result<Vec<DatasetInfo>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, format, clip_count, created_at FROM datasets ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(DatasetInfo {
            id: r.get(0)?,
            name: r.get(1)?,
            format: r.get(2)?,
            clip_count: r.get(3)?,
            created_at: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn insert_dataset(
    conn: &Connection,
    name: &str,
    format: &str,
    clip_count: i64,
    path: &Path,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO datasets (name, format, clip_count, path) VALUES (?1,?2,?3,?4)",
        params![name, format, clip_count, path.to_string_lossy()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn approved_clips(conn: &Connection) -> Result<Vec<Clip>> {
    let mut stmt = conn.prepare("SELECT * FROM clips WHERE approved=1 ORDER BY id ASC")?;
    let rows = stmt.query_map([], row_to_clip)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(v)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=?2",
        params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Music-video editing sessions
// ---------------------------------------------------------------------------

pub fn insert_session(
    conn: &Connection,
    name: &str,
    artist: Option<&str>,
    sequence_fps: f64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO edit_sessions (name, artist, sequence_fps) VALUES (?1,?2,?3)",
        params![name, artist, sequence_fps],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<EditSession> {
    Ok(EditSession {
        id: r.get("id")?,
        name: r.get("name")?,
        artist: r.get("artist")?,
        master_path: r.get("master_path")?,
        master_duration: r.get("master_duration")?,
        sequence_fps: r.get("sequence_fps")?,
        status: r.get("status")?,
        created_at: r.get("created_at")?,
    })
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<EditSession>> {
    let mut stmt =
        conn.prepare("SELECT * FROM edit_sessions ORDER BY created_at DESC")?;
    let rows = stmt.query_map([], row_to_session)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_session(conn: &Connection, id: i64) -> Result<Option<EditSession>> {
    let mut stmt = conn.prepare("SELECT * FROM edit_sessions WHERE id = ?1")?;
    let s = stmt.query_row(params![id], row_to_session).optional()?;
    Ok(s)
}

pub fn set_session_master(
    conn: &Connection,
    id: i64,
    master_path: &str,
    master_duration: Option<f64>,
) -> Result<()> {
    conn.execute(
        "UPDATE edit_sessions SET master_path=?2, master_duration=?3 WHERE id=?1",
        params![id, master_path, master_duration],
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn set_session_status(conn: &Connection, id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE edit_sessions SET status=?2 WHERE id=?1",
        params![id, status],
    )?;
    Ok(())
}

pub fn delete_session(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM edit_sessions WHERE id=?1", params![id])?;
    Ok(())
}

/// Persist the master-audio analysis (BPM + beat grid + sections) for a session.
pub fn set_session_analysis(
    conn: &Connection,
    id: i64,
    bpm: f64,
    analysis: &MasterAnalysis,
) -> Result<()> {
    let json = serde_json::to_string(analysis).unwrap_or_else(|_| "null".into());
    conn.execute(
        "UPDATE edit_sessions SET bpm=?2, analysis=?3, status='classified' WHERE id=?1",
        params![id, bpm, json],
    )?;
    Ok(())
}

/// Load a previously stored master analysis, if any.
pub fn get_session_analysis(conn: &Connection, id: i64) -> Result<Option<MasterAnalysis>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT analysis FROM edit_sessions WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

#[allow(clippy::too_many_arguments)]
pub fn insert_session_media(conn: &Connection, m: &SessionMedia) -> Result<i64> {
    let analysis_json = m.analysis.as_ref().and_then(|a| serde_json::to_string(a).ok());
    conn.execute(
        r#"INSERT INTO session_media
           (session_id, path, filename, kind, role, role_locked, duration_seconds,
            width, height, container_fps, source_fps, is_slow_mo, speed_pct,
            layer_group, confidence, audio_offset, sync_confidence, note, analysis,
            thumbnail_path)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)"#,
        params![
            m.session_id,
            m.path,
            m.filename,
            m.kind,
            m.role,
            m.role_locked as i64,
            m.duration_seconds,
            m.width,
            m.height,
            m.container_fps,
            m.source_fps,
            m.is_slow_mo as i64,
            m.speed_pct,
            m.layer_group,
            m.confidence,
            m.audio_offset,
            m.sync_confidence,
            m.note,
            analysis_json,
            m.thumbnail_path,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn row_to_session_media(r: &rusqlite::Row) -> rusqlite::Result<SessionMedia> {
    let analysis_json: Option<String> = r.get("analysis")?;
    let analysis = analysis_json.and_then(|j| serde_json::from_str(&j).ok());
    let role_locked: i64 = r.get("role_locked")?;
    let is_slow_mo: i64 = r.get("is_slow_mo")?;
    Ok(SessionMedia {
        id: r.get("id")?,
        session_id: r.get("session_id")?,
        path: r.get("path")?,
        filename: r.get("filename")?,
        kind: r.get("kind")?,
        role: r.get("role")?,
        role_locked: role_locked != 0,
        duration_seconds: r.get("duration_seconds")?,
        width: r.get("width")?,
        height: r.get("height")?,
        container_fps: r.get("container_fps")?,
        source_fps: r.get("source_fps")?,
        is_slow_mo: is_slow_mo != 0,
        speed_pct: r.get("speed_pct")?,
        layer_group: r.get("layer_group")?,
        confidence: r.get("confidence")?,
        audio_offset: r.get("audio_offset").ok().flatten(),
        sync_confidence: r.get("sync_confidence").ok().flatten(),
        note: r.get("note")?,
        analysis,
        thumbnail_path: r.get("thumbnail_path")?,
        created_at: r.get("created_at")?,
    })
}

/// Store the computed lip-sync alignment of a performance clip to the master.
pub fn set_media_audio_sync(
    conn: &Connection,
    media_id: i64,
    offset: f64,
    confidence: f64,
) -> Result<()> {
    conn.execute(
        "UPDATE session_media SET audio_offset=?1, sync_confidence=?2 WHERE id=?3",
        params![offset, confidence, media_id],
    )?;
    Ok(())
}

pub fn list_session_media(conn: &Connection, session_id: i64) -> Result<Vec<SessionMedia>> {
    let mut stmt = conn
        .prepare("SELECT * FROM session_media WHERE session_id=?1 ORDER BY id ASC")?;
    let rows = stmt.query_map(params![session_id], row_to_session_media)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn session_media_path_exists(
    conn: &Connection,
    session_id: i64,
    path: &str,
) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM session_media WHERE session_id=?1 AND path=?2",
            params![session_id, path],
            |r| r.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

pub fn set_media_role(conn: &Connection, media_id: i64, role: &str, locked: bool) -> Result<()> {
    conn.execute(
        "UPDATE session_media SET role=?2, role_locked=?3 WHERE id=?1",
        params![media_id, role, locked as i64],
    )?;
    Ok(())
}

pub fn delete_session_media(conn: &Connection, media_id: i64) -> Result<()> {
    conn.execute("DELETE FROM session_media WHERE id=?1", params![media_id])?;
    Ok(())
}

/// Next free layer-group number for a session (max existing + 1, starting at 1).
#[allow(dead_code)]
pub fn next_layer_group(conn: &Connection, session_id: i64) -> Result<i64> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(layer_group) FROM session_media WHERE session_id=?1",
            params![session_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(max.unwrap_or(0) + 1)
}

// ---------------------------------------------------------------------------
// Edit-decision-list (EDL) segments
// ---------------------------------------------------------------------------

fn row_to_edit_segment(r: &rusqlite::Row) -> rusqlite::Result<EditSegment> {
    Ok(EditSegment {
        id: r.get("id")?,
        session_id: r.get("session_id")?,
        order_index: r.get("order_index")?,
        media_id: r.get("media_id")?,
        src_in: r.get("src_in")?,
        src_out: r.get("src_out")?,
        timeline_in: r.get("timeline_in")?,
        timeline_out: r.get("timeline_out")?,
        speed_pct: r.get("speed_pct")?,
        section: r.get("section")?,
        reason: r.get("reason")?,
    })
}

/// Replace the whole EDL for a session with a freshly built one (transactional).
pub fn replace_edit_segments(
    conn: &mut Connection,
    session_id: i64,
    segments: &[EditSegment],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM edit_segments WHERE session_id=?1",
        params![session_id],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO edit_segments
               (session_id, order_index, media_id, src_in, src_out,
                timeline_in, timeline_out, speed_pct, section, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for s in segments {
            stmt.execute(params![
                session_id,
                s.order_index,
                s.media_id,
                s.src_in,
                s.src_out,
                s.timeline_in,
                s.timeline_out,
                s.speed_pct,
                s.section,
                s.reason,
            ])?;
        }
    }
    tx.execute(
        "UPDATE edit_sessions SET status='edited' WHERE id=?1",
        params![session_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// List the stored EDL for a session in timeline order.
pub fn list_edit_segments(conn: &Connection, session_id: i64) -> Result<Vec<EditSegment>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM edit_segments WHERE session_id=?1 ORDER BY order_index ASC",
    )?;
    let rows = stmt.query_map(params![session_id], row_to_edit_segment)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Load the global editing profile (defaults when none stored yet).
pub fn get_edit_profile(conn: &Connection) -> Result<EditProfile> {
    let json = get_setting(conn, "edit_profile")?;
    Ok(json
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

/// Persist the global editing profile.
pub fn set_edit_profile(conn: &Connection, profile: &EditProfile) -> Result<()> {
    let json = serde_json::to_string(profile).unwrap_or_else(|_| "{}".into());
    set_setting(conn, "edit_profile", &json)
}

// ---------------------------------------------------------------------------
// AI B-roll candidates
// ---------------------------------------------------------------------------

fn row_to_broll(r: &rusqlite::Row) -> rusqlite::Result<BrollCandidate> {
    Ok(BrollCandidate {
        id: r.get("id")?,
        session_id: r.get("session_id")?,
        section: r.get("section")?,
        idea: r.get("idea")?,
        prompt: r.get("prompt")?,
        image_path: r.get("image_path")?,
        video_path: r.get("video_path")?,
        thumbnail_path: r.get("thumbnail_path")?,
        status: r.get("status")?,
        note: r.get("note")?,
        created_at: r.get("created_at")?,
    })
}

/// Insert a new B-roll candidate row, returning its id.
pub fn insert_broll(conn: &Connection, b: &BrollCandidate) -> Result<i64> {
    conn.execute(
        r#"INSERT INTO broll_candidates
           (session_id, section, idea, prompt, image_path, video_path,
            thumbnail_path, status, note)
           VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"#,
        params![
            b.session_id,
            b.section,
            b.idea,
            b.prompt,
            b.image_path,
            b.video_path,
            b.thumbnail_path,
            b.status,
            b.note,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update the mutable fields of a B-roll candidate.
pub fn update_broll(conn: &Connection, b: &BrollCandidate) -> Result<()> {
    conn.execute(
        r#"UPDATE broll_candidates
           SET section=?2, idea=?3, prompt=?4, image_path=?5, video_path=?6,
               thumbnail_path=?7, status=?8, note=?9
           WHERE id=?1"#,
        params![
            b.id,
            b.section,
            b.idea,
            b.prompt,
            b.image_path,
            b.video_path,
            b.thumbnail_path,
            b.status,
            b.note,
        ],
    )?;
    Ok(())
}

pub fn get_broll(conn: &Connection, id: i64) -> Result<Option<BrollCandidate>> {
    let mut stmt = conn.prepare("SELECT * FROM broll_candidates WHERE id=?1")?;
    let mut rows = stmt.query_map(params![id], row_to_broll)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn list_broll(conn: &Connection, session_id: i64) -> Result<Vec<BrollCandidate>> {
    let mut stmt =
        conn.prepare("SELECT * FROM broll_candidates WHERE session_id=?1 ORDER BY id DESC")?;
    let rows = stmt.query_map(params![session_id], row_to_broll)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn delete_broll(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM broll_candidates WHERE id=?1", params![id])?;
    Ok(())
}

