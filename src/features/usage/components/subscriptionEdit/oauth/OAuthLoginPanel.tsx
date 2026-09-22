import { motion } from "framer-motion";
import { Check, Copy, ExternalLink, Loader2 } from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { CatalogEntry, OAuthStart } from "../../../types";

interface OAuthLoginPanelProps {
  selectedEntry: CatalogEntry;
  submitting: boolean;
  oauthIsActiveMode: boolean;
  oauthStart: OAuthStart | null;
  oauthPendingId: string | null;
  oauthStatus: string | null;
  oauthCallbackInput: string;
  setOauthCallbackInput: (value: string) => void;
  oauthSubmittingCallback: boolean;
  reduceMotion: boolean | null;
  onStartOAuth: () => void;
  onCopyAuthLink: () => void;
  onOpenOAuthLink: () => void;
  onSubmitCallback: () => void;
  onCancelOAuth: () => void;
}

type StepState = "upcoming" | "active" | "done";

function formatCountdown(secs: number): string {
  return `${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
}

function OAuthStep({
  index,
  state,
  label,
  children,
}: {
  index: number;
  state: StepState;
  label: string;
  children: ReactNode;
}) {
  return (
    <li className="flex gap-2.5">
      <span
        className={cn(
          "mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full border text-[10px] font-semibold transition-colors",
          state === "done" && "border-transparent bg-foreground/85 text-background",
          state === "active" && "border-foreground/35 text-foreground",
          state === "upcoming" && "border-border text-muted-foreground",
        )}
      >
        {state === "done" ? <Check className="h-3 w-3" /> : index}
      </span>
      <div className="min-w-0 flex-1 space-y-2">
        <p
          className={cn("text-xs leading-relaxed", state === "upcoming" ? "text-muted-foreground" : "text-foreground")}
        >
          {label}
        </p>
        {children}
      </div>
    </li>
  );
}

/** OAuth login panel: guided steps — generate link, open page, paste callback fallback. */
export function OAuthLoginPanel({
  selectedEntry,
  submitting,
  oauthIsActiveMode,
  oauthStart,
  oauthPendingId,
  oauthStatus,
  oauthCallbackInput,
  setOauthCallbackInput,
  oauthSubmittingCallback,
  reduceMotion,
  onStartOAuth,
  onCopyAuthLink,
  onOpenOAuthLink,
  onSubmitCallback,
  onCancelOAuth,
}: OAuthLoginPanelProps) {
  const { t } = useTranslation();
  const callbackDisabled = !oauthPendingId || oauthSubmittingCallback;
  const provider = selectedEntry.display_name;
  const [remainingSecs, setRemainingSecs] = useState<number | null>(null);

  useEffect(() => {
    const expiresIn = oauthStart?.expires_in_secs;
    if (!expiresIn) {
      setRemainingSecs(null);
      return;
    }
    const deadline = Date.now() + expiresIn * 1000;
    const tick = () => setRemainingSecs(Math.max(0, Math.ceil((deadline - Date.now()) / 1000)));
    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [oauthStart]);

  return (
    <section className="rounded-2xl border border-border bg-muted/40 px-4 py-4">
      <header className="space-y-0.5">
        <p className="text-[13px] font-semibold text-foreground">{t("usage.oauthPanelTitle", { provider })}</p>
        <p className="text-[11px] leading-relaxed text-muted-foreground">{t("usage.oauthPanelDesc", { provider })}</p>
      </header>

      <ol className="mt-3.5 list-none space-y-3.5">
        <OAuthStep index={1} state={oauthStart ? "done" : "active"} label={t("usage.oauthStepGenerate", { provider })}>
          {oauthStart ? (
            <div className="flex items-center gap-1 rounded-lg border border-border/70 bg-background/70 py-1 pl-2.5 pr-1">
              <code className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">
                {oauthStart.auth_url}
              </code>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                onClick={onCopyAuthLink}
                title={t("usage.oauthCopyLink")}
                aria-label={t("usage.oauthCopyLink")}
              >
                <Copy />
              </Button>
            </div>
          ) : (
            <Button type="button" size="sm" onClick={onStartOAuth} disabled={submitting}>
              {submitting && oauthIsActiveMode ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <ExternalLink className="h-3.5 w-3.5" />
              )}
              {t("usage.oauthStartLogin")}
            </Button>
          )}
        </OAuthStep>

        <OAuthStep index={2} state={oauthStart ? "active" : "upcoming"} label={t("usage.oauthStepAuthorize")}>
          <Button type="button" variant="outline" size="sm" onClick={onOpenOAuthLink} disabled={!oauthStart}>
            {t("usage.oauthOpenLink")}
            <ExternalLink className="h-3.5 w-3.5" />
          </Button>
        </OAuthStep>

        <OAuthStep index={3} state={oauthPendingId ? "active" : "upcoming"} label={t("usage.oauthStepFallback")}>
          <div className="flex flex-col gap-2 sm:flex-row">
            <Input
              value={oauthCallbackInput}
              onChange={(e) => setOauthCallbackInput(e.target.value)}
              placeholder={t("usage.oauthCallbackPlaceholder")}
              disabled={callbackDisabled}
              className="h-8 rounded-lg border-input-border bg-input text-xs text-foreground placeholder:text-foreground/45 disabled:opacity-100 disabled:bg-muted/50 disabled:text-foreground/55 disabled:placeholder:text-foreground/40"
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={onSubmitCallback}
              disabled={callbackDisabled || !oauthCallbackInput.trim()}
              className="shrink-0 disabled:opacity-100 disabled:border-border disabled:bg-muted/40 disabled:text-foreground/50"
            >
              {oauthSubmittingCallback && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
              {t("usage.oauthSubmitCallback")}
            </Button>
          </div>
          <p className="text-[10px] leading-relaxed text-muted-foreground/80">{t("usage.oauthCallbackHint")}</p>
        </OAuthStep>
      </ol>

      {oauthStatus && (
        <p className="mt-3.5 flex items-center gap-2 text-[11px] text-muted-foreground">
          <motion.span
            className="h-1.5 w-1.5 shrink-0 rounded-full bg-foreground/70"
            animate={reduceMotion ? undefined : { opacity: [0.35, 1, 0.35] }}
            transition={reduceMotion ? undefined : { duration: 1.6, repeat: Infinity, ease: "easeInOut" }}
          />
          <span className="min-w-0">
            {oauthStatus} · {t("usage.oauthKeepOpen")}
            {remainingSecs != null && (
              <span className="font-mono tabular-nums">
                {" "}
                · {t("usage.oauthExpiresIn", { time: formatCountdown(remainingSecs) })}
              </span>
            )}
          </span>
        </p>
      )}

      {oauthPendingId && (
        <div className="mt-3 flex items-center justify-between gap-3 border-t border-border/60 pt-3">
          <p className="text-[10px] leading-relaxed text-muted-foreground/80">{t("usage.oauthWaitingHint")}</p>
          <button
            type="button"
            className="shrink-0 rounded-md px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground transition-colors hover:text-foreground hover:underline underline-offset-2"
            onClick={onCancelOAuth}
          >
            {t("usage.cancelOAuth")}
          </button>
        </div>
      )}
    </section>
  );
}
