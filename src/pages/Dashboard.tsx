import { useEffect, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { StatCard } from "@/components/StatCard";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/controls";
import { api } from "@/lib/api";
import type { DashboardStats, GpuInfo } from "@/lib/types";
import { formatBytes, formatDuration, formatNumber } from "@/lib/utils";
import {
  Film,
  CheckCircle2,
  XCircle,
  Scissors,
  Timer,
  Gauge,
  HardDrive,
  Sparkles,
  Database,
  TrendingUp,
} from "lucide-react";

export interface SharedProps {
  gpu: GpuInfo | null;
  deps: { ffmpeg: boolean; ffprobe: boolean } | null;
}

const EMPTY: DashboardStats = {
  videosFound: 0,
  videosProcessed: 0,
  clipsCreated: 0,
  clipsApproved: 0,
  clipsRejected: 0,
  avgProcessSeconds: 0,
  datasetSizeBytes: 0,
  gpuMode: "cpu",
  storageFreeBytes: 0,
  storageTotalBytes: 0,
  avgTrainingScore: 0,
};

export function Dashboard({ gpu, deps }: SharedProps) {
  const [stats, setStats] = useState<DashboardStats>(EMPTY);

  useEffect(() => {
    let active = true;
    const load = async () => {
      try {
        const s = await api.dashboardStats();
        if (active) setStats(s);
      } catch {
        /* backend not ready */
      }
    };
    load();
    const t = setInterval(load, 3000);
    return () => {
      active = false;
      clearInterval(t);
    };
  }, []);

  const storageUsed = stats.storageTotalBytes - stats.storageFreeBytes;
  const storagePct = stats.storageTotalBytes
    ? (storageUsed / stats.storageTotalBytes) * 100
    : 0;
  const approvalRate =
    stats.clipsCreated > 0
      ? Math.round((stats.clipsApproved / stats.clipsCreated) * 100)
      : 0;

  return (
    <>
      <TopBar
        title="Dashboard"
        subtitle="Live overview of your dataset factory"
        gpu={gpu}
        deps={deps}
      />
      <div className="flex-1 overflow-y-auto p-6">
        <div className="grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-5">
          <StatCard
            label="Videos found"
            value={formatNumber(stats.videosFound)}
            icon={Film}
          />
          <StatCard
            label="Videos processed"
            value={formatNumber(stats.videosProcessed)}
            hint={`${
              stats.videosFound
                ? Math.round((stats.videosProcessed / stats.videosFound) * 100)
                : 0
            }% complete`}
            icon={Gauge}
            tone="accent"
          />
          <StatCard
            label="Clips created"
            value={formatNumber(stats.clipsCreated)}
            icon={Scissors}
          />
          <StatCard
            label="Clips approved"
            value={formatNumber(stats.clipsApproved)}
            hint={`${approvalRate}% approval`}
            icon={CheckCircle2}
            tone="good"
          />
          <StatCard
            label="Clips rejected"
            value={formatNumber(stats.clipsRejected)}
            icon={XCircle}
            tone="bad"
          />
        </div>

        <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
          <StatCard
            label="Avg process time"
            value={formatDuration(stats.avgProcessSeconds)}
            hint="per video"
            icon={Timer}
          />
          <StatCard
            label="Dataset size"
            value={formatBytes(stats.datasetSizeBytes)}
            icon={Database}
          />
          <StatCard
            label="Training score"
            value={stats.avgTrainingScore.toFixed(1)}
            hint="avg / 100"
            icon={TrendingUp}
            tone="accent"
          />
          <StatCard
            label="GPU mode"
            value={(gpu?.mode ?? stats.gpuMode).toUpperCase()}
            hint={gpu?.device ?? "compute device"}
            icon={Sparkles}
            tone={gpu && gpu.mode !== "cpu" ? "good" : "default"}
          />
        </div>

        <div className="mt-4 grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <HardDrive className="h-4 w-4 text-bds-muted" />
                Storage
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Progress value={storagePct} />
              <div className="mt-3 flex justify-between text-xs text-bds-muted">
                <span>{formatBytes(storageUsed)} used</span>
                <span>{formatBytes(stats.storageFreeBytes)} free</span>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Gauge className="h-4 w-4 text-bds-muted" />
                Pipeline throughput
              </CardTitle>
            </CardHeader>
            <CardContent>
              <Progress
                value={
                  stats.videosFound
                    ? (stats.videosProcessed / stats.videosFound) * 100
                    : 0
                }
                indicatorClassName="from-bds-good to-bds-info"
              />
              <div className="mt-3 flex justify-between text-xs text-bds-muted">
                <span>{formatNumber(stats.videosProcessed)} processed</span>
                <span>
                  {formatNumber(
                    Math.max(0, stats.videosFound - stats.videosProcessed)
                  )}{" "}
                  pending
                </span>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </>
  );
}
