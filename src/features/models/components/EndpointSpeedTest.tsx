import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";
import { AlertCircle, Check, Loader2, Plus, Trash2, X, Zap } from "lucide-react";
import { useCallback, useState } from "react";
import { cn } from "../../../lib/utils";

interface EndpointLatency {
  url: string;
  latency: number | null;
  status?: number;
  error?: string | null;
}

interface EndpointSpeedTestProps {
  open: boolean;
  onClose: () => void;
  endpoints?: string[];
  onSelect?: (url: string) => void;
}

const DEFAULT_ENDPOINTS = [
  "https://api.anthropic.com",
  "https://api.openai.com/v1",
  "https://api.deepseek.com",
  "https://open.bigmodel.cn/api/paas/v4",
  "https://api.moonshot.cn",
  "https://dashscope.aliyuncs.com",
  "https://openrouter.ai/api",
];

function getLatencyColor(ms: number | null): string {
  if (ms === null) return "text-destructive";
  if (ms < 300) return "text-emerald-500";
  if (ms < 500) return "text-yellow-500";
  if (ms < 800) return "text-amber-500";
  return "text-destructive";
}

function getLatencyBg(ms: number | null): string {
  if (ms === null) return "bg-destructive/5";
  if (ms < 300) return "bg-emerald-500/5";
  if (ms < 500) return "bg-yellow-500/5";
  if (ms < 800) return "bg-amber-500/5";
  return "bg-destructive/5";
}

export function EndpointSpeedTest({ open, onClose, endpoints: initialEndpoints, onSelect }: EndpointSpeedTestProps) {
  const [endpoints, setEndpoints] = useState<string[]>(
    initialEndpoints?.length ? initialEndpoints : [...DEFAULT_ENDPOINTS],
  );
  const [newUrl, setNewUrl] = useState("");
  const [testing, setTesting] = useState(false);
  const [results, setResults] = useState<EndpointLatency[]>([]);

  const addEndpoint = useCallback(() => {
    const trimmed = newUrl.trim();
    if (trimmed && !endpoints.includes(trimmed)) {
      setEndpoints((prev) => [...prev, trimmed]);
      setNewUrl("");
    }
  }, [newUrl, endpoints]);

  const removeEndpoint = useCallback((url: string) => {
    setEndpoints((prev) => prev.filter((u) => u !== url));
    setResults((prev) => prev.filter((r) => r.url !== url));
  }, []);

  const runTest = useCallback(async () => {
    setTesting(true);
    setResults([]);
    try {
      const res = await invoke<EndpointLatency[]>("test_model_endpoints", {
        urls: endpoints,
        timeoutSecs: 8,
      });
      res.sort((a, b) => {
        if (a.latency === null && b.latency === null) return 0;
        if (a.latency === null) return 1;
        if (b.latency === null) return -1;
        return a.latency - b.latency;
      });
      setResults(res);
    } catch (e) {
      console.error("Speed test failed:", e);
    } finally {
      setTesting(false);
    }
  }, [endpoints]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm"
          onClick={(e) => {
            if (e.target === e.currentTarget) onClose();
          }}
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.95, y: 20 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.95, y: 20 }}
            transition={{ type: "spring", stiffness: 400, damping: 30 }}
            className="w-full max-w-xl rounded-2xl border border-border bg-card shadow-2xl overflow-hidden"
          >
            {/* Header */}
            <div className="flex items-center justify-between px-5 py-4 border-b border-border">
              <h2 className="text-base font-semibold text-foreground flex items-center gap-2">
                <Zap className="w-5 h-5 text-amber-500" />
                端点测速
              </h2>
              <button
                type="button"
                onClick={onClose}
                className="p-1.5 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              >
                <X className="w-4 h-4" />
              </button>
            </div>

            {/* Body */}
            <div className="p-5 space-y-4 max-h-[60vh] overflow-y-auto scrollbar-thin">
              {/* Add endpoint */}
              <div className="flex gap-2">
                <input
                  type="url"
                  value={newUrl}
                  onChange={(e) => setNewUrl(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && addEndpoint()}
                  placeholder="添加自定义端点..."
                  className="flex-1 h-9 px-3 rounded-lg bg-background/60 border border-border text-sm text-foreground placeholder:text-muted-foreground/50 focus:outline-none focus:ring-1 focus:ring-amber-500/40 font-mono"
                />
                <button
                  type="button"
                  onClick={addEndpoint}
                  disabled={!newUrl.trim()}
                  className="px-3 h-9 rounded-lg bg-muted/50 border border-border text-muted-foreground hover:text-foreground disabled:opacity-30 transition-colors"
                >
                  <Plus className="w-4 h-4" />
                </button>
              </div>

              {/* Results / Endpoint list */}
              <div className="space-y-1.5">
                {(results.length > 0
                  ? results
                  : endpoints.map((url) => ({
                      url,
                      latency: null as number | null,
                      status: undefined,
                      error: null as string | null,
                    }))
                ).map((item) => (
                  <div
                    key={item.url}
                    className={cn(
                      "flex items-center justify-between px-3 py-2.5 rounded-xl border transition-all",
                      results.length > 0 && item.latency !== null
                        ? `${getLatencyBg(item.latency)} border-border/70`
                        : "bg-muted/30 border-border/50",
                    )}
                  >
                    <div className="flex items-center gap-2 min-w-0 flex-1">
                      {results.length > 0 &&
                        (item.latency !== null ? (
                          <Check className={cn("w-3.5 h-3.5 shrink-0", getLatencyColor(item.latency))} />
                        ) : (
                          <AlertCircle className="w-3.5 h-3.5 shrink-0 text-destructive" />
                        ))}
                      <span className="text-xs font-mono text-foreground/80 truncate">{item.url}</span>
                    </div>
                    <div className="flex items-center gap-2 shrink-0 ml-3">
                      {results.length > 0 && (
                        <span className={cn("text-xs font-semibold tabular-nums", getLatencyColor(item.latency))}>
                          {item.latency !== null ? `${item.latency}ms` : item.error ? "失败" : "—"}
                        </span>
                      )}
                      {onSelect && item.latency !== null && (
                        <button
                          type="button"
                          onClick={() => {
                            onSelect(item.url);
                            onClose();
                          }}
                          className="text-[10px] px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-600 hover:bg-emerald-500/20 transition-colors"
                        >
                          使用
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => removeEndpoint(item.url)}
                        className="p-1 rounded text-muted-foreground/40 hover:text-destructive transition-colors"
                      >
                        <Trash2 className="w-3 h-3" />
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Footer */}
            <div className="flex items-center justify-end gap-3 px-5 py-4 border-t border-border">
              <button
                type="button"
                onClick={onClose}
                className="px-4 py-2 rounded-lg text-sm text-muted-foreground hover:text-foreground hover:bg-muted/50 transition-colors"
              >
                关闭
              </button>
              <button
                type="button"
                onClick={runTest}
                disabled={testing || endpoints.length === 0}
                className="flex items-center gap-2 px-4 py-2 rounded-lg bg-amber-500 hover:bg-amber-600 text-white text-sm font-medium transition-colors disabled:opacity-50"
              >
                {testing ? <Loader2 className="w-4 h-4 animate-spin" /> : <Zap className="w-4 h-4" />}
                {testing ? "测速中..." : "开始测速"}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
