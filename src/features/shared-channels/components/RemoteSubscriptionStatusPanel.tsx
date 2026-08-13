import { CircleAlert, Loader2, RefreshCw, ShieldAlert, WifiOff } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { ChannelSubscription, ChannelSubscriptionRemoteStatus, ChannelSubscriptionView } from "../../../types";
import { checkSharedChannelUpdate, listSharedChannelSubscriptions } from "../api/channels";

type FrozenStatus = Exclude<ChannelSubscriptionRemoteStatus, "active" | "revoked">;

const presentation: Record<FrozenStatus, { titleKey: string; detailKey: string; icon: typeof WifiOff; tone: string }> =
  {
    offline: {
      titleKey: "sharedChannels.offlineTitle",
      detailKey: "sharedChannels.offlineDetail",
      icon: WifiOff,
      tone: "border-sky-500/30 bg-sky-500/5",
    },
    recoverable_failure: {
      titleKey: "sharedChannels.recoverableTitle",
      detailKey: "sharedChannels.recoverableDetail",
      icon: CircleAlert,
      tone: "border-amber-500/30 bg-amber-500/5",
    },
    integrity_error: {
      titleKey: "sharedChannels.integrityTitle",
      detailKey: "sharedChannels.integrityDetail",
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
  const { t } = useTranslation();
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
    <section className={`space-y-3 rounded-xl border p-4 ${state.tone}`} aria-label={t("sharedChannels.frozenAria")}>
      <div className="flex items-start gap-2">
        <Icon className="mt-0.5 size-4 shrink-0" />
        <div>
          <p className="text-sm font-semibold">{t(state.titleKey)}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t(state.detailKey)} {t("sharedChannels.frozenSuffix")}
          </p>
          {subscription.remote_state.message && (
            <p className="mt-2 text-[11px] text-muted-foreground">{subscription.remote_state.message}</p>
          )}
        </div>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">{t("sharedChannels.retryReadOnlyHint")}</p>
        <Button
          size="sm"
          variant="outline"
          aria-label={t("sharedChannels.retryValidationAria")}
          disabled={checking}
          onClick={() => void retry()}
        >
          {checking ? <Loader2 className="mr-1 size-3 animate-spin" /> : <RefreshCw className="mr-1 size-3" />}
          {t("sharedChannels.retryValidation")}
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
