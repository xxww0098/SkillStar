import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import type { SavedGroup } from "../../../lib/ipc/commands/models";
import { modelsKeys } from "./keys";

/** Saved groups only. A missing file is an empty list. */
export function useSavedGroups() {
  return useQuery<SavedGroup[]>({
    queryKey: modelsKeys.savedGroups(),
    queryFn: () => tauriInvoke("get_saved_groups"),
  });
}
