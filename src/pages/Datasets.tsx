import { useEffect, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/controls";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { DatasetInfo } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import { formatNumber, relativeTime } from "@/lib/utils";
import { Database, Download, Package } from "lucide-react";

const FORMATS = [
  { id: "cosmos-predict", label: "Cosmos Predict" },
  { id: "cosmos-transfer", label: "Cosmos Transfer" },
  { id: "lora", label: "LoRA" },
  { id: "video-ft", label: "Video Fine-Tuning" },
  { id: "nemo", label: "NVIDIA NeMo" },
];

export function Datasets({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [datasets, setDatasets] = useState<DatasetInfo[]>([]);
  const [name, setName] = useState("boostify-music-v1");
  const [format, setFormat] = useState("cosmos-predict");
  const [exporting, setExporting] = useState(false);

  const refresh = async () => {
    try {
      setDatasets(await api.listDatasets());
    } catch {
      /* backend not ready */
    }
  };

  useEffect(() => {
    refresh();
  }, []);

  const exportNow = async () => {
    try {
      setExporting(true);
      const out = await api.exportDataset(name, format);
      push("success", `Exported to ${out}`);
      refresh();
    } catch (e) {
      push("error", String(e));
    } finally {
      setExporting(false);
    }
  };

  return (
    <>
      <TopBar
        title="Datasets"
        subtitle="Organize & export training-ready datasets"
        gpu={gpu}
        deps={deps}
      />
      <div className="flex-1 overflow-y-auto p-6">
        <Card>
          <CardContent className="pt-5">
            <div className="flex items-center gap-2">
              <Package className="h-4 w-4 text-bds-accent" />
              <h3 className="text-sm font-semibold">Export dataset</h3>
            </div>
            <div className="mt-4 grid grid-cols-1 gap-3 md:grid-cols-[1fr_auto]">
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">Dataset name</label>
                <Input value={name} onChange={(e) => setName(e.target.value)} />
              </div>
              <div className="space-y-1">
                <label className="text-xs text-bds-muted">Target format</label>
                <div className="flex flex-wrap gap-2">
                  {FORMATS.map((f) => (
                    <button
                      key={f.id}
                      onClick={() => setFormat(f.id)}
                      className={`rounded-md border px-3 py-2 text-xs font-medium transition-colors cursor-pointer focus-ring ${
                        format === f.id
                          ? "border-bds-accent bg-bds-accent/10 text-bds-accent"
                          : "border-bds-border text-bds-muted hover:text-bds-fg"
                      }`}
                    >
                      {f.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
            <Button className="mt-4" onClick={exportNow} disabled={exporting}>
              <Download className="h-4 w-4" />
              {exporting ? "Exporting…" : "Export approved clips"}
            </Button>
            <p className="mt-2 text-[11px] text-bds-muted">
              Builds the dataset/ tree (videos, captions, metadata, pose, depth,
              edges, segmentation, optical_flow) and writes train/validation/test
              JSONL splits.
            </p>
          </CardContent>
        </Card>

        <h3 className="mb-3 mt-6 text-sm font-semibold text-bds-muted">
          Existing datasets
        </h3>
        {datasets.length === 0 ? (
          <div className="flex h-40 flex-col items-center justify-center rounded-lg border border-dashed border-bds-border text-center text-bds-muted">
            <Database className="h-8 w-8" />
            <p className="mt-2 text-sm">No datasets exported yet.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {datasets.map((d) => (
              <Card key={d.id}>
                <CardContent className="pt-4">
                  <div className="flex items-start justify-between">
                    <div>
                      <div className="font-medium">{d.name}</div>
                      <div className="text-[11px] text-bds-muted">
                        {relativeTime(d.createdAt)}
                      </div>
                    </div>
                    <Badge variant="accent">{d.format}</Badge>
                  </div>
                  <div className="mt-3 text-sm text-bds-muted">
                    {formatNumber(d.clipCount)} clips
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
