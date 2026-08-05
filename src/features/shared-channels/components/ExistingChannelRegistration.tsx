import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, ExternalLink, FolderGit2, Loader2, ScanSearch, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type {
  ExistingChannelRepositoryCandidate,
  ExistingChannelScanPreview,
  GitHubOrganization,
  SharedChannelDescriptor,
} from "../../../types";
import type { GitOperationProgress } from "../../../types/github";
import {
  cancelExistingSharedChannelRegistration,
  confirmExistingSharedChannel,
  listExistingChannelRepositories,
  scanExistingSharedChannel,
} from "../api/channels";

interface Props {
  organizations: GitHubOrganization[];
  onRegistered: (channel: SharedChannelDescriptor) => void;
}

export function ExistingChannelRegistration({ organizations, onRegistered }: Props) {
  const { t } = useTranslation();
  const ownerOrganizations = useMemo(() => organizations.filter((item) => item.viewer_is_admin), [organizations]);
  const [organizationId, setOrganizationId] = useState<number | null>(ownerOrganizations[0]?.id ?? null);
  const [repositories, setRepositories] = useState<ExistingChannelRepositoryCandidate[]>([]);
  const [repositoryId, setRepositoryId] = useState<number | null>(null);
  const [preview, setPreview] = useState<ExistingChannelScanPreview | null>(null);
  const [loadingRepositories, setLoadingRepositories] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [progress, setProgress] = useState("");
  const [error, setError] = useState("");
  const activeSessionRef = useRef<string | null>(null);

  useEffect(() => {
    if (ownerOrganizations.length === 0) {
      const staleSessionId = activeSessionRef.current;
      activeSessionRef.current = null;
      if (staleSessionId) void cancelExistingSharedChannelRegistration(staleSessionId);
      setOrganizationId(null);
      setRepositories([]);
      setRepositoryId(null);
      setPreview(null);
      setScanning(false);
      setProgress("");
      return;
    }
    if (!ownerOrganizations.some((item) => item.id === organizationId)) {
      setOrganizationId(ownerOrganizations[0].id);
    }
  }, [organizationId, ownerOrganizations]);

  useEffect(() => {
    if (organizationId === null) return;
    const staleSessionId = activeSessionRef.current;
    activeSessionRef.current = null;
    if (staleSessionId) void cancelExistingSharedChannelRegistration(staleSessionId);
    let active = true;
    setRepositories([]);
    setRepositoryId(null);
    setLoadingRepositories(true);
    setError("");
    setPreview(null);
    setScanning(false);
    setProgress("");
    void listExistingChannelRepositories(organizationId)
      .then((items) => {
        if (!active) return;
        setRepositories(items);
        setRepositoryId(items.find((item) => !item.already_registered)?.repository_id ?? null);
      })
      .catch((cause) => {
        if (active) setError(errorMessage(cause));
      })
      .finally(() => {
        if (active) setLoadingRepositories(false);
      });
    return () => {
      active = false;
    };
  }, [organizationId]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<GitOperationProgress>("skillstar://git-progress", ({ payload }) => {
      if (payload.session_id !== activeSessionRef.current) return;
      setProgress(payload.phase);
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

  const cancelSession = useCallback(async (clearPreview: boolean) => {
    const sessionId = activeSessionRef.current;
    activeSessionRef.current = null;
    setScanning(false);
    setProgress("");
    if (clearPreview) setPreview(null);
    if (sessionId) await cancelExistingSharedChannelRegistration(sessionId).catch(() => false);
  }, []);

  useEffect(
    () => () => {
      const sessionId = activeSessionRef.current;
      if (sessionId) void cancelExistingSharedChannelRegistration(sessionId);
    },
    [],
  );

  const startScan = async () => {
    if (organizationId === null || repositoryId === null) return;
    const sessionId = crypto.randomUUID();
    activeSessionRef.current = sessionId;
    setScanning(true);
    setProgress("preparing");
    setError("");
    setPreview(null);
    try {
      const next = await scanExistingSharedChannel(
        { organization_id: organizationId, repository_id: repositoryId },
        sessionId,
      );
      if (activeSessionRef.current !== sessionId) {
        await cancelExistingSharedChannelRegistration(next.session_id).catch(() => false);
        return;
      }
      setPreview(next);
      setProgress("completed");
    } catch (cause) {
      if (activeSessionRef.current === sessionId) {
        activeSessionRef.current = null;
        setError(errorMessage(cause));
        setProgress("");
        setScanning(false);
      }
    } finally {
      if (activeSessionRef.current === sessionId) setScanning(false);
    }
  };

  const confirm = async () => {
    if (!preview) return;
    setConfirming(true);
    setError("");
    try {
      const channel = await confirmExistingSharedChannel(preview.session_id);
      activeSessionRef.current = null;
      onRegistered(channel);
    } catch (cause) {
      // Keep the ID-bound preview so a transient confirmation failure can retry.
      setError(errorMessage(cause));
    } finally {
      setConfirming(false);
    }
  };

  const selectedOrganization = ownerOrganizations.find((item) => item.id === organizationId);

  if (preview) {
    return (
      <div className="space-y-4">
        {error && <ErrorBanner message={error} />}
        <div className="flex items-start justify-between gap-3">
          <div>
            <h3 className="text-sm font-semibold">
              {preview.repository.owner}/{preview.repository.name}
            </h3>
            <p className="mt-1 text-xs text-muted-foreground">
              Repository ID {preview.repository.repository_id} · {preview.total_files} files scanned
            </p>
          </div>
          <a
            href={preview.repository.html_url}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
          >
            GitHub <ExternalLink className="size-3" />
          </a>
        </div>

        <div className="rounded-xl border border-amber-500/30 bg-amber-500/5 p-4">
          <div className="flex items-center gap-2 text-sm font-semibold text-amber-700 dark:text-amber-300">
            <AlertTriangle className="size-4" />
            {t("sharedChannels.exposureTitle", { defaultValue: "Complete repository exposure" })}
          </div>
          <p className="mt-2 text-xs leading-relaxed text-muted-foreground">
            {t("sharedChannels.exposureWarning", {
              defaultValue:
                "Everyone who joins this channel can read every repository file and the complete Git history — not only the Skills listed below.",
            })}
          </p>
        </div>

        <InventorySection
          title={t("sharedChannels.detectedSkills", { defaultValue: "Detected Skills" })}
          empty={t("sharedChannels.noDetectedSkills", { defaultValue: "No Skills detected" })}
          items={preview.skills.map((skill) => `${skill.id} · ${skill.folder_path || "/"}`)}
        />
        <InventorySection
          title={t("sharedChannels.nonSkillFiles", { defaultValue: "Files outside Skills" })}
          empty={t("sharedChannels.noNonSkillFiles", { defaultValue: "No files outside Skills" })}
          items={preview.non_skill_files}
        />

        <div className="flex flex-wrap gap-2">
          <Button onClick={confirm} disabled={confirming}>
            {confirming && <Loader2 className="mr-1.5 size-4 animate-spin" />}
            {t("sharedChannels.confirmExisting", { defaultValue: "Confirm complete exposure and register" })}
          </Button>
          <Button variant="outline" onClick={() => void cancelSession(true)} disabled={confirming}>
            {t("common.cancel", { defaultValue: "Cancel" })}
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {error && <ErrorBanner message={error} />}
      <p className="text-sm text-muted-foreground">
        {t("sharedChannels.existingHint", {
          defaultValue: "Choose an organization-private repository already selected for the SkillStar GitHub App.",
        })}
      </p>
      <label className="block space-y-1.5 text-xs font-medium">
        <span>{t("sharedChannels.organization", { defaultValue: "GitHub organization" })}</span>
        <select
          value={organizationId ?? ""}
          onChange={(event) => setOrganizationId(Number(event.target.value))}
          className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
        >
          {ownerOrganizations.map((item) => (
            <option key={item.id} value={item.id}>
              {item.login}
            </option>
          ))}
        </select>
      </label>
      <label className="block space-y-1.5 text-xs font-medium">
        <span>{t("sharedChannels.repository", { defaultValue: "Selected private repository" })}</span>
        <select
          value={repositoryId ?? ""}
          onChange={(event) => setRepositoryId(Number(event.target.value))}
          disabled={loadingRepositories || repositories.length === 0}
          className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm disabled:opacity-60"
        >
          {repositories.map((repository) => (
            <option
              key={repository.repository_id}
              value={repository.repository_id}
              disabled={repository.already_registered}
            >
              {repository.owner}/{repository.name}
              {repository.already_registered ? " · already registered" : ""}
            </option>
          ))}
        </select>
      </label>

      {repositories.length === 0 && !loadingRepositories && (
        <div className="rounded-lg border border-dashed border-border p-4 text-xs text-muted-foreground">
          No eligible private repositories are selected for the App.
          {selectedOrganization && (
            <a
              href={`https://github.com/organizations/${encodeURIComponent(selectedOrganization.login)}/settings/installations`}
              target="_blank"
              rel="noreferrer"
              className="ml-1 inline-flex items-center gap-1 text-primary hover:underline"
            >
              Open GitHub App settings <ExternalLink className="size-3" />
            </a>
          )}
        </div>
      )}

      <div className="flex flex-wrap items-center gap-3">
        <Button onClick={startScan} disabled={scanning || loadingRepositories || repositoryId === null}>
          {scanning ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <ScanSearch className="mr-1.5 size-4" />}
          {t("sharedChannels.scanExposure", { defaultValue: "Scan exposure" })}
        </Button>
        {scanning && (
          <Button variant="outline" size="sm" onClick={() => void cancelSession(false)}>
            <X className="mr-1 size-3.5" /> Cancel scan
          </Button>
        )}
        {progress && (
          <span className="inline-flex items-center gap-1.5 text-xs capitalize text-muted-foreground">
            <FolderGit2 className="size-3.5" /> {progress}
          </span>
        )}
      </div>
    </div>
  );
}

function InventorySection({ title, empty, items }: { title: string; empty: string; items: string[] }) {
  return (
    <section className="rounded-xl border border-border p-4">
      <h4 className="text-xs font-semibold">{title}</h4>
      {items.length === 0 ? (
        <p className="mt-2 text-xs text-muted-foreground">{empty}</p>
      ) : (
        <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto font-mono text-[11px] text-muted-foreground">
          {items.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      )}
    </section>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
      {message}
    </div>
  );
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) return String(error.message);
  return String(error);
}
