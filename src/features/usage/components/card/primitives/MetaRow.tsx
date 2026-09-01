import { cn } from "@/lib/utils";

export interface MetaRowProps {
  label: string;
  value: string;
  accent: string;
  showDivider?: boolean;
  className?: string;
}

/** Label / mono-value row used in Cursor secondary credits and similar. */
export function MetaRow({ label, value, accent, showDivider = false, className }: MetaRowProps) {
  const color = accent.startsWith("#") ? accent : `#${accent}`;
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-2 text-[10px]",
        showDivider && "border-t border-zinc-200/50 pt-2",
        className,
      )}
    >
      <span className="font-semibold uppercase tracking-wider text-zinc-600">{label}</span>
      <span className="font-mono font-bold tabular-nums" style={{ color }}>
        {value}
      </span>
    </div>
  );
}
