/** Cloud sync operations: push all skills, pull the manifest, restore selected. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type { ManifestEntry, ManifestEntryView, S3InstallSummary, S3PushSummary } from "../../../lib/ipc/commands/s3";
import { s3Keys } from "./keys";

/** Download the cloud manifest and annotate each entry with local install
 * state. `retry: false` — an unreachable bucket stays unreachable. */
export function useCloudManifestQuery(targetId: string | null, enabled = true) {
  return useQuery<ManifestEntryView[]>({
    queryKey: s3Keys.manifest(targetId),
    queryFn: () => tauriInvoke("pull_cloud_manifest", { targetId: targetId! }),
    enabled: !!targetId && enabled,
    staleTime: 10_000,
    retry: false,
  });
}

export function usePushToCloud() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { targetId: string }) => tauriInvoke("push_skills_to_cloud", vars),
    onSuccess: (summary: S3PushSummary) => {
      qc.invalidateQueries({ queryKey: s3Keys.manifest(null) });
      toast.success(i18n.t("s3.toast.pushed"), {
        description: i18n.t("s3.toast.pushedDesc", {
          hub: summary.hubCount,
          local: summary.localCount,
        }),
      });
    },
    onError: (e: unknown) => toast.error(i18n.t("s3.toast.pushFailed"), { description: String(e) }),
  });
}

export function useInstallFromCloud() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { targetId: string; entries: ManifestEntry[] }) =>
      tauriInvoke("install_from_cloud_manifest", vars),
    onSuccess: (summary: S3InstallSummary) => {
      // The local hub changed — refresh everything skill-related.
      qc.invalidateQueries();
      const installed = summary.installed_names.length + summary.restored_names.length;
      toast.success(i18n.t("s3.toast.restored"), {
        description: i18n.t("s3.toast.restoredDesc", {
          installed,
          existing: summary.existing_names.length,
        }),
      });
    },
    onError: (e: unknown) => toast.error(i18n.t("s3.toast.restoreFailed"), { description: String(e) }),
  });
}

export type { ManifestEntry, ManifestEntryView, S3InstallSummary, S3PushSummary };
