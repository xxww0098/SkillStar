import { CircleUserRound } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import { GITHUB_ACCOUNT_MENU_EVENT, cn } from "../../../lib/utils";
import { GitHubAuthPanel } from "./GitHubAuthPanel";
import { useGitHubAuth } from "./useGitHubAuth";

/**
 * Sidebar entry for the GitHub account: it shows the current sign-in state at
 * a glance and opens the device-flow panel in a modal. Signing in is an
 * account-level action, so it lives in the sidebar toolbar rather than being
 * buried in a Settings section.
 */
export function GitHubAccountMenu({ collapsed }: { collapsed?: boolean }) {
  const { t } = useTranslation();
  const auth = useGitHubAuth();
  const [open, setOpen] = useState(false);

  const close = useCallback(() => setOpen(false), []);

  // Other surfaces (e.g. shared-channel sign-in prompts) ask for the panel by
  // event instead of importing this component's state.
  useEffect(() => {
    const handler = () => setOpen(true);
    window.addEventListener(GITHUB_ACCOUNT_MENU_EVENT, handler);
    return () => window.removeEventListener(GITHUB_ACCOUNT_MENU_EVENT, handler);
  }, []);

  const status = auth.status;
  const state = status?.state ?? "signed_out";
  const identity = status && status.state !== "signed_out" ? status.identity : null;
  const label = identity ? `@${identity.login}` : t("sidebar.githubSignIn");
  const title = state === "expired" ? t("settings.githubAuthExpired") : label;

  const avatar = identity?.avatar_url ? (
    <img src={identity.avatar_url} alt={label} className="w-5 h-5 rounded-full object-cover shrink-0" />
  ) : (
    <CircleUserRound className="w-[17px] h-[17px] shrink-0" />
  );

  const statusDot =
    state === "connected" ? "bg-emerald-500" : state === "expired" ? "bg-amber-500" : "bg-muted-foreground/40";

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={title}
        aria-haspopup="dialog"
        className={cn(
          "flex items-center rounded-lg text-muted-foreground transition duration-150 cursor-pointer focus-ring",
          "hover:bg-muted/40 hover:text-foreground",
          collapsed ? "relative w-8 h-8 justify-center mx-auto" : "w-full gap-2 px-2 py-1.5",
        )}
      >
        {avatar}
        {collapsed ? (
          <span
            aria-hidden
            className={cn("absolute bottom-0.5 right-0.5 w-1.5 h-1.5 rounded-full ring-2 ring-background", statusDot)}
          />
        ) : (
          <>
            <span className="flex-1 min-w-0 truncate text-left text-[12px] leading-tight">{label}</span>
            <span aria-hidden className={cn("w-1.5 h-1.5 rounded-full shrink-0", statusDot)} />
          </>
        )}
      </button>

      {/* Portalled: the sidebar is a transformed, sometimes overflow-hidden
          fixed rail, which would otherwise clip the modal. */}
      {createPortal(
        <ModalShell open={open} onClose={close} ariaLabel={t("settings.githubAccount")} panelClassName="max-w-md">
          <ModalHeader
            icon={<CircleUserRound className="w-4 h-4 text-primary" />}
            title={t("settings.githubAccount")}
            onClose={close}
          />
          <div className="px-6 py-5">
            <GitHubAuthPanel auth={auth} />
          </div>
        </ModalShell>,
        document.body,
      )}
    </>
  );
}
