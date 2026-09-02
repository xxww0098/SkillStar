import { Check, ChevronDown, Plus, Store, X } from "lucide-react";
import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
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
      <SettingsSectionHeader
        icon={<Store className="h-4 w-4" />}
        title={t("settings.marketplaceMirror")}
        meta={
          mirrorConfig.enabled && hosts.length > 0 ? (
            <span className="max-w-[260px] truncate rounded-md border border-border bg-muted/50 px-2 py-0.5 text-xs text-muted-foreground">
              {hosts[0]}
              {hosts.length > 1 ? ` +${hosts.length - 1}` : ""}
            </span>
          ) : undefined
        }
        action={
          ready ? (
            <Switch
              checked={mirrorConfig.enabled}
              onCheckedChange={(checked) => onConfigChange({ ...mirrorConfig, enabled: checked })}
              disabled={mirrorSaving}
              aria-label={t("settings.marketplaceMirrorEnable")}
            />
          ) : (
            <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
          )
        }
      />

      <div
        className={cn(
          "overflow-hidden rounded-xl border border-border transition-colors",
          mirrorConfig.enabled ? "bg-card" : "bg-card/50",
        )}
      >
        <button
          type="button"
          onClick={onToggleExpanded}
          aria-expanded={mirrorExpanded}
          className="flex w-full cursor-pointer items-center justify-between px-4 py-3 transition-colors hover:bg-muted/30 focus-ring"
        >
          <span className="text-sm font-medium text-foreground">{t("settings.marketplaceMirrorConfig")}</span>
          <div className="flex items-center gap-2">
            {mirrorSaved && <Check className="h-4 w-4 text-success" aria-hidden />}
            <ChevronDown
              aria-hidden
              className={cn(
                "h-4 w-4 text-muted-foreground transition-transform duration-200",
                !mirrorExpanded && "-rotate-90",
              )}
            />
          </div>
        </button>

        {mirrorExpanded && (
          <div className="space-y-3 border-t border-border px-4 pt-1 pb-4">
            <p className="px-1 text-xs leading-relaxed text-muted-foreground">
              {t("settings.marketplaceMirrorNotice")}
            </p>

            <div className="space-y-2">
              {hosts.map((host) => (
                <div key={host} className="flex items-center gap-2 rounded-lg border border-border px-3 py-2">
                  <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">{host}</span>
                  <button
                    type="button"
                    onClick={() => removeHost(host)}
                    aria-label={t("settings.marketplaceMirrorRemove", { host })}
                    className="shrink-0 cursor-pointer rounded-md p-1 text-muted-foreground transition-colors hover:bg-muted/20 hover:text-foreground focus-ring"
                  >
                    <X className="h-3.5 w-3.5" aria-hidden />
                  </button>
                </div>
              ))}
              {hosts.length === 0 && (
                <p className="px-1 text-xs text-muted-foreground">{t("settings.marketplaceMirrorEmpty")}</p>
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
                className="inline-flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md border border-border px-3 py-2 text-xs text-foreground transition-colors hover:bg-muted/20 focus-ring disabled:cursor-not-allowed disabled:opacity-40"
              >
                <Plus className="h-3.5 w-3.5" aria-hidden />
                {t("settings.marketplaceMirrorAdd")}
              </button>
            </div>
          </div>
        )}
      </div>
    </section>
  );
});
