import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";
import { useProfileNames } from "../../api/profiles";

/**
 * Saved profile names. Applying one calls the existing writer. The name list
 * is the only thing in the labeled list; the save form and the skip line sit
 * outside it.
 */
export function ProfileNames() {
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
    <div className="shrink-0 space-y-3 px-4 py-3">
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
                className="w-full truncate rounded-lg px-2 py-1 text-left text-xs text-foreground hover:bg-muted/40"
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
        className="flex flex-wrap gap-1"
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
          value={name}
          onChange={(event) => setName(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <input
          aria-label="profile agent"
          value={agentId}
          onChange={(event) => setAgentId(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <input
          aria-label="profile model"
          value={modelRef}
          onChange={(event) => setModelRef(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <button type="submit" className="rounded-lg px-2 py-1 text-xs text-foreground hover:bg-muted/40">
          Save profile
        </button>
        {error ? (
          <p role="alert" className="basis-full text-xs text-muted-foreground">
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
