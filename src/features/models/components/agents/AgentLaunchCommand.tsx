import { Copy } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../../components/ui/button";
import { buildClaudeLaunchCommand, type ClaudeCommandShell } from "../../lib/launchCommand";
import { ModelSegmentedControl } from "../providerForm/ProviderConfigPrimitives";

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
    <div className="space-y-2.5">
      <div className="flex items-center gap-2">
        <ModelSegmentedControl
          value={shell}
          onChange={setShell}
          ariaLabel={t("models.launch.shell")}
          className="max-w-xs flex-1"
          options={[
            { value: "unix", label: "macOS / Linux" },
            { value: "powershell", label: "Windows" },
          ]}
        />
        <Button type="button" variant="outline" size="sm" onClick={handleCopy} className="shrink-0">
          <Copy className="mr-1 h-3 w-3" />
          {t("models.launch.copyCommand")}
        </Button>
      </div>
      <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-[10px] border border-border/45 bg-muted/35 p-3 font-mono text-[11px] leading-5 text-muted-foreground">
        {command}
      </pre>
    </div>
  );
}
