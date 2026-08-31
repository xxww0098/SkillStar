import { motion } from "framer-motion";
import { Copy, ExternalLink, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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

/** OAuth login panel: start button, auth link, callback input, status. */
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
  return (
    <div className="space-y-3 rounded-2xl border border-border bg-muted/40 p-3.5">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <p className="text-xs font-semibold text-foreground">{t("usage.oauthPanelTitle")}</p>
          <p className="mt-1 max-w-[62ch] text-[11px] leading-relaxed text-foreground/70">
            {t("usage.oauthPanelDesc", { provider: selectedEntry.display_name })}
          </p>
        </div>
        <Button
          type="button"
          size="sm"
          onClick={onStartOAuth}
          disabled={submitting || !!oauthPendingId}
          className="shrink-0"
        >
          {submitting && oauthIsActiveMode ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <ExternalLink className="h-3.5 w-3.5" />
          )}
          {oauthPendingId ? t("usage.btnWaitingLogin") : t("usage.oauthStartLogin")}
        </Button>
      </div>

      {oauthStart ? (
        <div className="rounded-xl border border-dashed border-border bg-background/70 p-3">
          <p className="text-[11px] font-semibold text-foreground/75">{t("usage.oauthAuthLink")}</p>
          <p className="mt-1 max-h-24 overflow-y-auto break-all rounded-lg bg-muted/60 px-2.5 py-2 font-mono text-[11px] leading-relaxed text-foreground">
            {oauthStart.auth_url}
          </p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button type="button" size="xs" variant="outline" onClick={onCopyAuthLink}>
              <Copy className="h-3 w-3" />
              {t("usage.oauthCopyLink")}
            </Button>
            <Button type="button" size="xs" variant="outline" onClick={onOpenOAuthLink}>
              <ExternalLink className="h-3 w-3" />
              {t("usage.oauthOpenLink")}
            </Button>
          </div>
        </div>
      ) : (
        <p className="rounded-xl border border-dashed border-border/80 bg-muted/50 px-3 py-2.5 text-[11px] leading-relaxed text-foreground/60">
          {t("usage.oauthLinkPlaceholder")}
        </p>
      )}

      <div className="space-y-1.5">
        <p className="text-[11px] font-semibold text-foreground">{t("usage.oauthCallbackLabel")}</p>
        <div className="flex flex-col gap-2 sm:flex-row">
          <Input
            value={oauthCallbackInput}
            onChange={(e) => setOauthCallbackInput(e.target.value)}
            placeholder={t("usage.oauthCallbackPlaceholder")}
            disabled={callbackDisabled}
            className="h-9 rounded-xl border-input-border bg-input text-xs text-foreground placeholder:text-foreground/45 disabled:opacity-100 disabled:bg-muted/50 disabled:text-foreground/55 disabled:placeholder:text-foreground/40"
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
        <p className="text-[10px] leading-relaxed text-foreground/60">{t("usage.oauthCallbackHint")}</p>
      </div>

      {oauthStatus && (
        <div className="relative flex items-center gap-2 overflow-hidden rounded-xl border border-primary/20 bg-primary/10 px-3 py-2 text-[10px] text-primary">
          <motion.span
            className="pointer-events-none absolute inset-y-0 left-0 w-1/3 bg-gradient-to-r from-transparent via-primary/15 to-transparent"
            animate={reduceMotion ? undefined : { x: ["-120%", "320%"] }}
            transition={reduceMotion ? undefined : { duration: 1.7, repeat: Infinity, ease: "linear" }}
          />
          <Loader2 className="relative h-3.5 w-3.5 animate-spin" />
          <div className="relative min-w-0 flex-1">
            <p className="font-semibold">{oauthStatus}</p>
            <p className="mt-0.5 text-[9px] text-primary/75">{t("usage.oauthWaitingHint")}</p>
          </div>
          {oauthPendingId && (
            <button
              type="button"
              className="relative shrink-0 rounded-full px-2 py-1 underline hover:bg-primary/10"
              onClick={onCancelOAuth}
            >
              {t("usage.cancelOAuth")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
