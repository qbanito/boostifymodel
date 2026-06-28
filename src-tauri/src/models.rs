use serde::{Deserialize, Serialize};

/// A discovered source video file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFile {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub hash: Option<String>,
    pub size_bytes: i64,
    pub duration_seconds: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub fps: Option<f64>,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub status: String,
    pub processed: bool,
    pub dataset_id: Option<i64>,
    pub artist: Option<String>,
    pub project: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A generated clip extracted from a source video.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: i64,
    pub video_id: i64,
    pub path: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration_seconds: f64,
    pub caption: Option<String>,
    pub tags: Vec<String>,
    pub quality_score: Option<f64>,
    pub training_value: Option<f64>,
    pub status: String,
    pub approved: Option<bool>,
    pub thumbnail_path: Option<String>,
    pub analysis: Option<SceneAnalysis>,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneAnalysis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub people_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shot_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_movement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_of_day: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lighting: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub instruments: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub phase: String,
    pub files_discovered: u64,
    pub files_indexed: u64,
    pub files_skipped: u64,
    pub current_path: Option<String>,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineProgress {
    pub video_id: Option<i64>,
    pub stage: String,
    pub message: String,
    pub clips_created: u64,
    pub clips_approved: u64,
    pub clips_rejected: u64,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub videos_found: i64,
    pub videos_processed: i64,
    pub clips_created: i64,
    pub clips_approved: i64,
    pub clips_rejected: i64,
    pub avg_process_seconds: f64,
    pub dataset_size_bytes: i64,
    pub gpu_mode: String,
    pub storage_free_bytes: i64,
    pub storage_total_bytes: i64,
    pub avg_training_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetInfo {
    pub id: i64,
    pub name: String,
    pub format: String,
    pub clip_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub quality_threshold: f64,
    pub min_clip_seconds: f64,
    pub scene_threshold: f64,
    pub openai_api_key: String,
    pub nim_api_key: String,
    pub nim_model: String,
    pub output_dir: String,
    pub export_format: String,
    pub watch_enabled: bool,
    pub concurrency: i64,
    /// Name of the remote Brev GPU instance to control (start/stop) from the app.
    #[serde(default = "default_gpu_instance")]
    pub gpu_instance: String,
    /// Base URL of the private Boostify AI Engine (FastAPI on the GPU server),
    /// e.g. http://localhost:8080 via `brev port-forward`. Empty = use NVIDIA
    /// cloud + local Ken Burns instead of the installed models.
    #[serde(default)]
    pub ai_engine_url: String,
    /// API key for the AI Engine (sent as `x-api-key`).
    #[serde(default)]
    pub ai_engine_key: String,
    /// Preferred installed image model (flux-schnell | flux-dev | qwen-image).
    #[serde(default = "default_image_model")]
    pub image_model: String,
    /// Preferred installed video model (ltx-2.3 | wan-t2v | wan-i2v | wan-ti2v).
    /// Empty = keep local Ken Burns animation for B-roll/shots.
    #[serde(default)]
    pub video_model: String,
    /// Preferred installed music model (ace-step-xl-base | sft | turbo).
    #[serde(default = "default_music_model")]
    pub music_model: String,
    /// Hugging Face token (Inference Providers) — enables REAL AI image-to-video
    /// (Wan 2.2 on fal-ai) for B-roll/shots instead of the local Ken Burns pan.
    /// Empty = keep local Ken Burns animation.
    #[serde(default)]
    pub hf_token: String,
}

fn default_gpu_instance() -> String {
    "boostify1".into()
}

fn default_image_model() -> String {
    "flux-dev".into()
}

fn default_music_model() -> String {
    "ace-step-xl-base".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            quality_threshold: 60.0,
            min_clip_seconds: 2.0,
            scene_threshold: 0.4,
            openai_api_key: String::new(),
            nim_api_key: String::new(),
            nim_model: "nvidia/llama-3.1-nemotron-nano-vl-8b-v1".into(),
            output_dir: String::new(),
            export_format: "cosmos-predict".into(),
            watch_enabled: false,
            concurrency: 4,
            gpu_instance: default_gpu_instance(),
            ai_engine_url: String::new(),
            ai_engine_key: String::new(),
            image_model: default_image_model(),
            video_model: String::new(),
            music_model: default_music_model(),
            hf_token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub mode: String,
    pub device: String,
    pub available: bool,
}

/// Status of the remote Brev GPU server, surfaced to the UI so the user can
/// power it on/off and connect from inside the app.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuServerStatus {
    /// Whether the `brev` CLI is installed locally.
    pub installed: bool,
    /// Whether the user is logged in to Brev.
    pub logged_in: bool,
    /// The instance name we are controlling.
    pub instance: String,
    /// RUNNING | STOPPED | STARTING | STOPPING | NOT_FOUND | NOT_LOGGED_IN | NO_BREV | ERROR | UNKNOWN
    pub status: String,
    /// GPU type reported by Brev (e.g. "L4").
    pub gpu: String,
    /// Machine type reported by Brev.
    pub machine: String,
    /// SSH host alias to connect to (matches the instance name).
    pub ssh_host: String,
    /// Human-readable message / hint for the UI.
    pub message: String,
}

/// One model installed/available on the Boostify AI Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineModel {
    pub id: String,
    pub domain: String,
    pub label: String,
}

/// Status of the private Boostify AI Engine (the FastAPI inference server that
/// serves the installed FLUX/Qwen/LTX/Wan/ACE-Step models), surfaced to the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// Whether an engine URL is configured in Settings.
    pub configured: bool,
    /// Whether the engine answered /health.
    pub reachable: bool,
    /// The base URL we are talking to.
    pub base_url: String,
    /// Models advertised by the engine.
    pub models: Vec<EngineModel>,
    /// Human-readable message / hint for the UI.
    pub message: String,
}


#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipFilter {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub approved_only: Option<bool>,
    #[serde(default)]
    pub min_score: Option<f64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyStatus {
    pub ffmpeg: bool,
    pub ffprobe: bool,
}

// ---------------------------------------------------------------------------
// Music-video editing sessions
// ---------------------------------------------------------------------------

/// A music-video editing session — one song/master with its footage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSession {
    pub id: i64,
    pub name: String,
    pub artist: Option<String>,
    /// The song master (audio file, or a reference performance video).
    pub master_path: Option<String>,
    pub master_duration: Option<f64>,
    /// Target timeline frame rate (e.g. 23.976, 24, 25, 30).
    pub sequence_fps: f64,
    /// 'draft' | 'classified' | 'edited' | 'exported'.
    pub status: String,
    pub created_at: String,
}

/// A media file added to a session (footage clip, or the master).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMedia {
    pub id: i64,
    pub session_id: i64,
    pub path: String,
    pub filename: String,
    /// 'video' | 'audio'.
    pub kind: String,
    /// 'story' | 'performance' | 'master' | 'unsorted'.
    pub role: String,
    /// True when the user manually set the role (skip auto-reclassify).
    pub role_locked: bool,
    pub duration_seconds: Option<f64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// Container average frame rate.
    pub container_fps: Option<f64>,
    /// True capture rate (r_frame_rate) — high for slow-motion source.
    pub source_fps: Option<f64>,
    /// True when the source was shot faster than the timeline (slow-mo capable).
    pub is_slow_mo: bool,
    /// Recommended conform speed when placed on the timeline (seq/source*100).
    pub speed_pct: Option<f64>,
    /// Groups multiple angles/takes (layers) of the same master performance.
    pub layer_group: Option<i64>,
    /// Classification confidence 0..1.
    pub confidence: Option<f64>,
    /// Seconds into the master song where this clip's own audio begins
    /// (computed by cross-correlating the clip audio against the master).
    /// Used to lip-sync performance footage to the song.
    pub audio_offset: Option<f64>,
    /// Confidence 0..1 of the audio alignment above.
    pub sync_confidence: Option<f64>,
    /// Model rationale / one-line description.
    pub note: Option<String>,
    pub analysis: Option<SceneAnalysis>,
    pub thumbnail_path: Option<String>,
    pub created_at: String,
}

/// One entry of the generated edit decision list (filled by the edit engine).
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSegment {
    pub id: i64,
    pub session_id: i64,
    pub order_index: i64,
    pub media_id: i64,
    pub src_in: f64,
    pub src_out: f64,
    pub timeline_in: f64,
    pub timeline_out: f64,
    pub speed_pct: f64,
    pub section: Option<String>,
    pub reason: Option<String>,
}

/// Progress event emitted while a session ingests/classifies media.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionProgress {
    pub session_id: i64,
    pub stage: String,
    pub message: String,
    pub processed: u64,
    pub total: u64,
    pub done: bool,
}

/// A detected structural section of the song master.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SongSection {
    pub start: f64,
    pub end: f64,
    /// Heuristic label: intro | low | build | drop | bridge | outro.
    pub label: String,
    /// Normalized loudness 0..1 across the song.
    pub energy: f64,
}

/// Beat / tempo / structure analysis of a session's master audio.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterAnalysis {
    pub duration: f64,
    pub bpm: f64,
    /// Seconds of the first detected downbeat.
    pub first_beat: f64,
    pub beat_count: usize,
    /// Beat onset times in seconds.
    pub beats: Vec<f64>,
    pub sections: Vec<SongSection>,
}

/// Learned editing preferences that steer the edit engine. Nudged by user
/// feedback so the auto-editor improves over time (lightweight "training").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditProfile {
    /// Multiplier on beats-per-cut. <1 = faster cutting, >1 = slower.
    pub cadence: f64,
    /// 0..1 overall preference for performance over story footage.
    pub performance_bias: f64,
    /// 0..1 how often to cut away to story (b-roll) during performance runs.
    pub broll_freq: f64,
    /// 0..1 preference for conformed slow-motion in calm sections.
    pub slowmo_affinity: f64,
    /// 0..1 strength of camera/angle variation between consecutive cuts.
    pub variation: f64,
    /// How many edits have contributed to this profile (training count).
    pub samples: i64,
}

impl Default for EditProfile {
    fn default() -> Self {
        EditProfile {
            cadence: 1.0,
            performance_bias: 0.55,
            broll_freq: 0.4,
            slowmo_affinity: 0.6,
            variation: 0.6,
            samples: 0,
        }
    }
}

/// A visual "fingerprint" captured from the session's own footage. Used to keep
/// AI-generated B-roll coherent with the artist's color, lighting and style.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleReference {
    /// Dominant hex colors sampled across the footage.
    pub palette: Vec<String>,
    /// One-line natural-language style descriptor (mood, lighting, setting…).
    pub descriptor: String,
    /// Thumbnail paths of the keyframes used as the reference.
    pub keyframes: Vec<String>,
    /// Recurring performer name, when known.
    pub artist: Option<String>,
}

/// A single AI-generated B-roll asset (image, optionally animated to video)
/// that can be reviewed and inserted into the edit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrollCandidate {
    pub id: i64,
    pub session_id: i64,
    /// Target song section this b-roll was designed for (intro, drop…).
    pub section: String,
    /// Short human-readable idea label.
    pub idea: String,
    /// Full generation prompt sent to the image model.
    pub prompt: String,
    pub image_path: Option<String>,
    pub video_path: Option<String>,
    pub thumbnail_path: Option<String>,
    /// 'planned' | 'image' | 'video' | 'inserted' | 'failed'.
    pub status: String,
    pub note: Option<String>,
    pub created_at: String,
}

