import { AnimatePresence, motion } from "framer-motion";
import { Trash2, X } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { AgentIcon } from "../../../components/ui/AgentIcon";
import { Button } from "../../../components/ui/button";
import { agentIconCls, cn } from "../../../lib/utils";
import type { AgentProfile } from "../../../types";
import type { RemoteSkill } from "../../../lib/ipc/commands/ssh";
import { remoteAgentProfile } from "../lib/remoteAgentProfile";

interface Props {
  skill: RemoteSkill | null;
  onClose: () => void;
  onDelete: (skill: RemoteSkill) => void;
  deleting?: boolean;
  builtinProfiles: AgentProfile[];
}

export function RemoteSkillDrawer({ skill, onClose, onDelete, deleting, builtinProfiles }: Props) {
  const { t } = useTranslation();

  useEffect(() => {
    if (!skill) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [skill, onClose]);

  return (
    <AnimatePresence>
      {skill ? (
        <>
          <motion.button
            type="button"
            aria-label={t("common.close")}
            className="fixed inset-0 z-40 bg-black/40 backdrop-blur-[2px]"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={onClose}
          />
          <motion.aside
            className={cn(
              "fixed right-0 top-0 z-50 flex h-full w-full max-w-md flex-col",
              "border-l border-border/50 bg-card/95 shadow-xl backdrop-blur-md",
            )}
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", damping: 28, stiffness: 320 }}
          >
            <header className="flex items-center justify-between border-b border-border/40 px-5 py-4">
              <div className="min-w-0 flex-1">
                <h2 className="truncate text-lg font-semibold">{skill.name}</h2>
                {skill.agent ? (
                  <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
                    <AgentIcon
                      profile={remoteAgentProfile(skill.agent, builtinProfiles)}
                      className={agentIconCls(remoteAgentProfile(skill.agent, builtinProfiles).icon, "w-4 h-4")}
                    />
                    <span>{skill.agent}</span>
                  </div>
                ) : null}
              </div>
              <Button variant="ghost" size="icon-sm" onClick={onClose}>
                <X className="size-5" />
              </Button>
            </header>
            <div className="flex-1 overflow-y-auto px-5 py-4 text-sm">
              <dl className="space-y-3">
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
            </div>
            <footer className="flex gap-2 border-t border-border/40 px-5 py-4">
              <Button variant="destructive" className="flex-1" disabled={deleting} onClick={() => onDelete(skill)}>
                <Trash2 className="size-4" />
                {t("ssh.delete")}
              </Button>
            </footer>
          </motion.aside>
        </>
      ) : null}
    </AnimatePresence>
  );
}
