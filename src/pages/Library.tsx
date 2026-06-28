import { useCallback, useEffect, useRef, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Progress } from "@/components/ui/controls";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { ScanProgress, VideoFile } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import { formatBytes, formatDuration, formatNumber } from "@/lib/utils";
import {
  FolderOpen,
  Play,
  RefreshCw,
  HardDriveDownload,
  FileVideo,
  Trash2,
} from "lucide-react";

const STATUS_VARIANT: Record<string, "default" | "good" | "warn" | "bad" | "info" | "accent"> = {
  discovered: "default",
  indexed: "info",
  splitting: "warn",
  analyzing: "warn",
  scored: "accent",
  approved: "good",
  rejected: "bad",
  duplicate: "warn",
  error: "bad",
};

export function Library({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [videos, setVideos] = useState<VideoFile[]>([]);
  const [scan, setScan] = useState<ScanProgress | null>(null);
  const [scanning, setScanning] = useState(false);
  const [folder, setFolder] = useState<string | null>(null);
  const unlisten = useRef<(() => void) | null>(null);

  const refresh = useCallback(async () => {
    try {
      setVideos(await api.listVideos(800, 0));
    } catch {
      /* backend not ready */
    }
  }, []);

  useEffect(() => {
    refresh();
    api
      .onScanProgress((p) => {
        setScan(p);
        if (p.done) {
          setScanning(false);
          refresh();
          push("success", `Indexed ${formatNumber(p.filesIndexed)} videos`);
        }
      })
      .then((fn) => (unlisten.current = fn))
      .catch(() => {});
    return () => unlisten.current?.();
  }, [refresh, push]);

  const handlePick = async () => {
    try {
      const dir = await api.pickFolder();
      if (!dir) return;
      setFolder(dir);
      setScanning(true);
      setScan({
        phase: "scanning",
        filesDiscovered: 0,
        filesIndexed: 0,
        filesSkipped: 0,
        currentPath: null,
        done: false,
      });
      await api.scanFolder(dir);
    } catch (e) {
      setScanning(false);
      push("error", `Scan failed: ${String(e)}`);
    }
  };

  const handleProcessAll = async () => {
    try {
      await api.processAllPending();
      push("info", "Processing queued for all pending videos");
    } catch (e) {
      push("error", `Could not start: ${String(e)}`);
    }
  };

  const errorCount = videos.filter((v) => v.status === "error").length;

  const handleDeleteOne = async (v: VideoFile) => {
    try {
      await api.deleteVideos([v.id]);
      setVideos((prev) => prev.filter((x) => x.id !== v.id));
      push("success", `Removed ${v.filename} from library`);
    } catch (e) {
      push("error", `Could not remove: ${String(e)}`);
    }
  };

  const handleDeleteErrored = async () => {
    if (errorCount === 0) return;
    const ok = window.confirm(
      `Remove ${errorCount} errored ${
        errorCount === 1 ? "file" : "files"
      } from the library? This only clears the index entries \u2014 your original files on disk are not deleted.`
    );
    if (!ok) return;
    try {
      const removed = await api.deleteVideosByStatus("error");
      setVideos((prev) => prev.filter((v) => v.status !== "error"));
      push("success", `Removed ${removed} errored ${removed === 1 ? "file" : "files"}`);
    } catch (e) {
      push("error", `Could not remove errored files: ${String(e)}`);
    }
  };

  return (
    <>
      <TopBar
        title="Library"
        subtitle={folder ?? "Select a folder or drive to index"}
        gpu={gpu}
        deps={deps}
        busy={scanning}
      >
        <Button variant="secondary" size="sm" onClick={refresh}>
          <RefreshCw className="h-4 w-4" />
          Refresh
        </Button>
        <Button size="sm" onClick={handlePick}>
          <FolderOpen className="h-4 w-4" />
          Select folder
        </Button>
      </TopBar>

      <div className="flex-1 overflow-y-auto p-6">
        {scan && scanning && (
          <div className="card mb-4 p-4 animate-fade-in">
            <div className="mb-2 flex items-center justify-between text-sm">
              <span className="flex items-center gap-2 font-medium capitalize">
                <HardDriveDownload className="h-4 w-4 text-bds-accent" />
                {scan.phase}…
              </span>
              <span className="text-bds-muted">
                {formatNumber(scan.filesIndexed)} indexed ·{" "}
                {formatNumber(scan.filesDiscovered)} found ·{" "}
                {formatNumber(scan.filesSkipped)} skipped
              </span>
            </div>
            <Progress
              value={
                scan.filesDiscovered
                  ? (scan.filesIndexed / scan.filesDiscovered) * 100
                  : 8
              }
            />
            {scan.currentPath && (
              <p className="mt-2 truncate font-mono text-[11px] text-bds-muted">
                {scan.currentPath}
              </p>
            )}
          </div>
        )}

        {videos.length === 0 ? (
          <EmptyState onPick={handlePick} />
        ) : (
          <>
            <div className="mb-3 flex items-center justify-between">
              <p className="text-sm text-bds-muted">
                {formatNumber(videos.length)} videos indexed
                {errorCount > 0 && (
                  <span className="text-bds-bad">
                    {" "}· {formatNumber(errorCount)} with errors
                  </span>
                )}
              </p>
              <div className="flex items-center gap-2">
                {errorCount > 0 && (
                  <Button
                    variant="danger"
                    size="sm"
                    onClick={handleDeleteErrored}
                  >
                    <Trash2 className="h-4 w-4" />
                    Remove errored ({formatNumber(errorCount)})
                  </Button>
                )}
                <Button variant="outline" size="sm" onClick={handleProcessAll}>
                  <Play className="h-4 w-4" />
                  Process all pending
                </Button>
              </div>
            </div>
            <div className="card overflow-hidden">
              <table className="w-full text-sm">
                <thead className="border-b border-bds-border text-left text-xs text-bds-muted">
                  <tr>
                    <th className="px-4 py-2.5 font-medium">File</th>
                    <th className="px-4 py-2.5 font-medium">Duration</th>
                    <th className="px-4 py-2.5 font-medium">Resolution</th>
                    <th className="px-4 py-2.5 font-medium">Codec</th>
                    <th className="px-4 py-2.5 font-medium">Size</th>
                    <th className="px-4 py-2.5 font-medium">Status</th>
                    <th className="px-4 py-2.5 font-medium text-right">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {videos.map((v) => (
                    <tr
                      key={v.id}
                      className="border-b border-bds-border/50 transition-colors hover:bg-bds-surface2/50"
                    >
                      <td className="max-w-[360px] px-4 py-2.5">
                        <div className="flex items-center gap-2">
                          <FileVideo className="h-4 w-4 shrink-0 text-bds-muted" />
                          <span className="truncate">{v.filename}</span>
                        </div>
                      </td>
                      <td className="px-4 py-2.5 text-bds-muted">
                        {v.durationSeconds
                          ? formatDuration(v.durationSeconds)
                          : "—"}
                      </td>
                      <td className="px-4 py-2.5 text-bds-muted">
                        {v.width && v.height ? `${v.width}×${v.height}` : "—"}
                        {v.fps ? ` · ${Math.round(v.fps)}fps` : ""}
                      </td>
                      <td className="px-4 py-2.5 text-bds-muted">
                        {v.codec ?? "—"}
                      </td>
                      <td className="px-4 py-2.5 text-bds-muted">
                        {formatBytes(v.sizeBytes)}
                      </td>
                      <td className="px-4 py-2.5">
                        <Badge variant={STATUS_VARIANT[v.status] ?? "default"}>
                          {v.status}
                        </Badge>
                      </td>
                      <td className="px-4 py-2.5 text-right">
                        <Button
                          variant="ghost"
                          size="icon"
                          title="Remove from library"
                          aria-label={`Remove ${v.filename} from library`}
                          onClick={() => handleDeleteOne(v)}
                          className="text-bds-muted hover:text-bds-bad"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </div>
    </>
  );
}

function EmptyState({ onPick }: { onPick: () => void }) {
  return (
    <div className="flex h-[60vh] flex-col items-center justify-center text-center animate-fade-in">
      <div className="grid h-16 w-16 place-items-center rounded-2xl bg-bds-surface2">
        <FolderOpen className="h-8 w-8 text-bds-accent" />
      </div>
      <h2 className="mt-5 text-lg font-semibold">No library indexed yet</h2>
      <p className="mt-1 max-w-sm text-sm text-bds-muted">
        Point Boostify at a folder or an entire SSD. Every video is scanned,
        hashed and indexed — already-seen files are never reprocessed.
      </p>
      <Button className="mt-5" onClick={onPick}>
        <FolderOpen className="h-4 w-4" />
        Select a folder
      </Button>
    </div>
  );
}
