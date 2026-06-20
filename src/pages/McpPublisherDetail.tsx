import { AnimatePresence, motion } from "framer-motion";
import { ArrowLeft, ArrowUp, Boxes, ExternalLink, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageToolbar } from "../components/layout/PageToolbar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/EmptyState";
import { ExternalAnchor } from "../components/ui/ExternalAnchor";
import { Input } from "../components/ui/input";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import { McpMarketBrowser } from "../features/mcp/components/McpMarketBrowser";
import { McpServerForm, type McpServerFormValue } from "../features/mcp/components/McpServerForm";
import { PUBLISHER_BRAND_ICON, hasPublisherBrandIcon } from "../features/mcp/components/McpPublishers";
import { PublisherAvatar } from "../features/marketplace/components/OfficialPublishers";
import { useMcpServers } from "../features/mcp/hooks/useMcpServers";
import { DrawerShell } from "../features/models";
import { tauriInvoke } from "../lib/ipc";
import { toast } from "../lib/toast";
import type {
  LocalFirstResult,
  McpMarketEntry,
  McpPublisherSummary,
  McpServerEntry,
  SnapshotStatus,
  ViewMode,
} from "../types";

interface McpPublisherDetailProps {
  publisher: McpPublisherSummary;
  onBack: () => void;
}

function draftToDefaults(draft: McpServerEntry): Partial<McpServerFormValue> {
  return {
    name: draft.name,
    transport: draft.transport,
    command: draft.command,
    args: draft.args,
    env: draft.env,
    url: draft.url,
    headers: draft.headers,
    description: draft.description,
    homepage: draft.homepage,
    enabled: {},
  };
}

export function McpPublisherDetail({ publisher, onBack }: McpPublisherDetailProps) {
  const { t } = useTranslation();
  const { servers: mcpServers, createServer: createMcpServer } = useMcpServers();
  const [entries, setEntries] = useState<McpMarketEntry[]>([]);
  const [status, setStatus] = useState<SnapshotStatus | undefined>(undefined);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [mcpInstallDrawer, setMcpInstallDrawer] = useState<{
    sourceName: string;
    defaults: Partial<McpServerFormValue>;
  } | null>(null);
  const [mcpSaving, setMcpSaving] = useState(false);
  const [viewMode] = useState<ViewMode>("grid");
  const scrollRef = useRef<HTMLDivElement>(null);

  const isGithub = publisher.id === "github";
  const hasBrandIcon = hasPublisherBrandIcon(publisher.id);
  const mcpInstalledNames = useMemo(() => new Set(mcpServers.map((server) => server.name)), [mcpServers]);

  // Reset transient state when the publisher changes.
  useEffect(() => {
    setSearchQuery("");
    setShowBackToTop(false);
  }, [publisher.id]);

  // Local-first load (mirrors useMcpMarketplace but scoped to one publisher).
  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    const loadLocal = () =>
      tauriInvoke("list_mcp_servers_by_publisher_local", {
        publisherId: publisher.id,
      }) as Promise<LocalFirstResult<McpMarketEntry[]>>;

    (async () => {
      try {
        const result = await loadLocal();
        if (cancelled) return;
        setEntries(result.data ?? []);
        setStatus(result.snapshot_status);

        // Only the GitHub publisher benefits from a remote registry refresh.
        if (isGithub && result.snapshot_status === "stale") {
          setRefreshing(true);
          try {
            await tauriInvoke("sync_mcp_market_scope", { scope: "mcp_registry" });
            const fresh = await loadLocal();
            if (!cancelled) {
              setEntries(fresh.data ?? []);
              setStatus(fresh.snapshot_status);
            }
          } finally {
            if (!cancelled) setRefreshing(false);
          }
        }
      } catch (e) {
        if (import.meta.env.DEV) console.error("Failed to load MCP publisher servers:", e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [publisher.id, isGithub]);

  // Client-side filter (the registry FTS is shared; here we keep it simple).
  const visibleEntries = useMemo(() => {
    if (!searchQuery.trim()) return entries;
    const q = searchQuery.toLowerCase();
    return entries.filter(
      (entry) =>
        entry.name.toLowerCase().includes(q) ||
        entry.namespace.toLowerCase().includes(q) ||
        entry.description.toLowerCase().includes(q),
    );
  }, [entries, searchQuery]);

  const handleMcpInstall = useCallback(
    async (id: string) => {
      try {
        const draft = (await tauriInvoke("mcp_market_entry_to_draft", { id })) as McpServerEntry;
        setMcpInstallDrawer({ sourceName: draft.name, defaults: draftToDefaults(draft) });
      } catch (e) {
        if (import.meta.env.DEV) console.error("[McpPublisherDetail] open install draft failed:", e);
        toast.error(t("mcp.installFailed", { defaultValue: "Failed to start install" }));
      }
    },
    [t],
  );

  const handleMcpSubmit = useCallback(
    async (value: McpServerFormValue) => {
      setMcpSaving(true);
      try {
        const entry: Partial<McpServerEntry> = { ...value };
        await createMcpServer(entry);
        toast.success(t("mcp.added"));
        setMcpInstallDrawer(null);
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      } finally {
        setMcpSaving(false);
      }
    },
    [createMcpServer, t],
  );

  return (
    <div className="flex-1 min-w-0 flex overflow-hidden relative">
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <PageToolbar
          title={
            <div className="flex items-center gap-2 min-w-0">
              <Button
                variant="ghost"
                size="sm"
                onClick={onBack}
                className="gap-1.5 text-muted-foreground hover:text-foreground -ml-2"
              >
                <ArrowLeft className="w-4 h-4" />
                {t("publisherDetail.back")}
              </Button>
              <div className="w-px h-5 bg-border mx-1" />
              <span className="text-sm font-semibold whitespace-nowrap truncate">{publisher.name}</span>
            </div>
          }
          actions={
            <div className="flex items-center gap-2">
              <div className="relative w-56">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
                <Input
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={t("mcp.searchPlaceholder", {
                    defaultValue: "Search MCP servers...",
                  })}
                  className="pl-8 h-8 text-xs"
                />
              </div>
              {refreshing && (
                <span className="text-xs text-muted-foreground whitespace-nowrap">
                  {t("marketplace.refreshingSnapshot", { defaultValue: "Refreshing snapshot..." })}
                </span>
              )}
            </div>
          }
        />

        <motion.main
          ref={scrollRef}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="ss-page-scroll"
          onScroll={(e) => {
            setShowBackToTop(e.currentTarget.scrollTop > 300);
          }}
        >
          {/* Hero banner */}
          <div className="px-6 pt-6 pb-5 border-b border-border bg-gradient-to-b from-primary/5 to-transparent">
            <div className="flex items-start gap-5 max-w-4xl">
              {hasBrandIcon ? (
                <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-primary/15 to-primary/5 border border-primary/10 flex items-center justify-center shrink-0">
                  {PUBLISHER_BRAND_ICON[publisher.id]}
                </div>
              ) : (
                <PublisherAvatar name={publisher.id} size="lg" />
              )}
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2.5 mb-1">
                  <h2 className="text-heading-lg truncate">{publisher.name}</h2>
                  <Badge
                    variant="outline"
                    className="text-micro px-2 py-0.5 h-5 font-medium text-primary bg-primary/8 border-primary/20 shrink-0"
                  >
                    {t("publisherDetail.official")}
                  </Badge>
                </div>

                <div className="flex items-center gap-4 mt-2 flex-wrap">
                  <span className="text-sm text-muted-foreground flex items-center gap-1.5">
                    <Boxes className="w-3.5 h-3.5" />
                    {t("publisherDetail.mcpServers", {
                      count: publisher.server_count,
                      defaultValue: "{{count}} servers",
                    })}
                  </span>
                  <ExternalAnchor
                    href={publisher.url}
                    className="text-sm text-primary/70 hover:text-primary flex items-center gap-1.5 transition-colors ml-auto"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    {t("publisherDetail.viewOnSkillsSh", { defaultValue: "Open" })}
                  </ExternalAnchor>
                </div>
              </div>
            </div>
          </div>

          {/* Servers */}
          {loading ? (
            <div className="flex items-center justify-center py-20">
              <LoadingLogo size="lg" label={t("mcp.marketLoading", { defaultValue: "Loading..." })} />
            </div>
          ) : visibleEntries.length === 0 ? (
            <EmptyState
              icon={<Boxes className="w-6 h-6 text-muted-foreground" />}
              title={t("mcp.marketEmptyTitle", { defaultValue: "No MCP servers" })}
              description={
                searchQuery.trim()
                  ? t("mcp.marketNoMatchesDescription", { defaultValue: "Try a different keyword." })
                  : t("mcp.marketEmptyDescription", { defaultValue: "Nothing here yet." })
              }
              size="lg"
            />
          ) : (
            <McpMarketBrowser
              installedNames={mcpInstalledNames}
              entries={visibleEntries}
              status={status}
              isLoading={false}
              query={searchQuery}
              refreshing={refreshing}
              viewMode={viewMode}
              onRefresh={() => setRefreshing((r) => r)}
              onInstall={(id) => void handleMcpInstall(id)}
            />
          )}
        </motion.main>

        <AnimatePresence>
          {showBackToTop && (
            <motion.button
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              transition={{ duration: 0.15 }}
              onClick={() => scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
              className="absolute bottom-8 right-8 z-40 w-10 h-10 rounded-full bg-background/80 hover:bg-background border border-border/50 text-foreground/80 hover:text-foreground shadow-sm hover:shadow-md backdrop-blur-md flex items-center justify-center transition duration-200 cursor-pointer group"
              title={t("publisherDetail.backToTop")}
            >
              <ArrowUp className="w-4 h-4 transition-transform duration-200 group-hover:-translate-y-0.5" />
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {/* Install form drawer (market entry → create server) */}
      <DrawerShell
        open={mcpInstallDrawer != null}
        onOpenChange={(open) => {
          if (!open) setMcpInstallDrawer(null);
        }}
        title={
          <span className="flex items-center gap-2 text-foreground">
            <Boxes className="h-4 w-4 text-primary" />
            {mcpInstallDrawer?.sourceName ?? t("mcp.addServer")}
          </span>
        }
        subtitle={t("mcp.drawerPresetSubtitle")}
      >
        {mcpInstallDrawer ? (
          <McpServerForm
            key={mcpInstallDrawer.sourceName}
            defaults={mcpInstallDrawer.defaults}
            submitLabel={t("common.add")}
            onSubmit={handleMcpSubmit}
            submitting={mcpSaving}
          />
        ) : null}
      </DrawerShell>
    </div>
  );
}
