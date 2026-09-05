import { PackageSearch } from "lucide-react";
import { Popover } from "radix-ui";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { McpPreset } from "../../../types";

interface McpRecommendedStripProps {
  presets: readonly McpPreset[];
  /** Installed config keys, lowercased — already-added chips stay hidden. */
  installedNames: ReadonlySet<string>;
  onPick: (preset: McpPreset) => void;
  className?: string;
}

/**
 * Hermes catalog analog, as a compact popover rather than a page strip.
 * The 21k registry stays on the Catalog tab; this list is only presets.
 */
export function McpRecommendedStrip({ presets, installedNames, onPick, className }: McpRecommendedStripProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const available = useMemo(
    () => presets.filter((preset) => !installedNames.has(preset.name.trim().toLowerCase())),
    [presets, installedNames],
  );

  if (available.length === 0) return null;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <Button type="button" variant="outline" size="sm" className={cn("h-8 gap-1.5", className)} aria-expanded={open}>
          <PackageSearch className="h-3.5 w-3.5" />
          {t("mcp.recommendedStrip")}
          <span className="tabular-nums text-muted-foreground">{available.length}</span>
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="bottom"
          align="end"
          sideOffset={6}
          collisionPadding={12}
          className="z-50 w-[min(360px,calc(100vw-2rem))] max-h-[min(420px,70vh)] overflow-y-auto rounded-xl border border-border bg-card/95 p-1.5 shadow-xl backdrop-blur-xl animate-in fade-in-0 zoom-in-95"
        >
          <p className="px-2.5 py-1.5 text-[11px] leading-relaxed text-muted-foreground">{t("mcp.presetsTitle")}</p>
          <div className="flex flex-col gap-0.5">
            {available.map((preset) => (
              <button
                key={preset.id}
                type="button"
                onClick={() => {
                  onPick(preset);
                  setOpen(false);
                }}
                className="flex cursor-pointer flex-col items-start rounded-lg px-2.5 py-1.5 text-left transition-colors duration-150 hover:bg-muted/50 focus-ring"
              >
                <span className="text-xs font-medium text-foreground">{preset.name}</span>
                {preset.description ? (
                  <span className="mt-0.5 line-clamp-2 text-[11px] leading-relaxed text-muted-foreground">
                    {preset.description}
                  </span>
                ) : null}
              </button>
            ))}
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
