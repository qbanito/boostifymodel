// Premiere / Final Cut Pro 7 (xmeml v5) export. Produces an .xml that imports
// straight into Premiere Pro as a sequence: the song master on an audio track,
// the auto-cut footage on a video track, each clip colour-labelled by role
// (performance / story / master) and slow-motion clips carrying a Premiere
// constant-speed filter so the conform survives the round-trip.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::models::{EditSegment, EditSession, SessionMedia};

/// Premiere clip label colour for each footage role (valid `label2` values).
fn role_label(role: &str) -> &'static str {
    match role {
        "performance" => "Rose",
        "story" => "Caribbean",
        "master" => "Mango",
        _ => "Iris",
    }
}

/// FCP7 `<rate>` timebase + ntsc flag for a frame rate.
fn rate_parts(fps: f64) -> (i64, &'static str) {
    if (fps - 23.976).abs() < 0.05 {
        (24, "TRUE")
    } else if (fps - 29.97).abs() < 0.05 {
        (30, "TRUE")
    } else if (fps - 59.94).abs() < 0.05 {
        (60, "TRUE")
    } else {
        (fps.round() as i64, "FALSE")
    }
}

fn rate_xml(fps: f64) -> String {
    let (tb, ntsc) = rate_parts(fps);
    format!("<rate><timebase>{tb}</timebase><ntsc>{ntsc}</ntsc></rate>")
}

fn frames(sec: f64, fps: f64) -> i64 {
    (sec * fps).round() as i64
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Build a `file://` URL for a local absolute path.
fn file_url(path: &str) -> String {
    let mut url = String::from("file://localhost");
    for ch in path.chars() {
        match ch {
            '/' => url.push('/'),
            ' ' => url.push_str("%20"),
            c if c.is_ascii_alphanumeric()
                || matches!(c, '/' | '.' | '-' | '_' | '~') =>
            {
                url.push(c)
            }
            c => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    url.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    url
}

/// Premiere constant-speed filter (value is a percentage, e.g. 40 = 40%).
fn speed_filter(pct: f64) -> String {
    format!(
        "\t\t\t\t\t<filter>\n\
         \t\t\t\t\t\t<effect>\n\
         \t\t\t\t\t\t\t<name>Time Remap</name>\n\
         \t\t\t\t\t\t\t<effectid>timeremap</effectid>\n\
         \t\t\t\t\t\t\t<effectcategory>motion</effectcategory>\n\
         \t\t\t\t\t\t\t<effecttype>motion</effecttype>\n\
         \t\t\t\t\t\t\t<mediatype>video</mediatype>\n\
         \t\t\t\t\t\t\t<parameter authoringApp=\"PremierePro\">\n\
         \t\t\t\t\t\t\t\t<effectid>speed</effectid>\n\
         \t\t\t\t\t\t\t\t<name>speed</name>\n\
         \t\t\t\t\t\t\t\t<valuemin>-100000</valuemin>\n\
         \t\t\t\t\t\t\t\t<valuemax>100000</valuemax>\n\
         \t\t\t\t\t\t\t\t<value>{pct:.2}</value>\n\
         \t\t\t\t\t\t\t</parameter>\n\
         \t\t\t\t\t\t\t<parameter authoringApp=\"PremierePro\">\n\
         \t\t\t\t\t\t\t\t<effectid>reverse</effectid>\n\
         \t\t\t\t\t\t\t\t<name>reverse</name>\n\
         \t\t\t\t\t\t\t\t<value>FALSE</value>\n\
         \t\t\t\t\t\t\t</parameter>\n\
         \t\t\t\t\t\t\t<parameter authoringApp=\"PremierePro\">\n\
         \t\t\t\t\t\t\t\t<effectid>frameblending</effectid>\n\
         \t\t\t\t\t\t\t\t<name>frame blending</name>\n\
         \t\t\t\t\t\t\t\t<value>FALSE</value>\n\
         \t\t\t\t\t\t\t</parameter>\n\
         \t\t\t\t\t\t</effect>\n\
         \t\t\t\t\t</filter>\n"
    )
}

/// Render the whole xmeml document for a session edit.
fn build_xml(
    session: &EditSession,
    segments: &[EditSegment],
    media: &[SessionMedia],
) -> String {
    let fps = session.sequence_fps;
    let rate = rate_xml(fps);
    let seq_name = xml_escape(&session.name);
    let total_frames = segments
        .iter()
        .map(|s| frames(s.timeline_out, fps))
        .max()
        .unwrap_or(0)
        .max(1);

    let find = |id: i64| media.iter().find(|m| m.id == id);
    let width = segments
        .iter()
        .find_map(|s| find(s.media_id).and_then(|m| m.width))
        .unwrap_or(1920);
    let height = segments
        .iter()
        .find_map(|s| find(s.media_id).and_then(|m| m.height))
        .unwrap_or(1080);

    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE xmeml>\n");
    out.push_str("<xmeml version=\"5\">\n");
    out.push_str(&format!("\t<sequence id=\"seq-{}\">\n", session.id));
    out.push_str(&format!("\t\t<name>{seq_name}</name>\n"));
    out.push_str(&format!("\t\t<duration>{total_frames}</duration>\n"));
    out.push_str(&format!("\t\t{rate}\n"));
    out.push_str("\t\t<media>\n");

    // ---- Video ----
    out.push_str("\t\t\t<video>\n");
    out.push_str("\t\t\t\t<format>\n\t\t\t\t\t<samplecharacteristics>\n");
    out.push_str(&format!("\t\t\t\t\t\t{rate}\n"));
    out.push_str(&format!("\t\t\t\t\t\t<width>{width}</width>\n"));
    out.push_str(&format!("\t\t\t\t\t\t<height>{height}</height>\n"));
    out.push_str("\t\t\t\t\t</samplecharacteristics>\n\t\t\t\t</format>\n");
    out.push_str("\t\t\t\t<track>\n");

    let mut seen_files: HashSet<i64> = HashSet::new();
    for s in segments {
        let m = match find(s.media_id) {
            Some(m) => m,
            None => continue,
        };
        let name = xml_escape(&m.filename);
        let tl_in = frames(s.timeline_in, fps);
        let tl_out = frames(s.timeline_out, fps);
        let src_in = frames(s.src_in, fps);
        let src_out = frames(s.src_out, fps).max(src_in + 1);
        let clip_dur = frames(m.duration_seconds.unwrap_or(0.0), fps).max(src_out);
        let label = role_label(&m.role);

        out.push_str(&format!("\t\t\t\t\t<clipitem id=\"clip-{}\">\n", s.id));
        out.push_str(&format!("\t\t\t\t\t\t<name>{name}</name>\n"));
        out.push_str("\t\t\t\t\t\t<enabled>TRUE</enabled>\n");
        out.push_str(&format!("\t\t\t\t\t\t<duration>{clip_dur}</duration>\n"));
        out.push_str(&format!("\t\t\t\t\t\t{rate}\n"));
        out.push_str(&format!("\t\t\t\t\t\t<start>{tl_in}</start>\n"));
        out.push_str(&format!("\t\t\t\t\t\t<end>{tl_out}</end>\n"));
        out.push_str(&format!("\t\t\t\t\t\t<in>{src_in}</in>\n"));
        out.push_str(&format!("\t\t\t\t\t\t<out>{src_out}</out>\n"));

        // File definition (full on first use, reference afterwards).
        if seen_files.insert(m.id) {
            out.push_str(&format!("\t\t\t\t\t\t<file id=\"file-{}\">\n", m.id));
            out.push_str(&format!("\t\t\t\t\t\t\t<name>{name}</name>\n"));
            out.push_str(&format!(
                "\t\t\t\t\t\t\t<pathurl>{}</pathurl>\n",
                xml_escape(&file_url(&m.path))
            ));
            out.push_str(&format!("\t\t\t\t\t\t\t{rate}\n"));
            out.push_str(&format!("\t\t\t\t\t\t\t<duration>{clip_dur}</duration>\n"));
            out.push_str("\t\t\t\t\t\t\t<media><video><samplecharacteristics>\n");
            out.push_str(&format!("\t\t\t\t\t\t\t\t{rate}\n"));
            out.push_str(&format!(
                "\t\t\t\t\t\t\t\t<width>{}</width>\n",
                m.width.unwrap_or(width)
            ));
            out.push_str(&format!(
                "\t\t\t\t\t\t\t\t<height>{}</height>\n",
                m.height.unwrap_or(height)
            ));
            out.push_str(
                "\t\t\t\t\t\t\t</samplecharacteristics></video></media>\n",
            );
            out.push_str("\t\t\t\t\t\t</file>\n");
        } else {
            out.push_str(&format!("\t\t\t\t\t\t<file id=\"file-{}\"/>\n", m.id));
        }

        out.push_str(&format!(
            "\t\t\t\t\t\t<labels><label2>{label}</label2></labels>\n"
        ));
        if let Some(reason) = &s.reason {
            out.push_str(&format!(
                "\t\t\t\t\t\t<comments><mastercomment1>{}</mastercomment1></comments>\n",
                xml_escape(reason)
            ));
        }
        if s.speed_pct < 99.0 {
            out.push_str(&speed_filter(s.speed_pct));
        }
        out.push_str("\t\t\t\t\t</clipitem>\n");
    }
    out.push_str("\t\t\t\t</track>\n");
    out.push_str("\t\t\t</video>\n");

    // ---- Audio: the song master across the whole sequence ----
    if let Some(master) = media
        .iter()
        .find(|m| m.role == "master" || m.kind == "audio")
    {
        let name = xml_escape(&master.filename);
        let dur = frames(master.duration_seconds.unwrap_or(0.0), fps).max(total_frames);
        out.push_str("\t\t\t<audio>\n\t\t\t\t<track>\n");
        out.push_str("\t\t\t\t\t<clipitem id=\"audio-master\">\n");
        out.push_str(&format!("\t\t\t\t\t\t<name>{name}</name>\n"));
        out.push_str("\t\t\t\t\t\t<enabled>TRUE</enabled>\n");
        out.push_str(&format!("\t\t\t\t\t\t<duration>{dur}</duration>\n"));
        out.push_str(&format!("\t\t\t\t\t\t{rate}\n"));
        out.push_str("\t\t\t\t\t\t<start>0</start>\n");
        out.push_str(&format!("\t\t\t\t\t\t<end>{total_frames}</end>\n"));
        out.push_str("\t\t\t\t\t\t<in>0</in>\n");
        out.push_str(&format!("\t\t\t\t\t\t<out>{total_frames}</out>\n"));
        out.push_str("\t\t\t\t\t\t<file id=\"file-master\">\n");
        out.push_str(&format!("\t\t\t\t\t\t\t<name>{name}</name>\n"));
        out.push_str(&format!(
            "\t\t\t\t\t\t\t<pathurl>{}</pathurl>\n",
            xml_escape(&file_url(&master.path))
        ));
        out.push_str(&format!("\t\t\t\t\t\t\t{rate}\n"));
        out.push_str("\t\t\t\t\t\t\t<media><audio><samplecharacteristics>\n");
        out.push_str("\t\t\t\t\t\t\t\t<samplerate>48000</samplerate>\n");
        out.push_str("\t\t\t\t\t\t\t\t<depth>16</depth>\n");
        out.push_str("\t\t\t\t\t\t\t</samplecharacteristics></audio></media>\n");
        out.push_str("\t\t\t\t\t\t</file>\n");
        out.push_str("\t\t\t\t\t\t<labels><label2>Mango</label2></labels>\n");
        out.push_str("\t\t\t\t\t</clipitem>\n");
        out.push_str("\t\t\t\t</track>\n\t\t\t</audio>\n");
    }

    out.push_str("\t\t</media>\n");
    out.push_str("\t</sequence>\n");
    out.push_str("</xmeml>\n");
    out
}

/// Turn a session name into a filesystem-safe slug.
fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed: String = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if trimmed.is_empty() {
        "session".into()
    } else {
        trimmed
    }
}

/// Write the Premiere project folder for a session edit and return its path.
pub fn export_premiere(
    out_root: &Path,
    session: &EditSession,
    segments: &[EditSegment],
    media: &[SessionMedia],
) -> Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let folder = out_root.join(format!("{}_{}", slug(&session.name), stamp));
    fs::create_dir_all(&folder)?;

    let xml = build_xml(session, segments, media);
    let xml_path = folder.join(format!("{}.xml", slug(&session.name)));
    fs::write(&xml_path, xml.as_bytes())?;

    Ok(folder)
}
