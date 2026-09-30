import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { tauriInvoke } from "@/lib/ipc";
import type { SavedGroup } from "@/lib/ipc/commands/models";
import { modelsKeys } from "../../api/keys";
import { useSavedGroups } from "../../api/groups";

/**
 * Saved-group members. Add and remove send the whole list to the gateway
 * writer. A refusal is shown as the returned text, and the list stays.
 */
export function GroupMembers() {
  const queryClient = useQueryClient();
  const { data } = useSavedGroups();
  const groups = data ?? [];
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [newId, setNewId] = useState("");
  const [newMember, setNewMember] = useState("");

  async function commit(id: string, members: string[]): Promise<boolean> {
    try {
      await tauriInvoke("save_group_members", { id, members });
      setErrors((current) => ({ ...current, [id]: "" }));
      await queryClient.invalidateQueries({ queryKey: modelsKeys.savedGroups() });
      await queryClient.invalidateQueries({ queryKey: [...modelsKeys.all, "routing-page"] });
      return true;
    } catch (error) {
      const text = error instanceof Error ? error.message : "";
      setErrors((current) => ({ ...current, [id]: text }));
      return false;
    }
  }

  return (
    <div className="shrink-0 space-y-3 px-4 py-3">
      <form
        aria-label="new group"
        className="flex flex-wrap gap-1"
        onSubmit={(event) => {
          event.preventDefault();
          const id = newId.trim();
          if (!id) return;
          const member = newMember.trim();
          void commit(id, member ? [member] : []).then((saved) => {
            if (saved) {
              setNewId("");
              setNewMember("");
            }
          });
        }}
      >
        <input
          aria-label="new group id"
          value={newId}
          onChange={(event) => setNewId(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <input
          aria-label="new group member"
          value={newMember}
          onChange={(event) => setNewMember(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <button type="submit" className="rounded-lg px-2 py-1 text-xs text-foreground hover:bg-muted/40">
          Save group
        </button>
        {errors[newId.trim()] ? (
          <p role="alert" className="basis-full text-xs text-muted-foreground">
            {errors[newId.trim()]}
          </p>
        ) : null}
      </form>
      {groups.map((group) => (
        <GroupRow
          key={group.id}
          group={group}
          draft={drafts[group.id] ?? ""}
          error={errors[group.id] ?? ""}
          onDraft={(value) => setDrafts((current) => ({ ...current, [group.id]: value }))}
          onAdd={() => {
            const member = (drafts[group.id] ?? "").trim();
            if (!member) return;
            void commit(group.id, [...group.members, member]).then((saved) => {
              if (saved) setDrafts((current) => ({ ...current, [group.id]: "" }));
            });
          }}
          onRemove={(member) => {
            void commit(
              group.id,
              group.members.filter((item) => item !== member),
            );
          }}
        />
      ))}
    </div>
  );
}

function GroupRow({
  group,
  draft,
  error,
  onDraft,
  onAdd,
  onRemove,
}: {
  group: SavedGroup;
  draft: string;
  error: string;
  onDraft: (value: string) => void;
  onAdd: () => void;
  onRemove: (member: string) => void;
}) {
  const label = plainText(group.id);
  return (
    <div role="group" aria-label={`group members ${label}`.trim()} className="space-y-1">
      {label ? <div className="truncate text-xs text-muted-foreground">{label}</div> : null}
      <ul className="space-y-0.5">
        {group.members.map((member) => {
          const shown = plainText(member);
          return (
            <li key={member} className="flex items-center gap-1">
              {shown ? <span className="min-w-0 flex-1 truncate text-xs">{shown}</span> : <span className="flex-1" />}
              <button
                type="button"
                aria-label={shown ? `Remove ${shown}` : "Remove"}
                onClick={() => onRemove(member)}
                className="rounded-lg px-2 py-1 text-xs text-foreground hover:bg-muted/40"
              >
                Remove
              </button>
            </li>
          );
        })}
      </ul>
      <form
        className="flex gap-1"
        onSubmit={(event) => {
          event.preventDefault();
          onAdd();
        }}
      >
        <input
          aria-label={`member ${label}`.trim()}
          value={draft}
          onChange={(event) => onDraft(event.target.value)}
          className="min-w-0 flex-1 rounded-lg bg-muted/40 px-2 py-1 text-xs"
        />
        <button type="submit" className="rounded-lg px-2 py-1 text-xs text-foreground hover:bg-muted/40">
          Add
        </button>
      </form>
      {error ? (
        <p role="alert" className="text-xs text-muted-foreground">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function plainText(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
