import { cn } from "@/lib/utils";
import { Badge } from "./ui/badge";
import type { GpuInfo } from "@/lib/types";
import { Cpu, HardDriveDownload, Activity } from "lucide-react";

export function TopBar({
  title,
  subtitle,
  gpu,
  deps,
  busy,
  children,
}: {
  title: string;
  subtitle?: string;
  gpu?: GpuInfo | null;
  deps?: { ffmpeg: boolean; ffprobe: boolean } | null;
  busy?: boolean;
  children?: React.ReactNode;
}) {
  return (
    <header className="flex h-12 shrink-0 items-center justify-between border-b border-bds-border px-4">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <h1 className="truncate text-sm font-semibold tracking-tight">{title}</h1>
          {busy && (
            <Activity className="h-3.5 w-3.5 animate-pulse text-bds-accent" />
          )}
        </div>
        {subtitle && (
          <p className="truncate text-[11px] leading-tight text-bds-muted">{subtitle}</p>
        )}
      </div>

      <div className="flex items-center gap-1.5">
        {children}
        {deps && !(deps.ffmpeg && deps.ffprobe) && (
          <Badge variant="warn" className="gap-1">
            <HardDriveDownload className="h-3 w-3" />
            FFmpeg missing
          </Badge>
        )}
        {gpu && (
          <Badge
            variant={gpu.mode === "cpu" ? "default" : "good"}
            className={cn("gap-1", gpu.mode !== "cpu" && "animate-pulse-glow")}
          >
            <Cpu className="h-3 w-3" />
            {gpu.device}
          </Badge>
        )}
      </div>
    </header>
  );
}
