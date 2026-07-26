import { Activity, Boxes, Cable, Copy, Loader2, MoreHorizontal, SlidersHorizontal, Trash2 } from "lucide-react";
import { DropdownMenu, Tabs } from "radix-ui";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { DrawerShell } from "../../../../components/shared/DrawerShell";
import { ProviderBrandIcon } from "../../../../components/shared/ProviderBrandIcon";
import { cn } from "../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../types";
import { useAutosave } from "../../hooks/useAutosave";
import { useProviderForm } from "../../hooks/useProviderForm";
import type { ProviderEditorTab } from "../../types";
import { ConflictWarnings } from "../diagnostics/ConflictWarnings";
import { PostCreateGuide } from "./PostCreateGuide";
import { AdvancedTab } from "./tabs/AdvancedTab";
import { ConnectionTab } from "./tabs/ConnectionTab";
import { DiagnosticsTab } from "./tabs/DiagnosticsTab";
import { ModelsTab } from "./tabs/ModelsTab";

const TABS = [
  { id: "connection", labelKey: "models.tabs.connection", icon: Cable },
  { id: "models", labelKey: "models.tabs.models", icon: Boxes },
  { id: "advanced", labelKey: "models.tabs.advanced", icon: SlidersHorizontal },
  { id: "diagnostics", labelKey: "models.tabs.diagnostics", icon: Activity },
] satisfies { id: ProviderEditorTab; labelKey: string; icon: typeof Cable }[];

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
 * tab navigation. Close always flushes pending edits first (best-effort), then
 * dismisses — validation/network failure must never trap the user in the drawer.
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
  const { t } = useTranslation();
  const [guideDismissed, setGuideDismissed] = useState(false);
  const form = useProviderForm(provider);
  const { state: saveState, flush } = useAutosave({
    dirty: form.dirty,
    save: form.save,
    changeToken: form.values,
  });

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

  // Best-effort flush, then always dismiss. Blocking close on validation/network
  // error trapped users in the drawer (X / Esc / scrim all no-ops). Invalid
  // dirty state was never persisted anyway; save already toasts on failure.
  const closingRef = useRef(false);
  const requestClose = useCallback(async () => {
    if (closingRef.current) return;
    closingRef.current = true;
    try {
      await flush();
    } finally {
      onClose();
    }
  }, [flush, onClose]);

  return (
    <DrawerShell
      open={open}
      onOpenChange={(next) => {
        if (!next) void requestClose();
      }}
      maxWidthClassName="max-w-[640px]"
      autoFocus
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
      subtitle={<span>{t("models.drawer.subtitle")}</span>}
      headerAction={
        onDuplicate || onDelete ? (
          <DropdownMenu.Root>
            <DropdownMenu.Trigger asChild>
              <button
                type="button"
                aria-label={t("models.drawer.moreActions")}
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
                    {t("models.drawer.duplicate")}
                  </DropdownMenu.Item>
                ) : null}
                {onDelete ? (
                  <DropdownMenu.Item
                    onSelect={() => onDelete(provider)}
                    className="flex cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-xs text-destructive outline-none hover:bg-destructive/10"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                    {t("models.drawer.delete")}
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
                {t("models.save.footerSaving")}
              </span>
            ) : saveState === "dirty" ? (
              t("models.save.footerDirty")
            ) : saveState === "error" ? (
              <span className="text-destructive">{t("models.save.footerError")}</span>
            ) : (
              t("models.save.footerIdle")
            )}
          </span>
          <Button variant="outline" size="sm" onClick={() => void requestClose()}>
            {t("models.save.done")}
          </Button>
        </div>
      }
    >
      <div className="space-y-4">
        {showPostCreateGuide && !guideDismissed ? (
          <PostCreateGuide
            agentBound={agentBoundOnCreate}
            onTestConnection={() => setTab("diagnostics")}
            onGoConnect={() => void requestClose()}
            onDismiss={() => setGuideDismissed(true)}
          />
        ) : null}
        <ConflictWarnings providerId={provider.id} />

        <Tabs.Root value={tab} onValueChange={(next) => setTab(next as ProviderEditorTab)}>
          {/* Sticky, keyboard-navigable navigation inside the drawer scroll container. */}
          <Tabs.List
            className="sticky -top-5 z-10 -mx-1 grid grid-cols-4 gap-1 rounded-xl border border-border/55 bg-card/90 p-1 shadow-sm backdrop-blur-xl"
            aria-label={t("models.drawer.tablistAria")}
          >
            {TABS.map((tabDef) => {
              const Icon = tabDef.icon;
              return (
                <Tabs.Trigger
                  key={tabDef.id}
                  value={tabDef.id}
                  className={cn(
                    "flex min-h-9 min-w-0 items-center justify-center gap-1.5 rounded-[8px] px-2 text-xs font-semibold text-muted-foreground transition duration-200",
                    "hover:bg-background/40 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/35",
                    "data-[state=active]:bg-background data-[state=active]:text-foreground data-[state=active]:shadow-sm data-[state=active]:ring-1 data-[state=active]:ring-border/60",
                  )}
                >
                  <Icon className="h-3.5 w-3.5 shrink-0" />
                  <span className="truncate">{t(tabDef.labelKey)}</span>
                </Tabs.Trigger>
              );
            })}
          </Tabs.List>

          <Tabs.Content value="connection" className="mt-4 focus-visible:outline-none">
            <ConnectionTab form={form} />
          </Tabs.Content>
          <Tabs.Content value="models" className="mt-4 focus-visible:outline-none">
            <ModelsTab form={form} />
          </Tabs.Content>
          <Tabs.Content value="advanced" className="mt-4 focus-visible:outline-none">
            <AdvancedTab form={form} />
          </Tabs.Content>
          <Tabs.Content value="diagnostics" className="mt-4 focus-visible:outline-none">
            <DiagnosticsTab form={form} provider={provider} />
          </Tabs.Content>
        </Tabs.Root>
      </div>
    </DrawerShell>
  );
}

export function ProviderEditorDrawer(props: ProviderEditorDrawerProps) {
  // Remount when the provider identity changes so the form resets cleanly.
  return <ProviderEditorDrawerInner key={props.provider.id} {...props} />;
}
