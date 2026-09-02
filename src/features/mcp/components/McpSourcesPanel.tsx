import { Database, FolderOpen, Globe, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import { InsetPanel } from "../../../components/ui/InsetPanel";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import { StatusChip, type StatusChipTone } from "../../../components/ui/StatusChip";
import { Switch } from "../../../components/ui/switch";
import { cn } from "../../../lib/utils";
import { toast } from "../../../lib/toast";
import type { McpCustomSource, McpSourceKind } from "../../../types";
import { useMcpSources } from "../hooks/useMcpSources";
import type { McpSourceStatus } from "../lib/sourceHealth";

/**
 * Catalog source management.
 *
 * A source is data, not code: the merged catalog is whatever the enabled
 * sources produced, so switching one off or adding a private registry has to be
 * a first-class user action rather than a rebuild. Two things are shown next to
 * every row because they are not interchangeable: the **licence** decides
 * whether SkillStar may keep a long-lived local mirror of that source at all,
 * and the **freshness** says whether the rows currently in the catalog can be
 * trusted to be complete.
 *
 * Built-in sources can be disabled but not removed — their ids are part of the
 * merge-priority contract, and user sources are namespaced `custom:` so they
 * can never shadow one.
 */

function slugify(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

const HEALTH_TONE: Record<McpSourceStatus["health"], StatusChipTone> = {
  fresh: "success",
  degraded: "warning",
  stale: "muted",
  error: "danger",
  never: "muted",
};

function AddSourceForm({ onAdd, busy }: { onAdd: (source: McpCustomSource) => Promise<unknown>; busy: boolean }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [displayName, setDisplayName] = useState("");
  const [target, setTarget] = useState("");
  const [kind, setKind] = useState<McpSourceKind>("registry");

  const reset = () => {
    setDisplayName("");
    setTarget("");
    setKind("registry");
    setOpen(false);
  };

  const submit = async () => {
    const id = slugify(displayName || target);
    if (!id || !target.trim()) {
      toast.error(t("mcp.sourceAddInvalid"));
      return;
    }
    try {
      await onAdd({
        id,
        displayName: displayName.trim() || id,
        target: target.trim(),
        kind,
        // The official registry's spelling; a source that uses the other one
        // reports it on first sync and can be re-added. Guessing per-URL here
        // would be a second, worse copy of the backend's source table.
        cursorStyle: "camel",
        enabled: true,
        priorityOffset: 0,
      });
      toast.success(t("mcp.sourceAdded"));
      reset();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    }
  };

  if (!open) {
    return (
      <Button type="button" variant="outline" size="sm" className="gap-1.5" onClick={() => setOpen(true)}>
        <Plus className="h-3.5 w-3.5" />
        {t("mcp.sourceAdd")}
      </Button>
    );
  }

  return (
    <InsetPanel className="p-3">
      <div className="flex gap-2">
        {(["registry", "localDirectory"] as const).map((option) => (
          <button
            key={option}
            type="button"
            onClick={() => setKind(option)}
            className={cn(
              "flex-1 rounded-lg border px-3 py-1.5 text-xs font-medium transition",
              kind === option
                ? "border-primary/60 bg-primary/10 text-primary"
                : "border-border bg-background/40 text-muted-foreground hover:bg-muted/40",
            )}
          >
            {t(`mcp.sourceKind_${option}`)}
          </button>
        ))}
      </div>
      <Input
        value={displayName}
        onChange={(event) => setDisplayName(event.target.value)}
        placeholder={t("mcp.sourceNamePlaceholder")}
        className="h-9 text-xs"
      />
      <Input
        value={target}
        onChange={(event) => setTarget(event.target.value)}
        placeholder={kind === "registry" ? "https://registry.example.com/v0.1/servers" : "/path/to/servers.json"}
        className="h-9 font-mono text-xs"
      />
      <p className="text-[11px] leading-relaxed text-muted-foreground">{t("mcp.sourceAddHint")}</p>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={reset} disabled={busy}>
          {t("common.cancel")}
        </Button>
        <Button type="button" size="sm" onClick={() => void submit()} disabled={busy}>
          {t("common.add")}
        </Button>
      </div>
    </InsetPanel>
  );
}

interface McpSourcesPanelProps {
  className?: string;
}

export function McpSourcesPanel({ className }: McpSourcesPanelProps) {
  const { t } = useTranslation();
  const { sources, statuses, isLoading, addSource, removeSource, setSourceEnabled, mutating } = useMcpSources();
  const statusById = new Map(statuses.map((status) => [status.sourceId ?? status.scope, status]));

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-16">
        <LoadingLogo size="md" label={t("mcp.sourcesLoading")} />
      </div>
    );
  }

  return (
    <section className={cn("space-y-3", className)}>
      <div className="flex items-center gap-2 px-1">
        <Database className="h-3.5 w-3.5 text-primary" />
        <h2 className="text-sm font-semibold text-foreground">{t("mcp.sourcesTitle")}</h2>
        <span className="text-xs text-muted-foreground">{t("mcp.sourcesCount", { count: sources.length })}</span>
        <div className="ml-auto">
          <AddSourceForm onAdd={addSource} busy={mutating} />
        </div>
      </div>

      <p className="px-1 text-[11px] leading-relaxed text-muted-foreground">{t("mcp.sourcesIntro")}</p>

      <ul className="space-y-2">
        {sources.map((source) => {
          const status = statusById.get(source.id);
          const Icon = source.kind === "localDirectory" ? FolderOpen : Globe;
          return (
            <li
              key={source.id}
              className={cn(
                "rounded-xl border px-3 py-2.5",
                source.enabled ? "border-border/70 bg-background/50" : "border-border/40 bg-background/25 opacity-75",
              )}
            >
              <div className="flex items-center gap-2">
                <Icon className="h-3.5 w-3.5 shrink-0 text-primary" />
                <span className="text-xs font-medium text-foreground">{source.displayName}</span>
                {source.builtin ? (
                  <span className="rounded bg-muted px-1.5 text-micro text-muted-foreground">
                    {t("mcp.sourceBuiltin")}
                  </span>
                ) : null}
                {status ? (
                  <StatusChip size="sm" tone={HEALTH_TONE[status.health]}>
                    {t(`mcp.sourceHealth_${status.health}`)}
                  </StatusChip>
                ) : null}
                <div className="ml-auto flex items-center gap-2">
                  <Switch
                    size="sm"
                    checked={source.enabled}
                    onCheckedChange={(next) => void setSourceEnabled(source.id, next)}
                    disabled={mutating}
                    aria-label={t("mcp.sourceToggle", { name: source.displayName })}
                  />
                  {source.builtin ? null : (
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="h-7 w-7 p-0 text-muted-foreground hover:text-destructive"
                      onClick={() => void removeSource(source.id)}
                      disabled={mutating}
                      title={t("common.delete")}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  )}
                </div>
              </div>

              <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">{source.baseUrl}</p>

              <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                <span>{t(`mcp.sourceLicense_${source.license}`)}</span>
                <span className={source.mirrorable ? "" : "text-amber-600 dark:text-amber-400"}>
                  {source.mirrorable ? t("mcp.sourceMirrorable") : t("mcp.sourceNotMirrorable")}
                </span>
                {source.requiresKey ? (
                  <span className="text-amber-600 dark:text-amber-400">{t("mcp.sourceRequiresKey")}</span>
                ) : null}
                <span className="tabular-nums">{t("mcp.sourcePriority", { priority: source.priority })}</span>
              </div>

              {status?.degradedReason ? (
                <p className="mt-1.5 break-all text-[11px] leading-relaxed text-amber-600 dark:text-amber-400">
                  {status.degradedReason}
                </p>
              ) : null}
              {status?.lastError ? (
                <p className="mt-1.5 break-all font-mono text-[11px] leading-relaxed text-destructive">
                  {status.lastError}
                </p>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
