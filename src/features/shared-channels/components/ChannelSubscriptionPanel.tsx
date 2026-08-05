import { CheckCircle2, GitCommitHorizontal, Loader2, PackageCheck, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../../../components/ui/button";
import type {
  ChannelSubscription,
  ChannelSubscriptionReview,
  ChannelSubscriptionView,
  SharedChannelDescriptor,
} from "../../../types";
import {
  listSharedChannelSubscriptions,
  reviewSharedChannelSubscription,
  subscribeSharedChannel,
} from "../api/channels";
import { ChannelUpdatePanel } from "./ChannelUpdatePanel";

export function ChannelSubscriptionPanel({ channel }: { channel: SharedChannelDescriptor }) {
  const [review, setReview] = useState<ChannelSubscriptionReview | null>(null);
  const [subscription, setSubscription] = useState<ChannelSubscriptionView | ChannelSubscription | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [reviewResult, subscriptionsResult] = await Promise.allSettled([
        reviewSharedChannelSubscription(channel.repository_id),
        listSharedChannelSubscriptions(),
      ]);
      if (subscriptionsResult.status === "rejected") throw subscriptionsResult.reason;
      const subscriptions = subscriptionsResult.value;
      setSubscription(subscriptions.find((item) => item.repository_id === channel.repository_id) ?? null);
      if (reviewResult.status === "rejected") {
        setReview(null);
        setSelected(new Set());
        setError(subscriptionError(reviewResult.reason));
        return;
      }
      const nextReview = reviewResult.value;
      setReview(nextReview);
      setSelected(new Set(nextReview.skills.filter((skill) => skill.selected).map((skill) => skill.id)));
    } catch (cause) {
      setError(subscriptionError(cause));
    } finally {
      setLoading(false);
    }
  }, [channel.repository_id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const selectedCount = selected.size;
  const alreadySubscribed = subscription !== null;
  const subscriptionView = subscription && "read_only" in subscription ? subscription : null;
  const readOnly = Boolean(review?.read_only || subscriptionView?.read_only);
  const subscribedSkillCount = subscription
    ? "skills" in subscription
      ? subscription.skills.length
      : subscription.selected_skill_ids.length
    : 0;
  const selectedIds = useMemo(
    () => review?.skills.filter((skill) => selected.has(skill.id)).map((skill) => skill.id) ?? [],
    [review, selected],
  );

  const install = async () => {
    if (!review || alreadySubscribed || readOnly) return;
    setInstalling(true);
    setError("");
    try {
      const result = await subscribeSharedChannel(
        {
          repository_id: channel.repository_id,
          target: review.target,
          selected_skill_ids: selectedIds,
        },
        crypto.randomUUID(),
      );
      setSubscription(result);
    } catch (cause) {
      const message = subscriptionError(cause);
      await refresh().catch(() => undefined);
      setSelected(new Set(selectedIds));
      setError(message);
    } finally {
      setInstalling(false);
    }
  };

  if (loading) {
    return (
      <section className="rounded-xl border border-border p-4" aria-label="Channel release review">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" /> Loading latest published release…
        </div>
      </section>
    );
  }

  if (!review) {
    return (
      <div className="space-y-5">
        <section className="rounded-xl border border-border p-4" aria-label="Channel release review">
          <p className="text-sm font-medium">No installable release</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {error || "Ask a publisher to create the first channel release."}
          </p>
          {alreadySubscribed && (
            <p className="mt-2 text-xs text-emerald-600">
              The local subscription is still available with {subscribedSkillCount} selected Skills.
            </p>
          )}
          <Button size="sm" variant="outline" className="mt-3" onClick={() => void refresh()}>
            Retry release review
          </Button>
        </section>
        {alreadySubscribed && !readOnly && (
          <ChannelUpdatePanel key={channel.repository_id} repositoryId={channel.repository_id} />
        )}
      </div>
    );
  }

  return (
    <section className="space-y-4 rounded-xl border border-border p-4" aria-label="Channel release review">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold">Review published release</p>
          <a
            href={review.channel.html_url}
            target="_blank"
            rel="noreferrer"
            className="mt-1 block text-xs font-medium text-primary hover:underline"
          >
            {review.channel.owner}/{review.channel.name}
          </a>
          <p className="mt-1 text-xs text-muted-foreground">
            Revision {review.target.revision} · {review.target.tag_name} · published by @{review.publisher.login} ·{" "}
            <time dateTime={review.published_at}>{formatPublishedAt(review.published_at)}</time>
          </p>
        </div>
        <div className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-1 font-mono text-[10px] text-muted-foreground">
          <GitCommitHorizontal className="size-3" /> {review.target.commit_sha.slice(0, 12)}
        </div>
      </div>

      <div>
        <p className="text-sm font-medium">{review.title}</p>
        <p className="mt-1 whitespace-pre-wrap text-xs text-muted-foreground">{review.notes || "No release notes."}</p>
      </div>

      <div className="rounded-lg border border-amber-500/25 bg-amber-500/5 p-3">
        <div className="flex items-center gap-2 text-xs font-medium">
          <ShieldAlert className="size-4 text-amber-500" /> Repository exposure
        </div>
        <p className="mt-1 text-[11px] text-muted-foreground">
          This is a private organization repository. Your GitHub access can read its complete contents and full Git
          history, not only the Skills listed below.
        </p>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-3">
          <p className="text-xs font-medium">Skills in this release</p>
          <p className="text-[11px] text-muted-foreground">
            {selectedCount} of {review.skills.length} selected
          </p>
        </div>
        {review.skills.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border p-3 text-xs text-muted-foreground">
            This release contains no current Skills. You can still subscribe with an empty selection.
          </p>
        ) : (
          review.skills.map((skill) => (
            <label key={skill.id} className="flex items-start gap-3 rounded-lg border border-border px-3 py-2.5">
              <input
                type="checkbox"
                className="mt-0.5 size-4"
                checked={selected.has(skill.id)}
                disabled={alreadySubscribed || Boolean(readOnly) || installing}
                onChange={(event) => {
                  setSelected((current) => {
                    const next = new Set(current);
                    if (event.target.checked) next.add(skill.id);
                    else next.delete(skill.id);
                    return next;
                  });
                }}
              />
              <span className="min-w-0 flex-1">
                <span className="block text-xs font-medium">{skill.id}</span>
                <span className="block truncate font-mono text-[10px] text-muted-foreground">
                  {skill.content_root || "."}
                </span>
              </span>
            </label>
          ))
        )}
      </div>

      {error && <p className="text-xs text-destructive">{error}</p>}
      {readOnly ? (
        <p className="text-xs text-amber-600">
          This subscription was created by a newer SkillStar schema. It is visible here but cannot be changed.
        </p>
      ) : alreadySubscribed ? (
        <div className="flex items-center gap-2 text-xs text-emerald-600">
          <CheckCircle2 className="size-4" /> Subscribed to revision{" "}
          {subscription?.target?.revision ?? review.target.revision} with {subscribedSkillCount} selected Skills.
        </div>
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-[11px] text-muted-foreground">
            Accepting repository access and installing Skills are separate choices. Future new Skills will not be added
            automatically.
          </p>
          <Button size="sm" onClick={() => void install()} disabled={installing}>
            {installing ? (
              <Loader2 className="mr-1.5 size-4 animate-spin" />
            ) : (
              <PackageCheck className="mr-1.5 size-4" />
            )}
            Install selected & subscribe
          </Button>
        </div>
      )}
      {alreadySubscribed && !readOnly && (
        <ChannelUpdatePanel key={channel.repository_id} repositoryId={channel.repository_id} />
      )}
    </section>
  );
}

function formatPublishedAt(value: string): string {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return value;
  return parsed.toISOString().replace("T", " ").replace(".000Z", " UTC");
}

function subscriptionError(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
