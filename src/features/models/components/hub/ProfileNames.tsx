import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";
import { useProfileNames } from "../../api/profiles";

/**
 * Saved profile names. Applying one calls the existing writer. The name list
 * is the only thing in the labeled list; the save form and the skip line sit
 * outside it.
 */
export function ProfileNames() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data } = useProfileNames();
  const names = data ?? [];
  const [name, setName] = useState("");
  const [agentId, setAgentId] = useState("");
  const [modelRef, setModelRef] = useState("");
  const [error, setError] = useState("");
  const [skipped, setSkipped] = useState<string[]>([]);

  async function refresh() {
    await queryClient.invalidateQueries({ queryKey: modelsKeys.profileNames() });
    await queryClient.invalidateQueries({ queryKey: modelsKeys.board() });
  }

  return (
    <div className="space-y-2">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.profilesTitle")}
      </div>
      <ul aria-label="profiles" className="space-y-0.5">
        {names.map((item) => {
          const shown = plainText(item);
          if (!shown) return null;
          return (
            <li key={item}>
              <button
                type="button"
                onClick={() => {
                  void tauriInvoke("apply_profile", { name: item })
                    .then(async (result) => {
                      setError("");
                      setSkipped(result.skipped);
                      await refresh();
                    })
                    .catch((caught: unknown) => {
                      setError(caught instanceof Error ? caught.message : "");
                    });
                }}
                className="w-full truncate rounded-lg px-2.5 py-1.5 text-left text-xs text-foreground transition hover:bg-muted/40"
              >
                {shown}
              </button>
            </li>
          );
        })}
      </ul>
      {skipped.some((item) => plainText(item)) ? (
        <p className="text-xs text-muted-foreground">{skipped.map(plainText).filter(Boolean).join(" ")}</p>
      ) : null}
      <form
        aria-label="save profile"
        className="space-y-1.5 rounded-xl border border-border/60 bg-muted/20 p-2.5"
        onSubmit={(event) => {
          event.preventDefault();
          void tauriInvoke("save_profile", {
            name,
            agents: [{ id: agentId, modelRef }],
          })
            .then(async () => {
              setError("");
              setName("");
              setAgentId("");
              setModelRef("");
              await refresh();
            })
            .catch((caught: unknown) => {
              setError(caught instanceof Error ? caught.message : "");
            });
        }}
      >
        <input
          aria-label="profile name"
          placeholder={t("models.gateway.profileNamePlaceholder")}
          value={name}
          onChange={(event) => setName(event.target.value)}
          className="w-full rounded-lg border border-border/60 bg-background px-2 py-1 text-xs"
        />
        <div className="flex gap-1.5">
          <input
            aria-label="profile agent"
            placeholder="agent id"
            value={agentId}
            onChange={(event) => setAgentId(event.target.value)}
            className="min-w-0 flex-1 rounded-lg border border-border/60 bg-background px-2 py-1 font-mono text-xs"
          />
          <input
            aria-label="profile model"
            placeholder="provider/model"
            value={modelRef}
            onChange={(event) => setModelRef(event.target.value)}
            className="min-w-0 flex-[1.4] rounded-lg border border-border/60 bg-background px-2 py-1 font-mono text-xs"
          />
          <button
            type="submit"
            className="shrink-0 rounded-lg border border-border/60 bg-background px-2.5 py-1 text-xs text-foreground transition hover:bg-muted/50"
          >
            Save profile
          </button>
        </div>
        {error ? (
          <p role="alert" className="text-xs text-destructive">
            {error}
          </p>
        ) : null}
      </form>
    </div>
  );
}

function plainText(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
