import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/controls";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type {
  EditSession,
  EditSegment,
  EditProfile,
  EditFeedback,
  StyleReference,
  BrollCandidate,
  BrollProgress,
  EngineStatus,
  HumoProgress,
  HumoStatus,
  MasterAnalysis,
  MediaRole,
  SessionMedia,
  SessionProgress,
} from "@/lib/types";
import { cn, formatDuration } from "@/lib/utils";
import {
  Clapperboard,
  Plus,
  RefreshCw,
  Trash2,
  Music,
  Film,
  Gauge,
  Loader2,
  FileVideo,
  AudioWaveform,
  AudioLines,
  Scissors,
  Download,
  Sparkles,
  Wand2,
  ImagePlus,
  Palette,
  ChevronDown,
  ChevronRight,
  X,
  Play,
  Pause,
  Search,
  Database,
  Boxes,
  Disc3,
} from "lucide-react";

const SEQ_FPS_OPTIONS = [23.976, 24, 25, 29.97, 30, 50, 60];

const ROLE_COLUMNS: { role: MediaRole; label: string; hint: string; tint: string }[] = [
  { role: "master", label: "Master", hint: "Song audio / reference", tint: "#f59e0b" },
  { role: "performance", label: "Performance", hint: "Singing / playing to camera", tint: "#ec4899" },
  { role: "story", label: "Story", hint: "Narrative & b-roll", tint: "#22d3ee" },
  { role: "unsorted", label: "Unsorted", hint: "Not classified yet", tint: "#a3a3a3" },
];

function fpsLabel(fps: number | null): string {
  if (fps == null) return "—";
  // Show 23.98 instead of 23.976023... for readability.
  return Number.isInteger(fps) ? String(fps) : fps.toFixed(2);
}

export function Editor() {
  const { push } = useToast();
  const [sessions, setSessions] = useState<EditSession[]>([]);
  const [activeId, setActiveId] = useState<number | null>(null);
  const [media, setMedia] = useState<SessionMedia[]>([]);
  const [newName, setNewName] = useState("");
  const [newArtist, setNewArtist] = useState("");
  const [newFps, setNewFps] = useState(24);
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<SessionProgress | null>(null);
  const [analysis, setAnalysis] = useState<MasterAnalysis | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [edl, setEdl] = useState<EditSegment[]>([]);
  const [building, setBuilding] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [syncProgress, setSyncProgress] = useState<BrollProgress | null>(null);
  const [datasetSending, setDatasetSending] = useState(false);
  const [datasetProgress, setDatasetProgress] = useState<BrollProgress | null>(
    null
  );
  const [mvGenerating, setMvGenerating] = useState(false);
  const [mvProgress, setMvProgress] = useState<BrollProgress | null>(null);
  const [mvPath, setMvPath] = useState<string | null>(null);
  const [profile, setProfile] = useState<EditProfile | null>(null);
  const [broll, setBroll] = useState<BrollCandidate[]>([]);
  const [style, setStyle] = useState<StyleReference | null>(null);
  const [brollCount, setBrollCount] = useState(4);
  const [animateBroll, setAnimateBroll] = useState(true);
  const [generatingBroll, setGeneratingBroll] = useState(false);
  const [brollProgress, setBrollProgress] = useState<BrollProgress | null>(null);
  const [humoImage, setHumoImage] = useState<string | null>(null);
  const [humoAudio, setHumoAudio] = useState<string | null>(null);
  const [humoPrompt, setHumoPrompt] = useState("");
  const [humoQuality, setHumoQuality] = useState<"fast" | "standard" | "high">(
    "fast"
  );
  const [humoBusy, setHumoBusy] = useState(false);
  const [humoProgress, setHumoProgress] = useState<HumoProgress | null>(null);
  const [humoServer, setHumoServer] = useState<HumoStatus | null>(null);
  const [relightVideo, setRelightVideo] = useState<string | null>(null);
  const [relightHdri, setRelightHdri] = useState<string | null>(null);
  const [relightIntensity, setRelightIntensity] = useState(0.8);
  const [relightBusy, setRelightBusy] = useState(false);
  const [relightProgress, setRelightProgress] = useState<HumoProgress | null>(
    null
  );
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [musicPrompt, setMusicPrompt] = useState("");
  const [musicLyrics, setMusicLyrics] = useState("");
  const [musicSeconds, setMusicSeconds] = useState(30);
  const [musicBusy, setMusicBusy] = useState(false);
  const [musicPath, setMusicPath] = useState<string | null>(null);
  const [aiImgPrompt, setAiImgPrompt] = useState("");
  const [aiImgBusy, setAiImgBusy] = useState(false);
  const [aiImgPath, setAiImgPath] = useState<string | null>(null);
  const [aiEditSrc, setAiEditSrc] = useState<string | null>(null);
  const [preview, setPreview] = useState<{ src: string; title: string } | null>(
    null
  );
  const playerSeekRef = useRef<((t: number) => void) | null>(null);
  const progressUnlisten = useRef<(() => void) | null>(null);
  const brollUnlisten = useRef<(() => void) | null>(null);
  const humoUnlisten = useRef<(() => void) | null>(null);
  const relightUnlisten = useRef<(() => void) | null>(null);

  const active = useMemo(
    () => sessions.find((s) => s.id === activeId) ?? null,
    [sessions, activeId]
  );

  const loadSessions = useCallback(async () => {
    try {
      const list = await api.listEditSessions();
      setSessions(list);
      setActiveId((prev) => prev ?? list[0]?.id ?? null);
    } catch {
      /* backend not ready */
    }
  }, []);

  const loadMedia = useCallback(async (sessionId: number) => {
    try {
      setMedia(await api.listSessionMedia(sessionId));
    } catch {
      setMedia([]);
    }
  }, []);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  // Load the global training profile once.
  useEffect(() => {
    api
      .getEditProfile()
      .then((p) => setProfile(p))
      .catch(() => setProfile(null));
  }, []);

  useEffect(() => {
    if (activeId != null) loadMedia(activeId);
    else setMedia([]);
  }, [activeId, loadMedia]);

  // Load any stored master analysis when switching sessions.
  useEffect(() => {
    if (activeId == null) {
      setAnalysis(null);
      return;
    }
    api
      .getMasterAnalysis(activeId)
      .then((a) => setAnalysis(a))
      .catch(() => setAnalysis(null));
  }, [activeId]);

  // Load any previously built edit (EDL) when switching sessions.
  useEffect(() => {
    if (activeId == null) {
      setEdl([]);
      return;
    }
    api
      .listSessionEdl(activeId)
      .then((s) => setEdl(s))
      .catch(() => setEdl([]));
  }, [activeId]);

  // Load AI B-roll candidates + capture the visual style when switching sessions.
  useEffect(() => {
    if (activeId == null) {
      setBroll([]);
      setStyle(null);
      return;
    }
    api
      .listBroll(activeId)
      .then((b) => setBroll(b))
      .catch(() => setBroll([]));
    api
      .captureStyleReference(activeId)
      .then((s) => setStyle(s))
      .catch(() => setStyle(null));
  }, [activeId]);

  // Subscribe to B-roll generation progress.
  useEffect(() => {
    let mounted = true;
    api
      .onBrollProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setBrollProgress(p.done ? null : p);
        }
      })
      .then((un) => {
        brollUnlisten.current = un;
      });
    return () => {
      mounted = false;
      brollUnlisten.current?.();
      brollUnlisten.current = null;
    };
  }, [activeId]);

  // Subscribe to HuMo (AI performance clip) generation progress.
  useEffect(() => {
    let mounted = true;
    api
      .onHumoProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setHumoProgress(p.done ? null : p);
        }
      })
      .then((un) => {
        humoUnlisten.current = un;
      });
    return () => {
      mounted = false;
      humoUnlisten.current?.();
      humoUnlisten.current = null;
    };
  }, [activeId]);

  // Subscribe to Relight (HDRI re-lighting) progress.
  useEffect(() => {
    let mounted = true;
    api
      .onRelightProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setRelightProgress(p.done ? null : p);
        }
      })
      .then((un) => {
        relightUnlisten.current = un;
      });
    return () => {
      mounted = false;
      relightUnlisten.current?.();
      relightUnlisten.current = null;
    };
  }, [activeId]);

  // Probe whether the HuMo render server is reachable (best-effort).
  useEffect(() => {
    let mounted = true;
    api
      .humoStatus()
      .then((s) => {
        if (mounted) setHumoServer(s);
      })
      .catch(() => {
        if (mounted) setHumoServer({ ok: false, reachable: false });
      });
    return () => {
      mounted = false;
    };
  }, [activeId]);

  // Probe the Boostify AI Engine (installed FLUX/Qwen/LTX/Wan/ACE-Step models).
  useEffect(() => {
    let mounted = true;
    api
      .aiEngineStatus()
      .then((st) => {
        if (mounted) setEngine(st);
      })
      .catch(() => {
        if (mounted) setEngine(null);
      });
    return () => {
      mounted = false;
    };
  }, []);

  // Subscribe to performance lip-sync progress.
  useEffect(() => {
    let mounted = true;
    let un: (() => void) | null = null;
    api
      .onSyncProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setSyncProgress(p.done ? null : p);
        }
      })
      .then((u) => {
        un = u;
      });
    return () => {
      mounted = false;
      un?.();
    };
  }, [activeId]);

  // Subscribe to "send session footage to the dataset" progress.
  useEffect(() => {
    let mounted = true;
    let un: (() => void) | null = null;
    api
      .onSessionDatasetProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setDatasetProgress(p.done ? null : p);
          if (p.done) setDatasetSending(false);
        }
      })
      .then((u) => {
        un = u;
      });
    return () => {
      mounted = false;
      un?.();
    };
  }, [activeId]);

  // Subscribe to AI music-video generation progress.
  useEffect(() => {
    setMvPath(null);
    let mounted = true;
    let un: (() => void) | null = null;
    api
      .onMusicVideoProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setMvProgress(p.done ? null : p);
          if (p.done) setMvGenerating(false);
        }
      })
      .then((u) => {
        un = u;
      });
    return () => {
      mounted = false;
      un?.();
    };
  }, [activeId]);

  // Subscribe to ingest progress for the active session.
  useEffect(() => {
    let mounted = true;
    api
      .onSessionProgress((p) => {
        if (!mounted) return;
        if (activeId != null && p.sessionId === activeId) {
          setProgress(p.done ? null : p);
        }
      })
      .then((un) => {
        progressUnlisten.current = un;
      });
    return () => {
      mounted = false;
      progressUnlisten.current?.();
      progressUnlisten.current = null;
    };
  }, [activeId]);

  const createSession = async () => {
    const name = newName.trim();
    if (!name) {
      push("error", "Give the session a name");
      return;
    }
    try {
      const s = await api.createEditSession(
        name,
        newArtist.trim() || null,
        newFps
      );
      setSessions((prev) => [s, ...prev]);
      setActiveId(s.id);
      setNewName("");
      setNewArtist("");
      push("success", `Session "${s.name}" created`);
    } catch (e) {
      push("error", String(e));
    }
  };

  const deleteSession = async (id: number) => {
    try {
      await api.deleteEditSession(id);
      setSessions((prev) => prev.filter((s) => s.id !== id));
      if (activeId === id) setActiveId(null);
      push("success", "Session deleted");
    } catch (e) {
      push("error", String(e));
    }
  };

  const addMedia = async () => {
    if (!active) return;
    const paths = await api.pickMediaFiles();
    if (!paths.length) return;
    setBusy(true);
    try {
      await api.addSessionMedia(active.id, paths);
      await loadMedia(active.id);
      await loadSessions(); // master may have been set
      push("success", `Added ${paths.length} file(s)`);
    } catch (e) {
      push("error", String(e));
    } finally {
      setBusy(false);
      setProgress(null);
    }
  };

  const analyzeMaster = async () => {
    if (!active) return;
    if (!active.masterPath) {
      push("error", "Add the song master (audio) first");
      return;
    }
    setAnalyzing(true);
    try {
      const a = await api.analyzeMasterAudio(active.id);
      setAnalysis(a);
      await loadSessions();
      push(
        "success",
        `Detected ${Math.round(a.bpm)} BPM · ${a.sections.length} sections`
      );
    } catch (e) {
      push("error", String(e));
    } finally {
      setAnalyzing(false);
    }
  };

  const buildEdit = async () => {
    if (!active) return;
    if (!analysis) {
      push("error", "Analyze the master beats first");
      return;
    }
    setBuilding(true);
    try {
      const segs = await api.buildSessionEdl(active.id);
      setEdl(segs);
      await loadSessions();
      push("success", `Built ${segs.length} cuts from the beat grid`);
    } catch (e) {
      push("error", String(e));
    } finally {
      setBuilding(false);
    }
  };

  const syncPerformance = async () => {
    if (!active) return;
    if (!active.masterPath) {
      push("error", "Add the song master (audio) first");
      return;
    }
    const perfCount = media.filter(
      (m) => m.role === "performance" && m.kind === "video"
    ).length;
    if (perfCount === 0) {
      push("error", "No performance clips to sync (classify footage first)");
      return;
    }
    setSyncing(true);
    try {
      const updated = await api.syncPerformanceAudio(active.id);
      setMedia(updated);
      const synced = updated.filter(
        (m) => (m.syncConfidence ?? 0) >= 0.15
      ).length;
      // Re-cut so performance takes land in lip-sync with the song.
      if (analysis) {
        const segs = await api.buildSessionEdl(active.id);
        setEdl(segs);
      }
      push(
        synced > 0 ? "success" : "info",
        synced > 0
          ? `Lip-synced ${synced} performance clip(s) to the master`
          : "Could not confidently align the performance audio"
      );
    } catch (e) {
      push("error", String(e));
    } finally {
      setSyncing(false);
      setSyncProgress(null);
    }
  };

  const sendToDataset = async () => {
    if (!active) return;
    const footage = media.filter(
      (m) => m.kind === "video" && m.role !== "master"
    ).length;
    if (footage === 0) {
      push("error", "No footage in this session to send to the dataset");
      return;
    }
    setDatasetSending(true);
    try {
      const queued = await api.exportSessionToDataset(active.id);
      push(
        "success",
        `Sending ${queued} clip source(s) to the dataset — each is split, identified as performance / b-roll and captioned. Track progress in the Pipeline & Review tabs.`
      );
    } catch (e) {
      setDatasetSending(false);
      setDatasetProgress(null);
      push("error", String(e));
    }
  };

  const generateMv = async () => {
    if (!active) return;
    if (!analysis) {
      push("error", "Analyze the master beats first");
      return;
    }
    setMvGenerating(true);
    setMvPath(null);
    try {
      const path = await api.generateMusicVideo(active.id);
      setMvPath(path);
      push("success", "AI music video generated — playing below.");
    } catch (e) {
      setMvGenerating(false);
      setMvProgress(null);
      push("error", String(e));
    }
  };

  const exportEdit = async () => {
    if (!active) return;
    if (edl.length === 0) {
      push("error", "Build the edit first");
      return;
    }
    setExporting(true);
    try {
      const folder = await api.exportSessionEdit(active.id);
      await loadSessions();
      push("success", `Premiere XML exported → ${folder}`);
    } catch (e) {
      push("error", String(e));
    } finally {
      setExporting(false);
    }
  };

  const sendFeedback = async (fb: EditFeedback, rebuild: boolean) => {
    try {
      const p = await api.recordEditFeedback(fb);
      setProfile(p);
      if (rebuild && active && analysis) {
        const segs = await api.buildSessionEdl(active.id);
        setEdl(segs);
      }
    } catch (e) {
      push("error", String(e));
    }
  };

  const refreshStyle = async () => {
    if (!active) return;
    try {
      setStyle(await api.captureStyleReference(active.id));
      push("success", "Captured the footage style");
    } catch (e) {
      push("error", String(e));
    }
  };

  const generateBroll = async () => {
    if (!active) return;
    if (!analysis) {
      push("error", "Analyze the master beats first");
      return;
    }
    setGeneratingBroll(true);
    try {
      const list = await api.generateBroll(active.id, brollCount, animateBroll);
      setBroll(list);
      const rendered = list.filter(
        (b) => b.status === "video" || b.status === "image"
      ).length;
      push(
        rendered > 0 ? "success" : "info",
        rendered > 0
          ? `Generated ${rendered} B-roll shot(s)`
          : "Planned B-roll — add an NVIDIA key in Settings to render"
      );
    } catch (e) {
      push("error", String(e));
    } finally {
      setGeneratingBroll(false);
      setBrollProgress(null);
    }
  };

  const pickHumoImage = async () => {
    try {
      const p = await api.pickHumoImage();
      if (p) setHumoImage(p);
    } catch (e) {
      push("error", String(e));
    }
  };

  const pickHumoAudio = async () => {
    try {
      const p = await api.pickHumoAudio();
      if (p) setHumoAudio(p);
    } catch (e) {
      push("error", String(e));
    }
  };

  const generateHumo = async () => {
    if (!active) return;
    if (!humoImage || !humoAudio) {
      push("error", "Pick a reference image and an audio clip first");
      return;
    }
    setHumoBusy(true);
    setHumoProgress(null);
    try {
      await api.humoGenerate(
        active.id,
        humoImage,
        humoAudio,
        humoPrompt,
        humoQuality
      );
      await loadMedia(active.id);
      push("success", "AI performance clip added to Performance media");
    } catch (e) {
      push("error", String(e));
    } finally {
      setHumoBusy(false);
      setHumoProgress(null);
    }
  };

  const pickRelightVideo = async () => {
    try {
      const p = await api.pickRelightVideo();
      if (p) setRelightVideo(p);
    } catch (e) {
      push("error", String(e));
    }
  };

  const pickRelightHdri = async () => {
    try {
      const p = await api.pickHdri();
      if (p) setRelightHdri(p);
    } catch (e) {
      push("error", String(e));
    }
  };

  const generateRelight = async () => {
    if (!active) return;
    if (!relightVideo || !relightHdri) {
      push("error", "Pick a clip and an HDRI environment map first");
      return;
    }
    setRelightBusy(true);
    setRelightProgress(null);
    try {
      await api.relightClip(
        active.id,
        relightVideo,
        relightHdri,
        relightIntensity
      );
      await loadMedia(active.id);
      push("success", "Relit clip added to session media");
    } catch (e) {
      push("error", String(e));
    } finally {
      setRelightBusy(false);
      setRelightProgress(null);
    }
  };

  const generateMusic = async () => {
    if (!musicPrompt.trim()) {
      push("error", "Describe the music you want to generate");
      return;
    }
    setMusicBusy(true);
    setMusicPath(null);
    try {
      const path = await api.generateMusicTrack(
        musicPrompt,
        musicSeconds,
        musicLyrics,
      );
      setMusicPath(path);
      push("success", "Music generated with ACE-Step — playing below.");
    } catch (e) {
      push("error", String(e));
    } finally {
      setMusicBusy(false);
    }
  };

  const generateAiImage = async () => {
    if (!aiImgPrompt.trim()) {
      push("error", "Describe the image you want to generate");
      return;
    }
    setAiImgBusy(true);
    setAiImgPath(null);
    try {
      const path = await api.aiGenerateImage(aiImgPrompt);
      setAiImgPath(path);
      push("success", "Image generated — preview below.");
    } catch (e) {
      push("error", String(e));
    } finally {
      setAiImgBusy(false);
    }
  };

  const pickAiEditSrc = async () => {
    try {
      const path = await api.pickImageFile();
      if (path) setAiEditSrc(path);
    } catch (e) {
      push("error", String(e));
    }
  };

  const editAiImage = async () => {
    const src = aiEditSrc ?? aiImgPath;
    if (!src) {
      push("error", "Pick an image to edit first");
      return;
    }
    if (!aiImgPrompt.trim()) {
      push("error", "Describe the edit you want to make");
      return;
    }
    setAiImgBusy(true);
    try {
      const path = await api.aiEditImage(src, aiImgPrompt);
      setAiImgPath(path);
      setAiEditSrc(path);
      push("success", "Image edited — preview below.");
    } catch (e) {
      push("error", String(e));
    } finally {
      setAiImgBusy(false);
    }
  };


    if (!active) return;
    try {
      await api.insertBroll(c.id);
      setBroll((prev) =>
        prev.map((b) => (b.id === c.id ? { ...b, status: "inserted" } : b))
      );
      await loadMedia(active.id);
      if (analysis) {
        const segs = await api.buildSessionEdl(active.id);
        setEdl(segs);
        push("success", "B-roll inserted & edit re-cut");
      } else {
        push("success", "B-roll added as story footage");
      }
    } catch (e) {
      push("error", String(e));
    }
  };

  const deleteBroll = async (c: BrollCandidate) => {
    try {
      await api.deleteBroll(c.id);
      setBroll((prev) => prev.filter((b) => b.id !== c.id));
    } catch (e) {
      push("error", String(e));
    }
  };

  const convertBroll = async (c: BrollCandidate) => {
    try {
      const updated = await api.animateBroll(c.id);
      setBroll((prev) => prev.map((b) => (b.id === c.id ? updated : b)));
      push("success", "Image animated to a video clip");
    } catch (e) {
      push("error", String(e));
    }
  };

  const changeRole = async (m: SessionMedia, role: MediaRole) => {
    try {
      await api.setSessionMediaRole(m.id, role);
      setMedia((prev) =>
        prev.map((x) =>
          x.id === m.id ? { ...x, role, roleLocked: true } : x
        )
      );
    } catch (e) {
      push("error", String(e));
    }
  };

  const removeMedia = async (m: SessionMedia) => {
    try {
      await api.deleteSessionMedia(m.id);
      setMedia((prev) => prev.filter((x) => x.id !== m.id));
    } catch (e) {
      push("error", String(e));
    }
  };

  const grouped = useMemo(() => {
    const map: Record<MediaRole, SessionMedia[]> = {
      master: [],
      performance: [],
      story: [],
      unsorted: [],
    };
    for (const m of media) map[m.role]?.push(m);
    return map;
  }, [media]);

  return (
    <>
      <TopBar
        title="Video Editor"
        subtitle="Auto-classify footage into a music-video edit session"
        busy={busy}
      >
        {active && (
          <>
            <Button variant="secondary" size="sm" onClick={addMedia} disabled={busy}>
              {busy ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              Add media
            </Button>
            <Button
              variant="default"
              size="sm"
              onClick={analyzeMaster}
              disabled={analyzing || !active.masterPath}
              title={
                active.masterPath
                  ? "Detect BPM, beats & sections"
                  : "Add the song master first"
              }
            >
              {analyzing ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <AudioWaveform className="h-4 w-4" />
              )}
              Analyze beats
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={syncPerformance}
              disabled={syncing || !active.masterPath}
              title="Align performance footage audio to the master so it lip-syncs"
            >
              {syncing ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <AudioLines className="h-4 w-4" />
              )}
              Sync performance
            </Button>
            <Button
              variant="default"
              size="sm"
              onClick={buildEdit}
              disabled={building || !analysis}
              title={
                analysis
                  ? "Cut footage to the beats & sections"
                  : "Analyze the master beats first"
              }
            >
              {building ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Scissors className="h-4 w-4" />
              )}
              Build edit
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={exportEdit}
              disabled={exporting || edl.length === 0}
              title={
                edl.length > 0
                  ? "Export a Premiere/FCP7 XML project"
                  : "Build the edit first"
              }
            >
              {exporting ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Download className="h-4 w-4" />
              )}
              Export XML
            </Button>
            <Button
              variant="success"
              size="sm"
              onClick={sendToDataset}
              disabled={datasetSending}
              title="Split this session's footage into short clips, identify each as performance / b-roll, caption them and add them to the training dataset"
            >
              {datasetSending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Database className="h-4 w-4" />
              )}
              Send to dataset
            </Button>
            <Button
              variant="default"
              size="sm"
              onClick={generateMv}
              disabled={mvGenerating || !analysis}
              title={
                analysis
                  ? "Generate a full AI music video from the song (our own image+audio workflow)"
                  : "Analyze the master beats first"
              }
            >
              {mvGenerating ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Clapperboard className="h-4 w-4" />
              )}
              Generate music video
            </Button>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => loadMedia(active.id)}
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
          </>
        )}
      </TopBar>

      <div className="flex min-h-0 flex-1">
        {/* Sessions sidebar */}
        <div className="flex min-h-0 w-[260px] shrink-0 flex-col border-r border-bds-border">
          <div className="shrink-0 space-y-2 border-b border-bds-border p-3">
            <div className="text-xs font-medium text-bds-muted">New session</div>
            <Input
              placeholder="Song / project name"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              className="h-8 text-sm"
            />
            <Input
              placeholder="Artist (optional)"
              value={newArtist}
              onChange={(e) => setNewArtist(e.target.value)}
              className="h-8 text-sm"
            />
            <div className="flex items-center gap-2">
              <span className="text-xs text-bds-muted">Timeline</span>
              <select
                value={newFps}
                onChange={(e) => setNewFps(Number(e.target.value))}
                className="h-8 flex-1 rounded-md border border-bds-border bg-bds-surface2 px-2 text-sm focus-ring cursor-pointer"
              >
                {SEQ_FPS_OPTIONS.map((f) => (
                  <option key={f} value={f}>
                    {fpsLabel(f)} fps
                  </option>
                ))}
              </select>
            </div>
            <Button
              variant="default"
              size="sm"
              className="w-full"
              onClick={createSession}
            >
              <Plus className="h-4 w-4" />
              Create session
            </Button>
          </div>

          <div className="flex-1 overflow-y-auto p-2">
            {sessions.length === 0 && (
              <div className="px-2 py-6 text-center text-xs text-bds-muted">
                No sessions yet
              </div>
            )}
            {sessions.map((s) => (
              <button
                key={s.id}
                onClick={() => setActiveId(s.id)}
                className={cn(
                  "group flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-sm cursor-pointer transition-colors",
                  s.id === activeId
                    ? "bg-bds-surface2 text-bds-fg"
                    : "text-bds-muted hover:bg-bds-surface2/60 hover:text-bds-fg"
                )}
              >
                <Clapperboard className="h-4 w-4 shrink-0 text-bds-accent" />
                <span className="min-w-0 flex-1 truncate">
                  {s.name}
                  <span className="block text-[10px] text-bds-muted">
                    {fpsLabel(s.sequenceFps)} fps · {s.status}
                  </span>
                </span>
                <Trash2
                  className="h-3.5 w-3.5 shrink-0 text-bds-muted opacity-0 transition-opacity hover:text-bds-bad group-hover:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation();
                    deleteSession(s.id);
                  }}
                />
              </button>
            ))}
          </div>
        </div>

        {/* Main board */}
        <div className="flex min-w-0 flex-1 flex-col overflow-y-auto">
          {!active ? (
            <div className="grid flex-1 place-items-center text-center text-bds-muted">
              <div>
                <Clapperboard className="mx-auto mb-3 h-10 w-10 opacity-40" />
                <p className="text-sm">
                  Create or pick a session to start editing
                </p>
              </div>
            </div>
          ) : (
            <>
              {progress && (
                <div className="flex items-center gap-2 border-b border-bds-border bg-bds-surface/60 px-4 py-2 text-xs text-bds-muted">
                  <Loader2 className="h-3.5 w-3.5 animate-spin text-bds-accent" />
                  <span className="capitalize">{progress.stage}</span>
                  <span className="truncate">· {progress.message}</span>
                  <span className="ml-auto tabular-nums">
                    {progress.processed}/{progress.total}
                  </span>
                </div>
              )}

              {syncProgress && (
                <div className="flex items-center gap-2 border-b border-bds-border bg-bds-surface/60 px-4 py-2 text-xs text-bds-muted">
                  <AudioLines className="h-3.5 w-3.5 animate-pulse text-bds-accent" />
                  <span className="truncate">{syncProgress.message}</span>
                  <span className="ml-auto tabular-nums">
                    {syncProgress.processed}/{syncProgress.total}
                  </span>
                </div>
              )}

              {datasetProgress && (
                <div className="flex items-center gap-2 border-b border-bds-border bg-bds-surface/60 px-4 py-2 text-xs text-bds-muted">
                  <Database className="h-3.5 w-3.5 animate-pulse text-bds-good" />
                  <span className="truncate">{datasetProgress.message}</span>
                  <span className="ml-auto tabular-nums">
                    {datasetProgress.processed}/{datasetProgress.total}
                  </span>
                </div>
              )}

              {mvProgress && (
                <div className="flex items-center gap-2 border-b border-bds-border bg-bds-surface/60 px-4 py-2 text-xs text-bds-muted">
                  <Clapperboard className="h-3.5 w-3.5 animate-pulse text-bds-accent" />
                  <span className="truncate">{mvProgress.message}</span>
                  <span className="ml-auto tabular-nums">
                    {mvProgress.processed}/{mvProgress.total}
                  </span>
                </div>
              )}

              {mvPath && (
                <div className="border-b border-bds-border bg-black/40 p-4">
                  <div className="mb-2 flex items-center gap-2 text-xs text-bds-muted">
                    <Clapperboard className="h-3.5 w-3.5 text-bds-accent" />
                    <span>AI music video</span>
                  </div>
                  <video
                    key={mvPath}
                    src={convertFileSrc(mvPath)}
                    controls
                    playsInline
                    className="w-full max-h-[60vh] rounded-lg bg-black"
                  />
                </div>
              )}

              {analysis && <AnalysisBar analysis={analysis} />}

              {edl.length > 0 && (
                <EdlPlayer
                  edl={edl}
                  media={media}
                  masterPath={active.masterPath ?? null}
                  registerSeek={(fn) => {
                    playerSeekRef.current = fn;
                  }}
                />
              )}

              {edl.length > 0 && (
                <EdlTimeline
                  edl={edl}
                  media={media}
                  onPreview={(src, title) => setPreview({ src, title })}
                  onSeek={(t) => playerSeekRef.current?.(t)}
                />
              )}

              {edl.length > 0 && profile && (
                <TrainingPanel
                  profile={profile}
                  onFeedback={sendFeedback}
                  busy={building}
                />
              )}

              {analysis && (
                <BrollStudio
                  style={style}
                  broll={broll}
                  count={brollCount}
                  animate={animateBroll}
                  busy={generatingBroll}
                  progress={brollProgress}
                  engine={engine}
                  onCount={setBrollCount}
                  onAnimate={setAnimateBroll}
                  onGenerate={generateBroll}
                  onRefreshStyle={refreshStyle}
                  onInsert={insertBroll}
                  onConvert={convertBroll}
                  onDelete={deleteBroll}
                  onPreview={(src, title) => setPreview({ src, title })}
                />
              )}

              {/* HuMo — image + audio → AI performance clip */}
              <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
                <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
                  <Badge variant="accent" className="gap-1">
                    <Sparkles className="h-3 w-3" />
                    HuMo AI Performance
                  </Badge>
                  {humoServer?.ok ? (
                    <Badge variant="info" className="text-[9px]">
                      H100 online{humoServer.busy ? " · busy" : ""}
                    </Badge>
                  ) : (
                    <Badge variant="default" className="text-[9px]">
                      server offline
                    </Badge>
                  )}
                  <span className="text-bds-muted">
                    Turn a photo + an audio clip into a lip-synced performance
                    shot (HuMo-17B on the H100), dropped into Performance media
                  </span>
                </div>

                <div className="grid gap-2 sm:grid-cols-2">
                  <button
                    onClick={pickHumoImage}
                    disabled={humoBusy}
                    className="flex items-center gap-2 rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-left text-[11px] text-bds-muted cursor-pointer hover:text-bds-fg disabled:opacity-50"
                  >
                    <FileVideo className="h-4 w-4 shrink-0" />
                    <span className="truncate">
                      {humoImage
                        ? humoImage.split(/[\\/]/).pop()
                        : "Pick reference image…"}
                    </span>
                  </button>
                  <button
                    onClick={pickHumoAudio}
                    disabled={humoBusy}
                    className="flex items-center gap-2 rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-left text-[11px] text-bds-muted cursor-pointer hover:text-bds-fg disabled:opacity-50"
                  >
                    <AudioLines className="h-4 w-4 shrink-0" />
                    <span className="truncate">
                      {humoAudio
                        ? humoAudio.split(/[\\/]/).pop()
                        : "Pick audio clip…"}
                    </span>
                  </button>
                </div>

                <input
                  type="text"
                  value={humoPrompt}
                  onChange={(e) => setHumoPrompt(e.target.value)}
                  disabled={humoBusy}
                  placeholder="Prompt (optional) — e.g. the artist singing to camera, moody stage lighting"
                  className="mt-2 w-full rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-[11px] text-bds-fg outline-none placeholder:text-bds-muted focus:border-bds-accent"
                />

                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <select
                    value={humoQuality}
                    onChange={(e) =>
                      setHumoQuality(
                        e.target.value as "fast" | "standard" | "high"
                      )
                    }
                    disabled={humoBusy}
                    className="rounded-md border border-bds-border bg-bds-surface2/40 px-2 py-1.5 text-[11px] text-bds-fg outline-none focus:border-bds-accent"
                  >
                    <option value="fast">Fast · 480p · 30 steps</option>
                    <option value="standard">Standard · 480p · 50 steps</option>
                    <option value="high">High · 720p · 50 steps</option>
                  </select>
                  <Button
                    variant="default"
                    size="sm"
                    onClick={generateHumo}
                    disabled={humoBusy || !humoImage || !humoAudio}
                  >
                    {humoBusy ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Sparkles className="h-4 w-4" />
                    )}
                    Generate AI performance
                  </Button>
                  {!humoServer?.ok && (
                    <span className="text-[10px] text-bds-muted">
                      Start the tunnel: brev port-forward boostify-wan -p
                      8000:8000
                    </span>
                  )}
                </div>

                {humoProgress && (
                  <div className="mt-2">
                    <div className="mb-1 flex items-center justify-between text-[10px] text-bds-muted">
                      <span className="truncate">{humoProgress.message}</span>
                      <span>
                        {Math.round((humoProgress.progress ?? 0) * 100)}%
                      </span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-bds-surface2">
                      <div
                        className="h-full rounded-full bg-bds-accent transition-all"
                        style={{
                          width: `${Math.round(
                            (humoProgress.progress ?? 0) * 100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>
                )}
              </div>

              {/* NVIDIA Relight — re-light a clip to match a 360 HDRI */}
              <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
                <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
                  <Badge variant="accent" className="gap-1">
                    <Sparkles className="h-3 w-3" />
                    Relight (HDRI)
                  </Badge>
                  {engine?.reachable ? (
                    <Badge variant="info" className="text-[9px]">
                      engine online
                    </Badge>
                  ) : (
                    <Badge variant="default" className="text-[9px]">
                      engine offline
                    </Badge>
                  )}
                  <span className="text-bds-muted">
                    Re-illuminate the people in a clip to match the lighting of a
                    360° HDRI environment map (NVIDIA Relight)
                  </span>
                </div>

                <div className="grid gap-2 sm:grid-cols-2">
                  <button
                    onClick={pickRelightVideo}
                    disabled={relightBusy}
                    className="flex items-center gap-2 rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-left text-[11px] text-bds-muted cursor-pointer hover:text-bds-fg disabled:opacity-50"
                  >
                    <FileVideo className="h-4 w-4 shrink-0" />
                    <span className="truncate">
                      {relightVideo
                        ? relightVideo.split(/[\\/]/).pop()
                        : "Pick clip to relight…"}
                    </span>
                  </button>
                  <button
                    onClick={pickRelightHdri}
                    disabled={relightBusy}
                    className="flex items-center gap-2 rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-left text-[11px] text-bds-muted cursor-pointer hover:text-bds-fg disabled:opacity-50"
                  >
                    <Sparkles className="h-4 w-4 shrink-0" />
                    <span className="truncate">
                      {relightHdri
                        ? relightHdri.split(/[\\/]/).pop()
                        : "Pick 360° HDRI map…"}
                    </span>
                  </button>
                </div>

                <div className="mt-2 flex flex-wrap items-center gap-3">
                  <label className="flex items-center gap-2 text-[11px] text-bds-muted">
                    Intensity
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.05}
                      value={relightIntensity}
                      onChange={(e) =>
                        setRelightIntensity(parseFloat(e.target.value))
                      }
                      disabled={relightBusy}
                      className="accent-bds-accent"
                    />
                    <span className="tabular-nums text-bds-fg">
                      {Math.round(relightIntensity * 100)}%
                    </span>
                  </label>
                  <Button
                    variant="default"
                    size="sm"
                    onClick={generateRelight}
                    disabled={relightBusy || !relightVideo || !relightHdri}
                  >
                    {relightBusy ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Sparkles className="h-4 w-4" />
                    )}
                    Relight clip
                  </Button>
                  {!engine?.reachable && (
                    <span className="text-[10px] text-bds-muted">
                      Set the AI Engine URL in Settings to relight clips
                    </span>
                  )}
                </div>

                {relightProgress && (
                  <div className="mt-2">
                    <div className="mb-1 flex items-center justify-between text-[10px] text-bds-muted">
                      <span className="truncate">{relightProgress.message}</span>
                      <span>
                        {Math.round((relightProgress.progress ?? 0) * 100)}%
                      </span>
                    </div>
                    <div className="h-1.5 w-full overflow-hidden rounded-full bg-bds-surface2">
                      <div
                        className="h-full rounded-full bg-bds-accent transition-all"
                        style={{
                          width: `${Math.round(
                            (relightProgress.progress ?? 0) * 100
                          )}%`,
                        }}
                      />
                    </div>
                  </div>
                )}
              </div>

              {/* ACE-Step — text → original music track */}
              <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
                <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
                  <Badge variant="accent" className="gap-1">
                    <Disc3 className="h-3 w-3" />
                    AI Music (ACE-Step)
                  </Badge>
                  {engine?.reachable ? (
                    <Badge variant="info" className="text-[9px]">
                      engine online · {engine.models.length} models
                    </Badge>
                  ) : (
                    <Badge variant="default" className="text-[9px]">
                      engine offline
                    </Badge>
                  )}
                  <span className="text-bds-muted">
                    Generate an original track from a text description (and
                    optional lyrics) with your installed ACE-Step model
                  </span>
                </div>

                <input
                  type="text"
                  value={musicPrompt}
                  onChange={(e) => setMusicPrompt(e.target.value)}
                  disabled={musicBusy}
                  placeholder="Describe the music — e.g. dark trap beat, 140 BPM, melancholic piano, heavy 808s"
                  className="w-full rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-[11px] text-bds-fg outline-none placeholder:text-bds-muted focus:border-bds-accent"
                />
                <textarea
                  value={musicLyrics}
                  onChange={(e) => setMusicLyrics(e.target.value)}
                  disabled={musicBusy}
                  rows={2}
                  placeholder="Lyrics (optional) — leave empty for an instrumental"
                  className="mt-2 w-full resize-y rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-[11px] text-bds-fg outline-none placeholder:text-bds-muted focus:border-bds-accent"
                />

                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <label className="flex items-center gap-1.5 text-[11px] text-bds-muted">
                    Length
                    <select
                      value={musicSeconds}
                      onChange={(e) => setMusicSeconds(Number(e.target.value))}
                      disabled={musicBusy}
                      className="rounded-md border border-bds-border bg-bds-surface2/40 px-2 py-1.5 text-[11px] text-bds-fg outline-none focus:border-bds-accent"
                    >
                      <option value={15}>15s</option>
                      <option value={30}>30s</option>
                      <option value={60}>60s</option>
                      <option value={120}>120s</option>
                      <option value={180}>180s</option>
                    </select>
                  </label>
                  <Button
                    variant="default"
                    size="sm"
                    onClick={generateMusic}
                    disabled={musicBusy || !musicPrompt.trim()}
                  >
                    {musicBusy ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Disc3 className="h-4 w-4" />
                    )}
                    Generate music
                  </Button>
                  {!engine?.configured && (
                    <span className="text-[10px] text-bds-muted">
                      Set the AI Engine URL in Settings to enable music.
                    </span>
                  )}
                </div>

                {musicPath && (
                  <audio
                    controls
                    src={convertFileSrc(musicPath)}
                    className="mt-2 h-9 w-full"
                  />
                )}
              </div>

              {/* FLUX.2 Klein — text → image + image editing (NVIDIA fallback) */}
              <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
                <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
                  <Badge variant="accent" className="gap-1">
                    <ImagePlus className="h-3 w-3" />
                    AI Image Studio
                  </Badge>
                  {engine?.reachable ? (
                    <Badge variant="info" className="text-[9px]">
                      engine online
                    </Badge>
                  ) : (
                    <Badge variant="default" className="text-[9px]">
                      NVIDIA FLUX.2 Klein fallback
                    </Badge>
                  )}
                  <span className="text-bds-muted">
                    Generate a new image from text or edit an existing one. Uses
                    your installed engine, or NVIDIA FLUX.2 Klein 4B in the cloud
                    when the engine (H200) is off.
                  </span>
                </div>

                <textarea
                  value={aiImgPrompt}
                  onChange={(e) => setAiImgPrompt(e.target.value)}
                  disabled={aiImgBusy}
                  rows={2}
                  placeholder="Describe the image (for editing: describe the change) — e.g. cinematic neon portrait, rain, shallow depth of field"
                  className="w-full resize-y rounded-md border border-bds-border bg-bds-surface2/40 px-2.5 py-2 text-[11px] text-bds-fg outline-none placeholder:text-bds-muted focus:border-bds-accent"
                />

                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Button
                    variant="default"
                    size="sm"
                    onClick={generateAiImage}
                    disabled={aiImgBusy || !aiImgPrompt.trim()}
                  >
                    {aiImgBusy ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <ImagePlus className="h-4 w-4" />
                    )}
                    Generate image
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={pickAiEditSrc}
                    disabled={aiImgBusy}
                  >
                    <Search className="h-4 w-4" />
                    {aiEditSrc ? "Change source" : "Pick image to edit"}
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={editAiImage}
                    disabled={aiImgBusy || (!aiEditSrc && !aiImgPath) || !aiImgPrompt.trim()}
                  >
                    <Wand2 className="h-4 w-4" />
                    Edit image
                  </Button>
                </div>

                {aiEditSrc && (
                  <p className="mt-1.5 truncate text-[10px] text-bds-muted">
                    Editing source: {aiEditSrc.split("/").pop()}
                  </p>
                )}

                {aiImgPath && (
                  <img
                    src={convertFileSrc(aiImgPath)}
                    alt="AI generated"
                    className="mt-2 max-h-72 w-full rounded-md border border-bds-border object-contain"
                  />
                )}
              </div>

              {media.length === 0 ? (
                <div className="grid flex-1 place-items-center text-center text-bds-muted">
                  <div>
                    <FileVideo className="mx-auto mb-3 h-10 w-10 opacity-40" />
                    <p className="text-sm">
                      Add the song master and your footage to begin
                    </p>
                    <Button
                      variant="default"
                      size="sm"
                      className="mt-4"
                      onClick={addMedia}
                      disabled={busy}
                    >
                      <Plus className="h-4 w-4" />
                      Add media
                    </Button>
                  </div>
                </div>
              ) : (
                <div className="grid grid-cols-1 gap-3 p-4 lg:grid-cols-2 xl:grid-cols-4">
                  {ROLE_COLUMNS.map((col) => (
                    <div
                      key={col.role}
                      className="flex max-h-[60vh] flex-col overflow-hidden rounded-lg border border-bds-border bg-bds-surface/30"
                      style={{ borderTop: `3px solid ${col.tint}` }}
                    >
                      <div className="flex items-center justify-between border-b border-bds-border px-3 py-2">
                        <div className="flex items-center gap-2">
                          <span
                            className="h-2.5 w-2.5 shrink-0 rounded-full"
                            style={{ backgroundColor: col.tint }}
                          />
                          <div>
                            <div className="text-sm font-medium">{col.label}</div>
                            <div className="text-[10px] text-bds-muted">
                              {col.hint}
                            </div>
                          </div>
                        </div>
                        <Badge variant="default">
                          {grouped[col.role].length}
                        </Badge>
                      </div>
                      <div className="flex-1 space-y-2 overflow-y-auto p-2">
                        {grouped[col.role].map((m) => (
                          <MediaCard
                            key={m.id}
                            media={m}
                            tint={col.tint}
                            onRole={(r) => changeRole(m, r)}
                            onRemove={() => removeMedia(m)}
                          />
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </div>
      </div>

      {preview && (
        <div
          className="fixed inset-0 z-[120] flex items-center justify-center bg-black/80 p-6 backdrop-blur-sm"
          onClick={() => setPreview(null)}
        >
          <div
            className="relative max-h-full max-w-5xl overflow-hidden rounded-lg border border-bds-border bg-bds-surface shadow-2xl"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between gap-3 border-b border-bds-border px-4 py-2">
              <span className="truncate text-xs text-bds-fg">
                {preview.title}
              </span>
              <button
                onClick={() => setPreview(null)}
                className="rounded-md p-1 text-bds-muted cursor-pointer hover:text-bds-fg"
                title="Close preview"
              >
                <X className="h-4 w-4" />
              </button>
            </div>
            <img
              src={preview.src}
              alt={preview.title}
              className="max-h-[78vh] w-auto object-contain"
            />
          </div>
        </div>
      )}
    </>
  );
}

function MediaCard({
  media: m,
  onRole,
  onRemove,
  tint,
}: {
  media: SessionMedia;
  onRole: (role: MediaRole) => void;
  onRemove: () => void;
  tint?: string;
}) {
  const thumb = m.thumbnailPath ? convertFileSrc(m.thumbnailPath) : null;
  return (
    <div
      className="group overflow-hidden rounded-md border border-bds-border bg-bds-surface2/60"
      style={tint ? { borderLeft: `3px solid ${tint}` } : undefined}
    >
      <div className="relative aspect-video w-full bg-black/40">
        {thumb ? (
          <img
            src={thumb}
            alt={m.filename}
            className="h-full w-full object-cover"
            loading="lazy"
          />
        ) : (
          <div className="grid h-full w-full place-items-center text-bds-muted">
            {m.kind === "audio" ? (
              <Music className="h-6 w-6" />
            ) : (
              <Film className="h-6 w-6" />
            )}
          </div>
        )}
        <button
          onClick={onRemove}
          className="absolute right-1 top-1 grid h-6 w-6 place-items-center rounded bg-black/60 text-white opacity-0 transition-opacity hover:bg-bds-bad group-hover:opacity-100 cursor-pointer"
          title="Remove"
        >
          <Trash2 className="h-3.5 w-3.5" />
        </button>
        {m.isSlowMo && (
          <span className="absolute left-1 top-1 inline-flex items-center gap-1 rounded bg-bds-info/80 px-1.5 py-0.5 text-[10px] font-medium text-black">
            <Gauge className="h-3 w-3" />
            Slow-mo {m.speedPct != null ? `${Math.round(m.speedPct)}%` : ""}
          </span>
        )}
        {m.role === "performance" && (m.syncConfidence ?? 0) >= 0.15 && (
          <span
            className="absolute bottom-1 left-1 inline-flex items-center gap-1 rounded bg-bds-accent/85 px-1.5 py-0.5 text-[10px] font-medium text-black"
            title={`Lip-synced to the master (offset ${
              m.audioOffset != null ? m.audioOffset.toFixed(1) : "?"
            }s, ${Math.round((m.syncConfidence ?? 0) * 100)}% match)`}
          >
            <AudioLines className="h-3 w-3" />
            Synced
          </span>
        )}
      </div>

      <div className="space-y-1.5 p-2">
        <div className="truncate text-xs font-medium" title={m.filename}>
          {m.filename}
        </div>
        <div className="flex flex-wrap items-center gap-1 text-[10px] text-bds-muted">
          {m.durationSeconds != null && (
            <span>{formatDuration(m.durationSeconds)}</span>
          )}
          {m.sourceFps != null && (
            <Badge variant="default">{fpsLabel(m.sourceFps)} fps</Badge>
          )}
          {m.width != null && m.height != null && (
            <span>
              {m.width}×{m.height}
            </span>
          )}
          {m.confidence != null && m.kind === "video" && (
            <Badge
              variant={
                m.confidence >= 0.7
                  ? "good"
                  : m.confidence >= 0.5
                    ? "warn"
                    : "bad"
              }
            >
              {Math.round(m.confidence * 100)}%
            </Badge>
          )}
        </div>
        {m.note && (
          <div className="truncate text-[10px] italic text-bds-muted" title={m.note}>
            {m.note}
          </div>
        )}
        {m.kind === "video" && (
          <select
            value={m.role}
            onChange={(e) => onRole(e.target.value as MediaRole)}
            className="h-7 w-full rounded border border-bds-border bg-bds-surface px-1.5 text-[11px] focus-ring cursor-pointer"
          >
            <option value="performance">Performance</option>
            <option value="story">Story</option>
            <option value="unsorted">Unsorted</option>
          </select>
        )}
      </div>
    </div>
  );
}

const SECTION_COLORS: Record<string, string> = {
  intro: "#3b82f6",
  low: "#6366f1",
  build: "#f59e0b",
  drop: "#ef4444",
  bridge: "#8b5cf6",
  outro: "#475569",
};

function AnalysisBar({ analysis }: { analysis: MasterAnalysis }) {
  const dur = analysis.duration || 1;
  return (
    <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs">
        <Badge variant="accent" className="gap-1">
          <AudioWaveform className="h-3 w-3" />
          {Math.round(analysis.bpm)} BPM
        </Badge>
        <span className="text-bds-muted">
          {analysis.beatCount} beats · {analysis.sections.length} sections ·{" "}
          {formatDuration(analysis.duration)}
        </span>
      </div>

      {/* Section timeline with beat ticks */}
      <div className="relative h-9 w-full overflow-hidden rounded-md border border-bds-border bg-black/30">
        {analysis.sections.map((s, i) => {
          const left = (s.start / dur) * 100;
          const width = ((s.end - s.start) / dur) * 100;
          const color = SECTION_COLORS[s.label] ?? "#6366f1";
          return (
            <div
              key={i}
              className="absolute top-0 flex h-full items-center justify-center overflow-hidden"
              style={{
                left: `${left}%`,
                width: `${width}%`,
                backgroundColor: color,
                opacity: 0.35 + s.energy * 0.55,
              }}
              title={`${s.label} · ${formatDuration(s.start)}–${formatDuration(
                s.end
              )} · energy ${Math.round(s.energy * 100)}%`}
            >
              {width > 6 && (
                <span className="truncate px-1 text-[10px] font-medium uppercase tracking-wide text-white/90">
                  {s.label}
                </span>
              )}
            </div>
          );
        })}
        {/* Beat ticks (downsampled so we don't draw thousands of nodes) */}
        {analysis.beats
          .filter((_, i) => i % 4 === 0)
          .map((t, i) => (
            <div
              key={`b${i}`}
              className="absolute top-0 h-full w-px bg-white/15"
              style={{ left: `${(t / dur) * 100}%` }}
            />
          ))}
      </div>
    </div>
  );
}

const ROLE_TINT: Record<string, string> = {
  performance: "#ec4899",
  story: "#22d3ee",
  extra: "#a3a3a3",
};

function EdlPlayer({
  edl,
  media,
  masterPath,
  registerSeek,
}: {
  edl: EditSegment[];
  media: SessionMedia[];
  masterPath: string | null;
  registerSeek?: (fn: (t: number) => void) => void;
}) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const rafRef = useRef<number | null>(null);
  const segIdxRef = useRef(-1);
  const curUrlRef = useRef<string | null>(null);
  const playingRef = useRef(false);
  const [playing, setPlaying] = useState(false);
  const [t, setT] = useState(0);

  const total = edl.length > 0 ? edl[edl.length - 1].timelineOut : 0;
  const masterUrl = masterPath ? convertFileSrc(masterPath) : null;
  const clipUrl = (id: number) => {
    const p = media.find((m) => m.id === id)?.path;
    return p ? convertFileSrc(p) : null;
  };

  const segAt = (time: number) => {
    if (edl.length === 0) return -1;
    for (let i = 0; i < edl.length; i++) {
      if (time >= edl[i].timelineIn && time < edl[i].timelineOut) return i;
    }
    return time >= total ? edl.length - 1 : 0;
  };

  const applySegment = (idx: number, time: number) => {
    const s = edl[idx];
    const v = videoRef.current;
    if (!s || !v) return;
    const url = clipUrl(s.mediaId);
    if (!url) return;
    const speed = (s.speedPct || 100) / 100;
    const into = Math.max(0, (time - s.timelineIn) * speed);
    const target = s.srcIn + into;
    segIdxRef.current = idx;
    const start = () => {
      try {
        v.currentTime = target;
      } catch {
        /* seek may fail before metadata */
      }
      v.playbackRate = speed;
      if (playingRef.current) v.play().catch(() => {});
    };
    if (curUrlRef.current !== url) {
      curUrlRef.current = url;
      v.src = url;
      const onload = () => {
        v.removeEventListener("loadeddata", onload);
        start();
      };
      v.addEventListener("loadeddata", onload);
      v.load();
    } else {
      start();
    }
  };

  const stopLoop = () => {
    if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
  };

  const tick = () => {
    const a = audioRef.current;
    if (!a) return;
    const time = a.currentTime;
    setT(time);
    const idx = segAt(time);
    if (idx !== segIdxRef.current) {
      applySegment(idx, time);
    } else {
      const s = edl[idx];
      const v = videoRef.current;
      if (s && v && !v.seeking && v.readyState >= 2) {
        const speed = (s.speedPct || 100) / 100;
        const expected = s.srcIn + (time - s.timelineIn) * speed;
        if (Math.abs(v.currentTime - expected) > 0.35) {
          try {
            v.currentTime = expected;
          } catch {
            /* ignore */
          }
        }
      }
    }
    if (a.ended || (total > 0 && time >= total)) {
      playingRef.current = false;
      setPlaying(false);
      a.pause();
      videoRef.current?.pause();
      stopLoop();
      return;
    }
    rafRef.current = requestAnimationFrame(tick);
  };

  const play = async () => {
    const a = audioRef.current;
    if (!a || edl.length === 0) return;
    if (total > 0 && a.currentTime >= total) a.currentTime = 0;
    playingRef.current = true;
    setPlaying(true);
    applySegment(segAt(a.currentTime), a.currentTime);
    try {
      await a.play();
    } catch {
      /* autoplay guard */
    }
    stopLoop();
    rafRef.current = requestAnimationFrame(tick);
  };

  const pause = () => {
    playingRef.current = false;
    setPlaying(false);
    audioRef.current?.pause();
    videoRef.current?.pause();
    stopLoop();
  };

  const seek = (time: number) => {
    const a = audioRef.current;
    if (!a) return;
    const clamped = Math.max(0, Math.min(time, total || 0));
    a.currentTime = clamped;
    setT(clamped);
    segIdxRef.current = -1;
    applySegment(segAt(clamped), clamped);
  };

  // Expose seek to the parent (so the filmstrip can jump the playhead).
  useEffect(() => {
    registerSeek?.(seek);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [registerSeek, edl, media, masterPath, total]);

  // Reset when a new edit is built.
  useEffect(() => {
    pause();
    segIdxRef.current = -1;
    curUrlRef.current = null;
    setT(0);
    const a = audioRef.current;
    if (a) a.currentTime = 0;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [edl]);

  // Cleanup on unmount.
  useEffect(() => () => stopLoop(), []);

  return (
    <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
      <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
        <Badge variant="accent" className="gap-1">
          <Play className="h-3 w-3" />
          Preview
        </Badge>
        <span className="text-bds-muted">
          {masterUrl
            ? "Reproduce la edición montada con la canción"
            : "Sin master de audio — solo vídeo"}
        </span>
      </div>

      <div className="relative mx-auto aspect-video w-full max-w-2xl overflow-hidden rounded-lg border border-bds-border bg-black">
        <video
          ref={videoRef}
          muted
          playsInline
          className="h-full w-full bg-black object-contain"
        />
        {!playing && (
          <button
            onClick={play}
            className="absolute inset-0 grid place-items-center bg-black/40 transition hover:bg-black/30"
            title="Reproducir edición"
          >
            <span className="grid h-16 w-16 place-items-center rounded-full bg-bds-accent/90 text-black shadow-lg">
              <Play className="h-7 w-7 translate-x-0.5 fill-current" />
            </span>
          </button>
        )}
        {masterUrl && <audio ref={audioRef} src={masterUrl} preload="auto" />}
      </div>

      <div className="mx-auto mt-2 flex w-full max-w-2xl items-center gap-3">
        <Button
          size="icon"
          variant="secondary"
          onClick={playing ? pause : play}
          title={playing ? "Pausar" : "Reproducir"}
        >
          {playing ? (
            <Pause className="h-4 w-4" />
          ) : (
            <Play className="h-4 w-4" />
          )}
        </Button>
        <span className="shrink-0 text-[11px] tabular-nums text-bds-muted">
          {formatDuration(t)} / {formatDuration(total)}
        </span>
        <input
          type="range"
          min={0}
          max={total || 0}
          step={0.05}
          value={Math.min(t, total || 0)}
          onChange={(e) => seek(Number(e.target.value))}
          className="h-1.5 flex-1 cursor-pointer accent-bds-accent"
        />
      </div>
    </div>
  );
}

function EdlTimeline({
  edl,
  media,
  onPreview,
  onSeek,
}: {
  edl: EditSegment[];
  media: SessionMedia[];
  onPreview?: (src: string, title: string) => void;
  onSeek?: (t: number) => void;
}) {
  const total =
    edl.length > 0 ? edl[edl.length - 1].timelineOut || 1 : 1;
  const mediaOf = (id: number) => media.find((m) => m.id === id);
  const nameOf = (id: number) => mediaOf(id)?.filename ?? `media ${id}`;
  const thumbOf = (id: number) => {
    const t = mediaOf(id)?.thumbnailPath;
    return t ? convertFileSrc(t) : null;
  };
  const slowCount = edl.filter((s) => s.speedPct < 99).length;
  const brollCount = edl.filter((s) =>
    (s.reason ?? "").includes("ai b-roll")
  ).length;

  const roleOf = (s: EditSegment) => {
    const r = s.reason ?? "";
    if (r.includes("ai b-roll")) return "broll";
    if (r.includes("performance")) return "performance";
    if (r.includes("story")) return "story";
    return "extra";
  };
  const colorOf = (role: string) =>
    role === "broll"
      ? "#c084fc"
      : ROLE_TINT[role as keyof typeof ROLE_TINT] ?? "#a3a3a3";

  return (
    <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
      <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
        <Badge variant="accent" className="gap-1">
          <Scissors className="h-3 w-3" />
          {edl.length} cuts
        </Badge>
        <span className="text-bds-muted">
          {formatDuration(total)} timeline
          {slowCount > 0 && ` · ${slowCount} slow-mo`}
          {brollCount > 0 && ` · ${brollCount} AI b-roll`}
        </span>
      </div>

      {/* Proportional cut ribbon (overview) */}
      <div className="relative mb-2 flex h-3 w-full overflow-hidden rounded-md border border-bds-border bg-black/30">
        {edl.map((s) => {
          const width = ((s.timelineOut - s.timelineIn) / total) * 100;
          const role = roleOf(s);
          return (
            <div
              key={s.id}
              className="h-full border-r border-black/40"
              style={{
                width: `${width}%`,
                backgroundColor: colorOf(role),
                opacity: role === "broll" ? 0.9 : 0.55,
              }}
              title={`#${s.orderIndex + 1} · ${nameOf(s.mediaId)}`}
            />
          );
        })}
      </div>

      {/* Professional filmstrip — thumbnails of every clip in order */}
      <div className="flex gap-1.5 overflow-x-auto pb-1">
        {edl.map((s) => {
          const role = roleOf(s);
          const thumb = thumbOf(s.mediaId);
          const dur = s.timelineOut - s.timelineIn;
          const isBroll = role === "broll";
          return (
            <div
              key={s.id}
              onClick={() => onSeek?.(s.timelineIn)}
              className={cn(
                "group relative h-16 w-24 shrink-0 cursor-pointer overflow-hidden rounded-md border bg-black/40 text-left transition hover:ring-2 hover:ring-bds-accent"
              )}
              style={{ borderColor: colorOf(role), borderWidth: isBroll ? 2 : 1 }}
              title={`#${s.orderIndex + 1} · ${nameOf(s.mediaId)}\n${
                s.reason ?? ""
              }\n${formatDuration(s.timelineIn)}–${formatDuration(
                s.timelineOut
              )} (${dur.toFixed(1)}s) · click para reproducir desde aquí`}
            >
              {thumb ? (
                <img
                  src={thumb}
                  alt={nameOf(s.mediaId)}
                  className="h-full w-full object-cover"
                  loading="lazy"
                />
              ) : (
                <div className="grid h-full w-full place-items-center text-bds-muted">
                  <Film className="h-4 w-4" />
                </div>
              )}
              {/* play-from-here hint */}
              <span className="absolute inset-0 hidden place-items-center bg-black/30 group-hover:grid">
                <Play className="h-5 w-5 fill-white text-white" />
              </span>
              {/* preview still */}
              {thumb && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    onPreview?.(thumb, nameOf(s.mediaId));
                  }}
                  className="absolute bottom-0.5 left-1 z-10 hidden rounded bg-black/70 p-0.5 text-white group-hover:block"
                  title="Ver fotograma"
                >
                  <Search className="h-3 w-3" />
                </button>
              )}
              {/* index */}
              <span className="absolute left-1 top-1 rounded bg-black/70 px-1 text-[9px] tabular-nums text-white">
                {s.orderIndex + 1}
              </span>
              {/* AI b-roll marker */}
              {isBroll && (
                <span className="absolute right-1 top-1 rounded bg-[#c084fc] px-1 text-[8px] font-medium text-black">
                  AI
                </span>
              )}
              {/* slow-mo strip */}
              {s.speedPct < 99 && (
                <span className="absolute inset-x-0 bottom-0 h-1 bg-bds-info" />
              )}
              {/* duration */}
              <span className="absolute bottom-0.5 right-1 rounded bg-black/70 px-1 text-[8px] tabular-nums text-white">
                {dur.toFixed(1)}s
              </span>
            </div>
          );
        })}
      </div>

      {/* Legend */}
      <div className="mt-2 flex flex-wrap items-center gap-3 text-[10px] text-bds-muted">
        <span className="flex items-center gap-1">
          <span
            className="h-2 w-2 rounded-sm"
            style={{ backgroundColor: ROLE_TINT.performance }}
          />
          Performance
        </span>
        <span className="flex items-center gap-1">
          <span
            className="h-2 w-2 rounded-sm"
            style={{ backgroundColor: ROLE_TINT.story }}
          />
          Story
        </span>
        <span className="flex items-center gap-1">
          <span className="h-2 w-2 rounded-sm bg-[#c084fc]" />
          AI b-roll
        </span>
        <span className="flex items-center gap-1">
          <span className="h-2 w-2 rounded-sm bg-bds-info" />
          Slow-mo conform
        </span>
      </div>
    </div>
  );
}

type FeedbackPair = {
  label: string;
  value: number; // 0..1 (or normalized) for the meter
  less: EditFeedback;
  more: EditFeedback;
  lessLabel: string;
  moreLabel: string;
};

function TrainingPanel({
  profile,
  onFeedback,
  busy,
}: {
  profile: EditProfile;
  onFeedback: (fb: EditFeedback, rebuild: boolean) => void;
  busy: boolean;
}) {
  const pairs: FeedbackPair[] = [
    {
      label: "Cut speed",
      // cadence 0.4 (fast) .. 2 (slow) → invert so right = faster
      value: 1 - (profile.cadence - 0.4) / 1.6,
      less: "slower",
      more: "faster",
      lessLabel: "Slower",
      moreLabel: "Faster",
    },
    {
      label: "Performance vs story",
      value: profile.performanceBias,
      less: "more_story",
      more: "more_performance",
      lessLabel: "Story",
      moreLabel: "Perform",
    },
    {
      label: "B-roll cutaways",
      value: profile.brollFreq,
      less: "less_broll",
      more: "more_broll",
      lessLabel: "Less",
      moreLabel: "More",
    },
    {
      label: "Slow-mo feel",
      value: profile.slowmoAffinity,
      less: "less_slowmo",
      more: "more_slowmo",
      lessLabel: "Less",
      moreLabel: "More",
    },
    {
      label: "Camera variation",
      value: profile.variation,
      less: "less_variation",
      more: "more_variation",
      lessLabel: "Less",
      moreLabel: "More",
    },
  ];

  return (
    <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
      <div className="mb-2 flex items-center gap-2 text-xs">
        <Badge variant="info" className="gap-1">
          <Sparkles className="h-3 w-3" />
          Train the editor
        </Badge>
        <span className="text-bds-muted">
          Nudge a control to re-cut — the engine learns ({profile.samples}{" "}
          tweaks so far)
        </span>
      </div>

      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {pairs.map((p) => (
          <div
            key={p.label}
            className="rounded-md border border-bds-border bg-bds-surface2/40 p-2"
          >
            <div className="mb-1 flex items-center justify-between text-[11px] text-bds-muted">
              <span>{p.label}</span>
            </div>
            <div className="mb-1.5 h-1.5 w-full overflow-hidden rounded-full bg-black/30">
              <div
                className="h-full rounded-full bg-bds-accent transition-all"
                style={{
                  width: `${Math.round(
                    Math.min(1, Math.max(0, p.value)) * 100
                  )}%`,
                }}
              />
            </div>
            <div className="flex gap-1.5">
              <Button
                variant="secondary"
                size="sm"
                className="h-6 flex-1 text-[11px]"
                disabled={busy}
                onClick={() => onFeedback(p.less, true)}
              >
                {p.lessLabel}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                className="h-6 flex-1 text-[11px]"
                disabled={busy}
                onClick={() => onFeedback(p.more, true)}
              >
                {p.moreLabel}
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

const BROLL_STATUS: Record<
  BrollCandidate["status"],
  { label: string; variant: "default" | "accent" | "good" | "warn" | "bad" | "info" }
> = {
  planned: { label: "Planned", variant: "warn" },
  image: { label: "Image", variant: "info" },
  video: { label: "Video", variant: "good" },
  inserted: { label: "Inserted", variant: "accent" },
  failed: { label: "Failed", variant: "bad" },
};

function BrollStudio({
  style,
  broll,
  count,
  animate,
  busy,
  progress,
  engine,
  onCount,
  onAnimate,
  onGenerate,
  onRefreshStyle,
  onInsert,
  onConvert,
  onDelete,
  onPreview,
}: {
  style: StyleReference | null;
  broll: BrollCandidate[];
  count: number;
  animate: boolean;
  busy: boolean;
  progress: BrollProgress | null;
  engine: EngineStatus | null;
  onCount: (n: number) => void;
  onAnimate: (v: boolean) => void;
  onGenerate: () => void;
  onRefreshStyle: () => void;
  onInsert: (c: BrollCandidate) => void;
  onConvert: (c: BrollCandidate) => void;
  onDelete: (c: BrollCandidate) => void;
  onPreview?: (src: string, title: string) => void;
}) {
  const [collapsed, setCollapsed] = useState(false);
  const ready = broll.filter(
    (b) => b.status === "image" || b.status === "video"
  ).length;
  return (
    <div className="border-b border-bds-border bg-bds-surface/40 px-4 py-3">
      <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
        <button
          onClick={() => setCollapsed((v) => !v)}
          title={collapsed ? "Expand B-roll studio" : "Collapse B-roll studio"}
          className="flex items-center gap-1.5 rounded-md px-1 py-0.5 text-bds-muted cursor-pointer hover:text-bds-fg"
        >
          {collapsed ? (
            <ChevronRight className="h-3.5 w-3.5" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5" />
          )}
        </button>
        <Badge variant="accent" className="gap-1">
          <Wand2 className="h-3 w-3" />
          AI B-roll Studio
        </Badge>
        {broll.length > 0 && (
          <Badge variant="info" className="text-[9px]">
            {ready}/{broll.length} ready
          </Badge>
        )}
        {engine?.reachable && (
          <Badge variant="info" className="gap-1 text-[9px]">
            <Boxes className="h-3 w-3" />
            installed models
          </Badge>
        )}
        <span className="text-bds-muted">
          {engine?.reachable
            ? "Style-matched shots from your installed FLUX/Qwen image models, animated with LTX/Wan when a video model is set"
            : "Style-matched shots generated with NVIDIA models, inserted where the cut needs a cutaway"}
        </span>
      </div>

      {!collapsed && (
        <>
          {/* Captured style */}
      <div className="mb-3 flex flex-wrap items-center gap-3 rounded-md border border-bds-border bg-bds-surface2/40 p-2">
        <button
          onClick={onRefreshStyle}
          title="Re-capture the footage style"
          className="flex items-center gap-1.5 rounded-md px-1.5 py-1 text-[11px] text-bds-muted cursor-pointer hover:text-bds-fg"
        >
          <Palette className="h-3.5 w-3.5" />
          Style
        </button>
        {style && style.palette.length > 0 ? (
          <div className="flex items-center gap-1">
            {style.palette.slice(0, 6).map((c) => (
              <span
                key={c}
                className="h-4 w-4 rounded-sm border border-black/30"
                style={{ backgroundColor: c }}
                title={c}
              />
            ))}
          </div>
        ) : (
          <span className="text-[11px] text-bds-muted">No style captured yet</span>
        )}
        {style?.descriptor && (
          <span className="min-w-0 flex-1 truncate text-[11px] text-bds-muted">
            {style.descriptor}
          </span>
        )}
      </div>

      {/* Controls */}
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className="text-[11px] text-bds-muted">Shots</span>
        <select
          value={count}
          onChange={(e) => onCount(Number(e.target.value))}
          disabled={busy}
          className="h-7 rounded-md border border-bds-border bg-bds-surface2 px-2 text-xs focus-ring cursor-pointer"
        >
          {[2, 3, 4, 6, 8].map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
        <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-bds-muted">
          <input
            type="checkbox"
            checked={animate}
            disabled={busy}
            onChange={(e) => onAnimate(e.target.checked)}
            className="cursor-pointer accent-bds-accent"
          />
          Animate to video
        </label>
        <Button
          variant="default"
          size="sm"
          className="h-7"
          disabled={busy}
          onClick={onGenerate}
        >
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ImagePlus className="h-3.5 w-3.5" />
          )}
          Generate B-roll
        </Button>
        {progress && (
          <span className="flex items-center gap-1.5 text-[11px] text-bds-muted">
            <Loader2 className="h-3 w-3 animate-spin text-bds-accent" />
            <span className="truncate">{progress.message}</span>
            <span className="tabular-nums">
              {progress.processed}/{progress.total}
            </span>
          </span>
        )}
      </div>

      {/* Gallery */}
      {broll.length > 0 && (
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5">
          {broll.map((c) => {
            const st = BROLL_STATUS[c.status];
            const thumb = c.thumbnailPath
              ? convertFileSrc(c.thumbnailPath)
              : c.imagePath
              ? convertFileSrc(c.imagePath)
              : null;
            const insertable = c.status === "video" || c.status === "image";
            const previewSrc = c.imagePath
              ? convertFileSrc(c.imagePath)
              : thumb;
            return (
              <div
                key={c.id}
                className="group overflow-hidden rounded-md border border-bds-border bg-bds-surface2/60"
              >
                <div className="relative aspect-video w-full bg-black/40">
                  {thumb ? (
                    <button
                      type="button"
                      onClick={() =>
                        previewSrc && onPreview?.(previewSrc, c.idea)
                      }
                      className="block h-full w-full cursor-pointer"
                      title="Click to preview"
                    >
                      <img
                        src={thumb}
                        alt={c.idea}
                        className="h-full w-full object-cover transition group-hover:scale-[1.03]"
                        loading="lazy"
                      />
                    </button>
                  ) : (
                    <div className="grid h-full w-full place-items-center text-bds-muted">
                      <Wand2 className="h-5 w-5" />
                    </div>
                  )}
                  <div className="absolute left-1 top-1">
                    <Badge variant={st.variant} className="text-[9px]">
                      {st.label}
                    </Badge>
                  </div>
                  <div className="absolute right-1 top-1">
                    <Badge variant="default" className="text-[9px] capitalize">
                      {c.section}
                    </Badge>
                  </div>
                </div>
                <div className="space-y-1.5 p-2">
                  <div
                    className="truncate text-[11px] text-bds-fg"
                    title={c.idea}
                  >
                    {c.idea}
                  </div>
                  {c.note && (
                    <div
                      className="truncate text-[10px] text-bds-muted"
                      title={c.note}
                    >
                      {c.note}
                    </div>
                  )}
                  <div className="flex gap-1.5">
                    <Button
                      variant="secondary"
                      size="sm"
                      className="h-6 flex-1 text-[10px]"
                      disabled={!insertable || busy}
                      title={
                        c.status === "video"
                          ? "Insert as story footage & re-cut"
                          : c.status === "image"
                          ? "Animate locally & insert as story footage"
                          : "Generate this shot first"
                      }
                      onClick={() => onInsert(c)}
                    >
                      Insert
                    </Button>
                    {c.status === "image" && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-6 px-2 text-[10px]"
                        disabled={busy}
                        title="Animate this still into a motion video clip"
                        onClick={() => onConvert(c)}
                      >
                        <Film className="h-3 w-3" />
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-6 px-2 text-[10px]"
                      onClick={() => onDelete(c)}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
        </>
      )}
    </div>
  );
}

