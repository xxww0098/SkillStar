/** S3 target CRUD mutations + queries. Mirrors the SSH `api/hosts.ts` shape:
 * TanStack Query for the list, mutations that optimistically invalidate. */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import type { S3ConnectionTestResult, S3Target } from "../../../lib/ipc/commands/s3";
import { s3Keys } from "./keys";

export function useS3TargetsQuery() {
  return useQuery<S3Target[]>({
    queryKey: s3Keys.targets(),
    queryFn: () => tauriInvoke("list_s3_targets"),
    staleTime: 10_000,
  });
}

export function useAddS3Target() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { def: S3Target; secretAccessKey?: string }) => tauriInvoke("add_s3_target", vars),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: s3Keys.targets() });
      toast.success(i18n.t("s3.toast.targetAdded"));
    },
    onError: (e: unknown) => toast.error(i18n.t("s3.toast.targetAddFailed"), { description: String(e) }),
  });
}

export function useUpdateS3Target() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: string; def: S3Target; secretAccessKey?: string }) =>
      tauriInvoke("update_s3_target", vars),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: s3Keys.targets() });
      toast.success(i18n.t("s3.toast.targetUpdated"));
    },
    onError: (e: unknown) => toast.error(i18n.t("s3.toast.targetUpdateFailed"), { description: String(e) }),
  });
}

export function useDeleteS3Target() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: string }) => tauriInvoke("delete_s3_target", vars),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: s3Keys.targets() });
      toast.success(i18n.t("s3.toast.targetDeleted"));
    },
    onError: (e: unknown) => toast.error(i18n.t("s3.toast.targetDeleteFailed"), { description: String(e) }),
  });
}

/** Probe a target with HeadBucket. `retry: false` because a bad endpoint can
 * stall and the user wants fast feedback. */
export function useTestS3Connection() {
  return useMutation({
    mutationFn: (vars: { def: S3Target }) => tauriInvoke("test_s3_connection", vars),
    retry: false,
  });
}

export type { S3ConnectionTestResult, S3Target };
