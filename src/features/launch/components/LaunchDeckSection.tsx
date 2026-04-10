import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { ChevronDown, Loader2, Rocket, Save } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { detectPlatform } from "../../../lib/utils";
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

export function LaunchDeckSection({ projectName, projectPath }: LaunchDeckSectionProps) {
  const { t } = useTranslation();
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

  if (loading || !config) {
    return null;
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

  return (
    <div className="rounded-xl border border-border-subtle overflow-hidden">
      {/* Header */}
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer"
      >
        <div className="flex items-center gap-2.5">
          <div className="w-7 h-7 rounded-lg bg-primary/10 border border-primary/15 flex items-center justify-center">
            <Rocket className="w-3.5 h-3.5 text-primary/70" />
          </div>
          <span className="text-sm font-semibold text-foreground">{t("launch.title", "Launch")}</span>
          {paneCount > 1 && (
            <span className="text-[10px] font-medium text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded">
              {paneCount} {t("launch.panes", "面板")}
            </span>
          )}
        </div>
        <div className="flex items-center gap-3">
          {saving && (
            <span className="flex items-center gap-1 text-[10px] text-muted-foreground">
              <Loader2 className="w-3 h-3 animate-spin" />
            </span>
          )}
          {expanded && (
            // biome-ignore lint/a11y/noStaticElementInteractions: event isolation wrapper
            <span onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.stopPropagation()}>
              <ModeSwitch mode={effectiveMode} onModeChange={handleModeChange} disableMulti={isWindows} />
            </span>
          )}
          <ChevronDown
            className={`w-4 h-4 text-muted-foreground transition-transform duration-200 ${
              expanded ? "rotate-180" : ""
            }`}
          />
        </div>
      </button>

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
            <div className="px-4 pb-4 space-y-3">
              {/* tmux prompt */}
              {needsTmux && <TmuxPrompt />}

              {/* Layout editor */}
              <div
                className="relative rounded-xl border border-border/50 bg-background overflow-hidden p-2 shadow-sm transition-all duration-300"
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
              <div className="flex items-center justify-between pt-1">
                <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                  {saving ? (
                    <span className="flex items-center gap-1">
                      <Loader2 className="w-3 h-3 animate-spin" />
                      {t("launch.saving", "保存中…")}
                    </span>
                  ) : (
                    <span className="flex items-center gap-1">
                      <Save className="w-3 h-3" />
                      {t("launch.autoSaved", "自动保存 ✓")}
                    </span>
                  )}
                </div>
                <DeployButton
                  config={deployConfig}
                  projectPath={projectPath}
                  disabled={hasEmptyPanes || needsTmux || (isWindows && config.mode === "multi")}
                />
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
