import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium leading-none transition-colors",
  {
    variants: {
      variant: {
        default: "bg-bds-surface2 text-bds-muted border border-bds-border",
        accent: "bg-bds-accent/15 text-bds-accent border border-bds-accent/30",
        good: "bg-bds-good/15 text-bds-good border border-bds-good/30",
        warn: "bg-bds-warn/15 text-bds-warn border border-bds-warn/30",
        bad: "bg-bds-bad/15 text-bds-bad border border-bds-bad/30",
        info: "bg-bds-info/15 text-bds-info border border-bds-info/30",
      },
    },
    defaultVariants: { variant: "default" },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
