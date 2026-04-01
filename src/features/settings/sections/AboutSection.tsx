import { useMemo, useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { copyToClipboard } from "../../../lib/utils";
import { useTranslation } from "react-i18next";
import { Check, CheckCircle, Copy, ExternalLink, Terminal, XCircle, RefreshCw } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { Badge } from "../../../components/ui/badge";
import { toast } from "sonner";

interface AboutSectionProps {
  ghInstalled: boolean | null;
}

type GhInstallPlatform = "macos" | "windows" | "linux" | "unknown";

const GH_INSTALL_COMMANDS: Record<Exclude<GhInstallPlatform, "unknown">, string> = {
  macos: "brew install gh",
  windows: "winget install --id GitHub.cli",
  linux: "sudo apt install gh",
};

function detectPlatform(): GhInstallPlatform {
  if (typeof navigator === "undefined") return "unknown";
  const source = `${navigator.userAgent} ${navigator.platform}`.toLowerCase();
  if (source.includes("mac")) return "macos";
  if (source.includes("win")) return "windows";
  if (source.includes("linux")) return "linux";
  return "unknown";
}

export function AboutSection({ ghInstalled }: AboutSectionProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [appVersion, setAppVersion] = useState<string>("...");
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);

  useEffect(() => {
    getVersion()
      .then((v) => setAppVersion(v))
      .catch(() => setAppVersion("unknown"));
  }, []);

  const ghInstallPlatform = useMemo(() => detectPlatform(), []);
  const ghInstallCommand =
    ghInstallPlatform === "unknown" ? null : GH_INSTALL_COMMANDS[ghInstallPlatform];
  const platformPaths = [
    { platform: "Windows", path: "%USERPROFILE%\\.skillstar\\" },
    { platform: "Linux", path: "~/.skillstar/" },
    { platform: "macOS", path: "~/.skillstar/" },
  ] as const;

  const handleCopy = async () => {
    if (!ghInstallCommand) return;
    const copySuccess = await copyToClipboard(ghInstallCommand);
    if (copySuccess) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    }
  };

  const handleCheckUpdate = async () => {
    if (isCheckingUpdate) return;
    setIsCheckingUpdate(true);
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check();
      if (update) {
        toast.success(t("sidebar.newUpdate"), {
          description: t("sidebar.updateAvailable", { version: update.version }),
        });
      } else {
        toast.info(t("settings.upToDate"));
      }
    } catch (e) {
      toast.error(t("sidebar.updateError"));
    } finally {
      setIsCheckingUpdate(false);
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
            <Button
              size="sm"
              variant="outline"
              onClick={() => window.open("https://cli.github.com/", "_blank")}
            >
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
                  onClick={handleCopy}
                  className="p-1.5 rounded hover:bg-muted transition-colors text-muted-foreground cursor-pointer focus-ring"
                  title={t("settings.ghCopyCommand")}
                >
                  {copied ? <Check className="w-3.5 h-3.5 text-success" /> : <Copy className="w-3.5 h-3.5" />}
                </button>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground">
                {t("settings.ghInstallCommandUnavailable")}
              </p>
            )}
          </div>
        )}

        <div className="px-4 py-4 relative overflow-hidden group/version">
          {/* Subtle gradient background effect */}
          <div className="absolute inset-0 bg-gradient-to-r from-primary/5 via-transparent to-transparent opacity-0 group-hover/version:opacity-100 transition-opacity duration-500" />
          
          <div className="relative flex justify-between items-center text-sm">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2.5">
                <span className="text-foreground font-medium">{t("settings.version")}</span>
                <div className="px-2 py-0.5 rounded-md bg-zinc-500/10 border border-zinc-500/20 text-zinc-600 dark:text-zinc-400 font-mono text-xs shadow-sm flex items-center gap-1.5">
                  <div className="w-1.5 h-1.5 rounded-full bg-emerald-500 shadow-[0_0_8px_rgba(16,185,129,0.5)]" />
                  {appVersion}
                </div>
              </div>
            </div>
            <Button 
              size="sm" 
              variant="secondary" 
              className="h-8 text-xs px-3.5 rounded-full shadow-sm hover:shadow-md transition-all active:scale-95" 
              onClick={handleCheckUpdate} 
              disabled={isCheckingUpdate}
            >
              <RefreshCw className={`w-3.5 h-3.5 mr-2 ${isCheckingUpdate ? "animate-spin text-primary" : "text-muted-foreground"}`} />
              {isCheckingUpdate ? t("settings.checkingUpdate") : t("settings.checkUpdate")}
            </Button>
          </div>
        </div>

        <div className="px-4 py-3 space-y-1.5">
          {platformPaths.map((item) => (
            <div key={item.platform} className="flex items-center gap-2 text-xs">
              <span className="text-muted-foreground min-w-[56px]">{item.platform}</span>
              <code className="font-mono text-caption">{item.path}</code>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
