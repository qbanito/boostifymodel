use crate::models::Clip;
use anyhow::Result;
use serde_json::json;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The canonical dataset directory layout required by the spec.
const SUBDIRS: &[&str] = &[
    "videos",
    "captions",
    "metadata",
    "pose",
    "depth",
    "edges",
    "segmentation",
    "optical_flow",
];

/// Build the full dataset tree from approved clips and write JSONL splits.
/// Returns the absolute path of the dataset root.
pub fn export(out_root: &Path, name: &str, format: &str, clips: &[Clip]) -> Result<PathBuf> {
    let root = out_root.join(name);
    for sub in SUBDIRS {
        fs::create_dir_all(root.join(sub))?;
    }

    let mut records = Vec::with_capacity(clips.len());

    for (i, clip) in clips.iter().enumerate() {
        let stem = format!("{:06}", i);
        let video_rel = format!("videos/{stem}.mp4");
        let caption_rel = format!("captions/{stem}.txt");
        let meta_rel = format!("metadata/{stem}.json");

        // Copy the clip video into the dataset (best-effort).
        let src = Path::new(&clip.path);
        if src.exists() {
            let _ = fs::copy(src, root.join(&video_rel));
        }

        // Caption file.
        let caption = clip.caption.clone().unwrap_or_default();
        fs::write(root.join(&caption_rel), &caption)?;

        // Per-clip metadata.
        let meta = json!({
            "id": clip.id,
            "video": video_rel,
            "caption": caption,
            "tags": clip.tags,
            "duration_seconds": clip.duration_seconds,
            "quality_score": clip.quality_score,
            "training_value": clip.training_value,
            "analysis": clip.analysis,
        });
        fs::write(root.join(&meta_rel), serde_json::to_vec_pretty(&meta)?)?;

        // Placeholder control-signal sidecars (filled by motion-extraction sidecars).
        for (dir, ext) in [
            ("pose", "json"),
            ("depth", "json"),
            ("edges", "json"),
            ("segmentation", "json"),
            ("optical_flow", "json"),
        ] {
            let p = root.join(dir).join(format!("{stem}.{ext}"));
            if !p.exists() {
                let _ = fs::write(&p, "{}");
            }
        }

        records.push(format_record(format, &video_rel, &caption, clip));
    }

    write_splits(&root, &records)?;
    write_readme(&root, name, format, clips.len())?;

    Ok(root)
}

/// Format a single JSONL record according to the target training format.
fn format_record(format: &str, video_rel: &str, caption: &str, clip: &Clip) -> serde_json::Value {
    match format {
        "cosmos-predict" | "cosmos-transfer" => json!({
            "video": video_rel,
            "prompt": caption,
            "fps": 24,
            "tags": clip.tags,
        }),
        "lora" => json!({
            "file": video_rel,
            "text": caption,
        }),
        "nemo" => json!({
            "video_filepath": video_rel,
            "caption": caption,
            "duration": clip.duration_seconds,
        }),
        // generic video fine-tuning
        _ => json!({
            "path": video_rel,
            "caption": caption,
            "tags": clip.tags,
            "duration": clip.duration_seconds,
        }),
    }
}

/// Write train/validation/test JSONL with an 80/10/10 split.
fn write_splits(root: &Path, records: &[serde_json::Value]) -> Result<()> {
    let n = records.len();
    let train_end = (n as f64 * 0.8).round() as usize;
    let val_end = (n as f64 * 0.9).round() as usize;

    write_jsonl(&root.join("train.jsonl"), &records[..train_end.min(n)])?;
    write_jsonl(
        &root.join("validation.jsonl"),
        &records[train_end.min(n)..val_end.min(n)],
    )?;
    write_jsonl(&root.join("test.jsonl"), &records[val_end.min(n)..])?;
    Ok(())
}

fn write_jsonl(path: &Path, records: &[serde_json::Value]) -> Result<()> {
    let mut f = fs::File::create(path)?;
    for r in records {
        writeln!(f, "{}", serde_json::to_string(r)?)?;
    }
    Ok(())
}

fn write_readme(root: &Path, name: &str, format: &str, count: usize) -> Result<()> {
    let readme = format!(
        "# {name}\n\nBoostify Dataset Studio export.\n\n\
        - Format: `{format}`\n- Clips: {count}\n- Splits: train 80% / validation 10% / test 10%\n\n\
        Layout: videos/, captions/, metadata/, pose/, depth/, edges/, \
        segmentation/, optical_flow/ + train/validation/test.jsonl\n"
    );
    fs::write(root.join("README.md"), readme)?;
    Ok(())
}
