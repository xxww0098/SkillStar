import { motion } from "framer-motion";
import { ExternalLink, GripVertical, Pencil, RefreshCw, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { CatalogEntry, Subscription } from "../types";
import { PlanBadge } from "./PlanBadge";
import { ProviderLogo } from "./ProviderLogo";
import { UsageWindowBar } from "./UsageWindowBar";

interface SubscriptionCardProps {
  subscription: Subscription;
  catalog: CatalogEntry | undefined;
  onRefresh: (id: string) => Promise<void>;
  onEdit: (id: string) => void;
  onReauth?: (id: string) => void;
  /** Drag handle pointer-down; passed through to dnd lib. */
  onDragHandlePointerDown?: (e: React.PointerEvent) => void;
}

export function SubscriptionCard({
  subscription: sub,
  catalog,
  onRefresh,
  onEdit,
  onReauth,
  onDragHandlePointerDown,
}: SubscriptionCardProps) {
  const [refreshing, setRefreshing] = useState(false);
  const usage = sub.usage ?? null;
  const planName = (usage?.plan_name ?? sub.plan_tier ?? null) || null;
  const balance = usage?.balance ?? null;
  const renewDays = daysUntil(sub.renew_date);

  const handleRefresh = async () => {
    setRefreshing(true);
    try {
      await onRefresh(sub.id);
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <motion.article
      layout
      initial={{ opacity: 0, scale: 0.96 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.96 }}
      transition={{ duration: 0.18, ease: [0.22, 1, 0.36, 1] }}
      className={cn(
        "group relative flex flex-col rounded-2xl border bg-card/60 backdrop-blur-sm overflow-hidden",
        "border-border/60 hover:border-border transition-colors",
        "w-full sm:w-[240px] h-[280px] shrink-0",
        sub.requires_reauth && "border-red-500/40 ring-2 ring-red-500/20",
      )}
      aria-label={sub.display_name}
    >
      {/* ── Top bar ─────────────────────────────────────── */}
      <header className="flex items-start gap-2 p-3 pb-2">
        <ProviderLogo
          catalogId={sub.catalog_id}
          displayName={sub.display_name}
          brandColor={catalog?.brand_color ?? "6B7280"}
          size="md"
        />
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold truncate text-foreground">{sub.display_name}</h3>
          {catalog?.description && <p className="text-[10px] text-muted-foreground truncate">{catalog.description}</p>}
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <PlanBadge plan={planName} />
          <button
            type="button"
            onPointerDown={onDragHandlePointerDown}
            className="cursor-grab text-muted-foreground/40 opacity-0 group-hover:opacity-100 hover:text-muted-foreground transition-opacity"
            aria-label="拖动调整顺序"
            tabIndex={-1}
          >
            <GripVertical className="w-3.5 h-3.5" />
          </button>
        </div>
      </header>

      {/* ── Body: progress bars / balance / fallback ─────────────── */}
      <div className="flex-1 px-3 space-y-2 overflow-hidden">
        {usage?.hourly && <UsageWindowBar window={usage.hourly} />}
        {usage?.weekly && <UsageWindowBar window={usage.weekly} />}
        {usage?.monthly && <UsageWindowBar window={usage.monthly} />}
        {balance && <BalanceLine balance={balance} />}
        {!usage?.hourly && !usage?.weekly && !usage?.monthly && !balance && renderManual(sub)}
        {usage?.error && (
          <p className="text-[11px] text-amber-500 line-clamp-2" title={usage.error}>
            ⚠ {usage.error}
          </p>
        )}
      </div>

      {/* ── Footer ─────────────────────────────────────────────── */}
      <footer className="flex items-center justify-between gap-2 px-3 py-2 border-t border-border/40">
        <div className="text-[11px] text-muted-foreground truncate">
          {renewDays !== null ? (
            renewDays < 0 ? (
              <span className="text-red-400">已到期 {-renewDays}d</span>
            ) : renewDays === 0 ? (
              <span className="text-amber-400">今天到期</span>
            ) : renewDays <= 7 ? (
              <span className="text-amber-400">剩 {renewDays} 天</span>
            ) : (
              <span>剩 {renewDays} 天</span>
            )
          ) : (
            <span className="text-muted-foreground/60">未设到期</span>
          )}
        </div>
        <div className="flex items-center gap-0.5">
          {sub.requires_reauth ? (
            <Button size="icon-sm" variant="destructive" title="需要重新登录" onClick={() => onReauth?.(sub.id)}>
              <ShieldAlert className="w-3.5 h-3.5" />
            </Button>
          ) : (
            <Button
              size="icon-sm"
              variant="ghost"
              title="同步用量"
              onClick={handleRefresh}
              disabled={refreshing || sub.auth_mode === "manual"}
            >
              <RefreshCw className={cn("w-3.5 h-3.5", refreshing && "animate-spin")} />
            </Button>
          )}
          <Button size="icon-sm" variant="ghost" title="编辑" onClick={() => onEdit(sub.id)}>
            <Pencil className="w-3.5 h-3.5" />
          </Button>
          {catalog?.subscription_url && (
            <a
              href={catalog.subscription_url}
              target="_blank"
              rel="noreferrer"
              className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-accent/10 hover:text-foreground"
              title="去续费 / 打开控制台"
            >
              <ExternalLink className="w-3.5 h-3.5" />
            </a>
          )}
        </div>
      </footer>
    </motion.article>
  );
}

function daysUntil(epoch: number): number | null {
  if (!epoch || epoch <= 0) return null;
  const now = Math.floor(Date.now() / 1000);
  const diff = epoch - now;
  return Math.floor(diff / 86_400);
}

function BalanceLine({ balance }: { balance: NonNullable<Subscription["usage"]>["balance"] }) {
  if (!balance) return null;
  return (
    <div className="rounded-lg bg-muted/40 px-2 py-1.5">
      <div className="text-[10px] text-muted-foreground">余额</div>
      <div className="text-base font-semibold tabular-nums">
        {balance.currency === "CNY" ? "¥" : balance.currency === "USD" ? "$" : ""}
        {formatNumber(balance.total)}
      </div>
      {(balance.granted > 0 || balance.topped_up > 0) && (
        <div className="text-[10px] text-muted-foreground mt-0.5">
          赠 {formatNumber(balance.granted)} · 充 {formatNumber(balance.topped_up)}
        </div>
      )}
    </div>
  );
}

function renderManual(sub: Subscription) {
  const q = sub.manual_quota;
  if (!q || (!q.total_tokens && !q.used_tokens)) {
    return <p className="text-[11px] text-muted-foreground/70">未录入用量数据。点击编辑按钮维护。</p>;
  }
  const total = q.total_tokens ?? 0;
  const used = q.used_tokens ?? 0;
  const percent = total > 0 ? Math.round((used / total) * 100) : 0;
  return (
    <UsageWindowBar
      window={{
        label: q.period_label ?? "本月",
        used,
        total: q.total_tokens,
        percent,
        reset_at: null,
      }}
    />
  );
}

function formatNumber(n: number): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 10_000) {
    return `${(n / 10_000).toFixed(2)}万`;
  }
  return n.toFixed(2);
}
