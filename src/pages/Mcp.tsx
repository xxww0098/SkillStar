import { Boxes, Database, PackageSearch, Wrench } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { McpManager } from "../features/mcp/components/McpManager";
import { McpMarketPage } from "../features/mcp/components/McpMarketPage";
import { McpSourcesPanel } from "../features/mcp/components/McpSourcesPanel";
import { McpToolStatusPanel } from "../features/mcp/components/McpToolStatusPanel";
import { cn } from "../lib/utils";

export interface McpProps {
  onOpenMarket: () => void;
}

type McpTab = "installed" | "market" | "tools" | "sources";

const TABS: Array<{ id: McpTab; icon: typeof Boxes; label: string }> = [
  { id: "installed", icon: Boxes, label: "mcp.tabInstalled" },
  { id: "market", icon: PackageSearch, label: "mcp.tabMarket" },
  { id: "tools", icon: Wrench, label: "mcp.tabTools" },
  { id: "sources", icon: Database, label: "mcp.tabSources" },
];

/**
 * MCP mode page (Skills-mode sidebar entry).
 *
 * Four views over the same domain, which is why they share a page rather than a
 * navigation entry each: the servers you have installed, the catalog they came
 * from, the agent config files they are written into, and the sources that
 * catalog is merged from. Three of those four had no UI at all before — the
 * catalog was reachable only by drilling into one publisher at a time, and tool
 * status and source health were read by the app and never shown (audit
 * D.3-1/7/8).
 *
 * The Marketplace MCP tab keeps its publisher-grid entry point; `onOpenMarket`
 * still leads there, and it now lands on the same paginated query this page's
 * Market tab uses, scoped to one publisher.
 */
export function Mcp({ onOpenMarket }: McpProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<McpTab>("installed");

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <nav className="flex shrink-0 items-center gap-1 border-b border-border/70 bg-sidebar px-6 py-2">
        {TABS.map(({ id, icon: Icon, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            aria-current={tab === id ? "page" : undefined}
            className={cn(
              "inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs transition-all duration-150 cursor-pointer focus-ring select-none",
              tab === id
                ? "bg-primary/18 text-primary font-semibold ring-1 ring-inset ring-primary/30 shadow-2xs dark:bg-primary/20"
                : "text-muted-foreground font-medium hover:bg-muted/50 hover:text-foreground",
            )}
          >
            <Icon className="h-3.5 w-3.5" strokeWidth={tab === id ? 2.4 : 2} />
            {t(label)}
          </button>
        ))}
      </nav>

      {tab === "installed" ? (
        <McpManager onOpenMarket={onOpenMarket} />
      ) : tab === "market" ? (
        <McpMarketPage />
      ) : (
        <main className="ss-page-scroll">
          <div className="ss-page-stack">{tab === "tools" ? <McpToolStatusPanel /> : <McpSourcesPanel />}</div>
        </main>
      )}
    </div>
  );
}
