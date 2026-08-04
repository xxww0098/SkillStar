import { ArrowUpCircle, Filter, Loader2, MoreHorizontal, RefreshCw, Search } from "lucide-react";
import { useState, type ReactNode } from "react";
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
 * UA2 — Context banner
 * Toolbar has no update action. A full-width banner under the bar owns the primary CTA.
 * Card update is a tiny icon tucked behind a overflow affordance.
 */
export function VariantUA2({ skills, state, onToggleFilter, onUpdateAll, onUpdateOne }: Props) {
  const visible = state.filterUpdateOnly ? skills.filter((s) => s.update_available) : skills;
  const [openMenu, setOpenMenu] = useState<string | null>(null);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-border/60 px-4 py-2">
        <div className="flex flex-wrap items-center gap-2">
          <div className="text-sm font-semibold">我的技能</div>
          <div className="ml-auto flex items-center gap-1.5">
            <ToolbarGhost icon={<Search className="h-3.5 w-3.5" />} />
            <button
              type="button"
              onClick={onToggleFilter}
              className={cn(
                "flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-xs",
                state.filterUpdateOnly
                  ? "border-accent/40 bg-accent/10 text-accent-foreground"
                  : "border-border text-muted-foreground",
              )}
            >
              <Filter className="h-3.5 w-3.5" />
              仅更新
            </button>
            <ToolbarGhost icon={<RefreshCw className="h-3.5 w-3.5" />} />
          </div>
        </div>
      </div>

      {state.pendingCount > 0 && (
        <div className="mx-4 mt-3 flex flex-wrap items-center gap-3 rounded-xl border border-warning/35 bg-warning/10 px-3 py-2.5">
          <div className="min-w-0 flex-1">
            <div className="text-sm font-semibold text-warning-foreground">{state.pendingCount} 个技能可更新</div>
            <div className="text-[11px] text-muted-foreground">一点更新全部（点击瞬间快照），不受上方筛选影响。</div>
          </div>
          <button
            type="button"
            disabled={state.busy}
            onClick={onUpdateAll}
            className={cn(
              "flex h-9 items-center gap-1.5 rounded-lg bg-warning px-3.5 text-xs font-semibold text-warning-foreground",
              state.busy && "opacity-60",
            )}
          >
            {state.busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <ArrowUpCircle className="h-3.5 w-3.5" />}
            更新 {state.pendingCount} 项
          </button>
        </div>
      )}

      <p className="px-4 pt-2 text-[11px] text-muted-foreground">
        结构：主 CTA 落在内容区横幅；Toolbar 只管筛选；单卡更新藏进 ⋯ 菜单。
      </p>

      <div className="grid flex-1 grid-cols-1 gap-2 overflow-auto p-4 sm:grid-cols-2 lg:grid-cols-3">
        {visible.map((skill) => (
          <article key={skill.name} className="relative rounded-xl border border-border/70 bg-card p-3">
            <div className="flex items-start gap-2">
              <div className="min-w-0 flex-1">
                <h3 className="truncate text-sm font-semibold">{skill.name}</h3>
                <p className="mt-0.5 line-clamp-2 text-[11px] text-muted-foreground">{skill.description}</p>
              </div>
              {skill.update_available ? (
                <div className="relative shrink-0">
                  <button
                    type="button"
                    className="rounded-md p-1 text-muted-foreground hover:bg-muted"
                    onClick={() => setOpenMenu((cur) => (cur === skill.name ? null : skill.name))}
                    aria-label="更多"
                  >
                    <MoreHorizontal className="h-4 w-4" />
                  </button>
                  {openMenu === skill.name && (
                    <div className="absolute right-0 top-7 z-10 min-w-[7rem] rounded-lg border border-border bg-popover p-1 shadow-lg">
                      <button
                        type="button"
                        disabled={state.busy}
                        className="block w-full rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted"
                        onClick={() => {
                          setOpenMenu(null);
                          onUpdateOne(skill.name);
                        }}
                      >
                        更新此技能
                      </button>
                    </div>
                  )}
                </div>
              ) : (
                <span className="shrink-0 text-[11px] text-success">已安装</span>
              )}
            </div>
            {skill.update_available && (
              <div className="mt-2 inline-flex rounded-full bg-warning/15 px-2 py-0.5 text-[10px] text-warning-foreground">
                有更新
              </div>
            )}
          </article>
        ))}
      </div>
    </div>
  );
}

function ToolbarGhost({ icon }: { icon: ReactNode }) {
  return (
    <div className="flex h-8 w-8 items-center justify-center rounded-lg border border-border/70 text-muted-foreground">
      {icon}
    </div>
  );
}
