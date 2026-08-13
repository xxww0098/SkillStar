import { Check, ChevronDown, Plus, X } from "lucide-react";
import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import type { MarketplaceMirrorConfig } from "../../../types";

interface MarketplaceMirrorSectionProps {
  mirrorConfig: MarketplaceMirrorConfig;
  ready: boolean;
  mirrorExpanded: boolean;
  mirrorSaving: boolean;
  mirrorSaved: boolean;
  onToggleExpanded: () => void;
  onConfigChange: (next: MarketplaceMirrorConfig) => void;
}

const EMPTY_HOST = "";

/**
 * Marketplace (skills.sh) acceleration mirrors.
 *
 * The marketplace fetch chain tries `https://skills.sh` first, then each
 * configured mirror host in order — so a blocked or poisoned primary host
 * never takes the whole store offline. Mirrors are intermediaries; only add
 * hosts you trust to serve the same content.
 */
export const MarketplaceMirrorSection = memo(function MarketplaceMirrorSection({
  mirrorConfig,
  ready,
  mirrorExpanded,
  mirrorSaving,
  mirrorSaved,
  onToggleExpanded,
  onConfigChange,
}: MarketplaceMirrorSectionProps) {
  const { t } = useTranslation();
  // Local draft of the new-host input; committed on Enter/blur.
  const [draft, setDraft] = useState(EMPTY_HOST);

  const hosts = mirrorConfig.hosts ?? [];

  const commitDraft = () => {
    const trimmed = draft.trim().replace(/\/+$/, "");
    if (!trimmed || !trimmed.startsWith("https://")) {
      setDraft(EMPTY_HOST);
      return;
    }
    if (!hosts.some((host) => host === trimmed)) {
      onConfigChange({ ...mirrorConfig, hosts: [...hosts, trimmed] });
    }
    setDraft(EMPTY_HOST);
  };

  const removeHost = (host: string) => {
    onConfigChange({
      ...mirrorConfig,
      hosts: hosts.filter((existing) => existing !== host),
    });
  };

  return (
    <section>
      <button
        type="button"
        onClick={onToggleExpanded}
        className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer"
      >
        <span className="text-sm font-medium text-foreground">{t("settings.marketplaceMirrorConfig")}</span>
        <div className="flex items-center gap-2">
          {mirrorSaved && <Check className="w-4 h-4 text-emerald-500" />}
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !mirrorExpanded && "-rotate-90",
            )}
          />
        </div>
      </button>

      {mirrorExpanded && (
        <div className="px-4 pb-4 pt-1 border-t border-border space-y-3">
          <p className="text-xs text-muted-foreground leading-relaxed px-1">{t("settings.marketplaceMirrorNotice")}</p>

          <div className="flex items-center justify-between px-1">
            <label className="text-sm text-foreground">{t("settings.marketplaceMirrorEnable")}</label>
            <Switch
              disabled={!ready || mirrorSaving}
              checked={mirrorConfig.enabled}
              onCheckedChange={(checked) => onConfigChange({ ...mirrorConfig, enabled: checked })}
              aria-label={t("settings.marketplaceMirrorEnable")}
            />
          </div>

          <div className="space-y-2">
            {hosts.map((host) => (
              <div key={host} className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border">
                <span className="flex-1 min-w-0 text-xs text-muted-foreground truncate">{host}</span>
                <button
                  type="button"
                  onClick={() => removeHost(host)}
                  aria-label={t("settings.marketplaceMirrorRemove", { host })}
                  className="shrink-0 p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/20 transition-colors cursor-pointer"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
            ))}
            {hosts.length === 0 && (
              <p className="text-xs text-muted-foreground px-1">{t("settings.marketplaceMirrorEmpty")}</p>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Input
              type="text"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  commitDraft();
                }
              }}
              onBlur={commitDraft}
              placeholder="https://skills-mirror.example/"
              aria-label={t("settings.marketplaceMirrorAddPlaceholder")}
            />
            <button
              type="button"
              onClick={commitDraft}
              disabled={!draft.trim().startsWith("https://")}
              className="shrink-0 inline-flex items-center gap-1.5 px-3 py-2 rounded-md border border-border text-xs text-foreground hover:bg-muted/20 transition-colors cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
            >
              <Plus className="w-3.5 h-3.5" />
              {t("settings.marketplaceMirrorAdd")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
});
