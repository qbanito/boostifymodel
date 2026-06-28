import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium cursor-pointer transition-all duration-200 focus-ring disabled:pointer-events-none disabled:opacity-50 [&_svg]:size-4 [&_svg]:shrink-0 active:scale-[0.98]",
  {
    variants: {
      variant: {
        default:
          "bg-gradient-to-r from-bds-accent to-bds-accent2 text-black font-semibold hover:brightness-110 shadow-lg shadow-bds-accent/20",
        secondary:
          "bg-bds-surface2 text-bds-fg border border-bds-border hover:bg-bds-border/60",
        ghost: "text-bds-muted hover:text-bds-fg hover:bg-bds-surface2",
        outline:
          "border border-bds-border text-bds-fg hover:bg-bds-surface2 hover:border-bds-accent/40",
        danger: "bg-bds-bad/15 text-bds-bad border border-bds-bad/30 hover:bg-bds-bad/25",
        success:
          "bg-bds-good/15 text-bds-good border border-bds-good/30 hover:bg-bds-good/25",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 px-3 text-xs",
        lg: "h-11 px-6 text-base",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, ...props }, ref) => (
    <button
      ref={ref}
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  )
);
Button.displayName = "Button";
