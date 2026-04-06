/**
 * Unified hook for instant behavior-field reads/writes across all three apps.
 * Each control in BehaviorStrip calls `set(key, value)` which:
 *   1. Optimistically updates local state
 *   2. Invokes the corresponding backend command
 *   3. On failure, rolls back and shows a toast
 */

import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import type { ModelAppId } from "../components/AppCapsuleSwitcher";

// Dot-path helper: get nested value from object
function getByPath(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current == null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

// Dot-path helper: set nested value in object (immutable)
function setByPath(obj: Record<string, unknown>, path: string, value: unknown): Record<string, unknown> {
  const parts = path.split(".");
  if (parts.length === 1) {
    return { ...obj, [parts[0]]: value };
  }
  const [head, ...rest] = parts;
  const child = (obj[head] as Record<string, unknown>) || {};
  return { ...obj, [head]: setByPath(child, rest.join("."), value) };
}

export function useAppSettings(appId: ModelAppId) {
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(true);

  const load = useCallback(
    async (isSilent = false) => {
      if (!isSilent) setLoading(true);
      try {
        if (appId === "claude") {
          const config = (await invoke("get_claude_model_config")) as Record<string, unknown>;
          if (config && typeof config === "object") {
            setValues(config);
          }
        } else if (appId === "codex") {
          // Codex now returns pure TOML string from the single config.toml file
          const configText = (await invoke("get_codex_model_config")) as string;
          const parsed: Record<string, unknown> = {};

          if (typeof configText === "string" && configText.trim()) {
            let currentSection = "";
            for (const rawLine of configText.split("\n")) {
              const line = rawLine.trim();
              if (!line || line.startsWith("#")) continue;

              // Section match headers like [model] or [features]
              const secMatch = line.match(/^\[([\w.]+)\]$/);
              if (secMatch) {
                currentSection = secMatch[1];
                if (!parsed[currentSection]) {
                  parsed[currentSection] = {};
                }
                continue;
              }

              // Key-value matcher
              const kvMatch = line.match(/^([\w_-]+)\s*=\s*(.*)$/);
              if (kvMatch) {
                const key = kvMatch[1];
                let rawVal = kvMatch[2].trim();

                // Strip inline comments if they exist
                if (rawVal.includes(" #")) {
                  rawVal = rawVal.split(" #")[0].trim();
                }

                let finalVal: unknown = rawVal;
                // Strip quotes for strings
                if (
                  (rawVal.startsWith('"') && rawVal.endsWith('"')) ||
                  (rawVal.startsWith("'") && rawVal.endsWith("'"))
                ) {
                  finalVal = rawVal.slice(1, -1);
                } else if (rawVal === "true") {
                  finalVal = true;
                } else if (rawVal === "false") {
                  finalVal = false;
                } else if (!Number.isNaN(Number(rawVal))) {
                  finalVal = Number(rawVal);
                } else if (rawVal.startsWith("[") || rawVal.startsWith("{")) {
                  // Keep raw TOML arrays or tables as strings in local state so inputs can edit them
                  finalVal = rawVal;
                }

                if (currentSection) {
                  if (typeof parsed[currentSection] === "object") {
                    (parsed[currentSection] as Record<string, unknown>)[key] = finalVal;
                  }
                } else {
                  parsed[key] = finalVal;
                }
              }
            }
          }
          setValues(parsed);
        } else if (appId === "opencode") {
          const config = (await invoke("get_opencode_model_config")) as Record<string, unknown>;
          if (config && typeof config === "object") {
            setValues(config);
          }
        }
      } catch {
        /* silent — empty state is fine */
      } finally {
        if (!isSilent) setLoading(false);
      }
    },
    [appId],
  );

  // Write a single field. For Codex, value is passed as a TOML-encoded string.
  const set = useCallback(
    async (key: string, value: unknown) => {
      // Optimistic update
      setValues((prev) => setByPath(prev, key, value));
      try {
        if (appId === "claude") {
          await invoke("set_claude_setting", { key, value });
        } else if (appId === "codex") {
          // Codex backend expects an optional TOML-encoded string value
          let tomlValue: string | null = null;
          if (value !== undefined && value !== null) {
            if (typeof value === "boolean" || typeof value === "number") {
              tomlValue = String(value);
            } else if (typeof value === "string") {
              const trimmed = value.trim();
              if (trimmed.startsWith("[") || trimmed.startsWith("{")) {
                tomlValue = trimmed; // Inject raw array or dictionary struct directly
              } else {
                tomlValue = `"${value}"`;
              }
            } else {
              tomlValue = `"${value}"`;
            }
          }
          await invoke("set_codex_setting", { key, value: tomlValue });
        } else if (appId === "opencode") {
          await invoke("set_opencode_setting", { key, value });
        }
        window.dispatchEvent(new CustomEvent("skillstar_config_changed"));
      } catch (e) {
        toast.error(`设置失败: ${e}`);
        load(true); // rollback silently
      }
    },
    [appId, load],
  );

  const get = useCallback(
    (key: string): unknown => {
      return getByPath(values, key);
    },
    [values],
  );

  useEffect(() => {
    load(false);

    const onExternalChange = () => {
      load(true);
    };
    window.addEventListener("skillstar_config_changed", onExternalChange);
    return () => window.removeEventListener("skillstar_config_changed", onExternalChange);
  }, [load]);

  return { values, loading, set, get, reload: load };
}
