//! Boostify Music Video Generator — our own "reference image + track →
//! music video" workflow.
//!
//! Inspired by the common AI-music-video idea, but implemented entirely from
//! Boostify-native primitives so there is **no third-party (AGPL) code**:
//!   * master-audio analysis (`audio.rs`) drives beat-aligned shot durations,
//!   * NVIDIA FLUX stills (`genai::generate_image`) render each shot on-brand,
//!   * local ffmpeg motion (`genai::animate_still_local`) turns stills into
//!     moving clips, and this module assembles them + the master audio into a
//!     single, music-synced video.
//!
//! Pure helpers only; persistence / progress events live in `lib.rs`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::*;

/// One planned shot of the generated music video.
#[derive(Clone, Debug)]
pub struct ShotPlan {
    pub index: usize,
    #[allow(dead_code)]
    pub section: String,
    #[allow(dead_code)]
    pub start: f64,
    pub duration: f64,
    pub idea: String,
    pub prompt: String,
    pub seed: i64,
    /// ffmpeg Ken-Burns motion variant (0..3).
    pub motion: u8,
    /// True = the shot features the performing artist; false = atmosphere.
    #[allow(dead_code)]
    pub performer: bool,
}

fn ffmpeg_bin() -> String {
    crate::system::resolve_bin("ffmpeg", "FFMPEG_PATH")
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Build a full-song storyboard: a continuous sequence of shots spanning the
/// whole track, with per-section cadence so cuts land on the music's beats.
pub fn plan_shots(style: &StyleReference, analysis: &MasterAnalysis) -> Vec<ShotPlan> {
    let total = if analysis.duration > 1.0 {
        analysis.duration
    } else {
        analysis.beats.last().copied().unwrap_or(60.0)
    };

    let mut sections: Vec<SongSection> = analysis.sections.clone();
    if sections.is_empty() {
        sections.push(SongSection {
            start: 0.0,
            end: total,
            label: "build".into(),
            energy: 0.5,
        });
    }

    let ideas = shot_ideas(style);
    let mut shots: Vec<ShotPlan> = Vec::new();
    let mut idx = 0usize;

    for sec in &sections {
        let sec_start = sec.start.max(0.0);
        let sec_end = sec.end.min(total).max(sec_start);
        let mut cursor = sec_start;

        while cursor < sec_end - 0.4 {
            let base = base_len(&sec.label, sec.energy);
            let target = cursor + base;
            // Snap the cut to the nearest beat so edits feel musical.
            let end = snap_to_beat(&analysis.beats, target, cursor + 1.2, sec_end);
            let dur = (end - cursor).clamp(1.2, 8.0);

            let idea = ideas[idx % ideas.len()].to_string();
            // Performer shots on energetic sections, atmosphere on calm ones.
            let performer = matches!(sec.label.as_str(), "drop" | "build")
                || (idx % 2 == 0 && sec.energy > 0.45);
            let seed = (4096 + (idx as i64) * 6151) % 2_147_483_647;
            let prompt = shot_prompt(style, &sec.label, &idea, performer);

            shots.push(ShotPlan {
                index: idx,
                section: sec.label.clone(),
                start: cursor,
                duration: dur,
                idea,
                prompt,
                seed,
                motion: (idx % 4) as u8,
                performer,
            });

            cursor += dur;
            idx += 1;
            if idx > 400 {
                return shots; // safety cap for very long tracks
            }
        }
    }

    if shots.is_empty() {
        let prompt = shot_prompt(style, "build", ideas[0], true);
        shots.push(ShotPlan {
            index: 0,
            section: "build".into(),
            start: 0.0,
            duration: total.min(6.0).max(2.0),
            idea: ideas[0].to_string(),
            prompt,
            seed: 4096,
            motion: 0,
            performer: true,
        });
    }

    shots
}

/// Base shot length (seconds) for a section; higher energy → slightly faster
/// cutting.
fn base_len(section: &str, energy: f64) -> f64 {
    let b = match section {
        "drop" => 1.8,
        "build" => 2.6,
        "low" => 4.5,
        "bridge" => 4.0,
        "intro" => 5.0,
        "outro" => 5.5,
        _ => 3.5,
    };
    (b * (1.0 - energy.clamp(0.0, 1.0) * 0.25)).clamp(1.4, 6.5)
}

/// Nearest beat to `target` within `[min_end, hard_max]`, else `target` clamped.
fn snap_to_beat(beats: &[f64], target: f64, min_end: f64, hard_max: f64) -> f64 {
    let fallback = target.clamp(min_end, hard_max.max(min_end));
    if beats.is_empty() {
        return fallback;
    }
    let mut best = fallback;
    let mut best_d = f64::MAX;
    for &b in beats {
        if b < min_end || b > hard_max {
            continue;
        }
        let d = (b - target).abs();
        if d < best_d {
            best_d = d;
            best = b;
        }
    }
    best
}

/// Curated, style-aware shot ideas mixing performer and atmospheric framings.
fn shot_ideas(style: &StyleReference) -> Vec<&'static str> {
    let mut ideas: Vec<&'static str> = vec![
        "the artist performing center-frame under dramatic stage light",
        "slow push-in on the artist's silhouette against haze",
        "wide establishing shot of a moody urban rooftop at dusk",
        "neon reflections rippling across wet asphalt at night",
        "the artist walking through drifting smoke, backlit",
        "abstract bokeh of city lights melting out of focus",
        "extreme close-up of the artist's eyes, intense expression",
        "dust particles floating through a single shaft of light",
        "the artist mid-motion, dynamic dance energy, motion blur",
        "clouds time-lapsing over a dramatic skyline",
    ];

    let desc = style.descriptor.to_lowercase();
    let prioritize = |ideas: &mut Vec<&'static str>, needle: &str| {
        if let Some(pos) = ideas.iter().position(|s| s.contains(needle)) {
            let item = ideas.remove(pos);
            ideas.insert(0, item);
        }
    };
    if desc.contains("night") || desc.contains("neon") || desc.contains("club") {
        prioritize(&mut ideas, "neon");
    }
    if desc.contains("nature") || desc.contains("coast") || desc.contains("beach") {
        prioritize(&mut ideas, "skyline");
    }
    ideas
}

fn shot_mood(section: &str) -> &'static str {
    match section {
        "intro" => "calm, atmospheric, establishing",
        "low" => "introspective, intimate, restrained",
        "build" => "rising tension, momentum, anticipation",
        "drop" => "high energy, bold, kinetic",
        "bridge" => "dreamy, transitional, reflective",
        "outro" => "fading, resolved, cinematic farewell",
        _ => "cinematic, balanced",
    }
}

fn shot_prompt(style: &StyleReference, section: &str, idea: &str, performer: bool) -> String {
    let palette = if style.palette.is_empty() {
        String::new()
    } else {
        format!(" Color palette: {}.", style.palette.join(", "))
    };
    let artist = style.artist.as_deref().filter(|a| !a.trim().is_empty());
    let subject = match (performer, artist) {
        (true, Some(a)) => format!("Music-video shot featuring {a} as the performing artist. "),
        (true, None) => "Music-video shot featuring the performing artist. ".to_string(),
        (false, _) => "Atmospheric cinematic music-video shot. ".to_string(),
    };
    format!(
        "{subject}{idea}. Visual style: {desc}{palette} Mood: {mood}. \
         Photorealistic, filmic, anamorphic, shallow depth of field, volumetric \
         light, rich film grain, 16:9 widescreen. No text, no words, no letters, \
         no logo, no watermark, no captions.",
        desc = style.descriptor,
        mood = shot_mood(section),
    )
}

/// Concatenate the rendered shot clips (in order) and mux the master audio into
/// a single MP4. Returns true on success.
pub fn assemble(clips: &[PathBuf], audio: Option<&Path>, out: &Path, fps: f64) -> bool {
    if clips.is_empty() {
        return false;
    }
    let parent = out
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&parent);

    let fps = if fps.is_finite() && fps > 1.0 { fps } else { 24.0 };

    // 1) Build the concat-demuxer list file (absolute paths, single-quoted).
    let list_path = parent.join(format!("concat_{}.txt", nanos()));
    let mut list = String::new();
    for clip in clips {
        let p = clip.to_string_lossy().replace('\'', "'\\''");
        list.push_str(&format!("file '{}'\n", p));
    }
    if std::fs::write(&list_path, &list).is_err() {
        return false;
    }

    // 2) Concatenate into a silent video (re-encode for a uniform stream).
    let silent = parent.join(format!("silent_{}.mp4", nanos()));
    let concat_ok = Command::new(ffmpeg_bin())
        .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
        .arg(&list_path)
        .args([
            "-c:v", "libx264", "-preset", "veryfast", "-crf", "19", "-pix_fmt", "yuv420p",
            "-r", &format!("{}", fps as i64), "-an",
        ])
        .arg(&silent)
        .status();
    let _ = std::fs::remove_file(&list_path);
    if !matches!(concat_ok, Ok(s) if s.success()) || !silent.exists() {
        return false;
    }

    // 3) Mux the master audio (trim to the shorter stream).
    let ok = match audio {
        Some(a) if a.exists() => {
            let st = Command::new(ffmpeg_bin())
                .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
                .arg(&silent)
                .args(["-i"])
                .arg(a)
                .args([
                    "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac",
                    "-b:a", "192k", "-shortest", "-movflags", "+faststart",
                ])
                .arg(out)
                .status();
            matches!(st, Ok(s) if s.success())
        }
        _ => {
            // No audio — keep the silent assembly as the final output.
            std::fs::rename(&silent, out).is_ok()
        }
    };

    let _ = std::fs::remove_file(&silent);
    ok && out.exists()
}
