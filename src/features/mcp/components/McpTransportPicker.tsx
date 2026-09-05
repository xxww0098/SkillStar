import { Cloud, Radio, Sparkles, Terminal } from "lucide-react";
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
      <div className="mb-1.5 flex items-center gap-1">
        <label className="text-[13px] font-medium leading-none tracking-tight text-foreground">
          {t("mcp.fieldTransport")}
        </label>
        <InfoTip content={t("mcp.fieldTransportTip")} />
      </div>
      <div role="radiogroup" aria-label={t("mcp.fieldTransport")} className="grid grid-cols-3 gap-2">
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
                "flex min-h-12 min-w-0 cursor-pointer items-start gap-2.5 rounded-xl border px-3 py-2.5 text-left transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
                selected
                  ? option.deprecated
                    ? "border-amber-500/50 bg-amber-500/8"
                    : "border-primary/55 bg-primary/8"
                  : "border-border/70 bg-background/40 hover:border-border hover:bg-muted/30",
              )}
            >
              <Icon
                className={cn(
                  "mt-0.5 h-4 w-4 shrink-0",
                  option.deprecated
                    ? "text-amber-500 paper:text-amber-700"
                    : selected
                      ? "text-primary"
                      : "text-muted-foreground",
                )}
              />
              <span className="min-w-0 flex-1">
                <span className="flex items-center gap-1 text-[13px] font-semibold leading-tight tracking-tight text-foreground">
                  {t(`mcp.transport_${option.id}`)}
                  {option.recommended ? <Sparkles className="h-3 w-3 shrink-0 text-primary" aria-hidden /> : null}
                </span>
                <span
                  className={cn(
                    "mt-0.5 block text-micro font-medium leading-snug",
                    option.deprecated ? "text-amber-600 paper:text-amber-700" : "text-muted-foreground",
                  )}
                >
                  {t(`mcp.transportCaption_${option.id}`)}
                </span>
              </span>
            </button>
          );
        })}
      </div>
      <p className={cn("mt-2 text-caption", value === "sse" ? "text-amber-600 paper:text-amber-700" : undefined)}>
        {t(`mcp.transportHint_${value === "sse" || value === "http" ? value : "stdio"}`)}
      </p>
    </div>
  );
}
