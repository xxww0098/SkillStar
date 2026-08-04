import { AlertTriangle, Archive, Loader2, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { ModalCloseButton, ModalShell } from "../../../components/ui/ModalShell";
import type { SkillUpdateBlocked } from "../../../types";

interface LocalDivergenceDialogProps {
  blocked: SkillUpdateBlocked | null;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onPreserve: (localName: string) => void;
  onDiscard: () => void;
}

export function LocalDivergenceDialog({
  blocked,
  busy,
  error,
  onClose,
  onPreserve,
  onDiscard,
}: LocalDivergenceDialogProps) {
  const { t } = useTranslation();
  const [localName, setLocalName] = useState("");

  useEffect(() => {
    setLocalName(blocked?.suggested_local_name ?? "");
  }, [blocked]);

  const normalizedName = localName.trim();

  return (
    <ModalShell
      open={blocked !== null}
      onClose={onClose}
      ariaLabel={t("mySkills.localDivergenceTitle")}
      role="alertdialog"
      panelClassName="max-w-lg px-4"
      dismissable={!busy}
    >
      <div className="flex items-start justify-between gap-4 px-6 pt-5">
        <div className="flex items-start gap-3">
          <div className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-amber-500/10 text-amber-600 dark:text-amber-400">
            <AlertTriangle className="h-5 w-5" />
          </div>
          <div className="space-y-1">
            <h2 className="text-heading-sm">{t("mySkills.localDivergenceTitle")}</h2>
            <p className="text-caption leading-5">
              {t("mySkills.localDivergenceDescription", { name: blocked?.name })}
            </p>
          </div>
        </div>
        <ModalCloseButton onClose={onClose} disabled={busy} />
      </div>

      <div className="space-y-4 px-6 py-5">
        <div className="space-y-2">
          <label htmlFor="local-divergence-copy-name" className="text-xs font-medium text-foreground">
            {t("mySkills.localCopyName")}
          </label>
          <Input
            id="local-divergence-copy-name"
            value={localName}
            onChange={(event) => setLocalName(event.target.value)}
            disabled={busy}
            autoComplete="off"
          />
          <p className="text-micro leading-4 text-muted-foreground">{t("mySkills.localCopyHint")}</p>
        </div>

        <div className="rounded-xl border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
          {t("mySkills.discardDivergenceWarning")}
        </div>

        {error && (
          <div className="rounded-xl border border-destructive/20 px-3 py-2 text-xs text-destructive">{error}</div>
        )}
      </div>

      <div className="flex flex-wrap items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5">
        <Button variant="destructive" size="sm" onClick={onDiscard} disabled={busy}>
          <Trash2 className="h-3.5 w-3.5" />
          {t("mySkills.discardAndUpdate")}
        </Button>
        <Button size="sm" onClick={() => onPreserve(normalizedName)} disabled={busy || normalizedName.length === 0}>
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Archive className="h-3.5 w-3.5" />}
          {t("mySkills.preserveAndUpdate")}
        </Button>
      </div>
    </ModalShell>
  );
}
