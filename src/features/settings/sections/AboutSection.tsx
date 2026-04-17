import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";
import { motion } from "framer-motion";
import {
  Check,
  CheckCircle,
  Copy,
  ExternalLink,
  FolderOpen,
  GitBranch,
  RefreshCw,
  Sparkles,
  Terminal,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { openExternalUrl } from "../../../lib/externalOpen";
import { toast } from "../../../lib/toast";
import { copyToClipboard, detectPlatform, type Platform, resolveSkillstarDataPath } from "../../../lib/utils";
import type { GitStatus } from "../../../types";

interface AboutSectionProps {
  ghInstalled: boolean | null;
  onCheckUpdate?: () => Promise<{ found: boolean; version?: string; error?: boolean }>;
  isCheckingUpdate?: boolean;
}

type GhInstallPlatform = Platform;

const GH_INSTALL_COMMANDS: Record<Exclude<GhInstallPlatform, "unknown">, string> = {
  macos: "brew install gh",
  windows: "winget install --id GitHub.cli",
  linux: "sudo apt install gh",
};

export function AboutSection({ ghInstalled, onCheckUpdate, isCheckingUpdate = false }: AboutSectionProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState<string | null>(null);
  const [appVersion, setAppVersion] = useState<string>("...");
  const [resolvedDataPath, setResolvedDataPath] = useState<string | null>(null);

  // Git status
  const [gitStatus, setGitStatus] = useState<GitStatus | null>(null);

  useEffect(() => {
    getVersion()
      .then((v) => setAppVersion(v))
      .catch(() => setAppVersion("unknown"));
  }, []);

  useEffect(() => {
    invoke<GitStatus>("check_git_status")
      .then(setGitStatus)
      .catch(() =>
        setGitStatus({
          status: "NotInstalled",
          os: "Unknown",
          install_instructions: [],
          download_url: "https://git-scm.com/downloads",
        }),
      );
  }, []);

  const ghInstallPlatform = useMemo(() => detectPlatform(), []);
  const ghInstallCommand = ghInstallPlatform === "unknown" ? null : GH_INSTALL_COMMANDS[ghInstallPlatform];

  useEffect(() => {
    let active = true;

    homeDir()
      .then((home) => {
        if (!active) return;
        setResolvedDataPath(resolveSkillstarDataPath(home, ghInstallPlatform));
      })
      .catch(() => {
        if (!active) return;
        setResolvedDataPath(null);
      });

    return () => {
      active = false;
    };
  }, [ghInstallPlatform]);

  const platformPaths = useMemo(() => {
    switch (ghInstallPlatform) {
      case "windows":
        return [
          {
            platform: "Windows",
            path: resolvedDataPath ?? "%USERPROFILE%\\.skillstar\\",
            openPath: resolvedDataPath,
          },
        ];
      case "linux":
        return [{ platform: "Linux", path: resolvedDataPath ?? "~/.skillstar/", openPath: resolvedDataPath }];
      case "macos":
        return [{ platform: "macOS", path: resolvedDataPath ?? "~/.skillstar/", openPath: resolvedDataPath }];
      default:
        return [];
    }
  }, [ghInstallPlatform, resolvedDataPath]);

  const handleOpenFolder = useCallback(
    async (path: string) => {
      try {
        await invoke("open_folder", { path });
      } catch (error) {
        console.error("Failed to open folder:", error);
        toast.error(t("settings.openFolderFailed"));
      }
    },
    [t],
  );

  const handleCopy = useCallback(async (text: string, id: string) => {
    const copySuccess = await copyToClipboard(text);
    if (copySuccess) {
      setCopied(id);
      setTimeout(() => setCopied(null), 1600);
    }
  }, []);

  const handleCheckUpdate = async () => {
    if (!onCheckUpdate) return;
    try {
      const result = await onCheckUpdate();
      if (result.error) {
        // Error state is already handled by the updater hook (sidebar banner)
        return;
      }
      if (result.found) {
        const { toast: sonnerToast } = await import("sonner");
        sonnerToast.success(t("sidebar.newUpdate"), {
          description: t("settings.updateFoundDesc", { version: result.version }),
        });
      } else {
        const { toast: sonnerToast } = await import("sonner");
        sonnerToast.info(t("settings.upToDate"));
      }
    } catch {
      // Error state is already handled by the updater hook
    }
  };

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-zinc-500/10 flex items-center justify-center shrink-0 border border-zinc-500/20">
          <Terminal className="w-4 h-4 text-zinc-500" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.about")}</h2>
      </div>
      <div className="rounded-xl border border-border bg-card divide-y divide-border overflow-hidden">
        {/* ── Git CLI ──────────────────────────────────────────── */}
        <div className="px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            {gitStatus === null ? (
              <div className="w-4 h-4 rounded-full bg-muted animate-pulse" />
            ) : gitStatus.status === "Installed" ? (
              <CheckCircle className="w-4 h-4 text-emerald-500" />
            ) : (
              <XCircle className="w-4 h-4 text-destructive" />
            )}
            <span className="text-sm font-medium">
              <GitBranch className="w-3.5 h-3.5 inline mr-1 opacity-60" />
              Git
            </span>
          </div>
          {gitStatus?.status === "Installed" && <Badge variant="success">{t("settings.gitInstalled")}</Badge>}
          {gitStatus?.status === "NotInstalled" && (
            <Button size="sm" variant="outline" onClick={() => void openExternalUrl(gitStatus.download_url)}>
              <ExternalLink className="w-3 h-3" />
              {t("settings.gitInstall")}
            </Button>
          )}
        </div>

        {gitStatus?.status === "NotInstalled" && gitStatus.install_instructions.length > 0 && (
          <div className="px-4 py-3 bg-muted/20 space-y-2">
            <div className="text-xs text-muted-foreground flex items-center gap-1.5">
              <Terminal className="w-3.5 h-3.5 shrink-0" />
              <span>{t("settings.gitInstallCommandLabel", { os: gitStatus.os })}</span>
            </div>
            {gitStatus.install_instructions.map((inst) => (
              <div
                key={inst.label}
                className="rounded-md border border-border bg-card px-2.5 py-2 flex items-center gap-2"
              >
                <div className="flex-1 min-w-0">
                  <div className="text-micro text-muted-foreground mb-0.5">{inst.label}</div>
                  <code className="font-mono text-xs text-foreground/85 select-all break-all">{inst.command}</code>
                </div>
                <button
                  type="button"
                  onClick={() => handleCopy(inst.command, `git-${inst.label}`)}
                  className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground cursor-pointer focus-ring shrink-0"
                  title={t("settings.ghCopyCommand")}
                >
                  {copied === `git-${inst.label}` ? (
                    <Check className="w-3.5 h-3.5 text-success" />
                  ) : (
                    <Copy className="w-3.5 h-3.5" />
                  )}
                </button>
              </div>
            ))}
          </div>
        )}

        {/* ── GitHub CLI ──────────────────────────────────────── */}
        <div className="px-4 py-3 flex items-center justify-between">
          <div className="flex items-center gap-3">
            {ghInstalled === null ? (
              <div className="w-4 h-4 rounded-full bg-muted animate-pulse" />
            ) : ghInstalled ? (
              <CheckCircle className="w-4 h-4 text-emerald-500" />
            ) : (
              <XCircle className="w-4 h-4 text-destructive" />
            )}
            <span className="text-sm font-medium">GitHub CLI</span>
          </div>
          {ghInstalled === false && (
            <Button size="sm" variant="outline" onClick={() => void openExternalUrl("https://cli.github.com/")}>
              <ExternalLink className="w-3 h-3" />
              {t("settings.ghInstall")}
            </Button>
          )}
          {ghInstalled && <Badge variant="success">{t("settings.ghInstalled")}</Badge>}
        </div>

        {ghInstalled === false && (
          <div className="px-4 py-3 bg-muted/20">
            <div className="text-xs text-muted-foreground flex items-center gap-1.5 mb-2">
              <Terminal className="w-3.5 h-3.5 shrink-0" />
              <span>
                {t("settings.ghInstallCommandLabel", {
                  platform: t(`settings.ghInstallPlatform_${ghInstallPlatform}`),
                })}
              </span>
            </div>
            {ghInstallCommand ? (
              <div className="rounded-md border border-border bg-card px-2.5 py-2 flex items-center gap-2">
                <code className="font-mono text-xs text-foreground/85 select-all flex-1">{ghInstallCommand}</code>
                <button
                  type="button"
                  onClick={() => handleCopy(ghInstallCommand, "gh")}
                  className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground cursor-pointer focus-ring"
                  title={t("settings.ghCopyCommand")}
                >
                  {copied === "gh" ? <Check className="w-3.5 h-3.5 text-success" /> : <Copy className="w-3.5 h-3.5" />}
                </button>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">{t("settings.ghInstallCommandUnavailable")}</p>
            )}
          </div>
        )}

        <div className="px-4 py-4 relative overflow-hidden group/version">
          {/* Subtle gradient background effect */}
          <div className="absolute inset-0 bg-gradient-to-r from-primary/5 via-transparent to-transparent opacity-0 group-hover/version:opacity-100 transition-opacity duration-500" />

          <div className="relative flex justify-between items-center text-sm">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-3">
                <span className="text-sm font-medium">{t("settings.version")}</span>

                <motion.div className="relative flex items-center justify-center rounded-full p-[2px] overflow-hidden select-none pointer-events-none shadow-sm">
                  <motion.div
                    className="absolute inset-0 bg-gradient-to-r from-blue-500 via-indigo-500 to-purple-500 opacity-70 dark:opacity-80"
                    animate={{ backgroundPosition: ["0% 50%", "100% 50%", "0% 50%"] }}
                    transition={{ duration: 4, repeat: Infinity, ease: "linear" }}
                    style={{ backgroundSize: "200% 200%" }}
                  />
                  <div className="relative bg-card dark:bg-card/90 rounded-full flex items-center px-3 py-1 z-10 overflow-hidden backdrop-blur-sm">
                    <span className="relative z-20 font-mono text-xs font-bold leading-none bg-gradient-to-br from-indigo-500 to-purple-500 dark:from-indigo-400 dark:to-purple-400 bg-clip-text text-transparent flex items-center tracking-wider">
                      <Sparkles className="w-3.5 h-3.5 text-indigo-500/80 dark:text-indigo-400/80 animate-pulse mr-1" />
                      v{appVersion}
                    </span>
                  </div>
                </motion.div>
              </div>
            </div>
            <Button
              size="sm"
              variant="secondary"
              className="h-8 text-xs px-3.5 rounded-full shadow-sm hover:shadow-md transition-all active:scale-95"
              onClick={handleCheckUpdate}
              disabled={isCheckingUpdate}
            >
              <RefreshCw
                className={`w-3.5 h-3.5 mr-2 ${isCheckingUpdate ? "animate-spin text-primary" : "text-muted-foreground"}`}
              />
              {isCheckingUpdate ? t("settings.checkingUpdate") : t("settings.checkUpdate")}
            </Button>
          </div>
        </div>

        {platformPaths.length > 0 && (
          <div className="px-4 py-3 space-y-1.5">
            {platformPaths.map((item) => (
              <div key={item.platform} className="flex items-center gap-2 text-xs">
                <span className="text-muted-foreground min-w-[56px]">{item.platform}</span>
                <code className="font-mono text-caption flex-1 break-all">{item.path}</code>
                {item.openPath ? (
                  <Button
                    size="icon"
                    variant="ghost"
                    className="h-7 w-7 text-muted-foreground hover:text-foreground"
                    title={t("settings.openFolder")}
                    onClick={() => {
                      if (item.openPath) {
                        void handleOpenFolder(item.openPath);
                      }
                    }}
                  >
                    <FolderOpen className="w-4 h-4" />
                  </Button>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
