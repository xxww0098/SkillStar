import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  ArrowUpFromLine,
  Check,
  Copy,
  ExternalLink,
  KeyRound,
  Loader2,
  LogOut,
  RefreshCw,
  Timer,
  TriangleAlert,
  UserRoundCog,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Github } from "../../../components/ui/icons/Github";
import { openExternalUrl } from "../../../lib/externalOpen";
import { cn } from "../../../lib/utils";
import type { useGitHubAuth } from "./useGitHubAuth";

export type GitHubAuthController = ReturnType<typeof useGitHubAuth>;

/** Milliseconds left before an ISO deadline, floored at zero. */
function remainingMs(deadline: string | undefined): number {
  if (!deadline) return 0;
  const ms = new Date(deadline).getTime() - Date.now();
  return Number.isFinite(ms) ? Math.max(0, ms) : 0;
}

/** Live `m:ss` countdown for the device code, or null once it lapses. */
function useCountdown(deadline: string | undefined): string | null {
  const [left, setLeft] = useState(() => remainingMs(deadline));

  useEffect(() => {
    if (!deadline) return;
    setLeft(remainingMs(deadline));
    const timer = window.setInterval(() => setLeft(remainingMs(deadline)), 1_000);
    return () => window.clearInterval(timer);
  }, [deadline]);

  if (!deadline || left <= 0) return null;
  const total = Math.round(left / 1_000);
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}

/** Copy-to-clipboard with a short-lived confirmation, resilient to denied permissions. */
function useCopy(): { copied: boolean; copy: (value: string) => Promise<boolean> } {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 2_000);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = useCallback(async (value: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      return true;
    } catch {
      return false;
    }
  }, []);

  return { copied, copy };
}

/** Shared entrance for the four account states so switching never snaps. */
function StateFrame({ id, children }: { id: string; children: React.ReactNode }) {
  const prefersReducedMotion = useReducedMotion();
  return (
    <AnimatePresence mode="wait" initial={false}>
      <motion.div
        key={id}
        initial={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, y: 6 }}
        animate={{ opacity: 1, y: 0 }}
        exit={prefersReducedMotion ? { opacity: 0 } : { opacity: 0, y: -6 }}
        transition={{ duration: prefersReducedMotion ? 0.01 : 0.24, ease: [0.16, 1, 0.3, 1] }}
      >
        {children}
      </motion.div>
    </AnimatePresence>
  );
}

/** Credentials never leave the machine — say so where the user commits. */
function KeychainNote() {
  const { t } = useTranslation();
  return (
    <p className="flex items-start gap-2 text-[11px] leading-relaxed text-muted-foreground">
      <KeyRound className="mt-px h-3.5 w-3.5 shrink-0" strokeWidth={1.75} />
      {t("settings.githubAuthKeychain")}
    </p>
  );
}

function ScopeRow({ icon, name, hint }: { icon: React.ReactNode; name: string; hint: string }) {
  return (
    <li className="flex items-start gap-2.5">
      <span className="mt-0.5 text-muted-foreground">{icon}</span>
      <span className="min-w-0">
        <span className="font-mono text-[11.5px] font-medium text-foreground/90">{name}</span>
        <span className="ml-1.5 text-xs text-muted-foreground">{hint}</span>
      </span>
    </li>
  );
}

function SignInState({ auth }: { auth: GitHubAuthController }) {
  const { t } = useTranslation();

  return (
    <div className="space-y-5">
      <div>
        <p className="text-heading-sm">{t("settings.githubAuthSignIn")}</p>
        <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">{t("settings.githubAuthSignInHint")}</p>
      </div>

      <div className="rounded-xl border border-border bg-muted/30 p-3.5">
        <ul className="space-y-2.5">
          <ScopeRow
            icon={<UserRoundCog className="h-4 w-4" strokeWidth={1.75} />}
            name={t("settings.githubAuthScopeAdmin")}
            hint={t("settings.githubAuthScopeAdminHint")}
          />
          <ScopeRow
            icon={<ArrowUpFromLine className="h-4 w-4" strokeWidth={1.75} />}
            name={t("settings.githubAuthScopeContents")}
            hint={t("settings.githubAuthScopeContentsHint")}
          />
        </ul>
        <p className="mt-3 border-t border-border pt-2.5 text-[11px] leading-relaxed text-muted-foreground">
          {t("settings.githubAuthScopeNote")}
        </p>
      </div>

      <div className="space-y-3">
        <Button className="w-full" disabled={auth.busy} onClick={() => void auth.start()}>
          {auth.busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Github className="h-4 w-4" />}
          {t("settings.githubAuthStart")}
        </Button>
        <KeychainNote />
      </div>
    </div>
  );
}

function DeviceCodeState({ auth }: { auth: GitHubAuthController }) {
  const { t } = useTranslation();
  const prefersReducedMotion = useReducedMotion();
  const { copied, copy } = useCopy();
  const authorization = auth.authorization;
  const countdown = useCountdown(authorization?.expires_at);
  const code = authorization?.user_code ?? "";

  // The code has to reach the GitHub page, so opening it carries the copy.
  const openGitHub = useCallback(async () => {
    await copy(code);
    await openExternalUrl(authorization?.verification_uri ?? "");
  }, [authorization?.verification_uri, code, copy]);

  return (
    <div className="space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-center gap-2.5 min-w-0">
          <span className="relative flex h-2 w-2 shrink-0">
            {!prefersReducedMotion && (
              <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-primary opacity-60" />
            )}
            <span className="relative inline-flex h-2 w-2 rounded-full bg-primary" />
          </span>
          <p className="truncate text-sm font-medium text-foreground">{t("settings.githubAuthWaiting")}</p>
        </div>
        {countdown && (
          <span className="flex shrink-0 items-center gap-1 rounded-md bg-muted/40 px-2 py-0.5 text-[11px] text-muted-foreground">
            <Timer className="h-3 w-3" strokeWidth={1.75} />
            <span className="font-mono tabular-nums">{t("settings.githubAuthExpiresIn", { time: countdown })}</span>
          </span>
        )}
      </div>

      <button
        type="button"
        onClick={() => void copy(code)}
        aria-label={t("settings.githubAuthCopy")}
        className={cn(
          "group relative flex w-full items-center justify-center rounded-xl border bg-muted/25 px-4 py-5",
          "cursor-pointer transition duration-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/50",
          copied ? "border-emerald-500/50" : "border-border hover:border-primary/45 hover:bg-muted/35",
        )}
      >
        <span className="font-mono text-2xl font-semibold tabular-nums tracking-[0.22em] text-foreground">{code}</span>
        <span
          className={cn(
            "absolute right-3 flex items-center gap-1 text-[11px] transition duration-200",
            copied ? "text-emerald-400 paper:text-emerald-700" : "text-muted-foreground/70 group-hover:text-foreground",
          )}
        >
          {copied ? (
            <>
              <Check className="h-3.5 w-3.5" strokeWidth={2} />
              {t("settings.githubAuthCopied")}
            </>
          ) : (
            <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
          )}
        </span>
      </button>

      <ol className="space-y-1.5 text-xs text-muted-foreground">
        <li className="flex gap-2.5">
          <span className="font-mono text-[11px] text-muted-foreground">1</span>
          {t("settings.githubAuthStepCopy")}
        </li>
        <li className="flex gap-2.5">
          <span className="font-mono text-[11px] text-muted-foreground">2</span>
          {t("settings.githubAuthStepApprove")}
        </li>
      </ol>

      <div className="flex gap-2">
        <Button className="flex-1" onClick={() => void openGitHub()}>
          <ExternalLink className="h-4 w-4" />
          {t("settings.githubAuthOpen")}
        </Button>
        <Button variant="outline" onClick={() => void auth.cancel()}>
          {t("common.cancel")}
        </Button>
      </div>

      <KeychainNote />
    </div>
  );
}

function ConnectedState({
  auth,
  identity,
}: {
  auth: GitHubAuthController;
  identity: { login: string; avatar_url: string | null };
}) {
  const { t } = useTranslation();

  return (
    <div className="space-y-5">
      <div className="flex items-center gap-3.5">
        <span className="relative shrink-0">
          {identity.avatar_url ? (
            <img
              src={identity.avatar_url}
              alt={`@${identity.login}`}
              className="h-11 w-11 rounded-full border border-border object-cover"
            />
          ) : (
            <span className="flex h-11 w-11 items-center justify-center rounded-full border border-border bg-muted/40 text-foreground">
              <Github className="h-[22px] w-[22px]" strokeWidth={1.75} />
            </span>
          )}
          <span className="absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full bg-emerald-500 ring-2 ring-background" />
        </span>
        <div className="min-w-0">
          <p className="truncate text-[15px] font-semibold tracking-tight text-foreground">@{identity.login}</p>
          <p className="mt-0.5 truncate text-xs text-muted-foreground">{t("settings.githubAuthConnected")}</p>
        </div>
      </div>

      <div className="flex gap-2">
        <Button variant="outline" className="flex-1" disabled={auth.busy} onClick={() => void auth.refresh()}>
          <RefreshCw className={cn("h-4 w-4", auth.busy && "animate-spin")} />
          {t("common.refresh")}
        </Button>
        <Button variant="outline" className="flex-1" disabled={auth.busy} onClick={() => void auth.logout()}>
          <LogOut className="h-4 w-4" />
          {t("settings.githubAuthLogout")}
        </Button>
      </div>
    </div>
  );
}

function ExpiredState({ auth, identity }: { auth: GitHubAuthController; identity: { login: string } | null }) {
  const { t } = useTranslation();

  return (
    <div className="space-y-5">
      <div className="flex items-start gap-3.5">
        <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-2xl border border-amber-500/30 bg-amber-500/10 text-amber-400 paper:text-amber-700">
          <TriangleAlert className="h-[22px] w-[22px]" strokeWidth={1.75} />
        </span>
        <div className="min-w-0 pt-0.5">
          <p className="text-heading-sm text-amber-400 paper:text-amber-700">{t("settings.githubAuthExpired")}</p>
          <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
            {identity
              ? t("settings.githubAuthExpiredHint", { login: identity.login })
              : t("settings.githubAuthExpiredHintUnknown")}
          </p>
        </div>
      </div>

      <div className="flex gap-2">
        <Button className="flex-1" disabled={auth.busy} onClick={() => void auth.refresh()}>
          <RefreshCw className={cn("h-4 w-4", auth.busy && "animate-spin")} />
          {t("common.refresh")}
        </Button>
        <Button variant="outline" className="flex-1" disabled={auth.busy} onClick={() => void auth.start()}>
          {t("settings.githubAuthSignInAgain")}
        </Button>
      </div>
    </div>
  );
}

/**
 * Presentational GitHub sign-in surface. The controller lives in the caller so
 * the sidebar entry and this panel observe one authentication state instead of
 * two independently polled copies.
 */
export function GitHubAuthPanel({ auth }: { auth: GitHubAuthController }) {
  const { t } = useTranslation();
  const errorMessage =
    auth.error?.code === "proxy" || auth.error?.code === "network"
      ? t("settings.githubAuthProxyError")
      : auth.error?.message;
  const flowNotice =
    auth.flow?.state === "denied"
      ? t("settings.githubAuthDenied")
      : auth.flow?.state === "expired"
        ? t("settings.githubAuthDeviceExpired")
        : null;

  return (
    <div className="space-y-4">
      {auth.loading ? (
        <StateFrame id="loading">
          <div className="flex items-center gap-2 py-1 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t("settings.githubAuthLoading")}
          </div>
        </StateFrame>
      ) : auth.authorization ? (
        <StateFrame id="device">
          <DeviceCodeState auth={auth} />
        </StateFrame>
      ) : auth.status?.state === "connected" ? (
        <StateFrame id="connected">
          <ConnectedState auth={auth} identity={auth.status.identity} />
        </StateFrame>
      ) : auth.status?.state === "expired" ? (
        <StateFrame id="expired">
          <ExpiredState auth={auth} identity={auth.status.identity} />
        </StateFrame>
      ) : (
        <StateFrame id="signed-out">
          <SignInState auth={auth} />
        </StateFrame>
      )}

      {flowNotice && (
        <div className="flex items-center justify-between gap-3 rounded-xl border border-amber-500/30 bg-amber-500/5 px-3.5 py-2.5 text-xs text-amber-400 paper:text-amber-700">
          <span className="min-w-0">{flowNotice}</span>
          <button
            type="button"
            className="shrink-0 rounded-md px-1 font-medium underline underline-offset-4 transition hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-500/50"
            onClick={() => void auth.retry()}
          >
            {t("common.retry")}
          </button>
        </div>
      )}

      {errorMessage && (
        <div
          role="alert"
          className="flex items-center justify-between gap-3 rounded-xl border border-red-500/35 bg-red-500/5 px-3.5 py-2.5 text-xs text-red-400 paper:text-red-700"
        >
          <span className="min-w-0">{errorMessage}</span>
          <button
            type="button"
            className="shrink-0 rounded-md px-1 font-medium underline underline-offset-4 transition hover:opacity-80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red-500/50"
            onClick={() => void auth.retry()}
          >
            {t("common.retry")}
          </button>
        </div>
      )}
    </div>
  );
}
