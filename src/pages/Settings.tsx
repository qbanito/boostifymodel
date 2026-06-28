import { useEffect, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/controls";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { AppSettings } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import { Save, FolderOpen, Eye, Cpu, KeyRound, Sliders } from "lucide-react";

const DEFAULTS: AppSettings = {
  qualityThreshold: 60,
  minClipSeconds: 2,
  sceneThreshold: 0.4,
  openaiApiKey: "",
  nimApiKey: "",
  nimModel: "meta/llama-3.2-11b-vision-instruct",
  outputDir: "",
  exportFormat: "cosmos-predict",
  watchEnabled: false,
  concurrency: 4,
  gpuInstance: "boostify1",
};

export function SettingsPage({
  gpu,
  deps,
  onSaved,
}: SharedProps & { onSaved: () => void }) {
  const { push } = useToast();
  const [s, setS] = useState<AppSettings>(DEFAULTS);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api
      .getSettings()
      .then((loaded) => setS({ ...DEFAULTS, ...loaded }))
      .catch(() => {});
  }, []);

  const set = <K extends keyof AppSettings>(k: K, v: AppSettings[K]) =>
    setS((prev) => ({ ...prev, [k]: v }));

  const save = async () => {
    try {
      setSaving(true);
      await api.saveSettings(s);
      push("success", "Settings saved");
      onSaved();
    } catch (e) {
      push("error", String(e));
    } finally {
      setSaving(false);
    }
  };

  const pickOutput = async () => {
    const dir = await api.pickFolder();
    if (dir) set("outputDir", dir);
  };

  return (
    <>
      <TopBar title="Settings" subtitle="Tune the pipeline" gpu={gpu} deps={deps}>
        <Button size="sm" onClick={save} disabled={saving}>
          <Save className="h-4 w-4" />
          {saving ? "Saving…" : "Save"}
        </Button>
      </TopBar>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Sliders className="h-4 w-4 text-bds-muted" />
                Quality & splitting
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-4">
              <SliderField
                label="Quality threshold"
                hint="auto-approve clips above this score"
                min={0}
                max={100}
                value={s.qualityThreshold}
                onChange={(v) => set("qualityThreshold", v)}
                suffix="/100"
              />
              <SliderField
                label="Minimum clip length"
                min={1}
                max={20}
                value={s.minClipSeconds}
                onChange={(v) => set("minClipSeconds", v)}
                suffix="s"
              />
              <SliderField
                label="Scene-cut sensitivity"
                min={0.1}
                max={0.9}
                step={0.05}
                value={s.sceneThreshold}
                onChange={(v) => set("sceneThreshold", v)}
              />
              <SliderField
                label="Concurrency"
                hint="videos processed in parallel"
                min={1}
                max={16}
                value={s.concurrency}
                onChange={(v) => set("concurrency", v)}
              />
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <KeyRound className="h-4 w-4 text-bds-muted" />
                AI captioning (vision model)
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">OpenAI API key</label>
                <Input
                  type="password"
                  value={s.openaiApiKey}
                  onChange={(e) => set("openaiApiKey", e.target.value)}
                  placeholder="sk-…"
                />
                <p className="text-[11px] text-bds-muted">
                  Recommended. Uses <code>gpt-4o-mini</code> vision to caption each
                  clip from its thumbnail.
                </p>
              </div>
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">NVIDIA NIM API key</label>
                <Input
                  type="password"
                  value={s.nimApiKey}
                  onChange={(e) => set("nimApiKey", e.target.value)}
                  placeholder="nvapi-…"
                />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">NIM vision model</label>
                <Input
                  value={s.nimModel}
                  onChange={(e) => set("nimModel", e.target.value)}
                  placeholder="meta/llama-3.2-11b-vision-instruct"
                />
              </div>
              <p className="text-[11px] text-bds-muted">
                Captions describe each clip from its actual frame. OpenAI is tried
                first, then NVIDIA NIM. Without any key the pipeline falls back to
                a local heuristic captioner.
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <FolderOpen className="h-4 w-4 text-bds-muted" />
                Output
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">Dataset output folder</label>
                <div className="flex gap-2">
                  <Input
                    value={s.outputDir}
                    onChange={(e) => set("outputDir", e.target.value)}
                    placeholder="~/BoostifyDatasets"
                  />
                  <Button variant="secondary" size="icon" onClick={pickOutput}>
                    <FolderOpen className="h-4 w-4" />
                  </Button>
                </div>
              </div>
              <label className="flex items-center justify-between rounded-md border border-bds-border bg-bds-surface2 px-3 py-2.5">
                <span className="flex items-center gap-2 text-sm">
                  <Eye className="h-4 w-4 text-bds-muted" />
                  Watch mode
                  <span className="text-[11px] text-bds-muted">
                    auto-process new files
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={s.watchEnabled}
                  onChange={(e) => set("watchEnabled", e.target.checked)}
                  className="h-4 w-4 cursor-pointer accent-bds-accent"
                />
              </label>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2">
                <Cpu className="h-4 w-4 text-bds-muted" />
                System
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 text-sm">
              <Row label="Compute">
                <Badge variant={gpu && gpu.mode !== "cpu" ? "good" : "default"}>
                  {gpu?.device ?? "Detecting…"}
                </Badge>
              </Row>
              <Row label="FFmpeg">
                <Badge variant={deps?.ffmpeg ? "good" : "bad"}>
                  {deps?.ffmpeg ? "installed" : "missing"}
                </Badge>
              </Row>
              <Row label="FFprobe">
                <Badge variant={deps?.ffprobe ? "good" : "bad"}>
                  {deps?.ffprobe ? "installed" : "missing"}
                </Badge>
              </Row>
              {deps && !(deps.ffmpeg && deps.ffprobe) && (
                <p className="pt-1 text-[11px] text-bds-warn">
                  Install FFmpeg to enable scene splitting & probing.
                </p>
              )}
              <div className="space-y-1 pt-1">
                <label className="text-xs text-bds-muted">
                  GPU instance (Brev)
                </label>
                <Input
                  value={s.gpuInstance}
                  onChange={(e) => set("gpuInstance", e.target.value)}
                  placeholder="boostify1"
                />
                <p className="text-[11px] text-bds-muted">
                  Nombre de la instancia remota que enciendes/apagas desde la
                  pestaña <strong>GPU Server</strong>.
                </p>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-bds-muted">{label}</span>
      {children}
    </div>
  );
}

function SliderField({
  label,
  hint,
  value,
  min,
  max,
  step = 1,
  suffix,
  onChange,
}: {
  label: string;
  hint?: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  suffix?: string;
  onChange: (v: number) => void;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between">
        <label className="text-xs text-bds-muted">
          {label}
          {hint && <span className="ml-1 text-bds-muted/60">· {hint}</span>}
        </label>
        <span className="text-xs font-medium text-bds-fg">
          {value}
          {suffix}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full cursor-pointer accent-bds-accent"
      />
    </div>
  );
}
