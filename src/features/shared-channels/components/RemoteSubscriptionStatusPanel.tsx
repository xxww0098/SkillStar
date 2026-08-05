import { CircleAlert, Loader2, RefreshCw, ShieldAlert, WifiOff } from "lucide-react";
import { useState } from "react";
import { Button } from "../../../components/ui/button";
import type { ChannelSubscription, ChannelSubscriptionRemoteStatus, ChannelSubscriptionView } from "../../../types";
import { checkSharedChannelUpdate, listSharedChannelSubscriptions } from "../api/channels";

type FrozenStatus = Exclude<ChannelSubscriptionRemoteStatus, "active" | "revoked">;

const presentation: Record<FrozenStatus, { title: string; detail: string; icon: typeof WifiOff; tone: string }> = {
  offline: {
    title: "Shared channel is offline",
    detail: "SkillStar could not reach GitHub through the current network or proxy.",
    icon: WifiOff,
    tone: "border-sky-500/30 bg-sky-500/5",
  },
  recoverable_failure: {
    title: "Shared channel check needs attention",
    detail: "GitHub authentication, rate limiting, or a temporary API response prevented validation.",
    icon: CircleAlert,
    tone: "border-amber-500/30 bg-amber-500/5",
  },
  integrity_error: {
    title: "Shared channel integrity check failed",
    detail: "Repository identity, release metadata, paths, or content no longer match the trusted channel record.",
    icon: ShieldAlert,
    tone: "border-destructive/30 bg-destructive/5",
  },
};

export function RemoteSubscriptionStatusPanel({
  subscription,
  onSubscriptionChanged,
}: {
  subscription: ChannelSubscription | ChannelSubscriptionView;
  onSubscriptionChanged: (subscription: ChannelSubscription | ChannelSubscriptionView) => void;
}) {
  const status = subscription.remote_state.status as FrozenStatus;
  const state = presentation[status];
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState("");

  if (!state) return null;
  const Icon = state.icon;

  const retry = async () => {
    if (checking) return;
    setChecking(true);
    setError("");
    try {
      await checkSharedChannelUpdate(subscription.repository_id);
    } catch (cause) {
      setError(messageFrom(cause));
    } finally {
      const stored = await listSharedChannelSubscriptions().catch(() => []);
      const current = stored.find((item) => item.repository_id === subscription.repository_id);
      if (current) onSubscriptionChanged(current);
      setChecking(false);
    }
  };

  return (
    <section className={`space-y-3 rounded-xl border p-4 ${state.tone}`} aria-label="Frozen channel subscription">
      <div className="flex items-start gap-2">
        <Icon className="mt-0.5 size-4 shrink-0" />
        <div>
          <p className="text-sm font-semibold">{state.title}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {state.detail} Installed Skills and deployments remain unchanged; remote updates, history, and automatic
            upgrades stay disabled until validation succeeds.
          </p>
          {subscription.remote_state.message && (
            <p className="mt-2 text-[11px] text-muted-foreground">{subscription.remote_state.message}</p>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">Retry performs a read-only remote validation.</p>
        <Button
          size="sm"
          variant="outline"
          aria-label="Retry remote validation"
          disabled={checking}
          onClick={() => void retry()}
        >
          {checking ? <Loader2 className="mr-1 size-3 animate-spin" /> : <RefreshCw className="mr-1 size-3" />}
          Retry validation
        </Button>
      </div>
      {error && <p className="text-xs text-destructive">{error}</p>}
    </section>
  );
}

function messageFrom(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
