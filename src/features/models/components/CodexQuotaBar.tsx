import { motion } from "framer-motion";
import { cn } from "../../../lib/utils";

interface QuotaBarProps {
  label: string;
  percentage: number;
  resetTime?: number;
  windowMinutes?: number;
  windowPresent?: boolean;
  compact?: boolean;
}

function formatResetTime(resetTimestamp?: number): string {
  if (!resetTimestamp) return "";
  const now = Math.floor(Date.now() / 1000);
  const diff = resetTimestamp - now;
  if (diff <= 0) return "即将重置";
  if (diff < 60) return `${diff}s`;
  if (diff < 3600) return `${Math.ceil(diff / 60)}m`;
  if (diff < 86400) {
    const h = Math.floor(diff / 3600);
    const m = Math.ceil((diff % 3600) / 60);
    return m > 0 ? `${h}h${m}m` : `${h}h`;
  }
  const d = Math.floor(diff / 86400);
  const h = Math.ceil((diff % 86400) / 3600);
  return h > 0 ? `${d}d${h}h` : `${d}d`;
}

function getQuotaColor(percentage: number): {
  bar: string;
  text: string;
  bg: string;
  glow: string;
} {
  if (percentage >= 70) {
    return {
      bar: "from-emerald-400 to-emerald-500",
      text: "text-emerald-400",
      bg: "bg-emerald-500/10",
      glow: "shadow-emerald-500/20",
    };
  }
  if (percentage >= 40) {
    return {
      bar: "from-amber-400 to-amber-500",
      text: "text-amber-400",
      bg: "bg-amber-500/10",
      glow: "shadow-amber-500/20",
    };
  }
  if (percentage >= 15) {
    return {
      bar: "from-orange-400 to-orange-500",
      text: "text-orange-400",
      bg: "bg-orange-500/10",
      glow: "shadow-orange-500/20",
    };
  }
  return {
    bar: "from-red-400 to-red-500",
    text: "text-red-400",
    bg: "bg-red-500/10",
    glow: "shadow-red-500/20",
  };
}

/** Semantic label for quota level */
function getQuotaSemanticLabel(percentage: number): { label: string; className: string } {
  if (percentage >= 70) return { label: "充裕", className: "text-emerald-400/70" };
  if (percentage >= 40) return { label: "适中", className: "text-amber-400/70" };
  if (percentage >= 15) return { label: "注意", className: "text-orange-400/70" };
  return { label: "低", className: "text-red-400/70" };
}

export function CodexQuotaBar({ label, percentage, resetTime, compact = false }: QuotaBarProps) {
  const color = getQuotaColor(percentage);
  const reset = formatResetTime(resetTime);
  const clampedPct = Math.max(0, Math.min(100, percentage));
  const semantic = getQuotaSemanticLabel(percentage);

  if (compact) {
    return (
      <div className="flex items-center gap-2">
        <span className="text-[10px] text-muted-foreground/70 w-7 shrink-0">{label}</span>
        <div className="flex-1 h-1.5 rounded-full bg-muted/40 overflow-hidden">
          <motion.div
            className={cn("h-full rounded-full bg-gradient-to-r", color.bar)}
            initial={{ width: 0 }}
            animate={{ width: `${clampedPct}%` }}
            transition={{ duration: 0.8, ease: [0.22, 1, 0.36, 1] }}
          />
        </div>
        <span className={cn("text-[10px] font-mono tabular-nums w-8 text-right", color.text)}>{clampedPct}%</span>
        <span className={cn("text-[9px] w-6 shrink-0", semantic.className)}>{semantic.label}</span>
      </div>
    );
  }

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium text-muted-foreground">{label}</span>
        <div className="flex items-center gap-2">
          {reset && <span className="text-[10px] text-muted-foreground/60 font-mono">重置 {reset}</span>}
          <span className={cn("text-[11px] font-bold font-mono tabular-nums", color.text)}>{clampedPct}%</span>
          <span className={cn("text-[10px] font-medium", semantic.className)}>{semantic.label}</span>
        </div>
      </div>
      <div className="relative h-2 rounded-full bg-muted/40 overflow-hidden">
        <motion.div
          className={cn("absolute inset-y-0 left-0 rounded-full bg-gradient-to-r", color.bar)}
          initial={{ width: 0 }}
          animate={{ width: `${clampedPct}%` }}
          transition={{ duration: 0.8, ease: [0.22, 1, 0.36, 1] }}
        />
        {/* Shine effect */}
        <motion.div
          className="absolute inset-y-0 left-0 rounded-full opacity-30"
          initial={{ width: 0 }}
          animate={{ width: `${clampedPct}%` }}
          transition={{ duration: 0.8, ease: [0.22, 1, 0.36, 1] }}
          style={{
            background: "linear-gradient(90deg, transparent 0%, rgba(255,255,255,0.3) 50%, transparent 100%)",
          }}
        />
      </div>
    </div>
  );
}
