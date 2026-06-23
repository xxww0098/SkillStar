import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { Loader2, ShieldAlert, ShieldCheck, Terminal } from "lucide-react";
import { Button } from "../../../components/ui/button";
import type { PendingHostKey, SshProgressLine } from "../hooks/useConnectStream";

interface Props {
  lines: SshProgressLine[];
  pendingHostKey: PendingHostKey | null;
  /** True while a query/mutation is in flight (shows a spinner line). */
  active: boolean;
  onAcceptHostKey: (fingerprint: string) => void;
  onRejectHostKey: () => void;
}

const STATUS_COLOR: Record<string, string> = {
  start: "text-zinc-300",
  ok: "text-emerald-400",
  warn: "text-amber-400",
  fail: "text-red-400",
  pending: "text-sky-400",
};

const PHASE_LABEL: Record<string, string> = {
  dial: "dial",
  handshake: "kex",
  host_key: "key",
  auth: "auth",
  sftp: "sftp",
  scan: "scan",
  done: "done",
  error: "err",
};

export function ConnectionConsole({ lines, pendingHostKey, active, onAcceptHostKey, onRejectHostKey }: Props) {
  const { t } = useTranslation();
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to the latest line.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lines.length, pendingHostKey]);

  const hasContent = lines.length > 0 || active || pendingHostKey;

  return (
    <div className="flex flex-col rounded-lg border border-zinc-700/60 bg-zinc-950 overflow-hidden">
      <div className="flex items-center gap-2 border-b border-zinc-700/60 px-3 py-1.5">
        <Terminal className="size-3.5 text-zinc-400" />
        <span className="text-[11px] font-medium uppercase tracking-wider text-zinc-400">
          {t("ssh.console.title")}
        </span>
        {active ? <Loader2 className="ml-auto size-3.5 animate-spin text-primary" /> : null}
      </div>

      {hasContent ? (
        <div className="max-h-[180px] min-h-[80px] overflow-y-auto px-3 py-2 font-mono text-[11px] leading-relaxed text-zinc-200">
          {lines.map((line, i) => (
            <ConsoleLine key={`${line.tsMs}-${i}`} line={line} />
          ))}
          {pendingHostKey ? (
            <HostKeyPrompt
              fingerprint={pendingHostKey.fingerprint}
              onAccept={() => onAcceptHostKey(pendingHostKey.fingerprint)}
              onReject={onRejectHostKey}
            />
          ) : null}
          <div ref={bottomRef} />
        </div>
      ) : (
        <div className="px-3 py-3 text-[11px] text-zinc-500">{t("ssh.console.empty")}</div>
      )}
    </div>
  );
}

function ConsoleLine({ line }: { line: SshProgressLine }) {
  const label = PHASE_LABEL[line.phase] ?? line.phase;
  const color = STATUS_COLOR[line.status] ?? "text-zinc-300";
  const time = new Date(line.tsMs).toLocaleTimeString(undefined, {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  return (
    <div className="flex gap-2">
      <span className="shrink-0 text-zinc-500">{time}</span>
      <span className="shrink-0 w-9 text-zinc-400">[{label}]</span>
      <span className={color}>{line.message}</span>
    </div>
  );
}

function HostKeyPrompt({
  fingerprint,
  onAccept,
  onReject,
}: {
  fingerprint: string;
  onAccept: () => void;
  onReject: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mt-2 flex flex-col gap-2 rounded border border-sky-500/30 bg-sky-500/5 p-2">
      <div className="flex items-center gap-1.5 text-sky-300">
        <ShieldCheck className="size-3.5" />
        <span>{t("ssh.console.hostKeyPrompt")}</span>
      </div>
      <code className="break-all text-[10px] text-sky-200/90">{fingerprint}</code>
      <div className="flex gap-2">
        <Button size="sm" variant="default" className="h-7 text-[11px]" onClick={onAccept}>
          <ShieldCheck className="size-3" />
          {t("ssh.acceptHostKey")}
        </Button>
        <Button size="sm" variant="ghost" className="h-7 text-[11px]" onClick={onReject}>
          <ShieldAlert className="size-3" />
          {t("ssh.console.reject")}
        </Button>
      </div>
    </div>
  );
}
