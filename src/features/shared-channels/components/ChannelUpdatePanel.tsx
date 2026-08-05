import { AlertTriangle, CheckCircle2, Loader2, RefreshCw, ShieldAlert } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Button } from "../../../components/ui/button";
import type { ChannelUpdateItem, ChannelUpdateSnapshot, LocalDivergenceResolution } from "../../../types";
import { applySharedChannelUpdate, checkSharedChannelUpdate, getSharedChannelUpdateState } from "../api/channels";

export function ChannelUpdatePanel({ repositoryId }: { repositoryId: number }) {
  const [snapshot, setSnapshot] = useState<ChannelUpdateSnapshot | null>(null);
  const [checking, setChecking] = useState(true);
  const [applying, setApplying] = useState(false);
  const [error, setError] = useState("");
  const [resolutions, setResolutions] = useState<Record<string, LocalDivergenceResolution>>({});
  const [localNames, setLocalNames] = useState<Record<string, string>>({});

  const check = useCallback(async () => {
    setChecking(true);
    setError("");
    try {
      const next = await checkSharedChannelUpdate(repositoryId);
      setSnapshot(next);
      setResolutions({});
      setLocalNames((current) => suggestedNames(next.items, current));
    } catch (cause) {
      setError(updateError(cause));
    } finally {
      setChecking(false);
    }
  }, [repositoryId]);

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

  const actionable = useMemo(
    () =>
      (snapshot?.acknowledgement_required &&
        !snapshot.items.some((item) => item.state === "blocked" || item.state === "failed")) ||
      snapshot?.items.some(
        (item) =>
          item.state === "available" ||
          item.state === "notification" ||
          (item.state === "blocked" && resolutions[item.id]),
      ) ||
      false,
    [resolutions, snapshot],
  );

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
    } finally {
      setApplying(false);
    }
  };

  if (checking && !snapshot) {
    return (
      <section className="rounded-xl border border-border p-4" aria-label="Channel updates">
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" /> Checking the latest channel release…
        </p>
      </section>
    );
  }

  if (!snapshot) {
    return (
      <section className="rounded-xl border border-border p-4" aria-label="Channel updates">
        <p className="text-sm font-medium">Channel update check unavailable</p>
        <p className="mt-1 text-xs text-destructive">{error}</p>
        <Button size="sm" variant="outline" className="mt-3" onClick={() => void check()}>
          Retry check
        </Button>
      </section>
    );
  }

  const counts = snapshot.items.reduce<Record<string, number>>((result, item) => {
    result[item.change] = (result[item.change] ?? 0) + 1;
    return result;
  }, {});

  return (
    <section className="space-y-4 rounded-xl border border-border p-4" aria-label="Channel updates">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p className="text-sm font-semibold">Channel update · revision {snapshot.target.revision}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {snapshot.title} · published by @{snapshot.publisher.login} ·{" "}
            <time dateTime={snapshot.published_at}>{formatPublishedAt(snapshot.published_at)}</time>
          </p>
        </div>
        <span className={statusClass(snapshot.status)}>{statusLabel(snapshot.status)}</span>
      </div>

      <p className="whitespace-pre-wrap text-xs text-muted-foreground">{snapshot.notes || "No release notes."}</p>
      <div className="flex flex-wrap gap-2 text-[10px] text-muted-foreground">
        <span>{counts.added ?? 0} added</span>
        <span>{counts.updated ?? 0} updated</span>
        <span>{counts.removed ?? 0} removed</span>
        <span>{counts.unchanged ?? 0} unchanged</span>
      </div>

      {snapshot.check_error && (
        <p className="rounded-lg border border-amber-500/25 bg-amber-500/5 p-2 text-xs text-amber-700">
          Showing the last verified result because this check failed: {snapshot.check_error}
        </p>
      )}

      {snapshot.acknowledgement_required &&
        !snapshot.items.some((item) => ["available", "notification", "blocked", "failed"].includes(item.state)) && (
          <p className="rounded-lg border border-border bg-muted/40 p-2 text-xs text-muted-foreground">
            This release has no Skill content changes. Acknowledge it to advance the subscribed revision.
          </p>
        )}

      <div className="space-y-2">
        {snapshot.items.map((item) => (
          <UpdateItem
            key={item.id}
            item={item}
            disabled={applying}
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
          />
        ))}
      </div>

      {error && <p className="text-xs text-destructive">{error}</p>}
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Button size="sm" variant="outline" onClick={() => void check()} disabled={checking || applying}>
          {checking ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <RefreshCw className="mr-1.5 size-4" />}
          Check again
        </Button>
        <Button size="sm" onClick={() => void apply()} disabled={!actionable || applying || checking}>
          {applying && <Loader2 className="mr-1.5 size-4 animate-spin" />}
          {snapshot.acknowledgement_required &&
          !snapshot.items.some((item) => ["available", "notification", "blocked", "failed"].includes(item.state))
            ? "Acknowledge release"
            : "Apply safe updates"}
        </Button>
      </div>
    </section>
  );
}

function UpdateItem({
  item,
  disabled,
  localName,
  resolution,
  onLocalName,
  onResolution,
}: {
  item: ChannelUpdateItem;
  disabled: boolean;
  localName: string;
  resolution?: LocalDivergenceResolution;
  onLocalName: (value: string) => void;
  onResolution: (resolution: LocalDivergenceResolution) => void;
}) {
  const locallyBlocked = item.state === "blocked" && item.block_reason !== "removed_upstream";
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
        ) : item.state === "blocked" || item.state === "failed" ? (
          <AlertTriangle className="size-4 text-amber-500" />
        ) : null}
      </div>

      {item.change === "added" && (
        <p className="mt-2 text-[11px] text-muted-foreground">New in this release; it was not selected or installed.</p>
      )}
      {item.block_reason === "removed_upstream" && (
        <p className="mt-2 flex items-center gap-1.5 text-[11px] text-amber-700">
          <ShieldAlert className="size-3.5" /> Removed upstream; your installed copy is kept until you remove it
          explicitly.
        </p>
      )}
      {locallyBlocked && (
        <div className="mt-3 space-y-2 rounded-md bg-muted/50 p-2">
          <p className="text-[11px]">Local changes must be resolved before this Skill can update.</p>
          <input
            aria-label={`Local copy name for ${item.id}`}
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
              Preserve as .local
            </Button>
            <Button
              size="sm"
              variant={resolution?.kind === "discard" ? "destructive" : "outline"}
              disabled={disabled}
              onClick={() => onResolution({ kind: "discard" })}
            >
              Discard changes
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
