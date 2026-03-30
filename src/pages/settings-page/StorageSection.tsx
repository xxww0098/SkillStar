import { useTranslation } from "react-i18next";
import { motion, AnimatePresence } from "framer-motion";
import { HardDrive, Database, FolderGit2, FolderPlus, History, Loader2, Trash2, Globe, Stethoscope, Wrench } from "lucide-react";
import { Button } from "../../components/ui/button";

export interface StorageOverview {
  config_bytes: number;
  hub_bytes: number;
  hub_count: number;
  broken_count: number;
  local_count: number;
  local_bytes: number;
  cache_bytes: number;
  cache_count: number;
  cache_unused_count: number;
  cache_unused_bytes: number;
  history_count: number;
}

interface StorageSectionProps {
  overview: StorageOverview | null;
  loading: boolean;
  cleaning: boolean;
  forceDeletingTarget: "hub" | "cache" | "config" | null;
  cleaningBroken: boolean;
  formatBytes: (bytes: number) => string;
  onCleanAll: () => void;
  onForceDeleteHub: () => void;
  onForceDeleteCache: () => void;
  onCleanBroken: () => void;
}

export function StorageSection({
  overview,
  loading,
  cleaning,
  cleaningBroken,
  forceDeletingTarget,
  formatBytes,
  onCleanAll,
  onForceDeleteHub,
  onForceDeleteCache,
  onCleanBroken,
}: StorageSectionProps) {
  const { t } = useTranslation();

  const totalBytes = overview
    ? overview.config_bytes + overview.hub_bytes + overview.local_bytes + overview.cache_bytes
    : 0;

  const hasCleanable = overview
    ? overview.cache_unused_count > 0 || overview.history_count > 0
    : false;

  const hasBroken = overview ? overview.broken_count > 0 : false;

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-red-500/10 flex items-center justify-center shrink-0 border border-red-500/20">
          <HardDrive className="w-4 h-4 text-red-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.storage")}</h2>
      </div>
      <div className="rounded-xl border border-border bg-card overflow-hidden">
        {/* Total */}
        <div className="px-4 py-3 flex items-center justify-between border-b border-border/50 bg-muted/20">
          <div className="flex items-center gap-3">
            <div>
              <div className="text-sm font-medium text-foreground">{t("settings.storageTotal")}</div>
              {overview && !loading && (
                <div className="text-xs text-muted-foreground mt-0.5 font-mono">
                  {formatBytes(totalBytes)}
                </div>
              )}
            </div>
          </div>
          <Button
            size="sm"
            variant="outline"
            onClick={onCleanAll}
            disabled={cleaning || !hasCleanable}
            className="relative overflow-hidden shrink-0 text-destructive hover:bg-destructive/10 hover:text-destructive hover:border-destructive/30"
          >
            <AnimatePresence>
              {cleaning && (
                <motion.div
                  className="absolute inset-0 bg-gradient-to-r from-transparent via-destructive/20 to-transparent pointer-events-none"
                  initial={{ x: "-100%" }}
                  animate={{ x: "100%" }}
                  transition={{ repeat: Infinity, duration: 1, ease: "linear" }}
                />
              )}
            </AnimatePresence>
            {cleaning ? (
              <>
                <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin relative z-10" />
                <span className="relative z-10">{t("settings.cleaning")}</span>
              </>
            ) : (
              <>
                <Trash2 className="w-3.5 h-3.5 mr-1.5" />
                {t("settings.cleanAllCaches")}
              </>
            )}
          </Button>
        </div>

        {/* Breakdown rows */}
        {overview && !loading && (
          <div className="divide-y divide-border/30">
            {/* Skill Health */}
            {hasBroken && (
              <StorageRow
                icon={<Stethoscope className="w-3.5 h-3.5 text-amber-400" />}
                label={t("settings.skillHealth")}
                detail={t("settings.healthIssues", { count: overview.broken_count })}
                highlight
                action={
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={onCleanBroken}
                    disabled={cleaningBroken || forceDeletingTarget !== null}
                    className="h-7 px-2.5 text-[11px] text-amber-500 hover:bg-amber-500/10 hover:text-amber-400 hover:border-amber-500/30"
                  >
                    {cleaningBroken ? (
                      <>
                        <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                        {t("settings.repairing")}
                      </>
                    ) : (
                      <>
                        <Wrench className="w-3 h-3 mr-1" />
                        {t("settings.repairAll")}
                      </>
                    )}
                  </Button>
                }
              />
            )}

            {/* Skills Hub */}
            <StorageRow
              icon={<Database className="w-3.5 h-3.5 text-blue-400" />}
              label={t("settings.storageHub")}
              detail={
                hasBroken
                  ? `${formatBytes(overview.hub_bytes)} · ${t("settings.storageHubCount", { count: overview.hub_count })} (${t("settings.healthBroken", { count: overview.broken_count })})`
                  : `${formatBytes(overview.hub_bytes)} · ${t("settings.storageHubCount", { count: overview.hub_count })}`
              }
              highlight={hasBroken}
              action={
                <Button
                  size="sm"
                  variant="outline"
                  onClick={onForceDeleteHub}
                  disabled={forceDeletingTarget !== null}
                  className="h-7 px-2.5 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive hover:border-destructive/30"
                >
                  {forceDeletingTarget === "hub" ? (
                    <>
                      <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                      {t("settings.forceDeleting")}
                    </>
                  ) : (
                    t("settings.forceDelete")
                  )}
                </Button>
              }
            />

            {/* Local Skills */}
            {overview.local_count > 0 && (
              <StorageRow
                icon={<FolderPlus className="w-3.5 h-3.5 text-indigo-400" />}
                label={t("settings.storageLocal")}
                detail={`${formatBytes(overview.local_bytes)} · ${t("settings.storageLocalCount", { count: overview.local_count })}`}
              />
            )}

            {/* Repo Cache */}
            <StorageRow
              icon={<FolderGit2 className="w-3.5 h-3.5 text-amber-400" />}
              label={t("settings.repoCache")}
              detail={
                overview.cache_count > 0
                  ? `${formatBytes(overview.cache_bytes)} · ${t("settings.cacheRepos", { count: overview.cache_count })}` +
                    (overview.cache_unused_count > 0
                      ? ` · ${t("settings.cacheUnused", { count: overview.cache_unused_count })} (${formatBytes(overview.cache_unused_bytes)})`
                      : "")
                  : t("settings.storageEmpty")
              }
              highlight={overview.cache_unused_count > 0}
              action={
                <Button
                  size="sm"
                  variant="outline"
                  onClick={onForceDeleteCache}
                  disabled={forceDeletingTarget !== null}
                  className="h-7 px-2.5 text-[11px] text-destructive hover:bg-destructive/10 hover:text-destructive hover:border-destructive/30"
                >
                  {forceDeletingTarget === "cache" ? (
                    <>
                      <Loader2 className="w-3 h-3 mr-1 animate-spin" />
                      {t("settings.forceDeleting")}
                    </>
                  ) : (
                    t("settings.forceDelete")
                  )}
                </Button>
              }
              cleaning={cleaning}
            />

            {/* App Config */}
            <StorageRow
              icon={<Globe className="w-3.5 h-3.5 text-purple-400" />}
              label={t("settings.storageConfig")}
              detail={formatBytes(overview.config_bytes)}
            />

            {/* Repo History */}
            <StorageRow
              icon={<History className="w-3.5 h-3.5 text-emerald-400" />}
              label={t("settings.storageHistory")}
              detail={
                overview.history_count > 0
                  ? t("settings.storageHistoryCount", { count: overview.history_count })
                  : t("settings.storageEmpty")
              }
              highlight={overview.history_count > 0}
              cleaning={cleaning}
            />
          </div>
        )}

        {loading && (
          <div className="px-4 py-3 text-xs text-muted-foreground flex items-center gap-2">
            <Loader2 className="w-3 h-3 animate-spin" />
            {t("common.loading")}
          </div>
        )}
      </div>
    </section>
  );
}

function StorageRow({
  icon,
  label,
  detail,
  highlight,
  action,
  cleaning,
}: {
  icon: React.ReactNode;
  label: string;
  detail: string;
  highlight?: boolean;
  action?: React.ReactNode;
  cleaning?: boolean;
}) {
  return (
    <div className="px-4 py-2.5 flex items-center gap-3">
      <div className="w-6 h-6 rounded-md bg-muted/50 flex shrink-0 items-center justify-center">
        {icon}
      </div>
      <div className="min-w-0 flex-1">
        <div className="text-xs font-medium text-foreground/80">{label}</div>
        <div 
          className={`text-[11px] mt-0.5 transition-opacity ${highlight ? "text-amber-400/80" : "text-muted-foreground"} ${highlight && cleaning ? "animate-pulse" : ""}`}
        >
          {detail}
        </div>
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}
