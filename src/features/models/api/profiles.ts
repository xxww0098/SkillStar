import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import { modelsKeys } from "./keys";

/** Saved profile names. A missing file is an empty list. Load failure stays a query error. */
export function useProfileNames() {
  return useQuery<string[]>({
    queryKey: modelsKeys.profileNames(),
    queryFn: () => tauriInvoke("get_profile_names"),
  });
}
