import { useState, type ReactNode } from "react";
import { Popover } from "radix-ui";
import { useTranslation } from "react-i18next";
import type { RecentCallDto } from "@/types/generated/RecentCallDto";
import { ProviderBrandIcon } from "@/components/shared/ProviderBrandIcon";
import { cn } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { useModelsBoard } from "../../api/board";
import { useRecentCalls } from "../../api/recent";
import { useRoutingPage } from "../../api/routing";
import { getAgent } from "../../lib/agentRegistry";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { GatewayEndpoints } from "./GatewayEndpoints";
import { GroupMembers } from "./GroupMembers";
import { LanListen } from "./LanListen";
import { ModelPickerPopover } from "./ModelPickerPopover";
import { ProfileNames } from "./ProfileNames";
import { RoutingControl } from "./RoutingControl";

/** Small caps label that groups one functional block inside the gateway panel. */
function SectionLabel({ children }: { children: ReactNode }) {
  return <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">{children}</div>;
}

/** Dashed note shown only after that column's read succeeds with nothing in it. */
function EmptyNote({ text }: { text: string }) {
  return (
    <p className="mx-3 mb-3 rounded-xl border border-dashed border-border/70 px-4 py-5 text-center text-xs text-muted-foreground">
      {text}
    </p>
  );
}

/** Agent icon chip: the registry glyph, or a neutral tile for an unknown id. */
function AgentIcon({ id }: { id: string }) {
  const iconId = getAgent(id)?.iconId;
  if (iconId) return <AgentToolIcon toolId={iconId} size="sm" />;
  return <span aria-hidden className="h-6 w-6 shrink-0 rounded-md border border-border/50 bg-muted/40" />;
}

/** Only `127.0.0.1:<port>` is drawn. Anything else, including a vendor URL, is blank. */
function loopbackText(value: string): string {
  const prefix = "127.0.0.1:";
  if (!value.startsWith(prefix)) return "";
  const port = value.slice(prefix.length);
  if (!/^\d{1,5}$/.test(port)) return "";
  return value;
}

function plainCell(value: string): string {
  if (value.includes("://") || value.includes("api.openai.com")) return "";
  return value;
}

function tokenCell(value: string): string {
  return /^\d+$/.test(value) ? value : "";
}

/** 2xx is a quiet success dot; anything else reads as a failure chip. */
function StatusPill({ status }: { status: number }) {
  if (!Number.isInteger(status)) return null;
  const ok = status >= 200 && status < 300;
  return (
    <span
      className={cn(
        "inline-flex items-center rounded-md px-1.5 py-0.5 font-mono text-[11px] tabular-nums",
        ok ? "bg-success/10 text-success" : "bg-destructive/10 text-destructive",
      )}
    >
      {status}
    </span>
  );
}

/** Recent forwarded calls. Headers name the call. A vendor URL or a non-digit token is blank. */
function RecentCalls({ calls }: { calls: RecentCallDto[] }) {
  return (
    <div className="min-h-0 flex-1 overflow-auto px-3 pb-3">
      <table className="w-full min-w-[420px] table-fixed text-left text-xs text-foreground">
        <thead className="sticky top-0 bg-card">
          <tr className="border-b border-border/60 text-muted-foreground">
            <th className="w-[88px] whitespace-nowrap px-2 py-1 font-normal" scope="col">
              Time
            </th>
            <th className="w-24 whitespace-nowrap px-2 py-1 font-normal" scope="col">
              Agent
            </th>
            <th className="whitespace-nowrap px-2 py-1 font-normal" scope="col">
              Model
            </th>
            <th className="w-[72px] whitespace-nowrap px-2 py-1 font-normal" scope="col">
              Status
            </th>
            <th className="w-[72px] whitespace-nowrap px-2 py-1 text-right font-normal" scope="col">
              Tokens
            </th>
          </tr>
        </thead>
        <tbody>
          {calls.map((call, index) => (
            <tr
              key={`${call.at}-${call.agent}-${index}`}
              className="border-b border-border/40 transition-colors last:border-0 hover:bg-muted/30"
            >
              <td className="truncate px-2 py-1 font-mono tabular-nums text-muted-foreground">{plainCell(call.at)}</td>
              <td className="truncate px-2 py-1">{plainCell(call.agent)}</td>
              <td className="truncate px-2 py-1 font-mono">{plainCell(call.model)}</td>
              <td className="whitespace-nowrap px-2 py-1">
                <StatusPill status={call.status} />
              </td>
              <td className="px-2 py-1 text-right font-mono tabular-nums text-muted-foreground">
                {tokenCell(call.completion_tokens)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** One labelled list card in the left rail: Agents or Providers. */
function RailCard({
  label,
  count,
  ready,
  emptyText,
  children,
}: {
  label: string;
  count: number;
  ready: boolean;
  emptyText: string;
  children: ReactNode;
}) {
  return (
    <section
      aria-label={label}
      className="flex min-h-0 flex-1 basis-0 flex-col overflow-hidden rounded-2xl border border-border bg-card"
    >
      <header className="flex shrink-0 items-baseline gap-2 px-3 pb-1 pt-2.5">
        <h2 className="text-[13px] font-semibold tracking-wide text-foreground">{label}</h2>
        {ready ? <span className="text-[11px] tabular-nums text-muted-foreground">{count}</span> : null}
      </header>
      {ready && count === 0 ? (
        <EmptyNote text={emptyText} />
      ) : (
        <ul className="min-h-0 flex-1 space-y-0.5 overflow-auto px-1.5 pb-2">{children}</ul>
      )}
    </section>
  );
}

/**
 * The model an agent is on, as one quiet chip: a dot, then the saved display
 * name. An unset agent says so instead of pretending.
 */
function ModelChip({ label }: { label: string }) {
  const { t } = useTranslation();
  if (!label.trim()) {
    return <span className="truncate text-[11px] text-muted-foreground">{t("models.picker.unset")}</span>;
  }
  return (
    <span className="inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-md bg-primary/10 px-1.5 py-0.5 text-[11px] leading-4 text-accent-foreground">
      <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
      <span className="truncate font-mono">{label}</span>
    </span>
  );
}

/** Shown only after that column's read succeeds with nothing in it. */
const EMPTY_COLUMN = {
  agents: "还没有探测到可配置的 Agent",
  providers: "还没有密钥",
  gateway: "还没有调用",
} as const;

/**
 * Models page: a left rail of Agents and Providers, and a wide Gateway panel
 * split by function — endpoints, listen mode, profiles, routing, group
 * members, and the recent-calls table. Clicking an agent opens the picker;
 * the row itself carries the model that agent is on. A drawer request from
 * the retired workbench is ignored.
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
  const [pickerId, setPickerId] = useState<string | null>(null);
  const [choices, setChoices] = useState<{ id: string; label: string }[]>([]);
  const [choicesLoading, setChoicesLoading] = useState(false);
  void modelsDrawerRequest;
  void clearModelsDrawerRequest;

  const agents = data?.agents ?? [];
  const providers = data?.providers ?? [];
  const gatewayRows = data?.gateway ?? [];
  const selectedProviderName = providers.find((row) => row.id === selectedProviderId)?.name ?? "";
  const pickerAgent = pickerId === null ? null : (agents.find((row) => row.id === pickerId) ?? null);

  const openPicker = (id: string) => {
    if (pickerId === id) {
      setPickerId(null);
      return;
    }
    setPickerId(id);
    setChoices([]);
    setChoicesLoading(true);
    void tauriInvoke("get_model_choices", { agentId: id })
      .then((loaded) => setChoices(loaded))
      .finally(() => setChoicesLoading(false));
  };

  const reloadChoices = () => {
    if (pickerId === null) return;
    void tauriInvoke("get_model_choices", { agentId: pickerId }).then(setChoices);
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div data-tauri-drag-region className="h-4 w-full shrink-0" aria-hidden />
      <div className="flex min-h-0 flex-1 flex-col gap-2.5 overflow-y-auto px-3 pb-3 lg:grid lg:grid-cols-[300px_minmax(0,1fr)] lg:overflow-hidden">
        {/* Left rail: the two pickable lists. */}
        <div className="flex min-h-0 shrink-0 flex-col gap-2.5">
          <RailCard
            label={t("models.columns.agents")}
            count={agents.length}
            ready={boardQuery.isSuccess}
            emptyText={EMPTY_COLUMN.agents}
          >
            {agents.map((row) => {
              const loopback = loopbackText(row.loopback_label ?? "");
              return (
                <li key={row.id}>
                  <Popover.Root
                    open={pickerId === row.id}
                    onOpenChange={(open) => {
                      if (!open && pickerId === row.id) setPickerId(null);
                    }}
                  >
                    <Popover.Anchor asChild>
                      <button
                        type="button"
                        aria-label={row.name}
                        onClick={() => openPicker(row.id)}
                        className={cn(
                          "flex w-full min-w-0 cursor-pointer items-start gap-2 rounded-lg px-2 py-1 text-left text-[13px] text-foreground transition",
                          pickerId === row.id ? "bg-primary/12 ring-1 ring-primary/25" : "hover:bg-muted/40",
                        )}
                      >
                        <AgentIcon id={row.id} />
                        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                          <span className="flex min-w-0 items-baseline gap-2">
                            <span className="truncate font-medium">{row.name}</span>
                            {loopback ? (
                              <span className="ml-auto shrink-0 font-mono text-[10px] font-normal tabular-nums text-muted-foreground">
                                {loopback}
                              </span>
                            ) : null}
                          </span>
                          <ModelChip label={row.model_label ?? ""} />
                        </span>
                      </button>
                    </Popover.Anchor>
                    {pickerId === row.id && pickerAgent ? (
                      <Popover.Portal>
                        <ModelPickerPopover
                          agent={{
                            id: pickerAgent.id,
                            name: pickerAgent.name,
                            modelLabel: pickerAgent.model_label ?? "",
                          }}
                          choices={choices}
                          loading={choicesLoading}
                          onClose={() => setPickerId(null)}
                          onRenamed={reloadChoices}
                        />
                      </Popover.Portal>
                    ) : null}
                  </Popover.Root>
                </li>
              );
            })}
          </RailCard>
          <RailCard
            label={t("models.columns.providers")}
            count={providers.length}
            ready={boardQuery.isSuccess}
            emptyText={EMPTY_COLUMN.providers}
          >
            {providers.map((row) => (
              <li key={row.id}>
                <button
                  type="button"
                  aria-label={row.name}
                  onClick={() => setSelectedProviderId(row.id)}
                  className={cn(
                    "flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-lg px-2 py-1 text-left text-[13px] text-foreground transition hover:bg-muted/40",
                    selectedProviderId === row.id && "bg-primary/12 font-medium ring-1 ring-primary/25",
                  )}
                >
                  <ProviderBrandIcon providerName={row.name} size="xs" />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate">{row.name}</span>
                    {row.credential_summary ? (
                      <span className="truncate font-mono text-[11px] text-muted-foreground">
                        {row.credential_summary}
                      </span>
                    ) : null}
                  </span>
                </button>
              </li>
            ))}
          </RailCard>
        </div>

        {/* Gateway panel: configuration blocks, then the calls table. */}
        <section
          aria-label={t("models.columns.gateway")}
          className="flex h-full min-h-64 min-w-0 flex-col overflow-hidden rounded-2xl border border-border bg-card"
        >
          <header className="flex shrink-0 items-baseline gap-2 px-3 pb-1 pt-2.5">
            <h2 className="text-[13px] font-semibold tracking-wide text-foreground">{t("models.columns.gateway")}</h2>
          </header>
          <div className="grid min-h-0 flex-1 lg:grid-cols-[minmax(300px,340px)_minmax(0,1fr)]">
            {/* Configuration column: endpoints, listen mode, profiles, routing, groups. */}
            <div className="min-h-0 shrink-0 space-y-4 overflow-auto border-b border-border/60 px-3 py-2.5 lg:border-b-0 lg:border-r">
              <GatewayEndpoints />
              <LanListen />
              <ProfileNames />
              <div className="space-y-2">
                <SectionLabel>路由 · 亲和</SectionLabel>
                {routingPage?.provider && selectedProviderId ? (
                  <RoutingControl
                    owner="provider"
                    id={selectedProviderId}
                    title={selectedProviderName}
                    routing={routingPage.provider.routing}
                    affinity={routingPage.provider.affinity}
                  />
                ) : null}
                {groups.map((group) => (
                  <RoutingControl
                    key={group.id}
                    owner="group"
                    id={group.id}
                    title={group.id}
                    routing={group.routing}
                    affinity={group.affinity}
                  />
                ))}
                {!routingPage?.provider && groups.length === 0 ? (
                  <p className="text-xs text-muted-foreground">在左侧选择一个 Provider 后可调整路由。</p>
                ) : null}
              </div>
              <GroupMembers />
            </div>
            {/* Calls column: gateway rows (rare) above the recent-calls table. */}
            <div className="flex min-h-40 min-w-0 flex-1 flex-col">
              {gatewayRows.length > 0 ? (
                <ul className="shrink-0 space-y-0.5 px-3 pt-3">
                  {gatewayRows.map((row) => (
                    <li key={row.id} className="truncate px-2 py-1 text-xs text-foreground">
                      {row.name}
                    </li>
                  ))}
                </ul>
              ) : null}
              {recentQuery.isSuccess && calls.length === 0 ? (
                <div className="flex min-h-0 flex-1 items-center justify-center p-6">
                  <p className="text-sm text-muted-foreground">{EMPTY_COLUMN.gateway}</p>
                </div>
              ) : calls.length > 0 ? (
                <>
                  <div className="flex shrink-0 items-baseline gap-2 px-3 pb-1 pt-2.5">
                    <SectionLabel>最近调用</SectionLabel>
                    <span className="text-[11px] tabular-nums text-muted-foreground">{calls.length}</span>
                  </div>
                  <RecentCalls calls={calls} />
                </>
              ) : null}
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}
