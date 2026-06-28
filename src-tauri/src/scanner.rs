use anyhow::Result;
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};

/// Every container/codec extension we recognize as a video — format-agnostic.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    // Common
    "mp4", "mov", "mkv", "avi", "m4v", "webm", "flv", "wmv", "mpg", "mpeg", "ts", "m2ts", "mts",
    "m2v", "vob", "ogv", "3gp", "asf",
    // Broadcast / pro
    "mxf", "prores", "dnxhd", "dnxhr",
    // Cinema RAW
    "braw", "r3d", "ari", "arri",
    // Camera-specific
    "xavc", "crm", "cr2", "cr3", "dng", "gpr", // Canon / RAW / GoPro RAW
    "insv", "360", // 360 cameras
    "lrv", // DJI / GoPro low-res proxy
];

/// Directory names we never descend into (caches, system, VCS, proxies).
const DENY_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    ".Trash",
    ".Trashes",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".Spotlight-V100",
    ".fseventsd",
    ".cache",
    "CACHE",
    "Proxies",
];

/// For files larger than this we fingerprint via head+tail+size instead of
/// hashing every byte — orders of magnitude faster on multi-GB RAW footage.
const LARGE_FILE_THRESHOLD: u64 = 256 * 1024 * 1024; // 256 MiB
const SAMPLE_BYTES: usize = 8 * 1024 * 1024; // 8 MiB head & tail

/// Returns true if the path has a recognized video extension.
pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Modification time as whole seconds since the Unix epoch (0 on failure).
pub fn mtime_secs(meta: &Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Read up to `buf.len()` bytes, looping until the buffer is full or EOF.
fn fill(reader: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = reader.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

/// Compute a full SHA-256 over the file (streamed, 1 MiB buffer).
pub fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Content fingerprint used for dedup. Small files get a full SHA-256; large
/// media files get a sampled signature (size + first 8 MiB + last 8 MiB),
/// which is effectively unique for real footage while being far faster.
pub fn content_signature(path: &Path, size: u64) -> Result<String> {
    if size <= LARGE_FILE_THRESHOLD {
        return hash_file(path);
    }
    let mut hasher = Sha256::new();
    hasher.update(b"bds-sampled-v1");
    hasher.update(size.to_le_bytes());

    let mut f = File::open(path)?;
    let mut buf = vec![0u8; SAMPLE_BYTES];

    let n = fill(&mut f, &mut buf)?;
    hasher.update(&buf[..n]);

    f.seek(SeekFrom::End(-(SAMPLE_BYTES as i64)))?;
    let n = fill(&mut f, &mut buf)?;
    hasher.update(&buf[..n]);

    Ok(format!("{:x}", hasher.finalize()))
}

/// A discovered file on disk (before indexing).
pub struct Discovered {
    pub path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub mtime: i64,
    pub container: String,
    pub artist: Option<String>,
    pub project: Option<String>,
}

fn is_denied_dir(entry: &DirEntry) -> bool {
    entry.file_type().is_dir()
        && entry
            .file_name()
            .to_str()
            .map(|n| DENY_DIRS.contains(&n) || n.starts_with('.'))
            .unwrap_or(false)
}

/// Walk a directory tree (recursively) and yield every video file, pruning
/// junk/system directories. `on_progress(count, current_path)` is invoked as
/// files are discovered so the UI can show live discovery progress.
/// `artist` / `project` are inferred from the parent folder structure:
///   .../<artist>/<project>/<file>
pub fn discover(root: &Path, mut on_progress: impl FnMut(u64, &str)) -> Vec<Discovered> {
    let mut out = Vec::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_denied_dir(e));

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !entry.file_type().is_file() || !is_video(path) {
            continue;
        }
        // Skip macOS resource forks / hidden proxy files.
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("._") || name.starts_with('.') {
                continue;
            }
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let size_bytes = meta.len();
        if size_bytes == 0 {
            continue; // skip empty / still-copying files
        }
        let container = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let (artist, project) = infer_artist_project(root, path);
        let path_str = path.to_string_lossy().to_string();
        out.push(Discovered {
            path: path_str.clone(),
            filename: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            size_bytes: size_bytes as i64,
            mtime: mtime_secs(&meta),
            container,
            artist,
            project,
        });
        let count = out.len() as u64;
        if count % 64 == 0 {
            on_progress(count, &path_str);
        }
    }
    on_progress(out.len() as u64, "");
    out
}

/// Infer artist/project from the relative folder path under the scan root.
fn infer_artist_project(root: &Path, file: &Path) -> (Option<String>, Option<String>) {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let parts: Vec<String> = rel
        .parent()
        .map(|p| {
            p.components()
                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let artist = parts.first().cloned();
    let project = parts.get(1).cloned().or_else(|| artist.clone());
    (artist, project)
}
