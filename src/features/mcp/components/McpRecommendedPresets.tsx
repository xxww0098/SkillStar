import { Sparkles, X } from "lucide-react";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import type { McpPreset } from "../../../types";

interface McpRecommendedPresetsProps {
  presets: readonly McpPreset[];
  /** Installed config keys, lowercased — already-added chips stay hidden. */
  installedNames: ReadonlySet<string>;
  selectedPresetId?: string | null;
  onPick: (preset: McpPreset) => void;
  onReset?: () => void;
  className?: string;
}

/**
 * Recommended presets chips section displayed inside the "Add Server" modal.
 * Clicking a preset pre-fills the create form (or launches the install wizard if curated).
 */
export function McpRecommendedPresets({
  presets,
  installedNames,
  selectedPresetId,
  onPick,
  onReset,
  className,
}: McpRecommendedPresetsProps) {
  const { t } = useTranslation();

  const available = useMemo(
    () => presets.filter((preset) => !installedNames.has(preset.name.trim().toLowerCase())),
    [presets, installedNames],
  );

  if (available.length === 0) return null;

  return (
    <div className={cn("rounded-xl border border-border/70 bg-muted/20 p-3.5 shadow-xs transition-colors", className)}>
      <div className="mb-2.5 flex items-center justify-between gap-2">
        <div className="flex items-center gap-1.5 text-xs font-semibold text-foreground">
          <Sparkles className="h-3.5 w-3.5 text-primary" />
          <span>{t("mcp.recommendedStrip")}</span>
          <span className="rounded-full bg-primary/10 px-1.5 py-0.5 text-[10px] font-medium text-primary">
            {available.length}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[11px] text-muted-foreground">{t("mcp.presetsTitle")}</span>
          {selectedPresetId && onReset ? (
            <button
              type="button"
              onClick={onReset}
              className="inline-flex items-center gap-1 text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground focus-ring rounded px-1.5 py-0.5"
            >
              <X className="h-3 w-3" />
              {t("common.reset")}
            </button>
          ) : null}
        </div>
      </div>

      <div className="flex max-h-36 flex-wrap gap-1.5 overflow-y-auto pr-1">
        {available.map((preset) => {
          const isSelected = selectedPresetId === preset.id;
          return (
            <button
              key={preset.id}
              type="button"
              onClick={() => onPick(preset)}
              title={preset.description || preset.name}
              aria-pressed={isSelected}
              className={cn(
                "group inline-flex items-center gap-1.5 rounded-lg border px-2.5 py-1 text-xs transition-all duration-150 focus-ring",
                isSelected
                  ? "border-primary bg-primary/10 font-medium text-primary shadow-xs"
                  : "border-border/60 bg-background/80 text-foreground/90 hover:border-primary/40 hover:bg-muted/50 hover:text-foreground",
              )}
            >
              <span className="font-medium">{preset.name}</span>
              {preset.transport ? (
                <span
                  className={cn(
                    "rounded px-1 py-0.5 font-mono text-[10px] uppercase tracking-wider",
                    isSelected
                      ? "bg-primary/20 text-primary"
                      : "bg-muted text-muted-foreground group-hover:text-foreground",
                  )}
                >
                  {preset.transport}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
