import { useState, useEffect, useMemo, useCallback, useRef, lazy, Suspense } from "react";
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
import { SkillCard } from "../components/skills/SkillCard";
import { PublisherAvatar } from "../components/marketplace/OfficialPublishers";
import { useSkills } from "../hooks/useSkills";
import { cn, formatInstalls } from "../lib/utils";
import {
  MARKETPLACE_DESCRIPTION_BATCH_SIZE,
  applyMarketplaceDescriptionPatchToSkill,
  applyMarketplaceDescriptionPatches,
  hydrateDescriptionForSkill,
  hydrateDescriptionsForSkills,
  isMissingMarketplaceDescription,
} from "../lib/marketplaceDescriptionHydration";
import type { MarketplaceResult, OfficialPublisher, Skill } from "../types";

const DetailPanel = lazy(() =>
  import("../components/layout/DetailPanel").then((mod) => ({
    default: mod.DetailPanel,
  }))
);

interface PublisherDetailProps {
  publisher: OfficialPublisher;
  onBack: () => void;
}

/** Group skills by their source repo and sort groups by total installs */
function groupByRepo(skills: Skill[]): { repo: string; skills: Skill[] }[] {
  const map = new Map<string, Skill[]>();
  for (const skill of skills) {
    const repo = skill.source ?? "unknown";
    const list = map.get(repo) ?? [];
    list.push(skill);
    map.set(repo, list);
  }

  return Array.from(map.entries())
    .map(([repo, repoSkills]) => ({
      repo,
      skills: repoSkills.sort((a, b) => b.stars - a.stars),
    }))
    .sort((a, b) => {
      const aTotal = a.skills.reduce((sum, s) => sum + s.stars, 0);
      const bTotal = b.skills.reduce((sum, s) => sum + s.stars, 0);
      return bTotal - aTotal;
    });
}

export function PublisherDetail({ publisher, onBack }: PublisherDetailProps) {
  const { t } = useTranslation();
  const { skills: hubSkills, installSkill, updateSkill, uninstallSkill } =
    useSkills();
  const [skills, setSkills] = useState<Skill[]>([]);
  const [activeRepo, setActiveRepo] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<Skill | null>(null);
  const [showBackToTop, setShowBackToTop] = useState(false);
  const [installingNames, setInstallingNames] = useState<Set<string>>(
    new Set()
  );
  const [installedNames, setInstalledNames] = useState<Set<string>>(
    new Set()
  );
  const [installStatus, setInstallStatus] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const installedSkillNames = useMemo(
    () => new Set(hubSkills.map((skill) => skill.name)),
    [hubSkills]
  );

  // Reset local view state when switching publishers
  useEffect(() => {
    setActiveRepo(null);
    setSearchQuery("");
    setSelectedSkill(null);
    setShowBackToTop(false);
  }, [publisher.name, publisher.repo]);

  // Fetch publisher's skills
  useEffect(() => {
    let cancelled = false;
    setLoading(true);

    (async () => {
      try {
        const namespace = publisher.name.toLowerCase();
        const normalizeSource = (s: Skill) =>
          (s.source ?? s.author ?? "").toLowerCase();
        const belongsToPublisher = (s: Skill) => {
          const source = normalizeSource(s);
          return source === namespace || source.startsWith(`${namespace}/`);
        };

        const result = await invoke<MarketplaceResult>("search_skills_sh", {
          query: publisher.name,
        });
        if (cancelled) return;

        let filtered = result.skills.filter(belongsToPublisher);

        // Fallback: some publishers are better matched by owner/repo query.
        if (filtered.length === 0 && publisher.repo?.trim()) {
          const repoResult = await invoke<MarketplaceResult>("search_skills_sh", {
            query: `${publisher.name}/${publisher.repo}`,
          });
          if (cancelled) return;
          filtered = repoResult.skills.filter(belongsToPublisher);
        }

        filtered.sort((a, b) => b.stars - a.stars);
        setSkills(filtered);

        void (async () => {
          const patches = await hydrateDescriptionsForSkills(
            filtered,
            MARKETPLACE_DESCRIPTION_BATCH_SIZE
          );
          if (cancelled || patches.length === 0) return;
          setSkills((prev) => applyMarketplaceDescriptionPatches(prev, patches));
          setSelectedSkill((prev) =>
            applyMarketplaceDescriptionPatchToSkill(prev, patches)
          );
        })();
      } catch (e) {
        console.error("Failed to fetch publisher skills:", e);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [publisher.name, publisher.repo]);

  // Mark session-installed skills
  const displaySkills = useMemo(() => {
    return skills.map((s) => {
      const installed =
        s.installed || installedNames.has(s.name) || installedSkillNames.has(s.name);
      return installed === s.installed ? s : { ...s, installed };
    });
  }, [skills, installedNames, installedSkillNames]);

  useEffect(() => {
    if (!selectedSkill) return;
    const installed =
      selectedSkill.installed ||
      installedNames.has(selectedSkill.name) ||
      installedSkillNames.has(selectedSkill.name);
    if (installed === selectedSkill.installed) return;
    setSelectedSkill((prev) => (prev ? { ...prev, installed } : null));
  }, [selectedSkill, installedNames, installedSkillNames]);

  const repoGroups = useMemo(() => groupByRepo(displaySkills), [displaySkills]);
  const repoSummaries = useMemo(
    () =>
      repoGroups.map(({ repo, skills: repoSkills }) => ({
        repo,
        skills: repoSkills,
        skillCount: repoSkills.length,
        totalInstalls: repoSkills.reduce((sum, s) => sum + s.stars, 0),
      })),
    [repoGroups]
  );
  const activeRepoGroup = useMemo(
    () =>
      activeRepo
        ? repoSummaries.find((group) => group.repo === activeRepo) ?? null
        : null,
    [activeRepo, repoSummaries]
  );
  const visibleRepos = useMemo(() => {
    if (activeRepo) return [];
    if (!searchQuery.trim()) return repoSummaries;
    const q = searchQuery.toLowerCase();
    return repoSummaries.filter((group) =>
      group.repo.toLowerCase().includes(q)
    );
  }, [activeRepo, repoSummaries, searchQuery]);
  const visibleSkills = useMemo(() => {
    if (!activeRepoGroup) return [];
    if (!searchQuery.trim()) return activeRepoGroup.skills;
    const q = searchQuery.toLowerCase();
    return activeRepoGroup.skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.source?.toLowerCase().includes(q) ||
        s.author?.toLowerCase().includes(q)
    );
  }, [activeRepoGroup, searchQuery]);
  const shownSkillCount = activeRepoGroup
    ? activeRepoGroup.skillCount
    : displaySkills.length;

  const totalInstalls = useMemo(
    () => skills.reduce((sum, s) => sum + s.stars, 0),
    [skills]
  );

  // === Handlers ===

  const handleInstall = useCallback(
    async (url: string, name: string) => {
      setInstallingNames((prev) => new Set(prev).add(name));
      try {
        const skill = await installSkill(url, name);
        setInstalledNames((prev) => new Set(prev).add(name));
        setSelectedSkill((prev) =>
          prev?.name === name ? { ...prev, installed: true } : prev
        );
        const agentCount = skill.agent_links?.length ?? 0;
        setInstallStatus(
          agentCount > 0
            ? t("publisherDetail.installedSynced", { count: agentCount })
            : t("publisherDetail.installed")
        );
        setTimeout(() => setInstallStatus(null), 4000);
      } catch (e) {
        const message = String(e).toLowerCase();
        if (message.includes("already installed")) {
          setInstalledNames((prev) => new Set(prev).add(name));
          setSelectedSkill((prev) =>
            prev?.name === name ? { ...prev, installed: true } : prev
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
    [installSkill, t]
  );

  const handleUpdate = useCallback(
    async (name: string) => {
      try {
        await updateSkill(name);
      } catch (e) {
        console.error("Update failed:", e);
      }
    },
    [updateSkill]
  );

  const handleUninstall = useCallback(
    async (name: string) => {
      try {
        await uninstallSkill(name);
        setInstalledNames((prev) => {
          const next = new Set(prev);
          next.delete(name);
          return next;
        });
        if (selectedSkill?.name === name) {
          setSelectedSkill((prev) =>
            prev ? { ...prev, installed: false } : null
          );
        }
      } catch (e) {
        console.error("[PublisherDetail] Uninstall failed:", e);
      }
    },
    [uninstallSkill, selectedSkill]
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
    [uninstallSkill, handleInstall]
  );

  const handleSkillClick = useCallback(
    (skill: Skill) => {
      if (selectedSkill?.name === skill.name) {
        setSelectedSkill(null);
        return;
      }

      setSelectedSkill(skill);

      if (!isMissingMarketplaceDescription(skill.description)) {
        return;
      }

      void (async () => {
        const patches = await hydrateDescriptionForSkill(skill);
        if (patches.length === 0) return;

        setSkills((prev) => applyMarketplaceDescriptionPatches(prev, patches));
        setSelectedSkill((prev) =>
          applyMarketplaceDescriptionPatchToSkill(prev, patches)
        );
      })();
    },
    [selectedSkill]
  );



  return (
    <div className="flex-1 flex overflow-hidden relative">
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* ─── Top Bar ─────────────────────────────────────── */}
        <div className="h-14 flex items-center gap-3 px-6 border-b border-white/10 bg-card/40 backdrop-blur-md">
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

          {/* Search */}
          <div className="relative flex-1 max-w-sm ml-auto">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <Input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={
                activeRepo
                  ? t("publisherDetail.searchPlaceholder", { name: activeRepo })
                  : t("publisherDetail.searchPlaceholder", { name: publisher.name })
              }
              className="pl-9"
            />
          </div>

          {/* Install status toast */}
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
                    : "text-destructive"
                )}
              >
                {installStatus}
              </motion.span>
            )}
          </AnimatePresence>
        </div>

        {/* ─── Scrollable Content ──────────────────────────── */}
        <motion.main
          ref={scrollRef}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.2 }}
          className="flex-1 overflow-y-auto bg-gradient-to-br from-transparent via-card/10 to-transparent"
          onScroll={(e) => {
            setShowBackToTop(e.currentTarget.scrollTop > 300);
          }}
        >
          {/* ─── Hero Section ──────────────────────────────── */}
          <div className="px-6 pt-6 pb-5 border-b border-white/10 bg-gradient-to-b from-primary/5 to-transparent">
            <div className="flex items-start gap-5 max-w-4xl">
              <PublisherAvatar name={publisher.name} size="lg" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2.5 mb-1">
                  <h2 className="text-heading-lg truncate">{publisher.name}</h2>
                  <Badge
                    variant="outline"
                    className="text-[10px] px-2 py-0.5 h-5 font-medium text-primary bg-primary/8 border-primary/20 shrink-0"
                  >
                    {t("publisherDetail.official")}
                  </Badge>
                </div>

                {/* Stats row */}
                <div className="flex items-center gap-4 mt-2 flex-wrap">
                  <span className="text-sm text-muted-foreground flex items-center gap-1.5">
                    <Folder className="w-3.5 h-3.5" />
                    {t("publisherDetail.repos", { count: publisher.repo_count })}
                  </span>
                  <span className="text-sm text-muted-foreground flex items-center gap-1.5">
                    <Package className="w-3.5 h-3.5" />
                    {loading ? "..." : t("publisherDetail.skills", { count: shownSkillCount })}
                  </span>
                  {!loading && totalInstalls > 0 && (
                    <span className="text-sm text-muted-foreground">
                      {t("publisherDetail.totalInstalls", { count: formatInstalls(totalInstalls) })}
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

          {/* ─── Repo / Skill Drill-down ───────────────────── */}
          <div className="p-6">
            {loading ? (
              <SkillGridSkeleton count={6} />
            ) : displaySkills.length === 0 ? (
              <EmptyState
                icon={<Package className="w-6 h-6 text-muted-foreground" />}
                title={searchQuery.trim() ? t("publisherDetail.noMatch") : t("publisherDetail.noSkills")}
                description={searchQuery.trim() ? t("publisherDetail.tryDifferent") : t("publisherDetail.installDirect", { publisher: publisher.name, repo: publisher.repo })}
              />
            ) : !activeRepo ? (
              visibleRepos.length === 0 ? (
                <EmptyState
                  icon={<Folder className="w-6 h-6 text-muted-foreground" />}
                  title="No repos match your search"
                  description="Try a different repo keyword."
                />
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3 max-w-5xl">
                  {visibleRepos.map((group) => (
                    <motion.button
                      key={group.repo}
                      initial={{ opacity: 0, y: 10 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ duration: 0.2 }}
                      onClick={() => {
                        setActiveRepo(group.repo);
                        setSearchQuery("");
                        setSelectedSkill(null);
                        scrollRef.current?.scrollTo({ top: 0, behavior: "smooth" });
                      }}
                      className="text-left rounded-xl border border-white/10 bg-card/40 hover:bg-card/60 hover:border-primary/30 p-4 transition-all group backdrop-blur-sm"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-sm font-semibold truncate">
                          {group.repo}
                        </span>
                        <ChevronRight className="w-4 h-4 text-muted-foreground group-hover:text-primary transition-colors shrink-0" />
                      </div>
                      <div className="mt-3 flex items-center gap-3 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-1.5">
                          <Package className="w-3.5 h-3.5" />
                          {t("publisherDetail.repoSkills", { count: group.skillCount })}
                        </span>
                        <span className="inline-flex items-center gap-1.5">
                          <ArrowUp className="w-3.5 h-3.5" />
                          {formatInstalls(group.totalInstalls)} installs
                        </span>
                      </div>
                    </motion.button>
                  ))}
                </div>
              )
            ) : (
              <div className="space-y-4 max-w-5xl">
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
                    {activeRepoGroup?.repo ?? activeRepo}
                  </span>
                  <Badge
                    variant="outline"
                    className="text-[10px] px-1.5 py-0 h-4 font-normal text-muted-foreground bg-muted border-transparent"
                  >
                    {t("publisherDetail.repoSkills", { count: activeRepoGroup?.skillCount ?? 0 })}
                  </Badge>
                </div>

                {visibleSkills.length === 0 ? (
                  <EmptyState
                    icon={<Package className="w-6 h-6 text-muted-foreground" />}
                    title={t("publisherDetail.noMatch")}
                    description={t("publisherDetail.tryDifferent")}
                  />
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
                    {visibleSkills.map((skill) => (
                      <SkillCard
                        key={skill.name + skill.git_url}
                        skill={skill}
                        onClick={() => handleSkillClick(skill)}
                        onInstall={handleInstall}
                        onUpdate={handleUpdate}
                        installing={installingNames.has(skill.name)}
                        noAnimate
                      />
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </motion.main>

        {/* ─── Back to top ─────────────────────────────────── */}
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
              className="absolute bottom-8 right-8 z-40 w-10 h-10 rounded-full bg-background/80 hover:bg-background border border-border/50 text-foreground/80 hover:text-foreground shadow-sm hover:shadow-md backdrop-blur-md flex items-center justify-center transition-all duration-200 cursor-pointer group"
              title={t("publisherDetail.backToTop")}
            >
              <ArrowUp className="w-4 h-4 transition-transform duration-200 group-hover:-translate-y-0.5" />
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {/* ─── Detail Panel (right sidebar) ──────────────────── */}
      {selectedSkill && (
        <Suspense
          fallback={
            <div className="absolute right-0 top-0 bottom-0 w-[400px] h-full border-l border-white/10 bg-card/60 backdrop-blur-xl shadow-2xl overflow-y-auto z-50 rounded-tl-xl rounded-bl-xl flex items-center justify-center text-sm text-zinc-400">
              Loading details...
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
