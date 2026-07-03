import { motion } from "framer-motion";
import { AlertCircle, CheckCircle2, Loader2, MonitorCog, RefreshCw, RotateCcw, Send } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { fingerprintsApi } from "../api";
import type { FingerprintRow, SupportedIde } from "../types";

interface IdeProjectorsPanelProps {
  /**
   * Available fingerprints — passed in by the parent so this panel
   * doesn't re-fetch the list it shares with `FingerprintsPanel`.
   */
  fingerprints: FingerprintRow[];
  activeId: string | null;
}

/**
 * Phase 6 — write a fingerprint's IDE telemetry to each supported IDE's
 * on-disk `storage.json`. Per row:
 *   - status pill (installed / not installed / restored)
 *   - dropdown picking which fingerprint to project
 *   - "Apply" button + "Restore baseline" button (when baseline exists)
 */
export function IdeProjectorsPanel({ fingerprints, activeId }: IdeProjectorsPanelProps) {
  const { t } = useTranslation();
  const [ides, setIdes] = useState<SupportedIde[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // Per-IDE busy flag so multiple rows can disable independently.
  const [busy, setBusy] = useState<Record<string, boolean>>({});
  // Per-IDE selected fingerprint id (defaults to active or "original").
  const [picks, setPicks] = useState<Record<string, string>>({});

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await fingerprintsApi.listIdes();
      setIdes(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  const onApply = async (agentId: string) => {
    const fpId = picks[agentId] ?? activeId ?? "original";
    setBusy((b) => ({ ...b, [agentId]: true }));
    try {
      const updated = await fingerprintsApi.applyToIde(agentId, fpId);
      setIdes((prev) => prev.map((r) => (r.agentId === agentId ? updated : r)));
      const fpLabel = fingerprints.find((f) => f.id === fpId)?.name ?? fpId;
      toast.success(t("fingerprints.idePanel.applySuccess", { name: updated.displayName }), {
        description: t("fingerprints.idePanel.applySuccessDescription", { fingerprint: fpLabel }),
      });
    } catch (e) {
      toast.error(t("fingerprints.idePanel.applyFailed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusy((b) => ({ ...b, [agentId]: false }));
    }
  };

  const onRestore = async (agentId: string) => {
    if (!window.confirm(t("fingerprints.idePanel.restoreConfirm", { agent: agentId }))) return;
    setBusy((b) => ({ ...b, [agentId]: true }));
    try {
      const updated = await fingerprintsApi.restoreIde(agentId);
      setIdes((prev) => prev.map((r) => (r.agentId === agentId ? updated : r)));
      toast.success(t("fingerprints.idePanel.restoreSuccess", { name: updated.displayName }));
    } catch (e) {
      toast.error(t("fingerprints.idePanel.restoreFailed"), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusy((b) => ({ ...b, [agentId]: false }));
    }
  };

  return (
    <div className="space-y-3">
      <header className="flex items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2 text-sm font-semibold">
            <MonitorCog className="h-4 w-4 text-sky-500" />
            {t("fingerprints.idePanel.title")}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("fingerprints.idePanel.descriptionBefore")}{" "}
            <code className="rounded bg-zinc-100 px-1 py-0.5 text-[10px]">storage.json</code>
            {t("fingerprints.idePanel.descriptionAfter")}
          </p>
        </div>
        <Button onClick={reload} size="sm" variant="ghost" className="shrink-0" disabled={loading}>
          <RefreshCw className={cn("mr-1.5 h-3.5 w-3.5", loading && "animate-spin")} />
          {t("fingerprints.idePanel.refresh")}
        </Button>
      </header>

      {error && (
        <div className="flex items-start gap-2 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          {error}
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 rounded-xl border border-zinc-200/60 bg-white/60 px-3 py-4 text-sm text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("fingerprints.idePanel.scanning")}
        </div>
      ) : (
        <div className="space-y-2">
          {ides.map((row) => (
            <IdeRow
              key={row.agentId}
              row={row}
              fingerprints={fingerprints}
              activeId={activeId}
              selected={picks[row.agentId] ?? activeId ?? "original"}
              onPick={(id) => setPicks((p) => ({ ...p, [row.agentId]: id }))}
              busy={busy[row.agentId] === true}
              onApply={() => onApply(row.agentId)}
              onRestore={() => onRestore(row.agentId)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface IdeRowProps {
  row: SupportedIde;
  fingerprints: FingerprintRow[];
  activeId: string | null;
  selected: string;
  busy: boolean;
  onPick: (id: string) => void;
  onApply: () => void;
  onRestore: () => void;
}

function IdeRow({ row, fingerprints, activeId, selected, busy, onPick, onApply, onRestore }: IdeRowProps) {
  const { t } = useTranslation();
  return (
    <motion.div
      layout
      className={cn(
        "flex flex-col gap-2 rounded-2xl border bg-white/90 px-3.5 py-3 backdrop-blur-sm md:flex-row md:items-center",
        row.installed ? "border-zinc-200/70" : "border-dashed border-zinc-300/70 opacity-70",
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="font-medium">{row.displayName}</span>
          {row.installed ? (
            <span className="inline-flex items-center gap-1 rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] font-medium text-emerald-700">
              <CheckCircle2 className="h-3 w-3" />
              {t("fingerprints.idePanel.installed")}
            </span>
          ) : (
            <span className="rounded-full bg-zinc-100 px-2 py-0.5 text-[10px] font-medium text-zinc-500">
              {t("fingerprints.idePanel.notInstalled")}
            </span>
          )}
          {row.hasBaseline && (
            <span className="rounded-full bg-amber-50 px-2 py-0.5 text-[10px] font-medium text-amber-700">
              {t("fingerprints.idePanel.backedUp")}
            </span>
          )}
        </div>
        {row.storagePath && (
          <div className="mt-0.5 truncate font-mono text-[10px] text-zinc-500">{row.storagePath}</div>
        )}
        {row.current?.machine_id && (
          <div className="mt-1 truncate font-mono text-[10px] text-zinc-600">
            {t("fingerprints.idePanel.currentMachineIdLabel")}{" "}
            <span className="text-zinc-800">{row.current.machine_id}</span>
          </div>
        )}
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-1.5">
        <select
          className="h-7 rounded-md border border-zinc-200 bg-white px-1.5 text-xs"
          value={selected}
          onChange={(e) => onPick(e.target.value)}
          disabled={!row.installed || busy}
          aria-label={t("fingerprints.idePanel.selectFingerprintAria")}
        >
          {fingerprints.map((fp) => (
            <option key={fp.id} value={fp.id}>
              {fp.name}
              {fp.id === activeId ? t("fingerprints.idePanel.activeSuffix") : ""}
            </option>
          ))}
        </select>
        <Button size="sm" variant="default" className="h-7 text-xs" onClick={onApply} disabled={!row.installed || busy}>
          {busy ? <Loader2 className="mr-1 h-3 w-3 animate-spin" /> : <Send className="mr-1 h-3 w-3" />}
          {t("fingerprints.idePanel.apply")}
        </Button>
        <Button
          size="sm"
          variant="outline"
          className="h-7 text-xs"
          onClick={onRestore}
          disabled={!row.installed || !row.hasBaseline || busy}
          title={
            row.hasBaseline
              ? t("fingerprints.idePanel.restoreTitleAvailable")
              : t("fingerprints.idePanel.restoreTitleUnavailable")
          }
        >
          <RotateCcw className="mr-1 h-3 w-3" />
          {t("fingerprints.idePanel.restore")}
        </Button>
      </div>
    </motion.div>
  );
}
