import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, KeyRound } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

export function OpenCodeQuickLinks() {
  const { t } = useTranslation();
  const [busy, setBusy] = useState<"config" | "auth" | null>(null);

  const openConfig = useCallback(async () => {
    if (busy) return;
    setBusy("config");
    try {
      await invoke("open_opencode_config_dir");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [busy]);

  const openAuth = useCallback(async () => {
    if (busy) return;
    setBusy("auth");
    try {
      await invoke("open_opencode_auth_dir");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, [busy]);

  return (
    <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between rounded-xl border border-border/60 bg-muted/20 px-3 py-2.5">
      <p className="text-xs text-muted-foreground leading-relaxed">{t("modelPage.openCodeHint")}</p>
      <div className="flex flex-wrap items-center gap-2 shrink-0">
        <button
          type="button"
          onClick={() => void openConfig()}
          disabled={busy !== null}
          className="inline-flex items-center gap-1.5 rounded-lg border border-border/70 bg-background/80 px-2.5 py-1.5 text-[11px] font-medium text-foreground hover:bg-muted/60 transition-colors disabled:opacity-50"
        >
          <FolderOpen className="w-3.5 h-3.5 text-muted-foreground" />
          {busy === "config" ? t("common.loading") : t("modelPage.openCodeConfigFolder")}
        </button>
        <button
          type="button"
          onClick={() => void openAuth()}
          disabled={busy !== null}
          className="inline-flex items-center gap-1.5 rounded-lg border border-border/70 bg-background/80 px-2.5 py-1.5 text-[11px] font-medium text-foreground hover:bg-muted/60 transition-colors disabled:opacity-50"
        >
          <KeyRound className="w-3.5 h-3.5 text-muted-foreground" />
          {busy === "auth" ? t("common.loading") : t("modelPage.openCodeAuthFolder")}
        </button>
      </div>
    </div>
  );
}
