import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { Button } from "../../../components/ui/button";
import { agentIconCls } from "../../../lib/utils";
import type { AgentProfile } from "../../../types";
import type { RemoteSkill } from "../../../lib/ipc/commands/ssh";
import { DrawerShell } from "../../models";
import { remoteAgentProfile } from "../lib/remoteAgentProfile";

interface Props {
  skill: RemoteSkill | null;
  onClose: () => void;
  onDelete: (skill: RemoteSkill) => void;
  deleting?: boolean;
  builtinProfiles: AgentProfile[];
}

/**
 * Remote skill detail. Composes the shared {@link DrawerShell} (Radix Dialog +
 * canonical scrim/focus/animation) so it matches the Models drawers instead of
 * carrying its own spring + hand-rolled overlay.
 */
export function RemoteSkillDrawer({ skill, onClose, onDelete, deleting, builtinProfiles }: Props) {
  const { t } = useTranslation();
  const profile = skill?.agent ? remoteAgentProfile(skill.agent, builtinProfiles) : null;

  return (
    <DrawerShell
      open={!!skill}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      maxWidthClassName="max-w-md"
      title={
        <span className="flex min-w-0 items-center gap-2">
          {profile ? <AgentIcon profile={profile} className={agentIconCls(profile.icon, "w-4 h-4")} /> : null}
          <span className="truncate">{skill?.name}</span>
        </span>
      }
      subtitle={skill?.agent || undefined}
      footer={
        skill ? (
          <Button variant="destructive" className="w-full" disabled={deleting} onClick={() => onDelete(skill)}>
            <Trash2 className="size-4" />
            {t("ssh.delete")}
          </Button>
        ) : null
      }
    >
      {skill ? (
        <>
          <dl className="space-y-3 text-sm">
            <div>
              <dt className="text-xs font-medium text-muted-foreground">{t("ssh.drawer.path")}</dt>
              <dd className="mt-1 break-all font-mono text-xs">{skill.path}</dd>
            </div>
            <div>
              <dt className="text-xs font-medium text-muted-foreground">{t("ssh.drawer.size")}</dt>
              <dd className="mt-1 tabular-nums">{skill.size} B</dd>
            </div>
            {skill.modified ? (
              <div>
                <dt className="text-xs font-medium text-muted-foreground">{t("ssh.drawer.modified")}</dt>
                <dd className="mt-1">{skill.modified}</dd>
              </div>
            ) : null}
          </dl>
          <p className="mt-6 text-xs text-muted-foreground">{t("ssh.drawer.hubHint")}</p>
        </>
      ) : null}
    </DrawerShell>
  );
}
