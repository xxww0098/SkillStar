import { Check, ChevronDown, Search, Sparkles } from "lucide-react";
import { Popover } from "radix-ui";
import { useMemo, useState } from "react";
import { Input } from "../../../../components/ui/input";
import { cn } from "../../../../lib/utils";
import type { ModelCatalogEntry } from "../../../../types";
import { formatModelMetadata } from "../../lib/modelFormat";

export interface ModelSelectPopoverProps {
  models: string[];
  catalog?: ModelCatalogEntry[];
  current: string;
  onPick: (model: string) => void;
  disabled?: boolean;
  /** Rendered at the bottom of the list, e.g. "管理模型列表 →". */
  footerAction?: { label: string; onClick: () => void };
}

/** The one model picker — used by agent cards and the agent settings dialog. */
export function ModelSelectPopover({
  models,
  catalog = [],
  current,
  onPick,
  disabled,
  footerAction,
}: ModelSelectPopoverProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return models;
    return models.filter((m) => {
      const meta = catalog.find((entry) => entry.id === m);
      return m.toLowerCase().includes(q) || meta?.display_name?.toLowerCase().includes(q);
    });
  }, [models, catalog, query]);

  return (
    <Popover.Root
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setQuery("");
      }}
    >
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled}
          className={cn(
            "flex w-full items-center gap-2 rounded-xl border border-input-border bg-input px-3 py-2 text-left text-xs transition",
            "hover:border-primary/30 focus:outline-none focus:ring-2 focus:ring-primary/40",
            "disabled:cursor-not-allowed disabled:opacity-50",
          )}
        >
          <Sparkles className="h-3.5 w-3.5 text-primary/80" />
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground">
            {current || "未选择模型"}
          </span>
          <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          sideOffset={6}
          className="z-[95] w-[var(--radix-popover-trigger-width)] min-w-[260px] rounded-xl border border-border/60 bg-card/95 p-1.5 shadow-[0_20px_60px_-24px_var(--color-shadow)] backdrop-blur-2xl"
        >
          {models.length > 8 ? (
            <div className="relative mb-1.5">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground/60" />
              <Input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="搜索模型..."
                className="h-7 pl-7 text-[11px]"
              />
            </div>
          ) : null}
          <div className="max-h-72 overflow-y-auto">
            {filtered.length === 0 ? (
              <div className="px-3 py-3 text-center text-[11px] text-muted-foreground">没有匹配的模型</div>
            ) : (
              filtered.map((m) => {
                const meta = catalog.find((entry) => entry.id === m);
                return (
                  <button
                    key={m}
                    type="button"
                    onClick={() => {
                      setOpen(false);
                      onPick(m);
                    }}
                    className={cn(
                      "flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left",
                      "transition hover:bg-primary/10",
                      m === current && "bg-primary/10",
                    )}
                  >
                    <span className="min-w-0 flex-1">
                      <span
                        className={cn(
                          "block truncate font-mono text-[11px]",
                          m === current ? "text-primary" : "text-foreground",
                        )}
                      >
                        {meta?.display_name ? `${meta.display_name} · ${m}` : m}
                      </span>
                      {meta ? (
                        <span className="block truncate text-[10px] text-muted-foreground">
                          {formatModelMetadata(meta)}
                        </span>
                      ) : null}
                    </span>
                    {m === current ? <Check className="h-3 w-3 shrink-0 text-primary" /> : null}
                  </button>
                );
              })
            )}
          </div>
          {footerAction ? (
            <div className="mt-1.5 border-t border-border/40 pt-1.5">
              <button
                type="button"
                onClick={() => {
                  setOpen(false);
                  footerAction.onClick();
                }}
                className="flex w-full cursor-pointer items-center justify-between rounded-lg px-2.5 py-1.5 text-left text-[11px] text-primary transition hover:bg-primary/10"
              >
                {footerAction.label}
              </button>
            </div>
          ) : null}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
