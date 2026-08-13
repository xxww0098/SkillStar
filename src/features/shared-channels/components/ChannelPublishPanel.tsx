import { listen } from "@tauri-apps/api/event";
import { CheckCircle2, ExternalLink, GitCommitHorizontal, Loader2, Rocket, ScanSearch, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { ChannelPublishPreview, ChannelPublishResult, SharedChannelDescriptor } from "../../../types";
import type { GitOperationProgress } from "../../../types/github";
import { cancelSharedChannelPublish, previewSharedChannelPublish, publishSharedChannel } from "../api/channels";

interface Props {
  channel: SharedChannelDescriptor;
}

export function ChannelPublishPanel({ channel }: Props) {
  const { t } = useTranslation();
  const [title, setTitle] = useState("");
  const [notes, setNotes] = useState("");
  const [preview, setPreview] = useState<ChannelPublishPreview | null>(null);
  const [result, setResult] = useState<ChannelPublishResult | null>(null);
  const [scanning, setScanning] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState("");
  const activeSessionRef = useRef<string | null>(null);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<GitOperationProgress>("skillstar://git-progress", ({ payload }) => {
      if (payload.session_id === activeSessionRef.current) setProgress(payload.phase);
    })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {
        // Browser development mode has no native event bus.
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const cancelSession = useCallback(async () => {
    const sessionId = activeSessionRef.current;
    activeSessionRef.current = null;
    setScanning(false);
    setProgress("");
    setPreview(null);
    if (sessionId) await cancelSharedChannelPublish(sessionId).catch(() => false);
  }, []);

  useEffect(
    () => () => {
      const sessionId = activeSessionRef.current;
      if (sessionId) void cancelSharedChannelPublish(sessionId);
    },
    [],
  );

  useEffect(() => {
    void cancelSession();
    setTitle("");
    setNotes("");
    setResult(null);
    setError("");
  }, [cancelSession, channel.repository_id]);

  const counts = useMemo(() => {
    const next = { added: 0, updated: 0, unchanged: 0, removed: 0 };
    for (const skill of preview?.changes ?? []) next[skill.status] += 1;
    return next;
  }, [preview]);

  const scan = async () => {
    const stale = activeSessionRef.current;
    if (stale) await cancelSharedChannelPublish(stale).catch(() => false);
    const sessionId = crypto.randomUUID();
    activeSessionRef.current = sessionId;
    setScanning(true);
    setProgress("preparing");
    setPreview(null);
    setResult(null);
    setError("");
    try {
      const next = await previewSharedChannelPublish(channel.repository_id, sessionId);
      if (activeSessionRef.current !== sessionId) {
        await cancelSharedChannelPublish(next.session_id).catch(() => false);
        return;
      }
      setPreview(next);
      setProgress("completed");
    } catch (cause) {
      if (activeSessionRef.current === sessionId) {
        activeSessionRef.current = null;
        setProgress("");
        setError(errorMessage(cause));
      }
    } finally {
      if (activeSessionRef.current === sessionId) setScanning(false);
    }
  };

  const publish = async () => {
    if (!preview || !title.trim()) return;
    setPublishing(true);
    setError("");
    try {
      const published = await publishSharedChannel(preview.session_id, title.trim(), notes);
      activeSessionRef.current = null;
      setPreview(null);
      setProgress("");
      setResult(published);
    } catch (cause) {
      // The exact-commit preview remains retryable after transient rejection.
      setError(errorMessage(cause));
    } finally {
      setPublishing(false);
    }
  };

  if (result) {
    return (
      <section className="rounded-xl border border-emerald-500/25 bg-emerald-500/5 p-5">
        <div className="flex items-center gap-2 text-sm font-semibold">
          <CheckCircle2 className="size-5 text-emerald-500" />
          {t("sharedChannels.publishedTag", { tag: result.manifest.tag_name })}
        </div>
        <p className="mt-2 text-xs text-muted-foreground">
          {t("sharedChannels.publishedSkillsCount", { count: result.manifest.skills.length })}
        </p>
        <p className="mt-1 break-all font-mono text-[11px] text-muted-foreground">
          {t("sharedChannels.commitSha", { sha: result.manifest.commit_sha })}
        </p>
        <div className="mt-4 flex gap-2">
          <a
            href={result.release.html_url}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
          >
            {t("sharedChannels.openRelease")} <ExternalLink className="size-3" />
          </a>
          <button
            type="button"
            className="text-xs text-muted-foreground hover:text-foreground"
            onClick={() => setResult(null)}
          >
            {t("sharedChannels.publishAnother")}
          </button>
        </div>
      </section>
    );
  }

  return (
    <section className="space-y-4 rounded-xl border border-border p-5">
      <div>
        <h3 className="text-sm font-semibold">{t("sharedChannels.publishTitle")}</h3>
        <p className="mt-1 text-xs text-muted-foreground">{t("sharedChannels.publishHint")}</p>
      </div>

      {error && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
          {error}
        </div>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        <label className="block space-y-1.5 text-xs font-medium">
          <span>{t("sharedChannels.releaseTitle")}</span>
          <input
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            maxLength={120}
            className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            placeholder={t("sharedChannels.releaseTitlePlaceholder")}
          />
        </label>
        <label className="block space-y-1.5 text-xs font-medium sm:row-span-2">
          <span>{t("sharedChannels.releaseNotes")}</span>
          <textarea
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            maxLength={20_000}
            rows={4}
            className="w-full rounded-md border border-border bg-background px-3 py-2 text-sm"
            placeholder={t("sharedChannels.releaseNotesPlaceholder")}
          />
        </label>
      </div>

      {preview && (
        <div className="space-y-3 rounded-lg border border-border bg-muted/20 p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="inline-flex items-center gap-2 text-xs font-medium">
              <GitCommitHorizontal className="size-4" />
              {preview.tag_name}
            </div>
            <div className="flex gap-2 text-[11px] text-muted-foreground">
              <span>{t("sharedChannels.countsAdded", { count: counts.added })}</span>
              <span>{t("sharedChannels.countsUpdated", { count: counts.updated })}</span>
              <span>{t("sharedChannels.countsRemoved", { count: counts.removed })}</span>
              <span>{t("sharedChannels.countsUnchanged", { count: counts.unchanged })}</span>
            </div>
          </div>
          <p className="break-all font-mono text-[11px] text-muted-foreground">
            {t("sharedChannels.commitSha", { sha: preview.commit_sha })}
          </p>
          <ul className="max-h-44 space-y-1 overflow-y-auto font-mono text-[11px]">
            {preview.changes.map((skill) => (
              <li key={skill.id} className="flex items-center justify-between gap-3">
                <span className="truncate">
                  {skill.id} · {skill.content_root || "/"}
                </span>
                <span className="shrink-0 capitalize text-muted-foreground">{skill.status}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button variant={preview ? "outline" : "default"} onClick={scan} disabled={scanning || publishing}>
          {scanning ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <ScanSearch className="mr-1.5 size-4" />}
          {preview ? t("sharedChannels.rescanExactCommit") : t("sharedChannels.previewPublication")}
        </Button>
        {scanning && (
          <Button variant="outline" size="sm" onClick={() => void cancelSession()}>
            <X className="mr-1 size-3.5" /> {t("sharedChannels.cancelScan")}
          </Button>
        )}
        {preview && (
          <Button onClick={publish} disabled={publishing || !title.trim()}>
            {publishing ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <Rocket className="mr-1.5 size-4" />}
            {t("sharedChannels.publishTag", { tag: preview.tag_name })}
          </Button>
        )}
        {progress && (
          <span className="text-xs capitalize text-muted-foreground">
            {t("sharedChannels.scanProgress", { progress })}
          </span>
        )}
      </div>
    </section>
  );
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) return String(error.message);
  return String(error);
}
