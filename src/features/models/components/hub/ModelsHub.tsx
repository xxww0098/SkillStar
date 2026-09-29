import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModelsBoardRowDto } from "@/types/generated/ModelsBoardRowDto";
import { cn } from "@/lib/utils";
import { useModelsBoard } from "../../api/board";
import type { ModelsNavBridge } from "../../lib/navBridge";

type ColumnId = "agents" | "providers" | "gateway";

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
                    }}
                    className={cn(
                      "w-full cursor-pointer truncate rounded-lg px-2 py-1.5 text-left text-sm text-foreground hover:bg-muted/40",
                      entry.id === "providers" && selectedProviderId === row.id && "bg-primary/15 font-medium",
                    )}
                  >
                    {row.name}
                  </button>
                </li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </div>
  );
}
