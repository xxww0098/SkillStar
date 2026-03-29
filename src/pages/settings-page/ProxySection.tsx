import { useTranslation } from "react-i18next";
import { ChevronDown, Globe } from "lucide-react";
import { Input } from "../../components/ui/input";
import { Switch } from "../../components/ui/switch";
import { cn } from "../../lib/utils";
import type { ProxyConfig } from "../../types";

interface ProxySectionProps {
  proxyConfig: ProxyConfig;
  ready: boolean;
  proxyExpanded: boolean;
  proxySaving: boolean;
  proxySaved: boolean;
  onToggleExpanded: () => void;
  onConfigChange: (next: ProxyConfig) => void;
}

export function ProxySection({
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
    "flex h-9 w-full rounded-xl border border-input-border bg-input backdrop-blur-sm px-3 text-sm text-foreground shadow-sm transition-all duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40 focus-visible:border-primary/60";

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-orange-500/10 flex items-center justify-center shrink-0 border border-orange-500/20">
            <Globe className="w-4 h-4 text-orange-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.networkProxy")}</h2>
          {proxyConfig.enabled && proxyConfig.host && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border">
              {proxyConfig.proxy_type.toUpperCase()}://{proxyConfig.host}:{proxyConfig.port}
            </span>
          )}
        </div>
        
        {ready ? (
          <Switch
            checked={proxyConfig.enabled}
            onCheckedChange={(checked) => onConfigChange({ ...proxyConfig, enabled: checked })}
            disabled={proxySaving}
          />
        ) : (
          <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
        )}
      </div>

      <div className={cn("rounded-xl border border-border overflow-hidden transition-colors", proxyConfig.enabled ? "bg-card" : "bg-card/50")}>
        <button onClick={onToggleExpanded} className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer">
          <span className="text-sm font-medium text-foreground">{t("settings.proxyConfigTitle", { defaultValue: "Proxy Configuration" })}</span>
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !proxyExpanded && "-rotate-90"
            )}
          />
        </button>

        {proxyExpanded && (
          <div className="px-4 pb-4 pt-1 border-t border-border space-y-3">
            <div className="grid grid-cols-[120px_1fr_80px] gap-3">
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyType")}
                </label>
                <select
                  value={proxyConfig.proxy_type}
                  onChange={(e) => onConfigChange({ ...proxyConfig, proxy_type: e.target.value })}
                  className={`${formControlClass} pr-8`}
                >
                  <option value="http">HTTP</option>
                  <option value="https">HTTPS</option>
                  <option value="socks5">SOCKS5</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyHost")}
                </label>
                <Input
                  type="text"
                  value={proxyConfig.host}
                  onChange={(e) => onConfigChange({ ...proxyConfig, host: e.target.value })}
                  placeholder="127.0.0.1"
                />
              </div>
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyPort")}
                </label>
                <Input
                  type="number"
                  value={proxyConfig.port}
                  onChange={(e) =>
                    onConfigChange({ ...proxyConfig, port: parseInt(e.target.value, 10) || 7897 })
                  }
                  placeholder="7897"
                />
              </div>
            </div>

            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyUsername")}
                </label>
                <Input
                  type="text"
                  value={proxyConfig.username || ""}
                  onChange={(e) =>
                    onConfigChange({ ...proxyConfig, username: e.target.value || null })
                  }
                  placeholder={t("common.optional")}
                />
              </div>
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyPassword")}
                </label>
                <Input
                  type="password"
                  value={proxyConfig.password || ""}
                  onChange={(e) =>
                    onConfigChange({ ...proxyConfig, password: e.target.value || null })
                  }
                  placeholder={t("common.optional")}
                />
              </div>
              <div>
                <label className="text-xs text-muted-foreground block mb-1">
                  {t("settings.proxyBypass")}
                </label>
                <Input
                  type="text"
                  value={proxyConfig.bypass || ""}
                  onChange={(e) => onConfigChange({ ...proxyConfig, bypass: e.target.value || null })}
                  placeholder={t("settings.proxyBypassPlaceholder")}
                />
              </div>
            </div>

            <div className="flex items-center justify-end min-h-5">
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
}
