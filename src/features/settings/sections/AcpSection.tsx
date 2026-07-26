import { useQuery } from "@tanstack/react-query";
import { BookOpen, Bot, Check, ChevronDown, ChevronRight, Hammer, Map as MapIcon } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { type AcpConfig, tauriInvoke } from "../../../lib/ipc";
import type { SkillTutorialStyle } from "../../../types";
import { settingsKeys } from "../api/keys";

const DEFAULT_ACP_CONFIG: AcpConfig = {
  enabled: false,
  agent_command: "npx -y @agentclientprotocol/claude-agent-acp",
  agent_label: "Claude Code",
  tutorial_style: "guided",
};

/** Built-in agent presets for quick selection. */
const AGENT_PRESETS = [
  { label: "Claude Code", command: "npx -y @agentclientprotocol/claude-agent-acp" },
  { label: "OpenCode", command: "opencode acp" },
] as const;

const TUTORIAL_STYLES = [
  {
    value: "guided",
    labelKey: "settings.acpStyleGuided",
    descriptionKey: "settings.acpStyleGuidedDesc",
    icon: MapIcon,
  },
  {
    value: "reference",
    labelKey: "settings.acpStyleReference",
    descriptionKey: "settings.acpStyleReferenceDesc",
    icon: BookOpen,
  },
  {
    value: "workshop",
    labelKey: "settings.acpStyleWorkshop",
    descriptionKey: "settings.acpStyleWorkshopDesc",
    icon: Hammer,
  },
] as const satisfies ReadonlyArray<{
  value: SkillTutorialStyle;
  labelKey: string;
  descriptionKey: string;
  icon: typeof MapIcon;
}>;

function isSameAcpConfig(a: AcpConfig, b: AcpConfig): boolean {
  return (
    a.enabled === b.enabled &&
    a.agent_command === b.agent_command &&
    a.agent_label === b.agent_label &&
    a.tutorial_style === b.tutorial_style
  );
}

export function AcpSection() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<AcpConfig>(DEFAULT_ACP_CONFIG);
  const savedConfigRef = useRef<AcpConfig>(config);
  const [expanded, setExpanded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Load config once via Query (staleTime: Infinity mirrors the old
  // mount-only useEffect: no window-focus/interval refetch). `loaded` derives
  // from "settled" (success OR error), matching the old code: on failure it
  // still flipped `loaded = true` and left `config` at its hardcoded default
  // — which is exactly what arms the auto-save effect below.
  const configQuery = useQuery<AcpConfig>({
    queryKey: settingsKeys.acpConfig(),
    queryFn: () => tauriInvoke("get_acp_config"),
    staleTime: Infinity,
  });
  const loaded = configQuery.isSuccess || configQuery.isError;

  useEffect(() => {
    if (!configQuery.data) return;
    const loadedConfig = {
      ...configQuery.data,
      tutorial_style: configQuery.data.tutorial_style || "guided",
    } satisfies AcpConfig;
    setConfig(loadedConfig);
    savedConfigRef.current = loadedConfig;
  }, [configQuery.data]);

  const persistConfig = useCallback(
    (nextConfig: AcpConfig) => {
      setSaving(true);
      return tauriInvoke("save_acp_config", { config: nextConfig })
        .then(() => {
          savedConfigRef.current = nextConfig;
          setSaved(true);
          setTimeout(() => setSaved(false), 2000);
        })
        .catch(() => toast.error(t("setupHook.saveFailed")))
        .finally(() => setSaving(false));
    },
    [t],
  );

  // Auto-save on change — only when config actually differs from last saved
  useEffect(() => {
    if (!loaded || saving || isSameAcpConfig(config, savedConfigRef.current)) return;

    const timer = setTimeout(() => {
      void persistConfig(config);
    }, 600);

    return () => clearTimeout(timer);
  }, [config, loaded, persistConfig, saving]);

  const selectPreset = useCallback((preset: (typeof AGENT_PRESETS)[number]) => {
    setConfig((prev) => ({
      ...prev,
      agent_command: preset.command,
      agent_label: preset.label,
    }));
  }, []);

  const toggleEnabled = useCallback(() => {
    setConfig((prev) => ({ ...prev, enabled: !prev.enabled }));
  }, []);

  const selectTutorialStyle = useCallback(
    (tutorialStyle: SkillTutorialStyle) => {
      if (config.tutorial_style === tutorialStyle || saving) return;
      const nextConfig = { ...config, tutorial_style: tutorialStyle };
      setConfig(nextConfig);
      // A style click is a complete, discrete choice. Dispatch it immediately
      // so navigating back to the Skill cannot cancel the debounced save.
      void persistConfig(nextConfig);
    },
    [config, persistConfig, saving],
  );

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-violet-500/10 flex items-center justify-center shrink-0 border border-violet-500/20">
          <Bot className="w-4 h-4 text-violet-400" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.acpTitle")}</h2>
        {saved && (
          <span className="ml-auto mr-3 text-[11px] text-emerald-400 flex items-center gap-1">
            <Check className="w-3 h-3" />
            {t("common.saved")}
          </span>
        )}
        <button
          role="switch"
          aria-checked={config.enabled}
          onClick={toggleEnabled}
          className={`
            ${saved ? "" : "ml-auto"}
            relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full
            border-2 border-transparent transition-colors duration-200 ease-in-out
            focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring
            ${config.enabled ? "bg-primary" : "bg-muted"}
          `}
        >
          <span
            className={`
              pointer-events-none inline-block h-4 w-4 rounded-full bg-background shadow-lg ring-0
              transition-transform duration-200 ease-in-out
              ${config.enabled ? "translate-x-4" : "translate-x-0"}
            `}
          />
        </button>
      </div>

      <div className="rounded-xl border border-border bg-card">
        {/* Title bar — click to expand */}
        <div
          className="flex items-center justify-between px-4 py-3 cursor-pointer select-none"
          onClick={() => setExpanded(!expanded)}
        >
          <div>
            <p className="text-xs font-medium text-foreground">{t("settings.acpAgent")}</p>
            <p className="text-[11px] text-muted-foreground">{config.agent_label}</p>
          </div>
          {expanded ? (
            <ChevronDown className="w-4 h-4 text-muted-foreground" />
          ) : (
            <ChevronRight className="w-4 h-4 text-muted-foreground" />
          )}
        </div>

        {/* Expanded config */}
        {expanded && (
          <div className="px-4 pb-4 space-y-3 border-t border-border/50">
            <p className="text-[11px] text-muted-foreground pt-3 leading-relaxed">{t("settings.acpDesc")}</p>

            <div className="space-y-2">
              <div>
                <p className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                  {t("settings.acpTutorialStyle")}
                </p>
                <p className="mt-1 text-[10px] leading-relaxed text-muted-foreground/70">
                  {t("settings.acpTutorialStyleHint")}
                </p>
              </div>
              <div className="grid gap-2">
                {TUTORIAL_STYLES.map((option) => {
                  const active = config.tutorial_style === option.value;
                  const Icon = option.icon;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      aria-pressed={active}
                      disabled={saving}
                      onClick={() => selectTutorialStyle(option.value)}
                      className={`rounded-xl border p-3 text-left transition-colors ${
                        active
                          ? "border-primary/45 bg-primary/10 text-foreground"
                          : "border-border/60 bg-background/35 text-muted-foreground hover:border-border hover:bg-muted/40 hover:text-foreground"
                      }`}
                    >
                      <span className="flex items-start gap-3">
                        <span
                          className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border ${
                            active ? "border-primary/30 bg-primary/15 text-primary" : "border-border/60 bg-muted/50"
                          }`}
                        >
                          <Icon className="h-3.5 w-3.5" aria-hidden />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="flex flex-wrap items-center gap-2 text-xs font-semibold">
                            {t(option.labelKey)}
                            {option.value === "guided" ? (
                              <span className="rounded-full border border-primary/25 bg-primary/10 px-1.5 py-0.5 text-[9px] font-medium text-primary">
                                {t("settings.acpStyleRecommended")}
                              </span>
                            ) : null}
                          </span>
                          <span className="mt-1 block text-[10px] leading-relaxed text-muted-foreground">
                            {t(option.descriptionKey)}
                          </span>
                        </span>
                        {active ? <Check className="mt-1 h-3.5 w-3.5 shrink-0 text-primary" aria-hidden /> : null}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Agent presets */}
            <div className="space-y-1.5">
              <label className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                {t("settings.acpPresets")}
              </label>
              <div className="grid grid-cols-2 gap-1.5">
                {AGENT_PRESETS.map((preset) => {
                  const isActive = config.agent_command === preset.command;
                  return (
                    <button
                      key={preset.command}
                      onClick={() => selectPreset(preset)}
                      className={`
                        px-3 py-2 rounded-lg text-xs font-medium transition-all duration-150
                        border
                        ${
                          isActive
                            ? "bg-primary/15 border-primary/40 text-primary"
                            : "bg-card border-border/50 text-muted-foreground hover:border-border hover:text-foreground"
                        }
                      `}
                    >
                      {preset.label}
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Custom command input */}
            <div className="space-y-1.5">
              <label className="text-[11px] font-medium text-muted-foreground uppercase tracking-wider">
                {t("settings.acpCommand")}
              </label>
              <input
                type="text"
                value={config.agent_command}
                onChange={(e) =>
                  setConfig((prev) => ({
                    ...prev,
                    agent_command: e.target.value,
                    agent_label: AGENT_PRESETS.find((p) => p.command === e.target.value)?.label ?? "Custom",
                  }))
                }
                className="w-full rounded-lg bg-background/60 border border-border/50 px-3 py-2 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-primary/50"
                placeholder="npx -y @agentclientprotocol/claude-agent-acp"
                spellCheck={false}
              />
              <p className="text-[10px] text-muted-foreground/70">{t("settings.acpCommandHint")}</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
