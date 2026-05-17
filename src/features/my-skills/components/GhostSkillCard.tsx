import { motion } from "framer-motion";
import { Download, Loader2, Sparkles, X } from "lucide-react";
import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../../components/ui/button";
import { CardDescription, CardTitle } from "../../../components/ui/card";
import { CardTemplate } from "../../../components/ui/card-template";
import { cn } from "../../../lib/utils";
import type { RepoNewSkill } from "../../../types";

interface GhostSkillCardProps {
  skill: RepoNewSkill;
  onInstall: (skill: RepoNewSkill) => void;
  onDismiss: (repoSource: string, skillId: string) => void;
  onClick?: (skill: RepoNewSkill) => void;
}

function GhostSkillCardInner({ skill, onInstall, onDismiss, onClick }: GhostSkillCardProps) {
  const { t } = useTranslation();
  const [installing, setInstalling] = useState(false);

  const handleInstall = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setInstalling(true);
    try {
      await onInstall(skill);
    } catch {
      // Error handled by parent
    } finally {
      setInstalling(false);
    }
  };

  const handleDismiss = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDismiss(skill.repo_source, skill.skill_id);
  };

  const handleClick = () => {
    onClick?.(skill);
  };

  return (
    <motion.div
      initial={{ opacity: 0, scale: 0.95 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.9 }}
      transition={{ duration: 0.2 }}
    >
      <CardTemplate
        className={cn(
          "group/ghost transition-all duration-300",
          onClick ? "cursor-pointer" : "cursor-default",
          "opacity-65 hover:opacity-90",
          "border-dashed border-primary/25 hover:border-primary/40",
          "bg-primary/[0.02] hover:bg-primary/[0.04]",
          "shadow-none hover:shadow-[0_0_20px_rgba(var(--color-primary-rgb),0.06)]",
        )}
        onClick={handleClick}
        topRightSlot={
          <div className="flex items-center gap-1">
            {/* "新技能" badge */}
            <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-primary/12 text-primary text-[10px] font-semibold tracking-wide uppercase">
              <Sparkles className="w-3 h-3" />
              {t("ghostCard.newSkill", "新技能")}
            </span>
            {/* Dismiss button */}
            <button
              onClick={handleDismiss}
              className="p-1 rounded-md text-muted-foreground/50 hover:text-foreground hover:bg-muted/60 transition-all duration-150 opacity-0 group-hover/ghost:opacity-100 cursor-pointer"
              title={t("ghostCard.dismiss", "不再提示")}
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        }
        header={
          <div className="pt-1 pb-0">
            <CardTitle className="text-sm font-semibold truncate text-foreground/70 group-hover/ghost:text-foreground/90 transition-colors">
              {skill.skill_id}
            </CardTitle>
          </div>
        }
        body={
          <div className="flex-1">
            <CardDescription className="line-clamp-2 text-xs text-muted-foreground/70">
              {skill.description || t("ghostCard.noDescription", "暂无描述")}
            </CardDescription>
          </div>
        }
        footer={
          <div className="flex items-center justify-between w-full pt-0">
            <span className="text-[10px] text-muted-foreground/50 truncate max-w-[60%]">{skill.repo_source}</span>
            <Button
              size="sm"
              variant="outline"
              className="h-7 px-2.5 text-xs font-medium border-primary/30 text-primary hover:bg-primary/10 hover:border-primary/50 transition-all cursor-pointer"
              disabled={installing}
              onClick={handleInstall}
            >
              {installing ? (
                <Loader2 className="w-3.5 h-3.5 animate-spin" />
              ) : (
                <>
                  <Download className="w-3.5 h-3.5 mr-1" />
                  {t("ghostCard.install", "安装")}
                </>
              )}
            </Button>
          </div>
        }
      />
    </motion.div>
  );
}

export const GhostSkillCard = memo(GhostSkillCardInner);
