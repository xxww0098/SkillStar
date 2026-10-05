import { Activity, ChevronRight } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Popover } from "radix-ui";
import { useTranslation } from "react-i18next";
import type { RecentCallDto } from "@/types/generated/RecentCallDto";
import { ProviderBrandIcon } from "@/components/shared/ProviderBrandIcon";
import { cn } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { useModelsBoard } from "../../api/board";
import { useRecentCalls } from "../../api/recent";
import { useRouteComparison } from "../../api/routes";
import { useRoutingPage } from "../../api/routing";
import { getAgent } from "../../lib/agentRegistry";
import type { ModelsNavBridge } from "../../lib/navBridge";
import { AgentToolIcon } from "../shared/AgentToolIcon";
import { GatewayEndpoints } from "./GatewayEndpoints";
import { GroupMembers } from "./GroupMembers";
import { LanListen } from "./LanListen";
import { ModelPickerPopover } from "./ModelPickerPopover";
import { ProfileNames } from "./ProfileNames";
import { RouteCandidates, ServingAgents } from "./RouteCandidates";
import { RoutingControl } from "./RoutingControl";

/** Quiet card that owns one functional block of the workspace. */
function HubCard({
  label,
  ariaLabel,
  headerAside,
  children,
  className,
}: {
  label: ReactNode;
  ariaLabel?: string;
  headerAside?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section
      aria-label={typeof ariaLabel === "string" ? ariaLabel : undefined}
      className={cn(
        "flex min-h-0 min-w-0 flex-col overflow-hidden rounded-2xl border border-border/70 bg-card",
        "shadow-[0_1px_2px_rgba(15,23,42,0.04)]",
        className,
      )}
    >
      <header className="flex shrink-0 items-center gap-3 px-4 pb-2 pt-3">
        <h2 className="shrink-0 text-[13px] font-semibold tracking-wide text-foreground">{label}</h2>
        {headerAside ? <div className="ml-auto flex min-w-0 items-center gap-2">{headerAside}</div> : null}
      </header>
      {children}
    </section>
  );
}

/** Small caps label that groups one functional block inside a card. */
function SectionLabel({ children }: { children: ReactNode }) {
  return <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">{children}</div>;
}

/** Hairline that separates blocks inside one card without adding chrome. */
function SectionDivider() {
  return <div className="mx-4 border-t border-border/50" aria-hidden />;
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

/**
 * Recent forwarded calls from the ledger-backed view. Headers name the call;
 * an unknown in-token count, session, or latency is blank, and a vendor URL
 * or a non-digit token is blank.
 */
function RecentCalls({ calls }: { calls: RecentCallDto[] }) {
  const { t } = useTranslation();
  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <table className="w-full min-w-[620px] table-fixed text-left text-xs text-foreground">
        <thead className="sticky top-0 z-10 bg-card">
          <tr className="border-b border-border/60 text-[11px] text-muted-foreground">
            <th className="w-[88px] whitespace-nowrap px-4 py-1.5 font-normal" scope="col">
              {t("models.recentCalls.time")}
            </th>
            <th className="w-[76px] whitespace-nowrap px-2 py-1.5 font-normal" scope="col">
              {t("models.recentCalls.agent")}
            </th>
            <th className="whitespace-nowrap px-2 py-1.5 font-normal" scope="col">
              {t("models.recentCalls.model")}
            </th>
            <th className="w-[64px] whitespace-nowrap px-2 py-1.5 font-normal" scope="col">
              {t("models.recentCalls.session")}
            </th>
            <th className="w-[56px] whitespace-nowrap px-2 py-1.5 font-normal" scope="col">
              {t("models.recentCalls.status")}
            </th>
            <th className="w-[60px] whitespace-nowrap px-2 py-1.5 text-right font-normal" scope="col">
              {t("models.recentCalls.in")}
            </th>
            <th className="w-[60px] whitespace-nowrap px-2 py-1.5 text-right font-normal" scope="col">
              {t("models.recentCalls.tokens")}
            </th>
            <th className="w-[64px] whitespace-nowrap px-4 py-1.5 text-right font-normal" scope="col">
              {t("models.recentCalls.latency")}
            </th>
          </tr>
        </thead>
        <tbody>
          {calls.map((call, index) => (
            <tr
              key={`${call.at}-${call.agent}-${index}`}
              className="border-b border-border/35 transition-colors last:border-0 hover:bg-muted/25"
            >
              <td className="truncate px-4 py-1.5 font-mono tabular-nums text-muted-foreground">
                {plainCell(call.at)}
              </td>
              <td className="truncate px-2 py-1.5">{plainCell(call.agent)}</td>
              <td className="truncate px-2 py-1.5 font-mono">{plainCell(call.model)}</td>
              <td className="truncate px-2 py-1.5 font-mono text-muted-foreground" title={call.session}>
                {plainCell(call.session ?? "")}
              </td>
              <td className="whitespace-nowrap px-2 py-1.5">
                <StatusPill status={call.status} />
              </td>
              <td className="px-2 py-1.5 text-right font-mono tabular-nums text-muted-foreground">
                {tokenCell(call.in_tokens ?? "")}
              </td>
              <td className="px-2 py-1.5 text-right font-mono tabular-nums text-muted-foreground">
                {tokenCell(call.completion_tokens)}
              </td>
              <td className="px-4 py-1.5 text-right font-mono tabular-nums text-muted-foreground">
                {tokenCell(call.latency ?? "")}
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
      className="flex min-h-0 flex-1 basis-0 flex-col overflow-hidden rounded-2xl border border-border/70 bg-card shadow-[0_1px_2px_rgba(15,23,42,0.04)]"
    >
      <header className="flex shrink-0 items-baseline gap-2 px-4 pb-1.5 pt-3">
        <h2 className="text-[13px] font-semibold tracking-wide text-foreground">{label}</h2>
        {ready ? <span className="text-[11px] tabular-nums text-muted-foreground">{count}</span> : null}
      </header>
      {ready && count === 0 ? (
        <p className="mx-3 mb-3 flex flex-1 items-center justify-center rounded-xl border border-dashed border-border/60 px-4 py-5 text-center text-xs leading-relaxed text-muted-foreground">
          {emptyText}
        </p>
      ) : (
        <ul className="min-h-0 flex-1 space-y-0.5 overflow-auto px-2 pb-2">{children}</ul>
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
    return <span className="truncate text-[11px] text-muted-foreground/80">{t("models.picker.unset")}</span>;
  }
  return (
    <span className="inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-md bg-accent/60 px-1.5 py-0.5 text-[11px] leading-4 text-accent-foreground">
      <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
      <span className="truncate font-mono">{label}</span>
    </span>
  );
}

/**
 * Models page: a left rail of Agents and Providers; a workspace that stacks
 * the Gateway configuration card over the full-width calls ledger — pick in
 * the rail, configure in the card, observe in the table. Clicking an agent
 * opens the picker; the row itself carries the model that agent is on. A
 * drawer request from the retired workbench is ignored. A cross-view focus
 * request (from the Usage page) either focuses one agent's routes or shows
 * which agents route to one catalog.
 */
export function ModelsHub({
  selectedProviderId,
  setSelectedProviderId,
  modelsDrawerRequest,
  clearModelsDrawerRequest,
  modelsFocusRequest,
  clearModelsFocusRequest,
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
  /** The catalog the Usage quota card jumped to, until dismissed. */
  const [servingCatalogId, setServingCatalogId] = useState<string | null>(null);
  void modelsDrawerRequest;
  void clearModelsDrawerRequest;

  const agents = data?.agents ?? [];
  const providers = data?.providers ?? [];
  const gatewayRows = data?.gateway ?? [];
  const selectedProviderName = providers.find((row) => row.id === selectedProviderId)?.name ?? "";
  const pickerAgent = pickerId === null ? null : (agents.find((row) => row.id === pickerId) ?? null);

  // Cross-view focus (Usage → Models triangle): an agent opens its picker
  // (and its routes show below); a catalog switches the panel to the
  // serving join. The request is consumed on arrival.
  useEffect(() => {
    if (!modelsFocusRequest) return;
    if (modelsFocusRequest.kind === "agent") {
      setServingCatalogId(null);
      setPickerId(modelsFocusRequest.agentId);
    } else {
      setServingCatalogId(modelsFocusRequest.catalogId);
    }
    clearModelsFocusRequest();
  }, [modelsFocusRequest, clearModelsFocusRequest]);

  // The focused agent's model drives the candidate comparison.
  const routeModel = (pickerAgent?.model_label ?? "").trim() || null;
  const routeQuery = useRouteComparison(routeModel);

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
      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto px-4 pb-4 lg:grid lg:grid-cols-[300px_minmax(0,1fr)] lg:overflow-hidden">
        {/* Left rail: the two pickable lists. */}
        <div className="flex min-h-0 shrink-0 flex-col gap-3">
          <RailCard
            label={t("models.columns.agents")}
            count={agents.length}
            ready={boardQuery.isSuccess}
            emptyText={t("models.gateway.emptyAgents")}
          >
            {agents.map((row) => {
              const loopback = loopbackText(row.loopback_label ?? "");
              const open = pickerId === row.id;
              return (
                <li key={row.id}>
                  <Popover.Root
                    open={open}
                    onOpenChange={(next) => {
                      if (!next && open) setPickerId(null);
                    }}
                  >
                    <Popover.Anchor asChild>
                      <button
                        type="button"
                        aria-label={row.name}
                        onClick={() => openPicker(row.id)}
                        className={cn(
                          "group flex w-full min-w-0 cursor-pointer items-start gap-2.5 rounded-xl px-2.5 py-2 text-left text-[13px] text-foreground transition-colors",
                          open ? "bg-accent/70 ring-1 ring-primary/30" : "hover:bg-muted/50",
                        )}
                      >
                        <AgentIcon id={row.id} />
                        <span className="flex min-w-0 flex-1 flex-col gap-1">
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
                        <ChevronRight
                          aria-hidden
                          className={cn(
                            "mt-0.5 h-3.5 w-3.5 shrink-0 self-center transition",
                            open
                              ? "rotate-90 text-primary"
                              : "text-muted-foreground/0 group-hover:text-muted-foreground/70",
                          )}
                        />
                      </button>
                    </Popover.Anchor>
                    {open && pickerAgent ? (
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
            emptyText={t("models.gateway.emptyProviders")}
          >
            {providers.map((row) => {
              const selected = selectedProviderId === row.id;
              return (
                <li key={row.id}>
                  <button
                    type="button"
                    aria-label={row.name}
                    onClick={() => setSelectedProviderId(row.id)}
                    className={cn(
                      "flex w-full min-w-0 cursor-pointer items-center gap-2.5 rounded-xl px-2.5 py-2 text-left text-[13px] text-foreground transition-colors",
                      selected ? "bg-accent/70 ring-1 ring-primary/30" : "hover:bg-muted/50",
                    )}
                  >
                    <ProviderBrandIcon providerName={row.name} size="xs" />
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className="truncate font-medium">{row.name}</span>
                      {row.credential_summary ? (
                        <span className="truncate font-mono text-[11px] text-muted-foreground">
                          {row.credential_summary}
                        </span>
                      ) : null}
                    </span>
                    {selected ? <span aria-hidden className="h-1.5 w-1.5 shrink-0 rounded-full bg-primary" /> : null}
                  </button>
                </li>
              );
            })}
          </RailCard>
        </div>

        {/* Workspace: gateway configuration over the calls ledger. */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3">
          <HubCard
            label={t("models.columns.gateway")}
            ariaLabel={t("models.columns.gateway")}
            headerAside={<LanListen />}
            className="shrink-0"
          >
            <div className="px-1 pb-1">
              <GatewayEndpoints />
            </div>
            <SectionDivider />
            {/* Two columns on wide cards: routing decisions on the left,
                reusable presets (profiles, groups) on the right. */}
            <div className="grid gap-x-6 px-4 pb-4 pt-3.5 xl:grid-cols-2">
              <div className="min-w-0 space-y-2.5 pb-4 xl:pb-0">
                <SectionLabel>{t("models.gateway.routingSection")}</SectionLabel>
                <div className="space-y-1">
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
                    <p className="text-xs leading-relaxed text-muted-foreground">
                      {t("models.gateway.routingPickProvider")}
                    </p>
                  ) : null}
                </div>
                {/* Cross-view (slice 13): the catalog jump target first (the
                 *  Usage quota card's "which agents"), then the focused
                 *  agent's candidate chips. */}
                {servingCatalogId ? (
                  <div className="border-t border-border/50 pt-3">
                    <ServingAgents
                      catalogId={servingCatalogId}
                      agents={agents}
                      onOpenPicker={(agentId) => openPicker(agentId)}
                    />
                  </div>
                ) : null}
                {pickerAgent && routeModel ? (
                  <div className="border-t border-border/50 pt-3">
                    <RouteCandidates
                      agentName={pickerAgent.name}
                      modelRef={routeModel}
                      comparison={routeQuery.data}
                      loading={routeQuery.isLoading}
                      onOpenPicker={() => openPicker(pickerAgent.id)}
                    />
                  </div>
                ) : !servingCatalogId ? (
                  <p className="text-xs leading-relaxed text-muted-foreground/80">{t("models.routes.pickAgent")}</p>
                ) : null}
              </div>
              <div className="min-w-0 space-y-4 border-t border-border/50 pt-3.5 xl:border-l xl:border-t-0 xl:pl-6 xl:pt-0">
                <ProfileNames />
                <div className="border-t border-border/50 pt-3.5">
                  <GroupMembers />
                </div>
              </div>
            </div>
          </HubCard>

          <HubCard
            label={
              <span className="flex items-center gap-2">
                {t("models.gateway.recentCalls")}
                {recentQuery.isSuccess && calls.length > 0 ? (
                  <span className="text-[11px] font-normal tabular-nums text-muted-foreground">{calls.length}</span>
                ) : null}
              </span>
            }
            ariaLabel={t("models.gateway.recentCalls")}
            className="min-h-[220px] flex-1"
          >
            {gatewayRows.length > 0 ? (
              <ul className="shrink-0 space-y-0.5 border-b border-border/50 px-4 pb-2.5">
                {gatewayRows.map((row) => (
                  <li key={row.id} className="truncate px-1 py-1 text-xs text-foreground">
                    {row.name}
                  </li>
                ))}
              </ul>
            ) : null}
            {recentQuery.isSuccess && calls.length === 0 ? (
              <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
                <span className="flex h-9 w-9 items-center justify-center rounded-full bg-muted/60 text-muted-foreground">
                  <Activity className="h-4 w-4" aria-hidden />
                </span>
                <p className="text-[13px] text-muted-foreground">{t("models.gateway.emptyCalls")}</p>
                <p className="max-w-sm text-xs leading-relaxed text-muted-foreground/70">
                  {t("models.gateway.emptyCallsHint")}
                </p>
              </div>
            ) : calls.length > 0 ? (
              <RecentCalls calls={calls} />
            ) : (
              <div className="flex min-h-0 flex-1 items-center justify-center p-6">
                <p className="text-xs text-muted-foreground">{t("common.loading", "…")}</p>
              </div>
            )}
          </HubCard>
        </div>
      </div>
    </div>
  );
}
