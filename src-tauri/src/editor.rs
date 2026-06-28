// Edit engine — turns a master's beat/section analysis plus the classified
// footage into an edit-decision list (EDL). It aligns cuts to the music,
// shapes a narrative arc (establishing intro → energetic drops → calm outro),
// drops b-roll where it belongs, varies camera angles (multicam) and conforms
// slow-motion source to the timeline. An `EditProfile` (nudged by user
// feedback) steers all of these so the auto-editor improves over time.

use std::collections::HashMap;
use std::collections::VecDeque;

use crate::footage::FootageProfile;
use crate::models::{EditProfile, EditSegment, EditSession, MasterAnalysis, SessionMedia};

/// Shortest cut we will ever place on the timeline (seconds).
const MIN_SEG: f64 = 0.6;
/// Small head margin so we never cut on the very first frame of a take.
const HEAD_MARGIN: f64 = 0.2;
/// Gap left between reused windows of the same clip, for variety.
const REUSE_GAP: f64 = 0.15;

/// Base number of beats to hold a shot for, by section energy.
fn base_cadence(label: &str) -> f64 {
    match label {
        "drop" => 2.0,
        "build" => 4.0,
        "bridge" => 6.0,
        "intro" | "outro" | "low" => 8.0,
        _ => 4.0,
    }
}

/// How strongly a section wants performance footage (0..1).
fn section_perf_weight(label: &str) -> f64 {
    match label {
        "drop" => 0.9,
        "build" => 0.72,
        "low" => 0.4,
        "bridge" => 0.3,
        "intro" => 0.25,
        "outro" => 0.2,
        _ => 0.5,
    }
}

/// Calm sections where a conformed slow-motion shot reads as cinematic.
fn prefers_slow_mo(label: &str) -> bool {
    matches!(label, "intro" | "low" | "bridge" | "outro")
}

/// Label of the section covering time `t` (falls back to the nearest one).
fn section_at(analysis: &MasterAnalysis, t: f64) -> String {
    for s in &analysis.sections {
        if t >= s.start && t < s.end {
            return s.label.clone();
        }
    }
    analysis
        .sections
        .last()
        .map(|s| s.label.clone())
        .unwrap_or_else(|| "low".into())
}

fn usable_duration(m: &SessionMedia) -> f64 {
    m.duration_seconds.unwrap_or(0.0)
}

fn shot_type_of(m: &SessionMedia) -> Option<String> {
    m.analysis
        .as_ref()
        .and_then(|a| a.shot_type.clone())
        .map(|s| s.to_lowercase())
}

/// AI-generated B-roll inserted from the B-roll Studio (tagged on insert).
fn is_ai_broll(m: &SessionMedia) -> bool {
    m.analysis
        .as_ref()
        .map(|a| a.labels.iter().any(|l| l.eq_ignore_ascii_case("ai b-roll")))
        .unwrap_or(false)
}

/// Build the music-aligned, narrative EDL for a session. Returns segments with
/// `id = 0` (the database assigns the real ids on insert).
pub fn build_edl(
    session: &EditSession,
    analysis: &MasterAnalysis,
    media: &[SessionMedia],
    profile: &EditProfile,
    footage: &HashMap<i64, FootageProfile>,
) -> Vec<EditSegment> {
    let videos: Vec<&SessionMedia> = media.iter().filter(|m| m.kind == "video").collect();
    if videos.is_empty() {
        return Vec::new();
    }

    // 1. Derive the timeline cut points by accumulating beats until the
    //    section's (profile-scaled) cadence is reached. A repeating phrase
    //    pattern varies the hold length so cuts never feel mechanical (the
    //    same section no longer produces a row of identical-length shots).
    let cadence_pattern = [1.0f64, 0.5, 1.0, 1.5, 0.75, 1.25, 0.5, 2.0];
    let mut cuts: Vec<f64> = vec![0.0];
    if analysis.beats.len() > 2 {
        let mut acc = 0usize;
        let mut seg_start = 0.0f64;
        let mut seg_idx = 0usize;
        for &b in &analysis.beats {
            if b <= seg_start + 1e-3 {
                continue;
            }
            acc += 1;
            let mid = (seg_start + b) / 2.0;
            let mult = cadence_pattern[seg_idx % cadence_pattern.len()];
            let cad = (base_cadence(&section_at(analysis, mid)) * profile.cadence * mult)
                .round()
                .max(1.0) as usize;
            if acc >= cad && b - seg_start >= MIN_SEG {
                cuts.push(b);
                seg_start = b;
                acc = 0;
                seg_idx += 1;
            }
        }
    }
    let end = analysis.duration.max(cuts.last().copied().unwrap_or(0.0));
    if end - cuts.last().copied().unwrap_or(0.0) > 0.1 {
        cuts.push(end);
    }

    // Split footage: real shots form the BACKBONE of the edit; AI b-roll only
    // punctuates as periodic cutaways. (When there is no real footage at all,
    // b-roll is all we have and becomes the backbone instead.)
    let primary: Vec<&SessionMedia> =
        videos.iter().copied().filter(|m| !is_ai_broll(m)).collect();
    let broll: Vec<&SessionMedia> =
        videos.iter().copied().filter(|m| is_ai_broll(m)).collect();
    let has_primary = !primary.is_empty();
    let has_broll = !broll.is_empty();
    let has_perf = primary.iter().any(|m| m.role == "performance");
    let has_story = primary.iter().any(|m| m.role == "story");

    // Camera-variation window scales with the profile.
    let recent_window = (1.0 + profile.variation * 3.0).round().max(1.0) as usize;
    // Insert a b-roll cutaway every N primary cuts (higher freq = more often).
    let broll_interval = if !has_broll || profile.broll_freq <= 0.01 {
        usize::MAX
    } else {
        (3.0 + (1.0 - profile.broll_freq) * 6.0).round().max(2.0) as usize
    };

    let mut recent: VecDeque<i64> = VecDeque::new();
    let mut last_layer: Option<i64> = None;
    let mut since_broll = 0usize;
    let mut cursors: HashMap<i64, f64> = HashMap::new();
    // Source windows already consumed per clip, so the take's best moment is
    // not reused twice when a single clip is cut into more than once.
    let mut used_spans: HashMap<i64, Vec<(f64, f64)>> = HashMap::new();
    let mut used: HashMap<i64, usize> = HashMap::new();
    let mut segments: Vec<EditSegment> = Vec::new();

    let slots = cuts.len().saturating_sub(1);
    for i in 0..slots {
        let seg_start = cuts[i];
        let seg_end = cuts[i + 1];
        let timeline_dur = seg_end - seg_start;
        if timeline_dur < 1e-3 {
            continue;
        }
        let pos = if analysis.duration > 0.0 {
            (seg_start + seg_end) / 2.0 / analysis.duration
        } else {
            0.0
        };
        let label = section_at(analysis, (seg_start + seg_end) / 2.0);
        let want_slow = prefers_slow_mo(&label);

        // 2. Choose the pool for this slot. Real footage carries the edit; AI
        //    b-roll comes in as periodic cutaways (and on the calm intro/outro
        //    establishing beats). Falls back to whatever pool is non-empty.
        let want_broll = has_broll
            && (!has_primary
                || since_broll >= broll_interval
                || (matches!(label.as_str(), "intro" | "outro") && since_broll >= 1));
        let pool: &[&SessionMedia] = if want_broll {
            &broll
        } else if has_primary {
            &primary
        } else {
            &broll
        };

        // For primary slots, decide performance vs story by section energy and
        // the narrative arc (open/close on establishing shots).
        let sect_w = section_perf_weight(&label);
        let perf_w = (sect_w * (0.5 + profile.performance_bias)).clamp(0.0, 1.0);
        let mut desired_perf = perf_w >= 0.5;
        if has_story && (i == 0 || pos < 0.06 || pos > 0.94) {
            desired_perf = false;
        }
        let desired_role = if desired_perf && has_perf {
            "performance"
        } else {
            "story"
        };

        // 3. Score every candidate in the chosen pool and pick the best.
        let mut best: Option<(&SessionMedia, f64)> = None;
        for (k, m) in pool.iter().enumerate() {
            let mut score = 0.0f64;

            // Role match (only meaningful inside the real-footage pool).
            if !want_broll {
                if m.role == desired_role {
                    score += 3.0;
                } else {
                    score += 0.4;
                }
            }

            // Usage balancing: strongly favour the shots used least so EVERY
            // take makes it into the cut instead of a few clips on endless
            // repeat (this is what gets the long real takes integrated).
            let times_used = *used.get(&m.id).unwrap_or(&0);
            score -= times_used as f64 * 1.3;

            // Shot-type ↔ section framing.
            if let Some(st) = shot_type_of(m) {
                let wide = st.contains("wide")
                    || st.contains("establish")
                    || st.contains("long")
                    || st.contains("full");
                let close = st.contains("close")
                    || st.contains("cu")
                    || st.contains("macro")
                    || st.contains("detail");
                if matches!(label.as_str(), "intro" | "outro" | "bridge") && wide {
                    score += 1.2;
                }
                if matches!(label.as_str(), "drop" | "build") && close {
                    score += 1.0;
                }
            }

            // Slow-motion affinity: cinematic in calm, distracting in drops.
            if m.is_slow_mo {
                if want_slow {
                    score += profile.slowmo_affinity * 2.0;
                } else {
                    score -= 0.6;
                }
            }

            // Camera variation: avoid clips used recently and repeated angles.
            if recent.contains(&m.id) {
                score -= 3.0 * (0.4 + profile.variation);
            }
            if let (Some(lg), Some(ll)) = (m.layer_group, last_layer) {
                if lg == ll {
                    score -= 1.5 * profile.variation;
                }
            }

            // Deterministic tie-break jitter so identical clips still alternate.
            score += ((k as f64 * 0.61 + i as f64 * 0.37).sin()).abs() * 0.05;

            if best.map_or(true, |(_, bs)| score > bs) {
                best = Some((m, score));
            }
        }
        let m = match best.map(|(m, _)| m) {
            Some(m) => m,
            None => continue,
        };

        // 4. Conform: slow-mo plays back slower, consuming less source.
        //    Performance clips that were lip-sync aligned to the master play at
        //    real time and are positioned so their audio matches the song.
        let synced = if m.role == "performance" {
            m.audio_offset
                .filter(|_| m.sync_confidence.unwrap_or(0.0) >= 0.15)
        } else {
            None
        };
        let speed = if synced.is_some() {
            100.0
        } else if m.is_slow_mo {
            m.speed_pct.unwrap_or(100.0).clamp(10.0, 100.0)
        } else {
            100.0
        };
        let mut need = timeline_dur * speed / 100.0;

        let dur = usable_duration(m);
        let mut src_in;
        if let Some(offset) = synced {
            // Master time `seg_start` lives at clip source time `seg_start-offset`.
            src_in = seg_start - offset;
            if dur > 0.0 {
                if need > dur {
                    need = dur;
                }
                src_in = src_in.clamp(0.0, (dur - need).max(0.0));
            } else {
                src_in = src_in.max(0.0);
            }
        } else {
            // Prefer the best in-shot MOMENT of this take: a well-exposed,
            // sharp window whose motion matches the section energy (punchy on
            // drops/builds, calm holds elsewhere) and that we have not used
            // yet. This is what makes the edit "make sense" — cuts land on good
            // content inside the right shot instead of a blind cursor sweep.
            let want_motion = matches!(label.as_str(), "drop" | "build");
            let picked = footage.get(&m.id).and_then(|fp| {
                let avail = if dur > 0.0 { dur } else { fp.duration };
                let mut n = need;
                if n + 2.0 * HEAD_MARGIN > avail {
                    n = (avail - 2.0 * HEAD_MARGIN).max(0.0);
                }
                if n <= 0.0 {
                    return None;
                }
                let spans = used_spans.get(&m.id).map(|v| v.as_slice()).unwrap_or(&[]);
                fp.best_window(n, want_motion, spans, HEAD_MARGIN).map(|s| (s, n))
            });
            if let Some((s, n)) = picked {
                need = n;
                src_in = s;
            } else {
                // Fallback: sequential cursor through the clip (no profile).
                src_in = *cursors.get(&m.id).unwrap_or(&HEAD_MARGIN);
                if dur > 0.0 {
                    if need + 2.0 * HEAD_MARGIN > dur {
                        need = (dur - 2.0 * HEAD_MARGIN).max(0.0).min(dur);
                        if need <= 0.0 {
                            need = dur.min(timeline_dur);
                        }
                        src_in = ((dur - need) / 2.0).max(0.0);
                    } else if src_in + need + HEAD_MARGIN > dur {
                        src_in = HEAD_MARGIN;
                    }
                } else {
                    src_in = 0.0;
                }
            }
        }
        let src_out = src_in + need;
        cursors.insert(m.id, src_out + REUSE_GAP);
        used_spans.entry(m.id).or_default().push((src_in, src_out));

        // Bookkeeping for variation, usage balance + b-roll cadence.
        recent.push_back(m.id);
        while recent.len() > recent_window {
            recent.pop_front();
        }
        last_layer = m.layer_group.or(Some(m.id));
        *used.entry(m.id).or_insert(0) += 1;
        if is_ai_broll(m) {
            since_broll = 0;
        } else {
            since_broll += 1;
        }

        let role_lbl = match m.role.as_str() {
            "performance" => "performance",
            "story" => "story",
            _ => "extra",
        };
        let shot = shot_type_of(m).unwrap_or_default();
        let mut reason = format!("{label} · {role_lbl}");
        if !shot.is_empty() {
            reason.push_str(&format!(" · {shot}"));
        }
        if m.is_slow_mo {
            reason.push_str(&format!(" · slow-mo {:.0}%", speed));
        }
        if synced.is_some() {
            reason.push_str(" · synced");
        }
        if is_ai_broll(m) {
            reason.push_str(" · ai b-roll");
        }

        segments.push(EditSegment {
            id: 0,
            session_id: session.id,
            order_index: segments.len() as i64,
            media_id: m.id,
            src_in,
            src_out,
            timeline_in: seg_start,
            timeline_out: seg_end,
            speed_pct: speed,
            section: Some(label),
            reason: Some(reason),
        });
    }

    segments
}
