import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertTriangle,
  ChevronDown,
  Database,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  Globe,
  HardDrive,
  History,
  Loader2,
  Stethoscope,
  Trash2,
  Wrench,
  X,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { toast } from "../../../lib/toast";
import type { StorageOverview } from "../../../types";

interface StorageSectionProps {
  overview: StorageOverview | null;
  loading: boolean;
  cleaning: boolean;
  forceDeletingTarget: "hub" | "cache" | "config" | null;
  slowForceDeletingTarget: "hub" | "cache" | "config" | null;
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
  slowForceDeletingTarget,
  formatBytes,
  onCleanAll,
  onForceDeleteHub,
  onForceDeleteCache,
  onCleanBroken,
}: StorageSectionProps) {
  const { t } = useTranslation();
  const [pathStructureOpen, setPathStructureOpen] = useState(false);

  const handleOpenFolder = async (path: string) => {
    try {
      await invoke("open_folder", { path });
    } catch (error) {
      console.error("Failed to open folder:", error);
      toast.error(t("settings.openFolderFailed"));
    }
  };

  const totalBytes = overview
    ? overview.config_bytes + overview.hub_bytes + overview.local_bytes + overview.cache_bytes
    : 0;

  const hasBroken = overview ? overview.broken_count > 0 : false;
  const hubRelativePath = overview ? getRelativePathFromParent(overview.data_root_path, overview.hub_root_path) : null;

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-red-500/10 flex items-center justify-center shrink-0 border border-red-500/20">
          <HardDrive className="w-4 h-4 text-red-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.storage")}</h2>
      </div>
      <div className="mb-2 px-1 flex items-start justify-between gap-3">
        <p className="text-micro leading-relaxed text-muted-foreground/75">{t("settings.storagePathModelHint")}</p>
        <button
          type="button"
          onClick={() => setPathStructureOpen((prev) => !prev)}
          className="shrink-0 inline-flex items-center gap-1 text-micro text-muted-foreground hover:text-foreground transition-colors"
          aria-expanded={pathStructureOpen}
          aria-controls="storage-path-structure"
        >
          <span>{pathStructureOpen ? t("common.hide") : t("settings.viewPathStructure")}</span>
          <ChevronDown className={`w-3.5 h-3.5 transition-transform ${pathStructureOpen ? "rotate-180" : ""}`} />
        </button>
      </div>

      <AnimatePresence initial={false}>
        {pathStructureOpen && overview && !loading && (
          <motion.div
            id="storage-path-structure"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.18 }}
            className="overflow-hidden"
          >
            <div className="mb-3 rounded-xl border border-border/70 bg-card/70 p-3">
              <div className="grid gap-2 sm:grid-cols-2">
                <RootPathCard
                  title={t("settings.storageRootData")}
                  sourceTag={t("settings.storageSourceData")}
                  path={overview.data_root_path}
                  includes={[t("settings.storageConfig"), t("settings.storageHistory")]}
                />
                <RootPathCard
                  title={t("settings.storageRootHub")}
                  sourceTag={t("settings.storageSourceHub")}
                  path={overview.hub_root_path}
                  includes={[t("settings.storageHub"), t("settings.storageLocal"), t("settings.repoCache")]}
                />
              </div>
              <div className="mt-2 text-micro text-muted-foreground">
                {overview.is_hub_under_data
                  ? t("settings.storagePathRelationNested", { relative: hubRelativePath ?? ".agents" })
                  : t("settings.storagePathRelationIndependent")}
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="rounded-xl border border-border bg-card overflow-hidden">
        {/* Total */}
        <div className="px-4 py-3 flex items-center justify-between border-b border-border/50 bg-muted/20">
          <div className="flex items-center gap-3">
            <div>
              <div className="text-sm font-medium text-foreground">{t("settings.storageTotal")}</div>
              {overview && !loading && (
                <div className="text-xs text-muted-foreground mt-0.5 font-mono">{formatBytes(totalBytes)}</div>
              )}
            </div>
          </div>
          <Button
            size="sm"
            variant="outline"
            onClick={onCleanAll}
            disabled={cleaning || loading}
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
                sourceTag="hub"
                detail={t("settings.healthIssues", { count: overview.broken_count })}
                highlight
                action={
                  <div className="flex items-center gap-2">
                    <OpenFolderButton
                      onClick={() => void handleOpenFolder(overview.hub_path)}
                      label={t("settings.openFolderFor", { target: t("settings.storageHub") })}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={onCleanBroken}
                      disabled={cleaningBroken || forceDeletingTarget !== null}
                      className="h-7 px-2.5 text-micro text-amber-500 hover:bg-amber-500/10 hover:text-amber-400 hover:border-amber-500/30"
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
                  </div>
                }
              />
            )}

            {/* Skills Hub */}
            <StorageRow
              icon={<Database className="w-3.5 h-3.5 text-blue-400" />}
              label={t("settings.storageHub")}
              sourceTag="hub"
              detail={
                hasBroken
                  ? `${formatBytes(overview.hub_bytes)} · ${t("settings.storageHubCount", { count: overview.hub_count })} (${t("settings.healthBroken", { count: overview.broken_count })})`
                  : `${formatBytes(overview.hub_bytes)} · ${t("settings.storageHubCount", { count: overview.hub_count })}`
              }
              highlight={hasBroken}
              action={
                <div className="flex items-center gap-2">
                  <ForceDeleteButton
                    onClick={onForceDeleteHub}
                    disabled={forceDeletingTarget !== null}
                    isDeleting={forceDeletingTarget === "hub"}
                    isSlow={slowForceDeletingTarget === "hub"}
                    label={t("settings.forceDelete")}
                    confirmMsg={t("settings.confirmForceDelete")}
                  />
                  <OpenFolderButton
                    onClick={() => void handleOpenFolder(overview.hub_path)}
                    label={t("settings.openFolderFor", { target: t("settings.storageHub") })}
                  />
                </div>
              }
            />

            {/* Repo Cache */}
            <StorageRow
              icon={<FolderGit2 className="w-3.5 h-3.5 text-amber-400" />}
              label={t("settings.repoCache")}
              sourceTag="hub"
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
                <div className="flex items-center gap-2">
                  <ForceDeleteButton
                    onClick={onForceDeleteCache}
                    disabled={forceDeletingTarget !== null}
                    isDeleting={forceDeletingTarget === "cache"}
                    isSlow={slowForceDeletingTarget === "cache"}
                    label={t("settings.forceDelete")}
                    confirmMsg={t("settings.confirmForceDelete")}
                  />
                  <OpenFolderButton
                    onClick={() => void handleOpenFolder(overview.cache_path)}
                    label={t("settings.openFolderFor", { target: t("settings.repoCache") })}
                  />
                </div>
              }
              cleaning={cleaning}
            />

            {/* Local Skills */}
            <StorageRow
              icon={<FolderPlus className="w-3.5 h-3.5 text-indigo-400" />}
              label={t("settings.storageLocal")}
              sourceTag="hub"
              detail={
                overview.local_count > 0
                  ? `${formatBytes(overview.local_bytes)} · ${t("settings.storageLocalCount", { count: overview.local_count })}`
                  : t("settings.storageEmpty")
              }
              action={
                <OpenFolderButton
                  onClick={() => void handleOpenFolder(overview.local_path)}
                  label={t("settings.openFolderFor", { target: t("settings.storageLocal") })}
                />
              }
            />

            {/* App Config */}
            <StorageRow
              icon={<Globe className="w-3.5 h-3.5 text-purple-400" />}
              label={t("settings.storageConfig")}
              sourceTag="data"
              detail={formatBytes(overview.config_bytes)}
              action={
                <OpenFolderButton
                  onClick={() => void handleOpenFolder(overview.config_path)}
                  label={t("settings.openFolderFor", { target: t("settings.storageConfig") })}
                />
              }
            />

            {/* Repo History */}
            <StorageRow
              icon={<History className="w-3.5 h-3.5 text-emerald-400" />}
              label={t("settings.storageHistory")}
              sourceTag="data"
              detail={
                overview.history_count > 0
                  ? t("settings.storageHistoryCount", { count: overview.history_count })
                  : t("settings.storageEmpty")
              }
              highlight={overview.history_count > 0}
              action={
                <OpenFolderButton
                  onClick={() => void handleOpenFolder(overview.config_path)}
                  label={t("settings.openFolderFor", { target: t("settings.storageHistory") })}
                />
              }
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

function OpenFolderButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <Button
      size="icon"
      variant="ghost"
      onClick={onClick}
      className="h-7 w-7 text-muted-foreground hover:text-foreground"
      title={label}
    >
      <FolderOpen className="w-4 h-4" />
    </Button>
  );
}

function StorageRow({
  icon,
  label,
  sourceTag,
  detail,
  highlight,
  action,
  cleaning,
}: {
  icon: React.ReactNode;
  label: string;
  sourceTag?: "data" | "hub";
  detail: string;
  highlight?: boolean;
  action?: React.ReactNode;
  cleaning?: boolean;
}) {
  const { t } = useTranslation();

  return (
    <div className="px-4 py-2.5 flex items-center gap-3">
      <div className="w-6 h-6 rounded-md bg-muted/50 flex shrink-0 items-center justify-center">{icon}</div>
      <div className="min-w-0 flex-1">
        <div className="text-xs font-medium text-foreground/80 flex items-center gap-1.5">
          <span>{label}</span>
          {sourceTag && (
            <span className="px-1.5 py-0.5 rounded border border-border/60 bg-muted/40 text-[10px] uppercase tracking-wide text-muted-foreground/80">
              {sourceTag === "data" ? t("settings.storageSourceData") : t("settings.storageSourceHub")}
            </span>
          )}
        </div>
        <div
          className={`text-micro mt-0.5 transition-opacity ${highlight ? "text-amber-400/80" : "text-muted-foreground"} ${highlight && cleaning ? "animate-pulse" : ""}`}
        >
          {detail}
        </div>
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  );
}

function RootPathCard({
  title,
  sourceTag,
  path,
  includes,
}: {
  title: string;
  sourceTag: string;
  path: string;
  includes: string[];
}) {
  const { t } = useTranslation();

  return (
    <div className="rounded-lg border border-border/60 bg-muted/20 p-2.5 min-w-0">
      <div className="flex items-center gap-1.5 text-xs font-medium text-foreground/85">
        <span>{title}</span>
        <span className="px-1.5 py-0.5 rounded border border-border/60 bg-muted/30 text-[10px] uppercase tracking-wide text-muted-foreground/80">
          {sourceTag}
        </span>
      </div>
      <div className="mt-1 text-micro text-foreground/80 font-mono break-all">{path}</div>
      <div className="mt-1 text-micro text-muted-foreground">
        {t("settings.storagePathContains")}: {includes.join(" · ")}
      </div>
    </div>
  );
}

function getRelativePathFromParent(parentPath: string, childPath: string): string | null {
  const normalize = (path: string) => path.replace(/\\/g, "/").split("/").filter(Boolean);

  const parentParts = normalize(parentPath);
  const childParts = normalize(childPath);

  if (childParts.length < parentParts.length) {
    return null;
  }

  for (let i = 0; i < parentParts.length; i += 1) {
    if (parentParts[i].toLowerCase() !== childParts[i].toLowerCase()) {
      return null;
    }
  }

  const relative = childParts.slice(parentParts.length).join("/");
  return relative || ".";
}

function ForceDeleteButton({
  onClick,
  disabled,
  isDeleting,
  isSlow,
  label,
  confirmMsg,
}: {
  onClick: () => void;
  disabled: boolean;
  isDeleting: boolean;
  isSlow: boolean;
  label: string;
  confirmMsg: string;
}) {
  const [showConfirm, setShowConfirm] = useState(false);
  const prefersReducedMotion = useReducedMotion();
  const { t } = useTranslation();

  return (
    <>
      <div className="flex items-center gap-2">
        <Button
          size="icon"
          variant="ghost"
          onClick={() => setShowConfirm(true)}
          disabled={disabled || isDeleting}
          className={`h-7 w-7 text-muted-foreground hover:text-destructive ${isDeleting ? "animate-pulse text-destructive" : "hover:bg-destructive/10"}`}
          title={label}
        >
          {isDeleting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Trash2 className="w-4 h-4" />}
        </Button>
        {isDeleting && isSlow ? (
          <span className="text-micro text-amber-400/90 whitespace-nowrap">{t("settings.forceDeleteSlowHint")}</span>
        ) : null}
      </div>

      <AnimatePresence>
        {showConfirm && (
          <>
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: prefersReducedMotion ? 0.01 : 0.16 }}
              className="fixed inset-0 z-[100] bg-black/40 backdrop-blur-sm"
              onClick={() => setShowConfirm(false)}
            />

            <motion.div
              initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.96, y: 16 }}
              animate={prefersReducedMotion ? { opacity: 1 } : { opacity: 1, scale: 1, y: 0 }}
              exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, scale: 0.98, y: 12 }}
              transition={{
                duration: prefersReducedMotion ? 0.01 : 0.24,
                ease: [0.22, 1, 0.36, 1],
              }}
              className="fixed left-1/2 top-1/2 w-full max-w-md -translate-x-1/2 -translate-y-1/2 z-[100] px-4"
            >
              <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5">
                <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-destructive/20 blur-[60px] opacity-70" />
                <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-accent/10 blur-[60px] opacity-70" />
                <div className="relative z-10">
                  <div className="flex items-start justify-between gap-4 px-6 pt-5">
                    <div className="flex items-start gap-3">
                      <motion.div
                        animate={prefersReducedMotion ? undefined : { scale: [1, 1.04, 1], rotate: [0, -3, 0] }}
                        transition={{ duration: 0.42, ease: [0.22, 1, 0.36, 1] }}
                        className="mt-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl bg-destructive/10 text-destructive"
                      >
                        <AlertTriangle className="h-5 w-5" />
                      </motion.div>
                      <div className="space-y-1">
                        <h2 className="text-heading-sm">{label}</h2>
                        <p className="text-caption leading-5 pr-4 mt-2 mb-4 text-muted-foreground">{confirmMsg}</p>
                      </div>
                    </div>

                    <button
                      onClick={() => setShowConfirm(false)}
                      className="rounded-lg p-1.5 text-muted-foreground transition-colors hover:bg-muted cursor-pointer"
                    >
                      <X className="h-4 w-4" />
                    </button>
                  </div>

                  <div className="flex items-center justify-end gap-2 border-t border-border/60 px-6 py-3.5 bg-muted/20">
                    <Button variant="ghost" size="sm" onClick={() => setShowConfirm(false)}>
                      {t("common.cancel")}
                    </Button>
                    <Button
                      variant="destructive"
                      size="sm"
                      onClick={() => {
                        setShowConfirm(false);
                        onClick();
                      }}
                    >
                      <Trash2 className="h-3.5 w-3.5 mr-1.5" />
                      {t("common.confirm")}
                    </Button>
                  </div>
                </div>
              </div>
            </motion.div>
          </>
        )}
      </AnimatePresence>
    </>
  );
}
