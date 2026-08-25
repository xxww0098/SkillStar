import { KeyRound } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ProviderBrandIcon } from "../../../../../../components/shared/ProviderBrandIcon";
import { cn } from "../../../../../../lib/utils";
import type { ProviderEntryFlat } from "../../../../../../types";
import { AgentToolIcon } from "../../../shared/AgentToolIcon";
import type { AgentDescriptor } from "../../../../lib/agentRegistry";
import {
  findOfficialProvider,
  isToolOnOfficial,
  matrixProviders,
  toolSupportsOfficial,
} from "../../../../lib/officialProviders";
import { EditorPage } from "../EditorPage";
import type { EditorDetailStyle, ModelsHubData } from "../types";
import { visibleColumns } from "../AgentColumnCarousel";
import { ClaudeSurfaceIcon } from "../../../shared/ClaudeSurfaceIcon";
import { bindAgentForColumn, type MatrixColumn } from "../matrixColumns";
import { MatrixChrome, providerReady } from "../MatrixChrome";

export function Pill({ children, tone }: { children: ReactNode; tone?: "ok" | "warn" }) {
  return (
    <span
      className={cn(
        "rounded px-1.5 py-0.5 text-[9px] font-medium uppercase tracking-wide",
        tone === "ok" && "bg-emerald-500/10 text-emerald-400",
        tone === "warn" && "bg-amber-500/10 text-amber-400",
        !tone && "bg-muted/50 text-muted-foreground",
      )}
    >
      {children}
    </span>
  );
}

export function providerModels(provider: ProviderEntryFlat): string[] {
  return provider.models?.length ? provider.models : [provider.default_model].filter(Boolean);
}

export function isCompatible(provider: ProviderEntryFlat, agent: AgentDescriptor): boolean {
  return agent.requiredUrlField === "anthropic"
    ? Boolean(provider.base_url_anthropic)
    : Boolean(provider.base_url_openai);
}

type RichMatrixShellProps = {
  data: ModelsHubData;
  subtitle: string;
  legend: ReactNode;
  editorStyle?: EditorDetailStyle;
  /** How dense the provider meta column is. */
  providerCol?: "full" | "compact";
  /** Optional column hint under the agent name (e.g. Claude → "mapping"). */
  columnHint?: (column: MatrixColumn) => string | null;
  renderCell: (args: { provider: ProviderEntryFlat; column: MatrixColumn; agent: AgentDescriptor }) => ReactNode;
};

/**
 * Shared B2-family shell: rich provider×agent matrix beside ModelsSidebar.
 * Only the cell interaction / legend / editor chrome differ per sub-variant.
 */
export function RichMatrixShell({
  data,
  subtitle,
  legend,
  editorStyle = "sections",
  providerCol = "full",
  columnHint,
  renderCell,
}: RichMatrixShellProps) {
  const { t } = useTranslation();
  if (data.overlay.type === "create" || data.overlay.type === "app-ai" || data.overlay.type === "agent-settings") {
    return <EditorPage data={data} detailStyle={editorStyle} />;
  }

  const columns = visibleColumns(data);
  // Official seeds stay in store for activate, but never as cross-ref rows.
  const rows = matrixProviders(data.providers);

  return (
    <MatrixChrome data={data} title={t("models.matrix.title")} subtitle={subtitle} toolbar={legend}>
      {/*
          Sticky Provider col + Agent cols. Table is w-full so leftover width
          is a bordered filler column (grid lines, not blank). Extra agents
          still force horizontal scroll via min-w on agent cols.
        */}
      <div className="overflow-x-auto overscroll-x-contain rounded-2xl border border-border/80 bg-card/70 shadow-sm [-webkit-overflow-scrolling:touch]">
        <table className="w-full min-w-max border-collapse text-left text-xs">
          <colgroup>
            <col className="w-[200px]" />
            {columns.map((column) => (
              <col key={column.columnId} className={column.claudeSurface ? "w-[168px]" : "w-[152px]"} />
            ))}
            {/* Absorbs remaining width so row/column borders fill the frame */}
            <col />
          </colgroup>
          <thead>
            <tr className="border-b border-border/70">
              <th
                className={cn(
                  "sticky left-0 z-30 w-[200px] min-w-[200px] max-w-[200px] px-4 py-3 text-center",
                  "border-r border-border/80 bg-muted font-bold text-foreground",
                  "shadow-[4px_0_12px_-6px_rgba(0,0,0,0.35)]",
                )}
              >
                {t("models.matrix.providerHeader")}
              </th>
              {columns.map((column) => {
                const hint = columnHint?.(column);
                const wide = Boolean(column.claudeSurface);
                return (
                  <th
                    key={column.columnId}
                    className={cn(
                      "border-r border-border/50 bg-muted/60 px-2 py-3 text-center font-bold",
                      wide ? "w-[168px] min-w-[168px] max-w-[168px]" : "w-[152px] min-w-[152px] max-w-[152px]",
                    )}
                  >
                    <ColumnHeader data={data} column={column} hint={hint} />
                  </th>
                );
              })}
              <th className="min-w-[48px] bg-muted/60" aria-hidden />
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td colSpan={columns.length + 2} className="px-4 py-10 text-center text-[12px] text-muted-foreground">
                  {t("models.matrix.noThirdPartyProviders")}
                </td>
              </tr>
            ) : (
              rows.map((provider) => {
                const ready = providerReady(provider);
                const rowActive = data.selectedProviderId === provider.id;
                return (
                  <tr
                    key={provider.id}
                    className="border-b border-border/50 last:border-0 hover:bg-muted/20 transition-colors"
                  >
                    <td
                      className={cn(
                        "sticky left-0 z-20 w-[200px] min-w-[200px] max-w-[200px] px-4 py-3",
                        "border-r border-border/80 shadow-[4px_0_12px_-6px_rgba(0,0,0,0.35)]",
                        rowActive ? "bg-primary/[0.08]" : "bg-card",
                      )}
                    >
                      <ProviderCol
                        provider={provider}
                        ready={ready}
                        density={providerCol}
                        onEdit={() => data.setOverlay({ type: "edit", providerId: provider.id })}
                      />
                    </td>
                    {columns.map((column) => {
                      const wide = Boolean(column.claudeSurface);
                      const agent = bindAgentForColumn(column);
                      return (
                        <td
                          key={column.columnId}
                          className={cn(
                            "border-r border-border/35 px-2 py-2 align-middle",
                            wide ? "w-[168px] min-w-[168px] max-w-[168px]" : "w-[152px] min-w-[152px] max-w-[152px]",
                            rowActive && "bg-primary/[0.03]",
                          )}
                        >
                          {renderCell({ provider, column, agent })}
                        </td>
                      );
                    })}
                    <td className={cn("min-w-[48px]", rowActive && "bg-primary/[0.03]")} aria-hidden />
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </MatrixChrome>
  );
}

/** Agent column header: icon + Official switch for Claude / Codex (not a matrix row). */
function ColumnHeader({
  data,
  column,
  hint,
}: {
  data: ModelsHubData;
  column: MatrixColumn;
  hint: string | null | undefined;
}) {
  const { t } = useTranslation();
  const supportsOfficial = toolSupportsOfficial(column.bindToolId);
  const onOfficial = supportsOfficial && isToolOnOfficial(data.providers, data.toolActivations, column.bindToolId);

  const toggleOfficial = () => {
    if (!supportsOfficial) return;
    if (onOfficial) {
      void data.deactivateTool(column.bindToolId);
      return;
    }
    const seed = findOfficialProvider(data.providers, column.bindToolId);
    if (!seed) return;
    void data.activateTool(seed.id, column.bindToolId);
  };

  const title =
    column.claudeSurface === "cli"
      ? "Claude CLI"
      : column.claudeSurface === "desktop"
        ? "Claude Desktop"
        : column.displayName;

  const fallbackHint =
    hint ??
    (column.claudeSurface === "cli"
      ? "terminal"
      : column.claudeSurface === "desktop"
        ? "monitor"
        : column.kind === "multi"
          ? "multi"
          : "single");

  return (
    <div className="flex flex-col items-center gap-1">
      <div className="relative">
        {column.claudeSurface ? (
          <ClaudeSurfaceIcon surface={column.claudeSurface} size={18} />
        ) : (
          <AgentToolIcon toolId={column.bindToolId} size="sm" />
        )}
        {supportsOfficial ? (
          <button
            type="button"
            title={onOfficial ? t("models.matrix.officialOnClickToCancel") : t("models.matrix.switchToOfficial")}
            aria-pressed={onOfficial}
            onClick={toggleOfficial}
            className={cn(
              "absolute -right-1.5 -top-1.5 flex h-4 w-4 items-center justify-center rounded-full border transition-colors",
              onOfficial
                ? "border-sky-400/70 bg-sky-500 text-white shadow-[0_0_0_1px_rgba(56,189,248,0.35)]"
                : "border-border/70 bg-muted text-muted-foreground hover:border-sky-500/50 hover:text-sky-400",
            )}
          >
            <KeyRound className="h-2.5 w-2.5" />
          </button>
        ) : null}
      </div>
      <span className="max-w-[9rem] text-[10px] leading-tight">{title}</span>
      {supportsOfficial ? (
        <button
          type="button"
          onClick={toggleOfficial}
          className={cn(
            "rounded px-1.5 py-0.5 text-[9px] font-medium transition-colors",
            onOfficial ? "bg-sky-500/15 text-sky-400" : "text-muted-foreground hover:bg-sky-500/10 hover:text-sky-400",
          )}
        >
          {onOfficial ? t("models.matrix.officialEnabled") : t("models.matrix.backToOfficial")}
        </button>
      ) : (
        <span className="text-[9px] font-normal text-muted-foreground">{fallbackHint}</span>
      )}
    </div>
  );
}

function ProviderCol({
  provider,
  ready,
  density,
  onEdit,
}: {
  provider: ProviderEntryFlat;
  ready: { label: string; tone: "ok" | "warn" };
  density: "full" | "compact";
  onEdit: () => void;
}) {
  return (
    <div className="flex items-start gap-2">
      <ProviderBrandIcon
        presetId={provider.preset_id}
        providerName={provider.name}
        iconColor={provider.icon_color}
        size="sm"
      />
      <div className="min-w-0 flex-1">
        <button type="button" className="truncate font-medium hover:underline" onClick={onEdit}>
          {provider.name}
        </button>
        {density === "full" ? (
          <>
            <div className="mt-1 flex flex-wrap gap-1">
              <Pill tone={ready.tone}>{ready.label}</Pill>
              {provider.base_url_openai ? <Pill>OpenAI</Pill> : null}
              {provider.base_url_anthropic ? <Pill>Anthropic</Pill> : null}
            </div>
            <p className="mt-1 truncate font-mono text-[10px] text-muted-foreground">
              default {provider.default_model || "—"}
            </p>
          </>
        ) : (
          <p className="mt-0.5 truncate text-[10px] text-muted-foreground">
            <span className={ready.tone === "ok" ? "text-emerald-500/80" : "text-amber-500/80"}>{ready.label}</span>
            {" · "}
            <span className="font-mono">{provider.default_model || "—"}</span>
          </p>
        )}
      </div>
    </div>
  );
}
