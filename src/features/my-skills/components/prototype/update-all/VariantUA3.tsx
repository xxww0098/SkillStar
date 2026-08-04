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
 * UA3 — Dual-zone header
 * Left: title + filters. Right: dedicated update dock (count + CTA) that owns the visual weight.
 * Card: installed badge stays primary; update is icon-only, muted, no label.
 */
export function VariantUA3({ skills, state, onToggleFilter, onUpdateAll, onUpdateOne }: Props) {
  const visible = state.filterUpdateOnly ? skills.filter((s) => s.update_available) : skills;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div className="grid gap-2 rounded-2xl border border-border/70 bg-card/70 p-2 md:grid-cols-[1fr_auto]">
        <div className="flex flex-wrap items-center gap-2 rounded-xl px-2 py-1.5">
          <div className="text-sm font-semibold">我的技能</div>
          <Ghost icon={<Search className="h-3.5 w-3.5" />} />
          <button
            type="button"
            onClick={onToggleFilter}
            className={cn(
              "flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs",
              state.filterUpdateOnly ? "border-primary/40 bg-primary/10 text-primary" : "border-border text-muted-foreground",
            )}
          >
            <Filter className="h-3.5 w-3.5" />
            仅更新
          </button>
          <Ghost icon={<RefreshCw className="h-3.5 w-3.5" />} />
        </div>

        <div
          className={cn(
            "flex min-w-[220px] items-center justify-between gap-3 rounded-xl border px-3 py-2",
            state.pendingCount > 0
              ? "border-accent/40 bg-accent/10"
              : "border-dashed border-border/60 bg-muted/20 text-muted-foreground",
          )}
        >
          <div className="min-w-0">
            <div className="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground">待更新</div>
            <div className="text-lg font-bold tabular-nums leading-none">{state.pendingCount}</div>
          </div>
          <button
            type="button"
            disabled={state.busy || state.pendingCount === 0}
            onClick={onUpdateAll}
            className={cn(
              "flex h-9 items-center gap-1.5 rounded-lg px-3 text-xs font-semibold",
              state.pendingCount > 0
                ? "bg-accent text-accent-foreground"
                : "cursor-not-allowed bg-muted text-muted-foreground",
              state.busy && "opacity-60",
            )}
          >
            {state.busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowUpCircle className="h-3.5 w-3.5" />}
            {state.pendingCount > 0 ? `更新 ${state.pendingCount} 项` : "已是最新"}
          </button>
        </div>
      </div>

      <p className="text-[11px] text-muted-foreground">
        结构：右侧固定「更新坞」承载数量+动作；卡片上更新缩成无文案小图标。
      </p>

      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {visible.map((skill) => (
          <article key={skill.name} className="rounded-xl border border-border/70 bg-card p-3">
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <h3 className="truncate text-sm font-semibold">{skill.name}</h3>
                <p className="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{skill.description}</p>
              </div>
              <div className="flex shrink-0 items-center gap-1">
                {skill.update_available && (
                  <button
                    type="button"
                    title="更新此技能"
                    disabled={state.busy}
                    onClick={() => onUpdateOne(skill.name)}
                    className="rounded-md p-1 text-muted-foreground/70 hover:bg-muted hover:text-foreground"
                  >
                    <ArrowUpCircle className="h-3.5 w-3.5" />
                  </button>
                )}
                <span className="rounded-md bg-success/10 px-2 py-1 text-[11px] text-success">已安装</span>
              </div>
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}

function Ghost({ icon }: { icon: ReactNode }) {
  return (
    <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-border/70 text-muted-foreground">
      {icon}
    </div>
  );
}
