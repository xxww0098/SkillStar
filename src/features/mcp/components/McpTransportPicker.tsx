import { Cloud, Info, Radio, Sparkles, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { InfoTip } from "../../../components/ui/InfoTip";
import { cn } from "../../../lib/utils";

/**
 * Manual-create transport picker.
 *
 * Store values stay `stdio` / `http` / `sse`. `http` is Streamable HTTP — the
 * 2026-07-28 stateless protocol: no `initialize` handshake, no
 * `Mcp-Session-Id`. Showing the raw token "http" next to a deprecated "sse"
 * hid that ranking from anyone filling this form by hand.
 */

export type McpTransportId = "stdio" | "http" | "sse";

const OPTIONS: ReadonlyArray<{
  id: McpTransportId;
  icon: typeof Cloud;
  recommended?: boolean;
  deprecated?: boolean;
}> = [
  { id: "stdio", icon: Terminal },
  { id: "http", icon: Cloud, recommended: true },
  { id: "sse", icon: Radio, deprecated: true },
];

interface McpTransportPickerProps {
  value: string;
  onChange: (next: McpTransportId) => void;
}

export function McpTransportPicker({ value, onChange }: McpTransportPickerProps) {
  const { t } = useTranslation();

  return (
    <div>
      <div className="mb-1 flex items-center gap-1">
        <label className="text-xs font-medium leading-none tracking-tight text-foreground">
          {t("mcp.fieldTransport")}
        </label>
        <InfoTip content={t("mcp.fieldTransportTip")} />
      </div>
      <div role="radiogroup" aria-label={t("mcp.fieldTransport")} className="grid grid-cols-3 gap-1.5">
        {OPTIONS.map((option) => {
          const Icon = option.icon;
          const selected = value === option.id;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => onChange(option.id)}
              className={cn(
                "group relative flex min-h-[46px] min-w-0 cursor-pointer items-start gap-2 rounded-lg border p-2 text-left transition-all duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
                selected
                  ? option.deprecated
                    ? "border-amber-500/50 bg-amber-500/8 ring-1 ring-amber-500/30 shadow-2xs"
                    : "border-primary bg-primary/[0.07] ring-1 ring-primary/30 shadow-2xs"
                  : "border-border/70 bg-background/50 hover:border-border hover:bg-muted/30 hover:shadow-2xs",
              )}
            >
              <div
                className={cn(
                  "flex h-6 w-6 shrink-0 items-center justify-center rounded-md transition-colors duration-150",
                  selected
                    ? option.deprecated
                      ? "bg-amber-500/15 text-amber-600 paper:text-amber-700"
                      : "bg-primary/15 text-primary"
                    : "bg-muted/60 text-muted-foreground group-hover:bg-muted group-hover:text-foreground",
                )}
              >
                <Icon className="h-3.5 w-3.5" />
              </div>
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1 text-xs font-semibold leading-tight tracking-tight text-foreground">
                  {t(`mcp.transport_${option.id}`)}
                  {option.recommended ? <Sparkles className="h-2.5 w-2.5 shrink-0 text-primary" aria-hidden /> : null}
                </span>
                <span
                  className={cn(
                    "mt-0.5 block text-[10.5px] font-normal leading-tight",
                    selected && option.deprecated ? "text-amber-600 paper:text-amber-700" : "text-muted-foreground",
                  )}
                >
                  {t(`mcp.transportCaption_${option.id}`)}
                </span>
              </span>
            </button>
          );
        })}
      </div>
      <div
        className={cn(
          "mt-1.5 flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-[11px] transition-colors duration-150",
          value === "sse"
            ? "border-amber-500/30 bg-amber-500/8 text-amber-700 dark:text-amber-300"
            : "border-border/50 bg-muted/20 text-muted-foreground",
        )}
      >
        <Info className="h-3 w-3 shrink-0 opacity-70" />
        <span>{t(`mcp.transportHint_${value === "sse" || value === "http" ? value : "stdio"}`)}</span>
      </div>
    </div>
  );
}
