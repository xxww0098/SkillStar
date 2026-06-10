import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import type { ProviderSaveState } from "../../types";

/** Compact autosave state chip shown in the provider drawer header. */
export function SaveBadge({ state }: { state: ProviderSaveState }) {
  if (state === "saving") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-primary/20 bg-primary/10 px-2 py-0.5 text-[11px] font-medium text-primary">
        <Loader2 className="h-3 w-3 animate-spin" />
        保存中
      </span>
    );
  }
  if (state === "dirty") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[11px] font-medium text-amber-500">
        未保存
      </span>
    );
  }
  if (state === "error") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-destructive/25 bg-destructive/10 px-2 py-0.5 text-[11px] font-medium text-destructive">
        <AlertCircle className="h-3 w-3" />
        保存失败
      </span>
    );
  }
  if (state === "saved") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-success/20 bg-success/10 px-2 py-0.5 text-[11px] font-medium text-success">
        <CheckCircle2 className="h-3 w-3" />
        已保存
      </span>
    );
  }
  return null;
}
