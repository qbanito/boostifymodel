import { cn } from "@/lib/utils";
import { useTheme } from "@/lib/theme";
import {
  LayoutDashboard,
  FolderSearch,
  Workflow,
  Images,
  Search,
  Database,
  Settings,
  Boxes,
  Clapperboard,
  Server,
  Sun,
  Moon,
} from "lucide-react";

export type Page =
  | "dashboard"
  | "library"
  | "pipeline"
  | "review"
  | "editor"
  | "search"
  | "datasets"
  | "gpu"
  | "settings";

const NAV: { id: Page; label: string; icon: typeof LayoutDashboard }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "library", label: "Library", icon: FolderSearch },
  { id: "pipeline", label: "Pipeline", icon: Workflow },
  { id: "review", label: "Review", icon: Images },
  { id: "editor", label: "Video Editor", icon: Clapperboard },
  { id: "search", label: "Smart Search", icon: Search },
  { id: "datasets", label: "Datasets", icon: Database },
  { id: "gpu", label: "GPU Server", icon: Server },
  { id: "settings", label: "Settings", icon: Settings },
];

export function Sidebar({
  page,
  onNavigate,
}: {
  page: Page;
  onNavigate: (p: Page) => void;
}) {
  const { theme, toggle } = useTheme();
  return (
    <aside className="flex h-full w-[230px] shrink-0 flex-col border-r border-bds-border bg-bds-surface/40">
      <div className="flex items-center gap-2.5 px-5 py-5">
        <div className="grid h-9 w-9 place-items-center rounded-lg bg-gradient-to-br from-bds-accent to-bds-accent2 shadow-lg shadow-bds-accent/30">
          <Boxes className="h-5 w-5 text-black" />
        </div>
        <div className="leading-tight">
          <div className="text-[13px] font-semibold tracking-tight">
            Boostify
          </div>
          <div className="text-[11px] text-bds-muted">Dataset Studio</div>
        </div>
      </div>

      <nav className="flex-1 space-y-1 px-3 py-2">
        {NAV.map(({ id, label, icon: Icon }) => {
          const active = page === id;
          return (
            <button
              key={id}
              onClick={() => onNavigate(id)}
              className={cn(
                "group flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium cursor-pointer transition-all duration-200 focus-ring",
                active
                  ? "bg-bds-surface2 text-bds-fg"
                  : "text-bds-muted hover:text-bds-fg hover:bg-bds-surface2/60"
              )}
            >
              <Icon
                className={cn(
                  "h-[18px] w-[18px] transition-colors",
                  active ? "text-bds-accent" : "text-bds-muted group-hover:text-bds-fg"
                )}
              />
              {label}
              {active && (
                <span className="ml-auto h-1.5 w-1.5 rounded-full bg-bds-accent animate-pulse-glow" />
              )}
            </button>
          );
        })}
      </nav>

      <div className="space-y-2 px-3 py-3">
        <button
          onClick={toggle}
          className="flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm font-medium cursor-pointer text-bds-muted transition-all duration-200 hover:bg-bds-surface2/60 hover:text-bds-fg focus-ring"
          title={theme === "dark" ? "Cambiar a modo claro" : "Cambiar a modo oscuro"}
        >
          {theme === "dark" ? (
            <Sun className="h-[18px] w-[18px]" />
          ) : (
            <Moon className="h-[18px] w-[18px]" />
          )}
          {theme === "dark" ? "Modo claro" : "Modo oscuro"}
        </button>
        <div className="px-2 text-[10px] text-bds-muted/70">
          v0.1.0 · Cosmos-ready datasets
        </div>
      </div>
    </aside>
  );
}
