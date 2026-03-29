import { RefreshCw, ChevronDown, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useState } from "react";
import { toast } from "sonner";
import { cn } from "../../lib/utils";
import { useSkills } from "../../hooks/useSkills";
import type { SkillUpdateRefreshMode } from "../../lib/skillUpdateRefresh";

interface UpdateRefreshSectionProps {
  mode: SkillUpdateRefreshMode;
  onModeChange: (mode: SkillUpdateRefreshMode) => void;
}

const UPDATE_REFRESH_OPTIONS: { id: SkillUpdateRefreshMode; labelKey: string }[] = [
  { id: "auto", labelKey: "settings.updateRefreshAuto" },
  { id: "1m", labelKey: "settings.updateRefresh1m" },
  { id: "5m", labelKey: "settings.updateRefresh5m" },
  { id: "10m", labelKey: "settings.updateRefresh10m" },
  { id: "30m", labelKey: "settings.updateRefresh30m" },
];

export function UpdateRefreshSection({
  mode,
  onModeChange,
}: UpdateRefreshSectionProps) {
  const { t } = useTranslation();
  const { refresh } = useSkills();
  const [expanded, setExpanded] = useState(true);
  const [isChecking, setIsChecking] = useState(false);

  const handleRefreshNow = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (isChecking) return;
    
    setIsChecking(true);
    try {
      await refresh(true, true);
      toast.success(t("settings.refreshSuccess", { defaultValue: "已触发立马更新检查" }));
    } catch (error) {
      toast.error(String(error));
    } finally {
      setIsChecking(false);
    }
  };

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-sky-500/10 flex items-center justify-center shrink-0 border border-sky-500/20">
            <RefreshCw className="w-4 h-4 text-sky-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.updateRefresh")}</h2>
          {mode && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border">
              {t(UPDATE_REFRESH_OPTIONS.find(o => o.id === mode)?.labelKey || "")}
            </span>
          )}
        </div>
      </div>

      <div className="rounded-xl border border-border bg-card overflow-hidden">
        <button 
          onClick={() => setExpanded(!expanded)}
          className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer"
        >
          <span className="text-sm font-medium text-foreground">{t("settings.updateRefreshConfigTitle", { defaultValue: "Update Settings" })}</span>
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !expanded && "-rotate-90"
            )}
          />
        </button>

        {expanded && (
          <div className="px-4 pb-4 pt-1 border-t border-border space-y-4">
            <div className="flex flex-col gap-2.5">
              <div className="flex flex-col gap-1.5">
                <p className="text-xs text-muted-foreground">{t("settings.updateRefreshHint")}</p>
              </div>

              <div className="flex items-center gap-1 flex-wrap">
                {UPDATE_REFRESH_OPTIONS.map((option) => (
                  <button
                    key={option.id}
                    onClick={(e) => {
                      e.stopPropagation();
                      onModeChange(option.id);
                    }}
                    className={cn(
                      "px-2.5 py-1 rounded-lg text-xs font-medium transition-colors cursor-pointer",
                      mode === option.id
                        ? "bg-primary text-primary-foreground"
                        : "text-muted-foreground hover:bg-muted hover:text-foreground"
                    )}
                  >
                    {t(option.labelKey)}
                  </button>
                ))}
              </div>

              {mode === "auto" && (
                <p className="text-[11px] text-muted-foreground">
                  {t("settings.updateRefreshAutoHint")}
                </p>
              )}
              
              <div className="pt-2">
                <button
                  onClick={handleRefreshNow}
                  disabled={isChecking}
                  className="inline-flex items-center justify-center gap-1.5 px-3 py-1.5 text-xs font-medium bg-muted text-muted-foreground hover:bg-secondary hover:text-secondary-foreground rounded-lg transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {isChecking ? (
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  ) : (
                    <RefreshCw className="w-3.5 h-3.5" />
                  )}
                  {t("settings.refreshNow", { defaultValue: "立马检查更新" })}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
