//! AI editor agent — analyses a session timeline (the beat-aligned EDL of the
//! artist's real footage) and proposes concrete, music-video-grade edits the
//! way a human editor would: cut pacing, speed ramps, b-roll cutaways, section
//! transitions and Remotion effects.
//!
//! It is grounded in the REAL edit: every suggestion references actual segments,
//! sections and beats. A free NVIDIA text reasoner does the creative judgement
//! (`ai::reason_json`); when no key is available it falls back to deterministic
//! heuristics so the panel always returns something useful.

use crate::models::{
    AppSettings, EditAgentReport, EditEffect, EditProfile, EditSegment, EditSession, EditSuggestion,
    MasterAnalysis, SessionMedia, TimelineStats,
};
use std::collections::HashMap;

/// The Remotion effect presets the agent can propose and preview on a clip.
/// Each `id` maps to a branch in the Remotion `EditEffect` composition.
pub fn effect_catalog() -> Vec<EditEffect> {
    let e = |id: &str, label: &str, cat: &str, desc: &str| EditEffect {
        id: id.into(),
        label: label.into(),
        category: cat.into(),
        description: desc.into(),
    };
    vec![
        e("crossfade", "Crossfade", "transition", "Soft dissolve between two shots — good between sections."),
        e("whip-pan", "Whip pan", "transition", "Fast directional blur cut — energetic beat transition."),
        e("zoom-punch", "Zoom punch-in", "motion", "Sharp scale punch on the beat for emphasis."),
        e("beat-flash", "Beat flash", "motion", "White/exposure flash synced to the downbeat."),
        e("speed-ramp", "Speed ramp", "time", "Ramp from slow-motion into real-time on the hit."),
        e("rgb-glitch", "RGB glitch", "texture", "Chromatic-aberration glitch for drops / aggressive cuts."),
        e("light-leak", "Light leak", "texture", "Warm analog light wash over the shot."),
        e("film-burn", "Film burn", "texture", "Vintage film grain + burn for moody sections."),
        e("lower-third", "Lower third", "text", "Animated artist / lyric text overlay."),
        e("grade-warm", "Warm grade", "color", "Cinematic warm teal-and-orange color push."),
        e("grade-noir", "Noir grade", "color", "High-contrast desaturated mood grade."),
    ]
}

fn effect_ids() -> Vec<String> {
    effect_catalog().into_iter().map(|e| e.id).collect()
}

/// True for AI-generated B-roll inserted into the session (tagged on insert).
fn is_ai_broll(m: &SessionMedia) -> bool {
    m.analysis
        .as_ref()
        .map(|a| a.labels.iter().any(|l| l.eq_ignore_ascii_case("ai b-roll")))
        .unwrap_or(false)
}

fn nearest_beat_distance(t: f64, beats: &[f64]) -> f64 {
    beats
        .iter()
        .map(|b| (b - t).abs())
        .fold(f64::INFINITY, f64::min)
}

fn fmt_time(t: f64) -> String {
    let m = (t / 60.0).floor() as i64;
    let s = (t - (m as f64) * 60.0).floor() as i64;
    format!("{m}:{s:02}")
}

/// Section label covering timeline time `t`, if any.
fn section_at<'a>(analysis: &'a MasterAnalysis, t: f64) -> Option<&'a str> {
    analysis
        .sections
        .iter()
        .find(|s| t >= s.start && t < s.end)
        .map(|s| s.label.as_str())
}

/// Compute the high-level timeline statistics.
pub fn compute_stats(
    analysis: &MasterAnalysis,
    media: &[SessionMedia],
    segments: &[EditSegment],
) -> TimelineStats {
    let by_id: HashMap<i64, &SessionMedia> = media.iter().map(|m| (m.id, m)).collect();
    let segment_count = segments.len() as i64;
    let total_seconds = segments.last().map(|s| s.timeline_out).unwrap_or(0.0);
    let avg_cut_seconds = if segment_count > 0 {
        total_seconds / segment_count as f64
    } else {
        0.0
    };
    let bpm = analysis.bpm.max(1.0);
    let beat_seconds = 60.0 / bpm;
    let beats_per_cut = if beat_seconds > 0.0 {
        avg_cut_seconds / beat_seconds
    } else {
        0.0
    };

    let mut perf = 0i64;
    let mut story = 0i64;
    let mut broll = 0i64;
    let mut slowmo = 0i64;
    let mut off_beat = 0i64;
    for s in segments {
        if let Some(m) = by_id.get(&s.media_id) {
            match m.role.as_str() {
                "performance" => perf += 1,
                "story" => story += 1,
                _ => {}
            }
            if is_ai_broll(m) {
                broll += 1;
            }
            if m.is_slow_mo || s.speed_pct < 95.0 {
                slowmo += 1;
            }
        }
        if !analysis.beats.is_empty()
            && nearest_beat_distance(s.timeline_in, &analysis.beats) > 0.12
        {
            off_beat += 1;
        }
    }
    let denom = segment_count.max(1) as f64;
    TimelineStats {
        segment_count,
        total_seconds,
        avg_cut_seconds,
        bpm,
        beats_per_cut,
        performance_pct: perf as f64 / denom * 100.0,
        story_pct: story as f64 / denom * 100.0,
        broll_count: broll,
        slowmo_count: slowmo,
        off_beat_cuts: off_beat,
    }
}

/// Analyse the timeline and return a report with concrete suggestions. Uses the
/// NVIDIA reasoner when a key is configured, falling back to heuristics.
pub fn analyze(
    session: &EditSession,
    analysis: &MasterAnalysis,
    media: &[SessionMedia],
    segments: &[EditSegment],
    _profile: &EditProfile,
    settings: &AppSettings,
) -> EditAgentReport {
    let stats = compute_stats(analysis, media, segments);

    if let Some(report) = analyze_with_llm(session, analysis, media, segments, &stats, settings) {
        return report;
    }

    // Deterministic fallback.
    let suggestions = heuristic_suggestions(analysis, media, segments, &stats);
    EditAgentReport {
        model: "heuristic".into(),
        summary: format!(
            "Edit con {} cortes en {} ({:.1} beats por corte, {:.0}% performance / {:.0}% story).",
            stats.segment_count,
            fmt_time(stats.total_seconds),
            stats.beats_per_cut,
            stats.performance_pct,
            stats.story_pct
        ),
        pacing: pacing_note(&stats),
        stats,
        suggestions,
    }
}

fn pacing_note(stats: &TimelineStats) -> String {
    if stats.beats_per_cut > 8.0 {
        "El ritmo de corte es lento para un videoclip: los planos duran muchos beats.".into()
    } else if stats.beats_per_cut < 1.5 {
        "Cortes muy rápidos — vigila que no se vuelva mareante en las secciones tranquilas.".into()
    } else {
        "Ritmo de corte dentro de un rango musical razonable.".into()
    }
}

/// Build the compact, model-friendly description of the timeline.
fn timeline_brief(
    analysis: &MasterAnalysis,
    media: &[SessionMedia],
    segments: &[EditSegment],
    stats: &TimelineStats,
) -> String {
    let by_id: HashMap<i64, &SessionMedia> = media.iter().map(|m| (m.id, m)).collect();
    let mut out = String::new();
    out.push_str(&format!(
        "SONG: bpm={:.0} duration={:.1}s sections=[",
        analysis.bpm, analysis.duration
    ));
    for (i, s) in analysis.sections.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!(
            "{}({:.0}-{:.0}s e={:.2})",
            s.label, s.start, s.end, s.energy
        ));
    }
    out.push_str("]\n");
    out.push_str(&format!(
        "STATS: cuts={} avgCut={:.2}s beatsPerCut={:.1} perf={:.0}% story={:.0}% broll={} slowmo={} offBeat={}\n",
        stats.segment_count,
        stats.avg_cut_seconds,
        stats.beats_per_cut,
        stats.performance_pct,
        stats.story_pct,
        stats.broll_count,
        stats.slowmo_count,
        stats.off_beat_cuts
    ));
    out.push_str("TIMELINE (index | in-out | section | role | speed%):\n");
    // Cap the number of lines so the prompt stays small on long edits.
    let max_lines = 60usize;
    let step = (segments.len() / max_lines).max(1);
    for (i, s) in segments.iter().enumerate() {
        if i % step != 0 && i != segments.len() - 1 {
            continue;
        }
        let role = by_id
            .get(&s.media_id)
            .map(|m| {
                if is_ai_broll(m) {
                    "broll".to_string()
                } else {
                    m.role.clone()
                }
            })
            .unwrap_or_else(|| "?".into());
        out.push_str(&format!(
            "{} | {:.1}-{:.1} | {} | {} | {:.0}\n",
            s.order_index,
            s.timeline_in,
            s.timeline_out,
            s.section.clone().unwrap_or_else(|| "-".into()),
            role,
            s.speed_pct
        ));
    }
    out
}

fn analyze_with_llm(
    session: &EditSession,
    analysis: &MasterAnalysis,
    media: &[SessionMedia],
    segments: &[EditSegment],
    stats: &TimelineStats,
    settings: &AppSettings,
) -> Option<EditAgentReport> {
    let effects = effect_ids().join(", ");
    let system = format!(
        "You are a senior music-video editor reviewing a beat-aligned timeline. \
         Give concrete, actionable edit notes like a real editor: cut pacing, speed ramps, \
         b-roll cutaways, reordering, section transitions and post effects. \
         Be specific and reference the segment index and section. \
         Only propose effects from this catalog: [{effects}]. \
         Reply with STRICT JSON only, no prose, in this shape: \
         {{\"summary\": string, \"pacing\": string, \"suggestions\": [ \
         {{\"title\": string, \"kind\": \"pacing|reorder|speed|broll|transition|text|color|effect\", \
         \"target\": string, \"severity\": \"info|suggest|important\", \"rationale\": string, \
         \"action\": \"set_speed|split_faster|mark_broll|none\", \"segmentIndex\": number|null, \
         \"value\": number|null, \"effectId\": string|null }} ] }}. \
         For action set_speed, value is the new speed percent (e.g. 50 for slow-motion). \
         Give between 4 and 8 suggestions, ordered by impact."
    );
    let artist = session.artist.clone().unwrap_or_else(|| "the artist".into());
    let lyrics = session
        .lyrics
        .as_deref()
        .map(|l| {
            let s: String = l.chars().take(300).collect();
            format!("\nLYRICS (for theme/mood): {s}")
        })
        .unwrap_or_default();
    let user = format!(
        "Artist: {artist}{lyrics}\n\n{}\n\nReturn the JSON edit report now.",
        timeline_brief(analysis, media, segments, stats)
    );

    let (val, model) = crate::ai::reason_json(
        &system,
        &user,
        &settings.nim_api_key,
        &settings.nim_model,
    )?;

    let summary = val
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pacing = val
        .get("pacing")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let raw = val.get("suggestions").and_then(|v| v.as_array())?;
    let valid_effects = effect_ids();
    let mut suggestions = Vec::new();
    for (i, item) in raw.iter().enumerate() {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Suggestion")
            .to_string();
        let kind = item
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("pacing")
            .to_string();
        let target = item
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let severity = item
            .get("severity")
            .and_then(|v| v.as_str())
            .unwrap_or("suggest")
            .to_string();
        let rationale = item
            .get("rationale")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut action = item
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string();
        let segment_index = item
            .get("segmentIndex")
            .and_then(|v| v.as_i64());
        let value = item.get("value").and_then(|v| v.as_f64());
        let effect_id = item
            .get("effectId")
            .and_then(|v| v.as_str())
            .filter(|s| valid_effects.iter().any(|e| e == s))
            .map(str::to_string);

        // Guard actionable suggestions that lack the data to apply them.
        if (action == "set_speed" && (segment_index.is_none() || value.is_none()))
            || (action == "split_faster" && segment_index.is_none())
            || (action == "mark_broll" && segment_index.is_none())
        {
            action = "none".into();
        }

        suggestions.push(EditSuggestion {
            id: format!("ai-{i}"),
            title,
            kind,
            target,
            severity,
            rationale,
            action,
            segment_index,
            value,
            effect_id,
            applied: false,
        });
    }
    if suggestions.is_empty() {
        return None;
    }
    Some(EditAgentReport {
        model,
        summary: if summary.is_empty() {
            "Notas de edición generadas por IA.".into()
        } else {
            summary
        },
        pacing: if pacing.is_empty() {
            pacing_note(stats)
        } else {
            pacing
        },
        stats: stats.clone(),
        suggestions,
    })
}

/// Deterministic editor notes when no model is available — still grounded in the
/// real timeline so the panel is useful offline.
fn heuristic_suggestions(
    analysis: &MasterAnalysis,
    _media: &[SessionMedia],
    segments: &[EditSegment],
    stats: &TimelineStats,
) -> Vec<EditSuggestion> {
    let mut out: Vec<EditSuggestion> = Vec::new();
    let beat = 60.0 / stats.bpm.max(1.0);

    // 1. Global pacing too slow.
    if stats.beats_per_cut > 6.0 {
        // Find the single longest segment in a high-energy section to split.
        let target = segments
            .iter()
            .filter(|s| {
                section_at(analysis, s.timeline_in)
                    .map(|l| matches!(l, "build" | "drop"))
                    .unwrap_or(false)
            })
            .max_by(|a, b| {
                (a.timeline_out - a.timeline_in)
                    .partial_cmp(&(b.timeline_out - b.timeline_in))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| {
                segments.iter().max_by(|a, b| {
                    (a.timeline_out - a.timeline_in)
                        .partial_cmp(&(b.timeline_out - b.timeline_in))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
        if let Some(s) = target {
            out.push(EditSuggestion {
                id: "h-pace".into(),
                title: "Acelera el ritmo de corte".into(),
                kind: "pacing".into(),
                target: format!("Corte {} ({})", s.order_index, fmt_time(s.timeline_in)),
                severity: "important".into(),
                rationale: format!(
                    "Promedio de {:.1} beats por corte: para un videoclip conviene cortar más a menudo, sobre todo en build/drop. Divide este plano largo en dos.",
                    stats.beats_per_cut
                ),
                action: "split_faster".into(),
                segment_index: Some(s.order_index),
                value: None,
                effect_id: Some("zoom-punch".into()),
                applied: false,
            });
        }
    }

    // 2. Off-beat cuts.
    if stats.off_beat_cuts as f64 > stats.segment_count as f64 * 0.25 {
        out.push(EditSuggestion {
            id: "h-beat".into(),
            title: "Cuadra los cortes al beat".into(),
            kind: "pacing".into(),
            target: "Timeline".into(),
            severity: "suggest".into(),
            rationale: format!(
                "{} de {} cortes caen lejos del beat más cercano. Reconstruye el edit para alinearlos al tempo de {:.0} BPM.",
                stats.off_beat_cuts, stats.segment_count, stats.bpm
            ),
            action: "none".into(),
            segment_index: None,
            value: None,
            effect_id: None,
            applied: false,
        });
    }

    // 3. No slow-motion in a calm section → speed ramp.
    if stats.slowmo_count == 0 {
        if let Some(s) = segments.iter().find(|s| {
            section_at(analysis, s.timeline_in)
                .map(|l| matches!(l, "intro" | "low" | "bridge"))
                .unwrap_or(false)
        }) {
            out.push(EditSuggestion {
                id: "h-slowmo".into(),
                title: "Añade slow-motion en la sección tranquila".into(),
                kind: "speed".into(),
                target: format!("Corte {} ({})", s.order_index, fmt_time(s.timeline_in)),
                severity: "suggest".into(),
                rationale:
                    "No hay slow-motion en todo el edit. Ralentiza un plano de la parte calmada al 50% para dar respiro y contraste antes del drop.".into(),
                action: "set_speed".into(),
                segment_index: Some(s.order_index),
                value: Some(50.0),
                effect_id: Some("speed-ramp".into()),
                applied: false,
            });
        }
    }

    // 4. No b-roll cutaways.
    if stats.broll_count == 0 {
        out.push(EditSuggestion {
            id: "h-broll".into(),
            title: "Intercala cutaways de b-roll".into(),
            kind: "broll".into(),
            target: "Drop / build".into(),
            severity: "suggest".into(),
            rationale:
                "El edit es 100% tomas directas. Inserta b-roll (storytelling) en los picos para variar y respirar — genera tomas en el B-roll Studio.".into(),
            action: "none".into(),
            segment_index: None,
            value: None,
            effect_id: None,
            applied: false,
        });
    }

    // 5. Section transition effect.
    if let Some(drop) = analysis.sections.iter().find(|s| s.label == "drop") {
        if let Some(s) = segments
            .iter()
            .find(|s| (s.timeline_in - drop.start).abs() < beat * 2.0)
        {
            out.push(EditSuggestion {
                id: "h-drop-fx".into(),
                title: "Marca la entrada del drop".into(),
                kind: "effect".into(),
                target: format!("Drop @ {}", fmt_time(drop.start)),
                severity: "suggest".into(),
                rationale:
                    "Refuerza la llegada del drop con un flash al beat y un punch de zoom para impacto.".into(),
                action: "none".into(),
                segment_index: Some(s.order_index),
                value: None,
                effect_id: Some("beat-flash".into()),
                applied: false,
            });
        }
    }

    // 6. Performance/story balance.
    if stats.story_pct < 15.0 && stats.segment_count > 6 {
        out.push(EditSuggestion {
            id: "h-balance".into(),
            title: "Equilibra performance y narrativa".into(),
            kind: "reorder".into(),
            target: "Timeline".into(),
            severity: "info".into(),
            rationale: format!(
                "Solo {:.0}% del edit es story. Intercala más tomas narrativas entre los bloques de performance para contar algo.",
                stats.story_pct
            ),
            action: "none".into(),
            segment_index: None,
            value: None,
            effect_id: None,
            applied: false,
        });
    }

    if out.is_empty() {
        out.push(EditSuggestion {
            id: "h-ok".into(),
            title: "El edit está equilibrado".into(),
            kind: "info".into(),
            target: "Timeline".into(),
            severity: "info".into(),
            rationale:
                "El ritmo, el balance performance/story y el uso de velocidad están en rango. Prueba efectos del catálogo para darle acabado.".into(),
            action: "none".into(),
            segment_index: None,
            value: None,
            effect_id: Some("grade-warm".into()),
            applied: false,
        });
    }

    out
}

/// Apply a suggestion's machine action to the timeline, returning the new EDL.
/// Keeps the timeline consistent (re-flows downstream cuts after a change).
pub fn apply_suggestion(
    segments: &[EditSegment],
    suggestion: &EditSuggestion,
) -> Result<Vec<EditSegment>, String> {
    let mut segs: Vec<EditSegment> = segments.to_vec();
    segs.sort_by_key(|s| s.order_index);
    let idx = suggestion
        .segment_index
        .ok_or_else(|| "esta sugerencia no apunta a ningún corte".to_string())?;
    let pos = segs
        .iter()
        .position(|s| s.order_index == idx)
        .ok_or_else(|| "no se encontró el corte objetivo".to_string())?;

    match suggestion.action.as_str() {
        "set_speed" => {
            let new_speed = suggestion
                .value
                .ok_or_else(|| "falta el valor de velocidad".to_string())?
                .clamp(25.0, 400.0);
            let tl_in = segs[pos].timeline_in;
            let src_dur = (segs[pos].src_out - segs[pos].src_in).max(0.01);
            let old_tl = segs[pos].timeline_out - tl_in;
            let new_tl = src_dur * (100.0 / new_speed);
            let delta = new_tl - old_tl;
            segs[pos].speed_pct = new_speed;
            segs[pos].timeline_out = tl_in + new_tl;
            segs[pos].reason = Some(format!(
                "Speed ramp {new_speed:.0}% (IA)"
            ));
            // Shift everything after by the duration delta.
            for s in segs.iter_mut().skip(pos + 1) {
                s.timeline_in += delta;
                s.timeline_out += delta;
            }
        }
        "split_faster" => {
            let s = segs[pos].clone();
            let src_mid = (s.src_in + s.src_out) / 2.0;
            let tl_mid = (s.timeline_in + s.timeline_out) / 2.0;
            let first = EditSegment {
                src_out: src_mid,
                timeline_out: tl_mid,
                reason: Some("Split para acelerar el ritmo (IA)".into()),
                ..s.clone()
            };
            let second = EditSegment {
                id: 0,
                src_in: src_mid,
                timeline_in: tl_mid,
                reason: Some("Split para acelerar el ritmo (IA)".into()),
                ..s.clone()
            };
            segs[pos] = first;
            segs.insert(pos + 1, second);
        }
        "mark_broll" => {
            segs[pos].reason = Some("Marcado para cutaway de b-roll (IA)".into());
        }
        "trim_in" => {
            // Recorta el inicio: avanza src_in (y, por defecto, 0.5s).
            let amt = suggestion.value.unwrap_or(0.5).max(0.05);
            let max = (segs[pos].src_out - segs[pos].src_in - 0.1).max(0.0);
            let cut = amt.min(max);
            segs[pos].src_in += cut;
            segs[pos].reason = Some("Recorte de entrada manual".into());
        }
        "trim_out" => {
            // Recorta el final: adelanta src_out y acorta la duración en timeline.
            let amt = suggestion.value.unwrap_or(0.5).max(0.05);
            let max = (segs[pos].src_out - segs[pos].src_in - 0.1).max(0.0);
            let cut = amt.min(max);
            segs[pos].src_out -= cut;
            let new_tl = (segs[pos].timeline_out - segs[pos].timeline_in - cut).max(0.1);
            let delta = (segs[pos].timeline_out - segs[pos].timeline_in) - new_tl;
            segs[pos].timeline_out = segs[pos].timeline_in + new_tl;
            for s in segs.iter_mut().skip(pos + 1) {
                s.timeline_in -= delta;
                s.timeline_out -= delta;
            }
            segs[pos].reason = Some("Recorte de salida manual".into());
        }
        "remove" => {
            let removed = segs[pos].timeline_out - segs[pos].timeline_in;
            segs.remove(pos);
            for s in segs.iter_mut().skip(pos) {
                s.timeline_in -= removed;
                s.timeline_out -= removed;
            }
        }
        other => {
            return Err(format!("la acción '{other}' no es aplicable automáticamente"));
        }
    }

    // Re-index order_index to be contiguous.
    for (i, s) in segs.iter_mut().enumerate() {
        s.order_index = i as i64;
    }
    Ok(segs)
}
