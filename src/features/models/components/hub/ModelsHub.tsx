import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModelsBoardRowDto } from "@/types/generated/ModelsBoardRowDto";
import { cn } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { useModelsBoard } from "../../api/board";
import type { ModelsNavBridge } from "../../lib/navBridge";

type ColumnId = "agents" | "providers" | "gateway";

/** Providers column label. It receives the masked summary, not a key or a URL. */
function ProviderRowLabel({ name, credentialSummary }: { name: string; credentialSummary: string }) {
  return (
    <span className="flex w-full min-w-0 items-baseline gap-2">
      <span className="min-w-0 flex-1 truncate">{name}</span>
      {credentialSummary ? (
        <span className="shrink-0 text-xs font-normal text-muted-foreground">{credentialSummary}</span>
      ) : null}
    </span>
  );
}

const COLUMNS: { id: ColumnId; labelKey: string }[] = [
  { id: "agents", labelKey: "models.columns.agents" },
  { id: "providers", labelKey: "models.columns.providers" },
  { id: "gateway", labelKey: "models.columns.gateway" },
];

/**
 * Models page: Agents, Providers, Gateway. Column bodies may be empty.
 * A drawer request from the retired workbench is ignored.
 */
export function ModelsHub({
  selectedProviderId,
  setSelectedProviderId,
  modelsDrawerRequest,
  clearModelsDrawerRequest,
}: ModelsNavBridge) {
  const { t } = useTranslation();
  const { data } = useModelsBoard();
  const [column, setColumn] = useState<ColumnId | null>(null);
  const [picker, setPicker] = useState<{ id: string; name: string } | null>(null);
  const [choices, setChoices] = useState<{ id: string }[]>([]);
  void modelsDrawerRequest;
  void clearModelsDrawerRequest;

  const rows: Record<ColumnId, ModelsBoardRowDto[]> = {
    agents: data?.agents ?? [],
    providers: data?.providers ?? [],
    gateway: data?.gateway ?? [],
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />
      <div className="grid min-h-0 flex-1 grid-cols-3 gap-3 px-3 pb-3">
        {COLUMNS.map((entry) => (
          <section
            key={entry.id}
            aria-label={t(entry.labelKey)}
            onClick={() => setColumn(entry.id)}
            className={cn(
              "flex h-full min-h-48 min-w-0 cursor-pointer flex-col rounded-2xl border bg-card",
              column === entry.id ? "border-primary" : "border-border",
            )}
          >
            <h2 className="px-4 pt-4 text-sm font-semibold tracking-wide text-foreground">{t(entry.labelKey)}</h2>
            <ul className="min-h-0 flex-1 space-y-0.5 overflow-auto px-2 py-3">
              {rows[entry.id].map((row) => (
                <li key={row.id}>
                  <button
                    type="button"
                    onClick={() => {
                      setColumn(entry.id);
                      if (entry.id === "providers") setSelectedProviderId(row.id);
                      if (entry.id === "agents") {
                        const next = picker?.id === row.id ? null : { id: row.id, name: row.name };
                        setPicker(next);
                        setChoices([]);
                        if (next) {
                          void tauriInvoke("get_model_choices").then(setChoices);
                        }
                      }
                    }}
                    className={cn(
                      "w-full min-w-0 cursor-pointer rounded-lg px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted/40",
                      entry.id === "providers" ? "flex" : "truncate",
                      entry.id === "providers" && selectedProviderId === row.id && "bg-primary/15 font-medium",
                    )}
                  >
                    {entry.id === "providers" ? (
                      <ProviderRowLabel name={row.name} credentialSummary={row.credential_summary ?? ""} />
                    ) : (
                      row.name
                    )}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
      {picker ? (
        <div
          className="fixed inset-0 z-40 flex items-start justify-center bg-black/20 px-4 pt-24"
          onClick={() => setPicker(null)}
        >
          <div
            role="dialog"
            aria-label={picker.name}
            className="max-h-[70vh] w-full max-w-sm overflow-auto rounded-2xl border bg-card p-3 shadow-lg"
            onClick={(event) => event.stopPropagation()}
          >
            <ul className="space-y-0.5">
              {choices.map((choice) => (
                <li key={choice.id}>
                  <button
                    type="button"
                    className="w-full truncate rounded-lg px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted/40"
                    onClick={() => {
                      void tauriInvoke("save_agent_model", { agentId: picker.id, modelRef: choice.id }).then(
                        () => setPicker(null),
                        () => setPicker(null),
                      );
                    }}
                  >
                    {choice.id}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}
    </div>
  );
}
