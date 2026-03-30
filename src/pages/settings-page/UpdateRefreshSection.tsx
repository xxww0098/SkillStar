import { RefreshCw, ChevronDown, Loader2, EyeOff, Shield, Square } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { cn } from "../../lib/utils";
import { useSkills } from "../../hooks/useSkills";
import type { SkillUpdateRefreshMode } from "../../lib/skillUpdateRefresh";

interface PatrolStatus {
  running: boolean;
  interval_secs: number;
  skills_checked: number;
  updates_found: number;
  current_skill: string;
}

interface PatrolConfig {
  enabled: boolean;
  interval_secs: number;
}

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

const PATROL_INTERVALS: { secs: number; labelKey: string }[] = [
  { secs: 15, labelKey: "settings.patrolInterval15s" },
  { secs: 30, labelKey: "settings.patrolInterval30s" },
  { secs: 60, labelKey: "settings.patrolInterval60s" },
  { secs: 120, labelKey: "settings.patrolInterval120s" },
];

export function UpdateRefreshSection({
  mode,
  onModeChange,
}: UpdateRefreshSectionProps) {
  const { t } = useTranslation();
  const { refresh } = useSkills();
  const [expanded, setExpanded] = useState(true);
  const [isChecking, setIsChecking] = useState(false);

  // Patrol state
  const [patrolStatus, setPatrolStatus] = useState<PatrolStatus | null>(null);
  const [patrolInterval, setPatrolInterval] = useState(30);
  const [isStarting, setIsStarting] = useState(false);

  // Load patrol config + status on mount
  useEffect(() => {
    invoke<PatrolConfig>("get_patrol_config")
      .then((config) => setPatrolInterval(config.interval_secs))
      .catch(() => {});

    invoke<PatrolStatus>("get_patrol_status")
      .then(setPatrolStatus)
      .catch(() => {});
  }, []);

  // Listen for patrol events to update live status
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    listen<{ name: string; update_available: boolean; skills_checked: number; updates_found: number }>(
      "patrol://skill-checked",
      (event) => {
        setPatrolStatus((prev) => prev ? {
          ...prev,
          skills_checked: event.payload.skills_checked,
          updates_found: event.payload.updates_found,
          current_skill: event.payload.name,
          running: true,
        } : {
          running: true,
          interval_secs: patrolInterval,
          skills_checked: event.payload.skills_checked,
          updates_found: event.payload.updates_found,
          current_skill: event.payload.name,
        });
      }
    ).then((fn_) => { unlisten = fn_; });

    return () => { unlisten?.(); };
  }, [patrolInterval]);

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

  const handlePatrolIntervalChange = useCallback(async (secs: number) => {
    setPatrolInterval(secs);
    try {
      await invoke("save_patrol_config", {
        config: { enabled: patrolStatus?.running ?? false, interval_secs: secs },
      });
      // If patrol is currently running, restart with new interval
      if (patrolStatus?.running) {
        await invoke("start_patrol", { intervalSecs: secs });
      }
    } catch (e) {
      console.error("Failed to save patrol config:", e);
    }
  }, [patrolStatus?.running]);

  const handleEnterStealth = async () => {
    if (isStarting) return;
    setIsStarting(true);
    try {
      await invoke("start_patrol", { intervalSecs: patrolInterval });
      setPatrolStatus((prev) => ({
        ...(prev ?? { skills_checked: 0, updates_found: 0, current_skill: "" }),
        running: true,
        interval_secs: patrolInterval,
      }));
      toast.success(t("settings.enterStealthMessage", { defaultValue: "已进入隐遁模式" }));

      // Hide the window
      try {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        const window = getCurrentWindow();
        await window.hide();
      } catch {
        // Window API not available (dev mode)
      }
    } catch (e) {
      toast.error(String(e));
    } finally {
      setIsStarting(false);
    }
  };

  const handleStopPatrol = async () => {
    try {
      await invoke("stop_patrol");
      setPatrolStatus((prev) => prev ? { ...prev, running: false, current_skill: "" } : null);
      toast.success(t("settings.stealthStopped", { defaultValue: "已退出隐遁模式" }));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const isPatrolRunning = patrolStatus?.running ?? false;

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
              {mode === "patrol"
                ? t("settings.updateRefreshPatrol", { defaultValue: "隐遁" })
                : t(UPDATE_REFRESH_OPTIONS.find(o => o.id === mode)?.labelKey || "")}
            </span>
          )}
          {isPatrolRunning && (
            <span className="flex items-center gap-1 text-[10px] text-emerald-400 font-medium px-1.5 py-0.5 rounded-md bg-emerald-500/10 border border-emerald-500/20 animate-pulse">
              <Shield className="w-3 h-3" />
              {t("settings.patrolActive", { defaultValue: "巡检中" })}
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
            {/* Regular refresh modes */}
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
                      mode === option.id && mode !== "patrol"
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
              
              <div className="flex items-center gap-2 pt-1">
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

            {/* Stealth / Patrol section */}
            <div className="border-t border-border pt-4">
              <div className="flex items-center gap-2 mb-2.5">
                <div className="w-5 h-5 rounded-md bg-indigo-500/10 flex items-center justify-center border border-indigo-500/20">
                  <EyeOff className="w-3 h-3 text-indigo-400" />
                </div>
                <span className="text-xs font-semibold text-foreground tracking-tight">
                  {t("settings.stealthTitle", { defaultValue: "隐遁模式" })}
                </span>
              </div>

              <p className="text-[11px] text-muted-foreground mb-3 leading-relaxed">
                {t("settings.stealthHint", { defaultValue: "最小化窗口，在后台逐个检测技能更新状态，极低资源消耗。关闭窗口后继续运行。" })}
              </p>

              {/* Patrol interval picker */}
              <div className="flex items-center gap-1.5 mb-3 flex-wrap">
                <span className="text-[11px] text-muted-foreground mr-1">
                  {t("settings.patrolInterval", { defaultValue: "巡检间隔" })}
                </span>
                {PATROL_INTERVALS.map((opt) => (
                  <button
                    key={opt.secs}
                    onClick={() => handlePatrolIntervalChange(opt.secs)}
                    className={cn(
                      "px-2 py-0.5 rounded-md text-[11px] font-medium transition-colors cursor-pointer",
                      patrolInterval === opt.secs
                        ? "bg-indigo-500/20 text-indigo-400 border border-indigo-500/30"
                        : "text-muted-foreground hover:bg-muted hover:text-foreground border border-transparent"
                    )}
                  >
                    {t(opt.labelKey)}
                  </button>
                ))}
              </div>

              {/* Status indicator when running */}
              {isPatrolRunning && (
                <div className="flex items-center gap-2 mb-3 px-2.5 py-2 rounded-lg bg-emerald-500/5 border border-emerald-500/15">
                  <div className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse shrink-0" />
                  <span className="text-[11px] text-emerald-400/90 font-medium">
                    {patrolStatus?.current_skill
                      ? t("settings.patrolChecking", { defaultValue: "正在检测 {{name}}", name: patrolStatus.current_skill })
                      : t("settings.patrolActive", { defaultValue: "巡检中" })}
                  </span>
                  <span className="text-[10px] text-muted-foreground ml-auto">
                    {t("settings.patrolStats", {
                      defaultValue: "已检 {{checked}} · 发现 {{found}} 个更新",
                      checked: patrolStatus?.skills_checked ?? 0,
                      found: patrolStatus?.updates_found ?? 0,
                    })}
                  </span>
                </div>
              )}

              {/* Action buttons */}
              <div className="flex items-center gap-2">
                {!isPatrolRunning ? (
                  <button
                    onClick={handleEnterStealth}
                    disabled={isStarting}
                    className="inline-flex items-center justify-center gap-1.5 px-3.5 py-1.5 text-xs font-medium bg-indigo-500/15 text-indigo-400 hover:bg-indigo-500/25 border border-indigo-500/25 rounded-lg transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {isStarting ? (
                      <Loader2 className="w-3.5 h-3.5 animate-spin" />
                    ) : (
                      <EyeOff className="w-3.5 h-3.5" />
                    )}
                    {t("settings.enterStealth", { defaultValue: "进入隐遁" })}
                  </button>
                ) : (
                  <button
                    onClick={handleStopPatrol}
                    className="inline-flex items-center justify-center gap-1.5 px-3.5 py-1.5 text-xs font-medium bg-rose-500/10 text-rose-400 hover:bg-rose-500/20 border border-rose-500/20 rounded-lg transition-all cursor-pointer"
                  >
                    <Square className="w-3 h-3" />
                    {t("settings.exitStealth", { defaultValue: "退出隐遁" })}
                  </button>
                )}
              </div>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
