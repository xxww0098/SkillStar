import { Check, ChevronDown, Loader2, Plug } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Popover } from "radix-ui";
import { useState } from "react";
import { ProviderBrandIcon } from "../../../../components/shared/ProviderBrandIcon";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import {
  modelCompactPopoverTriggerClass,
  modelOptionClass,
  modelPopoverContentClass,
  modelPopoverTriggerClass,
} from "../providerForm/ProviderConfigPrimitives";

export interface ProviderSelectPopoverProps {
  providers: ProviderEntryFlat[];
  currentId?: string | null;
  onPick: (providerId: string) => void;
  onAddProvider?: () => void;
  disabled?: boolean;
  busy?: boolean;
  id?: string;
  ariaLabel: string;
  density?: "standard" | "compact";
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
  id,
  ariaLabel,
  density = "compact",
  triggerClassName,
}: ProviderSelectPopoverProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const current = providers.find((p) => p.id === currentId) ?? null;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          id={id}
          type="button"
          aria-label={ariaLabel}
          disabled={disabled || busy}
          className={cn(
            density === "standard" ? modelPopoverTriggerClass : modelCompactPopoverTriggerClass,
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
          className={cn(modelPopoverContentClass, "w-[var(--radix-popover-trigger-width)] min-w-[240px]")}
        >
          <div className="max-h-72 overflow-y-auto" role="listbox" aria-label={ariaLabel}>
            {providers.length === 0 ? (
              <div className="px-3 py-3 text-center text-[11px] text-muted-foreground">
                {t("models.picker.noCompatible")}
              </div>
            ) : (
              providers.map((p) => (
                <button
                  key={p.id}
                  type="button"
                  role="option"
                  aria-selected={currentId === p.id}
                  onClick={() => {
                    setOpen(false);
                    onPick(p.id);
                  }}
                  className={cn(modelOptionClass, currentId === p.id && "bg-primary/10 text-primary")}
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
                className={cn(modelOptionClass, "text-primary")}
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
