import { Activity, Loader2 } from "lucide-react";
import { memo, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { SettingsSectionHeader } from "../../../components/ui/SettingsSectionHeader";
import { tauriInvoke } from "../../../lib/ipc";
import { cn } from "../../../lib/utils";
import type { NetworkDiagnosis, NetworkHostCheck } from "../../../types";

const REC_I18N: Record<string, string> = {
  enable_github_mirrors: "settings.networkDoctorRecEnableMirrors",
  enable_proxy: "settings.networkDoctorRecEnableProxy",
  use_socks5h: "settings.networkDoctorRecUseSocks5h",
  check_proxy_reachability: "settings.networkDoctorRecCheckProxy",
  use_marketplace_wrap: "settings.networkDoctorRecMarketplaceWrap",
  all_github_paths_blocked: "settings.networkDoctorRecAllBlocked",
};

export const NetworkDoctorSection = memo(function NetworkDoctorSection() {
  const { t } = useTranslation();
  const [running, setRunning] = useState(false);
  const [diagnosis, setDiagnosis] = useState<NetworkDiagnosis | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await tauriInvoke("diagnose_network");
      setDiagnosis(result);
    } catch (err) {
      setDiagnosis(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }, []);

  return (
    <section>
      <SettingsSectionHeader
        icon={<Activity className="h-4 w-4" />}
        title={t("settings.networkDoctor")}
        action={
          <Button size="sm" variant="outline" onClick={() => void run()} disabled={running} aria-busy={running}>
            {running ? <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> : null}
            {t("settings.networkDoctorRun")}
          </Button>
        }
      />

      <div className="rounded-xl border border-border bg-card px-4 py-3 space-y-3">
        <p className="text-xs text-muted-foreground leading-relaxed">{t("settings.networkDoctorHint")}</p>

        {error && <p className="text-xs text-destructive">{error}</p>}

        {diagnosis && (
          <div className="space-y-3">
            <ul className="space-y-1.5">
              {diagnosis.checks.map((check) => (
                <CheckRow key={check.id} check={check} />
              ))}
            </ul>
            {diagnosis.recommendations.length > 0 && (
              <div className="rounded-lg border border-amber-500/20 bg-amber-500/5 px-3 py-2 space-y-1">
                <p className="text-xs font-medium text-amber-400 paper:text-amber-800">
                  {t("settings.networkDoctorRecommendations")}
                </p>
                <ul className="list-disc pl-4 space-y-0.5">
                  {diagnosis.recommendations.map((key) => (
                    <li key={key} className="text-xs text-muted-foreground">
                      {t(REC_I18N[key] ?? key)}
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
});

function CheckRow({ check }: { check: NetworkHostCheck }) {
  return (
    <li className="flex items-start justify-between gap-3 text-xs">
      <div className="min-w-0">
        <p className="font-medium text-foreground truncate">{check.label}</p>
        {check.url && <p className="text-muted-foreground truncate">{check.url}</p>}
        {check.detail && <p className="text-muted-foreground truncate">{check.detail}</p>}
      </div>
      <span
        className={cn(
          "shrink-0 rounded-md px-1.5 py-0.5 font-mono",
          check.status === "ok" && "bg-emerald-500/10 text-emerald-400 paper:text-emerald-700",
          check.status === "fail" && "bg-destructive/10 text-destructive",
          check.status === "skip" && "bg-muted text-muted-foreground",
        )}
      >
        {check.status === "ok" && check.latency_ms != null ? `${check.latency_ms}ms` : check.status}
      </span>
    </li>
  );
}
