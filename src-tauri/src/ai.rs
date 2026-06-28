//! AI services — scene analysis, captioning and tagging.
//!
//! This module is intentionally modular. Each function has a deterministic
//! local-heuristic implementation (so the pipeline always runs offline) plus a
//! clearly marked hook where a real model is meant to plug in:
//!
//!   * `analyze_scene`  -> YOLO (people/objects) + MediaPipe (pose/face)
//!   * `generate_caption` -> NVIDIA NIM vision-language model (e.g. `nvidia/vila`)
//!
//! The heavy models are expected to run as out-of-process sidecars / HTTP calls
//! so they can be swapped, scaled and GPU-accelerated independently.

use crate::models::SceneAnalysis;
use crate::splitter::FrameStats;

/// Local heuristic scene analysis from frame statistics + filename cues.
/// Replace/augment with YOLO + MediaPipe results when a sidecar is available.
pub fn analyze_scene(stats: &FrameStats, filename: &str, duration: f64) -> SceneAnalysis {
    let name = filename.to_lowercase();
    let mut a = SceneAnalysis::default();

    // --- Lighting / time of day from brightness ---
    a.time_of_day = Some(match stats.brightness {
        b if b < 35.0 => "night",
        b if b < 70.0 => "blue_hour",
        b if b < 110.0 => "golden_hour",
        _ => "day",
    }
    .to_string());

    a.lighting = Some(if stats.brightness < 45.0 {
        if contains_any(&name, &["neon", "club", "rave"]) {
            "neon"
        } else {
            "low_key"
        }
    } else if stats.brightness > 180.0 {
        "backlight"
    } else {
        "natural"
    }
    .to_string());

    // --- Shot type from detail/variance ---
    a.shot_type = Some(if stats.variance > 3200.0 {
        "wide"
    } else if stats.variance > 1400.0 {
        "medium"
    } else {
        "close_up"
    }
    .to_string());

    // --- Camera movement from filename cues ---
    a.camera_movement = Some(
        first_match(
            &name,
            &[
                ("drone", "drone"),
                ("aerial", "drone"),
                ("dji", "drone"),
                ("gimbal", "gimbal"),
                ("steadicam", "steadicam"),
                ("steady", "steadicam"),
                ("pov", "pov"),
                ("gopro", "pov"),
                ("handheld", "handheld"),
            ],
        )
        .unwrap_or("static")
        .to_string(),
    );

    // --- Setting ---
    a.setting = first_match(
        &name,
        &[
            ("studio", "studio"),
            ("concert", "concert"),
            ("live", "concert"),
            ("stage", "concert"),
            ("beach", "beach"),
            ("city", "city"),
            ("street", "city"),
            ("urban", "city"),
            ("nature", "nature"),
            ("forest", "nature"),
            ("car", "automobile"),
            ("auto", "automobile"),
        ],
    )
    .map(|s| s.to_string());

    // --- Mood / action ---
    a.mood = Some(
        first_match(
            &name,
            &[
                ("luxury", "luxury"),
                ("romantic", "romantic"),
                ("love", "romantic"),
                ("aggressive", "aggressive"),
                ("vintage", "vintage"),
                ("fashion", "fashion"),
            ],
        )
        .unwrap_or("cinematic")
        .to_string(),
    );

    a.action = first_match(
        &name,
        &[
            ("sing", "singing"),
            ("danc", "dancing"),
            ("walk", "walking"),
            ("talk", "talking"),
            ("perform", "performance"),
        ],
    )
    .map(|s| s.to_string());

    // --- Instruments ---
    for (kw, label) in [
        ("guitar", "guitar"),
        ("piano", "piano"),
        ("drum", "drums"),
        ("violin", "violin"),
        ("sax", "saxophone"),
    ] {
        if name.contains(kw) {
            a.instruments.push(label.to_string());
        }
    }

    // --- People / face (heuristic; real value comes from YOLO/MediaPipe) ---
    a.people_count = Some(if a.setting.as_deref() == Some("concert") {
        3
    } else {
        1
    });
    a.face_visible = Some(a.shot_type.as_deref() != Some("wide"));

    // --- Aggregate labels ---
    let mut labels: Vec<String> = vec![];
    for v in [
        &a.time_of_day,
        &a.lighting,
        &a.shot_type,
        &a.camera_movement,
        &a.setting,
        &a.mood,
        &a.action,
    ] {
        if let Some(s) = v {
            labels.push(s.replace('_', " "));
        }
    }
    labels.extend(a.instruments.iter().cloned());
    if duration >= 8.0 {
        labels.push("long take".into());
    }
    a.labels = labels;

    a
}

/// Generate auto tags from a scene analysis (DATASET auto-tagging).
pub fn auto_tags(a: &SceneAnalysis) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let push = |tags: &mut Vec<String>, s: &Option<String>| {
        if let Some(v) = s {
            tags.push(v.replace('_', " "));
        }
    };
    push(&mut tags, &a.action);
    push(&mut tags, &a.shot_type);
    push(&mut tags, &a.setting);
    push(&mut tags, &a.lighting);
    push(&mut tags, &a.time_of_day);
    push(&mut tags, &a.mood);
    push(&mut tags, &a.camera_movement);
    tags.extend(a.instruments.iter().cloned());
    tags.sort();
    tags.dedup();
    tags
}

/// Produce a rich training caption.
///
/// Tries a real vision-language model first (OpenAI `gpt-4o-mini` or an
/// OpenAI-compatible NVIDIA NIM endpoint) by sending the clip's thumbnail.
/// Falls back to a deterministic caption built from the scene analysis when no
/// key is configured or the request fails. Returns a single descriptive
/// sentence suitable for video-model training.
pub fn generate_caption(
    a: &SceneAnalysis,
    artist: Option<&str>,
    thumb: &std::path::Path,
    openai_api_key: &str,
    nim_api_key: &str,
    nim_model: &str,
) -> String {
    // Resolve keys: explicit settings value wins, else fall back to env vars so
    // the feature works even before the user fills in the Settings UI.
    let openai_key = first_nonempty(openai_api_key, &["OPENAI_API_KEY"]);
    let nim_key = first_nonempty(nim_api_key, &["NIM_API_KEY", "NVIDIA_API_KEY"]);

    // Encode the representative frame once for whichever provider is used.
    let image_b64 = read_image_b64(thumb);

    if let Some(b64) = image_b64.as_deref() {
        let prompt = caption_prompt(a, artist);

        if !openai_key.is_empty() {
            if let Some(c) = vlm_caption(
                "https://api.openai.com/v1/chat/completions",
                &openai_key,
                "gpt-4o-mini",
                &prompt,
                b64,
            ) {
                return c;
            }
        }

        if !nim_key.is_empty() {
            let model = if nim_model.trim().is_empty() {
                "meta/llama-3.2-11b-vision-instruct"
            } else {
                nim_model
            };
            if let Some(c) = vlm_caption(
                "https://integrate.api.nvidia.com/v1/chat/completions",
                &nim_key,
                model,
                &prompt,
                b64,
            ) {
                return c;
            }
        }
    }

    heuristic_caption(a, artist)
}

/// Returns `primary` when non-empty, otherwise the first non-empty value found
/// among the given environment variables (trimmed).
fn first_nonempty(primary: &str, env_vars: &[&str]) -> String {
    let p = primary.trim();
    if !p.is_empty() {
        return p.to_string();
    }
    for v in env_vars {
        if let Ok(val) = std::env::var(v) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return val;
            }
        }
    }
    String::new()
}

/// Read an image file and return it as base64 (no data-URL prefix). `None` on
/// read failure or empty file.
fn read_image_b64(path: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// Build the instruction sent to the vision model. The scene analysis is passed
/// only as a soft hint — the model is told to trust the pixels first.
fn caption_prompt(a: &SceneAnalysis, artist: Option<&str>) -> String {
    let hints = {
        let mut h: Vec<String> = Vec::new();
        if let Some(s) = a.shot_type.as_deref() {
            h.push(format!("shot~{}", s));
        }
        if let Some(l) = a.lighting.as_deref() {
            h.push(format!("light~{}", l.replace('_', " ")));
        }
        if let Some(t) = a.time_of_day.as_deref() {
            h.push(format!("time~{}", t.replace('_', " ")));
        }
        if let Some(c) = a.camera_movement.as_deref() {
            h.push(format!("camera~{}", c));
        }
        h.join(", ")
    };

    let artist_line = match artist {
        Some(name) if !name.trim().is_empty() => format!(
            "If a recurring performer is clearly the focus, you may refer to them as \"{}\". ",
            name.trim()
        ),
        _ => String::new(),
    };

    format!(
        "You are labeling a single frame from a video clip to train a text-to-video model. \
         Look carefully at the image and write ONE vivid English caption, a single sentence of \
         15 to 40 words, describing EXACTLY what is visible: the subject(s), what they are doing, \
         the setting/location, lighting, time of day, and the camera framing. \
         {artist_line}\
         Do NOT invent details you cannot see. Do NOT assume it is a music performance or concert \
         unless instruments, a stage, or an audience are actually visible. \
         Soft auto-detected hints (may be wrong, trust the image over these): {hints}. \
         Reply with only the caption, no preamble, quotes, or list markers."
    )
}

/// POST the thumbnail + prompt to an OpenAI-compatible chat-completions endpoint
/// and return the model's caption. Returns `None` on any network/parse error so
/// the caller can fall back to the heuristic.
fn vlm_caption(
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    image_b64: &str,
) -> Option<String> {
    let text = vlm_raw(url, api_key, model, prompt, image_b64, 220)?;
    let cleaned = clean_caption(&text);
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Low-level vision chat request. Returns the raw model message content (no
/// post-processing). `None` on any network/parse error.
fn vlm_raw(
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    image_b64: &str,
    max_tokens: u32,
) -> Option<String> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "temperature": 0.2,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url",
                  "image_url": { "url": format!("data:image/jpeg;base64,{image_b64}") } }
            ]
        }]
    });

    let resp = ureq::post(url)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(45))
        .send_json(body);

    let json: serde_json::Value = match resp {
        Ok(r) => r.into_json().ok()?,
        Err(ureq::Error::Status(code, r)) => {
            let detail = r
                .into_string()
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect::<String>();
            eprintln!("[vlm] {model} returned HTTP {code}: {detail}");
            return None;
        }
        Err(e) => {
            eprintln!("[vlm] {model} request failed: {e}");
            return None;
        }
    };

    json.pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Classify a single representative frame as a `performance` shot (the artist
/// singing/lip-syncing/playing to camera, on a stage, with mic/instruments) or a
/// `story` shot (narrative/cinematic b-roll, locations, actors, no direct
/// performance). Returns `(role, confidence 0..1, reason)`. Falls back to a
/// heuristic from the scene analysis when no vision provider is available.
pub fn classify_role(
    a: &SceneAnalysis,
    artist: Option<&str>,
    thumb: &std::path::Path,
    openai_api_key: &str,
    nim_api_key: &str,
    nim_model: &str,
) -> (String, f64, String) {
    let openai_key = first_nonempty(openai_api_key, &["OPENAI_API_KEY"]);
    let nim_key = first_nonempty(nim_api_key, &["NIM_API_KEY", "NVIDIA_API_KEY"]);
    let image_b64 = read_image_b64(thumb);

    if let Some(b64) = image_b64.as_deref() {
        let prompt = classify_prompt(artist);
        let mut raw: Option<String> = None;

        if !openai_key.is_empty() {
            raw = vlm_raw(
                "https://api.openai.com/v1/chat/completions",
                &openai_key,
                "gpt-4o-mini",
                &prompt,
                b64,
                120,
            );
        }
        if raw.is_none() && !nim_key.is_empty() {
            let model = if nim_model.trim().is_empty() {
                "meta/llama-3.2-11b-vision-instruct"
            } else {
                nim_model
            };
            raw = vlm_raw(
                "https://integrate.api.nvidia.com/v1/chat/completions",
                &nim_key,
                model,
                &prompt,
                b64,
                120,
            );
        }

        if let Some(text) = raw {
            if let Some(parsed) = parse_classification(&text) {
                return parsed;
            }
        }
    }

    heuristic_role(a)
}

fn classify_prompt(artist: Option<&str>) -> String {
    let artist_line = match artist {
        Some(name) if !name.trim().is_empty() => format!(
            "The recurring performer is \"{}\". ",
            name.trim()
        ),
        _ => String::new(),
    };
    format!(
        "You are sorting frames from a music video into two buckets. {artist_line}\
         Classify THIS frame as exactly one of:\n\
         - \"performance\": the artist is singing/rapping/lip-syncing or playing an instrument \
           to the camera, on a stage, holding a microphone, dancing as the focal subject, or any \
           direct-to-camera performance moment.\n\
         - \"story\": narrative or cinematic b-roll — locations, scenery, objects, hands, actors \
           acting out a scene, driving, walking, with no direct musical performance to camera.\n\
         Reply with STRICT JSON on one line and nothing else: \
         {{\"role\":\"performance\"|\"story\",\"confidence\":0.0-1.0,\"reason\":\"<=12 words\"}}."
    )
}

/// Parse the model's JSON classification reply. Tolerant of surrounding prose.
fn parse_classification(raw: &str) -> Option<(String, f64, String)> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    let slice = &raw[start..=end];
    let v: serde_json::Value = serde_json::from_str(slice).ok()?;
    let role = v.get("role").and_then(|r| r.as_str())?.to_lowercase();
    let role = if role.contains("perform") {
        "performance"
    } else if role.contains("story") {
        "story"
    } else {
        return None;
    };
    let confidence = v
        .get("confidence")
        .and_then(|c| c.as_f64())
        .unwrap_or(0.6)
        .clamp(0.0, 1.0);
    let reason = v
        .get("reason")
        .and_then(|r| r.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    Some((role.to_string(), confidence, reason))
}

/// Fallback classification from the cheap scene analysis when no VLM is up.
fn heuristic_role(a: &SceneAnalysis) -> (String, f64, String) {
    let has_instruments = !a.instruments.is_empty();
    let close = matches!(
        a.shot_type.as_deref(),
        Some("close_up") | Some("close-up") | Some("medium")
    );
    let face = a.face_visible.unwrap_or(false);
    let single_person = a.people_count.map(|n| n >= 1 && n <= 2).unwrap_or(false);
    if has_instruments || (face && close && single_person) {
        (
            "performance".to_string(),
            0.45,
            "heuristic: face/instrument focus".to_string(),
        )
    } else {
        (
            "story".to_string(),
            0.4,
            "heuristic: no clear performance".to_string(),
        )
    }
}


/// Strip wrapping quotes/whitespace and collapse newlines from a model reply.
fn clean_caption(raw: &str) -> String {
    let mut s = raw.trim().replace(['\n', '\r'], " ");
    while s.contains("  ") {
        s = s.replace("  ", " ");
    }
    let trimmed = s.trim().trim_matches('"').trim_matches('\'').trim();
    trimmed.to_string()
}

fn heuristic_caption(a: &SceneAnalysis, artist: Option<&str>) -> String {
    let setting = a.setting.as_deref().unwrap_or("an unspecified setting").replace('_', " ");
    let lighting = a.lighting.as_deref().unwrap_or("natural").replace('_', " ");
    let time = a.time_of_day.as_deref().unwrap_or("day").replace('_', " ");
    let shot = a.shot_type.as_deref().unwrap_or("medium").replace('_', " ");
    let camera = a
        .camera_movement
        .as_deref()
        .unwrap_or("static")
        .replace('_', " ");
    let mood = a.mood.as_deref().unwrap_or("cinematic");

    // Subject only when we actually have a name; never invent "a music artist".
    let subject = match artist {
        Some(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => match a.people_count {
            Some(n) if n >= 2 => "several people".to_string(),
            Some(1) => "a person".to_string(),
            _ => "the subject".to_string(),
        },
    };

    // Only mention an action if one was actually detected (from filename cues).
    let action_phrase = match a.action.as_deref() {
        Some(act) if !act.is_empty() => format!(" {}", act.replace('_', " ")),
        _ => String::new(),
    };

    let instruments = if a.instruments.is_empty() {
        String::new()
    } else {
        format!(" with a {}", a.instruments.join(" and "))
    };

    let camera_phrase = match camera.as_str() {
        "drone" => "captured by a sweeping aerial drone shot",
        "gimbal" => "filmed on a smooth gliding gimbal",
        "steadicam" => "tracked by a fluid steadicam",
        "pov" => "shown from an immersive point-of-view perspective",
        "handheld" => "filmed with an energetic handheld camera",
        _ => "framed in a steady locked-off shot",
    };

    format!(
        "A {mood} {shot} shot of {subject}{action_phrase}{instruments} in {setting}, \
         illuminated by {lighting} lighting during {time}, {camera_phrase}.",
    )
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn first_match<'a>(haystack: &str, pairs: &[(&str, &'a str)]) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(kw, _)| haystack.contains(kw))
        .map(|(_, v)| *v)
}
