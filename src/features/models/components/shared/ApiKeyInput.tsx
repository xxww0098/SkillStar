import { Check, Copy, ExternalLink, Eye, EyeOff } from "lucide-react";
import { useState } from "react";
import { cn } from "../../../../lib/utils";

interface ApiKeyInputProps {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  label?: string;
  className?: string;
  /** URL to get an API key — renders an external link button when provided */
  apiKeyUrl?: string;
}

export function ApiKeyInput({
  value,
  onChange,
  placeholder = "sk-...",
  label = "API Key",
  className,
  apiKeyUrl,
}: ApiKeyInputProps) {
  const [visible, setVisible] = useState(false);
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (!value) return;
    await navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={cn("space-y-1.5", className)}>
      <div className="flex items-center justify-between">
        <span className="text-xs font-medium text-muted-foreground uppercase tracking-wider">{label}</span>
        {apiKeyUrl && (
          <a
            href={apiKeyUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-1 text-[10px] text-muted-foreground/70 hover:text-primary transition-colors"
            title="获取 API Key"
          >
            <ExternalLink className="w-3 h-3" />
            <span>获取</span>
          </a>
        )}
      </div>
      <div className="relative flex items-center">
        <input
          type={visible ? "text" : "password"}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="w-full h-9 px-3 pr-20 rounded-lg bg-background/60 border border-border text-sm text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-primary/50 focus:border-primary/40 transition font-mono"
        />
        <div className="absolute right-1.5 flex items-center gap-0.5">
          {value && (
            <button
              type="button"
              onClick={handleCopy}
              className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              title={copied ? "Copied!" : "Copy"}
            >
              {copied ? <Check className="w-3.5 h-3.5 text-emerald-500" /> : <Copy className="w-3.5 h-3.5" />}
            </button>
          )}
          <button
            type="button"
            onClick={() => setVisible(!visible)}
            className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
            title={visible ? "Hide" : "Show"}
          >
            {visible ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>
    </div>
  );
}
