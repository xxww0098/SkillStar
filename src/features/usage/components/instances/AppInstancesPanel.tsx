import { Play, Plus, Square, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useAppInstances } from "../../hooks/useAppInstances";
import type { DesktopAppId } from "../../types";

export function AppInstancesPanel({ appId, compact = false }: { appId: DesktopAppId; compact?: boolean }) {
  const { t } = useTranslation();
  const { instances, loading, busyId, error, create, start, stop, remove } = useAppInstances(appId);
  const [name, setName] = useState("");
  const [deleteId, setDeleteId] = useState<string | null>(null);

  const handleCreate = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    try {
      await create(trimmed);
      setName("");
    } catch (err) {
      toast.error(t("usage.instanceCreateFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const handleStart = async (id: string, instanceName: string) => {
    try {
      await start(id);
      toast.success(t("usage.instanceStarted", { name: instanceName }));
    } catch (err) {
      toast.error(t("usage.instanceStartFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const handleStop = async (id: string, instanceName: string) => {
    try {
      await stop(id);
      toast.success(t("usage.instanceStoppedToast", { name: instanceName }));
    } catch (err) {
      toast.error(t("usage.instanceStopFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await remove(id);
      setDeleteId(null);
    } catch (err) {
      toast.error(t("usage.instanceDeleteFailed"), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  };

  return (
    <div className={cn("flex min-w-0 flex-col gap-2", compact ? "text-[11px]" : "text-xs")}>
      <p className="text-[11px] leading-relaxed text-muted-foreground">{t("usage.instancesHint")}</p>
      {error ? <p className="text-[11px] text-red-500">{error}</p> : null}
      {loading && instances.length === 0 ? (
        <p className="text-muted-foreground">{t("common.loading")}</p>
      ) : instances.length === 0 ? (
        <p className="text-muted-foreground">{t("usage.noInstances")}</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {instances.map((row) => (
            <li
              key={row.id}
              className="flex min-w-0 items-center gap-2 rounded-lg border border-border/70 bg-background/70 px-2 py-1.5"
            >
              <span
                className={cn("h-1.5 w-1.5 shrink-0 rounded-full", row.running ? "bg-emerald-500" : "bg-zinc-400")}
                aria-hidden
              />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium text-foreground">{row.name}</p>
                <p className="truncate text-[10px] text-muted-foreground">
                  {row.running ? t("usage.instanceRunning") : t("usage.instanceStopped")}
                </p>
              </div>
              {row.running ? (
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  title={t("usage.stopInstance")}
                  aria-label={t("usage.stopInstance")}
                  disabled={busyId === row.id}
                  onClick={() => void handleStop(row.id, row.name)}
                >
                  <Square className="h-3.5 w-3.5" aria-hidden />
                </Button>
              ) : (
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  title={t("usage.startInstance")}
                  aria-label={t("usage.startInstance")}
                  disabled={busyId === row.id}
                  onClick={() => void handleStart(row.id, row.name)}
                >
                  <Play className="h-3.5 w-3.5" aria-hidden />
                </Button>
              )}
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                title={t("usage.deleteInstance")}
                aria-label={t("usage.deleteInstance")}
                disabled={busyId === row.id || row.running}
                onClick={() => setDeleteId(row.id)}
                className="text-red-600 hover:text-red-700"
              >
                <Trash2 className="h-3.5 w-3.5" aria-hidden />
              </Button>
            </li>
          ))}
        </ul>
      )}
      {deleteId ? (
        <div className="flex items-center justify-between gap-2 rounded-md border border-red-200 bg-red-50 px-2 py-1.5 text-[11px] text-red-800">
          <span className="min-w-0 truncate">
            {t("usage.confirmDeleteInstance", {
              name: instances.find((row) => row.id === deleteId)?.name ?? deleteId,
            })}
          </span>
          <span className="flex shrink-0 gap-1">
            <Button type="button" size="sm" variant="ghost" onClick={() => setDeleteId(null)}>
              {t("common.cancel")}
            </Button>
            <Button type="button" size="sm" variant="destructive" onClick={() => void handleDelete(deleteId)}>
              {t("common.delete")}
            </Button>
          </span>
        </div>
      ) : null}
      <form
        className="flex min-w-0 items-center gap-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          void handleCreate();
        }}
      >
        <input
          type="text"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t("usage.instanceNamePlaceholder")}
          aria-label={t("usage.instanceNamePlaceholder")}
          className="h-8 min-w-0 flex-1 rounded-md border border-border/70 bg-background px-2 text-xs"
        />
        <Button type="submit" size="sm" variant="outline" disabled={!name.trim() || busyId === "create"}>
          <Plus className="h-3.5 w-3.5" aria-hidden />
          {t("usage.createInstance")}
        </Button>
      </form>
    </div>
  );
}
