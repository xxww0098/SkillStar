import { Zap } from "lucide-react";
import { cn } from "../../../../lib/utils";

interface EndpointInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  label?: string;
  onSpeedTest?: () => void;
  className?: string;
  readOnly?: boolean;
}

export function EndpointInput({
  value,
  onChange,
  placeholder = "https://api.example.com",
  label = "API Endpoint",
  onSpeedTest,
  className,
  readOnly,
}: EndpointInputProps) {
  return (
    <div className={cn("space-y-1.5", className)}>
      <label className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{label}</label>
      <div className="relative flex items-center">
        <input
          type="url"
          value={value}
          onChange={(e) => {
            if (!readOnly) onChange(e.target.value);
          }}
          readOnly={readOnly}
          placeholder={placeholder}
          className={cn(
            "w-full h-9 px-3 pr-10 rounded-lg bg-background/60 border border-border text-sm text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary/40 transition font-mono",
            readOnly && "opacity-70 bg-muted/50 focus:ring-0 focus:border-border cursor-not-allowed",
          )}
        />
        {onSpeedTest && !readOnly && (
          <button
            type="button"
            onClick={onSpeedTest}
            className="absolute right-1.5 p-1.5 rounded-md text-muted-foreground hover:text-amber-500 hover:bg-amber-500/10 transition-colors"
            title="端点测速"
          >
            <Zap className="w-3.5 h-3.5" />
          </button>
        )}
      </div>
    </div>
  );
}
