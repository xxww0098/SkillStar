import { Terminal } from "lucide-react";
import { Popover } from "radix-ui";
import { useTranslation } from "react-i18next";
import { cn } from "../../../lib/utils";
import { ConnectionConsole } from "./ConnectionConsole";
import type { PendingHostKey } from "../hooks/useConnectStream";
import type { SshProgressLine } from "../hooks/useConnectStream";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  lines: SshProgressLine[];
  pendingHostKey: PendingHostKey | null;
  active: boolean;
  onAcceptHostKey: (fingerprint: string) => void | Promise<void>;
  onRejectHostKey: () => void;
  /** Highlight trigger when host key needs trust */
  attention?: boolean;
}

export function RemoteConnectionLogPopover({
  open,
  onOpenChange,
  lines,
  pendingHostKey,
  active,
  onAcceptHostKey,
  onRejectHostKey,
  attention = false,
}: Props) {
  const { t } = useTranslation();
  const hasActivity = lines.length > 0 || pendingHostKey != null || active;

  return (
    <Popover.Root open={open} onOpenChange={onOpenChange}>
      <Popover.Trigger asChild>
        <button
          type="button"
          title={t("ssh.console.toggle")}
          className={cn(
            "relative flex h-8 w-8 items-center justify-center rounded-lg border border-border/80 bg-background/50 text-foreground/80 shadow-sm backdrop-blur-md hover:bg-accent/10 shrink-0 focus-ring",
            open && "border-primary/40 bg-primary/10 text-foreground",
            attention && !open && "border-warning/50 text-warning",
          )}
        >
          <Terminal className="size-3.5" />
          {attention && <span className="absolute -right-0.5 -top-0.5 size-2 rounded-full bg-warning animate-pulse" />}
          {hasActivity && !attention && (
            <span className="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-primary/80" />
          )}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          side="bottom"
          align="end"
          sideOffset={8}
          className="z-50 w-[min(420px,calc(100vw-2rem))] rounded-xl border border-border bg-card/95 p-3 shadow-xl backdrop-blur-xl animate-in fade-in-0 zoom-in-95"
        >
          <p className="mb-2 text-xs font-semibold text-foreground">{t("ssh.console.title")}</p>
          <ConnectionConsole
            lines={lines}
            pendingHostKey={pendingHostKey}
            active={active}
            onAcceptHostKey={onAcceptHostKey}
            onRejectHostKey={onRejectHostKey}
          />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
