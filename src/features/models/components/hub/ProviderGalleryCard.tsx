import { motion } from "framer-motion";
import { Copy, MoreHorizontal, Trash2 } from "lucide-react";
import { Popover } from "radix-ui";
import { useState, useMemo } from "react";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat, ToolActivationsMap } from "../../../../types";
import { getProviderToolBadges } from "../../hooks/useProvidersFlat";
import { AgentToolIcon, type AgentToolIconId } from "../shared/AgentToolIcon";
import { ProviderBrandIcon } from "../shared/ProviderBrandIcon";

const TOOL_ID_TO_ICON: Record<string, AgentToolIconId> = {
  "claude-code": "claude-code",
  codex: "codex",
  opencode: "opencode",
  gemini: "gemini",
};

export interface ProviderGalleryCardProps {
  provider: ProviderEntryFlat;
  toolActivations: ToolActivationsMap;
  onOpen: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
}

export function ProviderGalleryCard({
  provider,
  toolActivations,
  onOpen,
  onDuplicate,
  onDelete,
}: ProviderGalleryCardProps) {
  const [menuOpen, setMenuOpen] = useState(false);

  const activeBadges = useMemo(
    () => getProviderToolBadges(provider.id, toolActivations),
    [provider.id, toolActivations],
  );

  const hasOpenai = !!provider.base_url_openai;
  const hasAnthropic = !!provider.base_url_anthropic;
  const keySet = !!provider.api_key;

  return (
    <motion.div
      initial={{ opacity: 0, y: 10, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ duration: 0.25, ease: [0.22, 1, 0.36, 1] }}
      whileHover={{ y: -2 }}
      className="group relative"
    >
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          "relative flex h-full w-full cursor-pointer flex-col rounded-xl border border-border/55 bg-card/65 px-4 py-4 text-left backdrop-blur-2xl",
          "transition-all duration-300",
          "hover:border-primary/35 hover:bg-card-hover hover:shadow-[0_22px_60px_-32px_var(--color-shadow)]",
        )}
      >
        <div className="flex items-start gap-3">
          <ProviderBrandIcon
            presetId={provider.preset_id}
            providerName={provider.name}
            iconColor={provider.icon_color}
            size="md"
          />
          <div className="min-w-0 flex-1">
            <h3 className="truncate text-sm font-semibold text-foreground">{provider.name}</h3>
            <p className="mt-0.5 truncate text-[11px] text-muted-foreground">
              {provider.default_model || provider.models?.[0] || "未选择默认模型"}
            </p>
          </div>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-1.5">
          {keySet ? (
            <span className="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-400">
              Key
            </span>
          ) : (
            <span className="rounded-full bg-destructive/10 px-2 py-0.5 text-[10px] font-medium text-destructive">
              缺 Key
            </span>
          )}
          {hasOpenai ? (
            <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">OpenAI</span>
          ) : null}
          {hasAnthropic ? (
            <span className="rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
              Anthropic
            </span>
          ) : null}
        </div>

        <div className="mt-3 flex items-center gap-1.5 border-t border-border/40 pt-3">
          {activeBadges.length === 0 ? (
            <span className="text-[11px] text-muted-foreground/85">未绑定 Agent</span>
          ) : (
            <>
              <span className="text-[10px] uppercase tracking-wider text-muted-foreground/85">绑定</span>
              <span className="flex items-center gap-1">
                {activeBadges.map((toolId) => {
                  const iconId = TOOL_ID_TO_ICON[toolId];
                  if (!iconId) return null;
                  return (
                    <span key={toolId} className="inline-flex" title={toolId}>
                      <AgentToolIcon toolId={iconId} size="sm" />
                    </span>
                  );
                })}
              </span>
            </>
          )}
        </div>
      </button>

      {/* Hover-revealed action menu (top right) */}
      <div
        className={cn(
          "absolute right-3 top-3 transition-opacity",
          menuOpen ? "opacity-100" : "opacity-0 group-hover:opacity-100",
        )}
      >
        <Popover.Root open={menuOpen} onOpenChange={setMenuOpen}>
          <Popover.Trigger asChild>
            <button
              type="button"
              onClick={(e) => e.stopPropagation()}
              aria-label="更多操作"
              className="rounded-lg border border-border/50 bg-background/70 p-1.5 text-muted-foreground transition hover:border-primary/30 hover:text-foreground"
            >
              <MoreHorizontal className="h-3.5 w-3.5" />
            </button>
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Content
              align="end"
              sideOffset={6}
              onClick={(e) => e.stopPropagation()}
              className="z-[60] w-40 rounded-xl border border-border/60 bg-card/95 p-1 shadow-[0_20px_50px_-24px_var(--color-shadow)] backdrop-blur-2xl"
            >
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  onDuplicate();
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs transition hover:bg-primary/10"
              >
                <Copy className="h-3.5 w-3.5 text-muted-foreground" />
                复制
              </button>
              <button
                type="button"
                onClick={() => {
                  setMenuOpen(false);
                  onDelete();
                }}
                className="flex w-full cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-destructive transition hover:bg-destructive/10"
              >
                <Trash2 className="h-3.5 w-3.5" />
                删除
              </button>
            </Popover.Content>
          </Popover.Portal>
        </Popover.Root>
      </div>
    </motion.div>
  );
}
