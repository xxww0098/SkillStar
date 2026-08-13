import { AlertTriangle, CheckCircle2, Loader2, RefreshCw, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { Switch } from "../../../components/ui/switch";
import type {
  ChannelAutoUpdateExecution,
  ChannelAutoUpdatePauseReason,
  ChannelAutoUpdateState,
  ChannelSkillRollbackTarget,
  ChannelUpdateItem,
  ChannelUpdateSnapshot,
  LocalDivergenceResolution,
} from "../../../types";
import {
  applySharedChannelUpdate,
  checkSharedChannelUpdate,
  convertRemovedSharedChannelSkillToLocal,
  getSharedChannelAutoUpdateState,
  getSharedChannelUpdateState,
  installSharedChannelSkill,
  listSharedChannelSkillRollbackTargets,
  resumeSharedChannelSkillFollowing,
  rollbackSharedChannelSkill,
  runSharedChannelAutoUpdates,
  setSharedChannelAutoUpdateEnabled,
  uninstallRemovedSharedChannelSkill,
} from "../api/channels";

export function ChannelUpdatePanel({
  repositoryId,
  onChecked,
}: {
  repositoryId: number;
  onChecked?: () => Promise<void> | void;
}) {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<ChannelUpdateSnapshot | null>(null);
  const [checking, setChecking] = useState(true);
  const [applying, setApplying] = useState(false);
  const [autoSaving, setAutoSaving] = useState(false);
  const [autoUpdate, setAutoUpdate] = useState<ChannelAutoUpdateState | null>(null);
  const [error, setError] = useState("");
  const [resolutions, setResolutions] = useState<Record<string, LocalDivergenceResolution>>({});
  const [localNames, setLocalNames] = useState<Record<string, string>>({});
  const [histories, setHistories] = useState<Record<string, ChannelSkillRollbackTarget[] | undefined>>({});
  const [historySelections, setHistorySelections] = useState<Record<string, number>>({});
  const [historyBusy, setHistoryBusy] = useState<string | null>(null);
  const [skillAction, setSkillAction] = useState<string | null>(null);

  const refreshSubscription = useCallback(async () => {
    try {
      await onChecked?.();
    } catch {
      // The action error remains primary; a later panel refresh can retry the local state read.
    }
  }, [onChecked]);

  const check = useCallback(async () => {
    setChecking(true);
    setError("");
    try {
      const next = await checkSharedChannelUpdate(repositoryId);
      setSnapshot(next);
      setResolutions({});
      setHistories({});
      setHistorySelections({});
      setLocalNames((current) => suggestedNames(next.items, current));
      await refreshSubscription();
    } catch (cause) {
      setError(updateError(cause));
      await refreshSubscription();
    } finally {
      setChecking(false);
    }
  }, [refreshSubscription, repositoryId]);

  useEffect(() => {
    let active = true;
    void getSharedChannelUpdateState(repositoryId)
      .then((stored) => {
        if (active && stored) {
          setSnapshot(stored);
          setLocalNames((current) => suggestedNames(stored.items, current));
        }
      })
      .catch(() => undefined)
      .finally(() => {
        if (active) void check();
      });
    return () => {
      active = false;
    };
  }, [check, repositoryId]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void getSharedChannelAutoUpdateState(repositoryId)
      .then((state) => {
        if (active) setAutoUpdate(state);
      })
      .catch(() => undefined);
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<ChannelAutoUpdateExecution[]>("shared-channels://auto-update-completed", (event) => {
          const execution = event.payload.find((item) => item.repository_id === repositoryId);
          if (!active || !execution) return;
          setAutoUpdate((current) =>
            current
              ? { ...current, last_run: execution.run }
              : { enabled: true, next_check_at: null, last_run: execution.run },
          );
          void Promise.all([getSharedChannelUpdateState(repositoryId), getSharedChannelAutoUpdateState(repositoryId)])
            .then(([stored, autoState]) => {
              if (!active) return;
              if (stored) setSnapshot(stored);
              setAutoUpdate(autoState);
              return onChecked?.();
            })
            .catch(() => undefined);
        }),
      )
      .then((stop) => {
        if (active) unlisten = stop;
        else stop();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [onChecked, repositoryId]);

  const toggleAutoUpdate = async (enabled: boolean) => {
    if (autoSaving) return;
    setAutoSaving(true);
    setError("");
    try {
      const state = await setSharedChannelAutoUpdateEnabled(repositoryId, enabled);
      setAutoUpdate(state);
      if (enabled) {
        const executions = await runSharedChannelAutoUpdates(crypto.randomUUID());
        const execution = executions.find((item) => item.repository_id === repositoryId);
        if (execution) setAutoUpdate({ ...state, last_run: execution.run });
        const stored = await getSharedChannelUpdateState(repositoryId);
        if (stored) setSnapshot(stored);
        setAutoUpdate(await getSharedChannelAutoUpdateState(repositoryId));
      }
    } catch (cause) {
      setError(updateError(cause));
      const stored = await getSharedChannelAutoUpdateState(repositoryId).catch(() => null);
      if (stored) setAutoUpdate(stored);
      await refreshSubscription();
    } finally {
      setAutoSaving(false);
    }
  };

  const actionable = useMemo(() => {
    if (!snapshot || snapshot.check_error) return false;
    const selectedAtTarget = snapshot.items
      .filter((item) => item.selected)
      .every((item) => item.state === "current" || item.state === "applied");
    return (
      (snapshot.acknowledgement_required && selectedAtTarget) ||
      snapshot.items.some(
        (item) =>
          (item.state === "available" && !item.pinned_target) ||
          (item.state === "blocked" && !item.pinned_target && resolutions[item.id]),
      )
    );
  }, [resolutions, snapshot]);

  const apply = async () => {
    if (!snapshot || !actionable || applying) return;
    setApplying(true);
    setError("");
    try {
      const result = await applySharedChannelUpdate(
        {
          repository_id: repositoryId,
          target: snapshot.target,
          resolutions: Object.entries(resolutions).map(([skill_id, resolution]) => ({ skill_id, resolution })),
        },
        crypto.randomUUID(),
      );
      setSnapshot(result.snapshot);
      setResolutions({});
    } catch (cause) {
      setError(updateError(cause));
      const stored = await getSharedChannelUpdateState(repositoryId).catch(() => null);
      if (stored) setSnapshot(stored);
      await refreshSubscription();
    } finally {
      setApplying(false);
    }
  };

  const loadHistory = async (skillId: string) => {
    if (historyBusy || skillAction) return;
    setHistoryBusy(skillId);
    setError("");
    try {
      const targets = await listSharedChannelSkillRollbackTargets(repositoryId, skillId);
      setHistories((current) => ({ ...current, [skillId]: targets }));
      if (targets[0]) {
        setHistorySelections((current) => ({ ...current, [skillId]: targets[0].target.revision }));
      }
    } catch (cause) {
      setError(updateError(cause));
      await refreshSubscription();
    } finally {
      setHistoryBusy(null);
    }
  };

  const rollbackSkill = async (skillId: string) => {
    const target = histories[skillId]?.find((item) => item.target.revision === historySelections[skillId]);
    if (!target || skillAction) return;
    setSkillAction(skillId);
    setError("");
    try {
      const result = await rollbackSharedChannelSkill(
        {
          repository_id: repositoryId,
          skill_id: skillId,
          target: target.target,
          resolution: resolutions[skillId] ?? null,
        },
        crypto.randomUUID(),
      );
      setSnapshot(result.snapshot);
      setResolutions((current) => withoutKey(current, skillId));
      setHistories((current) => ({ ...current, [skillId]: undefined }));
      setHistorySelections((current) => withoutKey(current, skillId));
    } catch (cause) {
      setError(updateError(cause));
      const stored = await getSharedChannelUpdateState(repositoryId).catch(() => null);
      if (stored) setSnapshot(stored);
      await refreshSubscription();
    } finally {
      setSkillAction(null);
    }
  };

  const resumeFollowing = async (skillId: string) => {
    if (skillAction) return;
    setSkillAction(skillId);
    setError("");
    try {
      setSnapshot(await resumeSharedChannelSkillFollowing(repositoryId, skillId));
      setResolutions((current) => withoutKey(current, skillId));
      setHistorySelections((current) => withoutKey(current, skillId));
    } catch (cause) {
      setError(updateError(cause));
      await refreshSubscription();
    } finally {
      setSkillAction(null);
    }
  };

  const resolveRemovedSkill = async (skillId: string, action: "convert" | "uninstall") => {
    if (skillAction) return;
    const localName = (localNames[skillId] ?? `${skillId}.local`).trim();
    if (action === "convert" && !localName) return;
    setSkillAction(skillId);
    setError("");
    try {
      const result =
        action === "convert"
          ? await convertRemovedSharedChannelSkillToLocal({
              repository_id: repositoryId,
              skill_id: skillId,
              local_name: localName,
            })
          : await uninstallRemovedSharedChannelSkill(repositoryId, skillId);
      setSnapshot(result.snapshot);
      setLocalNames((current) => withoutKey(current, skillId));
      setHistories((current) => ({ ...current, [skillId]: undefined }));
      setHistorySelections((current) => withoutKey(current, skillId));
    } catch (cause) {
      setError(updateError(cause));
      const stored = await getSharedChannelUpdateState(repositoryId).catch(() => null);
      if (stored) setSnapshot(stored);
      await refreshSubscription();
    } finally {
      setSkillAction(null);
    }
  };

  const installAddedSkill = async (skillId: string) => {
    if (skillAction) return;
    setSkillAction(skillId);
    setError("");
    try {
      const result = await installSharedChannelSkill(repositoryId, skillId, crypto.randomUUID());
      setSnapshot(result.snapshot);
    } catch (cause) {
      setError(updateError(cause));
      const stored = await getSharedChannelUpdateState(repositoryId).catch(() => null);
      if (stored) setSnapshot(stored);
      await refreshSubscription();
    } finally {
      setSkillAction(null);
    }
  };

  if (checking && !snapshot) {
    return (
      <section className="rounded-xl border border-border p-4" aria-label={t("sharedChannels.updatesAria")}>
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" /> {t("sharedChannels.checkingRelease")}
        </p>
      </section>
    );
  }

  if (!snapshot) {
    return (
      <section className="space-y-3 rounded-xl border border-border p-4" aria-label={t("sharedChannels.updatesAria")}>
        <p className="text-sm font-medium">{t("sharedChannels.updateUnavailable")}</p>
        <p className="mt-1 text-xs text-destructive">{error}</p>
        {autoUpdate?.last_run && (
          <div className="rounded-lg border border-border bg-muted/20 p-3">
            <p className="text-xs font-medium">{t("sharedChannels.lastBackgroundCheck")}</p>
            <AutoUpdateResult state={autoUpdate} />
          </div>
        )}
        <Button size="sm" variant="outline" className="mt-3" onClick={() => void check()}>
          {t("sharedChannels.retryCheck")}
        </Button>
      </section>
    );
  }

  const counts = snapshot.items.reduce<Record<string, number>>((result, item) => {
    result[item.change] = (result[item.change] ?? 0) + 1;
    return result;
  }, {});

  return (
    <section className="space-y-4 rounded-xl border border-border p-4" aria-label={t("sharedChannels.updatesAria")}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold">
            {t("sharedChannels.updateRevisionTitle", { revision: snapshot.target.revision })}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t("sharedChannels.publishedByLine", {
              title: snapshot.title,
              login: snapshot.publisher.login,
            })}{" "}
            <time dateTime={snapshot.published_at}>{formatPublishedAt(snapshot.published_at)}</time>
          </p>
        </div>
        <span className={statusClass(snapshot.status)}>{statusLabel(snapshot.status)}</span>
      </div>

      <p className="whitespace-pre-wrap text-xs text-muted-foreground">
        {snapshot.notes || t("sharedChannels.noReleaseNotes")}
      </p>
      <div className="flex flex-wrap gap-2 text-[10px] text-muted-foreground">
        <span>{t("sharedChannels.countsAdded", { count: counts.added ?? 0 })}</span>
        <span>{t("sharedChannels.countsUpdated", { count: counts.updated ?? 0 })}</span>
        <span>{t("sharedChannels.countsRemoved", { count: counts.removed ?? 0 })}</span>
        <span>{t("sharedChannels.countsUnchanged", { count: counts.unchanged ?? 0 })}</span>
        <span>
          {t("sharedChannels.lastChecked")}{" "}
          <time dateTime={snapshot.checked_at}>{formatPublishedAt(snapshot.checked_at)}</time>
        </span>
      </div>

      {snapshot.check_error && (
        <p className="rounded-lg border border-amber-500/25 bg-amber-500/5 p-2 text-xs text-amber-700">
          {t("sharedChannels.staleResultHint", { error: snapshot.check_error })}
        </p>
      )}

      <div className="rounded-lg border border-border bg-muted/20 p-3">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-medium">{t("sharedChannels.protectedAutoUpgrades")}</p>
            <p className="mt-1 text-[11px] text-muted-foreground">{t("sharedChannels.autoUpgradeHint")}</p>
          </div>
          <Switch
            aria-label={t("sharedChannels.autoUpgradeAria")}
            checked={autoUpdate?.enabled ?? false}
            disabled={!autoUpdate || autoSaving || applying}
            onCheckedChange={(enabled) => void toggleAutoUpdate(enabled)}
          />
        </div>
        {autoUpdate?.last_run && <AutoUpdateResult state={autoUpdate} />}
      </div>

      {snapshot.acknowledgement_required &&
        !snapshot.items.some((item) =>
          ["available", "notification", "blocked", "failed", "removed_from_channel"].includes(item.state),
        ) && (
          <p className="rounded-lg border border-border bg-muted/40 p-2 text-xs text-muted-foreground">
            {t("sharedChannels.acknowledgeHint")}
          </p>
        )}

      <div className="space-y-2">
        {snapshot.items.map((item) => (
          <UpdateItem
            key={item.id}
            item={item}
            disabled={applying || Boolean(skillAction)}
            history={histories[item.id]}
            historyLoading={historyBusy === item.id}
            historySelection={historySelections[item.id]}
            skillAction={skillAction === item.id}
            localName={localNames[item.id] ?? item.suggested_local_name ?? `${item.id}.local`}
            resolution={resolutions[item.id]}
            onLocalName={(value) => {
              setLocalNames((current) => ({ ...current, [item.id]: value }));
              setResolutions((current) => {
                if (current[item.id]?.kind !== "preserve") return current;
                const next = { ...current };
                if (value.trim()) next[item.id] = { kind: "preserve", local_name: value.trim() };
                else delete next[item.id];
                return next;
              });
            }}
            onResolution={(resolution) => setResolutions((current) => ({ ...current, [item.id]: resolution }))}
            onLoadHistory={() => void loadHistory(item.id)}
            onHistorySelection={(revision) => setHistorySelections((current) => ({ ...current, [item.id]: revision }))}
            onRollback={() => void rollbackSkill(item.id)}
            onResume={() => void resumeFollowing(item.id)}
            onConvertRemoved={() => void resolveRemovedSkill(item.id, "convert")}
            onUninstallRemoved={() => void resolveRemovedSkill(item.id, "uninstall")}
            onInstallAdded={() => void installAddedSkill(item.id)}
          />
        ))}
      </div>

      {error && <p className="text-xs text-destructive">{error}</p>}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Button size="sm" variant="outline" onClick={() => void check()} disabled={checking || applying}>
          {checking ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <RefreshCw className="mr-1.5 size-4" />}
          {t("sharedChannels.checkAgain")}
        </Button>
        <Button size="sm" onClick={() => void apply()} disabled={!actionable || applying || checking}>
          {applying && <Loader2 className="mr-1.5 size-4 animate-spin" />}
          {snapshot.acknowledgement_required &&
          !snapshot.items.some((item) =>
            ["available", "notification", "blocked", "failed", "removed_from_channel"].includes(item.state),
          )
            ? t("sharedChannels.acknowledgeRelease")
            : t("sharedChannels.applySafeUpdates")}
        </Button>
      </div>
    </section>
  );
}

function AutoUpdateResult({ state }: { state: ChannelAutoUpdateState }) {
  const { t } = useTranslation();
  const run = state.last_run;
  if (!run) return null;
  const timestamp = run.completed_at ?? run.started_at;
  return (
    <div className="mt-3 space-y-2 border-t border-border/70 pt-3 text-[11px] text-muted-foreground">
      <p>
        {t("sharedChannels.lastAutoRun")}{" "}
        <span className="font-medium text-foreground">{run.status.replaceAll("_", " ")}</span> ·{" "}
        <time dateTime={timestamp}>{formatPublishedAt(timestamp)}</time>
      </p>
      {run.applied_skill_ids.length > 0 && (
        <p>{t("sharedChannels.applied", { ids: run.applied_skill_ids.join(", ") })}</p>
      )}
      {run.pauses.length > 0 && (
        <ul className="space-y-1" aria-label={t("sharedChannels.pauseAria")}>
          {run.pauses.map((pause, index) => (
            <li key={`${pause.skill_id ?? "channel"}-${pause.reason}-${index}`}>
              {pause.skill_id ?? t("sharedChannels.channelLabel")}: {autoPauseLabel(pause.reason, t)}
              {pause.detail ? ` — ${pause.detail}` : ""}
            </li>
          ))}
        </ul>
      )}
      {run.error && <p className="text-amber-700">{run.error}</p>}
      {run.retryable && <p>{t("sharedChannels.temporaryFailure")}</p>}
      {state.enabled && state.next_check_at && (
        <p>
          {t("sharedChannels.nextCheck")}{" "}
          <time dateTime={state.next_check_at}>{formatPublishedAt(state.next_check_at)}</time>
        </p>
      )}
    </div>
  );
}

function autoPauseLabel(
  reason: ChannelAutoUpdatePauseReason,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  const labels: Record<ChannelAutoUpdatePauseReason, string> = {
    pinned: t("sharedChannels.pausePinned"),
    local_content_changed: t("sharedChannels.pauseLocalChanged"),
    baseline_missing: t("sharedChannels.pauseBaselineMissing"),
    snapshot_failed: t("sharedChannels.pauseSnapshotFailed"),
    permission_changed: t("sharedChannels.pausePermissionChanged"),
    removed_upstream: t("sharedChannels.pauseRemovedUpstream"),
    integrity_error: t("sharedChannels.pauseIntegrityError"),
    unresolved_failure: t("sharedChannels.pauseUnresolvedFailure"),
    new_skill_requires_review: t("sharedChannels.pauseNewSkillReview"),
  };
  return labels[reason];
}

function UpdateItem({
  item,
  disabled,
  localName,
  history,
  historyLoading,
  historySelection,
  skillAction,
  resolution,
  onLocalName,
  onResolution,
  onLoadHistory,
  onHistorySelection,
  onRollback,
  onResume,
  onConvertRemoved,
  onUninstallRemoved,
  onInstallAdded,
}: {
  item: ChannelUpdateItem;
  disabled: boolean;
  localName: string;
  history?: ChannelSkillRollbackTarget[];
  historyLoading: boolean;
  historySelection?: number;
  skillAction: boolean;
  resolution?: LocalDivergenceResolution;
  onLocalName: (value: string) => void;
  onResolution: (resolution: LocalDivergenceResolution) => void;
  onLoadHistory: () => void;
  onHistorySelection: (revision: number) => void;
  onRollback: () => void;
  onResume: () => void;
  onConvertRemoved: () => void;
  onUninstallRemoved: () => void;
  onInstallAdded: () => void;
}) {
  const { t } = useTranslation();
  const locallyBlocked = !item.pinned_target && item.state === "blocked" && item.block_reason !== "removed_upstream";
  const removedFromChannel =
    item.state === "removed_from_channel" ||
    (item.change === "removed" && item.state === "blocked" && item.block_reason === "removed_upstream");
  return (
    <div className="rounded-lg border border-border px-3 py-2.5">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-xs font-medium">{item.id}</p>
          <p className="mt-0.5 text-[10px] text-muted-foreground">
            {item.change} · {item.state}
          </p>
        </div>
        {item.state === "applied" || item.state === "current" ? (
          <CheckCircle2 className="size-4 text-emerald-600" />
        ) : item.state === "blocked" || item.state === "failed" || removedFromChannel ? (
          <AlertTriangle className="size-4 text-amber-500" />
        ) : null}
      </div>

      {item.change === "added" && (
        <div className="mt-2 flex flex-wrap items-center justify-between gap-2">
          <p className="text-[11px] text-muted-foreground">{t("sharedChannels.newInRelease")}</p>
          {item.state === "notification" && (
            <Button
              size="sm"
              variant="outline"
              disabled={disabled || skillAction}
              aria-label={t("sharedChannels.installAndTrackAria", { skillId: item.id })}
              onClick={onInstallAdded}
            >
              {skillAction && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
              {t("sharedChannels.installAndTrack")}
            </Button>
          )}
        </div>
      )}
      {item.pinned_target ? (
        <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-md border border-amber-500/25 bg-amber-500/5 p-2">
          <p className="text-[11px] text-amber-700">
            {t("sharedChannels.pinnedToRevision", { revision: item.pinned_target.revision })}
          </p>
          <Button
            size="sm"
            variant="outline"
            disabled={disabled || skillAction}
            aria-label={t("sharedChannels.resumeFollowingAria", { skillId: item.id })}
            onClick={onResume}
          >
            {skillAction && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
            {t("sharedChannels.resumeFollowing")}
          </Button>
        </div>
      ) : item.selected && item.change !== "added" ? (
        <div className="mt-3 space-y-2 rounded-md border border-border/70 p-2">
          {history === undefined ? (
            <Button
              size="sm"
              variant="ghost"
              disabled={disabled || historyLoading || skillAction}
              aria-label={t("sharedChannels.viewHistoryAria", { skillId: item.id })}
              onClick={onLoadHistory}
            >
              {historyLoading && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
              {t("sharedChannels.viewHistory")}
            </Button>
          ) : history.length === 0 ? (
            <p className="text-[11px] text-muted-foreground">{t("sharedChannels.noEarlierRelease")}</p>
          ) : (
            <div className="flex flex-wrap items-center gap-2">
              <select
                aria-label={t("sharedChannels.historicalReleaseAria", { skillId: item.id })}
                value={historySelection ?? history[0].target.revision}
                disabled={disabled || skillAction}
                onChange={(event) => onHistorySelection(Number(event.target.value))}
                className="h-8 min-w-44 rounded-md border border-border bg-background px-2 text-xs"
              >
                {history.map((target) => (
                  <option key={target.target.revision} value={target.target.revision}>
                    {t("sharedChannels.revisionOption", {
                      revision: target.target.revision,
                      title: target.title,
                    })}
                  </option>
                ))}
              </select>
              <Button
                size="sm"
                variant="outline"
                disabled={disabled || skillAction}
                aria-label={t("sharedChannels.rollbackAria", { skillId: item.id })}
                onClick={onRollback}
              >
                {skillAction && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
                {t("sharedChannels.rollbackAndPin")}
              </Button>
            </div>
          )}
        </div>
      ) : null}
      {removedFromChannel && (
        <div className="mt-3 space-y-2 rounded-md border border-amber-500/25 bg-amber-500/5 p-2">
          <p className="flex items-center gap-1.5 text-[11px] text-amber-700">
            <ShieldAlert className="size-3.5" /> {t("sharedChannels.removedFromChannelHint")}
          </p>
          <input
            aria-label={t("sharedChannels.localCopyNameAria", { skillId: item.id })}
            value={localName}
            disabled={disabled || skillAction}
            onChange={(event) => onLocalName(event.target.value)}
            className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
          />
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={disabled || skillAction || !localName.trim()}
              aria-label={t("sharedChannels.convertToLocalAria", { skillId: item.id })}
              onClick={onConvertRemoved}
            >
              {skillAction && <Loader2 className="mr-1.5 size-3.5 animate-spin" />}
              {t("sharedChannels.convertToLocal")}
            </Button>
            <Button
              size="sm"
              variant="destructive"
              disabled={disabled || skillAction}
              aria-label={t("sharedChannels.uninstallAria", { skillId: item.id })}
              onClick={onUninstallRemoved}
            >
              {t("sharedChannels.uninstall")}
            </Button>
          </div>
        </div>
      )}
      {locallyBlocked && (
        <div className="mt-3 space-y-2 rounded-md bg-muted/50 p-2">
          <p className="text-[11px]">{t("sharedChannels.localBlockedHint")}</p>
          <input
            aria-label={t("sharedChannels.localCopyNameAria", { skillId: item.id })}
            value={localName}
            disabled={disabled}
            onChange={(event) => onLocalName(event.target.value)}
            className="h-8 w-full rounded-md border border-border bg-background px-2 text-xs"
          />
          <div className="flex flex-wrap gap-2">
            <Button
              size="sm"
              variant={resolution?.kind === "preserve" ? "default" : "outline"}
              disabled={disabled || !localName.trim()}
              onClick={() => onResolution({ kind: "preserve", local_name: localName.trim() })}
            >
              {t("sharedChannels.preserveAsLocal")}
            </Button>
            <Button
              size="sm"
              variant={resolution?.kind === "discard" ? "destructive" : "outline"}
              disabled={disabled}
              onClick={() => onResolution({ kind: "discard" })}
            >
              {t("sharedChannels.discardChanges")}
            </Button>
          </div>
        </div>
      )}
      {item.error && <p className="mt-2 text-[11px] text-destructive">{item.error}</p>}
    </div>
  );
}

function suggestedNames(items: ChannelUpdateItem[], current: Record<string, string>): Record<string, string> {
  const next = { ...current };
  for (const item of items) {
    if (item.suggested_local_name && !next[item.id]) next[item.id] = item.suggested_local_name;
  }
  return next;
}

function withoutKey<T>(current: Record<string, T>, key: string): Record<string, T> {
  const next = { ...current };
  delete next[key];
  return next;
}

function statusLabel(status: ChannelUpdateSnapshot["status"]): string {
  return status.replaceAll("_", " ");
}

function statusClass(status: ChannelUpdateSnapshot["status"]): string {
  const tone = status === "up_to_date" ? "bg-emerald-500/10 text-emerald-700" : "bg-amber-500/10 text-amber-700";
  return `rounded-full px-2 py-1 text-[10px] font-medium ${tone}`;
}

function updateError(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function formatPublishedAt(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toISOString().replace("T", " ").replace(".000Z", " UTC");
}
