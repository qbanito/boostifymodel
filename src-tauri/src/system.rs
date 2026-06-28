use crate::models::{DependencyStatus, GpuInfo};
use std::process::Command;

/// Detect the best available compute device: CUDA (NVIDIA) > Metal (Apple) > CPU.
pub fn detect_gpu() -> GpuInfo {
    // 1. NVIDIA CUDA via nvidia-smi
    if let Ok(out) = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if out.status.success() {
            let name = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("NVIDIA GPU")
                .trim()
                .to_string();
            if !name.is_empty() {
                return GpuInfo {
                    mode: "cuda".into(),
                    device: name,
                    available: true,
                };
            }
        }
    }

    // 2. Apple Silicon → Metal
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        return GpuInfo {
            mode: "metal".into(),
            device: "Apple GPU (Metal)".into(),
            available: true,
        };
    }

    // 3. CPU fallback
    GpuInfo {
        mode: "cpu".into(),
        device: "CPU".into(),
        available: false,
    }
}

fn binary_works(bin: &str, env_override: &str) -> bool {
    Command::new(resolve_bin(bin, env_override))
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the path to an external binary (ffmpeg/ffprobe), in priority order:
///   1. explicit env override (e.g. FFMPEG_PATH / FFPROBE_PATH)
///   2. a known install location that actually exists on disk — a GUI app
///      launched outside a login shell often does NOT inherit ~/.local/bin or
///      /opt/homebrew/bin on its PATH, so we probe them directly
///   3. the bare name, resolved via PATH at spawn time
pub fn resolve_bin(name: &str, env_override: &str) -> String {
    if let Ok(p) = std::env::var(env_override) {
        if !p.trim().is_empty() {
            return p;
        }
    }
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(std::path::Path::new(&home).join(".local/bin").join(name));
    }
    candidates.push(std::path::PathBuf::from(format!("/opt/homebrew/bin/{name}")));
    candidates.push(std::path::PathBuf::from(format!("/usr/local/bin/{name}")));
    for c in candidates {
        if c.is_file() {
            return c.to_string_lossy().into_owned();
        }
    }
    name.to_string()
}

pub fn check_dependencies() -> DependencyStatus {
    DependencyStatus {
        ffmpeg: binary_works("ffmpeg", "FFMPEG_PATH"),
        ffprobe: binary_works("ffprobe", "FFPROBE_PATH"),
    }
}

/// Return (free_bytes, total_bytes) for the filesystem containing `path`.
/// Uses `df -k` and is best-effort (returns zeros on failure).
pub fn storage_for(path: &str) -> (i64, i64) {
    #[cfg(unix)]
    {
        if let Ok(out) = Command::new("df").args(["-k", path]).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                if let Some(line) = text.lines().nth(1) {
                    let cols: Vec<&str> = line.split_whitespace().collect();
                    // df -k: Filesystem 1K-blocks Used Available ...
                    if cols.len() >= 4 {
                        let total = cols[1].parse::<i64>().unwrap_or(0) * 1024;
                        let avail = cols[3].parse::<i64>().unwrap_or(0) * 1024;
                        return (avail, total);
                    }
                }
            }
        }
    }
    (0, 0)
}
