import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export const selectClass =
  "h-9 w-full rounded-md border border-input-border bg-input px-2 text-sm text-foreground focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/40";

export function Field({
  label,
  hint,
  className,
  children,
}: {
  label: string;
  hint?: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={cn("space-y-1", className)}>
      {label.trim() ? <label className="block text-[10px] font-semibold text-muted-foreground">{label}</label> : null}
      {children}
      {hint && <p className="text-[10px] leading-normal text-muted-foreground">{hint}</p>}
    </div>
  );
}

export function toDateInput(epoch: number | undefined | null): string {
  if (!epoch || epoch <= 0) return "";
  const d = new Date(epoch * 1000);
  const y = d.getFullYear();
  const m = `${d.getMonth() + 1}`.padStart(2, "0");
  const day = `${d.getDate()}`.padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function parseDateInput(s: string): number {
  if (!s) return 0;
  const d = new Date(`${s}T00:00:00`);
  if (Number.isNaN(d.getTime())) return 0;
  return Math.floor(d.getTime() / 1000);
}
