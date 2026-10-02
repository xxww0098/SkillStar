import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import { modelsKeys } from "../../api/keys";
import { useListenMode } from "../../api/listen";

/**
 * Loopback or LAN as one segmented control. The sentence under the switch is
 * the address agents keep.
 */
export function LanListen() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data } = useListenMode();
  const mode = data === "lan" || data === "loopback" ? data : "";
  const [error, setError] = useState("");

  const modes = [
    { id: "loopback", label: t("models.gateway.listenLoopback") },
    { id: "lan", label: t("models.gateway.listenLan") },
  ] as const;

  return (
    <div role="group" aria-label={t("models.gateway.listenTitle")} className="space-y-2">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.listenTitle")}
      </div>
      <div className="flex w-fit rounded-lg bg-muted/50 p-0.5">
        {modes.map((item) => (
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
      <p className="text-xs leading-relaxed text-muted-foreground">{t("models.gateway.listenNote")}</p>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}
