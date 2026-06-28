import { useCallback, useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input, Textarea } from "@/components/ui/controls";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { Clip } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import { cn, formatDuration } from "@/lib/utils";
import {
  Check,
  X,
  RefreshCw,
  Images,
  Star,
  Tag,
  Save,
  Film,
  CheckSquare,
  Square,
} from "lucide-react";

function scoreTone(score: number | null) {
  if (score == null) return "default" as const;
  if (score >= 75) return "good" as const;
  if (score >= 50) return "warn" as const;
  return "bad" as const;
}

export function Review({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [clips, setClips] = useState<Clip[]>([]);
  const [selected, setSelected] = useState<Clip | null>(null);
  const [caption, setCaption] = useState("");
  const [tagInput, setTagInput] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [threshold, setThreshold] = useState(60);

  const refresh = useCallback(async () => {
    try {
      const c = await api.listClips({ limit: 300 });
      setClips(c);
      setSelected((prev) =>
        prev ? c.find((x) => x.id === prev.id) ?? c[0] ?? null : c[0] ?? null
      );
    } catch {
      /* backend not ready */
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    api
      .getSettings()
      .then((s) => setThreshold(s.qualityThreshold ?? 60))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (selected) {
      setCaption(selected.caption ?? "");
      setTagInput(selected.tags.join(", "));
    }
  }, [selected]);

  const approve = async (clip: Clip, approved: boolean) => {
    try {
      await api.setClipApproval(clip.id, approved);
      setClips((cs) =>
        cs.map((c) =>
          c.id === clip.id
            ? { ...c, approved, status: approved ? "approved" : "rejected" }
            : c
        )
      );
      if (selected?.id === clip.id)
        setSelected({ ...clip, approved, status: approved ? "approved" : "rejected" });
      push(approved ? "success" : "info", approved ? "Clip approved" : "Clip rejected");
    } catch (e) {
      push("error", String(e));
    }
  };

  const toggleSelect = (id: number) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectAll = () => setSelectedIds(new Set(clips.map((c) => c.id)));
  const selectPending = () =>
    setSelectedIds(new Set(clips.filter((c) => c.approved == null).map((c) => c.id)));
  const clearSelection = () => setSelectedIds(new Set());

  // Training score for a clip: trainingValue when available, else qualityScore.
  const clipScore = (c: Clip) => c.trainingValue ?? c.qualityScore ?? 0;
  const qualifyingCount = clips.filter((c) => clipScore(c) >= threshold).length;
  const selectQualifying = () => {
    const ids = clips.filter((c) => clipScore(c) >= threshold).map((c) => c.id);
    setSelectedIds(new Set(ids));
    if (ids.length === 0) push("info", "No clips reach that score yet");
  };

  const bulkApprove = async (approved: boolean) => {
    const ids = [...selectedIds];
    if (ids.length === 0) return;
    try {
      const n = await api.setClipsApproval(ids, approved);
      const status = approved ? "approved" : "rejected";
      setClips((cs) =>
        cs.map((c) =>
          selectedIds.has(c.id) ? { ...c, approved, status } : c
        )
      );
      setSelected((prev) =>
        prev && selectedIds.has(prev.id) ? { ...prev, approved, status } : prev
      );
      clearSelection();
      push(
        approved ? "success" : "info",
        `${n} clip${n === 1 ? "" : "s"} ${approved ? "approved" : "rejected"}`
      );
    } catch (e) {
      push("error", String(e));
    }
  };

  const saveCaption = async () => {
    if (!selected) return;
    try {
      const tags = tagInput
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      await api.updateClipCaption(selected.id, caption);
      await api.updateClipTags(selected.id, tags);
      setClips((cs) =>
        cs.map((c) => (c.id === selected.id ? { ...c, caption, tags } : c))
      );
      push("success", "Saved");
    } catch (e) {
      push("error", String(e));
    }
  };

  return (
    <>
      <TopBar
        title="Review"
        subtitle="Lightroom-style clip review & captioning"
        gpu={gpu}
        deps={deps}
      >
        <div className="flex items-center gap-1.5 rounded-md border border-bds-border bg-bds-surface2 px-2 py-1">
          <span className="text-xs text-bds-muted">≥</span>
          <Input
            type="number"
            min={0}
            max={100}
            value={threshold}
            onChange={(e) =>
              setThreshold(
                Math.max(0, Math.min(100, Number(e.target.value) || 0))
              )
            }
            className="h-7 w-14 px-1.5 text-center text-xs"
          />
          <span className="text-xs text-bds-muted">%</span>
          <Button variant="default" size="sm" onClick={selectQualifying}>
            <Star className="h-4 w-4" />
            Select qualifying ({qualifyingCount})
          </Button>
        </div>
        <Button variant="secondary" size="sm" onClick={selectPending}>
          <CheckSquare className="h-4 w-4" />
          Select pending
        </Button>
        <Button variant="secondary" size="sm" onClick={selectAll}>
          <CheckSquare className="h-4 w-4" />
          Select all
        </Button>
        <Button variant="secondary" size="sm" onClick={refresh}>
          <RefreshCw className="h-4 w-4" />
          Refresh
        </Button>
      </TopBar>

      <div className="flex min-h-0 flex-1">
        {/* Grid */}
        <div className="min-w-0 flex-1 overflow-y-auto p-5">
          {selectedIds.size > 0 && (
            <div className="sticky top-0 z-10 mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-bds-accent/40 bg-bds-surface/95 px-3 py-2 shadow-lg backdrop-blur animate-fade-in">
              <span className="text-sm font-medium text-bds-fg">
                {selectedIds.size} selected
              </span>
              <div className="ml-auto flex items-center gap-2">
                <Button
                  variant="success"
                  size="sm"
                  onClick={() => bulkApprove(true)}
                >
                  <Check className="h-4 w-4" />
                  Approve selected
                </Button>
                <Button
                  variant="danger"
                  size="sm"
                  onClick={() => bulkApprove(false)}
                >
                  <X className="h-4 w-4" />
                  Reject selected
                </Button>
                <Button variant="ghost" size="sm" onClick={clearSelection}>
                  Clear
                </Button>
              </div>
            </div>
          )}
          {clips.length === 0 ? (
            <div className="flex h-[60vh] flex-col items-center justify-center text-center">
              <Images className="h-10 w-10 text-bds-muted" />
              <p className="mt-3 text-sm text-bds-muted">
                No clips yet. Run the pipeline to generate clips.
              </p>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
              {clips.map((clip) => (
                <button
                  key={clip.id}
                  onClick={() => setSelected(clip)}
                  className={cn(
                    "group relative aspect-video overflow-hidden rounded-lg border bg-bds-surface2 text-left transition-all focus-ring",
                    selected?.id === clip.id
                      ? "border-bds-accent ring-2 ring-bds-accent/40"
                      : "border-bds-border hover:border-bds-accent/40",
                    selectedIds.has(clip.id) && "ring-2 ring-bds-accent",
                    clipScore(clip) >= threshold &&
                      !selectedIds.has(clip.id) &&
                      selected?.id !== clip.id &&
                      "ring-1 ring-bds-good/70"
                  )}
                >
                  {clip.thumbnailPath ? (
                    <img
                      src={convertFileSrc(clip.thumbnailPath)}
                      alt=""
                      className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
                    />
                  ) : (
                    <div className="grid h-full place-items-center">
                      <Film className="h-6 w-6 text-bds-muted" />
                    </div>
                  )}
                  <span
                    role="checkbox"
                    aria-checked={selectedIds.has(clip.id)}
                    aria-label="Select clip"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleSelect(clip.id);
                    }}
                    className={cn(
                      "absolute left-1.5 top-1.5 grid h-6 w-6 cursor-pointer place-items-center rounded-md border transition-colors",
                      selectedIds.has(clip.id)
                        ? "border-bds-accent bg-bds-accent text-black"
                        : "border-white/40 bg-black/40 text-white/80 opacity-0 group-hover:opacity-100"
                    )}
                  >
                    {selectedIds.has(clip.id) ? (
                      <CheckSquare className="h-4 w-4" />
                    ) : (
                      <Square className="h-4 w-4" />
                    )}
                  </span>
                  <div className="absolute inset-x-0 bottom-0 flex items-center justify-between bg-gradient-to-t from-black/80 to-transparent px-2 py-1.5">
                    <span className="flex items-center gap-1 text-[10px] text-white/80">
                      {clipScore(clip) >= threshold && (
                        <Star className="h-3 w-3 fill-bds-good text-bds-good" />
                      )}
                      {formatDuration(clip.durationSeconds)}
                    </span>
                    {clip.qualityScore != null && (
                      <Badge variant={scoreTone(clip.qualityScore)}>
                        <Star className="h-2.5 w-2.5" />
                        {Math.round(clip.qualityScore)}
                      </Badge>
                    )}
                  </div>
                  {clip.approved === true && (
                    <div className="absolute right-1.5 top-1.5 grid h-5 w-5 place-items-center rounded-full bg-bds-good text-black">
                      <Check className="h-3 w-3" />
                    </div>
                  )}
                  {clip.approved === false && (
                    <div className="absolute right-1.5 top-1.5 grid h-5 w-5 place-items-center rounded-full bg-bds-bad text-white">
                      <X className="h-3 w-3" />
                    </div>
                  )}
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Detail panel */}
        {selected && (
          <aside className="flex w-[360px] shrink-0 flex-col overflow-y-auto border-l border-bds-border bg-bds-surface/40 p-5 animate-fade-in">
            <div className="aspect-video overflow-hidden rounded-lg bg-bds-surface2">
              {selected.thumbnailPath ? (
                <img
                  src={convertFileSrc(selected.thumbnailPath)}
                  alt=""
                  className="h-full w-full object-cover"
                />
              ) : (
                <div className="grid h-full place-items-center">
                  <Film className="h-8 w-8 text-bds-muted" />
                </div>
              )}
            </div>

            <div className="mt-3 flex items-center gap-2">
              <Button
                variant="success"
                size="sm"
                className="flex-1"
                onClick={() => approve(selected, true)}
              >
                <Check className="h-4 w-4" />
                Approve
              </Button>
              <Button
                variant="danger"
                size="sm"
                className="flex-1"
                onClick={() => approve(selected, false)}
              >
                <X className="h-4 w-4" />
                Reject
              </Button>
            </div>

            <div className="mt-4 space-y-1">
              <label className="text-xs font-medium text-bds-muted">Caption</label>
              <Textarea
                rows={5}
                value={caption}
                onChange={(e) => setCaption(e.target.value)}
                placeholder="AI-generated training caption…"
              />
            </div>

            <div className="mt-3 space-y-1">
              <label className="flex items-center gap-1 text-xs font-medium text-bds-muted">
                <Tag className="h-3 w-3" />
                Tags (comma separated)
              </label>
              <Input
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                placeholder="singing, night, studio, close up"
              />
            </div>

            <Button className="mt-3" size="sm" onClick={saveCaption}>
              <Save className="h-4 w-4" />
              Save changes
            </Button>

            {selected.analysis && (
              <div className="mt-5">
                <div className="mb-2 text-xs font-medium text-bds-muted">
                  Scene analysis
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {Object.entries(selected.analysis)
                    .filter(([, v]) => v != null && v !== "")
                    .map(([k, v]) => (
                      <Badge key={k} variant="default">
                        {k}: {Array.isArray(v) ? v.join("/") : String(v)}
                      </Badge>
                    ))}
                </div>
              </div>
            )}

            <div className="mt-5 space-y-1.5 border-t border-bds-border pt-4 text-xs text-bds-muted">
              <Row label="Range">
                {formatDuration(selected.startSeconds)} –{" "}
                {formatDuration(selected.endSeconds)}
              </Row>
              <Row label="Quality">
                {selected.qualityScore?.toFixed(0) ?? "—"}
              </Row>
              <Row label="Training value">
                {selected.trainingValue?.toFixed(0) ?? "—"}
              </Row>
              <Row label="Status">{selected.status}</Row>
            </div>
          </aside>
        )}
      </div>
    </>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex justify-between">
      <span>{label}</span>
      <span className="text-bds-fg">{children}</span>
    </div>
  );
}
