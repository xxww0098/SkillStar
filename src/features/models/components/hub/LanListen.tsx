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
 * Loopback or LAN as one segmented control. The sentence under the switch is
 * the address agents keep.
 */
export function LanListen() {
  const queryClient = useQueryClient();
  const { data } = useListenMode();
  const mode = data === "lan" || data === "loopback" ? data : "";
  const [error, setError] = useState("");

  return (
    <div role="group" aria-label="lan listen" className="space-y-2">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">监听方式</div>
      <div className="flex w-fit rounded-lg bg-muted/50 p-0.5">
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
              "rounded-md px-3 py-1 text-xs transition",
              mode === item.id
                ? "bg-background font-medium text-foreground shadow-sm ring-1 ring-border/60"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {item.label}
          </button>
        ))}
      </div>
      <p className="text-xs leading-relaxed text-muted-foreground">写给 Agent 的地址仍是 127.0.0.1。</p>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}
