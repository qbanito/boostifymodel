import { useCallback, useEffect, useState } from "react";
import { Sidebar, type Page } from "./components/Sidebar";
import { ToastProvider } from "./components/ui/toast";
import { Dashboard } from "./pages/Dashboard";
import { Library } from "./pages/Library";
import { Pipeline } from "./pages/Pipeline";
import { Review } from "./pages/Review";
import { Editor } from "./pages/Editor";
import { SmartSearch } from "./pages/SmartSearch";
import { Datasets } from "./pages/Datasets";
import { GpuServer } from "./pages/GpuServer";
import { SettingsPage } from "./pages/Settings";
import { api } from "./lib/api";
import type { GpuInfo } from "./lib/types";

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const [gpu, setGpu] = useState<GpuInfo | null>(null);
  const [deps, setDeps] = useState<{ ffmpeg: boolean; ffprobe: boolean } | null>(
    null
  );

  const loadSystem = useCallback(async () => {
    try {
      const [g, d] = await Promise.all([
        api.gpuInfo(),
        api.checkDependencies(),
      ]);
      setGpu(g);
      setDeps(d);
    } catch {
      // backend not ready (e.g. plain browser preview)
    }
  }, []);

  useEffect(() => {
    loadSystem();
  }, [loadSystem]);

  const shared = { gpu, deps };

  return (
    <ToastProvider>
      <div className="flex h-screen w-screen overflow-hidden">
        <Sidebar page={page} onNavigate={setPage} />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {page === "dashboard" && <Dashboard {...shared} />}
          {page === "library" && <Library {...shared} />}
          {page === "pipeline" && <Pipeline {...shared} />}
          {page === "review" && <Review {...shared} />}
          {page === "editor" && <Editor />}
          {page === "search" && <SmartSearch {...shared} />}
          {page === "datasets" && <Datasets {...shared} />}
          {page === "gpu" && <GpuServer {...shared} />}
          {page === "settings" && (
            <SettingsPage {...shared} onSaved={loadSystem} />
          )}
        </main>
      </div>
    </ToastProvider>
  );
}
