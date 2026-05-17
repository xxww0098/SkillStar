import { Activity, Pencil, Trash2, Zap } from "lucide-react";
import { memo } from "react";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { LatencyResult, ProviderEntry } from "../../../types";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

export interface ProviderCardProps {
  provider: ProviderEntry;
  isActive: boolean;
  latency?: LatencyResult;
  onActivate: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onTest: () => void;
}

function getLatencyColor(latencyMs: number | null | undefined): string {
  if (latencyMs == null) return "text-muted-foreground";
  if (latencyMs < 500) return "text-emerald-400";
  if (latencyMs < 2000) return "text-amber-400";
  return "text-red-400";
}

function formatLatency(result?: LatencyResult): string {
  if (!result) return "—";
  if (result.status === "timeout") return "Timeout";
  if (result.status === "error") return "Error";
  if (result.latency_ms != null) return `${result.latency_ms}ms`;
  return "—";
}

function ProviderCardInner({ provider, isActive, latency, onActivate, onEdit, onDelete, onTest }: ProviderCardProps) {
  return (
    <div
      className={cn(
        "relative flex flex-col gap-3 p-4 rounded-xl border transition-all duration-200",
        "bg-card/80 backdrop-blur-sm border-border/60",
        "hover:bg-card-hover hover:-translate-y-0.5 hover:shadow-[0_8px_30px_-10px_var(--color-shadow)]",
        isActive && "border-primary/40 shadow-[0_0_12px_-4px_rgba(var(--color-primary-rgb),0.3)]",
      )}
    >
      {/* Header: name + category + provider icon */}
      <div className="flex items-center gap-2.5 min-w-0">
        <ProviderBrandIcon
          presetId={provider.preset_id}
          providerName={provider.name}
          iconColor={provider.icon_color}
          size="xs"
        />
        <span className="text-sm font-semibold text-foreground truncate">{provider.name}</span>
        <Badge variant="outline" className="text-micro px-1.5 py-0 h-4 font-normal shrink-0">
          {provider.category}
        </Badge>
      </div>

      {/* Status + Latency row */}
      <div className="flex items-center gap-2">
        {isActive ? (
          <Badge variant="success" className="text-micro px-1.5 py-0 h-4 font-medium">
            Active
          </Badge>
        ) : (
          <Badge variant="outline" className="text-micro px-1.5 py-0 h-4 font-medium text-muted-foreground">
            Inactive
          </Badge>
        )}

        <span className={cn("text-xs tabular-nums", getLatencyColor(latency?.latency_ms))}>
          {formatLatency(latency)}
        </span>
      </div>

      {/* Action buttons */}
      <div className="flex items-center gap-1 pt-1 border-t border-border/30">
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={onActivate}
          title="Activate"
          disabled={isActive}
          className={cn(isActive && "opacity-30")}
        >
          <Zap className="w-3.5 h-3.5" />
        </Button>
        <Button variant="ghost" size="icon-xs" onClick={onEdit} title="Edit">
          <Pencil className="w-3.5 h-3.5" />
        </Button>
        <Button variant="ghost" size="icon-xs" onClick={onDelete} title="Delete" className="hover:text-destructive">
          <Trash2 className="w-3.5 h-3.5" />
        </Button>
        <Button variant="ghost" size="icon-xs" onClick={onTest} title="Test latency" className="ml-auto">
          <Activity className="w-3.5 h-3.5" />
        </Button>
      </div>
    </div>
  );
}

export const ProviderCard = memo(ProviderCardInner);
