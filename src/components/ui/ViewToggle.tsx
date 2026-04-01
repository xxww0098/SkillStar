import { cn } from "../../lib/utils";
import type { ViewMode } from "../../types";

interface ViewToggleProps {
  viewMode: ViewMode;
  onViewModeChange: (mode: ViewMode) => void;
  className?: string;
}

export function ViewToggle({ viewMode, onViewModeChange, className }: ViewToggleProps) {
  return (
    <div className={cn("ss-view-toggle", className)}>
      <button
        onClick={() => onViewModeChange("grid")}
        aria-label="Grid view"
        className={cn(
          "ss-view-toggle-btn",
          viewMode === "grid" && "ss-view-toggle-btn-active"
        )}
      >
        <div
          className={cn(
            "ss-view-toggle-indicator",
            viewMode === "grid" ? "opacity-100" : "opacity-0"
          )}
        />
        <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
          <rect x="1" y="1" width="5" height="5" rx="1" />
          <rect x="8" y="1" width="5" height="5" rx="1" />
          <rect x="1" y="8" width="5" height="5" rx="1" />
          <rect x="8" y="8" width="5" height="5" rx="1" />
        </svg>
      </button>
      <button
        onClick={() => onViewModeChange("list")}
        aria-label="List view"
        className={cn(
          "ss-view-toggle-btn",
          viewMode === "list" && "ss-view-toggle-btn-active"
        )}
      >
        <div
          className={cn(
            "ss-view-toggle-indicator",
            viewMode === "list" ? "opacity-100" : "opacity-0"
          )}
        />
        <svg width="14" height="14" viewBox="0 0 14 14" fill="currentColor">
          <rect x="1" y="2" width="12" height="2" rx="0.5" />
          <rect x="1" y="6" width="12" height="2" rx="0.5" />
          <rect x="1" y="10" width="12" height="2" rx="0.5" />
        </svg>
      </button>
    </div>
  );
}
