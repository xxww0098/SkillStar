import { invoke } from "@tauri-apps/api/core";
import { Bot, Check, ChevronDown, ChevronRight } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

interface AcpConfig {
  enabled: boolean;
  agent_command: string;
  agent_label: string;
}

/** Built-in agent presets for quick selection. */
const AGENT_PRESETS = [
  { label: "Claude", command: "npx -y @agentclientprotocol/claude-agent-acp" },
  { label: "OpenCode", command: "opencode acp" },
] as const;

function isSameAcpConfig(a: AcpConfig, b: AcpConfig): boolean {
  return a.enabled === b.enabled && a.agent_command === b.agent_command && a.agent_label === b.agent_label;
}

export function AcpSection() {
  const { t } = useTranslation();
  const [config, setConfig] = useState<AcpConfig>({
    enabled: false,
    agent_command: "npx -y @agentclientprotocol/claude-agent-acp",
    agent_label: "Claude",
  });
  const savedConfigRef = useRef<AcpConfig>(config);
  const [loaded, setLoaded] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Load config
  useEffect(() => {
    invoke<AcpConfig>("get_acp_config")
      .then((cfg) => {
        setConfig(cfg);
        savedConfigRef.current = cfg;
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  // Auto-save on change — only when config actually differs from last saved
  useEffect(() => {
    if (!loaded || saving || isSameAcpConfig(config, savedConfigRef.current)) return;

    const timer = setTimeout(() => {
      setSaving(true);
      invoke("save_acp_config", { config })
        .then(() => {
          savedConfigRef.current = config;
          setSaved(true);
          setTimeout(() => setSaved(false), 2000);
        })
        .catch(() => toast.error(t("setupHook.saveFailed")))
        .finally(() => setSaving(false));
    }, 600);

    return () => clearTimeout(timer);
  }, [config, loaded, saving, t]);

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
