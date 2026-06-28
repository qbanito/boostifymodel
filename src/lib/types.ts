// Shared types — these mirror the serde structs returned by the Rust backend.

export type ProcessingStatus =
  | "discovered"
  | "indexed"
  | "splitting"
  | "analyzing"
  | "scored"
  | "approved"
  | "rejected"
  | "duplicate"
  | "error";

export interface VideoFile {
  id: number;
  path: string;
  filename: string;
  hash: string | null;
  sizeBytes: number;
  durationSeconds: number | null;
  width: number | null;
  height: number | null;
  fps: number | null;
  codec: string | null;
  container: string | null;
  status: ProcessingStatus;
  processed: boolean;
  datasetId: number | null;
  artist: string | null;
  project: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Clip {
  id: number;
  videoId: number;
  path: string;
  startSeconds: number;
  endSeconds: number;
  durationSeconds: number;
  caption: string | null;
  tags: string[];
  qualityScore: number | null;
  trainingValue: number | null;
  status: ProcessingStatus;
  approved: boolean | null;
  thumbnailPath: string | null;
  analysis: SceneAnalysis | null;
  createdAt: string;
}

export interface SceneAnalysis {
  peopleCount?: number;
  faceVisible?: boolean;
  shotType?: string; // close_up | medium | wide | drone ...
  cameraMovement?: string; // gimbal | steadicam | static | handheld ...
  setting?: string; // studio | concert | beach | city ...
  timeOfDay?: string; // day | night | golden_hour | blue_hour
  lighting?: string; // neon | backlight | warm | natural
  mood?: string; // romantic | aggressive | luxury | cinematic
  action?: string; // singing | dancing | walking | talking
  instruments?: string[];
  labels?: string[];
}

export interface ScanProgress {
  phase: string; // scanning | hashing | probing | done
  filesDiscovered: number;
  filesIndexed: number;
  filesSkipped: number;
  currentPath: string | null;
  done: boolean;
}

export interface PipelineProgress {
  videoId: number | null;
  stage: string;
  message: string;
  clipsCreated: number;
  clipsApproved: number;
  clipsRejected: number;
  done: boolean;
}

export interface DashboardStats {
  videosFound: number;
  videosProcessed: number;
  clipsCreated: number;
  clipsApproved: number;
  clipsRejected: number;
  avgProcessSeconds: number;
  datasetSizeBytes: number;
  gpuMode: string;
  storageFreeBytes: number;
  storageTotalBytes: number;
  avgTrainingScore: number;
}

export interface DatasetInfo {
  id: number;
  name: string;
  format: string;
  clipCount: number;
  createdAt: string;
}

export interface AppSettings {
  qualityThreshold: number;
  minClipSeconds: number;
  sceneThreshold: number;
  openaiApiKey: string;
  nimApiKey: string;
  nimModel: string;
  outputDir: string;
  exportFormat: string;
  watchEnabled: boolean;
  concurrency: number;
  gpuInstance: string;
}

export interface GpuInfo {
  mode: string; // cpu | cuda | metal
  device: string;
  available: boolean;
}

export interface GpuServerStatus {
  installed: boolean;
  loggedIn: boolean;
  instance: string;
  status: string; // RUNNING | STOPPED | STARTING | STOPPING | NOT_FOUND | NOT_LOGGED_IN | NO_BREV | ERROR | UNKNOWN
  gpu: string;
  machine: string;
  sshHost: string;
  message: string;
}

// ---- Music-video editing sessions ----

export type MediaRole = "story" | "performance" | "master" | "unsorted";

export interface EditSession {
  id: number;
  name: string;
  artist: string | null;
  masterPath: string | null;
  masterDuration: number | null;
  sequenceFps: number;
  status: string; // draft | classified | edited | exported
  createdAt: string;
}

export interface SessionMedia {
  id: number;
  sessionId: number;
  path: string;
  filename: string;
  kind: "video" | "audio";
  role: MediaRole;
  roleLocked: boolean;
  durationSeconds: number | null;
  width: number | null;
  height: number | null;
  containerFps: number | null;
  sourceFps: number | null;
  isSlowMo: boolean;
  speedPct: number | null;
  layerGroup: number | null;
  confidence: number | null;
  audioOffset: number | null;
  syncConfidence: number | null;
  note: string | null;
  analysis: SceneAnalysis | null;
  thumbnailPath: string | null;
  createdAt: string;
}

export interface SessionProgress {
  sessionId: number;
  stage: string; // probe | thumb | classify | done
  message: string;
  processed: number;
  total: number;
  done: boolean;
}

export interface SongSection {
  start: number;
  end: number;
  label: string; // intro | low | build | drop | bridge | outro
  energy: number; // 0..1
}

export interface MasterAnalysis {
  duration: number;
  bpm: number;
  firstBeat: number;
  beatCount: number;
  beats: number[];
  sections: SongSection[];
}

export interface EditSegment {
  id: number;
  sessionId: number;
  orderIndex: number;
  mediaId: number;
  srcIn: number;
  srcOut: number;
  timelineIn: number;
  timelineOut: number;
  speedPct: number;
  section: string | null;
  reason: string | null;
}

export interface EditProfile {
  cadence: number;
  performanceBias: number;
  brollFreq: number;
  slowmoAffinity: number;
  variation: number;
  samples: number;
}

export type EditFeedback =
  | "faster"
  | "slower"
  | "more_performance"
  | "more_story"
  | "more_broll"
  | "less_broll"
  | "more_slowmo"
  | "less_slowmo"
  | "more_variation"
  | "less_variation";

/** Visual fingerprint captured from the session's own footage. */
export interface StyleReference {
  palette: string[];
  descriptor: string;
  keyframes: string[];
  artist: string | null;
}

/** An AI-generated B-roll asset that can be reviewed and inserted. */
export interface BrollCandidate {
  id: number;
  sessionId: number;
  section: string;
  idea: string;
  prompt: string;
  imagePath: string | null;
  videoPath: string | null;
  thumbnailPath: string | null;
  status: "planned" | "image" | "video" | "inserted" | "failed";
  note: string | null;
  createdAt: string;
}

/** Progress event emitted while B-roll is generated. */
export interface BrollProgress {
  sessionId: number;
  stage: string;
  message: string;
  processed: number;
  total: number;
  done: boolean;
}
