import { useEffect, useMemo, useState } from "react";
import { PrototypeSwitcher } from "../../../../../components/shared/PrototypeSwitcher";
import { MOCK_SKILLS } from "./mockSkills";
import { StateDump } from "./StateDump";
import type { ProtoSkill, UpdateAllProtoState, UpdateAllVariant } from "./types";
import { VariantUA1 } from "./VariantUA1";
import { VariantUA2 } from "./VariantUA2";
import { VariantUA3 } from "./VariantUA3";

const VARIANT_META = [
  { key: "UA1", name: "Actions CTA · ghost 单卡" },
  { key: "UA2", name: "内容横幅 · ⋯ 菜单单卡" },
  { key: "UA3", name: "右侧更新坞 · 图标单卡" },
] as const;

function readVariant(): UpdateAllVariant {
  if (typeof window === "undefined") return "UA1";
  const raw = new URLSearchParams(window.location.search).get("variant");
  if (raw === "UA2" || raw === "UA3") return raw;
  return "UA1";
}

/**
 * PROTOTYPE ONLY — Update-All primary CTA + demoted per-card update.
 * DEV: `/#skills?variant=UA1|UA2|UA3` or `bun run prototype:update-all`.
 */
export function UpdateAllPrototype() {
  const [variant, setVariant] = useState<UpdateAllVariant>(readVariant);
  const [skills, setSkills] = useState<ProtoSkill[]>(MOCK_SKILLS);
  const [filterUpdateOnly, setFilterUpdateOnly] = useState(false);
  const [busy, setBusy] = useState(false);
  const [lastAction, setLastAction] = useState("mount");
  const [snapshotNames, setSnapshotNames] = useState<string[]>([]);

  useEffect(() => {
    const sync = () => setVariant(readVariant());
    window.addEventListener("popstate", sync);
    window.addEventListener("prototype-variant-change", sync);
    return () => {
      window.removeEventListener("popstate", sync);
      window.removeEventListener("prototype-variant-change", sync);
    };
  }, []);

  const pending = useMemo(() => skills.filter((s) => s.update_available), [skills]);
  const pendingCount = pending.length;

  const state: UpdateAllProtoState = {
    variant,
    pendingCount,
    snapshotNames,
    busy,
    lastAction,
    filterUpdateOnly,
  };

  const setVariantParam = (key: string) => {
    const next: UpdateAllVariant = key === "UA2" || key === "UA3" ? key : "UA1";
    const url = new URL(window.location.href);
    url.searchParams.set("variant", next);
    window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
    setVariant(next);
    setLastAction(`switch:${next}`);
  };

  const onUpdateAll = () => {
    const names = skills.filter((s) => s.update_available).map((s) => s.name);
    setSnapshotNames(names);
    setBusy(true);
    setLastAction(`updateAll:snapshot(${names.length})`);
    window.setTimeout(() => {
      setSkills((prev) => prev.map((s) => (names.includes(s.name) ? { ...s, update_available: false } : s)));
      setBusy(false);
      setLastAction(`updateAll:done(${names.length})`);
    }, 900);
  };

  const onUpdateOne = (name: string) => {
    setLastAction(`updateOne:${name}`);
    setSkills((prev) => prev.map((s) => (s.name === name ? { ...s, update_available: false } : s)));
  };

  const shared = {
    skills,
    state,
    onToggleFilter: () => {
      setFilterUpdateOnly((v) => !v);
      setLastAction("toggleFilter");
    },
    onUpdateAll,
    onUpdateOne,
  };

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-background">
      <div className="shrink-0 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-[11px] text-amber-950 dark:text-amber-100">
        <strong>PROTOTYPE</strong> — 更新全部主路径 · 用底部切换器或 ← → 比较 UA1/UA2/UA3 · 不写真实更新
      </div>

      {variant === "UA1" && <VariantUA1 {...shared} />}
      {variant === "UA2" && <VariantUA2 {...shared} />}
      {variant === "UA3" && <VariantUA3 {...shared} />}

      <div className="shrink-0 px-4 pb-16">
        <StateDump state={{ ...state }} />
      </div>

      <PrototypeSwitcher variants={[...VARIANT_META]} current={variant} onChange={setVariantParam} />
    </div>
  );
}

export function isUpdateAllPrototypeActive(): boolean {
  if (import.meta.env.PROD) return false;
  if (typeof window === "undefined") return false;
  const variant = new URLSearchParams(window.location.search).get("variant");
  return variant === "UA1" || variant === "UA2" || variant === "UA3";
}
