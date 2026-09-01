import { ChevronDown, Globe } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "../../../components/ui/input";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import type { ProxyConfig, ProxyType } from "../../../types";

interface ProxySectionProps {
  proxyConfig: ProxyConfig;
  ready: boolean;
  proxyExpanded: boolean;
  proxySaving: boolean;
  proxySaved: boolean;
  onToggleExpanded: () => void;
  onConfigChange: (next: ProxyConfig) => void;
}

export const ProxySection = memo(function ProxySection({
  proxyConfig,
  ready,
  proxyExpanded,
  proxySaving,
  proxySaved,
  onToggleExpanded,
  onConfigChange,
}: ProxySectionProps) {
  const { t } = useTranslation();
  const formControlClass =
    "flex h-9 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";

  return (
    <section>
      <SettingsSectionHeader
        icon={<Globe className="h-4 w-4" />}
        title={t("settings.networkProxy")}
        meta={
          proxyConfig.enabled && proxyConfig.host ? (
            <span className="max-w-[260px] truncate rounded-md border border-border bg-muted/50 px-2 py-0.5 text-xs text-muted-foreground">
              {proxyConfig.proxy_type.toUpperCase()}://{proxyConfig.host}:{proxyConfig.port}
            </span>
          ) : undefined
        }
        action={
          ready ? (
            <Switch
              checked={proxyConfig.enabled}
              onCheckedChange={(checked) => onConfigChange({ ...proxyConfig, enabled: checked })}
              disabled={proxySaving}
              aria-label={t("settings.networkProxy")}
            />
          ) : (
            <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
          )
        }
      />

      <div
        className={cn(
          "overflow-hidden rounded-xl border border-border transition-colors",
          proxyConfig.enabled ? "bg-card" : "bg-card/50",
        )}
      >
        <button
          type="button"
          onClick={onToggleExpanded}
          aria-expanded={proxyExpanded}
          className="flex w-full cursor-pointer items-center justify-between px-4 py-3 transition-colors hover:bg-muted/30 focus-ring"
        >
          <span className="text-sm font-medium text-foreground">
            {t("settings.proxyConfigTitle", { defaultValue: "Proxy Configuration" })}
          </span>
          <ChevronDown
            aria-hidden
            className={cn(
              "h-4 w-4 text-muted-foreground transition-transform duration-200",
              !proxyExpanded && "-rotate-90",
            )}
          />
        </button>

        {proxyExpanded && (
          <div className="space-y-3 border-t border-border px-4 pt-1 pb-4">
            <div className="grid grid-cols-[120px_1fr_80px] gap-3">
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyType")}</label>
                <select
                  value={proxyConfig.proxy_type}
                  onChange={(e) =>
                    onConfigChange({
                      ...proxyConfig,
                      proxy_type: e.target.value as ProxyType,
                    })
                  }
                  className={`${formControlClass} pr-8`}
                >
                  <option value="http">HTTP</option>
                  <option value="https">HTTPS</option>
                  <option value="socks5">SOCKS5</option>
                  <option value="socks5h">SOCKS5H</option>
                </select>
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyHost")}</label>
                <Input
                  type="text"
                  value={proxyConfig.host}
                  onChange={(e) => onConfigChange({ ...proxyConfig, host: e.target.value })}
                  placeholder="127.0.0.1"
                />
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyPort")}</label>
                <Input
                  type="number"
                  value={proxyConfig.port}
                  onChange={(e) => onConfigChange({ ...proxyConfig, port: parseInt(e.target.value, 10) || 7897 })}
                  placeholder="7897"
                />
              </div>
            </div>

            {proxyConfig.proxy_type === "socks5" || proxyConfig.proxy_type === "socks5h" ? (
              <p className="px-1 text-xs leading-relaxed text-muted-foreground">{t("settings.proxySocks5hHint")}</p>
            ) : null}

            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyUsername")}</label>
                <Input
                  type="text"
                  value={proxyConfig.username || ""}
                  onChange={(e) => onConfigChange({ ...proxyConfig, username: e.target.value || null })}
                  placeholder={t("common.optional")}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyPassword")}</label>
                <Input
                  type="password"
                  value={proxyConfig.password || ""}
                  onChange={(e) => onConfigChange({ ...proxyConfig, password: e.target.value || null })}
                  placeholder={t("common.optional")}
                />
              </div>
              <div>
                <label className="mb-1 block text-xs text-muted-foreground">{t("settings.proxyBypass")}</label>
                <Input
                  type="text"
                  value={proxyConfig.bypass || ""}
                  onChange={(e) => onConfigChange({ ...proxyConfig, bypass: e.target.value || null })}
                  placeholder={t("settings.proxyBypassPlaceholder")}
                />
              </div>
            </div>

            <div className="flex min-h-5 items-center justify-end">
              {proxySaving ? (
                <span className="text-xs text-muted-foreground">{t("common.saving")}</span>
              ) : proxySaved ? (
                <span className="text-xs text-success">{t("common.saved")}</span>
              ) : null}
            </div>
          </div>
        )}
      </div>
    </section>
  );
});
