use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn ffmpeg_bin() -> String {
    crate::system::resolve_bin("ffmpeg", "FFMPEG_PATH")
}

/// A planned clip segment.
#[derive(Debug, Clone, Copy)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
}

impl Segment {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }
}

/// Turn scene-cut timestamps into clip segments, dropping anything shorter
/// than `min_len`. If there are no cuts, the whole video becomes fixed-length
/// windows of `window` seconds.
pub fn plan_segments(cuts: &[f64], total: f64, min_len: f64, window: f64) -> Vec<Segment> {
    let mut segments = Vec::new();
    if cuts.is_empty() {
        let mut start = 0.0;
        while start < total {
            let end = (start + window).min(total);
            if end - start >= min_len {
                segments.push(Segment { start, end });
            }
            start = end;
        }
        return segments;
    }

    let mut prev = 0.0;
    for &cut in cuts {
        if cut - prev >= min_len {
            segments.push(Segment {
                start: prev,
                end: cut,
            });
        }
        prev = cut;
    }
    if total - prev >= min_len {
        segments.push(Segment {
            start: prev,
            end: total,
        });
    }
    segments
}

/// Cut a clip out of the source video (re-encoded for clean cuts + wide
/// compatibility). Returns true on success.
pub fn extract_clip(src: &Path, seg: Segment, out: &Path) -> bool {
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-y", "-ss"])
        .arg(format!("{:.3}", seg.start))
        .arg("-i")
        .arg(src)
        .args(["-t"])
        .arg(format!("{:.3}", seg.duration()))
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "20", "-pix_fmt", "yuv420p", "-an",
        ])
        .arg(out)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Extract a single JPEG thumbnail at `time` seconds.
pub fn extract_thumbnail(src: &Path, time: f64, out: &Path) -> bool {
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-y", "-ss"])
        .arg(format!("{:.3}", time))
        .arg("-i")
        .arg(src)
        .args(["-frames:v", "1", "-update", "1", "-vf", "scale=480:-1", "-q:v", "3"])
        .arg(out)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Generate a lightweight 720p H.264 proxy for smooth preview/scrub.
/// The original 4K source is never modified. Returns true on success.
pub fn make_proxy(src: &Path, out: &Path) -> bool {
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(src)
        .args([
            "-vf", "scale=-2:720",
            "-c:v", "libx264",
            "-preset", "veryfast",
            "-crf", "26",
            "-pix_fmt", "yuv420p",
            "-c:a", "aac",
            "-b:a", "128k",
            "-movflags", "+faststart",
        ])
        .arg(out)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Sample an 8x8 grayscale frame and return the 64 luma bytes.
/// This is the backbone of both the average-hash and the quality probe.
fn sample_8x8(src: &Path, time: f64) -> Option<Vec<u8>> {
    let mut child = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-ss"])
        .arg(format!("{:.3}", time))
        .arg("-i")
        .arg(src)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "scale=8:8,format=gray",
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut buf = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_end(&mut buf);
    }
    let _ = child.wait();
    if buf.len() >= 64 {
        Some(buf[..64].to_vec())
    } else {
        None
    }
}

/// Perceptual average-hash (aHash) as a 16-char hex string.
pub fn average_hash(src: &Path, time: f64) -> Option<String> {
    let px = sample_8x8(src, time)?;
    let mean: f64 = px.iter().map(|&b| b as f64).sum::<f64>() / 64.0;
    let mut bits: u64 = 0;
    for (i, &p) in px.iter().enumerate() {
        if p as f64 >= mean {
            bits |= 1 << i;
        }
    }
    Some(format!("{:016x}", bits))
}

/// Hamming distance between two aHash hex strings (0..=64). Returns 64 if
/// either string is unparseable.
pub fn hamming(a: &str, b: &str) -> u32 {
    match (u64::from_str_radix(a, 16), u64::from_str_radix(b, 16)) {
        (Ok(x), Ok(y)) => (x ^ y).count_ones(),
        _ => 64,
    }
}

/// A cheap visual-quality probe derived from the 8x8 sample.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameStats {
    pub brightness: f64, // mean luma 0..255
    pub variance: f64,   // luma variance — proxy for detail / sharpness
}

pub fn frame_stats(src: &Path, time: f64) -> Option<FrameStats> {
    let px = sample_8x8(src, time)?;
    let mean: f64 = px.iter().map(|&b| b as f64).sum::<f64>() / 64.0;
    let var: f64 =
        px.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / 64.0;
    Some(FrameStats {
        brightness: mean,
        variance: var,
    })
}

/// Build the clip output path inside the working/clips directory.
pub fn clip_path(work_dir: &Path, video_id: i64, index: usize) -> PathBuf {
    work_dir
        .join("clips")
        .join(format!("v{}_c{:04}.mp4", video_id, index))
}

pub fn thumb_path(work_dir: &Path, video_id: i64, index: usize) -> PathBuf {
    work_dir
        .join("thumbs")
        .join(format!("v{}_c{:04}.jpg", video_id, index))
}
