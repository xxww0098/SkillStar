import { lazy, Suspense } from "react";
import { LoadingLogo } from "../../../components/ui/LoadingLogo";
import type { RemoteSkill } from "../../../lib/ipc/commands/ssh";
import type { AgentProfile, Skill, SkillContent } from "../../../types";
// Deep import (not the ssh barrel) to avoid a my-skills <-> ssh barrel cycle.
import { RemoteSkillDrawer } from "../../ssh/components/RemoteSkillDrawer";

const DetailPanel = lazy(() =>
  import("../../../components/layout/DetailPanel").then((m) => ({ default: m.DetailPanel })),
);

/** Local skill detail — the full lazy {@link DetailPanel} (install/update/edit/publish). */
type LocalDetailProps = {
  kind: "local";
  skill: Skill | null;
  onClose: () => void;
  onInstall: (url: string, name: string) => void;
  onUpdate: (name: string) => void;
  onUninstall: (name: string) => void;
  uninstalling?: boolean;
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

  // Local arm: mount the lazy panel only once a skill is selected.
  if (!props.skill) return null;
  return (
    <Suspense
      fallback={
        <div className="absolute right-0 top-0 bottom-0 z-50 flex h-full w-full max-w-md items-center justify-center overflow-y-auto border-l border-border/45 bg-background/30 shadow-[0_24px_80px_-52px_var(--color-shadow)] backdrop-blur-xl">
          <LoadingLogo size="sm" />
        </div>
      }
    >
      <DetailPanel
        skill={props.skill}
        onClose={props.onClose}
        onInstall={props.onInstall}
        onUpdate={props.onUpdate}
        onUninstall={props.onUninstall}
        uninstalling={props.uninstalling}
        onReadContent={props.onReadContent}
        onSaveContent={props.onSaveContent}
        onPublish={props.onPublish}
      />
    </Suspense>
  );
}
