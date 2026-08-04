import { ArrowUpCircle, Filter, Loader2, RefreshCw, Search } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "../../../../../lib/utils";
import type { ProtoSkill, UpdateAllProtoState } from "./types";

type Props = {
  skills: ProtoSkill[];
  state: UpdateAllProtoState;
  onToggleFilter: () => void;
  onUpdateAll: () => void;
  onUpdateOne: (name: string) => void;
};

/**
 * UA1 — Actions CTA
 * Independent solid text button in the actions cluster;「仅更新」is filter-only.
 * Card update is a quiet ghost text control (no warning pulse).
 */
export function VariantUA1({ skills, state, onToggleFilter, onUpdateAll, onUpdateOne }: Props) {
  const visible = state.filterUpdateOnly ? skills.filter((s) => s.update_available) : skills;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="rounded-xl border border-border/70 bg-card/60 px-3 py-2">
        <div className="flex flex-wrap items-center gap-2">
          <div className="text-sm font-semibold tracking-tight">我的技能</div>
          <div className="ml-auto flex flex-wrap items-center gap-1.5">
            <FakeChip icon={<Search className="h-3.5 w-3.5" />} label="搜索" />
            <button
              type="button"
              onClick={onToggleFilter}
              className={cn(
                "flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs font-medium",
                state.filterUpdateOnly
                  ? "border-accent/50 bg-accent text-accent-foreground"
                  : "border-border bg-background text-muted-foreground hover:bg-muted/50",
              )}
            >
              <Filter className="h-3.5 w-3.5" />
              仅更新
              {state.pendingCount > 0 && (
                <span className="rounded-full bg-black/10 px-1.5 text-[10px] tabular-nums">{state.pendingCount}</span>
              )}
            </button>
            <FakeChip icon={<RefreshCw className="h-3.5 w-3.5" />} label="" square />

            {state.pendingCount > 0 && (
              <button
                type="button"
                disabled={state.busy}
                onClick={onUpdateAll}
                className={cn(
                  "flex h-8 items-center gap-1.5 rounded-lg bg-accent px-3 text-xs font-semibold text-accent-foreground shadow-sm",
                  state.busy && "opacity-60",
                )}
              >
                {state.busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowUpCircle className="h-3.5 w-3.5" />}
                更新 {state.pendingCount} 项
              </button>
            )}
          </div>
        </div>
        <p className="mt-1.5 text-[11px] text-muted-foreground">
          结构：主 CTA 独立成文案按钮；筛选与动作分离；卡片更新降为 ghost。
        </p>
      </div>

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {visible.map((skill) => (
          <article key={skill.name} className="rounded-xl border border-border/70 bg-card p-3 shadow-sm">
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <h3 className="truncate text-sm font-semibold">{skill.name}</h3>
                <p className="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{skill.description}</p>
                <p className="mt-1 truncate text-[10px] text-muted-foreground/80">{skill.source}</p>
              </div>
              {skill.update_available ? (
                <button
                  type="button"
                  disabled={state.busy}
                  onClick={() => onUpdateOne(skill.name)}
                  className="shrink-0 rounded-md px-2 py-1 text-[11px] text-muted-foreground underline-offset-2 hover:bg-muted/60 hover:text-foreground hover:underline"
                >
                  更新
                </button>
              ) : (
                <span className="shrink-0 rounded-md bg-success/10 px-2 py-1 text-[11px] text-success">已安装</span>
              )}
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}

function FakeChip({
  icon,
  label,
  square,
}: {
  icon: ReactNode;
  label: string;
  square?: boolean;
}) {
  return (
    <div
      className={cn(
        "flex h-8 items-center justify-center gap-1.5 rounded-lg border border-border/80 bg-background/50 text-xs text-muted-foreground",
        square ? "w-8" : "px-2.5",
      )}
    >
      {icon}
      {label}
    </div>
  );
}
