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
    case "BUILDING":
    case "PENDING":
      return { variant: "warn", label: "Encendiendo…" };
    case "STOPPING":
      return { variant: "warn", label: "Apagando…" };
    case "DELETING":
      return { variant: "warn", label: "Eliminando…" };
    case "STOPPED":
      return { variant: "default", label: "Apagado" };
    case "FAILED":
      return { variant: "bad", label: "Falló" };
    case "NOT_FOUND":
      return { variant: "bad", label: "No encontrada" };
    case "NOT_LOGGED_IN":
      return { variant: "warn", label: "Sin sesión" };
    case "NO_BREV":
      return { variant: "bad", label: "Brev no instalado" };
    case "EMPTY":
      return { variant: "default", label: "Sin instancias" };
    case "ERROR":
      return { variant: "bad", label: "Error" };
    default:
      return { variant: "default", label: status || "Desconocido" };
  }
}

// A "global" status row carries no instance name and signals a CLI-level
// condition (Brev missing, not logged in, empty org, error).
function isGlobalState(s: GpuServerStatus): boolean {
  return !s.instance;
}

export function GpuServer({ gpu, deps }: SharedProps) {
  const { push } = useToast();
  const [servers, setServers] = useState<GpuServerStatus[]>([]);
  const [busyName, setBusyName] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(
    async (silent = false) => {
      if (!silent) setLoading(true);
      try {
        const list = await api.gpuServerList();
        setServers(list);
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

  // Auto-poll while any instance is transitioning so the badges settle.
  useEffect(() => {
    const transitioning = servers.some(
      (s) =>
        s.status === "STARTING" ||
        s.status === "STOPPING" ||
        s.status === "BUILDING" ||
        s.status === "PENDING" ||
        s.status === "DELETING"
    );
    if (!transitioning) return;
    const t = setTimeout(() => refresh(true), 8000);
    return () => clearTimeout(t);
  }, [servers, refresh]);

  const power = async (name: string, on: boolean) => {
    setBusyName(name);
    try {
      const s = on
        ? await api.gpuServerStartNamed(name)
        : await api.gpuServerStopNamed(name);
      setServers((prev) => prev.map((p) => (p.instance === name ? s : p)));
      push("success", on ? "Servidor encendiéndose" : "Servidor apagándose");
    } catch (e) {
      push("error", String(e));
    } finally {
      setBusyName(null);
    }
  };

  const global = servers.length === 1 && isGlobalState(servers[0]) ? servers[0] : null;
  const instances = servers.filter((s) => !isGlobalState(s));

  return (
    <div className="flex h-full flex-col">
      <TopBar
        title="GPU Servers"
        subtitle="Enciende, apaga y conéctate a tus GPUs remotas (Brev)"
        gpu={gpu}
        deps={deps}
        busy={!!busyName || loading}
      >
        <Button variant="ghost" size="sm" onClick={() => refresh()} disabled={!!busyName}>
          <RefreshCw className={loading ? "h-4 w-4 animate-spin" : "h-4 w-4"} />
          Actualizar
        </Button>
      </TopBar>

      <div className="flex-1 overflow-auto p-6">
        <div className="mx-auto grid max-w-3xl gap-4">
          {global ? (
            <GlobalStateCard state={global} />
          ) : (
            <>
              <p className="text-xs text-bds-muted">
                {instances.length}{" "}
                {instances.length === 1 ? "instancia encontrada" : "instancias encontradas"}{" "}
                en tu organización de Brev.
              </p>
              {instances.map((s) => (
                <ServerCard
                  key={s.instance}
                  s={s}
                  busy={busyName === s.instance}
                  anyBusy={!!busyName}
                  onPower={(on) => power(s.instance, on)}
                />
              ))}
            </>
          )}

          <p className="text-center text-xs text-bds-muted/70">
            Las generaciones de video usan la instancia configurada en Ajustes → GPU.
          </p>
        </div>
      </div>
    </div>
  );
}

function ServerCard({
  s,
  busy,
  anyBusy,
  onPower,
}: {
  s: GpuServerStatus;
  busy: boolean;
  anyBusy: boolean;
  onPower: (on: boolean) => void;
}) {
  const meta = statusMeta(s.status);
  const running = s.status === "RUNNING";
  const stopped = s.status === "STOPPED" || s.status === "FAILED";
  const transitioning =
    s.status === "STARTING" ||
    s.status === "STOPPING" ||
    s.status === "BUILDING" ||
    s.status === "PENDING" ||
    s.status === "DELETING";

  return (
    <Card>
      <CardHeader className="flex items-center justify-between">
        <CardTitle className="flex items-center gap-2">
          <Server className="h-4 w-4 text-bds-accent" />
          {s.instance}
        </CardTitle>
        <Badge variant={meta.variant}>{meta.label}</Badge>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <Info icon={<Cpu className="h-4 w-4" />} label="GPU" value={s.gpu || "—"} />
          <Info
            icon={<HardDrive className="h-4 w-4" />}
            label="Máquina"
            value={s.machine || "—"}
          />
          <Info icon={<Terminal className="h-4 w-4" />} label="SSH" value={s.sshHost || "—"} />
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            variant="success"
            onClick={() => onPower(true)}
            disabled={anyBusy || running || transitioning}
          >
            <Power className="h-4 w-4" />
            {busy ? "…" : "Encender"}
          </Button>
          <Button
            variant="danger"
            onClick={() => onPower(false)}
            disabled={anyBusy || stopped || transitioning}
          >
            <PowerOff className="h-4 w-4" />
            {busy ? "…" : "Apagar"}
          </Button>
        </div>

        {running && (
          <div className="space-y-1 rounded-md border border-bds-border bg-bds-surface2 px-3 py-2 text-xs text-bds-muted">
            <p className="font-medium text-bds-fg">Conéctate</p>
            <p>
              SSH: <code className="text-bds-accent">ssh {s.sshHost}</code> · Navegador:{" "}
              <code className="text-bds-accent">brev open {s.instance}</code>
            </p>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function GlobalStateCard({ state }: { state: GpuServerStatus }) {
  const meta = statusMeta(state.status);
  return (
    <Card>
      <CardHeader className="flex items-center justify-between">
        <CardTitle className="flex items-center gap-2">
          <AlertTriangle className="h-4 w-4 text-bds-warn" />
          Servidores GPU
        </CardTitle>
        <Badge variant={meta.variant}>{meta.label}</Badge>
      </CardHeader>
      <CardContent className="space-y-2 py-4 text-sm text-bds-muted">
        {state.status === "NO_BREV" && (
          <p className="font-medium text-bds-fg">Brev no está instalado</p>
        )}
        {state.status === "NOT_LOGGED_IN" && (
          <p className="font-medium text-bds-fg">Inicia sesión en Brev</p>
        )}
        {state.status === "EMPTY" && (
          <p className="font-medium text-bds-fg">No hay instancias</p>
        )}
        {state.status === "ERROR" && (
          <p className="font-medium text-bds-fg">Error al consultar Brev</p>
        )}
        <p>{state.message || "Sin detalles."}</p>
        {state.status === "NOT_LOGGED_IN" && (
          <p>
            Ejecuta <code className="text-bds-accent">brev login</code> en la terminal y
            pulsa “Actualizar”.
          </p>
        )}
        {state.status === "NO_BREV" && (
          <p>
            Instala la CLI de Brev o define{" "}
            <code className="text-bds-accent">BREV_PATH</code> en los ajustes del sistema.
          </p>
        )}
      </CardContent>
    </Card>
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
