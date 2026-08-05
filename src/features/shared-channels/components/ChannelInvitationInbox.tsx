import { Check, Inbox, Loader2, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button } from "../../../components/ui/button";
import type { ChannelInvitation, SharedChannelDescriptor } from "../../../types";
import {
  acceptSharedChannelInvitation,
  declineSharedChannelInvitation,
  listSharedChannelInvitationInbox,
} from "../api/channels";

interface Props {
  onAccepted: (channel: SharedChannelDescriptor) => void;
  onImportFailed: () => void;
}

export function ChannelInvitationInbox({ onAccepted, onImportFailed }: Props) {
  const [invitations, setInvitations] = useState<ChannelInvitation[]>([]);
  const [loading, setLoading] = useState(true);
  const [actingId, setActingId] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      setInvitations(await listSharedChannelInvitationInbox());
    } catch (cause) {
      setError(messageFrom(cause));
      onImportFailed();
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const accept = async (invitation: ChannelInvitation) => {
    setActingId(invitation.id);
    setError("");
    try {
      const channel = await acceptSharedChannelInvitation(invitation.id);
      setInvitations((current) => current.filter((item) => item.id !== invitation.id));
      onAccepted(channel);
    } catch (cause) {
      setError(messageFrom(cause));
      onImportFailed();
    } finally {
      setActingId(null);
    }
  };

  const decline = async (invitation: ChannelInvitation) => {
    setActingId(invitation.id);
    setError("");
    try {
      await declineSharedChannelInvitation(invitation.id);
      setInvitations((current) => current.filter((item) => item.id !== invitation.id));
      setNotice(`${invitation.owner}/${invitation.repository_name}: cancelled`);
    } catch (cause) {
      setError(messageFrom(cause));
    } finally {
      setActingId(null);
    }
  };

  return (
    <div className="mx-auto max-w-2xl space-y-4">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <Inbox className="size-5 text-muted-foreground" />
            <h2 className="text-lg font-semibold">Repository invitation inbox</h2>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            GitHub invitations do not carry a SkillStar marker. Confirm the repository and inviter before importing it
            as a channel.
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={() => void refresh()} disabled={loading}>
          {loading ? <Loader2 className="mr-1.5 size-4 animate-spin" /> : <RefreshCw className="mr-1.5 size-4" />}
          Refresh
        </Button>
      </div>

      {error && (
        <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
          {error}
        </div>
      )}
      {notice && <div className="rounded-lg border border-border bg-muted/30 p-3 text-xs">{notice}</div>}

      {!loading && invitations.length === 0 && (
        <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-muted-foreground">
          No pending organization-private repository invitations.
        </div>
      )}

      <div className="space-y-2">
        {invitations.map((invitation) => (
          <article key={invitation.id} className="rounded-xl border border-border p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div>
                <h3 className="text-sm font-semibold">
                  {invitation.owner}/{invitation.repository_name}
                </h3>
                <p className="mt-1 text-xs text-muted-foreground">
                  Invited by @{invitation.inviter?.login ?? "unknown"} · {invitation.effective_role} · pending
                </p>
              </div>
              <div className="flex gap-2">
                <Button size="sm" onClick={() => void accept(invitation)} disabled={actingId !== null}>
                  {actingId === invitation.id ? (
                    <Loader2 className="mr-1 size-3 animate-spin" />
                  ) : (
                    <Check className="mr-1 size-3" />
                  )}
                  Accept and import
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void decline(invitation)}
                  disabled={actingId !== null}
                >
                  <X className="mr-1 size-3" /> Decline
                </Button>
              </div>
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}

function messageFrom(error: unknown): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
