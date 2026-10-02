import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { cn, copyToClipboard } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";

/**
 * The two protocol entrypoints the gateway serves, each with a copy button.
 * Only the loopback origin is shown — the section stays hidden until the
 * read lands, and a failed read draws nothing.
 */
export function GatewayEndpoints() {
  const { t } = useTranslation();
  const { data: origin } = useQuery<string>({
    queryKey: modelsKeys.loopbackOrigin(),
    queryFn: () => tauriInvoke("get_loopback_origin"),
    staleTime: 60_000,
  });
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    if (copied === null) return;
    const timer = window.setTimeout(() => setCopied(null), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  if (!origin) return null;
  const rows = [
    { id: "chat", label: "OpenAI", path: "/v1/chat/completions" },
    { id: "messages", label: "Anthropic", path: "/v1/messages" },
  ];

  return (
    <div className="space-y-2">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.endpoints")}
      </div>
      <ul className="space-y-1" aria-label={t("models.gateway.endpoints")}>
        {rows.map((row) => (
          <li
            key={row.id}
            className="flex min-w-0 items-center gap-1.5 rounded-lg border border-border/60 bg-muted/20 px-2 py-1.5"
          >
            <span className="shrink-0 rounded bg-background/70 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
              {row.label}
            </span>
            <code
              className="min-w-0 flex-1 truncate font-mono text-[11px] text-foreground"
              title={`${origin}${row.path}`}
            >
              {origin.replace(/^https?:\/\//, "")}
              {row.path}
            </code>
            <button
              type="button"
              aria-label={t("models.gateway.copyEndpoint", { protocol: row.label })}
              title={t("models.gateway.copyEndpoint", { protocol: row.label })}
              onClick={() => {
                void copyToClipboard(`${origin}${row.path}`).then((ok) => {
                  if (ok) setCopied(row.id);
                });
              }}
              className={cn(
                "flex h-6 w-6 shrink-0 items-center justify-center rounded-md transition",
                copied === row.id ? "text-success" : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
              )}
            >
              {copied === row.id ? <Check className="h-3 w-3" /> : <Copy className="h-3 w-3" />}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
