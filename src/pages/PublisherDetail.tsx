import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "../lib/ipc";
import { AnimatePresence, motion } from "framer-motion";
import { ArrowLeft, ArrowUp, ChevronRight, ExternalLink, Folder, GitBranch, Package, Search } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { DetailPanel } from "../components/layout/DetailPanel";
import { PageToolbar } from "../components/layout/PageToolbar";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/EmptyState";
import { ExternalAnchor } from "../components/ui/ExternalAnchor";
import { Input } from "../components/ui/input";
import { SkillGridSkeleton } from "../components/ui/Skeleton";
import { marketplaceKeys } from "../features/marketplace/api/keys";
import { PublisherAvatar } from "../components/shared/PublisherAvatar";
import { SkillGrid } from "../features/my-skills/components/SkillGrid";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { useAgentProfiles } from "../hooks/useAgentProfiles";
import type { LocalFirstResult } from "../types";
import { cn, formatInstalls } from "../lib/utils";
import type { OfficialPublisher, PublisherRepo, Skill } from "../types";

interface PublisherDetailProps {
  publisher: OfficialPublisher;
  onBack: () => void;
}

export function PublisherDetail({ publisher, onBack }: PublisherDetailProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const {
    skills: installedSkills,
    installSkill,
    updateSkill,
    uninstallSkill,
    pendingUpdateNames,
    toggleSkillForAgent,
    pendingAgentToggleKeys,
  } = useSkills();
  const { profiles } = useAgentProfiles();
  const [activeRepo, setActiveRepo] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [installingNames, setInstallingNames] = useState<Set<string>>(new Set());
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // One-shot-per-key guards: only the first stale snapshot for a given
  // publisher/repo triggers a background sync. Re-arm whenever the key changes.
  const publisherReposStaleTriggeredFor = useRef<string | null>(null);
  const repoSkillsStaleTriggeredFor = useRef<string | null>(null);

  useEffect(() => {
    setSelectedSkill((current) => {
      if (!current) return null;
      const installed = installedSkills.find((skill) => skill.name === current.name);
      return installed ? { ...current, ...installed } : current;
    });
  }, [installedSkills]);

  useEffect(() => {
    setActiveRepo(null);
    setSearchQuery("");
    setSelectedSkill(null);
    setShowBackToTop(false);
  }, [publisher.name, publisher.repo]);

  // Local-first load of the publisher's repos.
  const publisherReposQuery = useQuery<LocalFirstResult<PublisherRepo[]>>({
    queryKey: marketplaceKeys.publisherRepos(publisher.name),
    queryFn: () =>
      tauriInvoke("get_publisher_repos_local", {
        publisherName: publisher.name,
      }),
  });

  useEffect(() => {
    if (!publisherReposQuery.isError) return;
    if (import.meta.env.DEV) console.error("Failed to fetch publisher repos:", publisherReposQuery.error);
  }, [publisherReposQuery.isError, publisherReposQuery.error]);

  const publisherRepos = publisherReposQuery.data?.data ?? [];

  const syncPublisherReposMutation = useMutation({
    mutationFn: () =>
      tauriInvoke("sync_marketplace_scope", {
        scope: `publisher_repos:${publisher.name.toLowerCase()}`,
      }),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: marketplaceKeys.publisherRepos(publisher.name),
      }),
  });

  useEffect(() => {
    const status = publisherReposQuery.data?.snapshot_status;
    if (
      status === "stale" &&
      publisherReposStaleTriggeredFor.current !== publisher.name &&
      !syncPublisherReposMutation.isPending
    ) {
      publisherReposStaleTriggeredFor.current = publisher.name;
      syncPublisherReposMutation.mutate();
    }
  }, [publisherReposQuery.data?.snapshot_status, publisher.name, syncPublisherReposMutation]);

  // Local-first load of the active repo's skills.
  const repoSource = activeRepo ? `${publisher.name.toLowerCase()}/${activeRepo}` : null;

  const repoSkillsQuery = useQuery<LocalFirstResult<Skill[]>>({
    queryKey: marketplaceKeys.repoSkills(repoSource ?? ""),
    queryFn: () =>
      tauriInvoke("get_repo_skills_local", {
        source: repoSource ?? "",
      }),
    enabled: repoSource != null,
  });

  useEffect(() => {
    if (!repoSkillsQuery.isError) return;
    if (import.meta.env.DEV) console.error("Failed to resolve repo skills:", repoSkillsQuery.error);
  }, [repoSkillsQuery.isError, repoSkillsQuery.error]);

  const skills = repoSource ? (repoSkillsQuery.data?.data ?? []) : [];

  const syncRepoSkillsMutation = useMutation({
    mutationFn: () =>
      tauriInvoke("sync_marketplace_scope", {
        scope: `repo_skills:${repoSource}`,
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({
        queryKey: marketplaceKeys.repoSkills(repoSource ?? ""),
      });
      // The repo card's skill count is derived from the rows this sync rewrote.
      queryClient.invalidateQueries({
        queryKey: marketplaceKeys.publisherRepos(publisher.name),
      });
    },
  });

  useEffect(() => {
    if (!repoSource) return;
    const status = repoSkillsQuery.data?.snapshot_status;
    if (status === "stale" && repoSkillsStaleTriggeredFor.current !== repoSource && !syncRepoSkillsMutation.isPending) {
      repoSkillsStaleTriggeredFor.current = repoSource;
      syncRepoSkillsMutation.mutate();
    }
  }, [repoSource, repoSkillsQuery.data?.snapshot_status, syncRepoSkillsMutation]);

  const loading = activeRepo ? repoSkillsQuery.isLoading : publisherReposQuery.isLoading;
  const refreshing = syncPublisherReposMutation.isPending || syncRepoSkillsMutation.isPending;

  const visiblePublisherRepos = useMemo(() => {
    if (activeRepo) return [];
    if (!searchQuery.trim()) return publisherRepos;
    const normalizedQuery = searchQuery.toLowerCase();
    return publisherRepos.filter((repo) => repo.repo.toLowerCase().includes(normalizedQuery));
  }, [activeRepo, publisherRepos, searchQuery]);

  const visibleSkills = useMemo(() => {
    if (!activeRepo) return [];
    if (!searchQuery.trim()) return skills;
    const normalizedQuery = searchQuery.toLowerCase();
    return skills.filter(
      (skill) =>
        skill.name.toLowerCase().includes(normalizedQuery) ||
        skill.description.toLowerCase().includes(normalizedQuery) ||
        skill.source?.toLowerCase().includes(normalizedQuery) ||
        skill.author?.toLowerCase().includes(normalizedQuery),
    );
  }, [activeRepo, skills, searchQuery]);

  const shownSkillCount = activeRepo ? skills.length : publisherRepos.reduce((sum, repo) => sum + repo.skill_count, 0);

  const totalInstalls = useMemo(() => publisherRepos.reduce((sum, repo) => sum + repo.installs, 0), [publisherRepos]);

  // Patch the currently-active repo's cached skill list in place (mirrors the
  // old `setSkills((prev) => prev.map(...))` local-state updater, now applied
  // to the query cache so it stays in sync with what the grid reads).
  const patchSkill = useCallback(
    (name: string, updater: (skill: Skill) => Skill) => {
      if (!repoSource) return;
      queryClient.setQueryData<LocalFirstResult<Skill[]>>(marketplaceKeys.repoSkills(repoSource), (prev) =>
        prev
          ? {
              ...prev,
              data: prev.data.map((entry) => (entry.name === name ? updater(entry) : entry)),
            }
          : prev,
      );
    },
    [queryClient, repoSource],
  );

  const handleInstall = useCallback(
    async (url: string, name: string, agentId?: string) => {
      setInstallingNames((prev) => new Set(prev).add(name));
      try {
        const skill = await installSkill(url, name, agentId);
        patchSkill(name, (entry) => ({
          ...entry,
          installed: true,
          update_available: false,
          agent_links: skill.agent_links ?? entry.agent_links,
        }));
        setSelectedSkill((prev) =>
          prev?.name === name
            ? {
                ...prev,
                installed: true,
                update_available: false,
                agent_links: skill.agent_links ?? prev.agent_links,
              }
            : prev,
        );
        const agentCount = skill.agent_links?.length ?? 0;
        setInstallStatus(
          agentCount > 0 ? t("publisherDetail.installedSynced", { count: agentCount }) : t("publisherDetail.installed"),
        );
        setTimeout(() => setInstallStatus(null), 4000);
      } catch (e) {
        const message = String(e).toLowerCase();
        if (message.includes("already installed")) {
          patchSkill(name, (entry) => ({ ...entry, installed: true }));
          setSelectedSkill((prev) => (prev?.name === name ? { ...prev, installed: true } : prev));
          setInstallStatus(t("publisherDetail.installed"));
          setTimeout(() => setInstallStatus(null), 4000);
          return;
        }
        if (import.meta.env.DEV) console.error("[PublisherDetail] Install failed:", e);
        setInstallStatus(`✗ ${String(e)}`);
        setTimeout(() => setInstallStatus(null), 5000);
      } finally {
        setInstallingNames((prev) => {
          const next = new Set(prev);
          next.delete(name);
          return next;
        });
      }
    },
    [installSkill, patchSkill, t],
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
        patchSkill(name, (entry) => ({ ...entry, update_available: false }));
        setSelectedSkill((prev) => (prev?.name === name ? { ...prev, update_available: false } : prev));
      } catch (e) {
        if (import.meta.env.DEV) console.error("Update failed:", e);
      }
    },
    [updateSkill, patchSkill],
  );

  const handleUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        patchSkill(name, (entry) => ({
          ...entry,
          installed: false,
          update_available: false,
          agent_links: [],
        }));
        if (selectedSkill?.name === name) {
          setSelectedSkill((prev) =>
            prev
              ? {
                  ...prev,
                  installed: false,
                  update_available: false,
                  agent_links: [],
                }
              : null,
          );
        }
      } catch (e) {
        if (import.meta.env.DEV) console.error("[PublisherDetail] Uninstall failed:", e);
      }
    },
    [uninstallSkill, selectedSkill, patchSkill],
  );

  const handleReinstall = useCallback(
    async (url: string, name: string) => {
      try {
        await uninstallSkill(name);
        await handleInstall(url, name);
      } catch (e) {
        if (import.meta.env.DEV) console.error("[PublisherDetail] Reinstall failed:", e);
      }
    },
    [uninstallSkill, handleInstall],
  );

  const handleSkillClick = useCallback(
    (skill: Skill) => {
      if (selectedSkill?.name === skill.name) {
        setSelectedSkill(null);
        return;
      }
      setSelectedSkill(skill);
    },
    [selectedSkill],
  );

  return (
    <div className="flex-1 min-w-0 flex overflow-hidden relative">
      <div className="flex-1 min-w-0 flex flex-col overflow-hidden">
        <PageToolbar
          title={
            <div className="flex items-center gap-2 min-w-0">
              <Button
                variant="ghost"
                size="sm"
                onClick={onBack}
                className="gap-1.5 text-muted-foreground hover:text-foreground -ml-2"
              >
                <ArrowLeft className="w-4 h-4" />
                {t("publisherDetail.back")}
              </Button>
              <div className="w-px h-5 bg-border mx-1" />
              <span className="text-sm font-semibold whitespace-nowrap truncate">{publisher.name}</span>
              {activeRepo && (
                <>
                  <span className="text-muted-foreground">/</span>
                  <span className="text-sm text-foreground/80 font-mono truncate max-w-[320px]">{activeRepo}</span>
                </>
              )}
            </div>
          }
          actions={
            <div className="flex items-center gap-2">
              <div className="relative w-56">
                <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
                <Input
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  placeholder={
                    activeRepo
                      ? t("publisherDetail.searchPlaceholder", { name: activeRepo })
                      : t("publisherDetail.searchPlaceholder", { name: publisher.name })
                  }
                  className="pl-8 h-8 text-xs"
                />
              </div>
              <AnimatePresence>
                {installStatus && (
                  <motion.span
                    initial={{ opacity: 0, x: 10 }}
                    animate={{ opacity: 1, x: 0 }}
                    exit={{ opacity: 0 }}
                    className={cn(
                      "text-xs font-medium whitespace-nowrap",
                      installStatus.startsWith("✓") ? "text-success" : "text-destructive",
                    )}
                  >
                    {installStatus}
                  </motion.span>
                )}
              </AnimatePresence>
              {refreshing && (
                <span className="text-xs text-muted-foreground whitespace-nowrap">
                  {t("marketplace.refreshingSnapshot", { defaultValue: "Refreshing snapshot..." })}
                </span>
              )}
            </div>
          }
        />

        <motion.main
          ref={scrollRef}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="ss-page-scroll"
          onScroll={(e) => {
            setShowBackToTop(e.currentTarget.scrollTop > 300);
          }}
        >
          <div className="px-6 pt-6 pb-5 border-b border-border bg-gradient-to-b from-primary/5 to-transparent">
            <div className="flex items-start gap-5 max-w-4xl">
              <PublisherAvatar name={publisher.name} size="lg" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2.5 mb-1">
                  <h2 className="text-heading-lg truncate">{publisher.name}</h2>
                  <Badge
                    variant="outline"
                    className="text-micro px-2 py-0.5 h-5 font-medium text-primary bg-primary/8 border-primary/20 shrink-0"
                  >
                    {t("publisherDetail.official")}
                  </Badge>
                </div>

                <div className="flex items-center gap-4 mt-2 flex-wrap">
                  <span className="text-sm text-muted-foreground flex items-center gap-1.5">
                    <Folder className="w-3.5 h-3.5" />
                    {t("publisherDetail.repos", {
                      count: publisher.repo_count,
                    })}
                  </span>
                  <span className="text-sm text-muted-foreground flex items-center gap-1.5">
                    <Package className="w-3.5 h-3.5" />
                    {loading ? "..." : t("publisherDetail.skills", { count: shownSkillCount })}
                  </span>
                  {!loading && totalInstalls > 0 && (
                    <span className="text-sm text-muted-foreground">
                      {t("publisherDetail.totalInstalls", {
                        count: formatInstalls(totalInstalls),
                      })}
                    </span>
                  )}
                  <ExternalAnchor
                    href={publisher.url}
                    className="text-sm text-primary/70 hover:text-primary flex items-center gap-1.5 transition-colors ml-auto"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    {t("publisherDetail.viewOnSkillsSh")}
                  </ExternalAnchor>
                </div>
              </div>
            </div>
          </div>

          <div>
            {loading ? (
              <SkillGridSkeleton count={6} />
            ) : !activeRepo && publisherRepos.length === 0 ? (
              <EmptyState
                icon={<Package className="w-6 h-6 text-muted-foreground" />}
                title={searchQuery.trim() ? t("publisherDetail.noMatch") : t("publisherDetail.noSkills")}
                description={
                  searchQuery.trim()
                    ? t("publisherDetail.tryDifferent")
                    : t("publisherDetail.installDirect", {
                        publisher: publisher.name,
                        repo: publisher.repo,
                      })
                }
              />
            ) : !activeRepo ? (
              visiblePublisherRepos.length === 0 ? (
                <EmptyState
                  icon={<Folder className="w-6 h-6 text-muted-foreground" />}
                  title={t("publisherDetail.noReposMatch")}
                  description={t("publisherDetail.tryDifferentRepo")}
                />
              ) : (
                <div className="ss-decks-grid">
                  {visiblePublisherRepos.map((repo) => (
                    <motion.button
                      key={repo.repo}
                      initial={{ opacity: 0, y: 10 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.2 }}
                      onClick={() => {
                        setActiveRepo(repo.repo);
                        setSearchQuery("");
                        setSelectedSkill(null);
                        scrollRef.current?.scrollTo({
                          top: 0,
                          behavior: "smooth",
                        });
                      }}
                      className="text-left rounded-xl border border-border bg-card hover:bg-card-hover hover:border-primary/30 p-4 transition group"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-sm font-semibold truncate">
                          {publisher.name}/{repo.repo}
                        </span>
                        <ChevronRight className="w-4 h-4 text-muted-foreground group-hover:text-primary transition-colors shrink-0" />
                      </div>
                      <div className="mt-3 flex items-center gap-3 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <Package className="w-3.5 h-3.5" />
                          {t("publisherDetail.repoSkills", {
                            count: repo.skill_count,
                          })}
                        </span>
                        {repo.installs_label && (
                          <span className="inline-flex items-center gap-1.5">
                            <ArrowUp className="w-3.5 h-3.5" />
                            {t("publisherDetail.repoInstalls", { count: repo.installs_label })}
                          </span>
                        )}
                      </div>
                    </motion.button>
                  ))}
                </div>
              )
            ) : (
              <div className="space-y-4">
                <div className="flex items-center gap-2">
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => {
                      setActiveRepo(null);
                      setSearchQuery("");
                      setSelectedSkill(null);
                    }}
                    className="gap-1.5 -ml-2"
                  >
                    <ArrowLeft className="w-4 h-4" />
                    {t("publisherDetail.backToRepos")}
                  </Button>
                  <div className="w-px h-4 bg-border" />
                  <GitBranch className="w-3.5 h-3.5 text-muted-foreground" />
                  <span className="text-xs font-semibold text-foreground/80 font-mono truncate">{activeRepo}</span>
                  <Badge
                    variant="outline"
                    className="text-micro px-1.5 py-0 h-4 font-normal text-muted-foreground bg-muted border-transparent"
                  >
                    {t("publisherDetail.repoSkills", { count: skills.length })}
                  </Badge>
                </div>

                {loading ? (
                  <SkillGridSkeleton count={6} />
                ) : visibleSkills.length === 0 ? (
                  <EmptyState
                    icon={<Package className="w-6 h-6 text-muted-foreground" />}
                    title={t("publisherDetail.noMatch")}
                    description={t("publisherDetail.tryDifferent")}
                  />
                ) : (
                  <SkillGrid
                    skills={visibleSkills}
                    viewMode="grid"
                    columnStrategy="auto-fill"
                    minColumnWidth={320}
                    onSkillClick={handleSkillClick}
                    selectedSkills={selectedSkill ? new Set([selectedSkill.name]) : undefined}
                    onInstall={handleInstall}
                    installingNames={installingNames}
                    onUpdate={handleUpdate}
                    pendingUpdateNames={pendingUpdateNames}
                    profiles={profiles}
                    onToggleAgent={toggleSkillForAgent}
                    pendingAgentToggleKeys={pendingAgentToggleKeys}
                    emptyMessage={t("publisherDetail.noMatch")}
                  />
                )}
              </div>
            )}
          </div>
        </motion.main>

        <AnimatePresence>
          {showBackToTop && (
            <motion.button
              initial={{ opacity: 0, scale: 0.8 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              transition={{ duration: 0.15 }}
              onClick={() => scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })}
              className="absolute bottom-8 right-8 z-40 w-10 h-10 rounded-full bg-background/80 hover:bg-background border border-border/50 text-foreground/80 hover:text-foreground shadow-sm hover:shadow-md backdrop-blur-md flex items-center justify-center transition duration-200 cursor-pointer group"
              title={t("publisherDetail.backToTop")}
            >
              <ArrowUp className="w-4 h-4 transition-transform duration-200 group-hover:-translate-y-0.5" />
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {selectedSkill && (
        <DetailPanel
          skill={selectedSkill}
          onClose={() => setSelectedSkill(null)}
          onInstall={handleInstall}
          onUpdate={handleUpdate}
          onUninstall={handleUninstall}
          onReinstall={handleReinstall}
        />
      )}
    </div>
  );
}
