import { Copy, Loader2, MoreHorizontal, Trash2 } from "lucide-react";
import { DropdownMenu } from "radix-ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "../../../../components/ui/button";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import { useAutosave } from "../../hooks/useAutosave";
import { useProviderForm } from "../../hooks/useProviderForm";
import type { ProviderEditorTab } from "../../types";
import { ConflictWarnings } from "../diagnostics/ConflictWarnings";
import { PostCreateGuide } from "./PostCreateGuide";
import { DrawerShell } from "../shared/DrawerShell";
import { ProviderBrandIcon } from "../shared/ProviderBrandIcon";
import { SaveBadge } from "../shared/SaveBadge";
import { AdvancedTab } from "./tabs/AdvancedTab";
import { ConnectionTab } from "./tabs/ConnectionTab";
import { DiagnosticsTab } from "./tabs/DiagnosticsTab";
import { ModelsTab } from "./tabs/ModelsTab";

const TABS: { id: ProviderEditorTab; label: string }[] = [
  { id: "connection", label: "连接" },
  { id: "models", label: "模型" },
  { id: "advanced", label: "高级" },
  { id: "diagnostics", label: "诊断" },
];

export interface ProviderEditorDrawerProps {
  provider: ProviderEntryFlat;
  open: boolean;
  onClose: () => void;
  onDuplicate?: (provider: ProviderEntryFlat) => void;
  onDelete?: (provider: ProviderEntryFlat) => void;
  /** Tab to show when the drawer opens (deep links: 缺端点 → connection 等). */
  initialTab?: ProviderEditorTab;
  /** Show the one-time post-create guide banner. */
  showPostCreateGuide?: boolean;
  /** Step 3 (接入 Agent) already done via autoBind. */
  agentBoundOnCreate?: boolean;
}

/**
 * Provider editor drawer — owns the form, the autosave state machine and the
 * tab navigation. Closing the drawer flushes any pending edit before the
 * debounce fires, so nothing is silently lost.
 */
function ProviderEditorDrawerInner({
  provider,
  open,
  onClose,
  onDuplicate,
  onDelete,
  initialTab = "connection",
  showPostCreateGuide = false,
  agentBoundOnCreate = false,
}: ProviderEditorDrawerProps) {
  const [tab, setTab] = useState<ProviderEditorTab>(initialTab);
  const [guideDismissed, setGuideDismissed] = useState(false);
  const form = useProviderForm(provider);
  const { state: saveState, flush } = useAutosave({ dirty: form.dirty, save: form.save });

  // Post-create convenience: fetch the model catalog once if credentials allow.
  const autoFetched = useRef(false);
  const { values: formValues, modelCatalogEmpty } = {
    values: form.values,
    modelCatalogEmpty: form.values.modelCatalog.length === 0,
  };
  useEffect(() => {
    if (!showPostCreateGuide || autoFetched.current) return;
    if (!formValues.modelsUrl.trim() || !formValues.apiKey.trim() || !modelCatalogEmpty) return;
    autoFetched.current = true;
    void form.handleFetchModels();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showPostCreateGuide]);

  const requestClose = useCallback(() => {
    // Kick the pending save off synchronously before unmount; the mutation
    // completes (and invalidates the cache) even after the drawer is gone.
    void flush();
    onClose();
  }, [flush, onClose]);

  return (
    <DrawerShell
      open={open}
      onOpenChange={(next) => {
        if (!next) requestClose();
      }}
      maxWidthClassName="max-w-[640px]"
      title={
        <span className="flex min-w-0 items-center gap-2 text-foreground">
          <ProviderBrandIcon
            presetId={provider.preset_id}
            providerName={provider.name}
            iconColor={provider.icon_color}
            size="sm"
          />
          <span className="truncate">{form.values.name || provider.name}</span>
        </span>
      }
      subtitle={
        <span className="flex items-center gap-2">
          <span>连接 · 模型 · 高级 · 诊断</span>
          <SaveBadge state={saveState} />
        </span>
      }
      headerAction={
        onDuplicate || onDelete ? (
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label="更多操作"
                className="shrink-0 cursor-pointer rounded-lg p-1.5 text-muted-foreground transition hover:bg-muted/50 hover:text-foreground focus:outline-none focus:ring-2 focus:ring-primary/40"
              >
                <MoreHorizontal className="h-4 w-4" />
              </button>
            </DropdownMenu.Trigger>
            <DropdownMenu.Portal>
              <DropdownMenu.Content
                align="end"
                sideOffset={6}
                className="z-[90] min-w-[150px] rounded-xl border border-border/60 bg-card/95 p-1 shadow-xl backdrop-blur-2xl"
              >
                {onDuplicate ? (
                  <DropdownMenu.Item
                    onSelect={() => onDuplicate(provider)}
                    className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-foreground outline-none hover:bg-muted/40"
                  >
                    <Copy className="h-3.5 w-3.5" />
                    复制供应商
                  </DropdownMenu.Item>
                ) : null}
                {onDelete ? (
                  <DropdownMenu.Item
                    onSelect={() => onDelete(provider)}
                    className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-destructive outline-none hover:bg-destructive/10"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    删除供应商…
                  </DropdownMenu.Item>
                ) : null}
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        ) : null
      }
      footer={
        <div className="flex items-center justify-between gap-3">
          <span className="text-[11px] text-muted-foreground">
            {saveState === "saving" ? (
              <span className="inline-flex items-center gap-1.5">
                <Loader2 className="h-3 w-3 animate-spin" />
                保存中…
              </span>
            ) : saveState === "dirty" ? (
              "改动将自动保存"
            ) : saveState === "error" ? (
              <span className="text-destructive">保存失败，请检查表单</span>
            ) : (
              "所有改动自动保存到本机"
            )}
          </span>
          <Button variant="outline" size="sm" onClick={requestClose}>
            完成
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        {showPostCreateGuide && !guideDismissed ? (
          <PostCreateGuide
            agentBound={agentBoundOnCreate}
            onTestConnection={() => setTab("diagnostics")}
            onGoConnect={requestClose}
            onDismiss={() => setGuideDismissed(true)}
          />
        ) : null}
        <ConflictWarnings providerId={provider.id} />

        {/* Tab bar — sticky inside the drawer scroll container */}
        <div className="sticky -top-5 z-10 -mx-1 rounded-xl border border-border/50 bg-card/90 p-1 backdrop-blur-xl">
          <div className="grid grid-cols-4 gap-1" role="tablist" aria-label="供应商配置页签">
            {TABS.map((t) => (
              <button
                key={t.id}
                type="button"
                role="tab"
                aria-selected={tab === t.id}
                onClick={() => setTab(t.id)}
                className={cn(
                  "rounded-lg px-2 py-1.5 text-xs font-medium transition-colors",
                  tab === t.id
                    ? "bg-primary/15 text-primary"
                    : "text-muted-foreground hover:bg-muted/30 hover:text-foreground",
                )}
              >
                {t.label}
              </button>
            ))}
          </div>
        </div>

        {tab === "connection" && <ConnectionTab form={form} />}
        {tab === "models" && <ModelsTab form={form} />}
        {tab === "advanced" && <AdvancedTab form={form} />}
        {tab === "diagnostics" && <DiagnosticsTab form={form} provider={provider} />}
      </div>
    </DrawerShell>
  );
}

export function ProviderEditorDrawer(props: ProviderEditorDrawerProps) {
  // Remount when the provider identity changes so the form resets cleanly.
  return <ProviderEditorDrawerInner key={props.provider.id} {...props} />;
}
