import { DetailPanel } from "../../../components/layout/DetailPanel";
import type { RemoteSkill } from "../../../lib/ipc/commands/ssh";
import type { AgentProfile, Skill, SkillContent } from "../../../types";
import { RemoteSkillDrawer } from "../remote/RemoteSkillDrawer";

/** Local skill detail — the full {@link DetailPanel} (install/update/edit/publish). */
type LocalDetailProps = {
  kind: "local";
  skill: Skill | null;
  onClose: () => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  onUninstall: (name: string) => void;
  uninstalling?: boolean;
  onResolveRemoved?: (name: string) => void;
  onMigrate?: (name: string) => void;
  migrating?: boolean;
  onReadContent?: (name: string) => Promise<SkillContent>;
  onSaveContent?: (name: string, content: string) => Promise<void>;
  onPublish?: (name: string) => void;
};

/** Remote skill detail — metadata + delete only. */
type RemoteDetailProps = {
  kind: "remote";
  skill: RemoteSkill | null;
  onClose: () => void;
  onDelete: (skill: RemoteSkill) => void;
  deleting?: boolean;
  builtinProfiles: AgentProfile[];
};

export type ScopeDetailProps = LocalDetailProps | RemoteDetailProps;

/**
 * Single detail-surface entry point for both skill scopes. The `kind`
 * discriminant enforces capability at compile time — a remote callback on a
 * local drawer (or vice-versa) is a type error, strictly stronger than the old
 * runtime `skillsScope` conditionals. Each arm forwards to an unchanged body.
 */
export function ScopeDetailDrawer(props: ScopeDetailProps) {
  if (props.kind === "remote") {
    return (
      <RemoteSkillDrawer
        skill={props.skill}
        onClose={props.onClose}
        onDelete={props.onDelete}
        deleting={props.deleting}
        builtinProfiles={props.builtinProfiles}
      />
    );
  }

  // Local arm: mount the panel only once a skill is selected.
  if (!props.skill) return null;
  return (
    <DetailPanel
      skill={props.skill}
      onClose={props.onClose}
      onInstall={props.onInstall}
      onUpdate={props.onUpdate}
      onUninstall={props.onUninstall}
      uninstalling={props.uninstalling}
      onResolveRemoved={props.onResolveRemoved}
      onMigrate={props.onMigrate}
      migrating={props.migrating}
      onReadContent={props.onReadContent}
      onSaveContent={props.onSaveContent}
      onPublish={props.onPublish}
    />
  );
}
