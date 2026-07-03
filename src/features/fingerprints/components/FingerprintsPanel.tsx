import { AlertCircle, Fingerprint, Loader2, Plus } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { useFingerprints } from "../hooks/useFingerprints";
import type { FingerprintRow } from "../types";
import { CreateFromPresetDialog } from "./CreateFromPresetDialog";
import { EditFingerprintDialog } from "./EditFingerprintDialog";
import { FingerprintCard } from "./FingerprintCard";
import { IdeProjectorsPanel } from "./IdeProjectorsPanel";

interface FingerprintsPanelProps {
  showHeader?: boolean;
}

/**
 * Top-level panel mounted under Settings → 设备指纹.
 *
 * - Lists all stored fingerprints with the immutable "original" row pinned first.
 * - "+" opens [`CreateFromPresetDialog`] so the user picks a preset → fresh row.
 * - Activate / edit / delete actions live on each [`FingerprintCard`].
 * - [`EditFingerprintDialog`] mounts lazily when a row is selected for editing.
 */
export function FingerprintsPanel({ showHeader = true }: FingerprintsPanelProps = {}) {
  const { t } = useTranslation();
  const { items, activeId, presets, loading, error, createFromPreset, update, remove, setActive } = useFingerprints();
  const [createOpen, setCreateOpen] = useState(false);
  const [editing, setEditing] = useState<FingerprintRow | null>(null);

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(t("fingerprints.deleteConfirm", { name }))) return;
    try {
      await remove(id);
      toast.success(t("fingerprints.deleted"), { description: name });
    } catch (e) {
      toast.error(t("fingerprints.deleteFailed"), { description: e instanceof Error ? e.message : String(e) });
    }
  };

  const handleActivate = async (id: string, name: string) => {
    try {
      await setActive(id);
      toast.success(t("fingerprints.activated"), { description: name });
    } catch (e) {
      toast.error(t("fingerprints.activateFailed"), { description: e instanceof Error ? e.message : String(e) });
    }
  };

  return (
    <div className="space-y-4">
      <header className="flex items-start justify-between gap-3">
        <div>
          {showHeader && (
            <div className="flex items-center gap-2 text-sm font-semibold">
              <Fingerprint className="h-4 w-4 text-violet-500" />
              {t("fingerprints.title")}
            </div>
          )}
          <p className={`${showHeader ? "mt-1" : ""} text-xs text-muted-foreground`}>{t("fingerprints.description")}</p>
        </div>
        <Button onClick={() => setCreateOpen(true)} size="sm" className="shrink-0">
          <Plus className="mr-1.5 h-3.5 w-3.5" />
          {t("fingerprints.newFingerprint")}
        </Button>
      </header>

      {error && (
        <div className="flex items-start gap-2 rounded-xl border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
          <div>{error}</div>
        </div>
      )}

      {loading ? (
        <div className="flex items-center gap-2 rounded-xl border border-zinc-200/60 bg-white/60 px-3 py-4 text-sm text-muted-foreground">
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
          {t("fingerprints.loadingList")}
        </div>
      ) : items.length === 0 ? (
        <div className="rounded-2xl border border-dashed border-zinc-300 bg-white/50 px-4 py-6 text-center text-sm text-muted-foreground">
          {t("fingerprints.empty")}
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-2.5 lg:grid-cols-2">
          {items.map((row) => (
            <FingerprintCard
              key={row.id}
              row={row}
              onSetActive={() => handleActivate(row.id, row.name)}
              onEdit={() => setEditing(row)}
              onDelete={() => handleDelete(row.id, row.name)}
            />
          ))}
        </div>
      )}

      <CreateFromPresetDialog
        open={createOpen}
        presets={presets}
        onClose={() => setCreateOpen(false)}
        onSubmit={async (presetId, name) => {
          await createFromPreset(presetId, name);
        }}
      />

      <EditFingerprintDialog
        open={editing !== null}
        row={editing}
        onClose={() => setEditing(null)}
        onSubmit={async (input) => {
          if (!editing) return;
          await update(editing.id, input);
        }}
      />

      {/* IDE projectors — only after the fingerprint list is loaded so the
          dropdowns inside can resolve names. */}
      {!loading && items.length > 0 && (
        <div className="border-t border-zinc-200/60 pt-4">
          <IdeProjectorsPanel fingerprints={items} activeId={activeId} />
        </div>
      )}
    </div>
  );
}
