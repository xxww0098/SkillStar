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
      <div className="mb-1 flex items-center gap-1">
        <label className="text-xs font-medium text-foreground">{t("mcp.fieldTransport")}</label>
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
                "flex min-h-16 flex-col items-start gap-1 rounded-xl border px-2.5 py-2 text-left transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/40",
                selected
                  ? option.deprecated
                    ? "border-amber-500/50 bg-amber-500/8"
                    : "border-primary/60 bg-primary/8"
                  : "border-border/70 bg-background/40 hover:border-border hover:bg-muted/30",
              )}
            >
              <div className="flex w-full items-center gap-1">
                <Icon
                  className={cn(
                    "h-3.5 w-3.5 shrink-0",
                    option.deprecated
                      ? "text-amber-500 paper:text-amber-700"
                      : selected
                        ? "text-primary"
                        : "text-muted-foreground",
                  )}
                />
                {option.recommended ? <Sparkles className="ml-auto h-3 w-3 shrink-0 text-primary" aria-hidden /> : null}
              </div>
              <span className="text-[11px] font-medium leading-tight text-foreground">
                {t(`mcp.transport_${option.id}`)}
              </span>
              <span
                className={cn(
                  "text-micro leading-tight",
                  option.deprecated ? "text-amber-600 paper:text-amber-700" : "text-muted-foreground",
                )}
              >
                {t(`mcp.transportCaption_${option.id}`)}
              </span>
            </button>
          );
        })}
      </div>
      <p
        className={cn(
          "mt-1.5 text-[11px] leading-relaxed",
          value === "sse" ? "text-amber-600 paper:text-amber-700" : "text-muted-foreground",
        )}
      >
        {t(`mcp.transportHint_${value === "sse" || value === "http" ? value : "stdio"}`)}
      </p>
    </div>
  );
}
