import { useState, useEffect, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "../ui/button";
import { SearchInput } from "../ui/SearchInput";
import {
  X,
  Github,
  Globe,
  Lock,
  Loader2,
  ExternalLink,
  Check,
  Copy,
  AlertTriangle,
  Terminal,
  FolderOpen,
  FileText,
  ChevronRight,
  ChevronLeft,
  Plus,
  GitBranch,
} from "lucide-react";
import type { GhStatus, PublishResult, UserRepo } from "../../types";

interface PublishSkillModalProps {
  open: boolean;
  onClose: () => void;
  skillName: string;
  skillDescription: string;
  /** Called after successful publish with the new git_url */
  onPublished?: (gitUrl: string) => void;
}

type Phase = "checking" | "setup" | "pick-repo" | "form" | "publishing" | "done";

export function PublishSkillModal({
  open,
  onClose,
  skillName,
  skillDescription,
  onPublished,
}: PublishSkillModalProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>("checking");
  const [ghStatus, setGhStatus] = useState<GhStatus | null>(null);

  // Repo picker
  const [repos, setRepos] = useState<UserRepo[]>([]);
  const [loadingRepos, setLoadingRepos] = useState(false);
  const [repoSearch, setRepoSearch] = useState("");
  const [selectedRepo, setSelectedRepo] = useState<UserRepo | null>(null);
  const [loadingFolders, setLoadingFolders] = useState(false);
  const [repoFolders, setRepoFolders] = useState<string[]>([]);

  // Form — for "create new repo" mode
  const [createNew, setCreateNew] = useState(false);
  const [newRepoName, setNewRepoName] = useState("my-skills");
  const [newRepoDesc, setNewRepoDesc] = useState(skillDescription || "SkillStar skills collection");
  const [isPublic, setIsPublic] = useState(true);

  // Shared
  const [folderName, setFolderName] = useState(skillName);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<PublishResult | null>(null);
  const [copied, setCopied] = useState(false);
  const [skillFiles, setSkillFiles] = useState<string[]>([]);

  const checkGhStatus = useCallback(async () => {
    setPhase("checking");
    setError(null);
    try {
      const status = await invoke<GhStatus>("check_gh_status");
      setGhStatus(status);
      if (status.status === "Ready") {
        setPhase("pick-repo");
        loadRepos();
      } else {
        setPhase("setup");
      }
    } catch (e) {
      setError(String(e));
      setPhase("setup");
    }
  }, []);

  const loadRepos = async () => {
    setLoadingRepos(true);
    setError(null);
    try {
      const list = await invoke<UserRepo[]>("list_user_repos", { limit: 50 });
      setRepos(list);
    } catch (e) {
      console.warn("Failed to load repos", e);
      setRepos([]);
      setError(`Failed to load repositories: ${String(e)}`);
    } finally {
      setLoadingRepos(false);
    }
  };

  const selectRepo = async (repo: UserRepo) => {
    setSelectedRepo(repo);
    setLoadingFolders(true);
    try {
      const folders = await invoke<string[]>("inspect_repo_folders", {
        repoFullName: repo.full_name,
      });
      setRepoFolders(folders);
    } catch {
      setRepoFolders([]);
    } finally {
      setLoadingFolders(false);
      setPhase("form");
    }
  };

  const startCreateNew = () => {
    setCreateNew(true);
    setSelectedRepo(null);
    setRepoFolders([]);
    setPhase("form");
  };

  useEffect(() => {
    if (open) {
      setFolderName(skillName);
      setResult(null);
      setError(null);
      setCopied(false);
      setSelectedRepo(null);
      setCreateNew(false);
      setRepoSearch("");
      checkGhStatus();
    }
  }, [open, skillName, checkGhStatus]);

  // Load skill files when entering form phase
  useEffect(() => {
    if (phase === "form" && skillName) {
      invoke<string[]>("list_skill_files", { name: skillName })
        .then(setSkillFiles)
        .catch(() => setSkillFiles(["SKILL.md"]));
    }
  }, [phase, skillName]);

  const handlePublish = async () => {
    setPhase("publishing");
    setError(null);
    try {
      const res = await invoke<PublishResult>("publish_skill_to_github", {
        skillName,
        repoName: createNew ? newRepoName : (selectedRepo?.full_name.split("/")[1] || "my-skills"),
        description: createNew ? newRepoDesc : (selectedRepo?.description || ""),
        isPublic: createNew ? isPublic : (selectedRepo?.is_public ?? true),
        existingRepoUrl: createNew ? null : (selectedRepo?.url + ".git"),
        folderName,
      });
      setResult(res);
      setPhase("done");
      onPublished?.(res.git_url);
    } catch (e) {
      setError(String(e));
      setPhase("form");
    }
  };

  const handleCopy = async (text: string) => {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleClose = () => {
    onClose();
    setTimeout(() => {
      setPhase("checking");
      setGhStatus(null);
      setResult(null);
      setError(null);
      setRepos([]);
    }, 200);
  };

  const filteredRepos = repos.filter(
    (r) =>
      !repoSearch ||
      r.full_name.toLowerCase().includes(repoSearch.toLowerCase()) ||
      r.description.toLowerCase().includes(repoSearch.toLowerCase())
  );

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.15 }}
            className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm"
            onClick={handleClose}
          />

          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 12 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.96, y: 12 }}
            transition={{ type: "spring", bounce: 0.1, duration: 0.35 }}
            className={`fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-50 transition-all duration-300 ease-in-out w-full flex flex-col ${
              phase === "form" ? "max-w-[760px] h-[580px]" : "max-w-md h-auto"
            }`}
          >
            <div className="relative overflow-hidden rounded-[24px] border border-white/10 bg-card/95 shadow-[0_0_80px_-20px_rgba(0,0,0,0.5)] backdrop-blur-3xl ring-1 ring-white/5 flex-1 flex flex-col min-h-0">
              {/* Top ambient glow */}
              <div className="pointer-events-none absolute -left-20 -top-20 h-48 w-48 rounded-full bg-primary/20 blur-[60px] opacity-70" />
              <div className="pointer-events-none absolute -right-20 -top-20 h-48 w-48 rounded-full bg-blue-500/10 blur-[60px] opacity-70" />
              <div className="relative z-10 flex flex-col flex-1 h-full min-h-0">
              {/* Header */}
              <div className="flex items-center justify-between px-6 pt-4 shrink-0">
                <h2 className="text-heading-sm flex items-center gap-2">
                  <Github className="w-4.5 h-4.5" />
                  {t("publishModal.title")}
                </h2>
                <button
                  onClick={handleClose}
                  className="p-1.5 rounded-lg hover:bg-muted text-muted-foreground transition-colors cursor-pointer"
                >
                  <X className="w-4 h-4" />
                </button>
              </div>

              <div className="px-6 pb-2 pt-1 text-sm text-muted-foreground leading-relaxed">
                {phase === "pick-repo"
                  ? t("publishModal.pickRepoDesc")
                  : t("publishModal.description")}
              </div>

              <div className="px-6 py-4 space-y-4 overflow-y-auto flex-1">
                {/* Phase: Checking */}
                {phase === "checking" && (
                  <div className="flex items-center justify-center py-8 text-muted-foreground gap-2">
                    <Loader2 className="w-4 h-4 animate-spin" />
                    <span className="text-sm">{t("publishModal.checkingGh")}</span>
                  </div>
                )}

                {/* Phase: Setup */}
                {phase === "setup" && ghStatus && (
                  <div className="space-y-4">
                    {ghStatus.status === "NotInstalled" && (
                      <div className="rounded-xl border border-warning/30 bg-warning/5 p-4 space-y-3">
                        <div className="flex items-start gap-2.5">
                          <AlertTriangle className="w-4.5 h-4.5 text-warning mt-0.5 shrink-0" />
                          <div className="space-y-1.5">
                            <p className="text-sm font-medium text-foreground">{t("publishModal.ghNotInstalled")}</p>
                            <p className="text-xs text-muted-foreground">{t("publishModal.ghNotInstalledDesc")}</p>
                          </div>
                        </div>
                        <div className="bg-muted rounded-lg px-3 py-2 font-mono text-xs text-foreground/80 flex items-center gap-2">
                          <Terminal className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
                          <code className="flex-1 select-all">brew install gh</code>
                          <button onClick={() => handleCopy("brew install gh")} className="p-1 rounded hover:bg-card-hover text-muted-foreground cursor-pointer">
                            {copied ? <Check className="w-3 h-3 text-success" /> : <Copy className="w-3 h-3" />}
                          </button>
                        </div>
                        <a href="https://cli.github.com/" target="_blank" rel="noopener noreferrer" className="flex items-center gap-1.5 text-xs text-primary hover:underline">
                          <ExternalLink className="w-3 h-3" />cli.github.com
                        </a>
                      </div>
                    )}
                    {ghStatus.status === "NotAuthenticated" && (
                      <div className="rounded-xl border border-warning/30 bg-warning/5 p-4 space-y-3">
                        <div className="flex items-start gap-2.5">
                          <AlertTriangle className="w-4.5 h-4.5 text-warning mt-0.5 shrink-0" />
                          <div className="space-y-1.5">
                            <p className="text-sm font-medium text-foreground">{t("publishModal.ghNotLoggedIn")}</p>
                            <p className="text-xs text-muted-foreground">{t("publishModal.ghNotLoggedInDesc")}</p>
                          </div>
                        </div>
                        <div className="bg-muted rounded-lg px-3 py-2 font-mono text-xs text-foreground/80 flex items-center gap-2">
                          <Terminal className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
                          <code className="flex-1 select-all">gh auth login</code>
                          <button onClick={() => handleCopy("gh auth login")} className="p-1 rounded hover:bg-card-hover text-muted-foreground cursor-pointer">
                            {copied ? <Check className="w-3 h-3 text-success" /> : <Copy className="w-3 h-3" />}
                          </button>
                        </div>
                      </div>
                    )}
                    <Button variant="outline" className="w-full" onClick={checkGhStatus}>
                      <Loader2 className="w-4 h-4 mr-2" />{t("publishModal.recheckStatus")}
                    </Button>
                  </div>
                )}

                {/* Phase: Pick Repo */}
                {phase === "pick-repo" && (
                  <div className="space-y-3">
                    {ghStatus?.status === "Ready" && (
                      <div className="flex items-center gap-2 text-xs text-muted-foreground bg-muted/50 rounded-lg px-3 py-2 border border-border/50">
                        <Check className="w-3.5 h-3.5 text-success" />
                        {t("publishModal.authenticatedAs")}
                        <span className="font-medium text-foreground">{ghStatus.username}</span>
                      </div>
                    )}

                    {/* Create new button */}
                    <button
                      onClick={startCreateNew}
                      className="w-full flex items-center gap-3 px-3 py-2.5 rounded-lg border border-dashed border-primary/30 hover:bg-primary/5 hover:border-primary/50 transition-all cursor-pointer group"
                    >
                      <div className="w-8 h-8 rounded-lg bg-primary/10 flex items-center justify-center group-hover:bg-primary/15 transition-colors">
                        <Plus className="w-4 h-4 text-primary" />
                      </div>
                      <div className="text-left">
                        <p className="text-sm font-medium text-primary">{t("publishModal.createNewRepo")}</p>
                        <p className="text-[11px] text-muted-foreground">{t("publishModal.createNewRepoDesc")}</p>
                      </div>
                    </button>

                    {/* Search */}
                    <SearchInput
                      value={repoSearch}
                      onChange={(e) => setRepoSearch(e.target.value)}
                      placeholder={t("publishModal.searchRepos")}
                      className="h-8 w-full rounded-md border border-input bg-transparent shadow-sm pl-9 pr-3 text-xs"
                      iconClassName="left-3 w-3.5 h-3.5"
                    />

                    {/* Repo list */}
                    {loadingRepos ? (
                      <div className="flex items-center justify-center py-6 text-muted-foreground gap-2">
                        <Loader2 className="w-4 h-4 animate-spin" />
                        <span className="text-xs">{t("publishModal.loadingRepos")}</span>
                      </div>
                    ) : (
                      <div className="max-h-[240px] overflow-y-auto space-y-1 -mx-1 px-1">
                        {filteredRepos.length === 0 && (
                          <p className="text-xs text-muted-foreground text-center py-4">
                            {repoSearch ? t("publishModal.noMatchingRepos") : t("publishModal.noRepos")}
                          </p>
                        )}
                        {filteredRepos.map((repo) => (
                          <button
                            key={repo.full_name}
                            onClick={() => selectRepo(repo)}
                            className="w-full flex items-center gap-3 px-3 py-2 rounded-lg hover:bg-muted/80 transition-colors cursor-pointer text-left group"
                          >
                            <div className="w-8 h-8 rounded-lg bg-muted flex items-center justify-center shrink-0">
                              <GitBranch className="w-3.5 h-3.5 text-muted-foreground" />
                            </div>
                            <div className="min-w-0 flex-1">
                              <div className="flex items-center gap-1.5">
                                <span className="text-xs font-medium truncate">{repo.full_name}</span>
                                {repo.is_public ? (
                                  <Globe className="w-3 h-3 text-muted-foreground/50 shrink-0" />
                                ) : (
                                  <Lock className="w-3 h-3 text-muted-foreground/50 shrink-0" />
                                )}
                              </div>
                              {repo.description && (
                                <p className="text-[10px] text-muted-foreground truncate">{repo.description}</p>
                              )}
                            </div>
                            <ChevronRight className="w-3.5 h-3.5 text-muted-foreground/30 group-hover:text-muted-foreground/60 shrink-0" />
                          </button>
                        ))}
                      </div>
                    )}

                    {error && (
                      <div className="text-xs text-destructive bg-destructive/10 p-2.5 rounded-md border border-destructive/20">
                        {error}
                      </div>
                    )}
                  </div>
                )}

                {/* Phase: Form */}
                {phase === "form" && (
                  <div className="flex gap-6 items-start h-full pb-2">
                    {/* Left side: Form */}
                    <div className="flex-1 space-y-4 min-w-0">
                      {/* Target repo info */}
                      {selectedRepo && (
                        <div className="flex items-center gap-2.5 px-3 py-2.5 rounded-lg bg-muted/50 border border-border/50">
                          <GitBranch className="w-4 h-4 text-muted-foreground shrink-0" />
                          <div className="min-w-0 flex-1">
                            <p className="text-xs font-medium truncate">{selectedRepo.full_name}</p>
                            {repoFolders.length > 0 && (
                              <p className="text-[10px] text-muted-foreground">
                                {t("publishModal.existingSubdirs", { count: repoFolders.length })}
                              </p>
                            )}
                          </div>
                          <button
                            onClick={() => { setPhase("pick-repo"); setSelectedRepo(null); }}
                            className="text-[10px] text-primary hover:underline cursor-pointer shrink-0"
                          >
                            {t("publishModal.change")}
                          </button>
                        </div>
                      )}

                      {/* Create new repo fields */}
                      {createNew && (
                        <>
                          <div className="space-y-2">
                            <label className="text-sm font-medium">{t("publishModal.repoName")}</label>
                            <input
                              value={newRepoName}
                              onChange={(e) => setNewRepoName(e.target.value)}
                              placeholder="my-skills"
                              className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium">{t("publishModal.description_label")}</label>
                            <textarea
                              value={newRepoDesc}
                              onChange={(e) => setNewRepoDesc(e.target.value)}
                              placeholder="SkillStar skills collection"
                              rows={3}
                              className="flex min-h-[72px] w-full rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring resize-y custom-scrollbar"
                            />
                          </div>
                          <div className="space-y-2">
                            <label className="text-sm font-medium">{t("publishModal.visibility")}</label>
                            <div className="flex gap-2">
                              <button
                                onClick={() => setIsPublic(true)}
                                className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg border text-sm font-medium transition-all cursor-pointer ${
                                  isPublic ? "border-primary bg-primary/10 text-primary" : "border-border hover:bg-muted text-muted-foreground"
                                }`}
                              >
                                <Globe className="w-4 h-4" />{t("publishModal.public")}
                              </button>
                              <button
                                onClick={() => setIsPublic(false)}
                                className={`flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg border text-sm font-medium transition-all cursor-pointer ${
                                  !isPublic ? "border-primary bg-primary/10 text-primary" : "border-border hover:bg-muted text-muted-foreground"
                                }`}
                              >
                                <Lock className="w-4 h-4" />{t("publishModal.private")}
                              </button>
                            </div>
                          </div>
                        </>
                      )}

                      {/* Folder name — always shown */}
                      <div className="space-y-2">
                        <label className="text-sm font-medium">{t("publishModal.folderName")}</label>
                        <input
                          value={folderName}
                          onChange={(e) => setFolderName(e.target.value)}
                          placeholder={skillName}
                          className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                        />
                        <p className="text-[11px] text-muted-foreground">
                          {t("publishModal.folderHint")}
                        </p>
                      </div>

                      {error && (
                        <div className="text-xs text-destructive bg-destructive/10 p-2.5 rounded-md border border-destructive/20 mt-2">
                          {error}
                        </div>
                      )}
                    </div>

                    {/* Right side: Structure preview (always visible in form phase) */}
                    <div className="w-[300px] shrink-0 border-l border-border/40 pl-6 h-full flex flex-col pt-0.5">
                      <div className="flex items-center gap-1.5 text-sm font-medium text-foreground mb-3 shrink-0">
                        {t("publishModal.structurePreview")}
                      </div>
                      <div className="rounded-xl border border-border/80 bg-muted/20 p-4 font-mono text-[12px] text-foreground/80 space-y-1 shadow-sm overflow-y-auto flex-1 min-h-0 custom-scrollbar">
                        <div className="flex items-center gap-1.5 font-semibold text-foreground">
                          <FolderOpen className="w-4 h-4 text-primary/80" />
                          <span>{createNew ? newRepoName || "my-skills" : selectedRepo?.full_name.split("/")[1] || "repo"}/</span>
                        </div>
                        {/* Existing folders */}
                        {repoFolders.filter(f => f !== folderName).map((f) => (
                          <div key={f} className="flex items-center gap-1.5 pl-5 text-muted-foreground/60 py-0.5">
                            <FolderOpen className="w-3.5 h-3.5 opacity-70" />
                            <span>{f}/</span>
                          </div>
                        ))}
                        {/* New skill folder — highlighted */}
                        <div className="flex items-center gap-1.5 pl-5 text-primary font-medium py-1 bg-primary/10 -mx-2 px-2 rounded-md my-0.5">
                          <FolderOpen className="w-3.5 h-3.5" />
                          <span>{folderName || skillName}/</span>
                          <span className="text-[9.5px] bg-primary/15 text-primary px-1.5 py-0.5 rounded-sm font-normal ml-auto uppercase tracking-wide">
                            {repoFolders.includes(folderName) ? t("publishModal.updateLabel") : t("publishModal.newLabel")}
                          </span>
                        </div>
                        {/* Files inside skill folder */}
                        {(skillFiles.length > 0 ? skillFiles : ["SKILL.md"]).map((file, i) => {
                          const isSkillMd = file === "SKILL.md";
                          return (
                            <div key={i} className={`flex items-center gap-1.5 pl-10 py-0.5 ${isSkillMd ? "text-primary/90 font-medium" : "text-muted-foreground/80"}`}>
                              <FileText className={`w-3.5 h-3.5 ${isSkillMd ? "text-primary/70" : "opacity-60"}`} />
                              <span>{file}</span>
                            </div>
                          );
                        })}
                        {createNew && (
                          <div className="flex items-center gap-1.5 pl-5 text-muted-foreground/60 py-0.5 mt-1">
                            <FileText className="w-3.5 h-3.5 opacity-70" />
                            <span>README.md</span>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                )}

                {/* Phase: Publishing */}
                {phase === "publishing" && (
                  <div className="flex flex-col items-center justify-center py-8 gap-3">
                    <div className="relative">
                      <Loader2 className="w-8 h-8 animate-spin text-primary" />
                      <Github className="w-4 h-4 absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 text-primary/60" />
                    </div>
                    <p className="text-sm text-muted-foreground">
                      {t("publishModal.publishing", { name: folderName })}
                    </p>
                    <p className="text-xs text-muted-foreground/60">
                      {selectedRepo
                        ? t("publishModal.addingTo", { repo: selectedRepo.full_name })
                        : t("publishModal.creatingAndPush", { name: newRepoName })}
                    </p>
                  </div>
                )}

                {/* Phase: Done */}
                {phase === "done" && result && (
                  <div className="space-y-4 animate-in fade-in zoom-in-95">
                    <div className="flex items-center gap-3 py-3">
                      <div className="w-10 h-10 rounded-full bg-success/10 flex items-center justify-center">
                        <Check className="w-5 h-5 text-success" />
                      </div>
                      <div>
                        <p className="text-sm font-semibold text-foreground">{t("publishModal.published")}</p>
                        <p className="text-xs text-muted-foreground">
                          <span className="font-medium">{result.source_folder}/</span> → {result.url.split("/").slice(-2).join("/")}
                        </p>
                      </div>
                    </div>

                    <div className="bg-muted rounded-lg px-3 py-2.5 flex items-center gap-2 border border-border/50">
                      <code className="text-xs font-mono text-foreground/80 flex-1 truncate select-all">
                        {result.url}
                      </code>
                      <button
                        onClick={() => handleCopy(result.url)}
                        className="p-1 rounded hover:bg-card-hover text-muted-foreground cursor-pointer"
                      >
                        {copied ? <Check className="w-3.5 h-3.5 text-success" /> : <Copy className="w-3.5 h-3.5" />}
                      </button>
                    </div>

                    <a
                      href={result.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex items-center gap-2 text-xs text-primary hover:underline justify-center py-1"
                    >
                      <ExternalLink className="w-3.5 h-3.5" />
                      {t("publishModal.openOnGithub")}
                    </a>
                  </div>
                )}
              </div>

              {/* Footer */}
              {(phase === "form" || phase === "done") && (
                <div className="flex justify-end gap-2 px-6 py-3.5 border-t border-border/60 bg-muted/20 shrink-0 mt-auto">
                  {phase === "form" && (
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => setPhase("pick-repo")}
                      className="mr-auto text-muted-foreground hover:text-foreground pl-2"
                    >
                      <ChevronLeft className="w-4 h-4 mr-1" />
                      {t("publishModal.back")}
                    </Button>
                  )}
                  <Button variant="ghost" size="sm" onClick={handleClose}>
                    {phase === "done" ? t("common.close") : t("common.cancel")}
                  </Button>
                  {phase === "form" && (
                    <Button
                      size="sm"
                      onClick={handlePublish}
                      disabled={
                        !folderName.trim() ||
                        (createNew && !newRepoName.trim()) ||
                        loadingFolders
                      }
                    >
                      <Github className="w-4 h-4 mr-2" />
                      {selectedRepo ? t("publishModal.pushToRepo") : t("publishModal.createAndPush")}
                    </Button>
                  )}
                </div>
              )}
            </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}
