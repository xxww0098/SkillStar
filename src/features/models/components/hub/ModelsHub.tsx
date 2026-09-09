import { Loader2 } from "lucide-react";
import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { ProviderEntryFlat } from "../../../../types";
import { getProviderToolBadges, useProvidersFlat } from "../../hooks/useProvidersFlat";
import { ProviderEditorDrawer } from "../provider/ProviderEditorDrawer";
import { DeleteProviderDialog } from "./DeleteProviderDialog";
import { isNativeOfficialProvider } from "../../lib/officialProviders";
import { Button } from "@/components/ui/button";
import { ClaudeWorkbench } from "./claude/ClaudeWorkbench";
import { ClaudeProviderCreate } from "./claude/ClaudeProviderCreate";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { useModelsData } from "../../hooks/useModelsData";

/** Claude workspace with shared provider CRUD and complete deletion impact. */
export function ModelsHub(nav: ModelsNavBridge) {
  const { t } = useTranslation();
  const data = useModelsData(nav);
  const { createProvider, deleteProvider } = useProvidersFlat();

  const editProvider = useMemo(() => {
    const overlay = data.overlay;
    if (overlay.type !== "edit") return null;
    const provider = data.providers.find((p) => p.id === overlay.providerId) ?? null;
    // Native sign-in is a connection mode, not an editable API provider.
    if (provider && isNativeOfficialProvider(provider)) return null;
    return provider;
  }, [data.overlay, data.providers]);

  const deleteTarget = useMemo(() => {
    const overlay = data.overlay;
    if (overlay.type !== "delete") return null;
    return data.providers.find((p) => p.id === overlay.providerId) ?? null;
  }, [data.overlay, data.providers]);

  const editInitialTab = data.overlay.type === "edit" ? data.overlay.tab : undefined;
  const handleDuplicate = useCallback(
    async (p: ProviderEntryFlat) => {
      try {
        await createProvider({ ...p, id: "", name: t("models.hub.duplicateSuffix", { name: p.name }) });
        toast.success(t("models.toasts.duplicated"));
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [createProvider, t],
  );

  const confirmDelete = useCallback(
    async (p: ProviderEntryFlat) => {
      try {
        await deleteProvider(p.id);
        data.closeOverlay();
        toast.success(t("models.toasts.deleted", { name: p.name }));
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err));
      }
    },
    [deleteProvider, data, t],
  );

  if (data.isLoading) {
    return (
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />
        <main className="ss-page-scroll">
          <div className="flex min-h-[60vh] items-center justify-center">
            <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" />
          </div>
        </main>
      </div>
    );
  }

  return (
    <>
      {data.error ? (
        <main className="ss-page-scroll p-8 text-sm">
          <h1 className="mb-3 text-lg font-semibold">{t("models.claudeWorkbench.loadFailed")}</h1>
          <p role="alert" className="mb-4 break-words text-destructive">
            {String(data.error instanceof Error ? data.error.message : data.error)}
          </p>
          <Button onClick={() => void data.refresh()}>{t("models.claudeWorkbench.retryLoad")}</Button>
        </main>
      ) : (
        <>
          <div className={data.overlay.type === "create" ? "hidden" : "flex min-h-0 flex-1 flex-col"}>
            <ClaudeWorkbench data={data} />
          </div>
          {data.overlay.type === "create" && (
            <ClaudeProviderCreate
              onClose={data.closeOverlay}
              onCreated={(provider) => data.setOverlay({ type: "edit", providerId: provider.id })}
            />
          )}
        </>
      )}

      {editProvider ? (
        <ProviderEditorDrawer
          provider={editProvider}
          open
          initialTab={editInitialTab}
          onClose={data.closeOverlay}
          onDuplicate={(p) => void handleDuplicate(p)}
          onDelete={(p) => data.setOverlay({ type: "delete", providerId: p.id })}
        />
      ) : null}

      <DeleteProviderDialog
        provider={deleteTarget}
        affectedToolIds={deleteTarget ? getProviderToolBadges(deleteTarget.id, data.toolActivations) : []}
        onCancel={data.closeOverlay}
        onConfirm={(p) => void confirmDelete(p)}
      />
    </>
  );
}
