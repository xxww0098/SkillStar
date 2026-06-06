import { motion } from "framer-motion";
import { CheckCircle2, Edit3, Globe, Lock, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import type { FingerprintRow } from "../types";
import { tlsLabel } from "../utils";

interface FingerprintCardProps {
  row: FingerprintRow;
  onSetActive: () => void;
  onEdit: () => void;
  onDelete: () => void;
}

/**
 * Compact card for one fingerprint. Shows name + TLS profile label +
 * proxy hint + activate/edit/delete actions. The immutable `"original"` row
 * hides both edit and delete buttons.
 */
export function FingerprintCard({ row, onSetActive, onEdit, onDelete }: FingerprintCardProps) {
  return (
    <motion.div
      layout
      className={cn(
        "group relative flex flex-col gap-3 rounded-3xl border bg-white/95 backdrop-blur-xl",
        "px-4 py-3.5 shadow-sm transition-all hover:shadow-md",
        row.isActive ? "border-emerald-400/60 ring-2 ring-emerald-200/50" : "border-zinc-200/60",
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate font-medium">{row.name}</span>
            {row.isOriginal && (
              <span className="inline-flex items-center gap-1 rounded-full bg-zinc-100/80 px-2 py-0.5 text-[10px] font-medium text-zinc-600">
                <Lock className="h-3 w-3" />
                只读
              </span>
            )}
            {row.isActive && (
              <span className="inline-flex items-center gap-1 rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] font-medium text-emerald-700">
                <CheckCircle2 className="h-3 w-3" />
                活跃
              </span>
            )}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1 rounded-md bg-zinc-50 px-1.5 py-0.5">
              <Globe className="h-3 w-3" />
              {tlsLabel(row.tls)}
            </span>
            {row.network.proxy_url && (
              <span className="inline-flex items-center gap-1 rounded-md bg-amber-50 px-1.5 py-0.5 text-amber-700">
                代理 {hostFromUrl(row.network.proxy_url)}
              </span>
            )}
            {row.network.egress_country && (
              <span className="rounded-md bg-zinc-50 px-1.5 py-0.5">出口 {row.network.egress_country}</span>
            )}
          </div>
        </div>
        <div className="flex shrink-0 gap-1">
          {!row.isActive && (
            <Button size="sm" variant="outline" className="h-7 text-xs" onClick={onSetActive} aria-label="set active">
              设为活跃
            </Button>
          )}
          {!row.isOriginal && (
            <Button
              size="sm"
              variant="ghost"
              className="h-7 w-7 p-0 text-muted-foreground hover:text-violet-600"
              onClick={onEdit}
              aria-label="edit fingerprint"
              title="编辑"
            >
              <Edit3 className="h-3.5 w-3.5" />
            </Button>
          )}
          {!row.isOriginal && (
            <Button
              size="sm"
              variant="ghost"
              className="h-7 w-7 p-0 text-muted-foreground hover:text-red-600"
              onClick={onDelete}
              aria-label="delete fingerprint"
              title="删除"
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          )}
        </div>
      </div>
      <UserAgentPreview ua={row.http.user_agent} />
    </motion.div>
  );
}

function UserAgentPreview({ ua }: { ua: string }) {
  return (
    <div className="rounded-xl border border-zinc-200/40 bg-zinc-50/60 px-2.5 py-1.5">
      <div className="font-mono text-[11px] leading-snug text-zinc-700 line-clamp-2">{ua}</div>
    </div>
  );
}

function hostFromUrl(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.slice(0, 28);
  }
}
