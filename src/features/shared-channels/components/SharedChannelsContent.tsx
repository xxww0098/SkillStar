import { ExternalLink, GitBranch, Loader2, LockKeyhole, Plus, ShieldCheck, UsersRound } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import type { GitHubOrganization, SharedChannelDescriptor } from "../../../types";
import {
  createSharedChannel,
  listSharedChannelOrganizations,
  listSharedChannels,
  resumeSharedChannel,
} from "../api/channels";

interface Props {
  scopeSwitch: React.ReactNode;
}

export function SharedChannelsContent({ scopeSwitch }: Props) {
  const { t } = useTranslation();
  const [organizations, setOrganizations] = useState<GitHubOrganization[]>([]);
  const [channels, setChannels] = useState<SharedChannelDescriptor[]>([]);
  const [selectedRepositoryId, setSelectedRepositoryId] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [organization, setOrganization] = useState("");
  const [repositoryName, setRepositoryName] = useState("skillstar-shared");
  const [description, setDescription] = useState("Shared SkillStar channel");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    const [organizationResult, channelResult] = await Promise.allSettled([
      listSharedChannelOrganizations(),
      listSharedChannels(),
    ]);
    if (organizationResult.status === "fulfilled") {
      const nextOrganizations = organizationResult.value;
      setOrganizations(nextOrganizations);
      setOrganization((current) => current || nextOrganizations.find((item) => item.viewer_is_admin)?.login || "");
    }
    if (channelResult.status === "fulfilled") {
      const nextChannels = channelResult.value;
      setChannels(nextChannels);
      setSelectedRepositoryId((current) => current ?? nextChannels[0]?.repository_id ?? null);
      setShowCreate(nextChannels.length === 0);
    }
    const failure =
      organizationResult.status === "rejected"
        ? organizationResult.reason
        : channelResult.status === "rejected"
          ? channelResult.reason
          : null;
    if (failure) setError(sharedChannelError(failure));
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const selected = useMemo(
    () => channels.find((channel) => channel.repository_id === selectedRepositoryId) ?? channels[0] ?? null,
    [channels, selectedRepositoryId],
  );

  const handleCreate = async () => {
    if (!organization || !repositoryName.trim()) return;
    setSaving(true);
    setError("");
    try {
      const channel = await createSharedChannel({
        organization,
        repository_name: repositoryName.trim(),
        description: description.trim(),
      });
      setChannels((current) => [channel, ...current.filter((item) => item.repository_id !== channel.repository_id)]);
      setSelectedRepositoryId(channel.repository_id);
      setShowCreate(false);
    } catch (cause) {
      setError(sharedChannelError(cause));
      const pending = await listSharedChannels().catch(() => []);
      if (pending.length > 0) {
        setChannels(pending);
        setSelectedRepositoryId(pending[0].repository_id);
        setShowCreate(false);
      }
    } finally {
      setSaving(false);
    }
  };

  const handleResume = async (repositoryId: number) => {
    setSaving(true);
    setError("");
    try {
      const channel = await resumeSharedChannel(repositoryId);
      setChannels((current) => current.map((item) => (item.repository_id === repositoryId ? channel : item)));
    } catch (cause) {
      setError(sharedChannelError(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <div className="flex items-center gap-2">
          <UsersRound className="size-4 text-muted-foreground" />
          <h1 className="text-sm font-semibold text-foreground">
            {t("sharedChannels.title", { defaultValue: "Shared channels" })}
          </h1>
        </div>
        {scopeSwitch}
      </div>

      {loading ? (
        <div className="flex flex-1 items-center justify-center">
          <Loader2 className="size-5 animate-spin text-muted-foreground" />
        </div>
      ) : (
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <aside className="w-64 shrink-0 border-r border-border p-3">
            <Button className="mb-3 w-full" size="sm" onClick={() => setShowCreate(true)}>
              <Plus className="mr-1.5 size-4" />
              {t("sharedChannels.create", { defaultValue: "Create channel" })}
            </Button>
            <div className="space-y-1">
              {channels.map((channel) => (
                <button
                  type="button"
                  key={channel.repository_id}
                  onClick={() => {
                    setSelectedRepositoryId(channel.repository_id);
                    setShowCreate(false);
                  }}
                  className={`w-full rounded-lg px-2.5 py-2 text-left text-xs transition-colors ${
                    !showCreate && selected?.repository_id === channel.repository_id
                      ? "bg-primary/10 text-foreground"
                      : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
                  }`}
                >
                  <span className="block truncate font-medium">
                    {channel.owner}/{channel.name}
                  </span>
                  <span className="mt-0.5 block text-[10px]">
                    {channel.status === "active"
                      ? t("sharedChannels.active", { defaultValue: "Active" })
                      : t("sharedChannels.awaitingApp", { defaultValue: "App authorization required" })}
                  </span>
                </button>
              ))}
            </div>
          </aside>

          <main className="min-w-0 flex-1 overflow-y-auto p-6">
            {error && (
              <div className="mb-4 rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
                {error}
              </div>
            )}
            {showCreate ? (
              <CreateChannelForm
                organizations={organizations}
                organization={organization}
                setOrganization={setOrganization}
                repositoryName={repositoryName}
                setRepositoryName={setRepositoryName}
                description={description}
                setDescription={setDescription}
                saving={saving}
                onCreate={handleCreate}
              />
            ) : selected ? (
              <ChannelDetail channel={selected} saving={saving} onResume={handleResume} />
            ) : (
              <div className="flex min-h-64 items-center justify-center text-sm text-muted-foreground">
                {t("sharedChannels.empty", { defaultValue: "Create an organization-private channel to get started." })}
              </div>
            )}
          </main>
        </div>
      )}
    </div>
  );
}

function CreateChannelForm({
  organizations,
  organization,
  setOrganization,
  repositoryName,
  setRepositoryName,
  description,
  setDescription,
  saving,
  onCreate,
}: {
  organizations: GitHubOrganization[];
  organization: string;
  setOrganization: (value: string) => void;
  repositoryName: string;
  setRepositoryName: (value: string) => void;
  description: string;
  setDescription: (value: string) => void;
  saving: boolean;
  onCreate: () => void;
}) {
  const { t } = useTranslation();
  const adminOrganizations = organizations.filter((item) => item.viewer_is_admin);
  return (
    <div className="mx-auto max-w-xl space-y-5">
      <div>
        <h2 className="text-lg font-semibold">
          {t("sharedChannels.createTitle", { defaultValue: "Create a private shared channel" })}
        </h2>
        <p className="mt-1 text-sm text-muted-foreground">
          {t("sharedChannels.createHint", {
            defaultValue: "SkillStar creates a dedicated private repository in your GitHub organization.",
          })}
        </p>
      </div>

      <div className="rounded-xl border border-amber-500/25 bg-amber-500/5 p-4">
        <div className="flex items-center gap-2 text-sm font-medium">
          <ShieldCheck className="size-4 text-amber-500" />
          {t("sharedChannels.permissionTitle", { defaultValue: "Authorization boundary" })}
        </div>
        <ul className="mt-2 space-y-1 text-xs text-muted-foreground">
          <li>• Administration: write — manage direct channel membership.</li>
          <li>• Contents: write — publish immutable channel versions.</li>
          <li>• The GitHub App receives access to the complete selected repository.</li>
        </ul>
      </div>

      {adminOrganizations.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border p-5 text-sm text-muted-foreground">
          {t("sharedChannels.noOrganizations", {
            defaultValue: "No organization with owner access is available for this GitHub identity.",
          })}
        </div>
      ) : (
        <div className="space-y-4">
          <label className="block space-y-1.5 text-xs font-medium">
            <span>{t("sharedChannels.organization", { defaultValue: "GitHub organization" })}</span>
            <select
              value={organization}
              onChange={(event) => setOrganization(event.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            >
              {adminOrganizations.map((item) => (
                <option key={item.id} value={item.login}>
                  {item.login}
                </option>
              ))}
            </select>
          </label>
          <label className="block space-y-1.5 text-xs font-medium">
            <span>{t("sharedChannels.repositoryName", { defaultValue: "Repository name" })}</span>
            <input
              value={repositoryName}
              onChange={(event) => setRepositoryName(event.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            />
          </label>
          <label className="block space-y-1.5 text-xs font-medium">
            <span>{t("sharedChannels.description", { defaultValue: "Description" })}</span>
            <input
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            />
          </label>
          <Button onClick={onCreate} disabled={saving || !organization || !repositoryName.trim()}>
            {saving ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <LockKeyhole className="mr-1.5 size-4" />}
            {t("sharedChannels.createPrivateRepository", { defaultValue: "Create private repository" })}
          </Button>
        </div>
      )}
    </div>
  );
}

function ChannelDetail({
  channel,
  saving,
  onResume,
}: {
  channel: SharedChannelDescriptor;
  saving: boolean;
  onResume: (repositoryId: number) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="mx-auto max-w-2xl space-y-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <GitBranch className="size-5 text-muted-foreground" />
            <h2 className="text-lg font-semibold">
              {channel.owner}/{channel.name}
            </h2>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            Repository ID {channel.repository_id} · descriptor v{channel.descriptor_version}
          </p>
        </div>
        <a
          href={channel.html_url}
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
        >
          GitHub <ExternalLink className="size-3" />
        </a>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <InfoCard label={t("sharedChannels.role", { defaultValue: "Your role" })} value={channel.role} />
        <InfoCard
          label={t("sharedChannels.visibility", { defaultValue: "Visibility" })}
          value="Private organization repository"
        />
        <InfoCard label="Administration" value={channel.authorization.administration} />
        <InfoCard label="Contents" value={channel.authorization.contents} />
      </div>

      {channel.status === "active" ? (
        <div className="rounded-xl border border-emerald-500/25 bg-emerald-500/5 p-5 text-center">
          <ShieldCheck className="mx-auto size-7 text-emerald-500" />
          <p className="mt-2 text-sm font-medium">
            {t("sharedChannels.emptyActive", { defaultValue: "Channel ready — no Skills have been published yet." })}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">GitHub App scope: selected repository only.</p>
        </div>
      ) : (
        <div className="rounded-xl border border-amber-500/25 bg-amber-500/5 p-5">
          <p className="text-sm font-medium">
            {t("sharedChannels.authorizationPending", { defaultValue: "GitHub App authorization is incomplete" })}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            Install the SkillStar GitHub App for this organization, select this repository, then retry.
          </p>
          <Button className="mt-4" size="sm" onClick={() => onResume(channel.repository_id)} disabled={saving}>
            {saving && <Loader2 className="mr-1.5 size-4 animate-spin" />}
            {t("sharedChannels.retryAuthorization", { defaultValue: "Retry authorization" })}
          </Button>
        </div>
      )}
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="mt-1 text-sm font-medium capitalize">{value}</p>
    </div>
  );
}

function sharedChannelError(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
