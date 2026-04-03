import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, Zap, Check, Loader2, Wifi, WifiOff } from "lucide-react";
import { Input } from "../../../components/ui/input";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import type { GitHubMirrorConfig, GitHubMirrorPreset } from "../../../types";

interface GitHubMirrorSectionProps {
  mirrorConfig: GitHubMirrorConfig;
  ready: boolean;
  mirrorExpanded: boolean;
  mirrorSaving: boolean;
  mirrorSaved: boolean;
  onToggleExpanded: () => void;
  onConfigChange: (next: GitHubMirrorConfig) => void;
}

export function GitHubMirrorSection({
  mirrorConfig,
  ready,
  mirrorExpanded,
  mirrorSaving,
  mirrorSaved,
  onToggleExpanded,
  onConfigChange,
}: GitHubMirrorSectionProps) {
  const { t } = useTranslation();
  const [presets, setPresets] = useState<GitHubMirrorPreset[]>([]);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [testResults, setTestResults] = useState<Record<string, number | "error">>({});

  useEffect(() => {
    invoke<GitHubMirrorPreset[]>("get_github_mirror_presets").then(setPresets).catch(() => {});
  }, []);

  const handleTestMirror = async (url: string, id: string) => {
    setTestingId(id);
    try {
      const latency = await invoke<number>("test_github_mirror", { url });
      setTestResults((prev) => ({ ...prev, [id]: latency }));
    } catch {
      setTestResults((prev) => ({ ...prev, [id]: "error" }));
    } finally {
      setTestingId(null);
    }
  };

  const isCustom = mirrorConfig.preset_id === null;

  // Resolve effective URL for display
  const effectiveUrl = isCustom
    ? mirrorConfig.custom_url || ""
    : presets.find((p) => p.id === mirrorConfig.preset_id)?.url || "";

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-emerald-500/10 flex items-center justify-center shrink-0 border border-emerald-500/20">
            <Zap className="w-4 h-4 text-emerald-500" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">
            {t("settings.githubMirror")}
          </h2>
          {mirrorConfig.enabled && effectiveUrl && (
            <span className="text-xs text-muted-foreground ml-2 px-2 py-0.5 rounded-md bg-muted/50 border border-border truncate max-w-[260px]">
              {effectiveUrl}
            </span>
          )}
        </div>

        {ready ? (
          <Switch
            checked={mirrorConfig.enabled}
            onCheckedChange={(checked) => onConfigChange({ ...mirrorConfig, enabled: checked })}
            disabled={mirrorSaving}
          />
        ) : (
          <div className="h-5 w-9 rounded-full border border-border bg-muted/60" />
        )}
      </div>

      <div
        className={cn(
          "rounded-xl border border-border overflow-hidden transition-colors",
          mirrorConfig.enabled ? "bg-card" : "bg-card/50"
        )}
      >
        <button
          onClick={onToggleExpanded}
          className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors cursor-pointer"
        >
          <span className="text-sm font-medium text-foreground">
            {t("settings.githubMirrorConfig")}
          </span>
          <ChevronDown
            className={cn(
              "w-4 h-4 text-muted-foreground transition-transform duration-200",
              !mirrorExpanded && "-rotate-90"
            )}
          />
        </button>

        {mirrorExpanded && (
          <div className="px-4 pb-4 pt-1 border-t border-border space-y-3">
            {/* Security notice */}
            <p className="text-xs text-muted-foreground leading-relaxed px-1">
              {t("settings.githubMirrorNotice")}
            </p>

            {/* Preset selection */}
            <div className="space-y-1.5">
              {presets.map((preset) => {
                const isSelected = mirrorConfig.preset_id === preset.id;
                const testResult = testResults[preset.id];
                const isTesting = testingId === preset.id;

                return (
                  <label
                    key={preset.id}
                    className={cn(
                      "flex items-center gap-3 px-3 py-2.5 rounded-lg border cursor-pointer transition-all duration-150",
                      isSelected
                        ? "border-primary/50 bg-primary/5 ring-1 ring-primary/20"
                        : "border-border hover:border-border/80 hover:bg-muted/20"
                    )}
                  >
                    <input
                      type="radio"
                      name="mirror-preset"
                      checked={isSelected}
                      onChange={() =>
                        onConfigChange({ ...mirrorConfig, preset_id: preset.id, custom_url: null })
                      }
                      className="sr-only"
                    />
                    <div
                      className={cn(
                        "w-3.5 h-3.5 rounded-full border-2 shrink-0 transition-colors",
                        isSelected ? "border-primary bg-primary" : "border-muted-foreground/40"
                      )}
                    >
                      {isSelected && (
                        <div className="w-full h-full flex items-center justify-center">
                          <div className="w-1.5 h-1.5 rounded-full bg-white" />
                        </div>
                      )}
                    </div>

                    <div className="flex-1 min-w-0">
                      <div className="text-sm font-medium text-foreground">{preset.name}</div>
                      <div className="text-xs text-muted-foreground truncate">{preset.url}</div>
                    </div>

                    {/* Test / result indicator */}
                    <button
                      type="button"
                      onClick={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        handleTestMirror(preset.url, preset.id);
                      }}
                      disabled={isTesting}
                      className={cn(
                        "shrink-0 text-xs px-2.5 py-1 rounded-md border transition-colors cursor-pointer",
                        testResult !== undefined
                          ? testResult === "error"
                            ? "border-red-500/30 bg-red-500/10 text-red-400"
                            : "border-emerald-500/30 bg-emerald-500/10 text-emerald-400"
                          : "border-border text-muted-foreground hover:text-foreground hover:border-border/80"
                      )}
                    >
                      {isTesting ? (
                        <Loader2 className="w-3 h-3 animate-spin" />
                      ) : testResult !== undefined ? (
                        testResult === "error" ? (
                          <span className="flex items-center gap-1">
                            <WifiOff className="w-3 h-3" />
                            {t("settings.mirrorTestFail")}
                          </span>
                        ) : (
                          <span className="flex items-center gap-1">
                            <Wifi className="w-3 h-3" />
                            {testResult}ms
                          </span>
                        )
                      ) : (
                        t("settings.mirrorTest")
                      )}
                    </button>
                  </label>
                );
              })}

              {/* Custom option */}
              <label
                className={cn(
                  "flex items-center gap-3 px-3 py-2.5 rounded-lg border cursor-pointer transition-all duration-150",
                  isCustom
                    ? "border-primary/50 bg-primary/5 ring-1 ring-primary/20"
                    : "border-border hover:border-border/80 hover:bg-muted/20"
                )}
              >
                <input
                  type="radio"
                  name="mirror-preset"
                  checked={isCustom}
                  onChange={() => onConfigChange({ ...mirrorConfig, preset_id: null })}
                  className="sr-only"
                />
                <div
                  className={cn(
                    "w-3.5 h-3.5 rounded-full border-2 shrink-0 transition-colors",
                    isCustom ? "border-primary bg-primary" : "border-muted-foreground/40"
                  )}
                >
                  {isCustom && (
                    <div className="w-full h-full flex items-center justify-center">
                      <div className="w-1.5 h-1.5 rounded-full bg-white" />
                    </div>
                  )}
                </div>

                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium text-foreground">
                    {t("settings.mirrorCustom")}
                  </div>
                  {isCustom && (
                    <div className="mt-2">
                      <Input
                        type="text"
                        value={mirrorConfig.custom_url || ""}
                        onChange={(e) =>
                          onConfigChange({
                            ...mirrorConfig,
                            custom_url: e.target.value || null,
                          })
                        }
                        placeholder="https://your-mirror.example/"
                        onClick={(e) => e.stopPropagation()}
                      />
                    </div>
                  )}
                </div>

                {/* Test custom mirror */}
                {isCustom && mirrorConfig.custom_url && (
                  <button
                    type="button"
                    onClick={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      handleTestMirror(mirrorConfig.custom_url!, "custom");
                    }}
                    disabled={testingId === "custom"}
                    className={cn(
                      "shrink-0 text-xs px-2.5 py-1 rounded-md border transition-colors cursor-pointer",
                      testResults["custom"] !== undefined
                        ? testResults["custom"] === "error"
                          ? "border-red-500/30 bg-red-500/10 text-red-400"
                          : "border-emerald-500/30 bg-emerald-500/10 text-emerald-400"
                        : "border-border text-muted-foreground hover:text-foreground hover:border-border/80"
                    )}
                  >
                    {testingId === "custom" ? (
                      <Loader2 className="w-3 h-3 animate-spin" />
                    ) : testResults["custom"] !== undefined ? (
                      testResults["custom"] === "error" ? (
                        <span className="flex items-center gap-1">
                          <WifiOff className="w-3 h-3" />
                          {t("settings.mirrorTestFail")}
                        </span>
                      ) : (
                        <span className="flex items-center gap-1">
                          <Wifi className="w-3 h-3" />
                          {testResults["custom"]}ms
                        </span>
                      )
                    ) : (
                      t("settings.mirrorTest")
                    )}
                  </button>
                )}
              </label>
            </div>

            <div className="flex items-center justify-end min-h-5">
              {mirrorSaving ? (
                <span className="text-xs text-muted-foreground">{t("common.saving")}</span>
              ) : mirrorSaved ? (
                <span className="text-xs text-success flex items-center gap-1">
                  <Check className="w-3 h-3" />
                  {t("common.saved")}
                </span>
              ) : null}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
