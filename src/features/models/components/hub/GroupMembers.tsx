import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { tauriInvoke } from "@/lib/ipc";
import type { SavedGroup } from "@/lib/ipc/commands/models";
import { modelsKeys } from "../../api/keys";
import { useSavedGroups } from "../../api/groups";

const FIELD = "rounded-lg border border-border/60 bg-background px-2 py-1 font-mono text-xs";
const ACTION =
  "shrink-0 rounded-lg border border-border/60 bg-background px-2.5 py-1 text-xs text-foreground transition hover:bg-muted/50";

/**
 * Saved-group members. Add and remove send the whole list to the gateway
 * writer. A refusal is shown as the returned text, and the list stays.
 */
export function GroupMembers() {
  const { t } = useTranslation();
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
    <div className="space-y-2">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
        {t("models.gateway.groupsTitle")}
      </div>
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
          onFix={(member, next) => {
            void commit(
              group.id,
              group.members.map((item) => (item === member ? next : item)),
            );
          }}
        />
      ))}
      <form
        aria-label="new group"
        className="space-y-1.5 rounded-xl border border-border/60 bg-muted/20 p-2.5"
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
        <div className="flex gap-1.5">
          <input
            aria-label="new group id"
            placeholder={t("models.gateway.groupNamePlaceholder")}
            value={newId}
            onChange={(event) => setNewId(event.target.value)}
            className={`min-w-0 flex-1 ${FIELD}`}
          />
          <input
            aria-label="new group member"
            placeholder="provider/model"
            value={newMember}
            onChange={(event) => setNewMember(event.target.value)}
            className={`min-w-0 flex-[1.4] ${FIELD}`}
          />
          <button type="submit" className={ACTION}>
            Save group
          </button>
        </div>
        {errors[newId.trim()] ? (
          <p role="alert" className="text-xs text-destructive">
            {errors[newId.trim()]}
          </p>
        ) : null}
      </form>
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
  onFix,
}: {
  group: SavedGroup;
  draft: string;
  error: string;
  onDraft: (value: string) => void;
  onAdd: () => void;
  onRemove: (member: string) => void;
  onFix: (member: string, next: string) => void;
}) {
  const label = plainText(group.id);
  return (
    <div
      role="group"
      aria-label={`group members ${label}`.trim()}
      className="space-y-1.5 rounded-xl border border-border/60 bg-muted/20 px-3 py-2.5"
    >
      {label ? (
        <div className="flex items-baseline justify-between gap-2">
          <span className="truncate text-xs font-medium text-foreground">{label}</span>
          <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">{group.members.length}</span>
        </div>
      ) : null}
      <ul className="space-y-0.5">
        {group.members.map((member) => (
          <MemberLine
            key={member}
            member={member}
            onRemove={() => onRemove(member)}
            onFix={(next) => onFix(member, next)}
          />
        ))}
      </ul>
      <form
        className="flex gap-1.5"
        onSubmit={(event) => {
          event.preventDefault();
          onAdd();
        }}
      >
        <input
          aria-label={`member ${label}`.trim()}
          placeholder="provider/model"
          value={draft}
          onChange={(event) => onDraft(event.target.value)}
          className={`min-w-0 flex-1 ${FIELD}`}
        />
        <button type="submit" className={ACTION}>
          Add
        </button>
      </form>
      {error ? (
        <p role="alert" className="text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </div>
  );
}

function MemberLine({
  member,
  onRemove,
  onFix,
}: {
  member: string;
  onRemove: () => void;
  onFix: (next: string) => void;
}) {
  const { data } = useQuery({
    queryKey: modelsKeys.modelEfforts(member),
    queryFn: () => tauriInvoke("model_efforts", { id: member }),
  });
  const levels = data ?? [];
  const { model, effort } = splitMember(member, levels);
  const shown = plainText(model);
  return (
    <li className="flex items-center gap-1.5 rounded-lg px-1.5 py-1 transition hover:bg-muted/30">
      <div className="flex min-w-0 flex-1 flex-col">
        {shown ? <span className="truncate font-mono text-xs">{shown}</span> : <span />}
        {levels.length > 0 ? (
          <ul aria-label={`effort levels for ${shown}`} className="flex w-fit flex-wrap gap-x-2">
            {levels.map((level) => (
              <li key={level} className="text-[11px] text-muted-foreground">
                {level}
              </li>
            ))}
          </ul>
        ) : null}
      </div>
      {levels.length > 0 ? (
        <select
          aria-label={`effort for ${shown}`}
          value={levels.includes(effort) ? effort : ""}
          onChange={(event) => onFix(event.target.value ? `${model}:${event.target.value}` : model)}
          className="shrink-0 rounded-md border border-border/60 bg-background px-1.5 py-0.5 text-xs text-foreground"
        >
          <option value="">·</option>
          {levels.map((level) => (
            <option key={level} value={level}>
              {level}
            </option>
          ))}
        </select>
      ) : null}
      <button
        type="button"
        aria-label={shown ? `Remove ${shown}` : "Remove"}
        onClick={onRemove}
        className="shrink-0 rounded-md px-1.5 py-1 text-[11px] text-muted-foreground transition hover:bg-destructive/10 hover:text-destructive"
      >
        Remove
      </button>
    </li>
  );
}

function splitMember(member: string, levels: string[]): { model: string; effort: string } {
  const index = member.lastIndexOf(":");
  if (index <= 0) return { model: member, effort: "" };
  const effort = member.slice(index + 1);
  if (!levels.includes(effort)) return { model: member, effort: "" };
  return { model: member.slice(0, index), effort };
}

function plainText(value: string): string {
  if (value.includes("://") || value.includes("sk-") || value.includes("api.openai.com")) return "";
  return value;
}
