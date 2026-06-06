import { useQuery } from "@tanstack/react-query";
import { Download, ExternalLink, Globe, RefreshCw, Search, Star, Terminal } from "lucide-react";
import { useState } from "react";
import { Button } from "../../../components/ui/button";
import { ExternalAnchor } from "../../../components/ui/ExternalAnchor";
import { Markdown } from "../../../components/ui/Markdown";
import { tauriInvoke } from "../../../lib/ipc";
import type { LocalFirstResult, McpMarketServerDetail } from "../../../types";
import { ProviderDrawer } from "../../models/components/hub/ProviderDrawer";
import { useMcpMarketplace } from "../hooks/useMcpMarketplace";
import { McpMarketCard } from "./McpMarketCard";

function statusHint(status: string | undefined, updatedAt: string | null): string {
  switch (status) {
    case "seeding":
      return "首次从 GitHub MCP Registry 加载…";
    case "stale":
      return "正在后台更新…";
    case "remote_error":
      return "加载失败，请检查网络后重试";
    case "miss":
      return "暂无数据";
    default:
      return updatedAt ? `已更新 · ${new Date(updatedAt).toLocaleString()}` : "本地优先 · GitHub MCP Registry";
  }
}

interface McpMarketBrowserProps {
  /** Names of already-installed MCP servers, for the "已安装" badge. */
  installedNames: Set<string>;
  /** Open the create form prefilled from this marketplace entry id. */
  onInstall: (id: string) => void;
}

export function McpMarketBrowser({ installedNames, onInstall }: McpMarketBrowserProps) {
  const { entries, status, updatedAt, isLoading, query, setQuery, refresh, refreshing } = useMcpMarketplace();
  const [detailId, setDetailId] = useState<string | null>(null);

  const detailQuery = useQuery<LocalFirstResult<McpMarketServerDetail | null>>({
    queryKey: ["mcp-market", "detail", detailId],
    queryFn: () => tauriInvoke("get_mcp_market_server_detail_local", { id: detailId as string }),
    enabled: detailId != null,
  });
  const detail = detailQuery.data?.data ?? null;

  return (
    <section>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground/60" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索 MCP 服务器（名称 / 描述）"
            className="h-9 w-full rounded-lg border border-border/60 bg-card/50 pl-8 pr-3 text-sm text-foreground placeholder:text-muted-foreground/60 focus:border-primary/40 focus:outline-none"
          />
        </div>
        <Button variant="outline" size="sm" onClick={() => refresh()} disabled={refreshing} className="gap-1.5">
          <RefreshCw className={refreshing ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
          刷新
        </Button>
      </div>

      <p className="mb-3 text-[11px] text-muted-foreground/70">{statusHint(status, updatedAt)}</p>

      {isLoading || status === "seeding" ? (
        <div className="rounded-xl border border-border/55 bg-card/40 px-6 py-10 text-center text-sm text-muted-foreground">
          加载中…
        </div>
      ) : entries.length === 0 ? (
        <div className="rounded-xl border border-dashed border-border/60 bg-card/50 px-8 py-12 text-center">
          <p className="text-sm text-muted-foreground">{query ? "没有匹配的 MCP 服务器" : "暂无可浏览的 MCP 服务器"}</p>
          {status === "remote_error" ? (
            <Button variant="outline" onClick={() => refresh()} className="mt-4 gap-1.5">
              <RefreshCw className="h-4 w-4" />
              重试
            </Button>
          ) : null}
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {entries.map((entry) => (
            <McpMarketCard
              key={entry.id}
              entry={entry}
              installed={installedNames.has(entry.name)}
              onInstall={() => onInstall(entry.id)}
              onOpenDetail={() => setDetailId(entry.id)}
            />
          ))}
        </div>
      )}

      <ProviderDrawer
        open={detailId != null}
        onOpenChange={(open) => {
          if (!open) setDetailId(null);
        }}
        title={<span className="text-foreground">{detail?.name ?? "MCP 服务器"}</span>}
        subtitle={detail?.namespace}
      >
        {detailQuery.isLoading ? (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">加载中…</div>
        ) : detail ? (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
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
                  仓库
                </ExternalAnchor>
              ) : null}
            </div>

            {detail.description ? <p className="text-sm text-muted-foreground">{detail.description}</p> : null}

            <Button
              onClick={() => {
                const id = detail.id;
                setDetailId(null);
                onInstall(id);
              }}
              disabled={installedNames.has(detail.name)}
              className="w-full gap-1.5"
            >
              <Download className="h-4 w-4" />
              {installedNames.has(detail.name) ? "已安装" : "安装到工具…"}
            </Button>

            {detail.packages.length > 0 ? (
              <div className="space-y-2">
                <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Terminal className="h-3.5 w-3.5" />
                  本地运行 (stdio)
                </h4>
                {detail.packages.map((pkg) => (
                  <div
                    key={`${pkg.runtime}-${pkg.identifier}`}
                    className="rounded-lg border border-border/50 bg-card/40 p-3 text-xs"
                  >
                    <code className="font-mono text-foreground">
                      {pkg.runtime} {pkg.identifier}
                      {pkg.version ? `@${pkg.version}` : ""}
                    </code>
                    {pkg.requiredEnv.length > 0 ? (
                      <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                        需填写：{pkg.requiredEnv.join(", ")}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}

            {detail.remotes.length > 0 ? (
              <div className="space-y-2">
                <h4 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                  <Globe className="h-3.5 w-3.5" />
                  远程端点 (http / sse)
                </h4>
                {detail.remotes.map((remote) => (
                  <div key={remote.url} className="rounded-lg border border-border/50 bg-card/40 p-3 text-xs">
                    <code className="break-all font-mono text-foreground">
                      [{remote.transport}] {remote.url}
                    </code>
                    {remote.requiredHeaders.length > 0 ? (
                      <p className="mt-1.5 text-[11px] text-amber-600 dark:text-amber-400">
                        需填写请求头：{remote.requiredHeaders.join(", ")}
                      </p>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : null}

            {detail.readme ? (
              <div className="space-y-2">
                <h4 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">README</h4>
                <div className="max-h-[40vh] overflow-y-auto rounded-lg border border-border/50 bg-card/30 p-3">
                  <Markdown>{detail.readme}</Markdown>
                </div>
              </div>
            ) : null}
          </div>
        ) : (
          <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">未找到该服务器</div>
        )}
      </ProviderDrawer>
    </section>
  );
}
