import { Database, Wrench } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ModalHeader, ModalShell } from "../components/ui/ModalShell";
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

type McpView = "config" | "store";
type McpInspect = "tools" | "sources" | null;

function McpViewSwitch({ value, onChange }: { value: McpView; onChange: (next: McpView) => void }) {
  const { t } = useTranslation();
  const items: Array<{ id: McpView; label: string }> = [
    { id: "config", label: "mcp.tabConfig" },
    { id: "store", label: "mcp.tabStore" },
  ];

  return (
    <div
      role="tablist"
      aria-label={t("mcp.title")}
      className="flex h-8 items-center rounded-lg border border-border/70 bg-sidebar/30 p-0.5"
    >
      {items.map(({ id, label }) => (
        <button
          key={id}
          type="button"
          role="tab"
          aria-selected={value === id}
          onClick={() => onChange(id)}
          className={cn(
            "inline-flex h-full cursor-pointer items-center rounded-md px-2.5 text-xs transition-colors duration-150 focus-ring select-none",
            value === id
              ? "bg-accent font-semibold text-accent-foreground"
              : "font-medium text-muted-foreground hover:bg-sidebar-hover hover:text-foreground",
          )}
        >
          {t(label)}
        </button>
      ))}
    </div>
  );
}

/**
 * MCP page (Skills-mode sidebar entry) — two views, nothing else.
 *
 * **Config** is the installed servers: the answer to "what do I have, and is it
 * wired into the Agent I want". **Store** is the curated catalog you add from.
 * Agent-config and catalog-source inspectors are modals opened from the
 * matching view's toolbar, not peer tabs. The config view stays mounted while
 * hidden so a one-shot background probe and an in-flight import survive the
 * store hop.
 */
export function Mcp({ importRequest, onImportRequestHandled }: McpProps) {
  const { t } = useTranslation();
  const [view, setView] = useState<McpView>("config");
  const [inspect, setInspect] = useState<McpInspect>(null);
  const viewSwitch: ReactNode = <McpViewSwitch value={view} onChange={setView} />;

  useEffect(() => {
    if (!importRequest) return;
    setView("config");
    setInspect(null);
  }, [importRequest?.nonce]);

  const closeInspect = () => setInspect(null);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      <div className={cn("flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden", view !== "config" && "hidden")}>
        <McpManager
          title={viewSwitch}
          onOpenStore={() => setView("store")}
          onOpenTools={() => setInspect("tools")}
          importRequest={importRequest}
          onImportRequestHandled={onImportRequestHandled}
        />
      </div>
      {view === "store" ? <McpMarketPage title={viewSwitch} onOpenSources={() => setInspect("sources")} /> : null}

      <ModalShell
        open={inspect === "tools"}
        onClose={closeInspect}
        ariaLabel={t("mcp.toolStatusTitle")}
        panelClassName="max-w-[640px]"
        surfaceClassName="flex max-h-[min(720px,calc(100vh-2.5rem))] flex-col overflow-hidden"
        contentClassName="flex min-h-0 flex-col"
      >
        <ModalHeader
          icon={<Wrench className="h-4 w-4 text-primary" />}
          title={t("mcp.toolStatusTitle")}
          onClose={closeInspect}
        />
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5 pt-3">
          <McpToolStatusPanel />
        </div>
      </ModalShell>

      <ModalShell
        open={inspect === "sources"}
        onClose={closeInspect}
        ariaLabel={t("mcp.sourcesTitle")}
        panelClassName="max-w-[640px]"
        surfaceClassName="flex max-h-[min(720px,calc(100vh-2.5rem))] flex-col overflow-hidden"
        contentClassName="flex min-h-0 flex-col"
      >
        <ModalHeader
          icon={<Database className="h-4 w-4 text-primary" />}
          title={t("mcp.sourcesTitle")}
          onClose={closeInspect}
        />
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-5 pt-3">
          <McpSourcesPanel />
        </div>
      </ModalShell>
    </div>
  );
}
