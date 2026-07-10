import { cn } from "@/lib/utils";

export interface MetricCardProps {
  label: string;
  value: string;
  /** Brand / accent hex (with or without `#`). Used for border + value color. */
  accent: string;
  className?: string;
}

/**
 * Compact KPI tile (DeepSeek analytics cost cards, etc.).
 * Label: 9px uppercase; value: mono tabular.
 */
export function MetricCard({ label, value, accent, className }: MetricCardProps) {
  const color = accent.startsWith("#") ? accent : `#${accent}`;
  return (
    <div
      className={cn("rounded-xl border px-3 py-2", className)}
      style={{ borderColor: `${color}20`, backgroundColor: `${color}06` }}
    >
      <p className="text-[9px] font-semibold uppercase tracking-wider text-zinc-500">{label}</p>
      <p className="mt-1 font-mono text-sm font-bold tabular-nums" style={{ color }}>
        {value}
      </p>
    </div>
  );
}
