import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { TopBar } from "@/components/TopBar";
import { Input } from "@/components/ui/controls";
import { Badge } from "@/components/ui/badge";
import { api } from "@/lib/api";
import type { Clip } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import { formatDuration } from "@/lib/utils";
import { Search, Film, Sparkles } from "lucide-react";

const SUGGESTIONS = [
  "microphone",
  "neon light",
  "beach",
  "performance",
  "drone",
  "golden hour",
  "dancing",
  "piano",
  "luxury",
  "close up",
  "rain",
  "concert",
];

export function SmartSearch({ gpu, deps }: SharedProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Clip[]>([]);
  const [searched, setSearched] = useState(false);

  const run = async (q: string) => {
    setQuery(q);
    if (!q.trim()) return;
    try {
      const r = await api.searchClips(q);
      setResults(r);
      setSearched(true);
    } catch {
      setResults([]);
      setSearched(true);
    }
  };

  return (
    <>
      <TopBar
        title="Smart Search"
        subtitle="Find clips by content, mood, gear, location…"
        gpu={gpu}
        deps={deps}
      />
      <div className="flex-1 overflow-y-auto p-6">
        <div className="relative mx-auto max-w-2xl">
          <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-bds-muted" />
          <Input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && run(query)}
            placeholder="Search: 'singing close up neon night'…"
            className="h-11 pl-10 text-base"
          />
        </div>

        <div className="mx-auto mt-3 flex max-w-2xl flex-wrap justify-center gap-2">
          {SUGGESTIONS.map((s) => (
            <button
              key={s}
              onClick={() => run(s)}
              className="rounded-full border border-bds-border bg-bds-surface2 px-3 py-1 text-xs text-bds-muted transition-colors hover:border-bds-accent/40 hover:text-bds-fg cursor-pointer focus-ring"
            >
              {s}
            </button>
          ))}
        </div>

        <div className="mt-6">
          {!searched ? (
            <div className="flex h-[40vh] flex-col items-center justify-center text-center text-bds-muted">
              <Sparkles className="h-9 w-9" />
              <p className="mt-3 text-sm">
                Search across captions, tags and scene analysis.
              </p>
            </div>
          ) : results.length === 0 ? (
            <p className="text-center text-sm text-bds-muted">
              No clips match “{query}”.
            </p>
          ) : (
            <>
              <p className="mb-3 text-sm text-bds-muted">
                {results.length} result{results.length === 1 ? "" : "s"}
              </p>
              <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-4">
                {results.map((clip) => (
                  <div
                    key={clip.id}
                    className="group overflow-hidden rounded-lg border border-bds-border bg-bds-surface2"
                  >
                    <div className="aspect-video overflow-hidden">
                      {clip.thumbnailPath ? (
                        <img
                          src={convertFileSrc(clip.thumbnailPath)}
                          alt=""
                          className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
                        />
                      ) : (
                        <div className="grid h-full place-items-center">
                          <Film className="h-6 w-6 text-bds-muted" />
                        </div>
                      )}
                    </div>
                    <div className="p-2.5">
                      <p className="line-clamp-2 text-xs text-bds-fg">
                        {clip.caption ?? "Untitled clip"}
                      </p>
                      <div className="mt-2 flex flex-wrap gap-1">
                        <Badge variant="default">
                          {formatDuration(clip.durationSeconds)}
                        </Badge>
                        {clip.tags.slice(0, 2).map((t) => (
                          <Badge key={t} variant="accent">
                            {t}
                          </Badge>
                        ))}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      </div>
    </>
  );
}
