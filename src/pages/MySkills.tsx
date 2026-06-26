import {
  LocalSkillsContent,
  MySkillsRemoteHostPicker,
  MySkillsScopeSwitch,
  useMySkillsRemoteHosts,
  useMySkillsScope,
} from "../features/my-skills";
import { CloudSkillsContent } from "../features/s3";
import { RemoteSkillsContent, SshHostForm } from "../features/ssh";

interface MySkillsProps {
  initialFocusSkill?: string | null;
  onClearFocus?: () => void;
  onPackSkills?: (skills: string[]) => void;
  /** Pre-filled share code from clipboard auto-detect */
  initialShareCode?: string;
  /** Clear consumed share code */
  onClearShareCode?: () => void;
}

/**
 * My Skills shell. Owns only the scope value, the remote host state, and the
 * binary scope render — each scope is a self-contained `*Content` component that
 * owns its own toolbar + capabilities. Mirrors `pages/Models.tsx`.
 */
export function MySkills({
  initialFocusSkill,
  onClearFocus,
  onPackSkills,
  initialShareCode,
  onClearShareCode,
}: MySkillsProps = {}) {
  const { scope, setScope } = useMySkillsScope();
  const remoteHosts = useMySkillsRemoteHosts();

  const scopeSwitch = <MySkillsScopeSwitch scope={scope} onScopeChange={setScope} />;

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
      {scope === "local" ? (
        <LocalSkillsContent
          scopeSwitch={scopeSwitch}
          initialFocusSkill={initialFocusSkill}
          onClearFocus={onClearFocus}
          onPackSkills={onPackSkills}
          initialShareCode={initialShareCode}
          onClearShareCode={onClearShareCode}
        />
      ) : scope === "remote" ? (
        <RemoteSkillsContent
          host={remoteHosts.selectedHost}
          hostsLoading={remoteHosts.isLoadingHosts}
          hasHosts={(remoteHosts.hosts?.length ?? 0) > 0}
          onAddHost={remoteHosts.openAddHost}
          scopeSwitch={scopeSwitch}
          hostPicker={
            <MySkillsRemoteHostPicker
              hosts={remoteHosts.hosts}
              isLoading={remoteHosts.isLoadingHosts}
              selectedKey={remoteHosts.selectedKey}
              onSelect={remoteHosts.selectHost}
              onAdd={remoteHosts.openAddHost}
              onEdit={remoteHosts.openEditHost}
              onDelete={remoteHosts.deleteHost}
              onImport={remoteHosts.handleImportSystemHost}
            />
          }
        />
      ) : (
        <CloudSkillsContent scopeSwitch={scopeSwitch} />
      )}

      {scope === "remote" && (
        <SshHostForm
          open={remoteHosts.formOpen}
          onOpenChange={remoteHosts.setFormOpen}
          initial={remoteHosts.editing}
          onSubmit={remoteHosts.handleHostFormSubmit}
        />
      )}
    </div>
  );
}
