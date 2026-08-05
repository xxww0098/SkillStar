import { Loader2, RefreshCw, RotateCw, UserPlus, UsersRound, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button } from "../../../components/ui/button";
import type {
  ChannelInvitationAction,
  ChannelInviteRole,
  ChannelMembershipSnapshot,
  SharedChannelDescriptor,
} from "../../../types";
import {
  cancelSharedChannelInvitation,
  inviteSharedChannelMember,
  listSharedChannelMembership,
  resendSharedChannelInvitation,
} from "../api/channels";

interface Props {
  channel: SharedChannelDescriptor;
}

export function ChannelMembershipPanel({ channel }: Props) {
  const [snapshot, setSnapshot] = useState<ChannelMembershipSnapshot | null>(null);
  const [username, setUsername] = useState("");
  const [role, setRole] = useState<ChannelInviteRole>("subscriber");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [lastAction, setLastAction] = useState<ChannelInvitationAction | null>(null);
  const [denied, setDenied] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    setDenied(false);
    try {
      setSnapshot(await listSharedChannelMembership(channel.repository_id));
    } catch (cause) {
      if (errorCode(cause) === "permission_denied") {
        setDenied(true);
        return;
      }
      setError(messageFrom(cause));
    } finally {
      setLoading(false);
    }
  }, [channel.repository_id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const run = async (operation: () => Promise<ChannelInvitationAction>) => {
    setSaving(true);
    setError("");
    try {
      const action = await operation();
      setLastAction(action);
      await refresh();
      return true;
    } catch (cause) {
      const operationError = messageFrom(cause);
      setLastAction({
        repository_id: channel.repository_id,
        invitation_id: null,
        username,
        role,
        status: "failed",
      });
      // Re-invite is cancel-then-create and can fail after GitHub already
      // removed the old invitation. Always refresh before showing the error.
      await refresh();
      setError(operationError);
      return false;
    } finally {
      setSaving(false);
    }
  };

  const invite = async () => {
    const login = username.trim();
    if (!login) return;
    const succeeded = await run(() =>
      inviteSharedChannelMember({
        repository_id: channel.repository_id,
        username: login,
        role,
      }),
    );
    if (succeeded) setUsername("");
  };

  if (denied || (loading && snapshot === null && !error)) return null;

  return (
    <section className="rounded-xl border border-border p-4" aria-label="Channel members">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2 text-sm font-semibold">
            <UsersRound className="size-4" />
            Members and invitations
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            GitHub is the source of truth, including team and organization access.
          </p>
        </div>
        <Button
          size="icon"
          variant="ghost"
          aria-label="Refresh members"
          onClick={() => void refresh()}
          disabled={loading}
        >
          {loading ? <Loader2 className="size-4 animate-spin" /> : <RefreshCw className="size-4" />}
        </Button>
      </div>

      <div className="mt-4 grid gap-2 sm:grid-cols-[1fr_9rem_auto]">
        <label className="space-y-1 text-xs font-medium">
          <span>GitHub username</span>
          <input
            aria-label="GitHub username"
            value={username}
            onChange={(event) => setUsername(event.target.value)}
            className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm"
            placeholder="octocat"
          />
        </label>
        <label className="space-y-1 text-xs font-medium">
          <span>Channel role</span>
          <select
            aria-label="Channel role"
            value={role}
            onChange={(event) => setRole(event.target.value as ChannelInviteRole)}
            className="h-9 w-full rounded-md border border-border bg-background px-2 text-sm"
          >
            <option value="subscriber">Subscriber</option>
            <option value="publisher">Publisher</option>
          </select>
        </label>
        <Button className="self-end" onClick={() => void invite()} disabled={saving || !username.trim()}>
          {saving ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <UserPlus className="mr-1.5 size-4" />}
          Invite
        </Button>
      </div>

      {lastAction && (
        <p className={`mt-3 text-xs ${lastAction.status === "failed" ? "text-destructive" : "text-muted-foreground"}`}>
          {lastAction.username || "Invitation"}: {lastAction.status}
        </p>
      )}
      {error && <p className="mt-2 text-xs text-destructive">{error}</p>}

      {!loading && snapshot && (
        <div className="mt-4 space-y-4">
          <div>
            <h3 className="text-xs font-semibold">Accepted access</h3>
            <div className="mt-2 space-y-1.5">
              {snapshot.members.map((member) => (
                <div
                  key={member.user.id}
                  className="flex items-center justify-between rounded-md bg-muted/35 px-3 py-2 text-xs"
                >
                  <span className="font-medium">@{member.user.login}</span>
                  <span className="text-muted-foreground">
                    {member.role} · GitHub {member.github_role_name}
                  </span>
                </div>
              ))}
              {snapshot.members.length === 0 && (
                <p className="text-xs text-muted-foreground">No visible collaborators.</p>
              )}
            </div>
          </div>

          <div>
            <h3 className="text-xs font-semibold">Pending invitations</h3>
            <div className="mt-2 space-y-1.5">
              {snapshot.invitations.map((invitation) => (
                <div
                  key={invitation.id}
                  className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border px-3 py-2 text-xs"
                >
                  <span>
                    <span className="font-medium">@{invitation.invitee?.login ?? "unknown"}</span>
                    <span className="ml-2 text-muted-foreground">{invitation.effective_role} · pending</span>
                  </span>
                  <span className="flex items-center gap-1">
                    {invitation.effective_role !== "owner" && (
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={saving}
                        onClick={() =>
                          void run(() => resendSharedChannelInvitation(channel.repository_id, invitation.id))
                        }
                      >
                        <RotateCw className="mr-1 size-3" /> Re-invite
                      </Button>
                    )}
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={saving}
                      onClick={() =>
                        void run(() => cancelSharedChannelInvitation(channel.repository_id, invitation.id))
                      }
                    >
                      <X className="mr-1 size-3" /> Cancel
                    </Button>
                  </span>
                </div>
              ))}
              {snapshot.invitations.length === 0 && (
                <p className="text-xs text-muted-foreground">No pending invitations.</p>
              )}
            </div>
          </div>
          <p className="text-[11px] text-muted-foreground">
            Re-invite cancels the current GitHub invitation before creating a new one; the two steps are not atomic.
          </p>
        </div>
      )}
    </section>
  );
}

function messageFrom(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

function errorCode(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    return String((error as { code: unknown }).code);
  }
  return "";
}
