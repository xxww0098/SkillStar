import { Reorder, useDragControls } from "framer-motion";
import { GripVertical, Plus, Search } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { toast } from "sonner";
import { ProviderBrandIcon } from "@/features/models/components/ProviderBrandIcon";
import { DeleteConfirmDialog, ProviderContextMenu } from "@/features/models/components/ProviderContextMenu";
import { useProvidersFlat, getProviderToolBadges } from "@/features/models/hooks";
import { filterProviders } from "@/features/models/utils/filterProviders";
import { cn } from "@/lib/utils";
import type { ProviderEntryFlat, ToolActivationsMap } from "@/types";

export interface ModelsNavProps {
  selectedProviderId: string | null;
  onSelectProvider: (id: string) => void;
  /** Clear the selection. Called when the currently selected provider is deleted. */
  onClearSelection?: () => void;
  onAddProvider: () => void;
  collapsed: boolean;
}

/** Map tool_id to a short badge label */
const TOOL_BADGE_LABELS: Record<string, string> = {
  "claude-code": "C",
  codex: "X",
};

/** Latency dot color based on ms value */
function getLatencyDotColor(latencyMs: number | null | undefined): string {
  if (latencyMs == null) return "bg-muted-foreground/40"; // gray — untested
  if (latencyMs < 500) return "bg-emerald-400";
  if (latencyMs < 2000) return "bg-amber-400";
  return "bg-red-400";
}

export function ModelsNav({
  selectedProviderId,
  onSelectProvider,
  onClearSelection,
  onAddProvider,
  collapsed,
}: ModelsNavProps) {
  const { providers, toolActivations, reorderProviders, activateTool, deleteProvider, createProvider } =
    useProvidersFlat();
  const [searchQuery, setSearchQuery] = useState("");

  // Delete confirmation dialog state
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false);
  const [providerToDelete, setProviderToDelete] = useState<ProviderEntryFlat | null>(null);

  const affectedToolsForDelete = useMemo(() => {
    if (!providerToDelete) return [];
    return Object.entries(toolActivations)
      .filter(([, activation]) => activation?.provider_id === providerToDelete.id)
      .map(([toolId]) => toolId);
  }, [providerToDelete, toolActivations]);

  const handleOpenDeleteDialog = useCallback((provider: ProviderEntryFlat) => {
    setProviderToDelete(provider);
    setDeleteDialogOpen(true);
  }, []);

  const handleConfirmDelete = useCallback(async () => {
    if (!providerToDelete) return;
    const deletingSelected = providerToDelete.id === selectedProviderId;
    try {
      await deleteProvider(providerToDelete.id);
      toast.success(`已删除 ${providerToDelete.name}`);
      // Drop the persisted "last edited provider" if we just deleted it,
      // so the next Models entry doesn't try to reopen a missing record.
      if (deletingSelected) onClearSelection?.();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      toast.error(`删除失败: ${message}`);
    } finally {
      setDeleteDialogOpen(false);
      setProviderToDelete(null);
    }
  }, [providerToDelete, selectedProviderId, deleteProvider, onClearSelection]);

  const handleActivate = useCallback(
    async (provider: ProviderEntryFlat, toolId: string) => {
      await activateTool(provider.id, toolId);
    },
    [activateTool],
  );

  const handleActivateAll = useCallback(
    async (provider: ProviderEntryFlat) => {
      // Activate for all known tools
      await activateTool(provider.id, "claude-code");
      await activateTool(provider.id, "codex");
    },
    [activateTool],
  );

  const handleDuplicate = useCallback(
    async (provider: ProviderEntryFlat) => {
      await createProvider({
        name: `${provider.name} (副本)`,
        base_url_openai: provider.base_url_openai,
        base_url_anthropic: provider.base_url_anthropic,
        api_key: provider.api_key,
        models: provider.models,
        default_model: provider.default_model,
        icon_color: provider.icon_color,
        preset_id: provider.preset_id,
      });
    },
    [createProvider],
  );

  const filteredProviders = useMemo(() => filterProviders(providers, searchQuery), [providers, searchQuery]);

  const handleReorder = useCallback(
    (reordered: ProviderEntryFlat[]) => {
      const orderedIds = reordered.map((p) => p.id);
      void reorderProviders(orderedIds);
    },
    [reorderProviders],
  );

  // Empty state: no providers configured at all
  if (providers.length === 0) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex-1 flex flex-col items-center justify-center px-3 py-6">
          {!collapsed && <p className="text-xs text-muted-foreground text-center mb-3">尚未配置任何供应商</p>}
          <button
            onClick={onAddProvider}
            title={collapsed ? "新增供应商" : undefined}
            className={cn(
              "flex items-center gap-1.5 rounded-lg text-[12px] font-medium text-primary transition duration-150 cursor-pointer focus-ring",
              collapsed ? "justify-center w-8 h-8" : "px-2.5 py-1.5",
            )}
          >
            <Plus className="w-3.5 h-3.5" />
            {!collapsed && <span>新增供应商</span>}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Search input */}
      {!collapsed && (
        <div className="px-2 mb-2">
          <div className="relative">
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="搜索供应商..."
              className="w-full h-7 rounded-md border border-border/50 bg-muted/30 pl-7 pr-2 text-[12px] text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:ring-1 focus:ring-primary/40 focus:border-primary/50 transition"
            />
            <Search
              aria-hidden
              className="pointer-events-none absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-muted-foreground/60"
            />
          </div>
        </div>
      )}

      {/* Provider list */}
      <div className="flex-1 overflow-y-auto min-h-0">
        {filteredProviders.length === 0 && searchQuery.trim() ? (
          <div className="px-3 py-4 text-center">
            <p className="text-[11px] text-muted-foreground">无匹配结果</p>
          </div>
        ) : (
          <Reorder.Group
            axis="y"
            values={filteredProviders}
            onReorder={handleReorder}
            className="flex flex-col gap-0.5"
          >
            {filteredProviders.map((provider) => (
              <ProviderRow
                key={provider.id}
                provider={provider}
                isSelected={provider.id === selectedProviderId}
                toolActivations={toolActivations}
                collapsed={collapsed}
                onSelect={() => onSelectProvider(provider.id)}
                onActivate={(toolId) => handleActivate(provider, toolId)}
                onActivateAll={() => handleActivateAll(provider)}
                onDuplicate={() => handleDuplicate(provider)}
                onDelete={() => handleOpenDeleteDialog(provider)}
              />
            ))}
          </Reorder.Group>
        )}
      </div>

      {/* Add provider button — pinned at bottom of nav */}
      <div className={cn("shrink-0 pt-2 mt-1 border-t border-border/30", collapsed ? "px-1.5" : "px-2")}>
        <button
          onClick={onAddProvider}
          title={collapsed ? "新增供应商" : undefined}
          className={cn(
            "flex items-center gap-1.5 rounded-lg text-[12px] font-medium text-muted-foreground transition duration-150 cursor-pointer focus-ring w-full",
            collapsed ? "justify-center py-2" : "px-2.5 py-[7px]",
          )}
        >
          <Plus className="w-3.5 h-3.5" />
          {!collapsed && <span>新增供应商</span>}
        </button>
      </div>

      {/* Delete confirmation dialog */}
      <DeleteConfirmDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        providerName={providerToDelete?.name ?? ""}
        affectedTools={affectedToolsForDelete}
        onConfirm={() => void handleConfirmDelete()}
      />
    </div>
  );
}

/* ---------- Provider Row (Reorder.Item) ---------- */

interface ProviderRowProps {
  provider: ProviderEntryFlat;
  isSelected: boolean;
  toolActivations: ToolActivationsMap;
  collapsed: boolean;
  onSelect: () => void;
  onActivate: (toolId: string) => Promise<void>;
  onActivateAll: () => Promise<void>;
  onDuplicate: () => Promise<void>;
  onDelete: () => void;
}

function ProviderRow({
  provider,
  isSelected,
  toolActivations,
  collapsed,
  onSelect,
  onActivate,
  onActivateAll,
  onDuplicate,
  onDelete,
}: ProviderRowProps) {
  const dragControls = useDragControls();
  const badges = useMemo(() => getProviderToolBadges(provider.id, toolActivations), [provider.id, toolActivations]);
  const isActiveForAnyTool = badges.length > 0;

  // For latency, we use a placeholder since latency data is session-only
  // and not stored in the provider entry. The latency dot will show gray (untested)
  // until the user explicitly tests. In a future integration, latency results
  // could be stored in a separate state/context.
  const latencyMs: number | null = null;

  if (collapsed) {
    return (
      <Reorder.Item value={provider} dragListener={false} dragControls={dragControls} className="flex justify-center">
        <ProviderContextMenu
          provider={provider}
          onActivate={onActivate}
          onActivateAll={onActivateAll}
          onDuplicate={onDuplicate}
          onDelete={onDelete}
        >
          <button
            onClick={onSelect}
            title={provider.name}
            className={cn(
              "w-8 h-8 flex items-center justify-center rounded-lg transition duration-150 cursor-pointer focus-ring",
              isSelected ? "bg-primary/10" : isActiveForAnyTool ? "bg-muted/40" : "",
            )}
          >
            <ProviderBrandIcon
              presetId={provider.preset_id}
              providerName={provider.name}
              iconColor={provider.icon_color}
              size="xs"
              className="h-6 w-6 rounded-lg bg-transparent shadow-none"
            />
          </button>
        </ProviderContextMenu>
      </Reorder.Item>
    );
  }

  return (
    <Reorder.Item value={provider} dragListener={false} dragControls={dragControls} className="group">
      <ProviderContextMenu
        provider={provider}
        onActivate={onActivate}
        onActivateAll={onActivateAll}
        onDuplicate={onDuplicate}
        onDelete={onDelete}
      >
        <button
          onClick={onSelect}
          className={cn(
            "w-full flex items-center gap-2 rounded-lg text-[12px] px-2 py-[6px] transition duration-150 cursor-pointer focus-ring",
            isSelected
              ? "bg-primary/10 text-primary font-medium"
              : isActiveForAnyTool
                ? "bg-muted/30 text-foreground"
                : "text-muted-foreground",
          )}
        >
          {/* Drag handle */}
          <span
            onPointerDown={(e) => dragControls.start(e)}
            className="shrink-0 cursor-grab active:cursor-grabbing opacity-40 transition-opacity touch-none"
          >
            <GripVertical className="w-3 h-3" />
          </span>

          {/* Provider brand icon */}
          <ProviderBrandIcon
            presetId={provider.preset_id}
            providerName={provider.name}
            iconColor={provider.icon_color}
            size="xs"
            className="h-5 w-5 rounded-lg bg-transparent shadow-none"
          />

          {/* Provider name */}
          <span className="flex-1 truncate text-left">{provider.name}</span>

          {/* Tool badges */}
          {badges.length > 0 && (
            <div className="flex items-center gap-0.5 shrink-0">
              {badges.map((toolId) => (
                <span
                  key={toolId}
                  className="inline-flex items-center justify-center w-3.5 h-3.5 rounded text-[8px] font-bold bg-primary/15 text-primary"
                >
                  {TOOL_BADGE_LABELS[toolId] ?? toolId.charAt(0).toUpperCase()}
                </span>
              ))}
            </div>
          )}

          {/* Latency indicator dot */}
          <span
            className={cn("w-1.5 h-1.5 rounded-full shrink-0", getLatencyDotColor(latencyMs))}
            title={latencyMs != null ? `${latencyMs}ms` : "未测试"}
          />
        </button>
      </ProviderContextMenu>
    </Reorder.Item>
  );
}
