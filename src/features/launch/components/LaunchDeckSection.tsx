import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { AlertCircle, ChevronDown, Loader2, Rocket, Save } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigation } from "../../../hooks/useNavigation";
import { cn, detectPlatform } from "../../../lib/utils";
import { useAgentClis } from "../hooks/useAgentClis";
import type { LaunchMode, LayoutNode } from "../hooks/useLaunchConfig";
import { useLaunchConfig } from "../hooks/useLaunchConfig";
import { countPanes, useLayoutTree } from "../hooks/useLayoutTree";
import { DeployButton } from "./DeployButton";
import { ModeSwitch } from "./ModeSwitch";
import { PaneLayoutEditor } from "./PaneLayoutEditor";
import { TmuxPrompt } from "./TmuxPrompt";

interface LaunchDeckSectionProps {
  projectName: string;
  projectPath: string;
}

interface TmuxStatus {
  installed: boolean;
  version: string | null;
}

function LaunchDeckSkeleton() {
  return (
    <div className="rounded-xl border border-border-subtle overflow-hidden" aria-busy="true" aria-live="polite">
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border-subtle/40">
        <div className="flex flex-1 items-center gap-2.5 min-w-0 animate-pulse">
          <div className="w-7 h-7 rounded-lg bg-muted/50" />
          <div className="h-4 w-24 rounded-md bg-muted/50" />
          <div className="flex-1" />
          <div className="h-4 w-4 rounded-sm bg-muted/40 shrink-0" />
        </div>
        <div className="h-8 w-[104px] rounded-lg bg-muted/40 shrink-0 animate-pulse" />
      </div>
      <div className="p-4 space-y-3">
        <div className="h-[320px] rounded-xl bg-muted/20 border border-border/40 animate-pulse" />
        <div className="flex justify-between items-center pt-1">
          <div className="h-3 w-32 rounded bg-muted/35 animate-pulse" />
          <div className="h-9 w-36 rounded-lg bg-muted/40 animate-pulse" />
        </div>
      </div>
    </div>
  );
}

export function LaunchDeckSection({ projectName, projectPath }: LaunchDeckSectionProps) {
  const { t } = useTranslation();
  const { navigateToModels } = useNavigation();
  const isWindows = detectPlatform() === "windows";
  const [expanded, setExpanded] = useState(true);
  const [tmuxStatus, setTmuxStatus] = useState<TmuxStatus | null>(null);
  const agents = useAgentClis();
  const { config, setConfig, saving, loading } = useLaunchConfig(projectName);

  // Windows policy: force single mode and disable tmux multi mode.
  useEffect(() => {
    if (isWindows && config?.mode === "multi") {
      setConfig((prev) => ({ ...prev, mode: "single" }));
    }
  }, [isWindows, config?.mode, setConfig]);

  // Check tmux on expand
  useEffect(() => {
    if (isWindows) {
      return;
    }
    if (expanded && tmuxStatus === null) {
      invoke<TmuxStatus>("check_tmux")
        .then(setTmuxStatus)
        .catch(() => setTmuxStatus({ installed: false, version: null }));
    }
  }, [expanded, isWindows, tmuxStatus]);

  const handleModeChange = useCallback(
    (mode: LaunchMode) => {
      if (!config) return;
      if (isWindows && mode === "multi") return;

      setConfig((prev) => ({ ...prev, mode }));
    },
    [config, isWindows, setConfig],
  );

  const handleLayoutUpdate = useCallback(
    (newLayout: LayoutNode) => {
      setConfig((prev) => {
        const targetMode: LaunchMode = isWindows ? "single" : prev.mode;
        if (targetMode === "single") {
          return { ...prev, singleLayout: newLayout };
        }
        return { ...prev, multiLayout: newLayout };
      });
    },
    [isWindows, setConfig],
  );

  const effectiveMode: LaunchMode = isWindows ? "single" : (config?.mode ?? "single");
  const currentLayout = config ? (effectiveMode === "single" ? config.singleLayout : config.multiLayout) : null;
  const { split, remove, resize, assign } = useLayoutTree(currentLayout, handleLayoutUpdate);

  const goModels = useCallback(() => {
    navigateToModels();
  }, [navigateToModels]);

  if (loading || !config) {
    return <LaunchDeckSkeleton />;
  }

  const isMulti = effectiveMode === "multi";
  const editorHeight = 320;
  const needsTmux = !isWindows && isMulti && tmuxStatus !== null && !tmuxStatus.installed;
  const deployConfig = isWindows && config.mode === "multi" ? { ...config, mode: "single" as LaunchMode } : config;
  const paneCount = countPanes(currentLayout!);
  const hasEmptyPanes = (() => {
    const checkEmpty = (node: LayoutNode): boolean => {
      if (node.type === "pane") return !node.agentId;
      return checkEmpty(node.children[0]) || checkEmpty(node.children[1]);
    };
    return checkEmpty(currentLayout!);
  })();

  const noAgentClis = agents.filter((a) => a.installed).length === 0;

  const deployDisabled = hasEmptyPanes || needsTmux || (isWindows && config.mode === "multi");
  const deployDisabledReason = needsTmux
    ? t("launch.disabledTmux")
    : hasEmptyPanes
      ? t("launch.disabledEmptyPanes")
      : isWindows && config.mode === "multi"
        ? t("launch.disabledWindowsMulti")
        : null;

  return (
    <div className="rounded-xl border border-border-subtle overflow-hidden">
      {/* Header — collapse toggle separate from mode so mode stays visible when collapsed */}
      <div className="flex items-stretch gap-1 px-3 sm:px-4 py-2.5 border-b border-border-subtle/50 bg-muted/[0.08]">
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          aria-expanded={expanded}
          className="flex flex-1 items-center gap-2.5 min-w-0 rounded-lg px-1 py-1 text-left hover:bg-muted/35 transition-colors"
        >
          <div className="w-7 h-7 rounded-lg bg-primary/10 border border-primary/15 flex items-center justify-center shrink-0">
            <Rocket className="w-3.5 h-3.5 text-primary/70" />
          </div>
          <span className="text-sm font-semibold text-foreground truncate">{t("launch.title")}</span>
          {paneCount > 1 && (
            <span className="text-[10px] font-medium text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded shrink-0">
              {paneCount} {t("launch.panes")}
            </span>
          )}
          <span className="flex-1" />
          {saving && (
            <span className="flex items-center gap-1 text-[10px] text-muted-foreground shrink-0">
              <Loader2 className="w-3 h-3 animate-spin" />
            </span>
          )}
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200 shrink-0",
              expanded ? "rotate-180" : "",
            )}
          />
        </button>
        <div className="flex items-center shrink-0 py-0.5">
          <ModeSwitch mode={effectiveMode} onModeChange={handleModeChange} disableMulti={isWindows} />
        </div>
      </div>

      {/* Body */}
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: 0.2, ease: "easeInOut" }}
            className="overflow-hidden"
          >
            <div className="px-4 pb-4 pt-3 space-y-3">
              {noAgentClis && (
                <div className="rounded-xl border border-amber-500/25 bg-amber-500/[0.07] px-3.5 py-3 flex gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-amber-500/25 bg-amber-500/10">
                    <AlertCircle className="w-4 h-4 text-amber-500" />
                  </div>
                  <div className="min-w-0 flex-1 space-y-2">
                    <p className="text-xs leading-relaxed text-muted-foreground">{t("launch.bannerNoClis")}</p>
                    <button
                      type="button"
                      onClick={goModels}
                      className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:underline"
                    >
                      {t("launch.bannerModelsCta")} →
                    </button>
                  </div>
                </div>
              )}

              {!noAgentClis && hasEmptyPanes && (
                <div className="rounded-xl border border-amber-500/25 bg-amber-500/[0.07] px-3.5 py-3 flex gap-3">
                  <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-amber-500/25 bg-amber-500/10">
                    <AlertCircle className="w-4 h-4 text-amber-500" />
                  </div>
                  <div className="min-w-0 flex-1 space-y-2">
                    <p className="text-xs leading-relaxed text-muted-foreground">{t("launch.bannerEmptyPanes")}</p>
                    <button
                      type="button"
                      onClick={goModels}
                      className="text-xs font-medium text-amber-600 dark:text-amber-400 hover:underline"
                    >
                      {t("launch.bannerModelsCta")} →
                    </button>
                  </div>
                </div>
              )}

              {/* tmux prompt */}
              {needsTmux && <TmuxPrompt />}

              {/* Layout editor */}
              <div
                className="relative rounded-xl border border-border/50 bg-background overflow-hidden p-2 shadow-sm transition-all duration-300 min-h-[240px]"
                style={{ height: `${editorHeight}px` }}
              >
                {/* Subtle grid background for the entire editor */}
                <div className="absolute inset-0 pointer-events-none bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNDAiIGhlaWdodD0iNDAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PHBhdGggZD0iTTAgMGg0MHY0MEgweiIgZmlsbD0ibm9uZSIvPjxwYXRoIGQ9Ik0wIDAuNWg0ME0wIDM5LjVoNDBNMC41IDB2NDBNMzkuNSAwdjQwIiBzdHJva2U9InJnYmEoMCwgMCwgMCwgMC4wNCkiIHN0cm9rZS13aWR0aD0iMSIvPjwvc3ZnPg==')] [mask-image:radial-gradient(ellipse_at_center,black,transparent_70%)] opacity-50" />

                <div className="relative w-full h-full flex flex-col z-10">
                  <PaneLayoutEditor
                    node={currentLayout!}
                    agents={agents}
                    isMulti={isMulti}
                    onSplit={split}
                    onAssign={assign}
                    onRemove={remove}
                    onResize={resize}
                  />
                </div>
              </div>

              {/* Footer */}
              <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between pt-1">
                <div className="flex items-center gap-2 text-[10px] text-muted-foreground order-2 sm:order-1">
                  {saving ? (
                    <span className="flex items-center gap-1">
                      <Loader2 className="w-3 h-3 animate-spin" />
                      {t("launch.saving")}
                    </span>
                  ) : (
                    <span className="flex items-center gap-1">
                      <Save className="w-3 h-3" />
                      {t("launch.autoSaved")}
                    </span>
                  )}
                </div>
                <div className="flex flex-col items-stretch gap-1.5 sm:items-end order-1 sm:order-2">
                  {deployDisabled && deployDisabledReason && (
                    <p className="text-[10px] text-muted-foreground text-right max-w-[280px] leading-snug">
                      {deployDisabledReason}
                    </p>
                  )}
                  <DeployButton
                    config={deployConfig}
                    projectPath={projectPath}
                    disabled={deployDisabled}
                    disabledReason={deployDisabledReason}
                  />
                </div>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
