import { useEffect, useRef, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { PipelineProgress } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import {
  Play,
  ScanLine,
  Scissors,
  Brain,
  PenLine,
  Activity,
  Gauge,
  ShieldCheck,
  CopyCheck,
  Terminal,
} from "lucide-react";

const STAGES = [
  { id: "index", label: "Index", icon: ScanLine },
  { id: "split", label: "Scene split", icon: Scissors },
  { id: "analyze", label: "Scene analysis", icon: Brain },
  { id: "caption", label: "Captioning", icon: PenLine },
  { id: "motion", label: "Motion extract", icon: Activity },
  { id: "score", label: "Quality score", icon: Gauge },
  { id: "dedup", label: "Dedup", icon: CopyCheck },
  { id: "approve", label: "Approve", icon: ShieldCheck },
];

export function Pipeline({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [progress, setProgress] = useState<PipelineProgress | null>(null);
  const [logs, setLogs] = useState<string[]>([]);
  const [running, setRunning] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);
  const unlistenA = useRef<(() => void) | null>(null);
  const unlistenB = useRef<(() => void) | null>(null);

  useEffect(() => {
    api
      .onPipelineProgress((p) => {
        setProgress(p);
        if (p.done) setRunning(false);
      })
      .then((fn) => (unlistenA.current = fn))
      .catch(() => {});
    api
      .onLog((line) => {
        setLogs((l) => [...l.slice(-400), line]);
      })
      .then((fn) => (unlistenB.current = fn))
      .catch(() => {});
    return () => {
      unlistenA.current?.();
      unlistenB.current?.();
    };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [logs]);

  const start = async () => {
    try {
      setRunning(true);
      await api.processAllPending();
      push("info", "Pipeline started for all pending videos");
    } catch (e) {
      setRunning(false);
      push("error", `Pipeline error: ${String(e)}`);
    }
  };

  const activeStage = progress?.stage ?? "";

  return (
    <>
      <TopBar
        title="Pipeline"
        subtitle="Asynchronous, multi-stage clip factory"
        gpu={gpu}
        deps={deps}
        busy={running}
      >
        <Button size="sm" onClick={start} disabled={running}>
          <Play className="h-4 w-4" />
          {running ? "Running…" : "Run pipeline"}
        </Button>
      </TopBar>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4 lg:grid-cols-8">
          {STAGES.map(({ id, label, icon: Icon }) => {
            const active = activeStage === id && running;
            return (
              <div
                key={id}
                className={`card flex flex-col items-center gap-2 p-4 text-center transition-all ${
                  active ? "border-bds-accent/60 bg-bds-accent/5" : ""
                }`}
              >
                <Icon
                  className={`h-5 w-5 ${
                    active
                      ? "text-bds-accent animate-pulse"
                      : "text-bds-muted"
                  }`}
                />
                <span className="text-[11px] font-medium text-bds-fg">
                  {label}
                </span>
              </div>
            );
          })}
        </div>

        {progress && (
          <div className="mt-4 grid grid-cols-3 gap-4">
            <Card>
              <CardContent className="pt-4">
                <div className="text-xs text-bds-muted">Clips created</div>
                <div className="mt-1 text-2xl font-semibold">
                  {progress.clipsCreated}
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-4">
                <div className="text-xs text-bds-muted">Approved</div>
                <div className="mt-1 text-2xl font-semibold text-bds-good">
                  {progress.clipsApproved}
                </div>
              </CardContent>
            </Card>
            <Card>
              <CardContent className="pt-4">
                <div className="text-xs text-bds-muted">Rejected</div>
                <div className="mt-1 text-2xl font-semibold text-bds-bad">
                  {progress.clipsRejected}
                </div>
              </CardContent>
            </Card>
          </div>
        )}

        <Card className="mt-4">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Terminal className="h-4 w-4 text-bds-muted" />
              Live log
              {progress?.message && (
                <Badge variant="accent" className="ml-2">
                  {progress.message}
                </Badge>
              )}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div
              ref={logRef}
              className="h-[280px] overflow-y-auto rounded-md bg-black/40 p-3 font-mono text-[12px] leading-relaxed text-bds-muted"
            >
              {logs.length === 0 ? (
                <span className="text-bds-muted/60">
                  Waiting for pipeline activity…
                </span>
              ) : (
                logs.map((l, i) => (
                  <div key={i} className="whitespace-pre-wrap">
                    {l}
                  </div>
                ))
              )}
            </div>
          </CardContent>
        </Card>
      </div>
    </>
  );
}
