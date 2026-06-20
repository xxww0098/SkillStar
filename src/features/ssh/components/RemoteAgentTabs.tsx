import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { cn, agentIconCls } from "../../../lib/utils";
import type { AgentProfile } from "../../../types";
import type { RemoteAgentSkills } from "../../../lib/ipc/commands/ssh";
import { remoteAgentProfile } from "../lib/remoteAgentProfile";

interface Props {
  agents: RemoteAgentSkills[];
  selected: string | null;
  onSelect: (agentId: string | null) => void;
  builtinProfiles: AgentProfile[];
  loading?: boolean;
}

export function RemoteAgentTabs({ agents, selected, onSelect, builtinProfiles, loading }: Props) {
  const { t } = useTranslation();

  if (loading) {
    return (
      <div className="flex gap-2 overflow-x-auto pb-1">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="h-9 w-20 shrink-0 animate-pulse rounded-lg bg-muted/40" />
        ))}
      </div>
    );
  }

  if (agents.length === 0) {
    return <p className="text-xs text-muted-foreground">{t("ssh.noAgents")}</p>;
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <button
        type="button"
        onClick={() => onSelect(null)}
        className={cn(
          "rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors",
          selected === null
            ? "border-primary/40 bg-primary/10 text-foreground"
            : "border-border/50 bg-card/30 text-muted-foreground hover:bg-accent/10",
        )}
      >
        {t("ssh.allAgents")} ({agents.reduce((n, a) => n + a.count, 0)})
      </button>
      {agents.map((a) => {
        const profile = remoteAgentProfile(a.agent, builtinProfiles);
        const active = selected === a.agent;
        return (
          <button
            key={a.agent}
            type="button"
            title={a.path}
            onClick={() => onSelect(a.agent)}
            className={cn(
              "inline-flex items-center gap-2 rounded-lg border px-2.5 py-1.5 text-xs font-medium transition-colors",
              active
                ? "border-primary/40 bg-primary/10 text-foreground"
                : "border-border/50 bg-card/30 text-muted-foreground hover:bg-accent/10",
            )}
          >
            <AgentIcon profile={profile} className={agentIconCls(profile.icon, "w-4 h-4")} />
            <span className="max-w-[8rem] truncate">{profile.display_name}</span>
            <span className="tabular-nums text-muted-foreground">({a.count})</span>
          </button>
        );
      })}
    </div>
  );
}
