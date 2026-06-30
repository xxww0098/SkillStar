import { ExternalLink } from "lucide-react";
import { ExternalAnchor } from "@/components/ui/ExternalAnchor";
import { cn } from "@/lib/utils";
import type { CatalogEntry } from "../../types";
import type { CookieHelp } from "./cookieHelp";

interface CookieFieldProps {
  catalogId: string;
  selectedEntry: CatalogEntry | null;
  cookieHelp: CookieHelp;
  cookieHeader: string;
  setCookieHeader: (value: string) => void;
  planTier: string;
  setPlanTier: (value: string) => void;
}

/** Cookie-header paste box, with the OpenCode Go/Zen plan selector. */
export function CookieField({
  catalogId,
  selectedEntry,
  cookieHelp,
  cookieHeader,
  setCookieHeader,
  planTier,
  setPlanTier,
}: CookieFieldProps) {
  return (
    <div className="space-y-2.5 rounded-2xl border border-border bg-muted/30 p-3.5">
      <div className="flex items-start justify-between gap-3">
        <div>
          <h4 className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">🍪 浏览器 Cookie</h4>
          <p className="mt-1 text-[10px] leading-snug text-muted-foreground/75">{cookieHelp.title}</p>
        </div>
        {selectedEntry?.subscription_url && (
          <ExternalAnchor
            href={selectedEntry.subscription_url}
            className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-border bg-background/70 px-2 py-1.5 text-[10px] font-semibold text-foreground transition-colors hover:border-primary/40 hover:bg-primary/10 hover:text-primary"
            title={cookieHelp.openLabel}
          >
            <ExternalLink className="h-3 w-3" />
            {cookieHelp.openLabel}
          </ExternalAnchor>
        )}
      </div>

      {/* OpenCode: Go / Zen 选择器 */}
      {catalogId === "opencode" && (
        <div className="rounded-xl border border-border bg-background/60 p-2.5">
          <p className="mb-2 text-[10px] font-semibold text-muted-foreground">选择订阅</p>
          <div className="flex gap-1.5 rounded-lg border border-border bg-muted/50 p-1">
            {["Go", "Zen"].map((choice) => (
              <button
                key={choice}
                type="button"
                onClick={() => setPlanTier(choice)}
                className={cn(
                  "flex-1 rounded-md border px-3 py-1.5 text-[11px] font-bold tracking-wide transition-all duration-200",
                  planTier === choice
                    ? "border-primary/40 bg-primary/10 text-primary shadow-sm"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                )}
              >
                {choice === "Go" ? "🚀 Go" : "⚡ Zen"}
              </button>
            ))}
          </div>
          <p className="mt-1.5 text-[9px] leading-snug text-muted-foreground/70">
            {planTier === "Go" ? "$10/月 开源模型订阅" : planTier === "Zen" ? "按量付费 AI 网关" : "请选择 Go 或 Zen"}
          </p>
        </div>
      )}

      <p className="text-[10px] leading-relaxed text-muted-foreground">
        {cookieHelp.intro}
        {cookieHelp.requestTargets.map((target, index) => (
          <span key={target}>
            {index > 0 ? " 或 " : ""}
            <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">{target}</code>
          </span>
        ))}
        {cookieHelp.outro}
        <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">Copy as cURL</code>
        {cookieHelp.copyHint}
        <code className="mx-0.5 rounded bg-muted px-1 text-[10px]">Cookie:</code>
        后面的完整内容。
      </p>
      <textarea
        value={cookieHeader}
        onChange={(e) => setCookieHeader(e.target.value)}
        placeholder="sessionid=abc123; csrftoken=xyz789; ..."
        rows={3}
        className="w-full rounded-xl border border-input-border bg-input p-2.5 text-xs text-foreground placeholder:text-muted-foreground/60 resize-none focus:border-primary/50 focus:outline-none focus:ring-1 focus:ring-primary/40"
      />
      <p className="text-[9px] leading-normal text-muted-foreground/70">
        Cookie 会加密存储在本机。粘贴后点击「添加」即可保存，之后可点击刷新按钮拉取最新用量数据。
      </p>
    </div>
  );
}
