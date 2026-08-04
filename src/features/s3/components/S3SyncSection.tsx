import { Cloud, Pencil, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { AlertDialog } from "radix-ui";
import { Button } from "../../../components/ui/button";
import type { S3Target } from "../../../lib/ipc/commands/s3";
import { useAddS3Target, useDeleteS3Target, useS3TargetsQuery, useUpdateS3Target } from "../api/targets";
import { S3TargetForm, type S3TargetFormValues } from "./S3TargetForm";

/** Settings section: manage S3-compatible cloud sync targets (R2 / B2 / 七牛 /
 * OSS / COS / MinIO). Mirrors the SSH host CRUD UX: list + add/edit form modal.
 * Credentials (secret_access_key) live in the OS keyring, never shown. */
export function S3SyncSection() {
  const { t } = useTranslation();
  const targetsQuery = useS3TargetsQuery();
  const addTarget = useAddS3Target();
  const updateTarget = useUpdateS3Target();
  const deleteTarget = useDeleteS3Target();

  const [formOpen, setFormOpen] = useState(false);
  const [editing, setEditing] = useState<S3Target | null>(null);
  const [pendingDelete, setPendingDelete] = useState<S3Target | null>(null);

  const openAdd = () => {
    setEditing(null);
    setFormOpen(true);
  };
  const openEdit = (target: S3Target) => {
    setEditing(target);
    setFormOpen(true);
  };

  const handleSubmit = (values: S3TargetFormValues) => {
    const def: S3Target = {
      id: values.id ?? "",
      display_name: values.displayName.trim() || values.bucket.trim(),
      endpoint_url: values.endpointUrl.trim(),
      region: values.region.trim() || "us-east-1",
      bucket: values.bucket.trim(),
      prefix: values.prefix.trim(),
      access_key_id: values.accessKeyId.trim(),
      force_path_style: values.forcePathStyle,
    };
    const secret = values.secretAccessKey ? values.secretAccessKey.trim() : undefined;
    if (editing) {
      updateTarget.mutate({ id: editing.id, def, secretAccessKey: secret });
    } else {
      addTarget.mutate({ def, secretAccessKey: secret });
    }
    setFormOpen(false);
  };

  const confirmDelete = () => {
    if (!pendingDelete) return;
    deleteTarget.mutate({ id: pendingDelete.id });
    setPendingDelete(null);
  };

  const targets = targetsQuery.data ?? [];

  return (
    <section>
      <div className="flex items-center justify-between mb-3 px-1">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg bg-sky-500/10 flex items-center justify-center shrink-0 border border-sky-500/20">
            <Cloud className="w-4 h-4 text-sky-400" />
          </div>
          <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.s3Sync")}</h2>
        </div>
        <Button size="sm" variant="outline" onClick={openAdd}>
          <Plus className="w-4 h-4 mr-1.5" />
          {t("s3.addTarget")}
        </Button>
      </div>

      <div className="rounded-xl border border-border bg-card overflow-hidden">
        {targets.length === 0 ? (
          <div className="px-4 py-8 text-center">
            <p className="text-sm text-muted-foreground">{t("s3.empty")}</p>
            <p className="text-xs text-muted-foreground/70 mt-1">{t("s3.emptyHint")}</p>
          </div>
        ) : (
          <ul className="divide-y divide-border">
            {targets.map((target) => (
              <li key={target.id} className="flex items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <div className="text-sm font-medium text-foreground truncate">{target.display_name}</div>
                  <div className="text-xs text-muted-foreground truncate">
                    {target.bucket}
                    {target.endpoint_url ? ` · ${target.endpoint_url}` : ` · ${target.region}`}
                  </div>
                </div>
                <button
                  type="button"
                  onClick={() => openEdit(target)}
                  className="shrink-0 p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/40 transition-colors"
                  aria-label={t("common.edit")}
                >
                  <Pencil className="w-4 h-4" />
                </button>
                <button
                  type="button"
                  onClick={() => setPendingDelete(target)}
                  className="shrink-0 p-1.5 rounded-md text-muted-foreground hover:text-red-400 hover:bg-red-500/10 transition-colors"
                  aria-label={t("common.delete")}
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <p className="text-xs text-muted-foreground/70 leading-relaxed mt-2 px-1">{t("s3.securityNotice")}</p>

      <S3TargetForm open={formOpen} initial={editing} onOpenChange={setFormOpen} onSubmit={handleSubmit} />

      <AlertDialog.Root open={!!pendingDelete} onOpenChange={(open) => !open && setPendingDelete(null)}>
        <AlertDialog.Portal>
          <AlertDialog.Overlay className="fixed inset-0 z-[96] bg-black/40 backdrop-blur-sm" />
          <AlertDialog.Content className="fixed left-1/2 top-1/2 z-[97] w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border/60 bg-card p-6 shadow-xl">
            <div className="mb-4 flex items-start gap-3">
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-destructive/10 text-destructive">
                <Trash2 className="h-5 w-5" />
              </div>
              <div className="space-y-1">
                <AlertDialog.Title className="text-sm font-semibold text-foreground">
                  {t("common.delete")}
                </AlertDialog.Title>
                <AlertDialog.Description className="text-xs leading-relaxed text-muted-foreground">
                  {t("s3.confirmDelete", { name: pendingDelete?.display_name ?? "" })}
                </AlertDialog.Description>
              </div>
            </div>
            <div className="flex items-center justify-end gap-2 border-t border-border/40 pt-3">
              <AlertDialog.Cancel asChild>
                <Button variant="ghost" size="sm">
                  {t("common.cancel")}
                </Button>
              </AlertDialog.Cancel>
              <AlertDialog.Action asChild>
                <Button variant="destructive" size="sm" onClick={confirmDelete}>
                  {t("common.delete")}
                </Button>
              </AlertDialog.Action>
            </div>
          </AlertDialog.Content>
        </AlertDialog.Portal>
      </AlertDialog.Root>
    </section>
  );
}
