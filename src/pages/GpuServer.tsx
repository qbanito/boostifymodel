import { useCallback, useEffect, useState } from "react";
import { TopBar } from "@/components/TopBar";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useToast } from "@/components/ui/toast";
import { api } from "@/lib/api";
import type { GpuServerStatus } from "@/lib/types";
import type { SharedProps } from "./Dashboard";
import {
  Server,
  Power,
  PowerOff,
  RefreshCw,
  Cpu,
  HardDrive,
  Terminal,
  AlertTriangle,
} from "lucide-react";

type BadgeVariant = "default" | "accent" | "good" | "warn" | "bad" | "info";

function statusMeta(status: string): { variant: BadgeVariant; label: string } {
  switch (status) {
    case "RUNNING":
      return { variant: "good", label: "Encendido" };
    case "STARTING":
      return { variant: "warn", label: "Encendiendo…" };
    case "STOPPING":
      return { variant: "warn", label: "Apagando…" };
    case "STOPPED":
      return { variant: "default", label: "Apagado" };
    case "NOT_FOUND":
      return { variant: "bad", label: "No encontrada" };
    case "NOT_LOGGED_IN":
      return { variant: "warn", label: "Sin sesión" };
    case "NO_BREV":
      return { variant: "bad", label: "Brev no instalado" };
    case "ERROR":
      return { variant: "bad", label: "Error" };
    default:
      return { variant: "default", label: status || "Desconocido" };
  }
}

export function GpuServer({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [st, setSt] = useState<GpuServerStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(
    async (silent = false) => {
      if (!silent) setLoading(true);
      try {
        const s = await api.gpuServerStatus();
        setSt(s);
      } catch (e) {
        push("error", String(e));
      } finally {
        setLoading(false);
      }
    },
    [push]
  );

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Auto-poll while a transition is in flight so the badge settles.
  useEffect(() => {
    if (!st) return;
    if (st.status === "STARTING" || st.status === "STOPPING") {
      const t = setTimeout(() => refresh(true), 8000);
      return () => clearTimeout(t);
    }
  }, [st, refresh]);

  const power = async (on: boolean) => {
    setBusy(true);
    try {
      const s = on ? await api.gpuServerStart() : await api.gpuServerStop();
      setSt(s);
      push("success", on ? "Servidor encendiéndose" : "Servidor apagándose");
    } catch (e) {
      push("error", String(e));
    } finally {
      setBusy(false);
    }
  };

  const meta = st ? statusMeta(st.status) : { variant: "default" as BadgeVariant, label: "…" };
  const running = st?.status === "RUNNING";
  const stopped = st?.status === "STOPPED";
  const usable = !!st && st.installed && st.loggedIn;

  return (
    <div className="flex h-full flex-col">
      <TopBar
        title="GPU Server"
        subtitle="Enciende, apaga y conéctate a tu GPU remota (Brev)"
        gpu={gpu}
        deps={deps}
        busy={busy || loading}
      >
        <Button
          variant="ghost"
          size="sm"
          onClick={() => refresh()}
          disabled={busy}
        >
          <RefreshCw className={loading ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
          Actualizar
        </Button>
      </TopBar>

      <div className="flex-1 overflow-auto p-6">
        <div className="mx-auto grid max-w-3xl gap-4">
          <Card>
            <CardHeader className="flex items-center justify-between">
              <CardTitle className="flex items-center gap-2">
                <Server className="h-4 w-4 text-bds-accent" />
                {st?.instance ?? "Instancia"}
              </CardTitle>
              <Badge variant={meta.variant}>{meta.label}</Badge>
            </CardHeader>
            <CardContent className="space-y-5">
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                <Info icon={<Cpu className="h-4 w-4" />} label="GPU" value={st?.gpu || "—"} />
                <Info
                  icon={<HardDrive className="h-4 w-4" />}
                  label="Máquina"
                  value={st?.machine || "—"}
                />
                <Info
                  icon={<Terminal className="h-4 w-4" />}
                  label="SSH"
                  value={st?.sshHost || "—"}
                />
              </div>

              {st?.message && (
                <div className="flex items-start gap-2 rounded-md border border-bds-border bg-bds-surface2 px-3 py-2 text-xs text-bds-muted">
                  <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-bds-warn" />
                  <span>{st.message}</span>
                </div>
              )}

              <div className="flex flex-wrap gap-2">
                <Button
                  variant="success"
                  onClick={() => power(true)}
                  disabled={!usable || busy || running || st?.status === "STARTING"}
                >
                  <Power className="h-4 w-4" />
                  Encender
                </Button>
                <Button
                  variant="danger"
                  onClick={() => power(false)}
                  disabled={!usable || busy || stopped || st?.status === "STOPPING"}
                >
                  <PowerOff className="h-4 w-4" />
                  Apagar
                </Button>
              </div>
            </CardContent>
          </Card>

          {st && !st.installed && (
            <Card>
              <CardContent className="space-y-2 py-4 text-sm text-bds-muted">
                <p className="font-medium text-bds-fg">Brev no está instalado</p>
                <p>
                  Instala la CLI de Brev para controlar la GPU desde aquí. Si ya la
                  tienes, define <code className="text-bds-accent">BREV_PATH</code> en
                  los ajustes del sistema.
                </p>
              </CardContent>
            </Card>
          )}

          {st && st.installed && !st.loggedIn && (
            <Card>
              <CardContent className="space-y-2 py-4 text-sm text-bds-muted">
                <p className="font-medium text-bds-fg">Inicia sesión en Brev</p>
                <p>
                  Ejecuta <code className="text-bds-accent">brev login</code> en la
                  terminal y vuelve a pulsar “Actualizar”.
                </p>
              </CardContent>
            </Card>
          )}

          {running && (
            <Card>
              <CardContent className="space-y-2 py-4 text-sm text-bds-muted">
                <p className="font-medium text-bds-fg">Conéctate</p>
                <p>
                  El servidor está encendido. Conéctate por SSH con{" "}
                  <code className="text-bds-accent">ssh {st?.sshHost}</code> o abre la
                  instancia en el navegador con{" "}
                  <code className="text-bds-accent">brev open {st?.instance}</code>.
                </p>
              </CardContent>
            </Card>
          )}

          <p className="text-center text-xs text-bds-muted/70">
            Puedes cambiar la instancia que controla la app en Ajustes → GPU.
          </p>
        </div>
      </div>
    </div>
  );
}

function Info({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="rounded-md border border-bds-border bg-bds-surface2 px-3 py-2">
      <div className="flex items-center gap-1.5 text-xs text-bds-muted">
        {icon}
        {label}
      </div>
      <div className="mt-1 truncate text-sm font-medium text-bds-fg">{value}</div>
    </div>
  );
}
