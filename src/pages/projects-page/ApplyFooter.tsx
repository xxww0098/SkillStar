import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import { Check, RefreshCw } from "lucide-react";
import { Button } from "../../components/ui/button";

interface ApplyFooterProps {
  totalSkills: number;
  enabledAgentsCount: number;
  syncResult: number | null;
  saving: boolean;
  dirty: boolean;
  onApply: () => void;
}

export function ApplyFooter({
  totalSkills,
  enabledAgentsCount,
  syncResult,
  saving,
  dirty,
  onApply,
}: ApplyFooterProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-between px-6 py-3.5 border-t border-border/60 shrink-0">
      <div className="flex items-center gap-2">
        {totalSkills > 0 && (
          <span className="text-xs text-muted-foreground">
            {totalSkills} skill{totalSkills !== 1 ? "s" : ""} - {enabledAgentsCount} agent
            {enabledAgentsCount !== 1 ? "s" : ""}
          </span>
        )}
        {syncResult !== null && (
          <motion.span
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="text-[10px] text-success font-medium flex items-center gap-1"
          >
            <Check className="w-3 h-3" />
            {syncResult} synced
          </motion.span>
        )}
      </div>
      <Button size="sm" onClick={onApply} disabled={saving || !dirty}>
        {saving ? (
          <span className="flex items-center gap-1.5">
            <RefreshCw className="w-3.5 h-3.5 animate-spin" />
            {t("projects.syncing")}
          </span>
        ) : (
          <span className="flex items-center gap-1.5">
            <RefreshCw className="w-3.5 h-3.5" />
            {t("projects.apply")}
          </span>
        )}
      </Button>
    </div>
  );
}
