import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";
import { useListenMode } from "../../api/listen";
import { SegmentedControl } from "./SegmentedControl";

/**
 * Loopback or LAN as one compact segmented control that lives in the
 * Gateway card header. The trailing sentence is the address agents keep.
 */
export function LanListen() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data } = useListenMode();
  const mode = data === "lan" || data === "loopback" ? data : "";
  const [error, setError] = useState("");

  return (
    <div role="group" aria-label={t("models.gateway.listenTitle")} className="flex flex-wrap items-center gap-2.5">
      <span className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.listenTitle")}
      </span>
      <SegmentedControl
        options={[
          { id: "loopback", label: t("models.gateway.listenLoopback") },
          { id: "lan", label: t("models.gateway.listenLan") },
        ]}
        value={mode}
        onSelect={(next) => {
          void tauriInvoke("save_listen_mode", { mode: next })
            .then(async () => {
              setError("");
              await queryClient.invalidateQueries({ queryKey: modelsKeys.listenMode() });
            })
            .catch((caught: unknown) => {
              setError(caught instanceof Error ? caught.message : "");
            });
        }}
      />
      <span className="text-[11px] leading-4 text-muted-foreground/80">{t("models.gateway.listenNote")}</span>
      {error ? (
        <span role="alert" className="text-[11px] text-destructive">
          {error}
        </span>
      ) : null}
    </div>
  );
}
