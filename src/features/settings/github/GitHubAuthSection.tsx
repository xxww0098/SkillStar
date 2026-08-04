import { CheckCircle2, CircleUserRound, Loader2, LogOut, RefreshCw, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { openExternalUrl } from "../../../lib/externalOpen";
import { useGitHubAuth } from "./useGitHubAuth";

export function GitHubAuthSection() {
  const { t } = useTranslation();
  const auth = useGitHubAuth();
  const errorMessage =
    auth.error?.code === "proxy" || auth.error?.code === "network"
      ? t("settings.githubAuthProxyError")
      : auth.error?.message;

  return (
    <section>
      <div className="flex items-center gap-2 mb-3 px-1">
        <div className="w-7 h-7 rounded-lg bg-slate-500/10 flex items-center justify-center border border-slate-500/20">
          <CircleUserRound className="w-4 h-4 text-foreground" />
        </div>
        <h2 className="text-sm font-semibold text-foreground tracking-tight">{t("settings.githubAccount")}</h2>
      </div>

      <div className="rounded-xl border border-border bg-card p-4 space-y-4">
        {auth.loading ? (
          <div className="flex items-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="w-4 h-4 animate-spin" />
            {t("settings.githubAuthLoading")}
          </div>
        ) : auth.authorization ? (
          <div className="space-y-3">
            <div>
              <p className="text-sm font-medium text-foreground">{t("settings.githubAuthWaiting")}</p>
              <p className="text-xs text-muted-foreground mt-1">{t("settings.githubAuthWaitingHint")}</p>
            </div>
            <div className="rounded-lg border border-border bg-muted/30 px-4 py-3 text-center">
              <div className="font-mono text-xl font-semibold tracking-[0.18em] text-foreground">
                {auth.authorization.user_code}
              </div>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button size="sm" onClick={() => void openExternalUrl(auth.authorization?.verification_uri ?? "")}>
                {t("settings.githubAuthOpen")}
              </Button>
              <Button size="sm" variant="outline" onClick={() => void auth.cancel()}>
                {t("common.cancel")}
              </Button>
            </div>
          </div>
        ) : auth.status?.state === "connected" ? (
          <div className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-3 min-w-0">
              <CheckCircle2 className="w-5 h-5 text-emerald-500 shrink-0" />
              <div className="min-w-0">
                <p className="text-sm font-medium text-foreground">@{auth.status.identity.login}</p>
                <p className="text-xs text-muted-foreground">{t("settings.githubAuthConnected")}</p>
              </div>
            </div>
            <div className="flex gap-2 shrink-0">
              <Button size="sm" variant="outline" disabled={auth.busy} onClick={() => void auth.refresh()}>
                <RefreshCw className="w-3.5 h-3.5 mr-1.5" />
                {t("common.refresh")}
              </Button>
              <Button size="sm" variant="outline" disabled={auth.busy} onClick={() => void auth.logout()}>
                <LogOut className="w-3.5 h-3.5 mr-1.5" />
                {t("settings.githubAuthLogout")}
              </Button>
            </div>
          </div>
        ) : auth.status?.state === "expired" ? (
          <div className="space-y-3">
            <div>
              <p className="text-sm font-medium text-amber-500">{t("settings.githubAuthExpired")}</p>
              <p className="text-xs text-muted-foreground mt-1">
                {auth.status.identity
                  ? t("settings.githubAuthExpiredHint", { login: auth.status.identity.login })
                  : t("settings.githubAuthExpiredHintUnknown")}
              </p>
            </div>
            <div className="flex gap-2">
              <Button size="sm" disabled={auth.busy} onClick={() => void auth.refresh()}>
                {t("common.refresh")}
              </Button>
              <Button size="sm" variant="outline" disabled={auth.busy} onClick={() => void auth.start()}>
                {t("settings.githubAuthSignInAgain")}
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div>
              <p className="text-sm font-medium text-foreground">{t("settings.githubAuthSignIn")}</p>
              <p className="text-xs text-muted-foreground mt-1 leading-relaxed">{t("settings.githubAuthSignInHint")}</p>
            </div>
            <div className="rounded-lg border border-border bg-muted/20 p-3 flex gap-2.5">
              <ShieldCheck className="w-4 h-4 text-muted-foreground mt-0.5 shrink-0" />
              <p className="text-xs text-muted-foreground leading-relaxed">{t("settings.githubAuthPermissions")}</p>
            </div>
            <Button size="sm" disabled={auth.busy} onClick={() => void auth.start()}>
              {auth.busy ? <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" /> : null}
              {t("settings.githubAuthStart")}
            </Button>
          </div>
        )}

        {(auth.flow?.state === "denied" || auth.flow?.state === "expired") && (
          <div className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-3 text-xs text-amber-600">
            {auth.flow.state === "denied" ? t("settings.githubAuthDenied") : t("settings.githubAuthDeviceExpired")}
            <button type="button" className="ml-2 underline" onClick={() => void auth.retry()}>
              {t("common.retry")}
            </button>
          </div>
        )}

        {errorMessage && (
          <div
            role="alert"
            className="flex items-center justify-between gap-3 rounded-lg border border-red-500/30 bg-red-500/5 p-3 text-xs text-red-500"
          >
            <span>{errorMessage}</span>
            <button type="button" className="shrink-0 underline" onClick={() => void auth.retry()}>
              {t("common.retry")}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
