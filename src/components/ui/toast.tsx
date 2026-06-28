import * as React from "react";
import { cn } from "@/lib/utils";
import { CheckCircle2, AlertTriangle, Info, XCircle } from "lucide-react";

type ToastKind = "success" | "error" | "info" | "warn";
interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
}

const ToastCtx = React.createContext<{
  push: (kind: ToastKind, message: string) => void;
}>({ push: () => {} });

export function useToast() {
  return React.useContext(ToastCtx);
}

export function ToastProvider({ children }: { children: React.ReactNode }) {
  const [toasts, setToasts] = React.useState<Toast[]>([]);

  const push = React.useCallback((kind: ToastKind, message: string) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, kind, message }]);
    setTimeout(() => {
      setToasts((t) => t.filter((x) => x.id !== id));
    }, 4200);
  }, []);

  return (
    <ToastCtx.Provider value={{ push }}>
      {children}
      <div className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-[340px] flex-col gap-2">
        {toasts.map((t) => {
          const Icon = {
            success: CheckCircle2,
            error: XCircle,
            info: Info,
            warn: AlertTriangle,
          }[t.kind];
          const color = {
            success: "text-bds-good",
            error: "text-bds-bad",
            info: "text-bds-info",
            warn: "text-bds-warn",
          }[t.kind];
          return (
            <div
              key={t.id}
              className="glass pointer-events-auto flex items-start gap-3 rounded-lg px-4 py-3 shadow-2xl animate-fade-in"
            >
              <Icon className={cn("mt-0.5 h-4 w-4 shrink-0", color)} />
              <p className="text-sm leading-snug text-bds-fg">{t.message}</p>
            </div>
          );
        })}
      </div>
    </ToastCtx.Provider>
  );
}
