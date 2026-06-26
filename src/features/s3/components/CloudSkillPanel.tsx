import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Cloud, DownloadCloud, Loader2, RefreshCw, UploadCloud } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { useS3TargetsQuery } from "../api/targets";
import { useCloudManifestQuery, useInstallFromCloud, usePushToCloud } from "../api/sync";
import type { ManifestEntry, ManifestEntryView } from "../../../lib/ipc/commands/s3";

interface Props {
  /** Injected scope switch (Local / Remote / Cloud tabs) — rendered in the
   *  panel header so the user can hop scopes without leaving the page. */
  scopeSwitch: React.ReactNode;
}

/**
 * Cloud (S3) scope for My Skills. Shows a target picker + the cloud manifest as
 * a selectable card grid, with Push / Pull / Install-selected actions.
 *
 * Self-contained (owns its toolbar), mirroring `RemoteSkillsContent`. Deliberately
 * renders its own grid instead of reusing `SkillGrid` to avoid the my-skills ↔ ssh
 * barrel cycle the SSH feature documents.
 */
export function CloudSkillsContent({ scopeSwitch }: Props) {
  const { t } = useTranslation();
  const targetsQuery = useS3TargetsQuery();
  const targets = targetsQuery.data ?? [];

  const [selectedKey, setSelectedKey] = useState<string>("");
  const targetId = selectedKey || targets[0]?.id || null;

  const manifestQuery = useCloudManifestQuery(targetId);
  const push = usePushToCloud();
  const restore = useInstallFromCloud();

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");

  const entries = manifestQuery.data ?? [];
  const visible = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) => e.name.toLowerCase().includes(q) || e.description?.toLowerCase().includes(q));
  }, [entries, filter]);

  const toggle = (name: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  const toggleAll = () => {
    if (selected.size === visible.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(visible.map((e) => e.name)));
    }
  };

  const handlePush = () => {
    if (!targetId) return;
    push.mutate({ targetId });
  };

  const handleRestore = () => {
    if (!targetId || selected.size === 0) return;
    const toInstall: ManifestEntry[] = entries
      .filter((e) => selected.has(e.name) && !e.installed_locally)
      .map(viewToEntry);
    if (toInstall.length === 0) return;
    restore.mutate({ targetId, entries: toInstall }, { onSettled: () => setSelected(new Set()) });
  };

  // ── No targets configured ──
  if (targets.length === 0) {
    return (
      <Shell title={t("mySkills.scopeCloud")} scopeSwitch={scopeSwitch}>
        <div className="flex-1 flex items-center justify-center p-8">
          <div className="text-center max-w-sm">
            <Cloud className="w-10 h-10 text-muted-foreground/40 mx-auto mb-3" />
            <p className="text-sm text-muted-foreground">{t("s3.selectTarget")}</p>
            <p className="text-xs text-muted-foreground/70 mt-1">{t("s3.selectTargetHint")}</p>
          </div>
        </div>
      </Shell>
    );
  }

  return (
    <Shell title={t("mySkills.scopeCloud")} scopeSwitch={scopeSwitch}>
      {/* Toolbar: target picker + push/pull */}
      <div className="flex flex-wrap items-center gap-2 px-4 py-2.5 border-b border-border">
        <select
          value={targetId ?? ""}
          onChange={(e) => {
            setSelectedKey(e.target.value);
            setSelected(new Set());
          }}
          className="text-xs rounded-md border border-border bg-background px-2 py-1.5 max-w-[220px]"
        >
          {targets.map((tg) => (
            <option key={tg.id} value={tg.id}>
              {tg.display_name} · {tg.bucket}
            </option>
          ))}
        </select>

        <div className="flex-1" />

        <Button size="sm" variant="outline" onClick={handlePush} disabled={push.isPending}>
          {push.isPending ? (
            <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
          ) : (
            <UploadCloud className="w-4 h-4 mr-1.5" />
          )}
          {push.isPending ? t("s3.pushing") : t("s3.push")}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => manifestQuery.refetch()} disabled={manifestQuery.isFetching}>
          <RefreshCw className={`w-4 h-4 mr-1.5 ${manifestQuery.isFetching ? "animate-spin" : ""}`} />
          {manifestQuery.isFetching ? t("s3.pulling") : t("s3.pull")}
        </Button>
      </div>

      {/* Manifest grid */}
      {manifestQuery.isError ? (
        <div className="flex-1 flex items-center justify-center p-8 text-center">
          <p className="text-sm text-destructive">{t("s3.manifestError")}</p>
        </div>
      ) : entries.length === 0 ? (
        <div className="flex-1 flex items-center justify-center p-8 text-center">
          <div>
            <DownloadCloud className="w-10 h-10 text-muted-foreground/40 mx-auto mb-3" />
            <p className="text-sm text-muted-foreground">{t("s3.manifestEmpty")}</p>
          </div>
        </div>
      ) : (
        <>
          <div className="flex items-center gap-2 px-4 py-2 border-b border-border">
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder={t("s3.restoreDialog.searchPlaceholder")}
              className="text-xs rounded-md border border-border bg-background px-2 py-1.5 flex-1 max-w-xs"
            />
            <button
              type="button"
              onClick={toggleAll}
              className="text-xs text-muted-foreground hover:text-foreground px-2 py-1"
            >
              {t("s3.restoreDialog.selectAll")}
            </button>
            <span className="text-xs text-muted-foreground">
              {selected.size}/{visible.length}
            </span>
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            <ul className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2.5">
              {visible.map((entry) => (
                <li
                  key={entry.name}
                  className={`rounded-lg border p-3 cursor-pointer transition-colors ${
                    selected.has(entry.name)
                      ? "border-primary/50 bg-primary/5"
                      : "border-border hover:border-border/80 hover:bg-muted/20"
                  } ${entry.installed_locally ? "opacity-60" : ""}`}
                  onClick={() => !entry.installed_locally && toggle(entry.name)}
                >
                  <div className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      checked={selected.has(entry.name)}
                      disabled={entry.installed_locally}
                      onChange={() => toggle(entry.name)}
                      onClick={(e) => e.stopPropagation()}
                      className="mt-0.5"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="text-sm font-medium text-foreground truncate">{entry.name}</span>
                        {entry.installed_locally && (
                          <span className="text-[10px] px-1.5 py-0.5 rounded bg-emerald-500/15 text-emerald-500 shrink-0">
                            {t("s3.badgeInstalled")}
                          </span>
                        )}
                      </div>
                      {entry.description && (
                        <p className="text-xs text-muted-foreground line-clamp-2 mt-0.5">{entry.description}</p>
                      )}
                      <div className="flex items-center gap-1.5 mt-1.5">
                        <span
                          className={`text-[10px] px-1.5 py-0.5 rounded ${
                            entry.kind === "hub" ? "bg-sky-500/15 text-sky-400" : "bg-violet-500/15 text-violet-400"
                          }`}
                        >
                          {entry.kind === "hub" ? t("s3.badgeHub") : t("s3.badgeLocal")}
                        </span>
                        {entry.kind === "local" && entry.size_bytes != null && (
                          <span className="text-[10px] text-muted-foreground">{formatBytes(entry.size_bytes)}</span>
                        )}
                      </div>
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          </div>

          {/* Selection action bar */}
          {selected.size > 0 && (
            <div className="border-t border-border px-4 py-2.5 flex items-center justify-between bg-card">
              <span className="text-xs text-muted-foreground">
                {t("s3.restoreDialog.install", { count: selected.size })}
              </span>
              <Button size="sm" onClick={handleRestore} disabled={restore.isPending}>
                {restore.isPending ? (
                  <Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
                ) : (
                  <DownloadCloud className="w-4 h-4 mr-1.5" />
                )}
                {t("s3.restore")}
              </Button>
            </div>
          )}
        </>
      )}
    </Shell>
  );
}

function Shell({
  title,
  scopeSwitch,
  children,
}: {
  title: string;
  scopeSwitch: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-4 py-2 border-b border-border">
        <h1 className="text-sm font-semibold text-foreground">{title}</h1>
        {scopeSwitch}
      </div>
      {children}
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Convert a flat manifest view back into the tagged `ManifestEntry` the backend
 *  install command expects. */
function viewToEntry(v: ManifestEntryView): ManifestEntry {
  if (v.kind === "hub") {
    return {
      kind: "hub",
      name: v.name,
      git_url: v.git_url ?? "",
      source_folder: v.source_folder,
      tree_hash: v.tree_hash,
      description: v.description ?? "",
    };
  }
  return {
    kind: "local",
    name: v.name,
    tarball_key: v.tarball_key ?? "",
    sha256: v.sha256 ?? "",
    size_bytes: v.size_bytes ?? 0,
    description: v.description ?? "",
    uploaded_at: v.uploaded_at ?? "",
  };
}
