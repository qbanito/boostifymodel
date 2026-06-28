//! Master-audio analysis: decode the song to mono PCM with ffmpeg, then derive
//! a tempo (BPM), a beat grid, and energy-based structural sections. Pure DSP in
//! Rust — no external audio crates — so it works wherever ffmpeg is available.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::models::{MasterAnalysis, SongSection};

fn ffmpeg_bin() -> String {
    crate::system::resolve_bin("ffmpeg", "FFMPEG_PATH")
}

/// Sample rate we decode the master to. Low enough to be fast, high enough for
/// reliable onset/tempo estimation.
const SR: usize = 22_050;
/// Analysis hop in samples (~23 ms at 22.05 kHz).
const HOP: usize = 512;

/// Decode the audio track to mono signed-16 PCM and return the samples as f32
/// in -1.0..1.0. Returns `None` if ffmpeg is missing or the file has no audio.
fn decode_mono(path: &Path) -> Option<Vec<f32>> {
    let child = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            &SR.to_string(),
            "-f",
            "s16le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let out = child.wait_with_output().ok()?;
    if !out.status.success() || out.stdout.len() < 4 {
        return None;
    }

    let mut samples = Vec::with_capacity(out.stdout.len() / 2);
    for chunk in out.stdout.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]);
        samples.push(s as f32 / 32768.0);
    }
    Some(samples)
}

/// Short-time energy (RMS-like) envelope, one value per HOP.
fn energy_envelope(samples: &[f32]) -> Vec<f32> {
    let win = HOP * 2;
    let frames = samples.len() / HOP;
    let mut env = Vec::with_capacity(frames);
    for f in 0..frames {
        let start = f * HOP;
        let end = (start + win).min(samples.len());
        if start >= end {
            break;
        }
        let mut sum = 0.0f32;
        for &s in &samples[start..end] {
            sum += s * s;
        }
        env.push((sum / (end - start) as f32).sqrt());
    }
    env
}

/// Half-wave rectified difference of the energy envelope — a cheap onset
/// novelty function. Peaks mark note/percussion onsets.
fn novelty(env: &[f32]) -> Vec<f32> {
    let mut nov = vec![0.0f32; env.len()];
    for i in 1..env.len() {
        let d = env[i] - env[i - 1];
        nov[i] = if d > 0.0 { d } else { 0.0 };
    }
    nov
}

/// z-score normalize (mean 0, unit std) so cross-correlation is scale-invariant.
fn normalize(v: &[f32]) -> Vec<f32> {
    let n = v.len();
    if n == 0 {
        return Vec::new();
    }
    let mean = v.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let var = v
        .iter()
        .map(|&x| {
            let d = x as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / n as f64;
    let std = var.sqrt().max(1e-9);
    v.iter().map(|&x| ((x as f64 - mean) / std) as f32).collect()
}

/// Dot product of the clip window against the master at a given lag (both
/// already normalized) — proportional to the cross-correlation.
fn corr_at(m: &[f32], c: &[f32], lag: usize) -> f64 {
    let n = c.len();
    let mut sum = 0.0f64;
    for i in 0..n {
        sum += m[lag + i] as f64 * c[i] as f64;
    }
    sum / n as f64
}

/// Windowed Pearson correlation (0..1-ish) used as an alignment confidence.
fn pearson_at(m: &[f32], c: &[f32], lag: usize) -> f64 {
    let n = c.len();
    if n == 0 || lag + n > m.len() {
        return 0.0;
    }
    let mw = &m[lag..lag + n];
    let mean_m = mw.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let mean_c = c.iter().map(|&x| x as f64).sum::<f64>() / n as f64;
    let (mut num, mut dm, mut dc) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let a = mw[i] as f64 - mean_m;
        let b = c[i] as f64 - mean_c;
        num += a * b;
        dm += a * a;
        dc += b * b;
    }
    if dm <= 0.0 || dc <= 0.0 {
        return 0.0;
    }
    num / (dm.sqrt() * dc.sqrt())
}

/// Cross-correlate a clip's onset pattern against the master's to find the
/// time offset (seconds into the master song) where the clip's audio begins.
/// This is the alignment that lets us lip-sync performance footage to the
/// master: master time `T` maps to clip source time `T - offset`.
/// Returns `(offset_seconds, confidence 0..1)` or `None` when either file has
/// no usable audio track.
pub fn align_to_master(master_path: &Path, clip_path: &Path) -> Option<(f64, f64)> {
    let frame_period = HOP as f64 / SR as f64;

    let master = decode_mono(master_path)?;
    let clip = decode_mono(clip_path)?;
    if master.len() < SR || clip.len() < SR / 2 {
        return None; // need ~1s of master and ~0.5s of clip to align
    }

    let m_env = normalize(&novelty(&energy_envelope(&master)));
    let mut c_env = normalize(&novelty(&energy_envelope(&clip)));

    // Matching on the clip's first ~90s is enough to lock the alignment.
    let max_clip_frames = ((90.0 / frame_period) as usize).max(64);
    if c_env.len() > max_clip_frames {
        c_env.truncate(max_clip_frames);
    }
    let n = c_env.len();
    if n < 32 || m_env.len() < n {
        return None;
    }

    // Slide the clip across the master; coarse pass then refine for speed.
    let max_lag = m_env.len() - n;
    let coarse = (max_lag / 4000).max(1);
    let (mut best_lag, mut best_corr) = (0usize, f64::MIN);
    let mut lag = 0;
    while lag <= max_lag {
        let corr = corr_at(&m_env, &c_env, lag);
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
        lag += coarse;
    }
    let lo = best_lag.saturating_sub(coarse);
    let hi = (best_lag + coarse).min(max_lag);
    for lag in lo..=hi {
        let corr = corr_at(&m_env, &c_env, lag);
        if corr > best_corr {
            best_corr = corr;
            best_lag = lag;
        }
    }

    let offset = best_lag as f64 * frame_period;
    let confidence = pearson_at(&m_env, &c_env, best_lag).clamp(0.0, 1.0);
    Some((offset, confidence))
}

/// Estimate tempo (BPM) by autocorrelating the novelty function over the lag
/// range that maps to 60–190 BPM. Returns BPM clamped to a sensible range.
fn estimate_bpm(nov: &[f32]) -> f64 {
    let frame_period = HOP as f64 / SR as f64; // seconds per novelty sample
    let min_bpm = 70.0;
    let max_bpm = 180.0;
    // lag (in frames) = 60 / (bpm * frame_period)
    let min_lag = (60.0 / (max_bpm * frame_period)).round() as usize;
    let max_lag = (60.0 / (min_bpm * frame_period)).round() as usize;
    if nov.len() <= max_lag + 1 || max_lag <= min_lag {
        return 120.0;
    }

    let mut best_lag = min_lag;
    let mut best_score = f64::MIN;
    for lag in min_lag..=max_lag {
        let mut score = 0.0f64;
        let mut i = lag;
        while i < nov.len() {
            score += (nov[i] * nov[i - lag]) as f64;
            i += 1;
        }
        // Normalize a little by sqrt(lag) so very long lags aren't penalized.
        let score = score / (lag as f64).sqrt();
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    let bpm = 60.0 / (best_lag as f64 * frame_period);
    // Fold into a musical range so 60/180-ish halves/doubles read naturally.
    let mut b = bpm;
    while b < 80.0 {
        b *= 2.0;
    }
    while b > 175.0 {
        b /= 2.0;
    }
    (b * 100.0).round() / 100.0
}

/// Pick the first strong onset time (seconds) to anchor the beat grid.
fn first_onset(nov: &[f32]) -> f64 {
    let frame_period = HOP as f64 / SR as f64;
    if nov.is_empty() {
        return 0.0;
    }
    let peak = nov.iter().cloned().fold(0.0f32, f32::max);
    let thresh = peak * 0.3;
    for (i, &v) in nov.iter().enumerate() {
        if v >= thresh {
            return i as f64 * frame_period;
        }
    }
    0.0
}

/// Build a beat grid from a tempo + anchor across the whole duration.
fn beat_grid(bpm: f64, first: f64, duration: f64) -> Vec<f64> {
    let period = 60.0 / bpm.max(1.0);
    let mut beats = Vec::new();
    // Back-fill earlier beats so the grid starts near 0.
    let mut t = first;
    while t - period >= 0.0 {
        t -= period;
    }
    while t <= duration {
        if t >= 0.0 {
            beats.push((t * 1000.0).round() / 1000.0);
        }
        t += period;
    }
    beats
}

/// Segment the song into structural sections by smoothed loudness level.
fn detect_sections(env: &[f32], duration: f64) -> Vec<SongSection> {
    if env.is_empty() || duration <= 0.0 {
        return Vec::new();
    }
    let frame_period = HOP as f64 / SR as f64;

    // Smooth the envelope with a ~1s moving average.
    let smooth_win = ((1.0 / frame_period).round() as usize).max(1);
    let mut smooth = vec![0.0f32; env.len()];
    let mut acc = 0.0f32;
    for i in 0..env.len() {
        acc += env[i];
        if i >= smooth_win {
            acc -= env[i - smooth_win];
        }
        let n = (i + 1).min(smooth_win) as f32;
        smooth[i] = acc / n;
    }

    let max_e = smooth.iter().cloned().fold(0.0f32, f32::max).max(1e-6);

    // Bucket each frame into a coarse energy level 0..3.
    let level = |e: f32| -> u8 {
        let r = e / max_e;
        if r < 0.25 {
            0
        } else if r < 0.5 {
            1
        } else if r < 0.78 {
            2
        } else {
            3
        }
    };

    let min_section = 4.0; // seconds
    let mut sections: Vec<SongSection> = Vec::new();
    let mut seg_start = 0usize;
    let mut cur = level(smooth[0]);

    let push = |sections: &mut Vec<SongSection>, s: usize, e: usize, _lvl: u8| {
        let start = s as f64 * frame_period;
        let end = (e as f64 * frame_period).min(duration);
        if end - start < 0.5 {
            return;
        }
        // Average normalized energy over the segment.
        let mut sum = 0.0f32;
        for v in &smooth[s..e.min(smooth.len())] {
            sum += *v;
        }
        let n = (e.min(smooth.len()) - s).max(1) as f32;
        let energy = ((sum / n) / max_e).clamp(0.0, 1.0);
        sections.push(SongSection {
            start: (start * 100.0).round() / 100.0,
            end: (end * 100.0).round() / 100.0,
            label: String::new(),
            energy: (energy as f64 * 100.0).round() / 100.0,
        });
    };

    for i in 1..smooth.len() {
        let lvl = level(smooth[i]);
        if lvl != cur {
            let dur = (i - seg_start) as f64 * frame_period;
            if dur >= min_section {
                push(&mut sections, seg_start, i, cur);
                seg_start = i;
                cur = lvl;
            }
        }
    }
    push(&mut sections, seg_start, smooth.len(), cur);

    label_sections(&mut sections, duration);
    sections
}

/// Assign human labels to sections from their energy and position.
fn label_sections(sections: &mut [SongSection], duration: f64) {
    let n = sections.len();
    for (i, s) in sections.iter_mut().enumerate() {
        let pos = if duration > 0.0 {
            (s.start + s.end) / 2.0 / duration
        } else {
            0.0
        };
        s.label = if i == 0 && s.energy < 0.5 {
            "intro".to_string()
        } else if i == n - 1 && s.energy < 0.5 {
            "outro".to_string()
        } else if s.energy >= 0.78 {
            "drop".to_string()
        } else if s.energy >= 0.5 {
            "build".to_string()
        } else if pos > 0.4 && pos < 0.75 {
            "bridge".to_string()
        } else {
            "low".to_string()
        };
    }
}

/// Full master analysis: tempo, beat grid, and structural sections.
pub fn analyze_master(path: &Path, duration_hint: Option<f64>) -> Option<MasterAnalysis> {
    let samples = decode_mono(path)?;
    if samples.is_empty() {
        return None;
    }
    let duration = duration_hint
        .filter(|d| *d > 0.0)
        .unwrap_or_else(|| samples.len() as f64 / SR as f64);

    let env = energy_envelope(&samples);
    let nov = novelty(&env);
    let bpm = estimate_bpm(&nov);
    let first = first_onset(&nov);
    let beats = beat_grid(bpm, first, duration);
    let sections = detect_sections(&env, duration);

    Some(MasterAnalysis {
        duration: (duration * 100.0).round() / 100.0,
        bpm,
        first_beat: (first * 1000.0).round() / 1000.0,
        beat_count: beats.len(),
        beats,
        sections,
    })
}
