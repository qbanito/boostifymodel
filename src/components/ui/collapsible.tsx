import { useState, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Collapsible module section. Persists open/closed state per `id` in
 * localStorage so the layout is remembered between sessions.
 */
export function CollapsibleSection({
  id,
  title,
  icon,
  badge,
  actions,
  defaultOpen = true,
  className,
  children,
}: {
  id: string;
  title: string;
  icon?: ReactNode;
  badge?: ReactNode;
  actions?: ReactNode;
  defaultOpen?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const key = `bds-sec-${id}`;
  const [open, setOpen] = useState(() => {
    const v = localStorage.getItem(key);
    return v === null ? defaultOpen : v === "1";
  });
  const toggle = () => {
    setOpen((o) => {
      localStorage.setItem(key, o ? "0" : "1");
      return !o;
    });
  };

  return (
    <section className={cn("border-b border-bds-border", className)}>
      <div className="flex items-center gap-2 px-4 py-2">
        <button
          onClick={toggle}
          className="flex min-w-0 flex-1 items-center gap-2 text-left cursor-pointer focus-ring rounded"
        >
          <ChevronDown
            className={cn(
              "h-4 w-4 shrink-0 text-bds-muted transition-transform duration-200",
              !open && "-rotate-90"
            )}
          />
          {icon}
          <span className="truncate text-xs font-semibold tracking-tight">
            {title}
          </span>
          {badge}
        </button>
        {actions && <div className="flex items-center gap-1.5">{actions}</div>}
      </div>
      {open && <div className="animate-fade-in">{children}</div>}
    </section>
  );
}
