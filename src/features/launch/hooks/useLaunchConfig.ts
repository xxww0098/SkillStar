import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

// ── Types ────────────────────────────────────────────────────────────

export type LaunchMode = "single" | "multi";
export type SplitDirection = "h" | "v";

export interface PaneNode {
  type: "pane";
  id: string;
  agentId: string;
  providerId?: string;
  providerName?: string;
  modelId?: string;
  safeMode: boolean;
  extraArgs: string[];
}

export interface SplitNode {
  type: "split";
  direction: SplitDirection;
  ratio: number;
  children: [LayoutNode, LayoutNode];
}

export type LayoutNode = PaneNode | SplitNode;

export interface LaunchConfig {
  projectName: string;
  mode: LaunchMode;
  singleLayout: LayoutNode;
  multiLayout: LayoutNode;
  updatedAt: number;
}

// ── Default Config ──────────────────────────────────────────────────

function defaultConfig(projectName: string): LaunchConfig {
  const defaultPane: PaneNode = {
    type: "pane",
    id: "pane-1",
    agentId: "",
    safeMode: false,
    extraArgs: [],
  };
  return {
    projectName,
    mode: "single",
    singleLayout: defaultPane,
    multiLayout: { ...defaultPane, id: "pane-2" },
    updatedAt: Date.now(),
  };
}

// ── Hook ────────────────────────────────────────────────────────────

export function useLaunchConfig(projectName: string) {
  const [config, setConfig] = useState<LaunchConfig | null>(null);
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const skipAutoSave = useRef(false);

  // Load config on mount / project change
  useEffect(() => {
    if (!projectName) {
      setConfig(null);
      setLoading(false);
      return;
    }
    setLoading(true);
    invoke<LaunchConfig | null>("get_launch_config", { projectName })
      .then((c) => {
        skipAutoSave.current = true;
        setConfig(c ?? defaultConfig(projectName));
      })
      .catch(() => {
        skipAutoSave.current = true;
        setConfig(defaultConfig(projectName));
      })
      .finally(() => setLoading(false));
  }, [projectName]);

  // Debounced auto-save (800ms after config change)
  useEffect(() => {
    if (!config || loading) return;

    // Skip the first render after loading
    if (skipAutoSave.current) {
      skipAutoSave.current = false;
      return;
    }

    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);

    saveTimerRef.current = setTimeout(async () => {
      setSaving(true);
      try {
        await invoke("save_launch_config", {
          config: { ...config, updatedAt: Date.now() },
        });
      } catch (e) {
        console.error("Failed to save launch config:", e);
      }
      setSaving(false);
    }, 800);

    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, [config, loading]);

  const updateConfig = useCallback((updater: (prev: LaunchConfig) => LaunchConfig) => {
    setConfig((prev) => {
      if (!prev) return prev;
      return updater(prev);
    });
  }, []);

  return { config, setConfig: updateConfig, saving, loading };
}
