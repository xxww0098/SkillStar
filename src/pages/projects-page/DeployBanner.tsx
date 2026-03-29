import { AnimatePresence, motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Rocket, X } from "lucide-react";
import { Badge } from "../../components/ui/badge";

interface DeployBannerProps {
  pendingGroupSkills: string[] | null;
  onDismiss: () => void;
}

export function DeployBanner({ pendingGroupSkills, onDismiss }: DeployBannerProps) {
  const { t } = useTranslation();

  return (
    <AnimatePresence>
      {pendingGroupSkills && pendingGroupSkills.length > 0 && (
        <motion.div
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1 }}
          exit={{ height: 0, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="overflow-hidden"
        >
          <div className="flex items-center gap-3 px-6 py-2.5 bg-primary/5 border-b border-primary/20">
            <Rocket className="w-4 h-4 text-primary shrink-0" />
            <span className="text-sm">
              {t("projects.deployBanner", { count: pendingGroupSkills.length })}
            </span>
            <div className="flex-1" />
            <div className="flex flex-wrap gap-1 max-w-xs">
              {pendingGroupSkills.slice(0, 3).map((skillName) => (
                <Badge key={skillName} variant="outline" className="text-[10px] h-5">
                  {skillName}
                </Badge>
              ))}
              {pendingGroupSkills.length > 3 && (
                <Badge variant="outline" className="text-[10px] h-5">
                  +{pendingGroupSkills.length - 3}
                </Badge>
              )}
            </div>
            <button
              onClick={onDismiss}
              className="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
              aria-label={t("common.close")}
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
