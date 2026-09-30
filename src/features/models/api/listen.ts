import { useQuery } from "@tanstack/react-query";
import { tauriInvoke } from "../../../lib/ipc";
import { modelsKeys } from "./keys";

/** `loopback` or `lan`. A missing file is loopback. Load failure stays a query error. */
export function useListenMode() {
  return useQuery<string>({
    queryKey: modelsKeys.listenMode(),
    queryFn: () => tauriInvoke("get_listen_mode"),
  });
}
