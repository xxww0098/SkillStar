import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { modelsKeys } from "../../api/keys";
import { useListenMode } from "../../api/listen";

const MODES = [
  { id: "loopback", label: "环回" },
  { id: "lan", label: "局域网" },
] as const;

/**
 * Loopback or LAN. The sentence under the switch is the address agents keep.
 */
export function LanListen() {
  const queryClient = useQueryClient();
  const { data } = useListenMode();
  const mode = data === "lan" || data === "loopback" ? data : "";
  const [error, setError] = useState("");

  return (
    <div role="group" aria-label="lan listen" className="shrink-0 space-y-1 px-4 py-3">
      <div className="flex flex-wrap gap-1">
        {MODES.map((item) => (
          <button
            key={item.id}
            type="button"
            aria-pressed={mode === item.id}
            onClick={() => {
              void tauriInvoke("save_listen_mode", { mode: item.id })
                .then(async () => {
                  setError("");
                  await queryClient.invalidateQueries({ queryKey: modelsKeys.listenMode() });
                })
                .catch((caught: unknown) => {
                  setError(caught instanceof Error ? caught.message : "");
                });
            }}
            className={cn(
              "rounded-lg px-2 py-1 text-xs text-foreground",
              mode === item.id ? "bg-primary/15 font-medium" : "hover:bg-muted/40",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>
      <p className="text-xs text-muted-foreground">写给 Agent 的地址仍是 127.0.0.1。</p>
      {error ? (
        <p role="alert" className="text-xs text-muted-foreground">
          {error}
        </p>
      ) : null}
    </div>
  );
}
