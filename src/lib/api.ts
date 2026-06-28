import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  AppSettings,
  Clip,
  DashboardStats,
  DatasetInfo,
  EditSession,
  GpuInfo,
  GpuServerStatus,
  MasterAnalysis,
  EditSegment,
  EditProfile,
  EditFeedback,
  StyleReference,
  BrollCandidate,
  BrollProgress,
  PipelineProgress,
  ScanProgress,
  SessionMedia,
  SessionProgress,
  VideoFile,
} from "./types";

/**
 * Thin typed wrapper around the Rust backend commands.
 * Every method maps to a `#[tauri::command]` in src-tauri/src.
 */
export const api = {
  // ---- Library scanning / indexing ----
  async pickFolder(): Promise<string | null> {
    const selected = await open({ directory: true, multiple: false });
    return typeof selected === "string" ? selected : null;
  },

  scanFolder(path: string): Promise<void> {
    return invoke("scan_folder", { path });
  },

  cancelScan(): Promise<void> {
    return invoke("cancel_scan");
  },

  listVideos(limit = 500, offset = 0): Promise<VideoFile[]> {
    return invoke("list_videos", { limit, offset });
  },

  deleteVideos(ids: number[]): Promise<number> {
    return invoke("delete_videos", { ids });
  },

  deleteVideosByStatus(status: string): Promise<number> {
    return invoke("delete_videos_by_status", { status });
  },

  // ---- Pipeline ----
  processVideo(videoId: number): Promise<void> {
    return invoke("process_video", { videoId });
  },

  processAllPending(): Promise<void> {
    return invoke("process_all_pending");
  },

  // ---- Clips / review ----
  listClips(filter?: {
    query?: string;
    approvedOnly?: boolean;
    minScore?: number;
    limit?: number;
    offset?: number;
  }): Promise<Clip[]> {
    return invoke("list_clips", { filter: filter ?? {} });
  },

  setClipApproval(clipId: number, approved: boolean): Promise<void> {
    return invoke("set_clip_approval", { clipId, approved });
  },

  setClipsApproval(clipIds: number[], approved: boolean): Promise<number> {
    return invoke("set_clips_approval", { clipIds, approved });
  },

  updateClipCaption(clipId: number, caption: string): Promise<void> {
    return invoke("update_clip_caption", { clipId, caption });
  },

  updateClipTags(clipId: number, tags: string[]): Promise<void> {
    return invoke("update_clip_tags", { clipId, tags });
  },

  searchClips(query: string): Promise<Clip[]> {
    return invoke("search_clips", { query });
  },

  // ---- Dataset ----
  listDatasets(): Promise<DatasetInfo[]> {
    return invoke("list_datasets");
  },

  exportDataset(name: string, format: string): Promise<string> {
    return invoke("export_dataset", { name, format });
  },

  // ---- Stats / system ----
  dashboardStats(): Promise<DashboardStats> {
    return invoke("dashboard_stats");
  },

  gpuInfo(): Promise<GpuInfo> {
    return invoke("gpu_info");
  },

  checkDependencies(): Promise<{ ffmpeg: boolean; ffprobe: boolean }> {
    return invoke("check_dependencies");
  },

  // ---- Remote GPU server (Brev) ----
  gpuServerStatus(): Promise<GpuServerStatus> {
    return invoke("gpu_server_status");
  },

  gpuServerStart(): Promise<GpuServerStatus> {
    return invoke("gpu_server_start");
  },

  gpuServerStop(): Promise<GpuServerStatus> {
    return invoke("gpu_server_stop");
  },

  // ---- Settings ----
  getSettings(): Promise<AppSettings> {
    return invoke("get_settings");
  },

  saveSettings(settings: AppSettings): Promise<void> {
    return invoke("save_settings", { settings });
  },

  // ---- Watch mode ----
  setWatch(path: string, enabled: boolean): Promise<void> {
    return invoke("set_watch", { path, enabled });
  },

  // ---- Music-video editing sessions ----
  async pickMediaFiles(): Promise<string[]> {
    const selected = await open({
      multiple: true,
      filters: [
        {
          name: "Media",
          extensions: [
            "mp4", "mov", "m4v", "mkv", "avi", "webm", "mts", "m2ts",
            "mxf", "wmv", "flv", "wav", "mp3", "aac", "flac", "m4a",
            "aif", "aiff", "ogg",
          ],
        },
      ],
    });
    if (Array.isArray(selected)) return selected as string[];
    return typeof selected === "string" ? [selected] : [];
  },

  createEditSession(
    name: string,
    artist?: string | null,
    sequenceFps?: number,
  ): Promise<EditSession> {
    return invoke("create_edit_session", {
      name,
      artist: artist ?? null,
      sequenceFps: sequenceFps ?? null,
    });
  },

  listEditSessions(): Promise<EditSession[]> {
    return invoke("list_edit_sessions");
  },

  getEditSession(sessionId: number): Promise<EditSession | null> {
    return invoke("get_edit_session", { sessionId });
  },

  deleteEditSession(sessionId: number): Promise<void> {
    return invoke("delete_edit_session", { sessionId });
  },

  listSessionMedia(sessionId: number): Promise<SessionMedia[]> {
    return invoke("list_session_media", { sessionId });
  },

  addSessionMedia(sessionId: number, paths: string[]): Promise<SessionMedia[]> {
    return invoke("add_session_media", { sessionId, paths });
  },

  setSessionMediaRole(mediaId: number, role: string): Promise<void> {
    return invoke("set_session_media_role", { mediaId, role });
  },

  deleteSessionMedia(mediaId: number): Promise<void> {
    return invoke("delete_session_media", { mediaId });
  },

  analyzeMasterAudio(sessionId: number): Promise<MasterAnalysis> {
    return invoke("analyze_master_audio", { sessionId });
  },

  getMasterAnalysis(sessionId: number): Promise<MasterAnalysis | null> {
    return invoke("get_master_analysis", { sessionId });
  },

  buildSessionEdl(sessionId: number): Promise<EditSegment[]> {
    return invoke("build_session_edl", { sessionId });
  },

  listSessionEdl(sessionId: number): Promise<EditSegment[]> {
    return invoke("list_session_edl", { sessionId });
  },

  exportSessionEdit(sessionId: number): Promise<string> {
    return invoke("export_session_edit", { sessionId });
  },

  getEditProfile(): Promise<EditProfile> {
    return invoke("get_edit_profile");
  },

  updateEditProfile(profile: EditProfile): Promise<EditProfile> {
    return invoke("update_edit_profile", { profile });
  },

  recordEditFeedback(feedback: EditFeedback): Promise<EditProfile> {
    return invoke("record_edit_feedback", { feedback });
  },

  // ---- AI B-roll Studio ----
  captureStyleReference(sessionId: number): Promise<StyleReference> {
    return invoke("capture_style_reference", { sessionId });
  },

  generateBroll(
    sessionId: number,
    count: number,
    animate: boolean,
  ): Promise<BrollCandidate[]> {
    return invoke("generate_broll", { sessionId, count, animate });
  },

  listBroll(sessionId: number): Promise<BrollCandidate[]> {
    return invoke("list_broll", { sessionId });
  },

  insertBroll(candidateId: number): Promise<SessionMedia> {
    return invoke("insert_broll", { candidateId });
  },

  deleteBroll(candidateId: number): Promise<void> {
    return invoke("delete_broll", { candidateId });
  },

  animateBroll(candidateId: number): Promise<BrollCandidate> {
    return invoke("animate_broll", { candidateId });
  },

  generateMusicVideo(sessionId: number): Promise<string> {
    return invoke("generate_music_video", { sessionId });
  },

  syncPerformanceAudio(sessionId: number): Promise<SessionMedia[]> {
    return invoke("sync_performance_audio", { sessionId });
  },

  exportSessionToDataset(sessionId: number): Promise<number> {
    return invoke("export_session_to_dataset", { sessionId });
  },

  onBrollProgress(cb: (p: BrollProgress) => void): Promise<UnlistenFn> {
    return listen<BrollProgress>("broll:progress", (e) => cb(e.payload));
  },

  onMusicVideoProgress(cb: (p: BrollProgress) => void): Promise<UnlistenFn> {
    return listen<BrollProgress>("mvgen:progress", (e) => cb(e.payload));
  },

  onSyncProgress(cb: (p: BrollProgress) => void): Promise<UnlistenFn> {
    return listen<BrollProgress>("sync:progress", (e) => cb(e.payload));
  },

  onSessionDatasetProgress(cb: (p: BrollProgress) => void): Promise<UnlistenFn> {
    return listen<BrollProgress>("session_dataset:progress", (e) => cb(e.payload));
  },

  // ---- Events ----
  onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
    return listen<ScanProgress>("scan:progress", (e) => cb(e.payload));
  },
  onPipelineProgress(cb: (p: PipelineProgress) => void): Promise<UnlistenFn> {
    return listen<PipelineProgress>("pipeline:progress", (e) => cb(e.payload));
  },
  onLog(cb: (line: string) => void): Promise<UnlistenFn> {
    return listen<string>("app:log", (e) => cb(e.payload));
  },
  onSessionProgress(cb: (p: SessionProgress) => void): Promise<UnlistenFn> {
    return listen<SessionProgress>("session:progress", (e) => cb(e.payload));
  },
};

export type Api = typeof api;
