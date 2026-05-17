import { Activity, Loader2, RefreshCw } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { cn } from "../../../lib/utils";
import type { AppId, LatencyResult, ProviderEntry } from "../../../types";
import { useLatencyTestLegacy as useLatencyTest } from "../hooks/useLatencyTestLegacy";
import { useProviders } from "../hooks/useProviders";
import { ProviderBrandIcon } from "./ProviderBrandIcon";

// --- Sorting utility ---

export interface ProviderWithLatency {
  provider: ProviderEntry;
  appId: AppId;
  result: LatencyResult | undefined;
}

/**
 * Sort providers by latency: ok results ascending by ms, then timeout/error last.
 * Within timeout/error group, order is stable (by name).
 */
export function sortByLatency(items: ProviderWithLatency[]): ProviderWithLatency[] {
  return [...items].sort((a, b) => {
    const aOk = a.result?.status === "ok";
    const bOk = b.result?.status === "ok";

    // Both ok: sort ascending by latency_ms
    if (aOk && bOk) {
      return (a.result?.latency_ms ?? 0) - (b.result?.latency_ms ?? 0);
    }
    // ok comes before non-ok
    if (aOk && !bOk) return -1;
    if (!aOk && bOk) return 1;

    // Neither ok: no data comes after timeout/error, stable by name
    const aHasResult = a.result != null;
    const bHasResult = b.result != null;
    if (!aHasResult && bHasResult) return 1;
    if (aHasResult && !bHasResult) return -1;

    return a.provider.name.localeCompare(b.provider.name);
  });
}

// --- Badge helpers ---

type LatencyBadgeVariant = "success" | "warning" | "destructive" | "outline";

function getLatencyBadgeVariant(result: LatencyResult | undefined): LatencyBadgeVariant {
  if (!result) return "outline";
  if (result.status === "timeout" || result.status === "error") return "destructive";
  if (result.latency_ms != null) {
    if (result.latency_ms < 500) return "success";
    if (result.latency_ms < 2000) return "warning";
    return "destructive";
  }
  return "outline";
}

function getLatencyLabel(result: LatencyResult | undefined): string {
  if (!result) return "No data";
  if (result.status === "timeout") return "Timeout";
  if (result.status === "error") return result.error_message ?? "Error";
  if (result.latency_ms != null) return `${result.latency_ms}ms`;
  return "No data";
}

// --- Sub-components ---

interface LatencyRowProps {
  item: ProviderWithLatency;
  onTest: () => void;
  isTesting: boolean;
}

const LatencyRow = memo(function LatencyRow({ item, onTest, isTesting }: LatencyRowProps) {
  const { provider, appId, result } = item;
  const badgeVariant = getLatencyBadgeVariant(result);
  const label = getLatencyLabel(result);

  return (
    <div
      className={cn(
        "flex items-center gap-3 px-4 py-3 rounded-lg border transition-colors",
        "bg-card/60 backdrop-blur-sm border-border/40",
        "hover:bg-card/80",
      )}
    >
      <ProviderBrandIcon
        presetId={provider.preset_id}
        providerName={provider.name}
        iconColor={provider.icon_color}
        size="xs"
        className="h-6 w-6 rounded-lg bg-background/50"
      />

      {/* Provider name + app badge */}
      <div className="flex items-center gap-2 min-w-0 flex-1">
        <span className="text-sm font-medium text-foreground truncate">{provider.name}</span>
        <Badge variant="outline" className="text-micro px-1.5 py-0 h-4 font-normal shrink-0 uppercase">
          {appId}
        </Badge>
      </div>

      {/* Latency badge */}
      <Badge variant={badgeVariant} className="text-micro px-2 py-0 h-5 font-medium tabular-nums shrink-0">
        {label}
      </Badge>

      {/* Test single button */}
      <Button
        variant="ghost"
        size="icon-xs"
        onClick={onTest}
        disabled={isTesting}
        title="Test latency"
        className="shrink-0"
      >
        <Activity className="w-3.5 h-3.5" />
      </Button>
    </div>
  );
});

// --- Main component ---

function HealthDashboardInner() {
  const claudeProviders = useProviders("claude");
  const codexProviders = useProviders("codex");
  const { results, testOne, testAll, isTesting, lastTestedAt } = useLatencyTest();
  const [testingAll, setTestingAll] = useState(false);

  // Combine all providers from both AppIds
  const allItems: ProviderWithLatency[] = useMemo(() => {
    const items: ProviderWithLatency[] = [];

    for (const p of claudeProviders.providers) {
      const key = `claude:${p.id}`;
      items.push({ provider: p, appId: "claude", result: results.get(key) });
    }
    for (const p of codexProviders.providers) {
      const key = `codex:${p.id}`;
      items.push({ provider: p, appId: "codex", result: results.get(key) });
    }

    return items;
  }, [claudeProviders.providers, codexProviders.providers, results]);

  // Sort by latency
  const sortedItems = useMemo(() => sortByLatency(allItems), [allItems]);

  const handleTestAll = useCallback(async () => {
    setTestingAll(true);
    try {
      await testAll("claude");
      await testAll("codex");
    } finally {
      setTestingAll(false);
    }
  }, [testAll]);

  const handleTestOne = useCallback(
    (item: ProviderWithLatency) => {
      testOne(
        item.appId,
        item.provider.id,
        item.provider.settings_config.base_url,
        item.provider.settings_config.api_key,
      );
    },
    [testOne],
  );

  const isLoading = claudeProviders.isLoading || codexProviders.isLoading;

  return (
    <div className="flex flex-col gap-5 h-full">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <Activity className="w-5 h-5 text-primary" />
          <h2 className="text-lg font-semibold text-foreground">Health Dashboard</h2>
        </div>

        <div className="flex items-center gap-3">
          {lastTestedAt && (
            <span className="text-xs text-muted-foreground">
              Last tested: {new Date(lastTestedAt).toLocaleTimeString()}
            </span>
          )}
          <Button variant="outline" size="sm" onClick={handleTestAll} disabled={isTesting || isLoading}>
            {testingAll ? (
              <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
            ) : (
              <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
            )}
            Test All
          </Button>
        </div>
      </div>

      {/* Results list */}
      <div className="flex flex-col gap-2 overflow-y-auto flex-1 pr-1">
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
          </div>
        ) : sortedItems.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
            <Activity className="w-8 h-8 mb-2 opacity-40" />
            <p className="text-sm">No providers configured</p>
            <p className="text-xs mt-1">Add providers in the Providers page to test latency</p>
          </div>
        ) : (
          sortedItems.map((item) => (
            <LatencyRow
              key={`${item.appId}:${item.provider.id}`}
              item={item}
              onTest={() => handleTestOne(item)}
              isTesting={isTesting}
            />
          ))
        )}
      </div>
    </div>
  );
}

export const HealthDashboard = memo(HealthDashboardInner);
