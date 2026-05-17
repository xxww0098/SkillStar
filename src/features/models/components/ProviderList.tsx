import { useState } from "react";
import type { AppId } from "../../../types";
import { useLatencyTestLegacy as useLatencyTest } from "../hooks/useLatencyTestLegacy";
import { useProviders } from "../hooks/useProviders";
import { ProviderCard } from "./ProviderCard";

/**
 * Main feature component for the Models > Providers page.
 * Displays all providers grouped by AppId with CRUD actions.
 */
export function ProviderList() {
  const [selectedApp, setSelectedApp] = useState<AppId>("claude");
  const { providers, current } = useProviders(selectedApp);
  const { results, testOne } = useLatencyTest();

  return (
    <div className="space-y-6">
      {/* App tabs */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => setSelectedApp("claude")}
          className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
            selectedApp === "claude"
              ? "bg-primary/10 text-primary border border-primary/20"
              : "text-muted-foreground hover:text-foreground"
          }`}
        >
          Claude
        </button>
        <button
          type="button"
          onClick={() => setSelectedApp("codex")}
          className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
            selectedApp === "codex"
              ? "bg-primary/10 text-primary border border-primary/20"
              : "text-muted-foreground hover:text-foreground"
          }`}
        >
          Codex
        </button>
      </div>

      {/* Provider grid */}
      <div className="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
        {providers.map((provider) => (
          <ProviderCard
            key={provider.id}
            provider={provider}
            isActive={provider.id === current?.id}
            latency={results.get(`${selectedApp}:${provider.id}`)}
            onActivate={() => {}}
            onEdit={() => {}}
            onDelete={() => {}}
            onTest={() =>
              testOne(selectedApp, provider.id, provider.settings_config.base_url, provider.settings_config.api_key)
            }
          />
        ))}
      </div>

      {providers.length === 0 && (
        <div className="flex items-center justify-center min-h-[200px] text-muted-foreground">
          <p>No providers configured for {selectedApp === "claude" ? "Claude" : "Codex"}</p>
        </div>
      )}
    </div>
  );
}
