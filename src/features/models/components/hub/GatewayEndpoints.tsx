import { Check, Copy } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import { cn, copyToClipboard } from "@/lib/utils";
import { tauriInvoke } from "@/lib/ipc";
import { modelsKeys } from "../../api/keys";

/**
 * The four protocol entrypoints the gateway serves, one row per API
 * dialect. Each row is itself the copy target — click anywhere on it —
 * with the explicit copy affordance landing under the pointer on hover.
 * Only the loopback origin is shown; the block stays hidden until the read
 * lands, and a failed read draws nothing. The Gemini path keeps its
 * `{model}` placeholder: the caller substitutes the model it wants.
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
    { id: "responses", label: "Responses", path: "/v1/responses" },
    { id: "messages", label: "Anthropic", path: "/v1/messages" },
    { id: "gemini", label: "Gemini", path: "/v1beta/models/{model}:generateContent" },
  ];

  return (
    <div className="space-y-1.5 px-3 pb-3">
      <div className="px-1 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.endpoints")}
      </div>
      <ul className="grid gap-1.5 sm:grid-cols-2" aria-label={t("models.gateway.endpoints")}>
        {rows.map((row) => {
          const done = copied === row.id;
          return (
            <li key={row.id}>
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
                  "group flex w-full min-w-0 cursor-pointer items-center gap-2 rounded-lg px-2.5 py-2 text-left transition-colors",
                  "bg-muted/35 hover:bg-muted/70",
                  done && "bg-success/10",
                )}
              >
                <span className="w-[64px] shrink-0 text-[11px] font-medium text-muted-foreground">{row.label}</span>
                <code
                  className="min-w-0 flex-1 truncate font-mono text-[11.5px] tabular-nums text-foreground"
                  title={`${origin}${row.path}`}
                >
                  {origin.replace(/^https?:\/\//, "")}
                  {row.path}
                </code>
                <span
                  aria-hidden
                  className={cn(
                    "flex h-5 w-5 shrink-0 items-center justify-center transition",
                    done ? "text-success" : "text-muted-foreground/0 group-hover:text-muted-foreground",
                  )}
                >
                  {done ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
