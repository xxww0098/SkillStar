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
  onOpenMarket: () => void;
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
export function Mcp({ onOpenMarket, importRequest, onImportRequestHandled }: McpProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<McpTab>("fleet");

  useEffect(() => {
    if (!importRequest) return;
    setTab("fleet");
  }, [importRequest?.nonce]);

  const tabButton = ({ id, icon: Icon, label }: (typeof PRIMARY)[number], kind: "primary" | "secondary") => (
    <button
      key={id}
      type="button"
      onClick={() => setTab(id)}
      aria-current={tab === id ? "page" : undefined}
      className={cn(
        "inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs transition-all duration-150 cursor-pointer focus-ring select-none",
        tab === id
          ? kind === "primary"
            ? "bg-primary/18 text-primary font-semibold ring-1 ring-inset ring-primary/30 shadow-2xs dark:bg-primary/20"
            : "bg-muted text-foreground font-semibold ring-1 ring-inset ring-border"
          : "text-muted-foreground font-medium hover:bg-muted/50 hover:text-foreground",
      )}
    >
      <Icon className="h-3.5 w-3.5" strokeWidth={tab === id ? 2.4 : 2} />
      {t(label)}
    </button>
  );

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <nav className="flex shrink-0 items-center gap-1 border-b border-border/70 bg-sidebar px-6 py-2">
        {PRIMARY.map((item) => tabButton(item, "primary"))}
        <span className="mx-2 h-4 w-px bg-border/80" aria-hidden="true" />
        {SECONDARY.map((item) => tabButton(item, "secondary"))}
      </nav>

      <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden", tab !== "fleet" && "hidden")}>
        <McpManager
          onOpenMarket={onOpenMarket}
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
