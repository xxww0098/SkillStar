import { useCallback, useEffect, useState } from "react";
import { usageApi } from "../api";
import type { AppInstance, DesktopAppId } from "../types";

export function useAppInstances(appId: DesktopAppId | null) {
  const [instances, setInstances] = useState<AppInstance[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    if (!appId) {
      setInstances([]);
      return;
    }
    setLoading(true);
    try {
      setInstances(await usageApi.listAppInstances(appId));
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [appId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const run = useCallback(
    async (id: string | null, op: () => Promise<unknown>) => {
      setBusyId(id ?? "create");
      try {
        await op();
        await reload();
        setError(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
        throw err;
      } finally {
        setBusyId(null);
      }
    },
    [reload],
  );

  const create = useCallback(
    async (name: string) => {
      if (!appId) return;
      await run(null, () => usageApi.createAppInstance(appId, name));
    },
    [appId, run],
  );

  const start = useCallback(
    async (id: string) => {
      await run(id, () => usageApi.startAppInstance(id));
    },
    [run],
  );

  const stop = useCallback(
    async (id: string) => {
      await run(id, () => usageApi.stopAppInstance(id));
    },
    [run],
  );

  const remove = useCallback(
    async (id: string) => {
      await run(id, () => usageApi.deleteAppInstance(id));
    },
    [run],
  );

  return { instances, loading, busyId, error, reload, create, start, stop, remove };
}
