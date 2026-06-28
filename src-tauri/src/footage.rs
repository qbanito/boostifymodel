//! Footage shot analysis — finds the usable *takes* inside each uploaded clip
//! and scores the best *moments* within them, so the auto-editor cuts to good,
//! coherent content instead of a blind sequential window. This is what makes
//! the automatic edit "make sense": a long take is split at its hard cuts, the
//! dark / blurry / static stretches are avoided, and the chosen window matches
//! the energy of the song section (punchy motion on drops, calm holds on
//! intros).
//!
//! Pure ffmpeg: a single-pass 8x8 grayscale sampling of the clip (no ML, no
//! extra crates). Reused at EDL-build time; never persisted.

use std::path::Path;
use std::process::{Command, Stdio};

fn ffmpeg_bin() -> String {
    crate::system::resolve_bin("ffmpeg", "FFMPEG_PATH")
}

/// Frames per second sampled for the profile (one cheap ffmpeg pass).
const SAMPLE_FPS: f64 = 3.0;
/// Cap on sampled frames so very long clips stay fast (~5.5 min at 3 fps).
const MAX_SAMPLES: usize = 1000;
/// aHash hamming distance (0..64) above which two adjacent frames are a cut.
const CUT_DIST: u32 = 24;
/// Shortest take we keep as its own shot (seconds); shorter merges into a sibling.
const MIN_SHOT: f64 = 1.0;

/// One sampled frame's cheap visual statistics.
#[derive(Clone, Copy)]
pub struct FrameSample {
    pub t: f64,
    /// Mean luma 0..255.
    pub brightness: f64,
    /// Luma variance — a proxy for detail / sharpness (low = flat/blurry/black).
    pub detail: f64,
    /// Change vs the previous sampled frame, 0..64 (high = motion or a cut).
    pub motion: f64,
    /// Perceptual average-hash bits of this frame.
    bits: u64,
}

/// The analysed profile of one source clip.
pub struct FootageProfile {
    pub duration: f64,
    pub samples: Vec<FrameSample>,
    /// Detected takes (shot boundaries) as `[start, end)` seconds.
    pub shots: Vec<(f64, f64)>,
}

/// Sample the clip to a series of 8x8 grayscale frames in ONE ffmpeg pass and
/// return the 64 luma bytes of each. Far cheaper than seeking per frame.
fn sample_series(src: &Path, fps: f64) -> Vec<[u8; 64]> {
    let mut child = match Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args([
            "-vf",
            &format!("fps={fps},scale=8:8,format=gray"),
            "-f",
            "rawvideo",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut buf = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        use std::io::Read;
        let _ = out.read_to_end(&mut buf);
    }
    let _ = child.wait();

    let mut frames = Vec::with_capacity(buf.len() / 64);
    for chunk in buf.chunks_exact(64) {
        if frames.len() >= MAX_SAMPLES {
            break;
        }
        let mut px = [0u8; 64];
        px.copy_from_slice(chunk);
        frames.push(px);
    }
    frames
}

/// aHash bits of an 8x8 grayscale frame (bit set when pixel ≥ frame mean).
fn ahash_bits(px: &[u8; 64]) -> u64 {
    let mean: f64 = px.iter().map(|&b| b as f64).sum::<f64>() / 64.0;
    let mut bits = 0u64;
    for (i, &p) in px.iter().enumerate() {
        if p as f64 >= mean {
            bits |= 1 << i;
        }
    }
    bits
}

/// Analyse a clip into per-moment samples + shot (take) boundaries. Returns
/// `None` when the clip can't be sampled (missing file / no video).
pub fn analyze(src: &Path, duration: f64) -> Option<FootageProfile> {
    let frames = sample_series(src, SAMPLE_FPS);
    if frames.len() < 2 {
        return None;
    }
    let dt = 1.0 / SAMPLE_FPS;

    let mut samples: Vec<FrameSample> = Vec::with_capacity(frames.len());
    let mut prev_bits: Option<u64> = None;
    for (i, px) in frames.iter().enumerate() {
        let mean: f64 = px.iter().map(|&b| b as f64).sum::<f64>() / 64.0;
        let var: f64 = px.iter().map(|&b| (b as f64 - mean).powi(2)).sum::<f64>() / 64.0;
        let bits = ahash_bits(px);
        let motion = match prev_bits {
            Some(p) => (p ^ bits).count_ones() as f64,
            None => 0.0,
        };
        prev_bits = Some(bits);
        samples.push(FrameSample {
            t: i as f64 * dt,
            brightness: mean,
            detail: var,
            motion,
            bits,
        });
    }

    let total = if duration > 0.5 {
        duration
    } else {
        samples.len() as f64 * dt
    };

    // Shot boundaries: a hard cut is a big aHash jump between adjacent frames.
    let mut bounds: Vec<f64> = vec![0.0];
    for w in samples.windows(2) {
        let d = (w[0].bits ^ w[1].bits).count_ones();
        if d >= CUT_DIST {
            bounds.push(w[1].t);
        }
    }
    bounds.push(total);
    bounds.dedup_by(|a, b| (*a - *b).abs() < 1e-3);

    // Build [start,end) shots, merging any sliver shorter than MIN_SHOT.
    let mut shots: Vec<(f64, f64)> = Vec::new();
    for pair in bounds.windows(2) {
        let (s, e) = (pair[0], pair[1]);
        if e - s < MIN_SHOT {
            if let Some(last) = shots.last_mut() {
                last.1 = e; // extend the previous take over the sliver
                continue;
            }
        }
        shots.push((s, e));
    }
    if shots.is_empty() {
        shots.push((0.0, total));
    }

    Some(FootageProfile {
        duration: total,
        samples,
        shots,
    })
}

impl FootageProfile {
    /// Index of the first sample at-or-after `t`.
    fn idx_at(&self, t: f64) -> usize {
        match self
            .samples
            .binary_search_by(|s| s.t.partial_cmp(&t).unwrap_or(std::cmp::Ordering::Less))
        {
            Ok(i) => i,
            Err(i) => i.min(self.samples.len().saturating_sub(1)),
        }
    }

    /// Content quality of the window `[a,b]` on a 0..1 scale: well-exposed
    /// (not crushed-dark, not blown-out) and detailed (not flat / blurry).
    fn quality(&self, a: f64, b: f64) -> f64 {
        let (i0, i1) = (self.idx_at(a), self.idx_at(b).max(self.idx_at(a) + 1));
        let win = &self.samples[i0.min(self.samples.len().saturating_sub(1))
            ..i1.min(self.samples.len())];
        if win.is_empty() {
            return 0.0;
        }
        let mut q = 0.0;
        for s in win {
            // Exposure: peak at ~120 luma, falls off toward 0 / 255.
            let expo = (1.0 - (s.brightness - 120.0).abs() / 120.0).clamp(0.0, 1.0);
            // Detail: variance ~600+ reads as crisp; near-0 is flat/black.
            let detail = (s.detail / 600.0).clamp(0.0, 1.0);
            q += 0.6 * expo + 0.4 * detail;
        }
        q / win.len() as f64
    }

    /// Mean motion (0..1) across the window `[a,b]`.
    fn motion(&self, a: f64, b: f64) -> f64 {
        let (i0, i1) = (self.idx_at(a), self.idx_at(b).max(self.idx_at(a) + 1));
        let win = &self.samples[i0.min(self.samples.len().saturating_sub(1))
            ..i1.min(self.samples.len())];
        if win.is_empty() {
            return 0.0;
        }
        let m: f64 = win.iter().map(|s| s.motion).sum::<f64>() / win.len() as f64;
        (m / 64.0).clamp(0.0, 1.0)
    }

    /// Pick the best source-in (seconds) for a `need`-second window. Prefers
    /// well-exposed, detailed moments inside a SINGLE take, matches the section
    /// energy (`want_high_motion` → punchy moments, else calm holds), and avoids
    /// regions already consumed (`used_spans` = list of `[in,out]`). Returns
    /// `None` when no take is long enough (caller keeps its own fallback).
    pub fn best_window(
        &self,
        need: f64,
        want_high_motion: bool,
        used_spans: &[(f64, f64)],
        head: f64,
    ) -> Option<f64> {
        if need <= 0.0 || self.duration <= 0.0 {
            return None;
        }
        let stride = (need * 0.4).clamp(0.2, 1.0);
        let mut best: Option<(f64, f64)> = None; // (src_in, score)

        for &(s0, e0) in &self.shots {
            // Stay a small margin inside the take so we never cut on the splice.
            let lo = (s0 + head).max(0.0);
            let hi = e0 - head;
            if hi - lo < need {
                continue; // take too short for this slot
            }
            let mut start = lo;
            while start + need <= hi + 1e-6 {
                let end = start + need;
                let q = self.quality(start, end);
                let mot = self.motion(start, end);
                let mot_match = if want_high_motion { mot } else { 1.0 - mot };

                // Penalise overlap with already-used windows of this clip.
                let mut overlap = 0.0;
                for &(us, ue) in used_spans {
                    let o = (end.min(ue) - start.max(us)).max(0.0);
                    if o > 0.0 {
                        overlap += o / need;
                    }
                }

                let score = q * 1.6 + mot_match * 0.8 - overlap * 2.0;
                if best.map_or(true, |(_, bs)| score > bs) {
                    best = Some((start, score));
                }
                start += stride;
            }
        }
        best.map(|(src_in, _)| src_in)
    }

    /// Per-take quality verdict: flags shaky ("plano movido"), soft/blurry and
    /// poorly-exposed takes, scores the overall technical quality 0..1 and picks
    /// the timestamp of the single best representative frame (sharp, stable,
    /// well-exposed) to use as a screenshot reference for coherent B-roll.
    pub fn take_report(&self) -> TakeReport {
        let n = self.samples.len();
        if n == 0 {
            return TakeReport {
                score: 0.0,
                best_time: 0.0,
                shaky: false,
                soft: false,
                dark: false,
                verdict: "unknown".into(),
                issues: Vec::new(),
            };
        }

        let mut sum_bright = 0.0;
        let mut sum_detail = 0.0;
        let mut motion_vals: Vec<f64> = Vec::new();
        let mut best: (f64, f64) = (f64::MIN, self.samples[0].t); // (frame quality, time)

        for (i, s) in self.samples.iter().enumerate() {
            sum_bright += s.brightness;
            sum_detail += s.detail;
            // Ignore the first frame (no motion baseline) and hard cuts so a
            // multi-shot clip isn't mistaken for a shaky one.
            if i > 0 && s.motion < CUT_DIST as f64 {
                motion_vals.push(s.motion);
            }
            let expo = (1.0 - (s.brightness - 120.0).abs() / 120.0).clamp(0.0, 1.0);
            let det = (s.detail / 600.0).clamp(0.0, 1.0);
            let stab = (1.0 - s.motion / 64.0).clamp(0.0, 1.0);
            let q = expo * 0.5 + det * 0.3 + stab * 0.2;
            if q > best.0 {
                best = (q, s.t);
            }
        }

        let mean_bright = sum_bright / n as f64;
        let mean_detail = sum_detail / n as f64;
        let (mean_motion, jitter) = if motion_vals.is_empty() {
            (0.0, 0.0)
        } else {
            let mean = motion_vals.iter().sum::<f64>() / motion_vals.len() as f64;
            // Fraction of frames with sustained mid-high motion = handheld shake.
            let j = motion_vals.iter().filter(|&&m| m >= 10.0).count() as f64
                / motion_vals.len() as f64;
            (mean, j)
        };

        let shaky = mean_motion > 11.0 && jitter > 0.45;
        let soft = mean_detail < 300.0 || (mean_motion > 14.0 && mean_detail < 460.0);
        let dark = mean_bright < 35.0 || mean_bright > 225.0;

        let mut score = self
            .samples
            .iter()
            .map(|s| {
                let expo = (1.0 - (s.brightness - 120.0).abs() / 120.0).clamp(0.0, 1.0);
                let det = (s.detail / 600.0).clamp(0.0, 1.0);
                let stab = (1.0 - s.motion / 64.0).clamp(0.0, 1.0);
                expo * 0.5 + det * 0.3 + stab * 0.2
            })
            .sum::<f64>()
            / n as f64;
        if shaky {
            score *= 0.6;
        }
        if soft {
            score *= 0.7;
        }
        if dark {
            score *= 0.8;
        }

        let mut issues = Vec::new();
        if shaky {
            issues.push("plano movido / cámara inestable".to_string());
        }
        if soft {
            issues.push("enfoque blando o movido (poco detalle)".to_string());
        }
        if dark {
            if mean_bright < 35.0 {
                issues.push("subexpuesto (demasiado oscuro)".to_string());
            } else {
                issues.push("sobreexpuesto (zonas quemadas)".to_string());
            }
        }

        let verdict = if shaky {
            "shaky"
        } else if soft {
            "soft"
        } else if dark {
            "dark"
        } else {
            "good"
        }
        .to_string();

        TakeReport {
            score: (score.clamp(0.0, 1.0) * 100.0).round() / 100.0,
            best_time: best.1,
            shaky,
            soft,
            dark,
            verdict,
            issues,
        }
    }
}

/// Technical verdict for one take, derived from its sampled frames.
pub struct TakeReport {
    /// Overall technical quality 0..1 (exposure + sharpness + stability).
    pub score: f64,
    /// Timestamp (seconds) of the best representative frame to screenshot.
    pub best_time: f64,
    /// Sustained erratic motion — a handheld "plano movido".
    pub shaky: bool,
    /// Low detail across the take — out of focus / motion-blurred.
    pub soft: bool,
    /// Crushed-dark or blown-out exposure.
    #[allow(dead_code)]
    pub dark: bool,
    /// 'good' | 'shaky' | 'soft' | 'dark' | 'unknown'.
    pub verdict: String,
    /// Human-readable problems (Spanish) for the UI.
    pub issues: Vec<String>,
}
