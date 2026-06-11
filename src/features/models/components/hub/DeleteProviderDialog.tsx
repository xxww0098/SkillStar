import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AlertDialog } from "radix-ui";
import { Button } from "../../../../components/ui/button";
import type { ProviderEntryFlat } from "../../../../types";
import { getAgent } from "../../lib/agentRegistry";

export interface DeleteProviderDialogProps {
  provider: ProviderEntryFlat | null;
  /** Tool ids currently bound to this provider (will be disconnected). */
  affectedToolIds: string[];
  onCancel: () => void;
  onConfirm: (provider: ProviderEntryFlat) => void;
}

/** Confirmation before deleting a provider — lists the agents that would disconnect. */
export function DeleteProviderDialog({ provider, affectedToolIds, onCancel, onConfirm }: DeleteProviderDialogProps) {
  const { t } = useTranslation();
  const affectedNames = affectedToolIds.map((id) => getAgent(id)?.displayName ?? id);
  return (
    <AlertDialog.Root open={!!provider} onOpenChange={(open) => !open && onCancel()}>
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="fixed inset-0 z-[96] bg-black/40 backdrop-blur-sm" />
        <AlertDialog.Content className="fixed left-1/2 top-1/2 z-[97] w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border/60 bg-card p-6 shadow-xl">
          <div className="mb-4 flex items-start gap-3">
            <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-destructive/10 text-destructive">
              <Trash2 className="h-5 w-5" />
            </div>
            <div className="space-y-1">
              <AlertDialog.Title className="text-sm font-semibold text-foreground">
                {t("models.deleteDialog.title", { name: provider?.name ?? "" })}
              </AlertDialog.Title>
              <AlertDialog.Description className="text-xs leading-relaxed text-muted-foreground">
                {affectedNames.length > 0
                  ? t("models.deleteDialog.withAgents", { agents: affectedNames.join("、") })
                  : t("models.deleteDialog.noAgents")}
              </AlertDialog.Description>
            </div>
          </div>
          <div className="flex items-center justify-end gap-2 border-t border-border/40 pt-3">
            <AlertDialog.Cancel asChild>
              <Button variant="ghost" size="sm">
                {t("models.deleteDialog.cancel")}
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action asChild>
              <Button
                variant="destructive"
                size="sm"
                onClick={() => {
                  if (provider) onConfirm(provider);
                }}
              >
                {t("models.deleteDialog.confirm")}
              </Button>
            </AlertDialog.Action>
          </div>
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
