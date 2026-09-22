import { useQuery } from "@tanstack/react-query";
import { ArrowUpCircle, Check, Copy, Download, ExternalLink, Globe, Sparkles, Star, Terminal } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { DrawerShell } from "../../../components/shared/DrawerShell";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { Markdown } from "../../../components/ui/Markdown";
import { tauriInvoke } from "../../../lib/ipc";
import { toast } from "../../../lib/toast";
import { copyToClipboard } from "../../../lib/utils";
import type {
  LocalFirstResult,
  McpArgument,
  McpMarketEntry,
  McpMarketServerDetail,
  McpRegistryPackageSummary,
} from "../../../types";
import { mcpKeys } from "../api/keys";
import { useCardGridColumns } from "../hooks/useCardGrid";
import type { McpMarketSection } from "../lib/curatedShelves";
import { type McpEntryStatus, type McpInstalledIndex, resolveMcpEntryStatus } from "../lib/installState";
import { McpEntryIcon, McpMarketCard } from "./McpMarketCard";
import { McpDeprecatedBadge, McpSupersededBadge } from "./McpStateBadges";

/**
 * Renders a page of catalog entries as a card grid, plus the detail drawer a
 * card opens.
 *
 * It owns no page-level state: loading, empty, remote-error and pagination all
 * live on `McpMarketPage`, which is the only caller. That is deliberate — when
 * both components tried to own the empty state, the browser's copy was
 * unreachable (the page never rendered it without entries) and its retry
 * affordance never appeared.
 */
interface McpMarketBrowserProps {
  /**
   * Installed servers, indexed by source fingerprint. Replaces the old
   * `Set<string>` of config keys, which could not tell two servers with the
   * same sanitized name apart and had no version to compare against.
   */
  installedIndex: McpInstalledIndex;
  /** Open the install wizard for this marketplace entry id. */
  onInstall: (id: string) => void;
  entries: McpMarketEntry[];
  /**
   * Curated scope renders shelves — labeled groups the page pre-computed.
   * Absent means the flat card grid (full catalog).
   */
  sections?: McpMarketSection[];
}

export function McpMarketBrowser({ installedIndex, onInstall, entries, sections }: McpMarketBrowserProps) {
  const { t } = useTranslation();
  const [detailId, setDetailId] = useState<string | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const { gridStyle } = useCardGridColumns(containerRef, entries.length);

  const detailQuery = useQuery<LocalFirstResult<McpMarketServerDetail | null>>({
    queryKey: mcpKeys.marketDetail(detailId),
    queryFn: () => tauriInvoke("get_mcp_market_server_detail_local", { id: detailId as string }),
    enabled: detailId != null,
  });
  const detail = detailQuery.data?.data ?? null;

  const cardGrid = (list: McpMarketEntry[]) => (
    <div className="ss-cards-grid" style={gridStyle}>
      {list.map((entry) => (
        <div key={entry.id} className="h-full">
          <McpMarketCard
            entry={entry}
            status={resolveMcpEntryStatus(entry, installedIndex)}
            onInstall={() => onInstall(entry.id)}
            onOpenDetail={() => setDetailId(entry.id)}
          />
        </div>
      ))}
    </div>
  );

  return (
    <>
      <div ref={containerRef} className={sections ? "space-y-6" : undefined}>
        {sections
          ? sections.map((section) => (
              <section key={section.key || "other"}>
                <div className="mb-3 flex items-center gap-3">
                  <h3 className="shrink-0 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                    {t(`mcp.shelf_${section.key || "other"}`, { defaultValue: section.key })}
                  </h3>
                  <span className="text-[11px] tabular-nums text-muted-foreground/70">{section.entries.length}</span>
                  <div className="h-px min-w-6 flex-1 bg-border/50" />
                </div>
                {cardGrid(section.entries)}
              </section>
            ))
          : cardGrid(entries)}
      </div>

      <DrawerShell
        open={detailId != null}
        onOpenChange={(open) => {
          if (!open) setDetailId(null);
        }}
        title={
          detail ? (
            <span className="flex min-w-0 items-center gap-2.5">
              <McpEntryIcon entry={detail} />
              <span className="min-w-0 truncate text-foreground">{detail.name}</span>
              {detail.recommended ? <Sparkles className="h-3.5 w-3.5 shrink-0 text-primary" /> : null}
            </span>
          ) : (
            <span className="text-foreground">{t("mcp.title")}</span>
          )
        }
        subtitle={
          detail
            ? detail.namespace !== detail.name
              ? detail.namespace
              : detail.source
                ? `${t(`mcp.shelf_${detail.source}`, { defaultValue: detail.source })} · ${t(`mcp.kind_${detail.kind}`)}`
                : t(`mcp.kind_${detail.kind}`)
            : undefined
        }
      >
        {detailQuery.isLoading ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">
            {t("common.loading")}
          </div>
        ) : detail ? (
          <MarketDetail
            detail={detail}
            status={resolveMcpEntryStatus(detail, installedIndex)}
            onInstall={() => {
              const id = detail.id;
              setDetailId(null);
              onInstall(id);
            }}
          />
        ) : (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">{t("mcp.notFound")}</div>
        )}
      </DrawerShell>
    </>
  );
}

/**
 * One declared argument rendered as a shell token: named args become
 * `--name value`, positional args their bare value. Mirrors the tokenization
 * in `skillstar_app::mcp::draft::argument_tokens`.
 */
function argumentToken(arg: McpArgument): string | null {
  const value = arg.value ?? arg.default ?? null;
  if (arg.kind === "named" && arg.name) {
    const flag = arg.name.startsWith("-") ? arg.name : `--${arg.name}`;
    return value ? `${flag} ${value}` : flag;
  }
  return value;
}

/**
 * The command line the install wizard actually runs — mirrors
 * `skillstar_app::mcp::draft::package_args`: runtime args, then the launcher
 * convention (`-y` for npx/bunx, `run -i --rm` for docker), then
 * `identifier@version` (`:` separator for docker), then package args.
 */
function packageCommandLine(pkg: McpRegistryPackageSummary): string {
  const tokens: string[] = [pkg.runtime];
  for (const arg of pkg.runtimeArguments ?? []) {
    const token = argumentToken(arg);
    if (token) tokens.push(token);
  }
  const identifier = pkg.identifier.trim();
  const versioned =
    identifier && pkg.version ? `${identifier}${pkg.runtime === "docker" ? ":" : "@"}${pkg.version}` : identifier;
  if (pkg.runtime === "npx" || pkg.runtime === "bunx") {
    if (!tokens.includes("-y") && !tokens.includes("--yes")) tokens.push("-y");
    if (identifier) tokens.push(versioned);
  } else if (pkg.runtime === "docker") {
    if (!tokens.includes("run")) tokens.push("run", "-i", "--rm");
    if (identifier) tokens.push(versioned);
  } else if (identifier) {
    tokens.push(versioned);
  }
  for (const arg of pkg.packageArguments ?? []) {
    const token = argumentToken(arg);
    if (token) tokens.push(token);
  }
  return tokens.join(" ");
}

/** A single-copy affordance on a command block: icon swaps to a check briefly. */
function CopyCommandButton({ text }: { text: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      title={t("mcp.copyCommand")}
      onClick={async () => {
        if (await copyToClipboard(text)) {
          setCopied(true);
          setTimeout(() => setCopied(false), 1600);
        } else {
          toast.error(t("common.copyFailed"));
        }
      }}
      className="shrink-0 rounded-md p-1 text-muted-foreground/70 transition-colors hover:bg-muted hover:text-foreground focus-ring"
    >
      {copied ? <Check className="h-3.5 w-3.5 text-emerald-500" /> : <Copy className="h-3.5 w-3.5" />}
    </button>
  );
}

/**
 * The drawer's body: meta line → description → install CTA → how it runs →
 * README. The installed state is an info row, not a disabled button — a
 * greyed-out primary button still reads as the page's main action.
 */
function MarketDetail({
  detail,
  status,
  onInstall,
}: {
  detail: McpMarketServerDetail;
  status: McpEntryStatus;
  onInstall: () => void;
}) {
  const { t } = useTranslation();
  const enabledCount = status.installed ? Object.values(status.installed.enabled).filter(Boolean).length : 0;

  // Curated seeds write `readme = "# name\n\ndescription"` — rendering that
  // repeats the paragraph directly above it, so only real readmes show.
  const readmeBody = detail.readme?.replace(/^#\s+.*\n/, "").trim() ?? "";
  const showReadme = readmeBody.length > 0 && readmeBody !== detail.description.trim();

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5 text-xs text-muted-foreground">
        <McpDeprecatedBadge deprecated={status.deprecated} />
        <McpSupersededBadge superseded={status.superseded} />
        {detail.stars > 0 ? (
          <span className="inline-flex items-center gap-1">
            <Star className="h-3.5 w-3.5" />
            {detail.stars.toLocaleString()}
          </span>
        ) : null}
        {detail.license ? <span>{detail.license}</span> : null}
        {detail.version ? <span>v{detail.version}</span> : null}
        {detail.repoUrl ? (
          <ExternalAnchor href={detail.repoUrl} className="inline-flex items-center gap-1 hover:text-foreground">
            <ExternalLink className="h-3.5 w-3.5" />
            {t("mcp.repo")}
          </ExternalAnchor>
        ) : null}
      </div>

      {detail.description ? <p className="text-sm leading-relaxed text-foreground/80">{detail.description}</p> : null}

      {status.state === "installed" ? (
        <div className="flex items-center gap-2 rounded-xl border border-emerald-500/25 bg-emerald-500/10 px-3.5 py-2.5 text-sm text-emerald-600 dark:text-emerald-400 paper:text-emerald-700">
          <Check className="h-4 w-4 shrink-0" />
          <span>
            {t("mcp.badgeInstalled")}
            {enabledCount > 0 ? ` · ${t("mcp.installedTargets", { count: enabledCount })}` : ""}
          </span>
        </div>
      ) : (
        <Button onClick={onInstall} className="w-full gap-1.5">
          {status.state === "updateAvailable" ? (
            <ArrowUpCircle className="h-4 w-4" />
          ) : (
            <Download className="h-4 w-4" />
          )}
          {status.state === "updateAvailable" ? t("mcp.updateAction") : t("mcp.installToTools")}
        </Button>
      )}

      {detail.packages.length > 0 ? (
        <section className="space-y-2">
          <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            <Terminal className="h-3.5 w-3.5" />
            {t("mcp.localRun")}
          </h4>
          {detail.packages.map((pkg) => {
            const command = packageCommandLine(pkg);
            return (
              <div
                key={`${pkg.runtime}-${pkg.identifier}`}
                className="rounded-xl border border-border/50 bg-muted/30 px-3.5 py-2.5"
              >
                <div className="flex items-start gap-2">
                  <code className="min-w-0 flex-1 break-all pt-0.5 font-mono text-xs leading-relaxed text-foreground">
                    {command}
                  </code>
                  <CopyCommandButton text={command} />
                </div>
                {pkg.requiredEnv.length > 0 ? (
                  <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                    {t("mcp.requiredEnv", { keys: pkg.requiredEnv.join(", ") })}
                  </p>
                ) : null}
              </div>
            );
          })}
        </section>
      ) : null}

      {detail.remotes.length > 0 ? (
        <section className="space-y-2">
          <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
            <Globe className="h-3.5 w-3.5" />
            {t("mcp.remoteEndpoints")}
          </h4>
          {detail.remotes.map((remote) => (
            <div key={remote.url} className="rounded-xl border border-border/50 bg-muted/30 px-3.5 py-2.5">
              <div className="flex items-start gap-2">
                <span className="mt-0.5 shrink-0 rounded bg-muted px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wider text-muted-foreground">
                  {remote.transportType ?? remote.transport}
                </span>
                <code className="min-w-0 flex-1 break-all pt-0.5 font-mono text-xs leading-relaxed text-foreground">
                  {remote.url}
                </code>
                <CopyCommandButton text={remote.url} />
              </div>
              {remote.requiredHeaders.length > 0 ? (
                <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                  {t("mcp.requiredHeaders", { keys: remote.requiredHeaders.join(", ") })}
                </p>
              ) : null}
            </div>
          ))}
        </section>
      ) : null}

      {showReadme ? (
        <section className="space-y-2">
          <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">README</h4>
          <div className="max-h-[40vh] overflow-y-auto rounded-xl border border-border/50 bg-card/30 p-3">
            <Markdown>{detail.readme ?? ""}</Markdown>
          </div>
        </section>
      ) : null}
    </div>
  );
}
