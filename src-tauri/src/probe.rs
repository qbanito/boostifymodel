use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Result of probing a media file with ffprobe.
#[derive(Debug, Default, Clone)]
pub struct ProbeResult {
    pub duration: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
    /// True capture frame rate (r_frame_rate) — distinguishes slow-motion source.
    pub r_fps: Option<f64>,
    pub codec: Option<String>,
    pub has_audio: bool,
}

fn ffprobe_bin() -> String {
    crate::system::resolve_bin("ffprobe", "FFPROBE_PATH")
}

/// Probe a file's technical metadata. Returns None on any failure (missing
/// ffprobe, unreadable file, exotic RAW format ffprobe can't parse, etc.).
pub fn probe(path: &Path) -> Option<ProbeResult> {
    let output = Command::new(ffprobe_bin())
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: Value = serde_json::from_slice(&output.stdout).ok()?;
    let mut result = ProbeResult::default();

    if let Some(format) = json.get("format") {
        result.duration = format
            .get("duration")
            .and_then(|d| d.as_str())
            .and_then(|s| s.parse::<f64>().ok());
    }

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        if let Some(v) = streams
            .iter()
            .find(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("video"))
        {
            result.width = v.get("width").and_then(|w| w.as_i64());
            result.height = v.get("height").and_then(|h| h.as_i64());
            result.r_fps = v
                .get("r_frame_rate")
                .and_then(|r| r.as_str())
                .and_then(parse_fps);
            result.codec = v
                .get("codec_name")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            result.fps = v
                .get("avg_frame_rate")
                .and_then(|r| r.as_str())
                .and_then(parse_fps);
            if result.duration.is_none() {
                result.duration = v
                    .get("duration")
                    .and_then(|d| d.as_str())
                    .and_then(|s| s.parse::<f64>().ok());
            }
        }

        result.has_audio = streams
            .iter()
            .any(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("audio"));
    }

    Some(result)
}

/// Common timeline frame rates we conform footage to.
#[allow(dead_code)]
const COMMON_SEQ_FPS: [f64; 5] = [23.976, 24.0, 25.0, 29.97, 30.0];

/// Snap a measured frame rate to the nearest common rate (handles 23.976 vs 24).
#[allow(dead_code)]
pub fn normalize_fps(fps: f64) -> f64 {
    let mut best = fps;
    let mut best_d = f64::MAX;
    for c in COMMON_SEQ_FPS {
        let d = (c - fps).abs();
        if d < best_d {
            best_d = d;
            best = c;
        }
    }
    // Only snap when within ~1 fps, otherwise keep the real value (e.g. 60/120).
    if best_d <= 1.0 {
        best
    } else {
        fps
    }
}

/// Decide whether `source_fps` footage is slow-motion relative to a timeline at
/// `sequence_fps`, and the conform speed (%) to play it back at sequence rate.
///
/// Returns `(is_slow_mo, speed_pct)`. A 120fps clip on a 24fps timeline is slow
/// motion at 20% speed.
pub fn slow_mo_plan(source_fps: Option<f64>, sequence_fps: f64) -> (bool, Option<f64>) {
    let src = match source_fps {
        Some(f) if f > 0.0 => f,
        _ => return (false, None),
    };
    let seq = if sequence_fps > 0.0 { sequence_fps } else { 24.0 };
    // 1.4x faster capture than the timeline is our slow-mo threshold.
    let is_slow_mo = src > seq * 1.4;
    let speed_pct = (seq / src * 100.0 * 100.0).round() / 100.0;
    (is_slow_mo, Some(speed_pct))
}

/// Parse ffmpeg "num/den" frame-rate strings into f64.
fn parse_fps(raw: &str) -> Option<f64> {
    let mut split = raw.split('/');
    let num: f64 = split.next()?.parse().ok()?;
    let den: f64 = split.next().unwrap_or("1").parse().unwrap_or(1.0);
    if den == 0.0 {
        return None;
    }
    Some(num / den)
}

/// Detect scene-change timestamps using ffmpeg's `select` scene filter.
/// Returns a sorted list of cut points (seconds). Empty on failure.
pub fn detect_scenes(path: &Path, threshold: f64) -> Vec<f64> {
    let ffmpeg = std::env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".into());
    let filter = format!("select='gt(scene,{:.3})',showinfo", threshold);
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-i"])
        .arg(path)
        .args(["-filter:v", &filter, "-f", "null", "-"])
        .output();

    let Ok(output) = output else {
        return Vec::new();
    };

    // ffmpeg writes showinfo lines to stderr: "... pts_time:12.345 ...".
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut cuts = Vec::new();
    for line in stderr.lines() {
        if let Some(idx) = line.find("pts_time:") {
            let rest = &line[idx + "pts_time:".len()..];
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            if let Ok(t) = num.parse::<f64>() {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup();
    cuts
}
