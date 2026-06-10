import { Loader2, Zap } from "lucide-react";
import { memo, useCallback, useMemo } from "react";
import { Button } from "../../../../components/ui/button";
import { cn } from "../../../../lib/utils";
import { useEndpointSpeedTest } from "../../api/diagnostics";
import { endpointProbeLabel, endpointProbeTone, isEndpointReachable } from "../../lib/endpointProbe";

export interface EndpointSpeedPanelProps {
  urls: string[];
  apiKey: string;
  onApplyFastest?: (url: string, field: "openai" | "anthropic" | "models") => void;
  className?: string;
}

function EndpointSpeedPanelInner({ urls, apiKey, onApplyFastest, className }: EndpointSpeedPanelProps) {
  const { testEndpoints, results, isLoading, clearResults } = useEndpointSpeedTest();

  const handleTest = useCallback(() => {
    void testEndpoints(urls, apiKey);
  }, [testEndpoints, urls, apiKey]);

  const fastest = useMemo(() => {
    const ok = results.filter(isEndpointReachable);
    if (ok.length === 0) return null;
    return ok.reduce((best, cur) =>
      (cur.latency_ms ?? Number.MAX_SAFE_INTEGER) < (best.latency_ms ?? Number.MAX_SAFE_INTEGER) ? cur : best,
    );
  }, [results]);

  const canTest = urls.some((u) => u.trim()) && apiKey.trim();

  return (
    <div className={cn("space-y-2.5 rounded-xl border border-border/50 bg-muted/15 px-3 py-2.5", className)}>
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs font-medium text-muted-foreground">端点测速</p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={handleTest}
          disabled={isLoading || !canTest}
          className="h-7 gap-1.5 text-xs"
          aria-label="测速所有端点"
        >
          {isLoading ? <Loader2 className="h-3 w-3 animate-spin" /> : <Zap className="h-3 w-3" />}
          {isLoading ? "测速中…" : "测速端点"}
        </Button>
      </div>

      {!canTest && (
        <p className="text-[11px] leading-4 text-muted-foreground">填写 API Key 与至少一个端点 URL 后可测速</p>
      )}
      <p className="text-[10px] leading-4 text-muted-foreground/80">
        测速走设置里的系统代理。若超时或连接失败，请先在「设置 → 代理」启用本地代理（如 Clash）。
      </p>

      {results.length > 0 && (
        <ul className="space-y-1.5">
          {results.map((result) => (
            <li
              key={result.url}
              className={cn(
                "flex items-start justify-between gap-2 rounded-lg border px-2.5 py-1.5 text-[11px]",
                endpointProbeTone(result) === "ok" && "border-success/25 bg-success/5 text-foreground",
                endpointProbeTone(result) === "auth" && "border-amber-500/30 bg-amber-500/5 text-amber-600",
                endpointProbeTone(result) === "error" && "border-destructive/20 bg-destructive/5 text-destructive",
              )}
            >
              <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-muted-foreground">{result.url}</span>
              <span className="shrink-0 font-medium">{endpointProbeLabel(result)}</span>
            </li>
          ))}
        </ul>
      )}

      {fastest && onApplyFastest && (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 w-full text-xs text-primary"
          onClick={() => {
            const url = fastest.url;
            if (url.includes("/models")) {
              onApplyFastest(url, "models");
            } else if (url.includes("anthropic")) {
              onApplyFastest(url, "anthropic");
            } else {
              onApplyFastest(url, "openai");
            }
            clearResults();
          }}
        >
          应用最快端点 ({fastest.latency_ms}ms)
        </Button>
      )}
    </div>
  );
}

export const EndpointSpeedPanel = memo(EndpointSpeedPanelInner);
