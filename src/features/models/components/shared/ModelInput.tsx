import { ChevronDown, Loader2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "../../../../lib/utils";
import type { ModelListEntry } from "../../hooks/useModelFetch";

interface ModelInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  label?: string;
  className?: string;
  /** Pre-fetched model list for dropdown selection */
  fetchedModels?: ModelListEntry[];
  /** Whether models are currently being fetched */
  fetchingModels?: boolean;
}

export function ModelInput({
  value,
  onChange,
  placeholder = "model-name",
  label = "模型",
  className,
  fetchedModels,
  fetchingModels,
}: ModelInputProps) {
  const [dropdownOpen, setDropdownOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const hasModels = fetchedModels && fetchedModels.length > 0;

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!dropdownOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setDropdownOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [dropdownOpen]);

  return (
    <div className={cn("space-y-1.5", className)} ref={containerRef}>
      <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{label}</span>
      <div className="relative">
        <input
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className={cn(
            "w-full h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary/40 transition",
            hasModels && "pr-8",
          )}
        />
        {/* Dropdown trigger — only visible when we have fetched models */}
        {(hasModels || fetchingModels) && (
          <button
            type="button"
            onClick={() => setDropdownOpen(!dropdownOpen)}
            disabled={fetchingModels}
            className="absolute right-1.5 top-1/2 -translate-y-1/2 p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors disabled:opacity-50"
            title={fetchingModels ? "加载中..." : "选择模型"}
          >
            {fetchingModels ? (
              <Loader2 className="w-3.5 h-3.5 animate-spin" />
            ) : (
              <ChevronDown className={cn("w-3.5 h-3.5 transition-transform", dropdownOpen && "rotate-180")} />
            )}
          </button>
        )}

        {/* Dropdown list */}
        {dropdownOpen && hasModels && (
          <div className="absolute z-50 top-full mt-1 left-0 right-0 max-h-48 overflow-y-auto rounded-lg border border-border bg-card shadow-lg scrollbar-thin">
            {fetchedModels.map((model) => (
              <button
                key={model.id}
                type="button"
                onClick={() => {
                  onChange(model.id);
                  setDropdownOpen(false);
                }}
                className={cn(
                  "w-full text-left px-3 py-2 text-xs hover:bg-muted/50 transition-colors",
                  value === model.id ? "bg-primary/10 text-primary font-medium" : "text-foreground",
                )}
              >
                <span className="font-mono">{model.id}</span>
                {model.ownedBy && <span className="ml-2 text-muted-foreground/60">{model.ownedBy}</span>}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
