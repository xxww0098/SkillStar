import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useState } from "react";
import i18n from "../../../i18n";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import type {
  DecisionDownloadProgress,
  DecisionEngineInfo,
  DecisionModelStatus,
  DecisionOutcome,
} from "../../../types";

/** Event channel the backend emits download ticks on. */
export const DECISION_DOWNLOAD_EVENT = "decision://download-progress";

export const decisionKeys = {
  all: ["decision"] as const,
  status: () => [...decisionKeys.all, "status"] as const,
  engine: () => [...decisionKeys.all, "engine"] as const,
};

/** Checkpoint state on disk (cheap: sizes, not digests). */
export function useDecisionModelStatus() {
  return useQuery({
    queryKey: decisionKeys.status(),
    queryFn: () => tauriInvoke("decision_model_status"),
  });
}

/** Which device / dtype the loaded engine runs as, or `null` when unloaded. */
export function useDecisionEngineInfo() {
  return useQuery({
    queryKey: decisionKeys.engine(),
    queryFn: () => tauriInvoke("decision_engine_info"),
  });
}

export interface DecisionModelActions {
  // `Promise<unknown>` rather than `Promise<void>`: the mutations hand back
  // whatever the command returns (the load command returns engine info), and
  // the panel only awaits them.
  download: () => Promise<unknown>;
  cancelDownload: () => Promise<unknown>;
  verify: () => Promise<unknown>;
  load: () => Promise<unknown>;
  unload: () => Promise<unknown>;
  isDownloading: boolean;
  isVerifying: boolean;
  isLoading: boolean;
}

/** Download, verify, load and unload, with query invalidation and toasts. */
export function useDecisionModelActions(): DecisionModelActions {
  const queryClient = useQueryClient();
  const invalidate = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: decisionKeys.all });
  }, [queryClient]);

  const download = useMutation({
    mutationFn: () => tauriInvoke("decision_download_model"),
    onSuccess: () => {
      toast.success(i18n.t("settings.decision.downloadDone"));
      invalidate();
    },
    onError: (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      // A cancelled transfer is a user decision, not a failure.
      if (message.includes("cancelled")) {
        toast.info(i18n.t("settings.decision.downloadCancelled"));
        return;
      }
      toast.error(i18n.t("settings.decision.downloadFailed", { message }));
    },
    onSettled: invalidate,
  });

  const cancelDownload = useMutation({
    mutationFn: () => tauriInvoke("decision_cancel_download"),
  });

  const verify = useMutation({
    mutationFn: () => tauriInvoke("decision_verify_model"),
    onSuccess: () => toast.success(i18n.t("settings.decision.verifyDone")),
    onError: (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(i18n.t("settings.decision.verifyFailed", { message }));
    },
  });

  const load = useMutation({
    mutationFn: () => tauriInvoke("decision_load_engine"),
    onSuccess: () => {
      toast.success(i18n.t("settings.decision.engineLoaded"));
      invalidate();
    },
    onError: (error: unknown) => {
      const message = error instanceof Error ? error.message : String(error);
      toast.error(i18n.t("settings.decision.engineLoadFailed", { message }));
    },
    onSettled: invalidate,
  });

  const unload = useMutation({
    mutationFn: () => tauriInvoke("decision_unload_engine"),
    onSuccess: () => {
      toast.success(i18n.t("settings.decision.engineUnloaded"));
      invalidate();
    },
  });

  return {
    download: download.mutateAsync,
    cancelDownload: cancelDownload.mutateAsync,
    verify: verify.mutateAsync,
    load: load.mutateAsync,
    unload: unload.mutateAsync,
    isDownloading: download.isPending,
    isVerifying: verify.isPending,
    isLoading: load.isPending,
  };
}

/** Latest download tick, or `null` when nothing has been received. */
export function useDecisionDownloadProgress(): DecisionDownloadProgress | null {
  const [progress, setProgress] = useState<DecisionDownloadProgress | null>(null);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<DecisionDownloadProgress>(DECISION_DOWNLOAD_EVENT, (event) => {
      if (!disposed) setProgress(event.payload);
    });
    return () => {
      disposed = true;
      void unlisten.then((off) => off());
    };
  }, []);

  return progress;
}

/** Answer one payload. Kept as a mutation: a run is a user action, not state. */
export function useDecisionEvaluate() {
  return useMutation<DecisionOutcome, unknown, unknown>({
    mutationFn: (payload) => tauriInvoke("decision_evaluate", { payload }),
  });
}

/** Status summary used by the panel header badge. */
export function decisionStateLabel(status: DecisionModelStatus | undefined): string {
  if (!status) return i18n.t("settings.decision.stateUnknown");
  switch (status.state) {
    case "ready":
      return i18n.t("settings.decision.stateReady");
    case "partial":
      return i18n.t("settings.decision.statePartial");
    default:
      return i18n.t("settings.decision.stateMissing");
  }
}

/** Engine line for the panel header, e.g. `metal · f32`. */
export function engineLabel(engine: DecisionEngineInfo | null | undefined): string | null {
  if (!engine) return null;
  return `${engine.device} · ${engine.dtype}`;
}
