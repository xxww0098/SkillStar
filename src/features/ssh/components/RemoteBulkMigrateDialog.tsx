import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { Button } from "../../../components/ui/button";
import { ModalHeader, ModalShell } from "../../../components/ui/ModalShell";
import type { RemoteAgentSkills, RemoteSkill } from "../../../lib/ipc/commands/ssh";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  skills: RemoteSkill[];
  agents: RemoteAgentSkills[];
  onMigrateOne: (skill: RemoteSkill, agentSkillsDir: string) => Promise<void>;
  onComplete: () => void;
}

export function RemoteBulkMigrateDialog({ open, onOpenChange, skills, agents, onMigrateOne, onComplete }: Props) {
  const { t } = useTranslation();
  const [running, setRunning] = useState(false);
  const [done, setDone] = useState(0);
  const [failed, setFailed] = useState<string[]>([]);

  const standalone = useMemo(() => skills.filter((s) => (s.layout ?? "standalone") === "standalone"), [skills]);

  const agentDirFor = (agentId: string) => agents.find((a) => a.agent === agentId)?.path ?? "";

  const handleMigrateAll = async () => {
    if (standalone.length === 0) return;
    setRunning(true);
    setDone(0);
    const failedNames: string[] = [];
    for (const skill of standalone) {
      try {
        await onMigrateOne(skill, agentDirFor(skill.agent));
        setDone((d) => d + 1);
      } catch {
        failedNames.push(skill.name);
        setFailed([...failedNames]);
      }
    }
    setRunning(false);
    onComplete();
    if (failedNames.length === 0) onOpenChange(false);
  };

  const handleLater = () => {
    if (!running) onOpenChange(false);
  };

  return (
    <ModalShell
      open={open}
      onClose={handleLater}
      ariaLabel={t("ssh.bulkMigrate.title")}
      dismissable={!running}
      panelClassName="max-w-md"
    >
      <ModalHeader title={t("ssh.bulkMigrate.title")} closeDisabled={running} onClose={handleLater} />
      <div className="flex flex-col gap-4 p-5 pt-0">
        <p className="text-sm text-muted-foreground leading-relaxed">
          {t("ssh.bulkMigrate.body", { count: standalone.length })}
        </p>
        {standalone.length > 0 && (
          <ul className="max-h-40 overflow-y-auto rounded-lg border border-border/40 divide-y divide-border/30 text-xs">
            {standalone.map((s) => (
              <li key={s.path} className="px-3 py-2 font-medium truncate">
                {s.name}
                <span className="ml-2 font-normal text-muted-foreground">{s.agent}</span>
              </li>
            ))}
          </ul>
        )}
        {running && (
          <p className="text-xs text-muted-foreground flex items-center gap-2">
            <Loader2 className="size-3.5 animate-spin" />
            {t("ssh.bulkMigrate.progress", { done, total: standalone.length })}
          </p>
        )}
        {failed.length > 0 && !running && (
          <p className="text-xs text-destructive">{t("ssh.bulkMigrate.failed", { names: failed.join(", ") })}</p>
        )}
        <div className="flex justify-end gap-2">
          <Button variant="ghost" disabled={running} onClick={handleLater}>
            {t("ssh.bulkMigrate.later")}
          </Button>
          <Button disabled={running || standalone.length === 0} onClick={() => void handleMigrateAll()}>
            {running ? <Loader2 className="size-4 animate-spin mr-1" /> : null}
            {t("ssh.bulkMigrate.confirm")}
          </Button>
        </div>
      </div>
    </ModalShell>
  );
}
