import { Copy } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { cn } from "../../../../lib/utils";
import { buildClaudeLaunchCommand, type ClaudeCommandShell } from "../../lib/launchCommand";

/** Claude Code launch-command block: shell toggle + generated function + copy. */
export function AgentLaunchCommand({ model }: { model: string }) {
  const { t } = useTranslation();
  const [shell, setShell] = useState<ClaudeCommandShell>("unix");
  const command = buildClaudeLaunchCommand(model, shell);

  const handleCopy = useCallback(async () => {
    if (!command || typeof navigator === "undefined" || !navigator.clipboard) return;
    try {
      await navigator.clipboard.writeText(command);
    } catch {
      /* clipboard unavailable in some shells */
    }
  }, [command]);

  return (
    <div className="space-y-2 rounded-lg border border-border/40 bg-background/35 px-2.5 py-2">
      <div className="flex items-center gap-1.5">
        {(["unix", "powershell"] as const).map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => setShell(s)}
            className={cn(
              "h-6 rounded-md border px-2 text-[11px] font-medium transition-colors",
              shell === s
                ? "border-primary/45 bg-primary/10 text-primary"
                : "border-border/50 text-muted-foreground hover:text-foreground",
            )}
          >
            {s === "unix" ? "macOS / Linux" : "Windows"}
          </button>
        ))}
        <Button type="button" variant="ghost" size="sm" onClick={handleCopy} className="ml-auto h-6 px-2 text-[11px]">
          <Copy className="mr-1 h-3 w-3" />
          {t("models.launch.copyCommand")}
        </Button>
      </div>
      <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md bg-muted/40 p-2 font-mono text-[10px] leading-relaxed text-muted-foreground">
        {command}
      </pre>
    </div>
  );
}
