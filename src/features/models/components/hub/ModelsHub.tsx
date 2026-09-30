import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModelsBoardRowDto } from "@/types/generated/ModelsBoardRowDto";
import type { RecentCallDto } from "@/types/generated/RecentCallDto";
import { cn } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { useModelsBoard } from "../../api/board";
import { useRecentCalls } from "../../api/recent";
import { useRoutingPage } from "../../api/routing";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { GroupMembers } from "./GroupMembers";
import { LanListen } from "./LanListen";
import { ProfileNames } from "./ProfileNames";
import { RoutingControl } from "./RoutingControl";

type ColumnId = "agents" | "providers" | "gateway";

/** Agent name, then the loopback host:port when one was written. */
function AgentRowLabel({ name, loopbackLabel }: { name: string; loopbackLabel: string }) {
  const label = loopbackText(loopbackLabel);
  return (
    <span className="flex min-w-0 flex-col">
      <span className="truncate">{name}</span>
      {label ? <span className="truncate text-xs font-normal text-muted-foreground">{label}</span> : null}
    </span>
  );
}

/** Display name, or the upstream id when the name is missing or secret-shaped. */
function choiceText(choice: { id: string; label?: string }): string {
  const label = choice.label?.trim() ?? "";
  if (!label || label.includes("\n") || label.includes("\r") || label.includes("://") || label.includes("sk-")) {
    return choice.id;
  }
  return label;
}

/** Only `127.0.0.1:<port>` is drawn. Anything else, including a vendor URL, is blank. */
function loopbackText(value: string): string {
  const prefix = "127.0.0.1:";
  if (!value.startsWith(prefix)) return "";
  const port = value.slice(prefix.length);
  if (!/^\d{1,5}$/.test(port)) return "";
  return value;
}

/** Gateway column. Headers name the call. A vendor URL or a non-digit token is blank. */
function RecentCalls({ calls }: { calls: RecentCallDto[] }) {
  return (
    <div className="min-h-0 flex-1 overflow-auto px-2 py-3">
      <table className="w-full table-fixed text-left text-xs text-foreground">
        <thead>
          <tr className="text-muted-foreground">
            <th className="px-1 py-1 font-normal" scope="col">
              Time
            </th>
            <th className="px-1 py-1 font-normal" scope="col">
              Agent
            </th>
            <th className="px-1 py-1 font-normal" scope="col">
              Model
            </th>
            <th className="px-1 py-1 font-normal" scope="col">
              Status
            </th>
            <th className="px-1 py-1 font-normal" scope="col">
              Tokens
            </th>
          </tr>
        </thead>
        <tbody>
          {calls.map((call, index) => (
            <tr key={`${call.at}-${call.agent}-${index}`}>
              <td className="truncate px-1 py-1">{plainCell(call.at)}</td>
              <td className="truncate px-1 py-1">{plainCell(call.agent)}</td>
              <td className="truncate px-1 py-1">{plainCell(call.model)}</td>
              <td className="px-1 py-1">{statusText(call.status)}</td>
              <td className="px-1 py-1">{tokenCell(call.completion_tokens)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function plainCell(value: string): string {
  if (value.includes("://") || value.includes("api.openai.com")) return "";
  return value;
}

function statusText(status: number): string {
  return Number.isInteger(status) ? String(status) : "";
}

function tokenCell(value: string): string {
  return /^\d+$/.test(value) ? value : "";
}

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

/** Shown only after that column's read succeeds with nothing in it. */
const EMPTY_COLUMN: Record<ColumnId, string> = {
  agents: "还没有探测到可配置的 Agent",
  providers: "还没有密钥",
  gateway: "还没有调用",
};

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
  const boardQuery = useModelsBoard();
  const recentQuery = useRecentCalls();
  const { data } = boardQuery;
  const { data: routingPage } = useRoutingPage(selectedProviderId);
  const calls = recentQuery.data ?? [];
  const groups = routingPage?.groups ?? [];
  const [column, setColumn] = useState<ColumnId | null>(null);
  const [picker, setPicker] = useState<{ id: string; name: string } | null>(null);
  const [choices, setChoices] = useState<{ id: string; label?: string }[]>([]);
  const [nameId, setNameId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [nameError, setNameError] = useState("");
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
            {entry.id === "gateway" ? (
              <>
                <ProfileNames />
                <LanListen />
                {routingPage?.provider && selectedProviderId ? (
                  <RoutingControl
                    owner="provider"
                    id={selectedProviderId}
                    routing={routingPage.provider.routing}
                    affinity={routingPage.provider.affinity}
                  />
                ) : null}
                {groups.map((group) => (
                  <RoutingControl
                    key={group.id}
                    owner="group"
                    id={group.id}
                    routing={group.routing}
                    affinity={group.affinity}
                  />
                ))}
                <GroupMembers />
                {recentQuery.isSuccess && calls.length === 0 ? (
                  <p className="px-4 py-3 text-sm text-muted-foreground">{EMPTY_COLUMN.gateway}</p>
                ) : calls.length > 0 ? (
                  <RecentCalls calls={calls} />
                ) : null}
              </>
            ) : boardQuery.isSuccess && rows[entry.id].length === 0 ? (
              <p className="px-4 py-3 text-sm text-muted-foreground">{EMPTY_COLUMN[entry.id]}</p>
            ) : (
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
                        entry.id === "providers" && "flex",
                        entry.id === "providers" && selectedProviderId === row.id && "bg-primary/15 font-medium",
                      )}
                    >
                      {entry.id === "providers" ? (
                        <ProviderRowLabel name={row.name} credentialSummary={row.credential_summary ?? ""} />
                      ) : entry.id === "agents" ? (
                        <AgentRowLabel name={row.name} loopbackLabel={row.loopback_label ?? ""} />
                      ) : (
                        row.name
                      )}
                    </button>
                  </li>
                ))}
              </ul>
            )}
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
            <ul aria-label="model choices" className="space-y-0.5">
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
                    {choiceText(choice)}
                  </button>
                </li>
              ))}
            </ul>
            <form
              aria-label="model display name"
              className="mt-3 flex flex-wrap gap-1"
              onSubmit={(event) => {
                event.preventDefault();
                void tauriInvoke("save_model_name", { id: nameId, name: displayName })
                  .then(async () => {
                    setNameError("");
                    setDisplayName("");
                    setChoices(await tauriInvoke("get_model_choices"));
                  })
                  .catch((caught: unknown) => {
                    setNameError(caught instanceof Error ? caught.message : "");
                  });
              }}
            >
              <input
                aria-label="model id"
                value={nameId}
                onChange={(event) => setNameId(event.target.value)}
                className="min-w-0 flex-1 rounded-lg border bg-transparent px-2 py-1 text-xs"
              />
              <input
                aria-label="display name"
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                className="min-w-0 flex-1 rounded-lg border bg-transparent px-2 py-1 text-xs"
              />
              <button type="submit" className="rounded-lg px-2 py-1 text-xs text-foreground hover:bg-muted/40">
                保存显示名
              </button>
            </form>
            {nameError ? (
              <p role="alert" className="mt-1 text-xs text-muted-foreground">
                {nameError}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </div>
  );
}
