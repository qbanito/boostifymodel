import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";

export type MenuItem = {
  label: string;
  icon?: ReactNode;
  onClick?: () => void;
  disabled?: boolean;
  danger?: boolean;
  separator?: false;
};
export type MenuSeparator = { separator: true };
export type MenuEntry = MenuItem | MenuSeparator;

/** Lightweight right-click context menu rendered in a portal. */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const close = () => onClose();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("click", close);
    window.addEventListener("contextmenu", close);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("contextmenu", close);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
    };
  }, [onClose]);

  // Keep on-screen.
  const left = Math.min(x, window.innerWidth - 220);
  const top = Math.min(y, window.innerHeight - items.length * 32 - 12);

  return createPortal(
    <div
      ref={ref}
      className="fixed z-[200] min-w-[200px] overflow-hidden rounded-md border border-bds-border bg-bds-surface py-1 shadow-2xl animate-fade-in"
      style={{ left, top }}
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it, i) =>
        "separator" in it ? (
          <div key={i} className="my-1 h-px bg-bds-border" />
        ) : (
          <button
            key={i}
            disabled={it.disabled}
            onClick={() => {
              it.onClick?.();
              onClose();
            }}
            className={cn(
              "flex w-full items-center gap-2.5 px-3 py-1.5 text-left text-xs cursor-pointer transition-colors disabled:opacity-40 disabled:pointer-events-none",
              it.danger
                ? "text-bds-bad hover:bg-bds-bad/15"
                : "text-bds-fg hover:bg-bds-surface2"
            )}
          >
            {it.icon && <span className="shrink-0 [&_svg]:size-3.5">{it.icon}</span>}
            {it.label}
          </button>
        )
      )}
    </div>,
    document.body
  );
}
