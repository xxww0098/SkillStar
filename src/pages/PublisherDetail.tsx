import {
  useState,
  useEffect,
  useMemo,
  useCallback,
  useRef,
  lazy,
  Suspense,
} from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  ArrowLeft,
  ChevronRight,
  ExternalLink,
  Folder,
  Package,
  Search,
  ArrowUp,
  GitBranch,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../components/ui/button";
import { Badge } from "../components/ui/badge";
import { Input } from "../components/ui/input";
import { EmptyState } from "../components/ui/EmptyState";
import { SkillGridSkeleton } from "../components/ui/Skeleton";
import { LoadingLogo } from "../components/ui/LoadingLogo";
import { SkillGrid } from "../features/my-skills/components/SkillGrid";
import { PublisherAvatar } from "../features/marketplace/components/OfficialPublishers";
import { useSkills } from "../features/my-skills/hooks/useSkills";
import { cn, formatInstalls } from "../lib/utils";
import type {
  LocalFirstResult,
  OfficialPublisher,
  PublisherRepo,
  Skill,
} from "../types";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  })),
);

interface PublisherDetailProps {
  publisher: OfficialPublisher;
  onBack: () => void;
}

export function PublisherDetail({ publisher, onBack }: PublisherDetailProps) {
  const { t } = useTranslation();
  const { installSkill, updateSkill, uninstallSkill, pendingUpdateNames } =
    useSkills();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [activeRepo, setActiveRepo] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [installingNames, setInstallingNames] = useState<Set<string>>(
    new Set(),
  );
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const [publisherRepos, setPublisherRepos] = useState<PublisherRepo[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setActiveRepo(null);
    setSearchQuery("");
    setSelectedSkill(null);
    setShowBackToTop(false);
  }, [publisher.name, publisher.repo]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    (async () => {
      try {
        const readLocal = () =>
          invoke<LocalFirstResult<PublisherRepo[]>>(
            "get_publisher_repos_local",
            {
              publisherName: publisher.name,
            },
          );
        const result = await readLocal();
        if (cancelled) return;
        setPublisherRepos(result.data);

        if (result.snapshot_status === "stale") {
          setRefreshing(true);
          try {
            await invoke("sync_marketplace_scope", {
              scope: `publisher_repos:${publisher.name.toLowerCase()}`,
            });
            const fresh = await readLocal();
            if (!cancelled) {
              setPublisherRepos(fresh.data);
            }
          } finally {
            if (!cancelled) setRefreshing(false);
          }
        }
      } catch (e) {
        console.error("Failed to fetch publisher repos:", e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [publisher.name]);

  useEffect(() => {
    if (!activeRepo) {
      setSkills([]);
      return;
    }

    let cancelled = false;
    setLoading(true);

    (async () => {
      try {
        const source = `${publisher.name.toLowerCase()}/${activeRepo}`;
        const readLocal = () =>
          invoke<LocalFirstResult<Skill[]>>("get_repo_skills_local", {
            source,
          });
        const result = await readLocal();
        if (cancelled) return;
        setSkills(result.data);

        if (result.snapshot_status === "stale") {
          setRefreshing(true);
          try {
            await invoke("sync_marketplace_scope", {
              scope: `repo_skills:${source}`,
            });
            const fresh = await readLocal();
            if (!cancelled) {
              setSkills(fresh.data);
            }
          } finally {
            if (!cancelled) setRefreshing(false);
          }
        }
      } catch (e) {
        console.error("Failed to resolve repo skills:", e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [publisher.name, activeRepo]);

  const visiblePublisherRepos = useMemo(() => {
    if (activeRepo) return [];
    if (!searchQuery.trim()) return publisherRepos;
    const normalizedQuery = searchQuery.toLowerCase();
    return publisherRepos.filter((repo) =>
      repo.repo.toLowerCase().includes(normalizedQuery),
    );
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

  const shownSkillCount = activeRepo
    ? skills.length
    : publisherRepos.reduce((sum, repo) => sum + repo.skill_count, 0);

  const totalInstalls = useMemo(
    () => publisherRepos.reduce((sum, repo) => sum + repo.installs, 0),
    [publisherRepos],
  );

  const handleInstall = useCallback(
    async (url: string, name: string) => {
      setInstallingNames((prev) => new Set(prev).add(name));
      try {
        const skill = await installSkill(url, name);
        setSkills((prev) =>
          prev.map((entry) =>
            entry.name === name
              ? {
                  ...entry,
                  installed: true,
                  update_available: false,
                  agent_links: skill.agent_links ?? entry.agent_links,
                }
              : entry,
          ),
        );
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
          agentCount > 0
            ? t("publisherDetail.installedSynced", { count: agentCount })
            : t("publisherDetail.installed"),
        );
        setTimeout(() => setInstallStatus(null), 4000);
      } catch (e) {
        const message = String(e).toLowerCase();
        if (message.includes("already installed")) {
          setSkills((prev) =>
            prev.map((entry) =>
              entry.name === name ? { ...entry, installed: true } : entry,
            ),
          );
          setSelectedSkill((prev) =>
            prev?.name === name ? { ...prev, installed: true } : prev,
          );
          setInstallStatus(t("publisherDetail.installed"));
          setTimeout(() => setInstallStatus(null), 4000);
          return;
        }
        console.error("[PublisherDetail] Install failed:", e);
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
    [installSkill, t],
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
        setSkills((prev) =>
          prev.map((entry) =>
            entry.name === name ? { ...entry, update_available: false } : entry,
          ),
        );
        setSelectedSkill((prev) =>
          prev?.name === name ? { ...prev, update_available: false } : prev,
        );
      } catch (e) {
        console.error("Update failed:", e);
      }
    },
    [updateSkill],
  );

  const handleUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        setSkills((prev) =>
          prev.map((entry) =>
            entry.name === name
              ? {
                  ...entry,
                  installed: false,
                  update_available: false,
                  agent_links: [],
                }
              : entry,
          ),
        );
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
        console.error("[PublisherDetail] Uninstall failed:", e);
      }
    },
    [uninstallSkill, selectedSkill],
  );

  const handleReinstall = useCallback(
    async (url: string, name: string) => {
      try {
        await uninstallSkill(name);
        await handleInstall(url, name);
      } catch (e) {
        console.error("[PublisherDetail] Reinstall failed:", e);
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
        <div className="h-14 flex items-center gap-3 px-6 border-b border-border bg-sidebar">
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

          <h1 className="text-heading-md whitespace-nowrap truncate">
            {publisher.name}
          </h1>
          {activeRepo && (
            <>
              <span className="text-muted-foreground">/</span>
              <span className="text-sm text-foreground/80 font-mono truncate max-w-[320px]">
                {activeRepo}
              </span>
            </>
          )}

          <div className="relative flex-1 max-w-sm ml-auto">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={
                activeRepo
                  ? t("publisherDetail.searchPlaceholder", { name: activeRepo })
                  : t("publisherDetail.searchPlaceholder", {
                      name: publisher.name,
                    })
              }
              className="pl-9"
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
                  installStatus.startsWith("✓")
                    ? "text-success"
                    : "text-destructive",
                )}
              >
                {installStatus}
              </motion.span>
            )}
          </AnimatePresence>
          {refreshing && (
            <span className="text-xs text-muted-foreground whitespace-nowrap">
              {t("marketplace.refreshingSnapshot", {
                defaultValue: "Refreshing snapshot...",
              })}
            </span>
          )}
        </div>

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
                    {loading
                      ? "..."
                      : t("publisherDetail.skills", { count: shownSkillCount })}
                  </span>
                  {!loading && totalInstalls > 0 && (
                    <span className="text-sm text-muted-foreground">
                      {t("publisherDetail.totalInstalls", {
                        count: formatInstalls(totalInstalls),
                      })}
                    </span>
                  )}
                  <a
                    href={publisher.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-sm text-primary/70 hover:text-primary flex items-center gap-1.5 transition-colors ml-auto"
                  >
                    <ExternalLink className="w-3.5 h-3.5" />
                    {t("publisherDetail.viewOnSkillsSh")}
                  </a>
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
                title={
                  searchQuery.trim()
                    ? t("publisherDetail.noMatch")
                    : t("publisherDetail.noSkills")
                }
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
                  title="No repos match your search"
                  description="Try a different repo keyword."
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
                            {repo.installs_label} installs
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
                    Repos
                  </Button>
                  <div className="w-px h-4 bg-border" />
                  <GitBranch className="w-3.5 h-3.5 text-muted-foreground" />
                  <span className="text-xs font-semibold text-foreground/80 font-mono truncate">
                    {activeRepo}
                  </span>
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
                    onInstall={handleInstall}
                    installingNames={installingNames}
                    onUpdate={handleUpdate}
                    pendingUpdateNames={pendingUpdateNames}
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
              onClick={() =>
                scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" })
              }
              className="absolute bottom-8 right-8 z-40 w-10 h-10 rounded-full bg-background/80 hover:bg-background border border-border/50 text-foreground/80 hover:text-foreground shadow-sm hover:shadow-md backdrop-blur-md flex items-center justify-center transition duration-200 cursor-pointer group"
              title={t("publisherDetail.backToTop")}
            >
              <ArrowUp className="w-4 h-4 transition-transform duration-200 group-hover:-translate-y-0.5" />
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {selectedSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 w-full max-w-md h-full border-l border-border bg-card backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center">
              <LoadingLogo size="sm" />
            </div>
          }
        >
          <DetailPanel
            skill={selectedSkill}
            onClose={() => setSelectedSkill(null)}
            onInstall={handleInstall}
            onUpdate={handleUpdate}
            onUninstall={handleUninstall}
            onReinstall={handleReinstall}
          />
        </Suspense>
      )}
    </div>
  );
}
