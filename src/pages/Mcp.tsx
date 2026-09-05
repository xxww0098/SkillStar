import { Boxes, Database, PackageSearch, Wrench } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { McpManager } from "../features/mcp/components/McpManager";
import { McpMarketPage } from "../features/mcp/components/McpMarketPage";
import { McpSourcesPanel } from "../features/mcp/components/McpSourcesPanel";
import { McpToolStatusPanel } from "../features/mcp/components/McpToolStatusPanel";
import { cn } from "../lib/utils";
import type { McpImportRequest } from "../lib/deepLink";

export interface McpProps {
  importRequest?: McpImportRequest | null;
  onImportRequestHandled?: () => void;
}

type McpTab = "fleet" | "catalog" | "tools" | "sources";

const PRIMARY: Array<{ id: McpTab; icon: typeof Boxes; label: string }> = [
  { id: "fleet", icon: Boxes, label: "mcp.tabFleet" },
  { id: "catalog", icon: PackageSearch, label: "mcp.tabMarket" },
];

const SECONDARY: Array<{ id: McpTab; icon: typeof Boxes; label: string }> = [
  { id: "tools", icon: Wrench, label: "mcp.tabTools" },
  { id: "sources", icon: Database, label: "mcp.tabSources" },
];

/**
 * MCP command center (Skills-mode sidebar entry).
 *
 * Primary segments are Fleet and Catalog — the daily install/run surface.
 * Tools and Sources stay reachable but visually secondary. The fleet view
 * stays mounted while hidden so a one-shot background probe and an in-flight
 * import drawer survive tab switches.
 */
export function Mcp({ importRequest, onImportRequestHandled }: McpProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<McpTab>("fleet");

  useEffect(() => {
    if (!importRequest) return;
    setTab("fleet");
  }, [importRequest?.nonce]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <nav
        aria-label={t("mcp.title")}
        className="flex h-11 shrink-0 items-center gap-2 border-b border-border/70 bg-sidebar px-6"
      >
        <div className="flex h-8 items-center rounded-lg border border-border/70 bg-sidebar/30 p-0.5">
          {PRIMARY.map(({ id, icon: Icon, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              aria-current={tab === id ? "page" : undefined}
              className={cn(
                "inline-flex h-full cursor-pointer items-center gap-1.5 rounded-md px-2.5 text-xs transition-colors duration-150 focus-ring select-none",
                tab === id
                  ? "bg-accent font-semibold text-accent-foreground"
                  : "font-medium text-muted-foreground hover:bg-sidebar-hover hover:text-foreground",
              )}
            >
              <Icon className="h-3.5 w-3.5" strokeWidth={tab === id ? 2.4 : 2} />
              {t(label)}
            </button>
          ))}
        </div>
        <div className="flex items-center gap-0.5">
          {SECONDARY.map(({ id, icon: Icon, label }) => (
            <button
              key={id}
              type="button"
              onClick={() => setTab(id)}
              aria-current={tab === id ? "page" : undefined}
              className={cn(
                "inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-md px-2 text-xs transition-colors duration-150 focus-ring select-none",
                tab === id
                  ? "bg-muted font-semibold text-foreground ring-1 ring-inset ring-border"
                  : "font-medium text-muted-foreground hover:bg-muted/50 hover:text-foreground",
              )}
            >
              <Icon className="h-3.5 w-3.5" strokeWidth={tab === id ? 2.4 : 2} />
              {t(label)}
            </button>
          ))}
        </div>
      </nav>

      <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden", tab !== "fleet" && "hidden")}>
        <McpManager
          onOpenMarket={() => setTab("catalog")}
          importRequest={importRequest}
          onImportRequestHandled={onImportRequestHandled}
        />
      </div>
      {tab === "catalog" ? (
        <McpMarketPage />
      ) : tab === "tools" || tab === "sources" ? (
        <main className="ss-page-scroll">
          <div className="ss-page-stack">{tab === "tools" ? <McpToolStatusPanel /> : <McpSourcesPanel />}</div>
        </main>
      ) : null}
    </div>
  );
}
