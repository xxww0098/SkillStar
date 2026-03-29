import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, CheckCircle, Copy, ExternalLink, Terminal, XCircle } from "lucide-react";
import { Button } from "../../components/ui/button";
import { Badge } from "../../components/ui/badge";

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
  const ghInstallPlatform = useMemo(() => detectPlatform(), []);
  const ghInstallCommand =
    ghInstallPlatform === "unknown" ? null : GH_INSTALL_COMMANDS[ghInstallPlatform];
  const platformPaths = [
    { platform: "Windows", path: "%APPDATA%\\skillstar\\" },
    { platform: "Linux", path: "~/.local/share/skillstar/" },
    { platform: "macOS", path: "~/.skillstar/" },
  ] as const;

  const handleCopy = async () => {
    if (!ghInstallCommand) return;
    try {
      await navigator.clipboard.writeText(ghInstallCommand);
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch {
      // Ignore clipboard failures.
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
                  className="p-1 rounded hover:bg-muted transition-colors text-muted-foreground cursor-pointer"
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

        <div className="px-4 py-3">
          <div className="flex justify-between text-sm">
            <span className="text-muted-foreground font-medium">{t("settings.version")}</span>
            <span className="font-mono">0.1.0</span>
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
