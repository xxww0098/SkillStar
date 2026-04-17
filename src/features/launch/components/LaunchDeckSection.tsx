import { AnimatePresence, motion } from "framer-motion";
import { AlertCircle, ChevronDown, Loader2, Rocket, Save } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigation } from "../../../hooks/useNavigation";
import { cn } from "../../../lib/utils";
import { useAgentClis } from "../hooks/useAgentClis";
import type { LaunchConfig, LayoutNode, PaneNode } from "../hooks/useLaunchConfig";
import { useLaunchConfig } from "../hooks/useLaunchConfig";
import { DeployButton } from "./DeployButton";
import { PaneCell } from "./PaneCell";

interface LaunchDeckSectionProps {
  projectName: string;
  projectPath: string;
}

function defaultPane(): PaneNode {
  return {
    type: "pane",
    id: "pane-1",
    agentId: "",
    safeMode: false,
    extraArgs: [],
  };
}

function findFirstPane(node: LayoutNode | null | undefined): PaneNode | null {
  if (!node) return null;
  if (node.type === "pane") return node;
  return findFirstPane(node.children[0]) ?? findFirstPane(node.children[1]);
}

function primaryPane(config: LaunchConfig): PaneNode {
  const pane = findFirstPane(config.singleLayout) ?? findFirstPane(config.multiLayout) ?? defaultPane();
  return {
    ...defaultPane(),
    ...pane,
    type: "pane",
  };
}

function paneEquals(a: PaneNode | null, b: PaneNode): boolean {
  if (!a) return false;
  return (
    a.id === b.id &&
    a.agentId === b.agentId &&
    a.providerId === b.providerId &&
    a.providerName === b.providerName &&
    a.modelId === b.modelId &&
    a.safeMode === b.safeMode &&
    a.extraArgs.join(",") === b.extraArgs.join(",")
  );
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
  const [expanded, setExpanded] = useState(true);
  const agents = useAgentClis();
  const { config, setConfig, saving, loading } = useLaunchConfig(projectName);

  // Launch Deck is single-pane only. Keep config normalized to single mode.
  useEffect(() => {
    if (!config) return;
    const pane = primaryPane(config);
    const single = findFirstPane(config.singleLayout);
    const multi = findFirstPane(config.multiLayout);
    const normalized =
      config.mode === "single" &&
      config.singleLayout.type === "pane" &&
      config.multiLayout.type === "pane" &&
      paneEquals(single, pane) &&
      paneEquals(multi, pane);
    if (!normalized) {
      setConfig((prev) => ({
        ...prev,
        mode: "single",
        singleLayout: pane,
        multiLayout: { ...pane },
      }));
    }
  }, [config, setConfig]);

  const assign = useCallback(
    (paneId: string, agentId: string, providerId?: string, providerName?: string, modelId?: string) => {
      setConfig((prev) => {
        const current = primaryPane(prev);
        const nextPane: PaneNode = {
          ...current,
          id: paneId || current.id,
          agentId,
          providerId,
          providerName,
          modelId,
        };
        return {
          ...prev,
          mode: "single",
          singleLayout: nextPane,
          multiLayout: { ...nextPane },
        };
      });
    },
    [setConfig],
  );

  const goModels = useCallback(() => {
    navigateToModels();
  }, [navigateToModels]);

  if (loading || !config) {
    return <LaunchDeckSkeleton />;
  }

  const currentPane = primaryPane(config);
  const editorHeight = 320;
  const deployConfig: LaunchConfig = {
    ...config,
    mode: "single",
    singleLayout: currentPane,
    multiLayout: { ...currentPane },
  };
  const hasEmptyPanes = !currentPane.agentId;

  const noAgentClis = agents.filter((a) => a.installed).length === 0;

  const deployDisabled = hasEmptyPanes;
  const deployDisabledReason = hasEmptyPanes ? t("launch.disabledEmptyPanes") : null;

  return (
    <div className="rounded-xl border border-border-subtle overflow-hidden">
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
                    <p className="text-xs leading-relaxed text-muted-foreground">{t("launch.bannerNoAgent")}</p>
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

              <div
                className="relative rounded-xl border border-border/50 bg-background overflow-hidden p-2 shadow-sm transition-all duration-300 min-h-[240px]"
                style={{ height: `${editorHeight}px` }}
              >
                <div className="absolute inset-0 pointer-events-none bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNDAiIGhlaWdodD0iNDAiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PHBhdGggZD0iTTAgMGg0MHY0MEgweiIgZmlsbD0ibm9uZSIvPjxwYXRoIGQ9Ik0wIDAuNWg0ME0wIDM5LjVoNDBNMC41IDB2NDBNMzkuNSAwdjQwIiBzdHJva2U9InJnYmEoMCwgMCwgMCwgMC4wNCkiIHN0cm9rZS13aWR0aD0iMSIvPjwvc3ZnPg==')] [mask-image:radial-gradient(ellipse_at_center,black,transparent_70%)] opacity-50" />

                <div className="relative w-full h-full flex flex-col z-10">
                  <PaneCell pane={currentPane} agents={agents} onAssign={assign} />
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
