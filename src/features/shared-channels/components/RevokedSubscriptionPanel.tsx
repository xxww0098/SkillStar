import { ArchiveRestore, Loader2, RefreshCw, ShieldOff, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { ChannelSubscription, ChannelSubscriptionView } from "../../../types";
import {
  convertRevokedSharedChannelSkillToLocal,
  checkSharedChannelUpdate,
  listSharedChannelSubscriptions,
  uninstallRevokedSharedChannelSkill,
} from "../api/channels";

export function RevokedSubscriptionPanel({
  subscription,
  onSubscriptionChanged,
}: {
  subscription: ChannelSubscription | ChannelSubscriptionView;
  onSubscriptionChanged: (subscription: ChannelSubscription | ChannelSubscriptionView) => void;
}) {
  const { t } = useTranslation();
  const initialIds =
    "skills" in subscription ? subscription.skills.map((skill) => skill.id) : subscription.selected_skill_ids;
  const [skillIds, setSkillIds] = useState(initialIds);
  const [localNames, setLocalNames] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [checkingAccess, setCheckingAccess] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setSkillIds(initialIds);
  }, [initialIds.join("\0")]);

  const resolve = async (skillId: string, action: "convert" | "uninstall") => {
    if (busy) return;
    const localName = (localNames[skillId] ?? `${skillId}.local`).trim();
    if (action === "convert" && !localName) return;
    setBusy(skillId);
    setError("");
    try {
      const result =
        action === "convert"
          ? await convertRevokedSharedChannelSkillToLocal({
              repository_id: subscription.repository_id,
              skill_id: skillId,
              local_name: localName,
            })
          : await uninstallRevokedSharedChannelSkill(subscription.repository_id, skillId);
      setSkillIds((current) => current.filter((id) => id !== result.skill_id));
      if (result.subscription) onSubscriptionChanged(result.subscription);
    } catch (cause) {
      setError(messageFrom(cause));
      const stored = await listSharedChannelSubscriptions().catch(() => []);
      const current = stored.find((item) => item.repository_id === subscription.repository_id);
      if (current) onSubscriptionChanged(current);
    } finally {
      setBusy(null);
    }
  };

  const checkAccess = async () => {
    if (checkingAccess || busy) return;
    setCheckingAccess(true);
    setError("");
    try {
      await checkSharedChannelUpdate(subscription.repository_id);
      const stored = await listSharedChannelSubscriptions();
      const current = stored.find((item) => item.repository_id === subscription.repository_id);
      if (current) onSubscriptionChanged(current);
    } catch (cause) {
      setError(messageFrom(cause));
      const stored = await listSharedChannelSubscriptions().catch(() => []);
      const current = stored.find((item) => item.repository_id === subscription.repository_id);
      if (current) onSubscriptionChanged(current);
    } finally {
      setCheckingAccess(false);
    }
  };

  return (
    <section
      className="space-y-4 rounded-xl border border-amber-500/30 bg-amber-500/5 p-4"
      aria-label={t("sharedChannels.revokedAria")}
    >
      <div className="flex items-start gap-2">
        <ShieldOff className="mt-0.5 size-4 text-amber-600" />
        <div>
          <p className="text-sm font-semibold">{t("sharedChannels.revokedTitle")}</p>
          <p className="mt-1 text-xs text-muted-foreground">{t("sharedChannels.revokedPreservedHint")}</p>
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">{t("sharedChannels.recheckHint")}</p>
        <Button
          size="sm"
          variant="outline"
          aria-label={t("sharedChannels.checkAccessAria")}
          disabled={checkingAccess || Boolean(busy)}
          onClick={() => void checkAccess()}
        >
          {checkingAccess ? <Loader2 className="mr-1 size-3 animate-spin" /> : <RefreshCw className="mr-1 size-3" />}
          {t("sharedChannels.checkAccess")}
        </Button>
      </div>

      <div className="space-y-2">
        {skillIds.map((skillId) => (
          <div key={skillId} className="rounded-lg border border-border bg-background/70 p-3">
            <p className="text-xs font-medium">{skillId}</p>
            <div className="mt-2 flex flex-wrap items-end gap-2">
              <label className="min-w-44 flex-1 space-y-1 text-[11px]">
                <span>{t("sharedChannels.localCopyName")}</span>
                <input
                  aria-label={t("sharedChannels.localCopyNameAria", { skillId })}
                  value={localNames[skillId] ?? `${skillId}.local`}
                  onChange={(event) => setLocalNames((current) => ({ ...current, [skillId]: event.target.value }))}
                  className="h-8 w-full rounded-md border border-border bg-background px-2 font-mono text-xs"
                  disabled={checkingAccess || Boolean(busy)}
                />
              </label>
              <Button
                size="sm"
                variant="outline"
                aria-label={t("sharedChannels.keepAsLocalAria", { skillId })}
                disabled={checkingAccess || Boolean(busy)}
                onClick={() => void resolve(skillId, "convert")}
              >
                {busy === skillId ? (
                  <Loader2 className="mr-1 size-3 animate-spin" />
                ) : (
                  <ArchiveRestore className="mr-1 size-3" />
                )}
                {t("sharedChannels.keepAsLocal")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t("sharedChannels.uninstallAria", { skillId })}
                disabled={checkingAccess || Boolean(busy)}
                onClick={() => void resolve(skillId, "uninstall")}
              >
                <Trash2 className="mr-1 size-3" /> {t("sharedChannels.uninstall")}
              </Button>
            </div>
          </div>
        ))}
        {skillIds.length === 0 && <p className="text-xs text-muted-foreground">{t("sharedChannels.noSkillsRemain")}</p>}
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
