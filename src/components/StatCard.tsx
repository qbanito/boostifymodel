import { cn } from "@/lib/utils";
import type { LucideIcon } from "lucide-react";

export function StatCard({
  label,
  value,
  hint,
  icon: Icon,
  tone = "default",
}: {
  label: string;
  value: string | number;
  hint?: string;
  icon?: LucideIcon;
  tone?: "default" | "good" | "warn" | "bad" | "accent";
}) {
  const toneText = {
    default: "text-bds-fg",
    good: "text-bds-good",
    warn: "text-bds-warn",
    bad: "text-bds-bad",
    accent: "text-bds-accent",
  }[tone];

  return (
    <div className="card group relative overflow-hidden p-4 transition-colors hover:border-bds-accent/30">
      <div className="flex items-start justify-between">
        <span className="text-xs font-medium text-bds-muted">{label}</span>
        {Icon && (
          <Icon className="h-4 w-4 text-bds-muted/60 transition-colors group-hover:text-bds-accent" />
        )}
      </div>
      <div className={cn("mt-2 text-2xl font-semibold tracking-tight", toneText)}>
        {value}
      </div>
      {hint && <div className="mt-1 text-[11px] text-bds-muted">{hint}</div>}
    </div>
  );
}
