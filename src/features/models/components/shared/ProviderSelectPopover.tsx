import { Check, ChevronDown, Loader2, Plug } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Popover } from "radix-ui";
import { useState } from "react";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

export interface ProviderSelectPopoverProps {
  providers: ProviderEntryFlat[];
  currentId?: string | null;
  onPick: (providerId: string) => void;
  onAddProvider?: () => void;
  disabled?: boolean;
  busy?: boolean;
  /** Extra classes for the trigger (status tint etc.). */
  triggerClassName?: string;
}

/** The one provider picker used by agent cards and the agent settings dialog. */
export function ProviderSelectPopover({
  providers,
  currentId,
  onPick,
  onAddProvider,
  disabled,
  busy,
  triggerClassName,
}: ProviderSelectPopoverProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const current = providers.find((p) => p.id === currentId) ?? null;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled || busy}
          className={cn(
            "flex w-full items-center gap-2 rounded-xl border border-input-border bg-input px-3 py-2 text-left text-xs transition",
            "hover:border-primary/30 focus:outline-none focus:ring-2 focus:ring-primary/40",
            "disabled:cursor-not-allowed disabled:opacity-50",
            triggerClassName,
          )}
        >
          {current ? (
            <>
              <ProviderBrandIcon
                presetId={current.preset_id}
                providerName={current.name}
                iconColor={current.icon_color}
                size="xs"
              />
              <span className="min-w-0 flex-1 truncate font-medium text-foreground">{current.name}</span>
            </>
          ) : (
            <>
              <span className="flex h-5 w-5 items-center justify-center rounded-md bg-muted text-muted-foreground">
                <Plug className="h-3 w-3" />
              </span>
              <span className="min-w-0 flex-1 truncate text-muted-foreground">{t("models.picker.pickProvider")}</span>
            </>
          )}
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin text-muted-foreground" />
          ) : (
            <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
          )}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          className="z-[95] w-[var(--radix-popover-trigger-width)] min-w-[240px] rounded-xl border border-border/60 bg-card/95 p-1.5 shadow-[0_20px_60px_-24px_var(--color-shadow)] backdrop-blur-2xl"
        >
          <div className="max-h-72 overflow-y-auto">
            {providers.length === 0 ? (
              <div className="px-3 py-3 text-center text-[11px] text-muted-foreground">
                {t("models.picker.noCompatible")}
              </div>
            ) : (
              providers.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => {
                    setOpen(false);
                    onPick(p.id);
                  }}
                  className={cn(
                    "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs",
                    "transition hover:bg-primary/10",
                    currentId === p.id && "bg-primary/10 text-primary",
                  )}
                >
                  <ProviderBrandIcon presetId={p.preset_id} providerName={p.name} iconColor={p.icon_color} size="xs" />
                  <span className="min-w-0 flex-1 truncate font-medium text-foreground">{p.name}</span>
                  {currentId === p.id ? <Check className="h-3 w-3 text-primary" /> : null}
                </button>
              ))
            )}
          </div>
          {onAddProvider ? (
            <div className="mt-1.5 border-t border-border/40 pt-1.5">
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  onAddProvider();
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs text-primary transition hover:bg-primary/10"
              >
                <Plug className="h-3.5 w-3.5" />
                {t("models.picker.addProvider")}
              </button>
            </div>
          ) : null}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
